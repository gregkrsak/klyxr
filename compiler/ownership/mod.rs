//! Owned moves, acyclic loans, and stable structured-loop references in ordinary HIR.
//! Availability and active loans are orthogonal private state; no runtime behavior.
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use crate::{
    diagnostics::Diagnostic,
    hir::{
        BorrowKind, ExprKind, ExprType, FunctionId, LocalId, ParameterId, ParameterType, Place,
        Program, TypedExpr, ValueFunction, ValueStatement, ValueType,
    },
    lexer::Span,
};

type OwnershipResult<T = ()> = Result<T, Box<Diagnostic>>;

// A count is consumed only by analysis of its own path. At a conditional the
// suffix is shared, but sibling-only counts are removed before edge expiry.
type FutureUses = BTreeMap<Place, usize>;
fn future_uses(statements: &[ValueStatement]) -> FutureUses {
    let mut uses = FutureUses::new();
    count_statements(statements, &mut uses);
    uses
}
fn count_statements(statements: &[ValueStatement], uses: &mut FutureUses) {
    for statement in statements {
        match statement {
            ValueStatement::Continue { .. }
            | ValueStatement::Break { .. }
            | ValueStatement::ReturnNoValue { .. } => {}
            ValueStatement::While {
                condition, body, ..
            } => {
                // Finite traversal/suffix bookkeeping only. Recurrent loan
                // obligations are discovered and retained separately, not counted.
                count_expression(condition, uses);
                count_statements(body, uses);
            }
            ValueStatement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                count_expression(condition, uses);
                count_statements(then_body, uses);
                count_statements(else_body, uses);
            }
            ValueStatement::CallNoValue { arguments, .. } => {
                for argument in arguments {
                    count_expression(argument, uses);
                }
            }
            ValueStatement::Let { initializer, .. } => count_expression(initializer, uses),
            ValueStatement::Return { value, .. } | ValueStatement::Assign { value, .. } => {
                count_expression(value, uses)
            }
            ValueStatement::DerefAssign {
                reference, value, ..
            } => {
                *uses.entry(*reference).or_default() += 1;
                count_expression(value, uses);
            }
        }
    }
}
fn count_expression(expression: &TypedExpr, uses: &mut FutureUses) {
    match &expression.kind {
        ExprKind::IfValue {
            condition,
            then_value,
            else_value,
        } => {
            count_expression(condition, uses);
            count_expression(then_value, uses);
            count_expression(else_value, uses);
        }
        ExprKind::Deref { reference } => *uses.entry(*reference).or_default() += 1,
        ExprKind::Parameter(id) => *uses.entry(Place::Parameter(*id)).or_default() += 1,
        ExprKind::Local(id) => *uses.entry(Place::Local(*id)).or_default() += 1,
        ExprKind::Call { arguments, .. } => {
            for argument in arguments {
                count_expression(argument, uses);
            }
        }
        ExprKind::Unary { operand, .. } => count_expression(operand, uses),
        ExprKind::Binary { left, right, .. } => {
            count_expression(left, uses);
            count_expression(right, uses);
        }
        _ => {}
    }
}
fn subtract_uses(future: &mut FutureUses, uses: &FutureUses) {
    for (place, count) in uses {
        let remaining = future.get_mut(place).expect("counted branch use");
        *remaining = remaining
            .checked_sub(*count)
            .expect("branch uses belong to suffix");
    }
}
fn with_branch_uses(continuation: &FutureUses, branch: &FutureUses) -> FutureUses {
    let mut future = continuation.clone();
    for (place, count) in branch {
        *future.entry(*place).or_default() += count;
    }
    future
}

// Finite, syntax-directed summaries. Break/continue replace the same-iteration
// suffix with the target loop's outside continuation. Recurrence is NOT encoded
// here: the independently scoped LoanId obligations remain authoritative.
fn block_future(
    body: &[ValueStatement],
    after: &FutureUses,
    continuing: &FutureUses,
) -> FutureUses {
    let mut future = after.clone();
    for statement in body.iter().rev() {
        future = statement_future(statement, &future, continuing);
    }
    future
}
fn statement_future(
    statement: &ValueStatement,
    after: &FutureUses,
    continuing: &FutureUses,
) -> FutureUses {
    match statement {
        ValueStatement::ReturnNoValue { .. } => FutureUses::new(),
        ValueStatement::Return { value, .. } => {
            let mut uses = FutureUses::new();
            count_expression(value, &mut uses);
            uses
        }
        ValueStatement::Continue { .. } | ValueStatement::Break { .. } => continuing.clone(),
        ValueStatement::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            let mut future = block_future(then_body, after, continuing);
            for (place, count) in block_future(else_body, after, continuing) {
                let entry = future.entry(place).or_default();
                *entry = (*entry).max(count);
            }
            count_expression(condition, &mut future);
            future
        }
        _ => {
            let mut future = after.clone();
            count_statements(std::slice::from_ref(statement), &mut future);
            future
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Fallthrough,
    Continue,
    Break,
    NoFallthrough,
    Return,
}

// Discover syntactic reference-place uses on all checked paths, including nested
// loops. This set creates scoped recurrent obligations; it is not FutureUses.
fn reference_uses(body: &[ValueStatement]) -> BTreeSet<Place> {
    fn expression(expr: &TypedExpr, uses: &mut BTreeSet<Place>) {
        match &expr.kind {
            ExprKind::Parameter(id) => {
                uses.insert(Place::Parameter(*id));
            }
            ExprKind::Local(id) => {
                uses.insert(Place::Local(*id));
            }
            ExprKind::Deref { reference } => {
                uses.insert(*reference);
            }
            ExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    expression(argument, uses);
                }
            }
            ExprKind::IfValue {
                condition,
                then_value,
                else_value,
            } => {
                expression(condition, uses);
                expression(then_value, uses);
                expression(else_value, uses);
            }
            ExprKind::Unary { operand, .. } => expression(operand, uses),
            ExprKind::Binary { left, right, .. } => {
                expression(left, uses);
                expression(right, uses);
            }
            _ => {}
        }
    }
    fn statements(body: &[ValueStatement], uses: &mut BTreeSet<Place>) {
        for statement in body {
            match statement {
                ValueStatement::Continue { .. }
                | ValueStatement::Break { .. }
                | ValueStatement::ReturnNoValue { .. } => {}
                ValueStatement::While {
                    condition, body, ..
                } => {
                    expression(condition, uses);
                    statements(body, uses);
                }
                ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    expression(condition, uses);
                    statements(then_body, uses);
                    statements(else_body, uses);
                }
                ValueStatement::CallNoValue { arguments, .. } => {
                    for argument in arguments {
                        expression(argument, uses);
                    }
                }
                ValueStatement::Let { initializer, .. } => expression(initializer, uses),
                ValueStatement::Assign { value, .. } | ValueStatement::Return { value, .. } => {
                    expression(value, uses)
                }
                ValueStatement::DerefAssign {
                    reference, value, ..
                } => {
                    uses.insert(*reference);
                    expression(value, uses);
                }
            }
        }
    }
    let mut uses = BTreeSet::new();
    statements(body, &mut uses);
    uses
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnershipKind {
    Copy,
    Move,
}
fn ownership_kind(ty: ValueType) -> OwnershipKind {
    match ty {
        ValueType::Bool | ValueType::Range(_) | ValueType::SharedRef(_) => OwnershipKind::Copy,
        ValueType::Record(_) | ValueType::MutableRef(_) => OwnershipKind::Move,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transfer {
    Local(LocalId),
    Argument(ParameterId),
    Return(FunctionId),
    WriteThrough(Place),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Available,
    Moved { span: Span, transfer: Transfer },
    ConditionalMove { span: Span },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LoanId(usize);
#[derive(Clone)]
struct Loan {
    // External reference parameters have no owner identity in this function.
    owner: Option<Place>,
    kind: BorrowKind,
    span: Span,
    held: usize,
}

/// Inspect canonical typed HIR; report the first substantive move/loan error.
/// compile_source runs resolution and typing before this phase.
pub fn check(program: &Program) -> Result<(), Vec<Diagnostic>> {
    for function in program.functions().iter().filter_map(|f| f.as_ordinary()) {
        Checker::new(program, function)
            .body(function)
            .map_err(|e| vec![*e])?;
    }
    Ok(())
}
// Each nested loop owns a distinct header boundary. Branch snapshots inherit it;
// locals introduced in an outer iteration are pre-existing at an inner header.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LoopScope {
    header_places: BTreeSet<Place>,
    header_handles: BTreeSet<Place>,
    header_loans: BTreeSet<LoanId>,
    condition: bool,
}
impl LoopScope {
    fn new(header: &Checker<'_>, condition: bool) -> Self {
        Self {
            header_places: header.states.keys().copied().collect(),
            header_handles: header.handles.keys().copied().collect(),
            header_loans: header.loans.keys().copied().collect(),
            condition,
        }
    }
}
// Each saved header predates pushing its own target. Rc shares immutable
// snapshots among branch copies; nesting is a finite stack, never a CFG solver.
#[derive(Clone)]
struct LoopTarget<'a> {
    header: Rc<Checker<'a>>,
    exit: Rc<Checker<'a>>,
    outside: FutureUses,
}
#[derive(Clone)]
struct Checker<'a> {
    program: &'a Program,
    states: BTreeMap<Place, State>,
    loans: BTreeMap<LoanId, Loan>,
    handles: BTreeMap<Place, LoanId>,
    remaining: FutureUses,
    next_loan: usize,
    loop_scope: Option<LoopScope>,
    // One frame per structured loop; branches inherit all enclosing obligations.
    recurrent: Vec<BTreeSet<LoanId>>,
    targets: Vec<LoopTarget<'a>>,
    // Scoped ONLY to an actual return operand, not Transfer::Return (which also
    // labels non-terminal conditions). Nested argument evaluation inherits it.
    terminal_return: bool,
}
impl<'a> Checker<'a> {
    fn new(program: &'a Program, function: &ValueFunction) -> Self {
        let mut checker = Self {
            program,
            states: BTreeMap::new(),
            loans: BTreeMap::new(),
            handles: BTreeMap::new(),
            remaining: future_uses(&function.body),
            next_loan: 0,
            loop_scope: None,
            recurrent: Vec::new(),
            targets: Vec::new(),
            terminal_return: false,
        };
        for id in &function.parameters {
            if let ParameterType::Value(ty) = program.parameter(*id).ty {
                let place = Place::Parameter(*id);
                if ownership_kind(ty) == OwnershipKind::Move {
                    checker.states.insert(place, State::Available);
                }
                let kind = match ty {
                    ValueType::SharedRef(_) => Some(BorrowKind::Shared),
                    ValueType::MutableRef(_) => Some(BorrowKind::Mutable),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let loan = checker.add_loan(None, kind, program.parameter(*id).span);
                    checker.handles.insert(place, loan);
                }
            }
        }
        checker
    }
    fn body(&mut self, function: &ValueFunction) -> OwnershipResult {
        let flow = self.statements_with(&function.body, function.id, &FutureUses::new())?;
        if function.return_type.is_some() {
            assert_eq!(flow, Flow::Return);
        } else {
            assert!(matches!(flow, Flow::Return | Flow::Fallthrough));
            self.complete_no_value(function.span)?;
        }
        Ok(())
    }
    #[cfg(test)]
    fn statements(
        &mut self,
        statements: &[ValueStatement],
        function: FunctionId,
    ) -> OwnershipResult {
        let mut after = self.remaining.clone();
        subtract_uses(&mut after, &future_uses(statements));
        self.statements_with(statements, function, &after)
            .map(|_| ())
    }
    fn statements_with(
        &mut self,
        statements: &[ValueStatement],
        function: FunctionId,
        after: &FutureUses,
    ) -> OwnershipResult<Flow> {
        let call_before = self.clone();
        let continuing = self
            .targets
            .last()
            .map(|t| t.outside.clone())
            .unwrap_or_default();
        let mut suffix = after.clone();
        let mut summaries = Vec::with_capacity(statements.len());
        for statement in statements.iter().rev() {
            let before = statement_future(statement, &suffix, &continuing);
            summaries.push((before.clone(), suffix));
            suffix = before;
        }
        summaries.reverse();
        self.remaining = suffix;
        self.expire();
        for (statement, (before_uses, after_uses)) in statements.iter().zip(summaries) {
            self.remaining = before_uses;
            self.expire();
            match self.statement(statement, function, &after_uses) {
                Ok(Flow::Fallthrough) => {
                    self.remaining = after_uses;
                    self.expire();
                }
                Ok(flow) => return Ok(flow),
                Err(error) => {
                    // Return owns a narrower transaction: its static operand-only
                    // continuation and pre-operation expiry must NOT be undone.
                    // Enclosing compound operations still retain their own atomicity.
                    if !matches!(
                        statement,
                        ValueStatement::Return { .. } | ValueStatement::ReturnNoValue { .. }
                    ) {
                        *self = call_before;
                    }
                    return Err(error);
                }
            }
        }
        self.remaining = after.clone();
        self.expire();
        Ok(Flow::Fallthrough)
    }
    fn statement(
        &mut self,
        statement: &ValueStatement,
        function: FunctionId,
        after: &FutureUses,
    ) -> OwnershipResult<Flow> {
        match statement {
            ValueStatement::ReturnNoValue { span } => return self.bare_return(*span),
            ValueStatement::CallNoValue {
                function,
                arguments,
                span,
            } => {
                self.call_statement(*function, arguments, *span)?;
            }
            ValueStatement::Continue { span } => return self.continue_edge(*span),
            ValueStatement::Break { span } => return self.break_edge(*span),
            ValueStatement::While {
                condition,
                body,
                span,
            } => {
                self.while_loop(condition, body, *span, function)?;
            }
            ValueStatement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                return self.conditional(condition, then_body, else_body, function, after);
            }
            ValueStatement::Let {
                local, initializer, ..
            } => {
                let loan = self.expression(initializer, Transfer::Local(*local))?;
                let place = Place::Local(*local);
                if ownership_kind(self.program.local(*local).ty) == OwnershipKind::Move {
                    self.states.insert(place, State::Available);
                }
                if let Some(loan) = loan {
                    self.handles.insert(place, loan);
                }
            }
            ValueStatement::Assign { local, value, span } => {
                let place = Place::Local(*local);
                if ownership_kind(self.program.local(*local).ty) != OwnershipKind::Copy {
                    return Err(self.diagnostic(*span, format!("cannot replace non-Copy owned local `{}`", self.metadata(place).0), "replacement would displace an owned value; displacement/destruction semantics are not yet defined; KED-011 permits only Copy-safe direct local reassignment", None));
                }
                // The RHS sees the old value. Unlike write-through, direct
                // assignment does not hold a target loan during RHS evaluation.
                self.expression(value, Transfer::Local(*local))?;
                self.expire();
                self.available(place, *span)?;
                if let Some(loan) = self.conflict(place, Some(BorrowKind::Mutable)) {
                    return Err(self.diagnostic(*span, format!("cannot assign `{}` while it is borrowed", self.metadata(place).0), "direct assignment requires exclusive mutation access; any active shared or exclusive borrow conflicts with the write", Some(loan.span)));
                }
                // Copy replacement leaves the existing owner available and
                // creates neither a new identity nor loan provenance.
            }
            ValueStatement::DerefAssign {
                reference,
                value,
                span,
            } => {
                let loan = self.borrowed_access(*reference, *span, true)?;
                // A final target use must still protect the whole write statement,
                // including RHS dereferences and nested calls that trigger expiry.
                self.loans
                    .get_mut(&loan)
                    .expect("live write provenance")
                    .held += 1;
                self.expression(value, Transfer::WriteThrough(*reference))?;
                // RHS calls may transfer this move-only handle. That must not
                // leave an apparently valid write through a now-moved handle.
                self.available(*reference, *span)?;
                self.loans.get_mut(&loan).expect("write-held loan").held -= 1;
            }
            ValueStatement::Return { value, span } => {
                return self.return_edge(value, function, *span);
            }
        }
        self.expire();
        Ok(Flow::Fallthrough)
    }
    fn loop_reference_error(&self, span: Span) -> Box<Diagnostic> {
        self.diagnostic(span, "reference activity is unsupported in while conditions".into(), "recurring condition reference activity requires future cyclic loan/lifetime analysis; reference access is permitted only in the body", None)
    }
    fn complete_no_value(&self, span: Span) -> OwnershipResult {
        if self.loans.values().any(|loan| loan.held != 0) {
            return Err(self.diagnostic(span, "no-value function completion requires balanced operation holds".into(), "receiving calls and borrowed writes must complete their holds; completion never clears holds or repairs ownership", None));
        }
        Ok(())
    }
    fn bare_return(&mut self, span: Span) -> OwnershipResult<Flow> {
        self.remaining.clear();
        self.expire();
        let before = self.clone(); // Empty summary/expiry precede this snapshot.
                                   // No operand, no terminal-expression permission, no local normalization.
        match self.complete_no_value(span) {
            Ok(()) => Ok(Flow::Return),
            Err(error) => {
                *self = before;
                Err(error)
            }
        }
    }
    fn call_statement(
        &mut self,
        function: FunctionId,
        arguments: &[TypedExpr],
        span: Span,
    ) -> OwnershipResult {
        let before = self.clone(); // Caller prepared finite liveness and expiry.
        self.terminal_return = false;
        let result = (|| {
            self.call_arguments(function, arguments)?;
            // Unlike nested expression calls, the entire statement must balance.
            self.complete_no_value(span)
        })();
        match result {
            Ok(()) => {
                self.terminal_return = before.terminal_return;
                Ok(())
            }
            Err(error) => {
                *self = before;
                Err(error)
            }
        }
    }
    fn call_arguments(&mut self, function: FunctionId, arguments: &[TypedExpr]) -> OwnershipResult {
        let callee = self
            .program
            .function(function)
            .as_ordinary()
            .expect("typed ordinary calls");
        let mut held = Vec::new();
        for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
            if let Some(loan) = self.expression(argument, Transfer::Argument(*parameter))? {
                self.loans
                    .get_mut(&loan)
                    .expect("live reference provenance")
                    .held += 1;
                held.push(loan);
            }
            self.expire();
        }
        // Nested calls release only their own holds, never the receiving call's.
        for id in held {
            self.loans.get_mut(&id).expect("call-held loan").held -= 1;
        }
        self.expire();
        Ok(())
    }
    fn return_edge(
        &mut self,
        value: &TypedExpr,
        function: FunctionId,
        span: Span,
    ) -> OwnershipResult<Flow> {
        // Exact typing already happened. This finite context is static input,
        // not an operand side effect: install/expire BEFORE taking the snapshot.
        let mut uses = FutureUses::new();
        count_expression(value, &mut uses);
        self.remaining = uses;
        self.expire();
        let before = self.clone();
        self.terminal_return = true;
        let result = (|| {
            self.expression(value, Transfer::Return(function))?;
            // Fallible final validation belongs in the same transaction. Never
            // discharge recurrence, reset provenance/allocator or clear holds.
            if self.loans.values().any(|loan| loan.held != 0) {
                return Err(self.diagnostic(span,
                    "return cannot complete with active reference operation holds".into(),
                    "receiving calls and borrowed writes must complete their holds before return; terminal permission does not bypass loan protection", None));
            }
            Ok(Flow::Return)
        })();
        match result {
            Ok(flow) => {
                self.terminal_return = before.terminal_return;
                Ok(flow) // No local successor normalization or ownership-state join.
            }
            Err(error) => {
                *self = before;
                Err(error)
            }
        }
    }
    fn loop_handle(&self, _reference: Place, span: Span) -> OwnershipResult {
        if self
            .loop_scope
            .as_ref()
            .is_some_and(|scope| scope.condition)
        {
            return Err(self.loop_reference_error(span));
        }
        Ok(())
    }
    fn loop_type(&self, ty: ExprType, span: Span) -> OwnershipResult {
        if self
            .loop_scope
            .as_ref()
            .is_some_and(|scope| scope.condition)
        {
            match ty {
                ExprType::SharedRef(_) | ExprType::MutableRef(_) => return Err(self.loop_reference_error(span)),
                ExprType::Record(_) => return Err(self.diagnostic(span, "non-Copy ownership activity is unsupported in while conditions".into(), "recurring condition ownership requires future generalized cyclic ownership analysis; only the loop body permits iteration-local Move values", None)),
                _ => {}
            }
        }
        Ok(())
    }
    fn same_loop_state(&self, header: &Self) -> bool {
        // Compare availability, not diagnostic move sites/transfers. Loan spans,
        // display names and syntactic FutureUses are likewise not semantic state.
        let availability = |state: &State| std::mem::discriminant(state);
        self.states
            .iter()
            .map(|(p, s)| (p, availability(s)))
            .eq(header.states.iter().map(|(p, s)| (p, availability(s))))
            && self.handles == header.handles
            && self
                .loans
                .iter()
                .map(|(id, l)| (id, l.owner, l.kind, l.held))
                .eq(header
                    .loans
                    .iter()
                    .map(|(id, l)| (id, l.owner, l.kind, l.held)))
            && self.next_loan == header.next_loan
            && self.recurrent == header.recurrent
    }
    fn while_loop(
        &mut self,
        condition: &TypedExpr,
        body: &[ValueStatement],
        span: Span,
        function: FunctionId,
    ) -> OwnershipResult {
        self.assert_no_holds();
        let mut outside = self.remaining.clone();
        let mut loop_uses = future_uses(body);
        count_expression(condition, &mut loop_uses);
        subtract_uses(&mut outside, &loop_uses);
        // Establish obligations before any expiry or body operation. No missing
        // provenance may be reconstructed, even on a path skipping its body use.
        let mut iteration = self.clone();
        let mut obligations = BTreeSet::new();
        for place in reference_uses(body) {
            if let Some(id) = iteration.handles.get(&place) {
                if !iteration.loans.contains_key(id) {
                    return Err(self.diagnostic(span, "required loop-carried loan is no longer active".into(), "a carried reference must retain its original continuously active provenance; expired loans cannot be reconstructed", None));
                }
                obligations.insert(*id);
            }
        }
        iteration.recurrent.push(obligations);
        iteration.expire();
        let header = Rc::new(iteration.clone());
        iteration.loop_scope = Some(LoopScope::new(&header, true));
        iteration.expression(condition, Transfer::Return(function))?;
        iteration.expire();
        iteration
            .loop_scope
            .as_mut()
            .expect("current loop")
            .condition = false;
        // Capture the normalized canonical false exit BEFORE inspecting any
        // candidate. A break can validate against it but can never define it.
        let false_exit = Rc::new(iteration.false_exit(&outside, self.loop_scope.clone()));
        iteration.targets.push(LoopTarget {
            header: header.clone(),
            exit: false_exit.clone(),
            outside: outside.clone(),
        });
        if iteration.statements_with(body, function, &outside)? == Flow::Fallthrough {
            iteration.validate_loop_backedge(&header, span)?;
        }
        // Every terminal edge was independently checked. No arbitrary exit join:
        // the independently checked false state is the one canonical successor.
        *self = (*false_exit).clone();
        Ok(())
    }
    fn false_exit(&self, outside: &FutureUses, enclosing_scope: Option<LoopScope>) -> Self {
        let mut exit = self.clone();
        // Install finite outside uses before any post-discharge expiry.
        exit.remaining = outside.clone();
        exit.recurrent.pop().expect("current loop obligation frame");
        exit.loop_scope = enclosing_scope;
        exit.expire();
        exit
    }
    fn break_edge(&mut self, span: Span) -> OwnershipResult<Flow> {
        let before = self.clone();
        let target = self
            .targets
            .last()
            .expect("resolved break inside while")
            .clone();
        let result = (|| {
            self.break_integrity(&target, span)?;
            self.project_break_exit(&target, span)?;
            if !self.same_loop_state(&target.exit) {
                return Err(self.diagnostic(span, "break exit does not match canonical loop-exit ownership and loan state".into(), "break may clean target-iteration locals and end only its own recurrence; carried availability and surviving provenance cannot be repaired or replaced", None));
            }
            Ok(Flow::Break)
        })();
        if result.is_err() {
            *self = before;
        }
        result
    }
    fn break_integrity(&self, target: &LoopTarget<'a>, span: Span) -> OwnershipResult {
        let header = &target.header;
        // Frame evidence would disappear at pop; validate its exact position and
        // membership (including an empty target frame) and every enclosing frame.
        if self.recurrent != header.recurrent || self.recurrent.is_empty() {
            return Err(self.diagnostic(span, "break exit recurrent-obligation integrity check failed".into(), "the exact target frame and every enclosing obligation must remain unchanged until break normalization", None));
        }
        let enclosing_targets_match = self.targets.len() == header.targets.len() + 1
            && self.targets.iter().zip(&header.targets).all(|(a, b)| {
                Rc::ptr_eq(&a.header, &b.header)
                    && Rc::ptr_eq(&a.exit, &b.exit)
                    && a.outside == b.outside
            });
        if !enclosing_targets_match
            || self.loop_scope != Some(LoopScope::new(header, false))
            || target.outside != target.exit.remaining
        {
            return Err(self.diagnostic(span, "break exit target-scope integrity check failed".into(), "break must retain its saved innermost target and canonical outside continuation without changing enclosing loop context", None));
        }
        if self.loans.values().any(|loan| loan.held != 0) {
            return Err(self.diagnostic(span, "break exit has unbalanced operation holds".into(), "call and write holds must complete before loop exit; normalization never clears holds to manufacture equality", None));
        }
        // Carried provenance must be continuously valid BEFORE recurrence-only
        // loans can expire. This is not whole-state header equality: fresh locals,
        // their availability, handles and loans may still exist before cleanup.
        for (place, id) in &header.handles {
            if self.handles.get(place) != Some(id) {
                return Err(self.diagnostic(span, "break exit changed carried reference provenance".into(), "a carried handle cannot be removed, refreshed or replaced, even if exit expiry would hide the change", None));
            }
        }
        for (id, original) in &header.loans {
            if !self
                .loans
                .get(id)
                .is_some_and(|loan| loan.owner == original.owner && loan.kind == original.kind)
            {
                return Err(self.diagnostic(span, "break exit carried-loan integrity check failed".into(), "original carried loan identity, owner, kind and active status must remain continuously valid before target-frame discharge", None));
            }
        }
        if self
            .loans
            .keys()
            .any(|id| id.0 < header.next_loan && !header.loans.contains_key(id))
        {
            return Err(self.diagnostic(
                span,
                "break exit contains resurrected carried provenance".into(),
                "expired loans may not be recreated and then erased by exit normalization",
                None,
            ));
        }
        Ok(())
    }
    fn project_break_exit(&mut self, target: &LoopTarget<'a>, span: Span) -> OwnershipResult {
        // Restricted, non-repairing projection of THIS candidate, never a copy of
        // the canonical exit. The integrity gate must precede these reductions.
        self.remaining = target.outside.clone();
        self.scope_loop_locals(&target.header);
        self.recurrent.pop().expect("validated exact target frame");
        self.targets.pop().expect("validated innermost target");
        self.loop_scope = target.header.loop_scope.clone();
        self.expire();
        self.normalize_loop_allocator(&target.header, span, "break exit")
    }
    fn continue_edge(&mut self, span: Span) -> OwnershipResult<Flow> {
        let before = self.clone();
        let target = self
            .targets
            .last()
            .expect("resolved continue inside while")
            .clone();
        // Cleanup and comparison precede (and never perform) frame discharge.
        match self.validate_loop_backedge(&target.header, span) {
            Ok(()) => Ok(Flow::Continue),
            Err(mut error) => {
                error.message = format!("continue backedge rejected: {}", error.message);
                *self = before;
                Err(error)
            }
        }
    }
    fn validate_loop_backedge(&mut self, header: &Self, span: Span) -> OwnershipResult {
        self.finish_loop_provenance(header, span)?;
        self.assert_no_holds();
        // This check precedes discharge of the current recurrent frame.
        if !self.same_loop_state(header) {
            return Err(self.diagnostic(span, "while loop does not preserve ownership and loan state".into(), "every accepted backedge must preserve carried availability and exact continuously active loan identity/owner/kind; general cyclic ownership/lifetime analysis remains future work", None));
        }
        Ok(())
    }
    fn finish_loop_provenance(&mut self, header: &Self, span: Span) -> OwnershipResult {
        self.scope_loop_locals(header);
        self.expire();
        self.normalize_loop_allocator(header, span, "while backedge")
    }
    fn normalize_loop_allocator(
        &mut self,
        header: &Self,
        span: Span,
        edge: &str,
    ) -> OwnershipResult {
        // Never roll the allocator back until every new loan and every surviving
        // handle referring to newly allocated provenance is proven absent.
        if self.loans.keys().any(|id| !header.loans.contains_key(id))
            || self.handles.values().any(|id| id.0 >= header.next_loan)
        {
            return Err(self.diagnostic(span, format!("iteration-created reference state survives {edge}"), "all newly created provenance must be discharged before allocator normalization; allocator reuse cannot hide surviving handles or loans", None));
        }
        if self.next_loan < header.next_loan {
            return Err(self.diagnostic(span, "while loop does not preserve loan allocation state".into(), "loan allocation must remain monotonic during an iteration; normalization is permitted only after proving newly allocated provenance absent", None));
        }
        self.next_loan = header.next_loan;
        Ok(())
    }
    fn scope_loop_locals(&mut self, header: &Self) {
        // Static body-local IDs may evolve within this iteration, but do not
        // become header state. This is lexical cleanup, not runtime destruction.
        self.states
            .retain(|place, _| header.states.contains_key(place));
        self.handles
            .retain(|place, _| header.handles.contains_key(place));
    }
    fn conditional(
        &mut self,
        condition: &TypedExpr,
        then_body: &[ValueStatement],
        else_body: &[ValueStatement],
        function: FunctionId,
        after: &FutureUses,
    ) -> OwnershipResult<Flow> {
        self.expression(condition, Transfer::Return(function))?;
        self.expire();
        self.assert_no_holds();
        let mut yes = self.clone();
        let mut no = self.clone();
        let yes_flow = yes.statements_with(then_body, function, after)?;
        let no_flow = no.statements_with(else_body, function, after)?;
        match (yes_flow, no_flow) {
            (a, b) if a != Flow::Fallthrough && b != Flow::Fallthrough => {
                return Ok(if a == b { a } else { Flow::NoFallthrough });
            }
            (Flow::Fallthrough, Flow::Fallthrough) => self.join(yes, no, after.clone()),
            _ => {
                let mut survivor = if yes_flow == Flow::Fallthrough {
                    yes
                } else {
                    no
                };
                survivor.assert_no_holds();
                survivor.scope_loop_locals(self);
                survivor.expire();
                *self = survivor;
            }
        }
        Ok(Flow::Fallthrough)
    }
    fn split(&self, then_uses: &FutureUses, else_uses: &FutureUses) -> (Self, Self, FutureUses) {
        let mut continuation = self.remaining.clone();
        subtract_uses(&mut continuation, then_uses);
        subtract_uses(&mut continuation, else_uses);
        self.assert_no_holds();
        let mut then_state = self.clone();
        let mut else_state = self.clone();
        then_state.remaining = with_branch_uses(&continuation, then_uses);
        else_state.remaining = with_branch_uses(&continuation, else_uses);
        then_state.expire();
        else_state.expire();
        (then_state, else_state, continuation)
    }
    fn join(&mut self, then_state: Self, else_state: Self, continuation: FutureUses) {
        then_state.assert_no_holds();
        else_state.assert_no_holds();
        for (place, state) in &mut self.states {
            if *state == State::Available {
                let moved = [then_state.states.get(place), else_state.states.get(place)]
                    .into_iter()
                    .flatten()
                    .find_map(|state| match state {
                        State::Moved { span, .. } | State::ConditionalMove { span } => Some(*span),
                        State::Available => None,
                    });
                if let Some(span) = moved {
                    *state = State::ConditionalMove { span };
                }
            }
        }
        // Only provenance present before the split may survive. Sibling-created
        // loan IDs can overlap privately, but are never merged or allowed to
        // escape through branch-local handles. Activity is a union, not an
        // intersection: a loan required on any incoming path protects the join.
        self.loans
            .retain(|id, _| then_state.loans.contains_key(id) || else_state.loans.contains_key(id));
        self.remaining = continuation;
        self.next_loan = then_state.next_loan.max(else_state.next_loan);
        self.assert_no_holds();
        self.expire();
    }
    fn assert_no_holds(&self) {
        assert!(
            self.loans.values().all(|loan| loan.held == 0),
            "operation holds cannot escape a statement or branch"
        );
    }
    fn expression(
        &mut self,
        expression: &TypedExpr,
        transfer: Transfer,
    ) -> OwnershipResult<Option<LoanId>> {
        let before = self.clone();
        match self.expression_inner(expression, transfer) {
            Ok(loan) => Ok(loan),
            Err(error) => {
                *self = before;
                Err(error)
            }
        }
    }
    fn expression_inner(
        &mut self,
        expression: &TypedExpr,
        transfer: Transfer,
    ) -> OwnershipResult<Option<LoanId>> {
        if self.loop_scope.is_some() {
            self.loop_type(expression.ty, expression.span)?;
        }
        match &expression.kind {
            ExprKind::IfValue {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, transfer)?;
                self.expire();
                let mut then_uses = FutureUses::new();
                let mut else_uses = FutureUses::new();
                count_expression(then_value, &mut then_uses);
                count_expression(else_value, &mut else_uses);
                let (mut then_state, mut else_state, continuation) =
                    self.split(&then_uses, &else_uses);
                // Every leaf transfers into the same source local. Reference
                // results have already been rejected by typing.
                assert!(then_state.expression(then_value, transfer)?.is_none());
                then_state.expire();
                assert!(else_state.expression(else_value, transfer)?.is_none());
                else_state.expire();
                self.join(then_state, else_state, continuation);
                Ok(None)
            }
            ExprKind::Parameter(id) => {
                self.consume(Place::Parameter(*id), expression.span, transfer)
            }
            ExprKind::Local(id) => self.consume(Place::Local(*id), expression.span, transfer),
            ExprKind::Deref { reference } => {
                self.borrowed_access(*reference, expression.span, false)?;
                self.expire();
                // The result is a Copy referent, not a reference argument/handle.
                Ok(None)
            }
            ExprKind::Borrow { kind, place } => {
                self.borrow(*place, *kind, expression.span).map(Some)
            }
            ExprKind::Call {
                function,
                arguments,
            } => {
                self.call_arguments(*function, arguments)?;
                // Reference returns are forbidden: call results are fresh owned values.
                Ok(None)
            }
            ExprKind::Unary { operand, .. } => {
                self.expression(operand, transfer)?;
                Ok(None)
            }
            ExprKind::Binary { left, right, .. } => {
                self.expression(left, transfer)?;
                self.expression(right, transfer)?;
                Ok(None)
            }
            ExprKind::BoolLiteral(_)
            | ExprKind::IntegerLiteral(_)
            | ExprKind::FieldAccess(_)
            | ExprKind::OldField(_) => Ok(None),
        }
    }
    fn borrowed_access(
        &mut self,
        reference: Place,
        span: Span,
        write: bool,
    ) -> OwnershipResult<LoanId> {
        self.loop_handle(reference, span)?;
        self.available(reference, span)?;
        let referent = match self.metadata(reference).1 {
            ValueType::SharedRef(ty) | ValueType::MutableRef(ty) => ty.value_type(),
            _ => unreachable!("typed borrowed access uses reference values"),
        };
        if ownership_kind(referent) == OwnershipKind::Move {
            let ValueType::Record(id) = referent else {
                unreachable!("non-Copy referents are records")
            };
            let name = &self.program.record(id).name;
            let message = if write {
                format!("cannot replace non-Copy value `{name}` through mutable reference before destruction semantics are defined")
            } else {
                format!("cannot move non-Copy value `{name}` out through borrowed reference")
            };
            return Err(self.diagnostic(span, message, "only Copy referents may be materialized or replaced through references; no cloning, displacement, or destruction is implicit", None));
        }
        let id = self.live_handle(reference, span)?;
        if let Some(remaining) = self.remaining.get_mut(&reference) {
            *remaining -= 1;
        }
        Ok(id)
    }
    fn live_handle(&self, place: Place, span: Span) -> OwnershipResult<LoanId> {
        self.handles.get(&place).copied().filter(|id| self.loans.contains_key(id))
            .ok_or_else(|| self.diagnostic(span, format!("reference `{}` no longer has an active loan", self.metadata(place).0), "reference access requires continuously active original provenance; loans cannot resurrect", None))
    }
    fn metadata(&self, place: Place) -> (&str, ValueType) {
        match place {
            Place::Parameter(id) => {
                let parameter = self.program.parameter(id);
                let ParameterType::Value(ty) = parameter.ty else {
                    unreachable!("ordinary places use value parameters")
                };
                (&parameter.name, ty)
            }
            Place::Local(id) => {
                let local = self.program.local(id);
                (&local.name, local.ty)
            }
        }
    }
    fn add_loan(&mut self, owner: Option<Place>, kind: BorrowKind, span: Span) -> LoanId {
        let id = LoanId(self.next_loan);
        self.next_loan += 1;
        self.loans.insert(
            id,
            Loan {
                owner,
                kind,
                span,
                held: 0,
            },
        );
        id
    }
    fn expire(&mut self) {
        let handles = &self.handles;
        let remaining = &self.remaining;
        let states = &self.states;
        let recurrent = &self.recurrent;
        self.loans.retain(|id, loan| {
            loan.held > 0
                || recurrent.iter().any(|frame| frame.contains(id))
                || handles.iter().any(|(place, handle)| {
                    handle == id
                        && remaining.get(place).copied().unwrap_or(0) > 0
                        // ConditionalMove is unusable, but can carry an active
                        // loan on a path where the handle was not transferred.
                        && !matches!(
                            states.get(place),
                            Some(State::Moved { .. })
                        )
                })
        });
    }
    fn conflict(&self, place: Place, requested: Option<BorrowKind>) -> Option<&Loan> {
        let copy = ownership_kind(self.metadata(place).1) == OwnershipKind::Copy;
        self.loans.values().find(|loan| {
            loan.owner == Some(place)
                && match requested {
                    Some(BorrowKind::Shared) => loan.kind == BorrowKind::Mutable,
                    Some(BorrowKind::Mutable) => true,
                    None => !copy || loan.kind == BorrowKind::Mutable,
                }
        })
    }
    fn borrow(&mut self, place: Place, kind: BorrowKind, span: Span) -> OwnershipResult<LoanId> {
        if self
            .loop_scope
            .as_ref()
            .is_some_and(|scope| scope.condition)
        {
            return Err(self.loop_reference_error(span));
        }
        self.available(place, span)?;
        if kind == BorrowKind::Mutable
            && !matches!(place, Place::Local(id) if self.program.local(id).mutable)
        {
            return Err(self.diagnostic(span, format!("cannot mutably borrow immutable value `{}`", self.metadata(place).0), "exclusive borrowing requires a mutable owned local declared with let mut; ordinary owned parameters are immutable", None));
        }
        if let Some(loan) = self.conflict(place, Some(kind)) {
            let name = self.metadata(place).0;
            let message = match (kind, loan.kind) {
                (BorrowKind::Mutable, BorrowKind::Shared) => format!(
                    "cannot create exclusive borrow of `{name}` while shared borrow is active"
                ),
                (BorrowKind::Shared, BorrowKind::Mutable) => format!(
                    "cannot create shared borrow of `{name}` while exclusive borrow is active"
                ),
                (BorrowKind::Mutable, BorrowKind::Mutable) => {
                    format!("cannot create a second exclusive borrow of `{name}`")
                }
                _ => unreachable!("shared loans may overlap"),
            };
            return Err(self.diagnostic(
                span,
                message,
                "exclusive loans conflict with every overlapping loan",
                Some(loan.span),
            ));
        }
        Ok(self.add_loan(Some(place), kind, span))
    }
    fn consume(
        &mut self,
        place: Place,
        span: Span,
        transfer: Transfer,
    ) -> OwnershipResult<Option<LoanId>> {
        if matches!(
            self.metadata(place).1,
            ValueType::SharedRef(_) | ValueType::MutableRef(_)
        ) {
            self.loop_handle(place, span)?;
            if matches!(self.metadata(place).1, ValueType::MutableRef(_))
                && self.loop_scope.as_ref().is_some_and(|scope| {
                    scope.header_handles.contains(&place)
                        || self
                            .handles
                            .get(&place)
                            .is_some_and(|id| scope.header_loans.contains(id))
                })
            {
                return Err(self.diagnostic(span, format!("cannot move loop-carried mutable reference `{}`", self.metadata(place).0), "a carried mutable handle must preserve pre-existing availability and unchanged provenance on loop backedges and canonical exits; by-value arguments remain moves, not implicit reborrows", None));
            }
        }
        if self
            .loop_scope
            .as_ref()
            .is_some_and(|scope| scope.header_places.contains(&place))
            && matches!(self.metadata(place).1, ValueType::Record(_))
            && !self.terminal_return
        {
            return Err(self.diagnostic(span, format!("cannot move pre-existing non-Copy value `{}` in while loop", self.metadata(place).0), "the conservative pre-existing-state boundary applies to loop backedges and canonical exits alike; terminal break paths do not relax it; outside the actual return-expression tree, only iteration-local Move values may transfer in the loop body; future generalized cyclic ownership analysis remains unresolved", None));
        }
        self.available(place, span)?;
        if self.handles.contains_key(&place) {
            self.live_handle(place, span)?;
        }
        if let Some(loan) = self.conflict(place, None) {
            let name = self.metadata(place).0;
            let message = if loan.kind == BorrowKind::Mutable {
                format!("cannot use `{name}` while it is exclusively borrowed")
            } else {
                format!("cannot move `{name}` while it is shared-borrowed")
            };
            return Err(self.diagnostic(span, message, "non-Copy owners cannot move while borrowed; exclusive loans also prevent direct Copy owner access", Some(loan.span)));
        }
        if ownership_kind(self.metadata(place).1) == OwnershipKind::Move {
            self.states.insert(place, State::Moved { span, transfer });
        }
        if let Some(remaining) = self.remaining.get_mut(&place) {
            *remaining -= 1;
        }
        Ok(self.handles.get(&place).copied())
    }
    fn diagnostic(
        &self,
        span: Span,
        message: String,
        required: &str,
        earlier: Option<Span>,
    ) -> Box<Diagnostic> {
        Diagnostic {
            message,
            span,
            required: required.into(),
            known: earlier
                .map(|s| {
                    format!(
                        "the conflicting borrow began at line {}, column {}",
                        s.line, s.column
                    )
                })
                .into_iter()
                .collect(),
            conclusion: "ownership checking stopped; this program has not been established valid"
                .into(),
        }
        .into()
    }
    fn available(&self, place: Place, span: Span) -> OwnershipResult {
        let name = self.metadata(place).0;
        if let Some(State::ConditionalMove { span: earlier }) = self.states.get(&place) {
            return Err(
                Diagnostic {
                    message: format!(
                        "value `{name}` may have been moved on a previous control-flow path"
                    ),
                    span,
                    required: "a non-Copy place must remain available on every incoming branch"
                        .into(),
                    known: vec![format!(
                        "a move occurred at line {}, column {}",
                        earlier.line, earlier.column
                    )],
                    conclusion:
                        "ownership checking stopped; this program has not been established valid"
                            .into(),
                }
                .into(),
            );
        }
        if let Some(State::Moved {
            span: earlier,
            transfer: previous,
        }) = self.states.get(&place)
        {
            let destination = match previous {
                Transfer::WriteThrough(reference) => {
                    format!("through reference `{}`", self.metadata(*reference).0)
                }
                Transfer::Local(id) => format!("into local `{}`", self.program.local(*id).name),
                Transfer::Argument(id) => format!(
                    "into by-value parameter `{}`",
                    self.program.parameter(*id).name
                ),
                Transfer::Return(id) => format!(
                    "by return from function `{}`",
                    self.program.function(*id).name()
                ),
            };
            return Err(
                Diagnostic {
                    message: format!("use of moved value `{name}`"),
                    span,
                    required: "a non-Copy value cannot be reused after transfer; use its new owner"
                        .into(),
                    known: vec![format!(
                        "`{name}` was previously moved {destination} at line {}, column {}",
                        earlier.line, earlier.column
                    )],
                    conclusion:
                        "ownership checking stopped; this program has not been established valid"
                            .into(),
                }
                .into(),
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile_source, parse_source, resolve, types};
    const DECLARATIONS: &str = "type Percent = range 0..100; record Ticket { value: Percent } record Other { value: Percent }";
    fn typed(body: &str) -> Program {
        let ast = parse_source(&format!("{DECLARATIONS} {body}")).unwrap();
        types::check(resolve::resolve(&ast).unwrap()).unwrap()
    }
    #[test]
    fn non_copy_assignment_rejects_before_consuming_either_owner() {
        let source = "type Percent = range 0..100; record Ticket { value: Percent } fn f(first: Ticket, second: Ticket) -> Ticket { let mut current = first; current = second; return current; }";
        let program =
            types::check(resolve::resolve(&parse_source(source).unwrap()).unwrap()).unwrap();
        let function = program.functions()[0].as_ordinary().unwrap();
        let mut checker = Checker::new(&program, function);
        checker
            .statements(&function.body[..1], function.id)
            .unwrap();
        let before = checker.states.clone();
        let error = checker
            .statements(&function.body[1..2], function.id)
            .unwrap_err();
        assert!(error.message.contains("cannot replace non-Copy"));
        assert_eq!(before, checker.states);
        assert_eq!(
            checker.states[&Place::Local(program.locals()[0].id)],
            State::Available
        );
        assert_eq!(
            checker.states[&Place::Parameter(function.parameters[1])],
            State::Available
        );
        assert!(checker.loans.is_empty());
    }

    #[test]
    fn classification_uses_canonical_value_types_not_aggregate_fields_or_names() {
        let mut program = compile_source(DECLARATIONS).unwrap();
        for range in &mut program.ranges {
            range.name = "display".into();
        }
        for record in &mut program.records {
            record.name = "display".into();
        }
        assert_eq!(ownership_kind(ValueType::Bool), OwnershipKind::Copy);
        assert_eq!(
            ownership_kind(ValueType::Range(program.ranges()[0].id)),
            OwnershipKind::Copy
        );
        for record in program.records() {
            assert_eq!(
                ownership_kind(ValueType::Record(record.id)),
                OwnershipKind::Move
            );
        }
    }
    #[test]
    fn parameters_start_available_and_return_consumes_the_new_local_owner() {
        let program = typed("fn forward(ticket: Ticket, unused: Ticket) -> Ticket { let next = ticket; return next; }");
        let f = program.functions()[0].as_ordinary().unwrap();
        let mut checker = Checker::new(&program, f);
        assert_eq!(
            checker.states[&Place::Parameter(f.parameters[0])],
            State::Available
        );
        assert_eq!(
            checker.states[&Place::Parameter(f.parameters[1])],
            State::Available
        );
        checker.body(f).unwrap();
        let local = program.locals()[0].id;
        assert!(
            matches!(checker.states[&Place::Parameter(f.parameters[0])], State::Moved { transfer: Transfer::Local(id), .. } if id == local)
        );
        assert!(
            matches!(checker.states[&Place::Local(local)], State::Moved { transfer: Transfer::Return(id), .. } if id == f.id)
        );
        assert_eq!(
            checker.states[&Place::Parameter(f.parameters[1])],
            State::Available
        );
    }
    #[test]
    fn ownership_state_is_isolated_to_each_function_and_copy_values_are_not_consumed() {
        let program = typed("fn first(ticket: Ticket) -> Ticket { return ticket; } fn second(ticket: Ticket, value: Percent) -> Ticket { let first = value; let second = value; return ticket; }");
        let first = program.functions()[0].as_ordinary().unwrap();
        let second = program.functions()[1].as_ordinary().unwrap();
        let mut checker = Checker::new(&program, first);
        checker.body(first).unwrap();
        let mut checker = Checker::new(&program, second);
        assert!(!checker
            .states
            .contains_key(&Place::Parameter(first.parameters[0])));
        checker.body(second).unwrap();
        assert_eq!(checker.states.len(), 1);
        assert!(!checker
            .states
            .contains_key(&Place::Parameter(second.parameters[1])));
        for local in program.locals() {
            assert!(!checker.states.contains_key(&Place::Local(local.id)));
        }
    }
    #[test]
    fn parameter_local_function_and_type_metadata_renaming_preserves_ownership() {
        fn rename(program: &mut Program) {
            for range in &mut program.ranges {
                range.name = "display".into();
            }
            for record in &mut program.records {
                record.name = "display".into();
            }
            for parameter in &mut program.parameters {
                parameter.name = "display".into();
            }
            for local in &mut program.locals {
                local.name = "display".into();
            }
            for f in &mut program.functions {
                if let crate::hir::Function::Ordinary(f) = f {
                    f.name = "display".into();
                }
            }
        }
        let identity = "fn identity(ticket: Ticket) -> Ticket { return ticket; }";
        let valid = format!("{identity} fn forward(ticket: Ticket) -> Ticket {{ let first = identity(ticket); let next = first; return next; }}");
        let mut program = typed(&valid);
        rename(&mut program);
        check(&program).unwrap();
        for body in ["fn bad(ticket: Ticket) -> Ticket { let next = ticket; return ticket; }", "fn bad(ticket: Ticket) -> Ticket { let first = ticket; let next = first; return first; }", "fn bad(ticket: Ticket) -> Ticket { let next = identity(ticket); return ticket; }"] {
            let mut program = typed(&format!("{identity} {body}"));
            let original = check(&program).unwrap_err();
            rename(&mut program);
            let renamed = check(&program).unwrap_err();
            assert_eq!(renamed.len(), 1);
            assert_eq!(original[0].span, renamed[0].span);
            assert_eq!(renamed[0].message, "use of moved value `display`");
            assert!(renamed[0].known[0].contains("`display`"));
        }
    }
    #[test]
    fn loan_provenance_liveness_and_conflicts_ignore_all_display_names() {
        fn rename(program: &mut Program) {
            for range in &mut program.ranges {
                range.name = "display".into();
            }
            for record in &mut program.records {
                record.name = "display".into();
            }
            for field in &mut program.fields {
                field.name = "display".into();
            }
            for parameter in &mut program.parameters {
                parameter.name = "display".into();
            }
            for local in &mut program.locals {
                local.name = "display".into();
            }
            for f in &mut program.functions {
                if let crate::hir::Function::Ordinary(f) = f {
                    f.name = "display".into();
                }
            }
        }
        let declarations = "fn inspect(value: &Ticket) -> bool { return true; } fn exclusive(value: &mut Ticket) -> bool { return true; }";
        for body in [
            "let view = &ticket; let first = view; let second = view; let a = inspect(first); let b = inspect(second); return ticket;",
            "let mut owned = ticket; let first = &mut owned; let second = first; let a = exclusive(second); let b = exclusive(&mut owned); return owned;",
            "let view = &ticket; let unused = view; return ticket;",
        ] {
            let mut program = typed(&format!("{declarations} fn example(ticket: Ticket) -> Ticket {{ {body} }}"));
            let original = program.functions()[2].as_ordinary().unwrap().body.clone();
            check(&program).unwrap();
            rename(&mut program);
            assert_eq!(program.functions()[2].as_ordinary().unwrap().body, original);
            check(&program).unwrap();
        }
        for body in [
            "let view = &ticket; let alias = view; let moved = ticket; let ok = inspect(alias); return moved;",
            "let mut owned = ticket; let access = &mut owned; let next = access; let view = &owned; let ok = exclusive(next); return owned;",
            "let mut owned = ticket; let access = &mut owned; let ok = exclusive(access); let bad = exclusive(access); return owned;",
        ] {
            let mut program = typed(&format!("{declarations} fn example(ticket: Ticket) -> Ticket {{ {body} }}"));
            let original = check(&program).unwrap_err();
            rename(&mut program);
            let renamed = check(&program).unwrap_err();
            assert_eq!(original[0].span, renamed[0].span);
            assert!(renamed[0].message.contains("`display`"));
            assert!(!renamed[0].message.contains("LoanId"));
            assert_eq!(original[0].known.len(), renamed[0].known.len());
        }
    }
    #[test]
    fn reference_classification_and_external_provenance_are_function_local() {
        let program = typed("fn sink(value: &mut Ticket) -> bool { return true; } fn relay(value: &mut Ticket) -> bool { let next = value; return sink(next); } fn shared(value: &Ticket) -> bool { let first = value; let second = value; return true; }");
        let referent = crate::hir::ReferentType::Record(program.records()[0].id);
        assert_eq!(
            ownership_kind(ValueType::SharedRef(referent)),
            OwnershipKind::Copy
        );
        assert_eq!(
            ownership_kind(ValueType::MutableRef(referent)),
            OwnershipKind::Move
        );
        let f = program.functions()[1].as_ordinary().unwrap();
        let mut checker = Checker::new(&program, f);
        assert_eq!(checker.loans.len(), 1);
        assert!(checker.loans.values().all(|loan| loan.owner.is_none()));
        checker.body(f).unwrap();
        assert!(checker.loans.is_empty());
        assert!(matches!(
            checker.states[&Place::Parameter(f.parameters[0])],
            State::Moved { .. }
        ));
        let f = program.functions()[2].as_ordinary().unwrap();
        let mut checker = Checker::new(&program, f);
        assert!(checker.states.is_empty());
        checker.body(f).unwrap();
        assert!(checker.states.is_empty());
        assert!(checker.loans.is_empty());
    }
    #[test]
    fn dereference_and_write_identity_remain_independent_of_diagnostic_names() {
        fn rename(program: &mut Program) {
            for range in &mut program.ranges {
                range.name = "display".into();
            }
            for record in &mut program.records {
                record.name = "display".into();
            }
            for field in &mut program.fields {
                field.name = "display".into();
            }
            for parameter in &mut program.parameters {
                parameter.name = "display".into();
            }
            for local in &mut program.locals {
                local.name = "display".into();
            }
            for f in &mut program.functions {
                if let crate::hir::Function::Ordinary(f) = f {
                    f.name = "display".into();
                }
            }
        }
        for body in [
            "fn good(value: Percent, replacement: Percent) -> Percent { let mut owned = value; let view = &owned; let alias = view; let before = *view; let after = *alias; let access = &mut owned; *access = replacement; *access = *access; return owned; }",
            "fn good(value: &mut Percent) -> Percent { let first = value; let observed = *first; return *first; }",
        ] {
            let mut program = typed(body);
            let original = program.functions()[0].as_ordinary().unwrap().body.clone();
            check(&program).unwrap();
            rename(&mut program);
            assert_eq!(original, program.functions()[0].as_ordinary().unwrap().body);
            check(&program).unwrap();
        }
        for body in [
            "fn bad(value: &Ticket) -> Ticket { return *value; }",
            "fn bad(value: &mut Ticket, replacement: Ticket) -> bool { *value = replacement; return true; }",
            "fn bad(value: Percent) -> Percent { let mut owned = value; let access = &mut owned; let next = access; return *access; }",
            "fn bad(value: Percent) -> Percent { let mut owned = value; let access = &mut owned; *access = owned; return owned; }",
            "fn bad(value: Percent) -> Percent { let mut owned = value; let view = &owned; let access = &mut owned; return *view; }",
        ] {
            let mut program = typed(body);
            let original = check(&program).unwrap_err();
            rename(&mut program);
            let renamed = check(&program).unwrap_err();
            assert_eq!(original[0].span, renamed[0].span);
            assert!(renamed[0].message.contains("`display`"));
            assert_eq!(original[0].known.len(), renamed[0].known.len());
            assert!(!renamed[0].message.contains("LoanId"));
        }
    }
    #[test]
    fn borrowed_copy_access_preserves_handle_availability_and_loan_identity() {
        let program = typed("fn update(value: &mut Percent, replacement: Percent) -> Percent { let before = *value; *value = replacement; return *value; }");
        let f = program.functions()[0].as_ordinary().unwrap();
        let reference = Place::Parameter(f.parameters[0]);
        let mut checker = Checker::new(&program, f);
        let original_loan = checker.handles[&reference];
        let ValueStatement::Let {
            local, initializer, ..
        } = &f.body[0]
        else {
            panic!("let")
        };
        assert_eq!(
            checker
                .expression(initializer, Transfer::Local(*local))
                .unwrap(),
            None
        );
        assert_eq!(checker.states[&reference], State::Available);
        assert_eq!(checker.handles[&reference], original_loan);
        assert_eq!(checker.loans.len(), 1);
        assert_eq!(checker.next_loan, 1);
        // Full read/write/read leaves the handle available and releases the original
        // external loan after last use; no replacement/destruction state is added.
        let mut checker = Checker::new(&program, f);
        checker.body(f).unwrap();
        assert_eq!(checker.states[&reference], State::Available);
        assert_eq!(checker.next_loan, 1);
        assert!(checker.loans.is_empty());
    }
    #[test]
    fn possible_joined_loan_survives_conditional_handle_unavailability() {
        let p = typed("fn sink(access: &mut Percent) -> bool { return true; } fn bad(flag: bool, value: Percent) -> Percent { let mut owned = value; let access = &mut owned; if flag { let done = sink(access); } let observed = owned; return *access; }");
        let f = p.functions()[1].as_ordinary().unwrap();
        let mut checker = Checker::new(&p, f);
        checker.statements(&f.body[..3], f.id).unwrap();
        let access = Place::Local(p.locals()[1].id);
        assert!(matches!(
            checker.states[&access],
            State::ConditionalMove { .. }
        ));
        let loan = checker.handles[&access];
        assert!(checker.loans.contains_key(&loan));
        checker.expire();
        assert!(checker.loans.contains_key(&loan));
        assert!(checker.loans.values().all(|loan| loan.held == 0));
        let error = checker.statements(&f.body[3..], f.id).unwrap_err();
        assert!(error.message.contains("exclusively borrowed"));
    }
    #[test]
    fn all_path_expiry_does_not_restore_the_moved_handle_or_export_branch_loans() {
        let p = typed("fn sink(access: &mut Percent) -> bool { return true; } fn example(flag: bool, value: Percent) -> Percent { let mut owned = value; let access = &mut owned; if flag { let next = access; let done = sink(next); } else { let other = &mut owned; *other = value; } return owned; }");
        let f = p.functions()[1].as_ordinary().unwrap();
        let mut checker = Checker::new(&p, f);
        checker.statements(&f.body[..3], f.id).unwrap();
        assert!(matches!(
            checker.states[&Place::Local(p.locals()[1].id)],
            State::ConditionalMove { .. }
        ));
        assert!(checker.loans.is_empty());
        assert_eq!(checker.handles.len(), 1);
        assert!(!checker
            .handles
            .contains_key(&Place::Local(p.locals()[2].id)));
        assert!(!checker
            .handles
            .contains_key(&Place::Local(p.locals()[4].id)));
        checker.statements(&f.body[3..], f.id).unwrap();
    }
    #[test]
    fn future_use_decomposition_preserves_suffix_and_excludes_sibling_counts() {
        let p = typed("fn example(flag: bool, view: &Percent, access: &mut Percent) -> Percent { if flag { let observed = *view; } else { let observed = *access; } return *view; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let ValueStatement::If {
            condition,
            then_body,
            else_body,
            ..
        } = &f.body[0]
        else {
            panic!()
        };
        let mut all = future_uses(&f.body);
        let mut condition_uses = FutureUses::new();
        count_expression(condition, &mut condition_uses);
        subtract_uses(&mut all, &condition_uses);
        let then_uses = future_uses(then_body);
        let else_uses = future_uses(else_body);
        subtract_uses(&mut all, &then_uses);
        subtract_uses(&mut all, &else_uses);
        let view = Place::Parameter(f.parameters[1]);
        let access = Place::Parameter(f.parameters[2]);
        let then_future = with_branch_uses(&all, &then_uses);
        let else_future = with_branch_uses(&all, &else_uses);
        assert_eq!(then_future[&view], 2);
        assert_eq!(then_future[&access], 0);
        assert_eq!(else_future[&view], 1);
        assert_eq!(else_future[&access], 1);
    }
    #[test]
    fn path_liveness_outcomes_ignore_all_display_names() {
        fn rename(p: &mut Program) {
            for r in &mut p.ranges {
                r.name = "display".into();
            }
            for r in &mut p.records {
                r.name = "display".into();
            }
            for field in &mut p.fields {
                field.name = "display".into();
            }
            for parameter in &mut p.parameters {
                parameter.name = "display".into();
            }
            for local in &mut p.locals {
                local.name = "display".into();
            }
            for f in &mut p.functions {
                if let crate::hir::Function::Ordinary(f) = f {
                    f.name = "display".into();
                }
            }
        }
        for body in [
            "fn example(flag: bool, value: Percent) -> Percent { let mut owned = value; let view = &owned; if flag { let alias = view; let observed = *alias; } else { let access = &mut owned; *access = value; } return owned; }",
            "fn example(flag: bool, value: Percent) -> Percent { let mut owned = value; let first = &mut owned; if flag { let second = first; *second = value; } else { let observed = owned; } return owned; }",
        ] {
            let mut p = typed(body);
            check(&p).unwrap();
            let before = crate::mir::lower(&p);
            rename(&mut p);
            check(&p).unwrap();
            assert_eq!(before, crate::mir::lower(&p));
        }
        for body in [
            "fn bad(flag: bool, value: Percent) -> Percent { let mut owned = value; let view = &owned; if flag { let observed = *view; } else { let access = &mut owned; } return *view; }",
            "fn sink(access: &mut Percent) -> bool { return true; } fn bad(flag: bool, value: Percent) -> Percent { let mut owned = value; let access = &mut owned; if flag { let done = sink(access); } let observed = owned; return *access; }",
        ] {
            let mut p = typed(body);
            let before = check(&p).unwrap_err();
            rename(&mut p);
            let after = check(&p).unwrap_err();
            assert_eq!(before[0].span, after[0].span);
            assert_eq!(before[0].known, after[0].known);
            assert!(after[0].message.contains("display"));
        }
    }
    #[test]
    fn loops_preserve_header_state_and_scope_nested_copy_locals() {
        let p = typed("fn f(flag: bool, value: Percent, ticket: Ticket) -> Percent { let mut owned = value; let view = &owned; while flag { let copied = owned; if flag { let observed = copied; } while flag { let nested = copied; } } return *view; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        c.expire();
        let header = c.clone();
        assert_eq!(header.loans.len(), 1);
        c.statements(&f.body[2..3], f.id).unwrap();
        assert!(c.same_loop_state(&header));
        assert_ne!(c.remaining, header.remaining);
        assert!(c.loop_scope.is_none());
        assert_eq!(c.states.len(), 1); // Untouched record parameter only.
        assert_eq!(c.handles.len(), 1); // No body-local provenance escapes.
        c.statements(&f.body[3..], f.id).unwrap();
        assert!(c.loans.is_empty());
    }
    #[test]
    fn expired_loans_do_not_resurrect_at_backedges() {
        let p = typed("fn f(flag: bool, value: Percent) -> Percent { let mut owned = value; let view = &owned; let observed = *view; while flag { owned = value; while flag { owned = owned; } } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..3], f.id).unwrap();
        assert!(c.loans.is_empty());
        let header = c.clone();
        c.statements(&f.body[3..4], f.id).unwrap();
        assert!(c.same_loop_state(&header));
        assert!(c.loans.is_empty());
    }
    #[test]
    fn forbidden_loop_activity_cannot_silently_change_outer_state() {
        for (body, message) in [
            ("fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { let moved = ticket; } return ticket; }", "pre-existing non-Copy value"),
            ("fn f(flag: bool, access: &mut Percent, value: Percent) -> Percent { while flag { let moved = access; } return value; }", "loop-carried mutable reference"),
        ] {
            let p = typed(body);
            let f = p.functions()[0].as_ordinary().unwrap();
            let mut c = Checker::new(&p, f);
            c.expire();
            let header = c.clone();
            let error = c.statements(&f.body[..1], f.id).unwrap_err();
            assert!(error.message.contains(message));
            assert!(error.required.contains("unchanged provenance") || error.required.contains("future generalized cyclic"));
            assert!(c.same_loop_state(&header));
        }
    }
    #[test]
    fn loop_state_equality_detects_drift_but_ignores_display_metadata() {
        let mut p = typed("fn f(flag: bool, value: Percent, ticket: Ticket) -> Percent { let mut owned = value; let view = &owned; while flag { let copied = owned; } return *view; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let mut drift = c.clone();
        drift.states.insert(
            Place::Parameter(f.parameters[2]),
            State::ConditionalMove { span: f.span },
        );
        assert!(!drift.same_loop_state(&c));
        let mut drift = c.clone();
        drift.handles.clear();
        assert!(!drift.same_loop_state(&c));
        let mut drift = c.clone();
        drift.loans.clear();
        assert!(!drift.same_loop_state(&c));
        let mut drift = c.clone();
        drift.next_loan += 1;
        assert!(!drift.same_loop_state(&c));
        let mut metadata = c.clone();
        for loan in metadata.loans.values_mut() {
            loan.span = Span {
                start: 0,
                end: 0,
                ..f.span
            };
        }
        assert!(metadata.same_loop_state(&c));
        let owner = Place::Parameter(f.parameters[2]);
        metadata.states.insert(
            owner,
            State::Moved {
                span: f.span,
                transfer: Transfer::Return(f.id),
            },
        );
        let mut same_availability = metadata.clone();
        same_availability.states.insert(
            owner,
            State::Moved {
                span: Span {
                    start: 0,
                    end: 0,
                    ..f.span
                },
                transfer: Transfer::Local(p.locals()[0].id),
            },
        );
        assert!(same_availability.same_loop_state(&metadata));
        let before = crate::mir::lower(&p);
        check(&p).unwrap();
        for local in &mut p.locals {
            local.name = "display".into();
        }
        for param in &mut p.parameters {
            param.name = "display".into();
        }
        for range in &mut p.ranges {
            range.name = "display".into();
        }
        for f in &mut p.functions {
            if let crate::hir::Function::Ordinary(f) = f {
                f.name = "display".into();
            }
        }
        check(&p).unwrap();
        assert_eq!(before, crate::mir::lower(&p));
    }
    #[test]
    fn iteration_local_move_states_evolve_then_leave_header_unchanged() {
        let p = typed("fn fresh() -> Ticket { return fresh(); } fn consume(value: Ticket) -> bool { return true; } fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { let first = fresh(); let second = first; let unused = fresh(); let done = consume(second); } return ticket; }");
        let f = p.functions()[2].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.expire();
        let header = c.clone();
        let ValueStatement::While {
            condition, body, ..
        } = &f.body[0]
        else {
            panic!()
        };
        let mut iteration = c.clone();
        iteration.loop_scope = Some(LoopScope::new(&header, true));
        iteration
            .expression(condition, Transfer::Return(f.id))
            .unwrap();
        iteration.loop_scope.as_mut().unwrap().condition = false;
        iteration.statements(body, f.id).unwrap();
        let places: Vec<_> = body
            .iter()
            .map(|s| match s {
                ValueStatement::Let { local, .. } => Place::Local(*local),
                _ => panic!(),
            })
            .collect();
        assert!(matches!(iteration.states[&places[0]], State::Moved { .. }));
        assert!(matches!(iteration.states[&places[1]], State::Moved { .. }));
        assert_eq!(iteration.states[&places[2]], State::Available);
        assert!(!iteration.same_loop_state(&header));
        iteration.scope_loop_locals(&header);
        assert!(iteration.same_loop_state(&header));
        assert_eq!(iteration.states.len(), 1);
        // Actual loop checking uses the same cleanup, including owners not consumed
        // explicitly; scope exit does not imply runtime destruction or cleanup code.
        c.statements(&f.body[..1], f.id).unwrap();
        assert!(c.same_loop_state(&header));
        assert!(c.loop_scope.is_none());
        c.statements(&f.body[1..], f.id).unwrap();
    }
    #[test]
    fn carried_move_and_replacement_failures_preserve_original_checker_state() {
        for body in [
            "fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { let moved = ticket; } return ticket; }",
            "fn consume(value: Ticket) -> bool { return true; } fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { let done = consume(ticket); } return ticket; }",
            "fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { if flag { let moved = ticket; } } return ticket; }",
            "fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { while flag { let moved = ticket; } } return ticket; }",
            "fn fresh() -> Ticket { return fresh(); } fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { let mut local = fresh(); local = ticket; } return ticket; }",
            "fn fresh() -> Ticket { return fresh(); } fn pair(first: Ticket, second: Ticket) -> bool { return true; } fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { let local = fresh(); let done = pair(local, ticket); } return ticket; }",
        ] {
            let p = typed(body);
            let f = p.functions().last().unwrap().as_ordinary().unwrap();
            let mut c = Checker::new(&p, f);
            c.expire();
            let header = c.clone();
            let error = c.statements(&f.body[..1], f.id).unwrap_err();
            assert!(error.message.contains("pre-existing non-Copy") || error.message.contains("cannot replace non-Copy"));
            assert!(c.same_loop_state(&header));
            assert_eq!(c.remaining, header.remaining);
            // The transfer guard itself is atomic, independently of loop cloning.
            c.loop_scope = Some(LoopScope::new(&header, false));
            assert!(c.consume(Place::Parameter(f.parameters[1]), f.span, Transfer::Return(f.id)).is_err());
            assert!(c.same_loop_state(&header));
            assert_eq!(c.remaining, header.remaining);
        }
    }
    #[test]
    fn nested_loop_header_protects_outer_iteration_owner_without_mutation() {
        let p = typed("fn fresh() -> Ticket { return fresh(); } fn f(flag: bool) -> bool { while flag { let outer = fresh(); while flag { let moved = outer; } } return flag; }");
        let f = p.functions()[1].as_ordinary().unwrap();
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        let mut c = Checker::new(&p, f);
        c.loop_scope = Some(LoopScope::new(&c, false));
        c.statements(&body[..1], f.id).unwrap();
        let ValueStatement::Let { local, .. } = body[0] else {
            panic!()
        };
        let owner = Place::Local(local);
        assert!(!c
            .loop_scope
            .as_ref()
            .unwrap()
            .header_places
            .contains(&owner));
        let outer = c.clone();
        let error = c.statements(&body[1..], f.id).unwrap_err();
        assert!(error
            .message
            .contains("pre-existing non-Copy value `outer`"));
        assert_eq!(c.states[&owner], State::Available);
        assert!(c.same_loop_state(&outer));
        assert_eq!(c.remaining, outer.remaining);
    }
    #[test]
    fn lexical_cleanup_cannot_hide_drift_of_a_header_owner() {
        let p = typed("fn fresh() -> Ticket { return fresh(); } fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { let local = fresh(); } return ticket; }");
        let f = p.functions()[1].as_ordinary().unwrap();
        let c = Checker::new(&p, f);
        let mut drift = c.clone();
        drift.states.insert(
            Place::Parameter(f.parameters[1]),
            State::Moved {
                span: f.span,
                transfer: Transfer::Return(f.id),
            },
        );
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        let ValueStatement::Let { local, .. } = body[0] else {
            panic!()
        };
        drift.states.insert(Place::Local(local), State::Available);
        drift.scope_loop_locals(&c);
        assert!(!drift.states.contains_key(&Place::Local(local)));
        assert!(!drift.same_loop_state(&c));
    }
    #[test]
    fn iteration_local_acceptance_and_carried_rejections_ignore_display_names() {
        fn rename(p: &mut Program) {
            for r in &mut p.ranges {
                r.name = "display".into();
            }
            for r in &mut p.records {
                r.name = "display".into();
            }
            for field in &mut p.fields {
                field.name = "display".into();
            }
            for parameter in &mut p.parameters {
                parameter.name = "display".into();
            }
            for local in &mut p.locals {
                local.name = "display".into();
            }
            for function in &mut p.functions {
                if let crate::hir::Function::Ordinary(f) = function {
                    f.name = "display".into();
                }
            }
        }
        for body in [
            "fn fresh() -> Ticket { return fresh(); } fn consume(ticket: Ticket) -> bool { return true; } fn f(flag: bool) -> bool { while flag { let first = fresh(); let second = first; if flag { let done = consume(second); } else { let done = consume(second); } } return flag; }",
            "fn fresh() -> Ticket { return fresh(); } fn consume(ticket: Ticket) -> bool { return true; } fn f(flag: bool) -> bool { while flag { let first = fresh(); while flag { let second = fresh(); let done = consume(second); } let done = consume(first); } return flag; }",
        ] {
            let mut p = typed(body);
            check(&p).unwrap();
            let before = crate::mir::lower(&p);
            rename(&mut p);
            check(&p).unwrap();
            assert_eq!(before, crate::mir::lower(&p));
        }
        for body in [
            "fn f(flag: bool, ticket: Ticket) -> bool { while flag { let moved = ticket; } return flag; }",
            "fn fresh() -> Ticket { return fresh(); } fn f(flag: bool) -> bool { while flag { let outer = fresh(); while flag { let moved = outer; } } return flag; }",
        ] {
            let mut p = typed(body);
            let before = check(&p).unwrap_err();
            rename(&mut p);
            let after = check(&p).unwrap_err();
            assert_eq!(before[0].span, after[0].span);
            assert_eq!(before[0].required, after[0].required);
            assert_eq!(before[0].known, after[0].known);
            assert!(after[0].message.contains("pre-existing non-Copy value `display`"));
        }
    }
    #[test]
    fn header_handle_transfer_and_condition_guards_are_atomic() {
        let p = typed("fn f(flag: bool, access: &mut Percent, value: Percent) -> Percent { while flag { let next = access; } return value; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.expire();
        let header = c.clone();
        let reference = Place::Parameter(f.parameters[1]);
        c.loop_scope = Some(LoopScope::new(&header, false));
        assert!(c
            .consume(reference, f.span, Transfer::Return(f.id))
            .unwrap_err()
            .message
            .contains("loop-carried mutable reference"));
        assert!(c.same_loop_state(&header));
        assert_eq!(c.remaining, header.remaining);
        c.loop_scope.as_mut().unwrap().condition = true;
        for write in [false, true] {
            assert!(c
                .borrowed_access(reference, f.span, write)
                .unwrap_err()
                .message
                .contains("while conditions"));
        }
        assert!(c.same_loop_state(&header));
        assert_eq!(c.remaining, header.remaining);
        c.assert_no_holds();
    }
    #[test]
    fn body_loans_and_aliases_evolve_then_cleanup_restores_header_allocator() {
        let p = typed("fn f(flag: bool, value: Percent) -> Percent { let mut current = value; let outside = &value; while flag { let view = &value; let alias = view; let observed = *alias; let access = &mut current; *access = observed; } return *outside; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let header = c.clone();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let mut iteration = c.clone();
        iteration.loop_scope = Some(LoopScope::new(&header, false));
        iteration.statements(&body[..2], f.id).unwrap();
        let ValueStatement::Let { local: view, .. } = body[0] else {
            panic!()
        };
        let ValueStatement::Let { local: alias, .. } = body[1] else {
            panic!()
        };
        let provenance = iteration.handles[&Place::Local(view)];
        assert_eq!(provenance, iteration.handles[&Place::Local(alias)]);
        assert_eq!(provenance.0, header.next_loan);
        assert!(iteration.loans.contains_key(&provenance));
        iteration.statements(&body[2..], f.id).unwrap();
        assert!(iteration.next_loan > header.next_loan);
        assert!(iteration.handles.len() > header.handles.len());
        iteration.finish_loop_provenance(&header, *span).unwrap();
        assert!(iteration.same_loop_state(&header));
        assert_eq!(iteration.next_loan, header.next_loan);
        c.statements(&f.body[2..3], f.id).unwrap();
        assert!(c.same_loop_state(&header));
    }
    #[test]
    fn allocator_normalization_cannot_hide_surviving_loan_or_handle() {
        let p = typed("fn f(view: &Percent, value: Percent) -> Percent { return *view; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let header = Checker::new(&p, f);
        let reference = Place::Parameter(f.parameters[0]);
        let owner = Place::Parameter(f.parameters[1]);
        for dangling in [false, true] {
            let mut broken = header.clone();
            let new = broken.add_loan(Some(owner), BorrowKind::Shared, f.span);
            // Fault injection: a header handle must never acquire new provenance.
            broken.handles.insert(reference, new);
            if dangling {
                broken.loans.remove(&new);
            }
            let allocated = broken.next_loan;
            let error = broken.finish_loop_provenance(&header, f.span).unwrap_err();
            assert!(error
                .message
                .contains("iteration-created reference state survives"));
            assert_eq!(broken.next_loan, allocated);
            assert_eq!(broken.handles[&reference], new);
        }
        let mut broken = header.clone();
        let new = broken.add_loan(Some(owner), BorrowKind::Mutable, f.span);
        broken.loans.get_mut(&new).unwrap().held = 1; // Escaped operation hold.
        let allocated = broken.next_loan;
        assert!(broken.finish_loop_provenance(&header, f.span).is_err());
        assert_eq!(broken.next_loan, allocated);
        assert!(broken.loans.contains_key(&new));
        let mut broken = header.clone();
        broken.next_loan -= 1;
        assert!(broken
            .finish_loop_provenance(&header, f.span)
            .unwrap_err()
            .message
            .contains("allocation state"));
        assert_ne!(broken.next_loan, header.next_loan);
    }
    #[test]
    fn lexical_handle_cleanup_expires_loan_before_safe_allocator_reuse() {
        let p = typed(
            "fn f(value: Percent) -> Percent { while false { let view = &value; } return value; }",
        );
        let f = p.functions()[0].as_ordinary().unwrap();
        let header = Checker::new(&p, f);
        let ValueStatement::While { body, span, .. } = &f.body[0] else {
            panic!()
        };
        let ValueStatement::Let { local, .. } = body[0] else {
            panic!()
        };
        let mut iteration = header.clone();
        let new = iteration.add_loan(
            Some(Place::Parameter(f.parameters[0])),
            BorrowKind::Shared,
            *span,
        );
        iteration.handles.insert(Place::Local(local), new);
        // A branch/body scoped handle cannot escape even with stale suffix counts.
        iteration.remaining.insert(Place::Local(local), 1);
        iteration.expire();
        assert!(iteration.loans.contains_key(&new));
        iteration.finish_loop_provenance(&header, *span).unwrap();
        assert!(!iteration.handles.contains_key(&Place::Local(local)));
        assert!(!iteration.loans.contains_key(&new));
        assert!(iteration.same_loop_state(&header));
    }
    #[test]
    fn post_loop_loan_reuse_cannot_alias_transient_provenance_or_header_loan() {
        let p = typed("fn f(flag: bool, value: Percent) -> Percent { let mut first = value; let mut second = value; let outside = &first; while flag { let view = &second; let before = *view; } let access = &mut second; *access = value; return *outside; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..3], f.id).unwrap();
        let header = c.clone();
        c.statements(&f.body[3..4], f.id).unwrap();
        assert!(c.same_loop_state(&header));
        let reused = LoanId(header.next_loan);
        assert!(!c.loans.contains_key(&reused));
        c.statements(&f.body[4..5], f.id).unwrap();
        let ValueStatement::Let { local: access, .. } = f.body[4] else {
            panic!()
        };
        let ValueStatement::Let { local: second, .. } = f.body[1] else {
            panic!()
        };
        assert_eq!(c.handles[&Place::Local(access)], reused);
        assert_eq!(c.loans[&reused].owner, Some(Place::Local(second)));
        assert_eq!(c.loans[&reused].kind, BorrowKind::Mutable);
        assert_eq!(c.loans.len(), 2);
        for (place, id) in &header.handles {
            assert_eq!(c.handles[place], *id);
        }
        c.statements(&f.body[5..], f.id).unwrap();
        assert!(c.loans.is_empty());
    }
    #[test]
    fn nested_loops_preserve_outer_provenance_and_restore_relative_boundary() {
        let p = typed("fn f(flag: bool, value: Percent) -> Percent { while flag { let outer = &value; while flag { let inner = &value; let observed = *inner; } let later = *outer; } return value; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[0] else {
            panic!()
        };
        let mut c = Checker::new(&p, f);
        let header = c.clone();
        c.loop_scope = Some(LoopScope::new(&header, false));
        c.statements(&body[..1], f.id).unwrap();
        let ValueStatement::Let { local, .. } = body[0] else {
            panic!()
        };
        let outer = Place::Local(local);
        let provenance = c.handles[&outer];
        let before_inner = c.clone();
        c.statements(&body[1..2], f.id).unwrap();
        assert!(c.same_loop_state(&before_inner));
        assert!(c.loop_handle(outer, *span).is_ok()); // Restored outer boundary.
        let mut inner = c.clone();
        inner.loop_scope = Some(LoopScope::new(&c, false));
        assert!(inner.loop_handle(outer, *span).is_ok());
        assert!(inner
            .loop_scope
            .as_ref()
            .unwrap()
            .header_handles
            .contains(&outer));
        assert!(c.loans.contains_key(&provenance));
        c.statements(&body[2..], f.id).unwrap();
        c.finish_loop_provenance(&header, *span).unwrap();
        assert!(c.same_loop_state(&header));
    }
    #[test]
    fn untouched_exclusive_header_loan_remains_semantically_identical() {
        let p = typed("fn f(flag: bool, value: Percent) -> Percent { let mut first = value; let mut second = value; let outside = &mut first; while flag { let inside = &mut second; *inside = value; } return *outside; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..3], f.id).unwrap();
        let header = c.clone();
        c.statements(&f.body[3..4], f.id).unwrap();
        assert!(c.same_loop_state(&header));
        assert_eq!(c.loans.len(), 1);
        assert_eq!(c.loans.values().next().unwrap().kind, BorrowKind::Mutable);
    }
    #[test]
    fn iteration_borrowing_uses_identity_independent_of_display_metadata() {
        fn rename(p: &mut Program) {
            for r in &mut p.ranges {
                r.name = "display".into();
            }
            for r in &mut p.records {
                r.name = "display".into();
            }
            for f in &mut p.fields {
                f.name = "display".into();
            }
            for v in &mut p.parameters {
                v.name = "display".into();
            }
            for v in &mut p.locals {
                v.name = "display".into();
            }
            for f in &mut p.functions {
                if let crate::hir::Function::Ordinary(f) = f {
                    f.name = "display".into();
                }
            }
        }
        for body in [
            "fn f(flag: bool, value: Percent) -> Percent { let mut current = value; while flag { let view = &current; if flag { let alias = view; let before = *alias; } else { let access = &mut current; *access = value; } } return current; }",
            "fn f(flag: bool, value: Percent) -> Percent { while flag { let outer = &value; while flag { let inner = &value; let before = *inner; } let after = *outer; } return value; }",
        ] {
            let mut p = typed(body);
            check(&p).unwrap();
            let before = crate::mir::lower(&p);
            rename(&mut p);
            check(&p).unwrap();
            assert_eq!(before, crate::mir::lower(&p));
        }
        for body in [
            "fn f(flag: bool, access: &mut Percent, value: Percent) -> Percent { while flag { let moved = access; } return value; }",
            "fn f(flag: bool, value: Percent) -> Percent { let mut owned = value; while flag { let access = &mut owned; while flag { let moved = access; } } return value; }",
        ] {
            let mut p = typed(body);
            let before = check(&p).unwrap_err();
            rename(&mut p);
            let after = check(&p).unwrap_err();
            assert_eq!(before[0].span, after[0].span);
            assert_eq!(before[0].required, after[0].required);
            assert_eq!(before[0].known, after[0].known);
            assert!(after[0].message.contains("loop-carried mutable reference `display`"));
        }
    }
    fn assert_atomic(actual: &Checker<'_>, before: &Checker<'_>) {
        assert_eq!(actual.states, before.states);
        assert_eq!(actual.handles, before.handles);
        assert_eq!(actual.remaining, before.remaining);
        assert_eq!(actual.next_loan, before.next_loan);
        assert_eq!(actual.terminal_return, before.terminal_return);
        assert_eq!(actual.recurrent, before.recurrent);
        assert_eq!(actual.targets.len(), before.targets.len());
        for (actual, before) in actual.targets.iter().zip(&before.targets) {
            assert_eq!(actual.outside, before.outside);
            assert!(Rc::ptr_eq(&actual.header, &before.header));
            assert!(Rc::ptr_eq(&actual.exit, &before.exit));
        }
        let loans = |c: &Checker<'_>| {
            c.loans
                .iter()
                .map(|(id, l)| (*id, l.owner, l.kind, l.span, l.held))
                .collect::<Vec<_>>()
        };
        assert_eq!(loans(actual), loans(before));
        let scope = |c: &Checker<'_>| {
            c.loop_scope.as_ref().map(|s| {
                (
                    s.header_places.clone(),
                    s.header_handles.clone(),
                    s.header_loans.clone(),
                    s.condition,
                )
            })
        };
        assert_eq!(scope(actual), scope(before));
    }
    fn pin_body<'a>(c: &mut Checker<'a>, body: &[ValueStatement]) -> Checker<'a> {
        let finite = c.remaining.clone();
        let frame = reference_uses(body)
            .iter()
            .filter_map(|p| c.handles.get(p).copied())
            .collect();
        c.recurrent.push(frame);
        assert_eq!(finite, c.remaining); // Obligations never encode or repair counts.
        c.expire();
        let header = c.clone();
        c.loop_scope = Some(LoopScope::new(&header, false));
        header
    }
    #[test]
    fn recurrent_shared_identity_survives_reads_aliases_and_cleanup() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let first = *view; let alias = view; let second = *alias; let last = *view; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = pin_body(&mut c, body);
        let id = *header.loans.keys().next().unwrap();
        for statement in body {
            c.statements(std::slice::from_ref(statement), f.id).unwrap();
            assert_eq!(c.loans.len(), 1);
            assert!(c.loans.contains_key(&id));
            assert_eq!(c.loans[&id].owner, header.loans[&id].owner);
        }
        assert!(c.handles.len() > header.handles.len());
        c.validate_loop_backedge(&header, *span).unwrap();
        assert!(c.same_loop_state(&header));
        assert_eq!(c.handles, header.handles); // Alias removed, original still live.
        assert!(c.loans.contains_key(&id)); // Checked before obligation discharge.
        c.recurrent.pop();
        c.expire();
        assert!(c.loans.is_empty());
    }
    #[test]
    fn recurrent_mutable_reads_and_copy_writes_preserve_exact_provenance() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; let access = &mut owned; while flag { let first = *access; *access = *access; *access = true; let second = *access; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = pin_body(&mut c, body);
        for statement in body {
            c.statements(std::slice::from_ref(statement), f.id).unwrap();
            assert_eq!(c.handles, header.handles);
            assert_eq!(c.states, header.states);
            assert_eq!(c.loans.len(), 1);
            c.assert_no_holds();
        }
        c.validate_loop_backedge(&header, *span).unwrap();
        assert!(c.same_loop_state(&header));
    }
    #[test]
    fn skipped_branch_and_zero_finite_counts_cannot_expire_recurrent_loan() {
        let p = typed("fn f(flag: bool, choice: bool) -> bool { let owned = false; let view = &owned; while flag { if choice { let seen = *view; } } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = pin_body(&mut c, body);
        let ValueStatement::If {
            condition,
            then_body,
            else_body,
            ..
        } = &body[0]
        else {
            panic!()
        };
        c.expression(condition, Transfer::Return(f.id)).unwrap();
        let (mut yes, mut no, suffix) = c.split(&future_uses(then_body), &future_uses(else_body));
        no.remaining.clear();
        no.expire();
        assert_eq!(no.loans.len(), 1); // Skipped use cannot open a protection gap.
        yes.statements(then_body, f.id).unwrap();
        assert_eq!(yes.loans.len(), 1);
        c.join(yes, no, suffix);
        assert_eq!(c.loans.len(), 1);
        c.validate_loop_backedge(&header, *span).unwrap();
    }
    #[test]
    fn false_exit_uses_suffix_not_backedge_liveness() {
        for later in [false, true] {
            let source = if later {
                "fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; } return *view; }"
            } else {
                "fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; } return owned; }"
            };
            let p = typed(source);
            let f = p.functions()[0].as_ordinary().unwrap();
            let mut c = Checker::new(&p, f);
            c.statements(&f.body[..2], f.id).unwrap();
            let id = *c.loans.keys().next().unwrap();
            c.statements(&f.body[2..3], f.id).unwrap();
            assert!(c.recurrent.is_empty());
            assert_eq!(c.loans.contains_key(&id), later);
            c.statements(&f.body[3..], f.id).unwrap();
            assert!(c.loans.is_empty());
        }
    }
    #[test]
    fn inner_exit_keeps_enclosing_obligation_even_without_finite_uses() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { while flag { let seen = *view; } } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = pin_body(&mut c, body);
        c.statements(body, f.id).unwrap();
        assert_eq!(c.recurrent.len(), 1);
        assert_eq!(c.recurrent, header.recurrent);
        assert_eq!(c.loans.len(), 1);
        for place in header.handles.keys() {
            assert_eq!(c.remaining[place], 0);
        }
        c.validate_loop_backedge(&header, *span).unwrap();
        c.recurrent.pop();
        c.expire();
        assert!(c.loans.is_empty());
    }
    #[test]
    fn outer_iteration_local_provenance_carries_inner_then_dies_before_outer_backedge() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; while flag { let view = &owned; while flag { let seen = *view; } let after = *view; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[1] else {
            panic!()
        };
        let header = pin_body(&mut c, body);
        assert!(header.recurrent[0].is_empty());
        c.statements(&body[..1], f.id).unwrap();
        let id = *c.loans.keys().next().unwrap();
        let inner_header = c.clone();
        c.statements(&body[1..2], f.id).unwrap();
        assert!(c.same_loop_state(&inner_header));
        assert!(c.loans.contains_key(&id));
        c.statements(&body[2..], f.id).unwrap();
        assert!(c.loans.is_empty());
        c.validate_loop_backedge(&header, *span).unwrap();
        assert_eq!(c.next_loan, header.next_loan);
        assert!(c.handles.is_empty());
    }
    #[test]
    fn expired_carried_provenance_cannot_be_resurrected_at_loop_entry() {
        let p = typed("fn f(flag: bool, view: &bool) -> bool { while flag { let seen = *view; } return flag; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.loans.clear();
        let before = c.clone();
        let ValueStatement::While {
            condition,
            body,
            span,
        } = &f.body[0]
        else {
            panic!()
        };
        assert!(c
            .while_loop(condition, body, *span, f.id)
            .unwrap_err()
            .message
            .contains("no longer active"));
        assert_atomic(&c, &before);
        let place = Place::Parameter(f.parameters[1]);
        assert!(c.borrowed_access(place, *span, false).is_err());
        assert_atomic(&c, &before);
    }
    #[test]
    fn backedge_validation_rejects_identity_owner_kind_and_obligation_drift() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = pin_body(&mut c, body);
        c.statements(body, f.id).unwrap();
        let id = *header.loans.keys().next().unwrap();
        let mut missing = c.clone();
        missing.loans.remove(&id);
        assert!(missing.validate_loop_backedge(&header, *span).is_err());
        let mut changed_owner = c.clone();
        changed_owner.loans.get_mut(&id).unwrap().owner = None;
        assert!(changed_owner
            .validate_loop_backedge(&header, *span)
            .is_err());
        let mut changed_kind = c.clone();
        changed_kind.loans.get_mut(&id).unwrap().kind = BorrowKind::Mutable;
        assert!(changed_kind.validate_loop_backedge(&header, *span).is_err());
        let mut released = c.clone();
        released.recurrent.pop();
        released.expire();
        assert!(released.loans.is_empty());
        assert!(released.validate_loop_backedge(&header, *span).is_err());
        let mut refreshed = c.clone();
        let loan = refreshed.add_loan(header.loans[&id].owner, BorrowKind::Shared, *span);
        *refreshed.handles.values_mut().next().unwrap() = loan;
        let allocation = refreshed.next_loan;
        assert!(refreshed
            .validate_loop_backedge(&header, *span)
            .unwrap_err()
            .message
            .contains("survives while backedge"));
        assert_eq!(refreshed.next_loan, allocation); // Normalization cannot hide refresh.
    }
    #[test]
    fn failed_carried_transfer_and_write_rhs_are_fully_atomic() {
        let p = typed("fn sink(access: &mut bool) -> bool { return false; } fn f(flag: bool) -> bool { let mut owned = false; let access = &mut owned; while flag { *access = sink(access); } return owned; }");
        let f = p.functions()[1].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[2] else {
            panic!()
        };
        pin_body(&mut c, body);
        let before = c.clone();
        let reference = *c.handles.keys().next().unwrap();
        assert!(c
            .consume(reference, f.span, Transfer::Return(f.id))
            .unwrap_err()
            .message
            .contains("loop-carried mutable"));
        assert_atomic(&c, &before);
        assert!(c
            .statements(body, f.id)
            .unwrap_err()
            .message
            .contains("loop-carried mutable"));
        assert_atomic(&c, &before);
        c.assert_no_holds();
        let ValueStatement::DerefAssign { value, .. } = &body[0] else {
            panic!()
        };
        let id = c.handles[&reference];
        c.loans.get_mut(&id).unwrap().held = 1;
        let held_before = c.clone();
        assert!(c
            .expression(value, Transfer::WriteThrough(reference))
            .is_err());
        assert_atomic(&c, &held_before); // Failed RHS preserves its caller's existing hold.
    }
    #[test]
    fn nested_call_failure_restores_earlier_argument_counts_and_holds() {
        let p = typed("fn sink(access: &mut bool) -> bool { return false; } fn both(view: &bool, value: bool) -> bool { return value; } fn f(flag: bool) -> bool { let first = false; let view = &first; let mut second = false; let access = &mut second; while flag { let done = both(view, sink(access)); } return false; }");
        let f = p.functions()[2].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..4], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[4] else {
            panic!()
        };
        pin_body(&mut c, body);
        let before = c.clone();
        let ValueStatement::Let { initializer, .. } = &body[0] else {
            panic!()
        };
        assert!(c.expression(initializer, Transfer::Return(f.id)).is_err());
        assert_atomic(&c, &before);
        c.assert_no_holds();
    }
    #[test]
    fn failed_recurrent_owner_assignment_restores_rhs_liveness() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; let view = &owned; while flag { owned = *view; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[2] else {
            panic!()
        };
        pin_body(&mut c, body);
        let before = c.clone();
        assert!(c
            .statements(body, f.id)
            .unwrap_err()
            .message
            .contains("cannot assign"));
        assert_atomic(&c, &before);
    }
    #[test]
    fn recurrent_conflicting_borrow_rejects_without_allocator_or_hold_changes() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; let access = &mut owned; while flag { let other = &owned; *access = true; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[2] else {
            panic!()
        };
        pin_body(&mut c, body);
        let before = c.clone();
        let owner = c.loans.values().next().unwrap().owner.unwrap();
        assert!(c
            .borrow(owner, BorrowKind::Shared, f.span)
            .unwrap_err()
            .message
            .contains("exclusive borrow"));
        assert_atomic(&c, &before);
        assert!(c.statements(&body[..1], f.id).is_err());
        assert_atomic(&c, &before);
    }
    #[test]
    fn post_loop_new_loan_cannot_alias_carried_or_transient_provenance() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; let view = &owned; while flag { let seen = *view; let temporary = &owned; let second = *temporary; } let access = &mut owned; *access = true; return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let carried = *c.loans.keys().next().unwrap();
        let allocator = c.next_loan;
        c.statements(&f.body[2..3], f.id).unwrap();
        assert!(c.loans.is_empty());
        assert_eq!(c.next_loan, allocator);
        c.statements(&f.body[3..4], f.id).unwrap();
        let fresh = *c.loans.keys().next().unwrap();
        assert_ne!(carried, fresh);
        assert_eq!(fresh, LoanId(allocator));
        assert_eq!(c.loans[&fresh].kind, BorrowKind::Mutable);
        c.statements(&f.body[4..], f.id).unwrap();
        assert!(c.loans.is_empty());
    }
    #[test]
    fn recurrent_semantics_and_mir_ignore_all_diagnostic_display_names() {
        fn rename(p: &mut Program) {
            for r in &mut p.ranges {
                r.name = "display".into();
            }
            for r in &mut p.records {
                r.name = "display".into();
            }
            for field in &mut p.fields {
                field.name = "display".into();
            }
            for parameter in &mut p.parameters {
                parameter.name = "display".into();
            }
            for local in &mut p.locals {
                local.name = "display".into();
            }
            for f in &mut p.functions {
                if let crate::hir::Function::Ordinary(f) = f {
                    f.name = "display".into();
                }
            }
        }
        let mut p = typed("fn f(flag: bool, value: Percent) -> Percent { let view = &value; while flag { let alias = view; while flag { let seen = *alias; } } return value; }");
        check(&p).unwrap();
        let before = crate::mir::lower(&p);
        rename(&mut p);
        check(&p).unwrap();
        assert_eq!(before, crate::mir::lower(&p));
        let mut p = typed("fn f(flag: bool) -> bool { let mut owned = false; let view = &owned; while flag { let seen = *view; owned = true; } return owned; }");
        let before = check(&p).unwrap_err();
        rename(&mut p);
        let after = check(&p).unwrap_err();
        assert_eq!(before[0].span, after[0].span);
        assert_eq!(before[0].required, after[0].required);
        assert_eq!(before[0].known, after[0].known);
        assert!(after[0].message.contains("display"));
    }

    fn install_continue_target<'a>(
        c: &mut Checker<'a>,
        body: &[ValueStatement],
        outside: FutureUses,
    ) -> Checker<'a> {
        let header = pin_body(c, body);
        let exit = Rc::new(header.false_exit(&outside, header.loop_scope.clone()));
        c.targets.push(LoopTarget {
            header: Rc::new(header.clone()),
            exit,
            outside,
        });
        header
    }
    #[test]
    fn continue_preserves_recurrent_only_shared_and_mutable_provenance() {
        for source in [
            "fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; continue; } return owned; }",
            "fn f(flag: bool) -> bool { let mut owned = false; let access = &mut owned; while flag { *access = true; continue; } return owned; }",
        ] {
            let p = typed(source); let f = p.functions()[0].as_ordinary().unwrap();
            let mut c = Checker::new(&p, f); c.statements(&f.body[..2], f.id).unwrap();
            let ValueStatement::While { body, span, .. } = &f.body[2] else { panic!() };
            let header = install_continue_target(&mut c, body, FutureUses::new());
            let id = *c.loans.keys().next().unwrap();
            c.remaining.clear(); c.expire();
            assert!(c.loans.contains_key(&id)); assert_eq!(c.loans[&id].held, 0);
            assert_eq!(c.continue_edge(*span).unwrap(), Flow::Continue);
            assert!(c.same_loop_state(&header));
            assert_eq!(c.loans[&id].owner, header.loans[&id].owner);
            assert_eq!(c.loans[&id].kind, header.loans[&id].kind);
            assert_eq!(c.recurrent, header.recurrent); // Continue never discharges a frame.
            assert_eq!(c.targets.len(), 1);
        }
    }
    #[test]
    fn continue_validation_cleanup_failure_restores_every_state_field() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let alias = view; let seen = *alias; continue; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        c.statements_with(&body[..2], f.id, &FutureUses::new())
            .unwrap();
        let carried = *header.loans.keys().next().unwrap();
        for fault in 0..5 {
            let mut broken = c.clone();
            match fault {
                0 => broken.loans.get_mut(&carried).unwrap().owner = None,
                1 => broken.loans.get_mut(&carried).unwrap().kind = BorrowKind::Mutable,
                2 => {
                    broken.loans.remove(&carried);
                }
                3 => {
                    broken.recurrent.clear();
                }
                _ => {
                    *broken.handles.values_mut().next().unwrap() = LoanId(999);
                }
            }
            let before = broken.clone();
            let e = broken.continue_edge(*span).unwrap_err();
            assert!(e.message.contains("continue backedge rejected"));
            assert_atomic(&broken, &before); // Alias cleanup must also roll back.
            assert!(broken.handles.len() > header.handles.len());
        }
    }
    #[test]
    fn continue_allocator_cannot_hide_surviving_fresh_loan_or_handle() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; continue; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        for surviving_handle in [false, true] {
            let mut broken = c.clone();
            let fresh = broken.add_loan(None, BorrowKind::Shared, *span);
            if surviving_handle {
                let place = *header.handles.keys().next().unwrap();
                broken.handles.insert(place, fresh);
                broken.remaining.insert(place, 1);
            } else {
                broken.loans.get_mut(&fresh).unwrap().held = 1;
            }
            let before = broken.clone();
            assert!(broken
                .continue_edge(*span)
                .unwrap_err()
                .message
                .contains("iteration-created reference state"));
            assert_atomic(&broken, &before);
            assert!(broken.next_loan > header.next_loan);
            assert!(broken.loans.contains_key(&fresh));
        }
    }
    #[test]
    fn continue_rejects_injected_carried_availability_change_atomically() {
        let p = typed(
            "fn f(flag: bool, ticket: Ticket) -> bool { while flag { continue; } return flag; }",
        );
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, span, .. } = &f.body[0] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        c.states.insert(
            Place::Parameter(f.parameters[1]),
            State::Moved {
                span: *span,
                transfer: Transfer::Return(f.id),
            },
        );
        let before = c.clone();
        assert!(c
            .continue_edge(*span)
            .unwrap_err()
            .message
            .contains("does not preserve"));
        assert_atomic(&c, &before);
    }
    #[test]
    fn continue_cleans_transient_local_provenance_before_allocator_reset() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; while flag { let access = &mut owned; *access = true; continue; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[1] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        assert_eq!(
            c.statements_with(body, f.id, &FutureUses::new()).unwrap(),
            Flow::Continue
        );
        assert!(c.same_loop_state(&header));
        assert!(c.handles.is_empty());
        assert!(c.loans.is_empty());
        assert_eq!(c.next_loan, header.next_loan);
    }
    #[test]
    fn continue_edge_future_is_known_before_conflicting_owner_operation() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; while flag { let view = &owned; if flag { owned = true; continue; } let seen = *view; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[1] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let ValueStatement::If { then_body, .. } = &body[1] else {
            panic!()
        };
        let after = future_uses(&body[2..]);
        let before = block_future(then_body, &after, &FutureUses::new());
        let ValueStatement::Let { local, .. } = &body[0] else {
            panic!()
        };
        assert_eq!(before.get(&Place::Local(*local)).copied().unwrap_or(0), 0);
        assert!(after[&Place::Local(*local)] > 0);
        assert_eq!(
            c.statements_with(body, f.id, &FutureUses::new()).unwrap(),
            Flow::Fallthrough
        );
        assert!(c.loans.is_empty());
    }
    #[test]
    fn all_continue_false_exit_is_independent_and_retains_post_loop_suffix_only() {
        for post_use in [false, true] {
            let suffix = if post_use {
                "return *view;"
            } else {
                "return owned;"
            };
            let p = typed(&format!("fn f(flag: bool) -> bool {{ let owned = false; let view = &owned; while flag {{ if flag {{ let seen = *view; continue; }} else {{ continue; }} }} {suffix} }}"));
            let f = p.functions()[0].as_ordinary().unwrap();
            let mut c = Checker::new(&p, f);
            c.statements(&f.body[..2], f.id).unwrap();
            let id = *c.loans.keys().next().unwrap();
            c.statements(&f.body[2..3], f.id).unwrap();
            assert!(c.targets.is_empty());
            assert!(c.recurrent.is_empty());
            assert!(c.loop_scope.is_none());
            assert_eq!(c.loans.contains_key(&id), post_use);
            c.statements(&f.body[3..], f.id).unwrap();
            assert!(c.loans.is_empty());
        }
    }
    #[test]
    fn nested_all_continue_false_exit_preserves_outer_frame_and_target() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { while flag { if flag { let seen = *view; continue; } else { continue; } } continue; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let outer = install_continue_target(&mut c, body, FutureUses::new());
        c.statements_with(&body[..1], f.id, &FutureUses::new())
            .unwrap();
        assert_eq!(c.recurrent, outer.recurrent);
        assert_eq!(c.targets.len(), 1);
        assert!(c.same_loop_state(&outer));
        c.remaining.clear();
        c.expire();
        assert_eq!(c.loans.len(), 1);
        assert_eq!(c.continue_edge(*span).unwrap(), Flow::Continue);
        assert!(c.same_loop_state(&outer));
    }
    #[test]
    fn failed_nested_continue_operation_restores_parent_target_and_summary() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; let view = &owned; while flag { while flag { owned = true; let seen = *view; continue; } continue; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[2] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let before = c.clone();
        assert!(c
            .statements_with(body, f.id, &FutureUses::new())
            .unwrap_err()
            .message
            .contains("cannot assign"));
        assert_atomic(&c, &before);
    }
    #[test]
    fn continue_metadata_renaming_preserves_identity_cfg_and_ownership() {
        let mut p = typed("fn f(flag: bool) -> bool { let mut owned = false; let view = &owned; while flag { if flag { let alias = view; let seen = *alias; continue; } else { continue; } } return owned; }");
        check(&p).unwrap();
        let before = crate::mir::lower(&p);
        for local in &mut p.locals {
            local.name = "renamed".into();
        }
        for parameter in &mut p.parameters {
            parameter.name = "renamed".into();
        }
        check(&p).unwrap();
        assert_eq!(before, crate::mir::lower(&p));
    }

    #[test]
    fn outer_local_inner_continue_retains_original_id_then_outer_cleanup_reclaims_it() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; while flag { let view = &owned; while flag { let seen = *view; continue; } continue; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[1] else {
            panic!()
        };
        let outer = install_continue_target(&mut c, body, FutureUses::new());
        let uses = future_uses(&body[1..]);
        c.statements_with(&body[..1], f.id, &uses).unwrap();
        let id = *c.loans.keys().next().unwrap();
        let ValueStatement::While {
            body: inner_body,
            span: inner_span,
            ..
        } = &body[1]
        else {
            panic!()
        };
        let inner = install_continue_target(&mut c, inner_body, FutureUses::new());
        c.remaining.clear();
        c.expire();
        assert_eq!(c.recurrent.len(), 2);
        assert_eq!(c.continue_edge(*inner_span).unwrap(), Flow::Continue);
        assert!(c.same_loop_state(&inner));
        assert!(c.loans.contains_key(&id));
        assert_eq!(c.targets.len(), 2);
        assert_eq!(c.recurrent.len(), 2);
        // Emulate the checked inner false edge: only its frame/context ends.
        c.recurrent.pop();
        c.targets.pop();
        c.loop_scope = Some(LoopScope::new(&outer, false));
        c.expire();
        assert!(c.loans.is_empty());
        assert_eq!(c.continue_edge(*span).unwrap(), Flow::Continue);
        assert!(c.same_loop_state(&outer));
        assert_eq!(c.next_loan, outer.next_loan);
        assert!(c.handles.is_empty());
    }

    // Inspect the actual normalized edge before while_loop consumes it. Public
    // acceptance alone could otherwise conceal an unvalidated break candidate.
    #[test]
    fn break_recurrent_only_candidate_reduces_to_independent_false_exit() {
        for handle in ["let view = &owned;", "let view = &mut owned;"] {
            let p = typed(&format!("fn f(flag: bool) -> bool {{ let mut owned = false; {handle} while flag {{ let seen = *view; break; }} return owned; }}"));
            let f = p.functions()[0].as_ordinary().unwrap();
            let mut c = Checker::new(&p, f);
            c.statements(&f.body[..2], f.id).unwrap();
            let ValueStatement::While { body, span, .. } = &f.body[2] else {
                panic!()
            };
            let header = install_continue_target(&mut c, body, FutureUses::new());
            let target = c.targets.last().unwrap().clone();
            let canonical = (*target.exit).clone();
            let id = *c.loans.keys().next().unwrap();
            c.remaining.clear();
            c.expire();
            assert!(c.recurrent[0].contains(&id));
            assert!(c.loans.contains_key(&id));
            assert_eq!(c.break_edge(*span).unwrap(), Flow::Break);
            assert!(c.same_loop_state(&canonical));
            assert!(!c.same_loop_state(&header)); // Legitimate exit reduction, not a backedge.
            assert!(c.recurrent.is_empty());
            assert!(c.targets.is_empty());
            assert!(c.loop_scope.is_none());
            assert!(c.loans.is_empty());
            assert_eq!(c.handles, header.handles); // Dead spelling does not recreate a loan.
            assert_eq!(c.next_loan, header.next_loan);
            assert_atomic(&target.exit, &canonical); // Candidate never defines/replaces it.
        }
    }
    #[test]
    fn break_installs_outside_before_expiry_and_preserves_original_provenance() {
        for mutable in [false, true] {
            for recurrent in [false, true] {
                let binding = if mutable {
                    "let view = &mut owned;"
                } else {
                    "let view = &owned;"
                };
                let use_in_body = if recurrent { "let seen = *view;" } else { "" };
                let use_after = if mutable {
                    "*view = true;"
                } else {
                    "let later = *view;"
                };
                let p = typed(&format!("fn f(flag: bool) -> bool {{ let mut owned = false; {binding} while flag {{ {use_in_body} break; }} {use_after} return owned; }}"));
                let f = p.functions()[0].as_ordinary().unwrap();
                let mut c = Checker::new(&p, f);
                c.statements(&f.body[..2], f.id).unwrap();
                let ValueStatement::While { body, span, .. } = &f.body[2] else {
                    panic!()
                };
                let outside = future_uses(&f.body[3..]);
                let header = install_continue_target(&mut c, body, outside.clone());
                let target = c.targets.last().unwrap().clone();
                let id = *header.loans.keys().next().unwrap();
                // Simulate exhausted edge bookkeeping without expiring evidence.
                c.remaining.clear();
                assert_eq!(c.break_edge(*span).unwrap(), Flow::Break);
                assert_eq!(c.remaining, outside);
                assert!(c.same_loop_state(&target.exit));
                assert_eq!(c.handles, header.handles);
                assert_eq!(c.next_loan, header.next_loan);
                assert_eq!(c.loans[&id].owner, header.loans[&id].owner);
                assert_eq!(c.loans[&id].kind, header.loans[&id].kind);
                assert_eq!(c.loans[&id].held, 0);
                assert!(c.recurrent.is_empty());
                c.statements(&f.body[3..], f.id).unwrap();
                assert!(c.loans.is_empty());
            }
        }
    }
    #[test]
    fn break_gate_rejects_frame_drift_before_legitimate_pop_hides_it() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        for frame in [BTreeSet::new(), BTreeSet::from([LoanId(999)])] {
            let mut broken = c.clone();
            *broken.recurrent.last_mut().unwrap() = frame;
            let before = broken.clone();
            let target = broken.targets.last().unwrap().clone();
            let mut erased = broken.clone();
            erased.project_break_exit(&target, *span).unwrap();
            assert!(erased.same_loop_state(&target.exit)); // Cleanup would erase the defect.
            assert!(broken
                .break_edge(*span)
                .unwrap_err()
                .message
                .contains("recurrent-obligation integrity"));
            assert_atomic(&broken, &before); // Restore injected state, not the header.
        }
    }
    #[test]
    fn break_gate_rejects_carried_corruption_before_recurrent_only_expiry() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        let id = *header.loans.keys().next().unwrap();
        for fault in 0..3 {
            let mut broken = c.clone();
            match fault {
                0 => broken.loans.get_mut(&id).unwrap().owner = None,
                1 => broken.loans.get_mut(&id).unwrap().kind = BorrowKind::Mutable,
                _ => {
                    broken.loans.remove(&id);
                }
            }
            let before = broken.clone();
            let target = broken.targets.last().unwrap().clone();
            let mut erased = broken.clone();
            erased.project_break_exit(&target, *span).unwrap();
            assert!(erased.same_loop_state(&target.exit)); // All evidence legitimately expires.
            assert!(broken
                .break_edge(*span)
                .unwrap_err()
                .message
                .contains("carried-loan integrity"));
            assert_atomic(&broken, &before);
        }
    }
    #[test]
    fn break_gate_never_clears_unbalanced_carried_or_fresh_holds() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        for fresh in [false, true] {
            let mut broken = c.clone();
            let id = if fresh {
                broken.add_loan(None, BorrowKind::Shared, *span)
            } else {
                *broken.loans.keys().next().unwrap()
            };
            broken.loans.get_mut(&id).unwrap().held = 1;
            let before = broken.clone();
            assert!(broken
                .break_edge(*span)
                .unwrap_err()
                .message
                .contains("unbalanced operation holds"));
            assert_atomic(&broken, &before);
            assert_eq!(broken.loans[&id].held, 1);
        }
    }
    #[test]
    fn break_gate_rejects_fresh_or_missing_carried_handle_without_allocator_reset() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        let place = *header.handles.keys().next().unwrap();
        for fault in 0..3 {
            let mut broken = c.clone();
            match fault {
                0 => {
                    broken.handles.remove(&place);
                }
                1 => {
                    broken.handles.insert(place, LoanId(999));
                }
                _ => {
                    let id = broken.add_loan(None, BorrowKind::Shared, *span);
                    broken.handles.insert(place, id);
                }
            }
            let before = broken.clone();
            assert!(broken
                .break_edge(*span)
                .unwrap_err()
                .message
                .contains("carried reference provenance"));
            assert_atomic(&broken, &before);
        }
    }
    #[test]
    fn break_rejects_availability_drift_after_cleanup_and_rolls_back_every_field() {
        let p = typed("fn f(flag: bool, ticket: Ticket) -> bool { let owned = false; let view = &owned; while flag { let alias = view; let seen = *alias; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        c.statements_with(&body[..2], f.id, &FutureUses::new())
            .unwrap();
        let ticket = Place::Parameter(f.parameters[1]);
        c.states.insert(
            ticket,
            State::Moved {
                span: *span,
                transfer: Transfer::Return(f.id),
            },
        );
        let before = c.clone();
        let target = c.targets.last().unwrap().clone();
        c.break_integrity(&target, *span).unwrap(); // No raw whole-state header comparison.
        let mut projected = c.clone();
        projected.project_break_exit(&target, *span).unwrap();
        assert_eq!(projected.states[&ticket], before.states[&ticket]); // Never repair availability.
        assert_eq!(projected.handles, header.handles);
        assert!(projected.loans.is_empty());
        assert!(projected.recurrent.is_empty());
        assert!(projected.targets.is_empty());
        assert!(!projected.same_loop_state(&target.exit));
        assert!(c
            .break_edge(*span)
            .unwrap_err()
            .message
            .contains("canonical loop-exit"));
        assert_atomic(&c, &before); // Alias and frame cleanup is transactional too.
    }
    #[test]
    fn break_allocator_proof_precedes_reset_and_failure_is_atomic() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { let seen = *view; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        let mut broken = c.clone();
        broken.next_loan = 0;
        let before = broken.clone();
        assert!(broken
            .break_edge(*span)
            .unwrap_err()
            .message
            .contains("allocation state"));
        assert_atomic(&broken, &before); // Failed after frame pop and expiry, yet exact rollback.
        for handle in [false, true] {
            let mut broken = header.clone();
            let id = broken.add_loan(None, BorrowKind::Shared, *span);
            if handle {
                broken.loans.remove(&id);
                broken
                    .handles
                    .insert(*header.handles.keys().next().unwrap(), id);
            }
            let before = broken.clone();
            assert!(broken
                .normalize_loop_allocator(&header, *span, "break exit")
                .unwrap_err()
                .message
                .contains("survives break exit"));
            assert_atomic(&broken, &before);
        }
        let mut sibling = c.clone();
        assert_eq!(sibling.break_edge(*span).unwrap(), Flow::Break);
        assert_eq!(c.next_loan, header.next_loan);
        assert_eq!(c.targets.len(), 1);
    }
    #[test]
    fn break_cleans_real_fresh_local_provenance_before_allocator_reuse() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; while flag { let access = &mut owned; let second = access; *second = true; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[1] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        let target = c.targets.last().unwrap().clone();
        c.statements_with(&body[..3], f.id, &FutureUses::new())
            .unwrap();
        assert!(c.next_loan > header.next_loan);
        assert!(!c.handles.is_empty());
        assert_eq!(c.break_edge(*span).unwrap(), Flow::Break);
        assert!(c.same_loop_state(&target.exit));
        assert!(c.handles.is_empty());
        assert!(c.loans.is_empty());
        assert_eq!(c.next_loan, header.next_loan);
        assert!(c.states.is_empty()); // Branch-local Move handles were scoped, not repaired.
    }
    #[test]
    fn inner_break_pops_exactly_one_frame_even_when_empty_and_preserves_outer_context() {
        for inner_use in ["", "let inside = *view;"] {
            let p = typed(&format!("fn f(flag: bool) -> bool {{ let owned = false; let view = &owned; while flag {{ while flag {{ {inner_use} break; }} let seen = *view; continue; }} return owned; }}"));
            let f = p.functions()[0].as_ordinary().unwrap();
            let mut c = Checker::new(&p, f);
            c.statements(&f.body[..2], f.id).unwrap();
            let ValueStatement::While { body, span, .. } = &f.body[2] else {
                panic!()
            };
            let outer = install_continue_target(&mut c, body, FutureUses::new());
            let outer_target = c.targets.last().unwrap().clone();
            let ValueStatement::While {
                body: inner_body,
                span: inner_span,
                ..
            } = &body[0]
            else {
                panic!()
            };
            let inner = install_continue_target(&mut c, inner_body, future_uses(&body[1..]));
            let inner_target = c.targets.last().unwrap().clone();
            let id = *c.loans.keys().next().unwrap();
            c.remaining.clear();
            c.expire();
            assert_eq!(c.recurrent.len(), 2);
            assert_eq!(c.recurrent[0], BTreeSet::from([id]));
            assert_eq!(c.break_edge(*inner_span).unwrap(), Flow::Break);
            assert!(c.same_loop_state(&inner_target.exit));
            assert_eq!(c.recurrent, outer.recurrent);
            assert_eq!(c.loans[&id].owner, inner.loans[&id].owner);
            assert_eq!(c.loans[&id].kind, inner.loans[&id].kind);
            assert_eq!(c.targets.len(), 1);
            assert!(Rc::ptr_eq(&c.targets[0].header, &outer_target.header));
            assert!(Rc::ptr_eq(&c.targets[0].exit, &outer_target.exit));
            assert_eq!(c.loop_scope, Some(LoopScope::new(&outer, false)));
            assert_eq!(c.continue_edge(*span).unwrap(), Flow::Continue);
            assert!(c.same_loop_state(&outer));
        }
    }
    #[test]
    fn inner_only_recurrence_can_end_without_popping_empty_outer_frame() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; while flag { let view = &owned; while flag { let seen = *view; break; } owned = true; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[1] else {
            panic!()
        };
        let outer = install_continue_target(&mut c, body, FutureUses::new());
        c.statements_with(&body[..1], f.id, &future_uses(&body[1..]))
            .unwrap();
        let ValueStatement::While {
            body: inner_body,
            span: inner_span,
            ..
        } = &body[1]
        else {
            panic!()
        };
        install_continue_target(&mut c, inner_body, future_uses(&body[2..]));
        let id = *c.loans.keys().next().unwrap();
        assert!(c.recurrent[1].contains(&id));
        assert_eq!(c.break_edge(*inner_span).unwrap(), Flow::Break);
        assert_eq!(c.recurrent, vec![BTreeSet::new()]);
        assert!(c.loans.is_empty());
        assert_eq!(c.targets.len(), 1);
        assert!(c.handles.values().any(|saved| *saved == id));
        c.statements_with(&body[2..3], f.id, &FutureUses::new())
            .unwrap();
        assert_eq!(c.break_edge(*span).unwrap(), Flow::Break);
        assert_eq!(c.next_loan, outer.next_loan);
        assert!(c.handles.is_empty());
        assert!(c.recurrent.is_empty());
        assert!(c.targets.is_empty());
    }
    #[test]
    fn nested_break_failure_preserves_injected_frames_scope_targets_and_sibling() {
        let p = typed("fn f(flag: bool, ticket: Ticket) -> bool { let owned = false; let view = &owned; while flag { while flag { break; } let seen = *view; break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let ValueStatement::While {
            body: inner_body,
            span: inner_span,
            ..
        } = &body[0]
        else {
            panic!()
        };
        install_continue_target(&mut c, inner_body, future_uses(&body[1..]));
        for fault in 0..6 {
            let mut broken = c.clone();
            match fault {
                0 => broken.recurrent.swap(0, 1),
                1 => broken.recurrent[0].clear(),
                2 => broken.loop_scope.as_mut().unwrap().condition = true,
                3 => {
                    broken.targets[0].outside.clear();
                }
                4 => {
                    broken.states.insert(
                        Place::Parameter(f.parameters[1]),
                        State::Moved {
                            span: *inner_span,
                            transfer: Transfer::Return(f.id),
                        },
                    );
                }
                _ => broken.next_loan = 0,
            }
            // Ensure the last fault is observable even if the original outer map was empty.
            if fault == 3 {
                broken.targets[0]
                    .outside
                    .insert(Place::Parameter(f.parameters[0]), 99);
            }
            let before = broken.clone();
            assert!(broken.break_edge(*inner_span).is_err());
            assert_atomic(&broken, &before);
            let mut sibling = c.clone();
            assert_eq!(sibling.break_edge(*inner_span).unwrap(), Flow::Break);
            assert_eq!(sibling.targets.len(), 1);
            assert_eq!(sibling.break_edge(*span).unwrap(), Flow::Break);
            assert!(sibling.targets.is_empty());
            assert!(sibling.recurrent.is_empty());
        }
    }
    #[test]
    fn break_rejects_recreated_dead_carried_loan_before_exit_erases_it() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { break; } return owned; }");
        let f = p.functions()[0].as_ordinary().unwrap();
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, span, .. } = &f.body[2] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        assert!(header.loans.is_empty());
        let id = *header.handles.values().next().unwrap();
        c.loans.insert(
            id,
            Loan {
                owner: None,
                kind: BorrowKind::Shared,
                span: *span,
                held: 0,
            },
        );
        let before = c.clone();
        assert!(c
            .break_edge(*span)
            .unwrap_err()
            .message
            .contains("resurrected"));
        assert_atomic(&c, &before);
    }
    #[test]
    fn all_terminal_mixed_edges_have_no_survivor_or_fabricated_suffix_join() {
        for arms in [
            "if flag { break; } else { continue; }",
            "if flag { continue; } else { break; }",
        ] {
            let p = typed(&format!(
                "fn f(flag: bool) -> bool {{ while flag {{ {arms} }} return true; }}"
            ));
            let f = p.functions()[0].as_ordinary().unwrap();
            let mut c = Checker::new(&p, f);
            let ValueStatement::While { body, .. } = &f.body[0] else {
                panic!()
            };
            install_continue_target(&mut c, body, FutureUses::new());
            assert_eq!(
                c.statements_with(body, f.id, &FutureUses::new()).unwrap(),
                Flow::NoFallthrough
            );
            // The enclosing state is not replaced by either terminal arm.
            assert_eq!(c.recurrent.len(), 1);
            assert_eq!(c.targets.len(), 1);
        }
    }
    #[test]
    fn break_canonical_ids_ignore_all_diagnostic_display_names() {
        let rename = |p: &mut Program| {
            for range in &mut p.ranges {
                range.name = "display".into();
            }
            for record in &mut p.records {
                record.name = "display".into();
            }
            for function in &mut p.functions {
                match function {
                    crate::hir::Function::Ordinary(f) => f.name = "display".into(),
                    crate::hir::Function::Verified(f) => f.name = "display".into(),
                }
            }
            for parameter in &mut p.parameters {
                parameter.name = "display".into();
            }
            for local in &mut p.locals {
                local.name = "display".into();
            }
        };
        let mut p = typed("fn f(flag: bool) -> bool { let mut owned = false; while flag { let view = &owned; if flag { owned = true; break; } let seen = *view; } return owned; }");
        check(&p).unwrap();
        let before = crate::mir::lower(&p);
        rename(&mut p);
        check(&p).unwrap();
        assert_eq!(before, crate::mir::lower(&p));
        let mut p = typed("fn f(flag: bool) -> bool { let mut owned = false; let view = &owned; while flag { if flag { owned = true; break; } let seen = *view; } return owned; }");
        let before = check(&p).unwrap_err();
        rename(&mut p);
        let after = check(&p).unwrap_err();
        assert_eq!(before[0].span, after[0].span);
        assert_eq!(before[0].required, after[0].required);
        assert!(after[0].message.contains("display"));
    }

    // Frozen KED-018 return-entry state: summaries/expiry are static inputs,
    // and the terminal operand is the only transactional operation.
    fn return_entry<'a>(c: &Checker<'a>, value: &TypedExpr) -> Checker<'a> {
        let mut entry = c.clone();
        entry.remaining.clear();
        count_expression(value, &mut entry.remaining);
        entry.expire();
        entry
    }
    fn operand(statement: &ValueStatement) -> (&TypedExpr, Span) {
        let ValueStatement::Return { value, span } = statement else {
            panic!("return operand")
        };
        (value, *span)
    }
    const RETURN_HELPERS: &str = "fn take(ticket: Ticket) -> Ticket { return ticket; } fn inspect(view: &Ticket) -> bool { return true; } fn holding(view: &Ticket, ticket: Ticket) -> Ticket { return ticket; } fn after(seen: bool, ticket: Ticket) -> Ticket { return ticket; } fn combine(first: Ticket, view: &bool, second: Ticket) -> Ticket { return second; } fn sink(access: &mut bool) -> bool { return true; } fn fresh() -> Ticket { return fresh(); }";
    fn return_typed(body: &str) -> Program {
        typed(&format!("{RETURN_HELPERS} {body}"))
    }
    fn return_function(p: &Program) -> &ValueFunction {
        p.functions().last().unwrap().as_ordinary().unwrap()
    }

    #[test]
    fn return_finite_summary_is_operand_only_even_with_all_skipped_contexts() {
        let p = typed("fn f(flag: bool, value: bool) -> bool { return value; }");
        let f = return_function(&p);
        let (value, _) = operand(&f.body[0]);
        let outside = BTreeMap::from([(Place::Parameter(f.parameters[0]), 9)]);
        let expected = BTreeMap::from([(Place::Parameter(f.parameters[1]), 1)]);
        assert_eq!(statement_future(&f.body[0], &outside, &outside), expected);
        assert_eq!(block_future(&f.body, &outside, &outside), expected);
        let mut c = Checker::new(&p, f);
        c.remaining = outside;
        let entry = return_entry(&c, value);
        assert_eq!(entry.remaining, expected);
    }
    #[test]
    fn return_entry_expiry_is_not_rolled_back_to_old_outside_summary() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { let view = &ticket; while flag { return combine(ticket, &flag, ticket); } return after(inspect(view), ticket); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[1] else {
            panic!()
        };
        let outside = future_uses(&f.body[2..]);
        install_continue_target(&mut c, body, outside.clone());
        assert_eq!(c.loans.len(), 1);
        let (value, span) = operand(&body[0]);
        let entry = return_entry(&c, value);
        assert!(entry.loans.is_empty());
        assert!(!entry
            .remaining
            .contains_key(&Place::Local(p.locals()[0].id)));
        assert!(c
            .return_edge(value, f.id, span)
            .unwrap_err()
            .message
            .contains("moved value"));
        assert_atomic(&c, &entry);
        // The statement-list wrapper must preserve exactly that same boundary.
        let mut direct = entry.clone();
        direct.remaining = outside.clone();
        assert!(direct.statements_with(body, f.id, &outside).is_err());
        assert_atomic(&direct, &entry);
        assert!(!c.terminal_return);
    }
    #[test]
    fn terminal_permission_is_not_the_return_transfer_tag_or_while_condition_tag() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { return take(take(ticket)); } return fresh(); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let (value, span) = operand(&body[0]);
        c = return_entry(&c, value);
        let entry = c.clone();
        assert!(c
            .expression(value, Transfer::Return(f.id))
            .unwrap_err()
            .message
            .contains("pre-existing non-Copy"));
        assert_atomic(&c, &entry);
        assert_eq!(c.return_edge(value, f.id, span).unwrap(), Flow::Return);
        assert!(matches!(
            c.states[&Place::Parameter(f.parameters[1])],
            State::Moved { .. }
        ));
        assert!(!c.terminal_return);
        assert_eq!(c.targets.len(), 1);
        assert_eq!(c.recurrent.len(), 1);
    }
    #[test]
    fn return_hold_conflict_restores_created_provenance_and_permission() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { return holding(&ticket, ticket); } return fresh(); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let (value, span) = operand(&body[0]);
        let entry = return_entry(&c, value);
        assert!(c
            .return_edge(value, f.id, span)
            .unwrap_err()
            .message
            .contains("shared-borrowed"));
        assert_atomic(&c, &entry);
        assert_eq!(c.next_loan, 0);
        assert!(!c.terminal_return);
        // A failed operand cannot grant a later ordinary expression permission.
        let ticket = TypedExpr {
            kind: ExprKind::Parameter(f.parameters[1]),
            ty: value.ty,
            span,
        };
        assert!(c
            .expression(&ticket, Transfer::Return(f.id))
            .unwrap_err()
            .message
            .contains("pre-existing non-Copy"));
    }
    #[test]
    fn late_nested_return_argument_rolls_back_earlier_move_borrow_counts_and_holds() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { return combine(take(ticket), &flag, ticket); } return fresh(); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let (value, span) = operand(&body[0]);
        let entry = return_entry(&c, value);
        assert!(c
            .return_edge(value, f.id, span)
            .unwrap_err()
            .message
            .contains("moved value"));
        assert_atomic(&c, &entry);
        assert_eq!(
            c.states[&Place::Parameter(f.parameters[1])],
            State::Available
        );
        assert!(c.loans.is_empty());
    }
    #[test]
    fn fallible_final_return_validation_restores_successful_operand_effects() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { return take(ticket); } return fresh(); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let (value, span) = operand(&body[0]);
        let held = c.add_loan(None, BorrowKind::Shared, span);
        c.loans.get_mut(&held).unwrap().held = 1; // Invalid escaped operation hold.
        let entry = return_entry(&c, value);
        let mut successful_operand = entry.clone();
        successful_operand.terminal_return = true;
        successful_operand
            .expression(value, Transfer::Return(f.id))
            .unwrap();
        assert!(matches!(
            successful_operand.states[&Place::Parameter(f.parameters[1])],
            State::Moved { .. }
        ));
        assert!(c
            .return_edge(value, f.id, span)
            .unwrap_err()
            .message
            .contains("operation holds"));
        assert_atomic(&c, &entry);
        assert_eq!(c.loans[&held].held, 1); // Rollback, never hold clearing/repair.
    }
    #[test]
    fn return_success_preserves_original_recurrence_and_never_normalizes_local_successor() {
        let p = typed("fn f(flag: bool) -> bool { let mut owned = false; let view = &owned; while flag { let temporary = &flag; return *view; } return owned; }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[2] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, future_uses(&f.body[3..]));
        let carried = *header.loans.keys().next().unwrap();
        let mut after = FutureUses::new();
        count_expression(operand(&body[1]).0, &mut after);
        c.statements_with(&body[..1], f.id, &after).unwrap();
        let next = c.next_loan;
        assert!(next > header.next_loan);
        let frames = c.recurrent.clone();
        let targets = c.targets.clone();
        let handles = c.handles.clone();
        let (value, span) = operand(&body[1]);
        assert_eq!(c.return_edge(value, f.id, span).unwrap(), Flow::Return);
        assert_eq!(c.next_loan, next); // No header allocator reset.
        assert_eq!(c.handles, handles); // No lexical successor cleanup.
        assert_eq!(c.recurrent, frames); // No frame discharge.
        assert_eq!(c.loans[&carried].owner, header.loans[&carried].owner);
        assert_eq!(c.loans[&carried].kind, header.loans[&carried].kind);
        assert_eq!(c.remaining[&Place::Local(p.locals()[1].id)], 0);
        assert!(Rc::ptr_eq(&c.targets[0].header, &targets[0].header));
        assert!(Rc::ptr_eq(&c.targets[0].exit, &targets[0].exit));
        assert!(c.loans.values().all(|l| l.held == 0));
        assert!(!c.terminal_return);
    }
    #[test]
    fn return_only_reference_use_discovers_original_recurrent_loan() {
        let p = typed("fn f(flag: bool) -> bool { let owned = false; let view = &owned; while flag { return *view; } return owned; }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[2] else {
            panic!()
        };
        let carried = c.handles[&Place::Local(p.locals()[1].id)];
        install_continue_target(&mut c, body, FutureUses::new());
        assert_eq!(c.recurrent.last().unwrap(), &BTreeSet::from([carried]));
        let (value, span) = operand(&body[0]);
        c.return_edge(value, f.id, span).unwrap();
        assert!(c.loans.contains_key(&carried));
        assert!(!c.terminal_return);
    }
    #[test]
    fn return_nested_call_does_not_expire_recurrence_after_last_finite_reference_use() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { let view = &ticket; while flag { return after(inspect(view), ticket); } return fresh(); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[1] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, FutureUses::new());
        let id = *header.loans.keys().next().unwrap();
        let (value, span) = operand(&body[0]);
        let entry = return_entry(&c, value);
        assert!(c
            .return_edge(value, f.id, span)
            .unwrap_err()
            .message
            .contains("shared-borrowed"));
        assert_atomic(&c, &entry);
        assert_eq!(c.recurrent[0], BTreeSet::from([id]));
        assert_eq!(c.loans[&id].owner, header.loans[&id].owner);
    }
    #[test]
    fn return_empty_inner_frame_cannot_discharge_enclosing_recurrence() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { let view = &ticket; while flag { while flag { return ticket; } let seen = inspect(view); } return fresh(); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body: outer, .. } = &f.body[1] else {
            panic!()
        };
        install_continue_target(&mut c, outer, FutureUses::new());
        let ValueStatement::While { body: inner, .. } = &outer[0] else {
            panic!()
        };
        install_continue_target(&mut c, inner, FutureUses::new());
        assert_eq!(c.recurrent.len(), 2);
        assert!(!c.recurrent[0].is_empty());
        assert!(c.recurrent[1].is_empty());
        let (value, span) = operand(&inner[0]);
        let entry = return_entry(&c, value);
        assert!(c
            .return_edge(value, f.id, span)
            .unwrap_err()
            .message
            .contains("shared-borrowed"));
        assert_atomic(&c, &entry);
    }
    #[test]
    fn return_permission_never_authorizes_carried_mutable_handle_transfer() {
        let p = return_typed("fn f(flag: bool, access: &mut bool) -> bool { while flag { return sink(access); } return false; }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let (value, span) = operand(&body[0]);
        let entry = return_entry(&c, value);
        assert!(c
            .return_edge(value, f.id, span)
            .unwrap_err()
            .message
            .contains("loop-carried mutable reference"));
        assert_atomic(&c, &entry);
    }
    #[test]
    fn return_success_balances_nested_direct_borrow_call_holds() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { return after(inspect(&ticket), ticket); } return fresh(); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let (value, span) = operand(&body[0]);
        assert_eq!(c.return_edge(value, f.id, span).unwrap(), Flow::Return);
        assert!(c.loans.is_empty());
        assert_eq!(c.next_loan, 1);
        assert!(!c.terminal_return);
    }
    #[test]
    fn return_branch_state_does_not_enter_survivor_or_canonical_false_exit() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { if flag { let temporary = &flag; return ticket; } else { continue; } } return ticket; }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        let header = install_continue_target(&mut c, body, future_uses(&f.body[1..]));
        let before = c.clone();
        assert_eq!(
            c.statements_with(body, f.id, &FutureUses::new()).unwrap(),
            Flow::NoFallthrough
        );
        // Both alternatives terminate locally; neither becomes a merged state.
        assert_eq!(c.states, before.states);
        assert_eq!(c.next_loan, before.next_loan);
        assert!(!c.terminal_return);
        let exit = &c.targets[0].exit;
        assert_eq!(
            exit.states[&Place::Parameter(f.parameters[1])],
            State::Available
        );
        assert_eq!(exit.next_loan, header.next_loan);
        assert!(!exit.terminal_return);
    }
    #[test]
    fn return_survivor_keeps_actual_move_effect_without_repair() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { if flag { return fresh(); } else { let moved = ticket; } return ticket; }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let after = future_uses(&f.body[1..]);
        assert_eq!(
            c.statements_with(&f.body[..1], f.id, &after).unwrap(),
            Flow::Fallthrough
        );
        assert!(matches!(
            c.states[&Place::Parameter(f.parameters[1])],
            State::Moved { .. }
        ));
        let (value, span) = operand(&f.body[1]);
        let entry = return_entry(&c, value);
        assert!(c
            .return_edge(value, f.id, span)
            .unwrap_err()
            .message
            .contains("moved value"));
        assert_atomic(&c, &entry);
    }
    #[test]
    fn return_callee_checker_never_inherits_callers_terminal_permission() {
        let p = return_typed("fn caller(flag: bool, ticket: Ticket) -> Ticket { while flag { return invalid(flag, ticket); } return fresh(); } fn invalid(flag: bool, ticket: Ticket) -> Ticket { while flag { let moved = ticket; return moved; } return fresh(); }");
        let errors = check(&p).unwrap_err();
        assert!(errors[0].message.contains("pre-existing non-Copy"));
    }
    #[test]
    fn return_metadata_renaming_preserves_ids_cfg_recurrence_and_errors() {
        fn rename(p: &mut Program) {
            for range in &mut p.ranges {
                range.name = "display".into();
            }
            for record in &mut p.records {
                record.name = "display".into();
            }
            for f in &mut p.functions {
                if let crate::hir::Function::Ordinary(f) = f {
                    f.name = "display".into();
                }
            }
            for parameter in &mut p.parameters {
                parameter.name = "display".into();
            }
            for local in &mut p.locals {
                local.name = "display".into();
            }
        }
        let mut p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { while flag { if flag { return take(ticket); } else { continue; } } return ticket; }");
        check(&p).unwrap();
        let before = crate::mir::lower(&p);
        rename(&mut p);
        check(&p).unwrap();
        assert_eq!(before, crate::mir::lower(&p));
        let mut p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { let view = &ticket; while flag { return after(inspect(view), ticket); } return fresh(); }");
        let before = check(&p).unwrap_err();
        rename(&mut p);
        let after = check(&p).unwrap_err();
        assert_eq!(before[0].span, after[0].span);
        assert_eq!(before[0].required, after[0].required);
        assert!(after[0].message.contains("display"));
    }

    #[test]
    fn return_tagged_while_condition_has_no_terminal_permission() {
        let p = return_typed("fn predicate(ticket: Ticket) -> bool { return true; } fn f(ticket: Ticket) -> Ticket { while predicate(ticket) { return ticket; } return fresh(); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While {
            condition, body, ..
        } = &f.body[0]
        else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        c.loop_scope.as_mut().unwrap().condition = true;
        c.remaining.clear();
        count_expression(condition, &mut c.remaining);
        let before = c.clone();
        assert!(c
            .expression(condition, Transfer::Return(f.id))
            .unwrap_err()
            .message
            .contains("while conditions"));
        assert_atomic(&c, &before);
        assert!(!c.terminal_return);
    }
    #[test]
    fn returning_branch_move_allocator_and_permission_never_enter_real_fallthrough() {
        let p = return_typed("fn f(flag: bool, ticket: Ticket) -> Ticket { if flag { let temporary = &flag; return take(ticket); } let moved = ticket; return moved; }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let after = future_uses(&f.body[1..]);
        assert_eq!(
            c.statements_with(&f.body[..1], f.id, &after).unwrap(),
            Flow::Fallthrough
        );
        assert_eq!(
            c.states[&Place::Parameter(f.parameters[1])],
            State::Available
        );
        assert_eq!(c.next_loan, 0);
        assert!(c.loans.is_empty());
        assert!(c.handles.is_empty());
        assert!(!c.terminal_return);
        assert_eq!(
            c.statements_with(&f.body[1..], f.id, &FutureUses::new())
                .unwrap(),
            Flow::Return
        );
    }

    // K19-01 tests invoke the statement transaction directly. A surrounding
    // expression/statement-list/conditional rollback cannot mask a partial commit.
    const NO_VALUE_HELPERS: &str = "fn observe(view: &bool) {} fn consume(ticket: Ticket) {} fn receive(first: Ticket, held: &bool, last: Ticket) {} fn nested(first: Ticket, held: &bool, last: bool) {} fn touch(access: &mut bool) -> bool { return true; } fn read(view: &bool) -> bool { return *view; } fn shared_bool(view: &bool, last: bool) {} fn reset(access: &mut bool) {}";
    fn no_value_typed(source: &str) -> Program {
        typed(&format!("{NO_VALUE_HELPERS} {source}"))
    }
    fn statement_call(statement: &ValueStatement) -> (FunctionId, &[TypedExpr], Span) {
        let ValueStatement::CallNoValue {
            function,
            arguments,
            span,
        } = statement
        else {
            panic!("no-value call")
        };
        (*function, arguments, *span)
    }
    fn prepare_call(c: &mut Checker<'_>, arguments: &[TypedExpr]) {
        c.remaining.clear();
        for argument in arguments {
            count_expression(argument, &mut c.remaining);
        }
        c.expire();
    }
    #[test]
    fn no_value_late_argument_failure_restores_direct_transaction_entry() {
        let p =
            no_value_typed("fn f(ticket: Ticket, flag: bool) { receive(ticket, &flag, ticket); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let (callee, args, span) = statement_call(&f.body[0]);
        prepare_call(&mut c, args);
        let entry = c.clone();
        assert!(c
            .call_statement(callee, args, span)
            .unwrap_err()
            .message
            .contains("moved value"));
        assert_atomic(&c, &entry);
        assert_eq!(c.next_loan, 0);
        assert!(c.loans.is_empty());
        assert_eq!(
            c.states[&Place::Parameter(f.parameters[0])],
            State::Available
        );
    }
    #[test]
    fn no_value_nested_hold_failure_restores_prefix_move_and_all_borrow_state() {
        let p=no_value_typed("fn f(ticket: Ticket) { let mut value = false; nested(ticket, &value, touch(&mut value)); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let (callee, args, span) = statement_call(&f.body[1]);
        prepare_call(&mut c, args);
        let entry = c.clone();
        assert!(c
            .call_statement(callee, args, span)
            .unwrap_err()
            .message
            .contains("shared borrow is active"));
        assert_atomic(&c, &entry);
        assert_eq!(c.next_loan, 0);
        assert!(!c.terminal_return);
    }
    #[test]
    fn no_value_final_call_validation_rolls_back_successful_arguments_without_clearing_fault() {
        let p = no_value_typed("fn f(ticket: Ticket) { consume(ticket); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let (callee, args, span) = statement_call(&f.body[0]);
        prepare_call(&mut c, args);
        let defect = c.add_loan(None, BorrowKind::Shared, span);
        c.loans.get_mut(&defect).unwrap().held = 1;
        let entry = c.clone();
        let mut successful = c.clone();
        successful.call_arguments(callee, args).unwrap();
        assert!(matches!(
            successful.states[&Place::Parameter(f.parameters[0])],
            State::Moved { .. }
        ));
        assert!(c
            .call_statement(callee, args, span)
            .unwrap_err()
            .message
            .contains("balanced operation holds"));
        assert_atomic(&c, &entry);
        assert_eq!(c.loans[&defect].held, 1);
    }
    #[test]
    fn no_value_call_error_preserves_nested_loop_context_recurrence_and_finite_continuation() {
        let p=no_value_typed("fn f(flag: bool) { let mut owned = false; let view = &owned; while flag { while flag { shared_bool(view, touch(&mut owned)); return; } observe(view); } }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body: outer, .. } = &f.body[2] else {
            panic!()
        };
        install_continue_target(&mut c, outer, FutureUses::new());
        let ValueStatement::While { body: inner, .. } = &outer[0] else {
            panic!()
        };
        install_continue_target(&mut c, inner, FutureUses::new());
        let (callee, args, span) = statement_call(&inner[0]);
        prepare_call(&mut c, args);
        let entry = c.clone();
        assert_eq!(entry.recurrent.len(), 2);
        assert!(c.call_statement(callee, args, span).is_err());
        assert_atomic(&c, &entry);
    }
    #[test]
    fn no_value_call_never_inherits_value_return_move_permission() {
        let p = no_value_typed(
            "fn f(flag: bool, ticket: Ticket) { while flag { consume(ticket); return; } }",
        );
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let ValueStatement::While { body, .. } = &f.body[0] else {
            panic!()
        };
        install_continue_target(&mut c, body, FutureUses::new());
        let (callee, args, span) = statement_call(&body[0]);
        prepare_call(&mut c, args);
        // Even privileged inherited context cannot give a statement the exemption.
        c.terminal_return = true;
        let entry = c.clone();
        assert!(c
            .call_statement(callee, args, span)
            .unwrap_err()
            .message
            .contains("pre-existing non-Copy"));
        assert_atomic(&c, &entry);
    }
    #[test]
    fn no_value_successful_call_has_exact_move_destination_and_balanced_holds() {
        let p = no_value_typed(
            "fn f(ticket: Ticket, flag: bool, second: Ticket) { receive(ticket, &flag, second); }",
        );
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let (callee, args, span) = statement_call(&f.body[0]);
        prepare_call(&mut c, args);
        c.call_statement(callee, args, span).unwrap();
        let callee = p.function(callee).as_ordinary().unwrap();
        assert!(
            matches!(c.states[&Place::Parameter(f.parameters[0])],State::Moved{transfer:Transfer::Argument(id),..} if id==callee.parameters[0])
        );
        assert!(
            matches!(c.states[&Place::Parameter(f.parameters[2])],State::Moved{transfer:Transfer::Argument(id),..} if id==callee.parameters[2])
        );
        assert!(c.remaining.values().all(|n| *n == 0));
        assert!(c.loans.is_empty());
        assert_eq!(c.next_loan, 1);
        assert!(!c.terminal_return);
    }
    #[test]
    fn no_value_nested_calls_release_only_their_own_holds() {
        let p = no_value_typed(
            "fn f(value: bool) { let view = &value; shared_bool(view, read(view)); }",
        );
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let (callee, args, span) = statement_call(&f.body[1]);
        prepare_call(&mut c, args);
        let id = c.handles[&Place::Local(p.locals()[0].id)];
        // Inspect the receiving call's prefix independently of the shared helper.
        let loan = c
            .expression(
                &args[0],
                Transfer::Argument(p.function(callee).as_ordinary().unwrap().parameters[0]),
            )
            .unwrap()
            .unwrap();
        assert_eq!(loan, id);
        c.loans.get_mut(&id).unwrap().held += 1;
        c.expire();
        c.expression(
            &args[1],
            Transfer::Argument(p.function(callee).as_ordinary().unwrap().parameters[1]),
        )
        .unwrap();
        assert_eq!(c.loans[&id].held, 1); // Nested read must not release the receiving hold.
        c.loans.get_mut(&id).unwrap().held -= 1;
        c.expire();
        assert!(c.loans.is_empty());
        let mut full = Checker::new(&p, f);
        full.statements(&f.body[..1], f.id).unwrap();
        prepare_call(&mut full, args);
        full.call_statement(callee, args, span).unwrap();
        assert!(full.loans.is_empty());
    }
    #[test]
    fn no_value_arguments_are_in_finite_and_recurrent_traversal_including_nested_calls() {
        let p=no_value_typed("fn f(flag: bool) { let value = false; let view = &value; while flag { while flag { shared_bool(view, read(view)); } } }");
        let f = return_function(&p);
        let view = Place::Local(p.locals()[1].id);
        let ValueStatement::While { body, .. } = &f.body[2] else {
            panic!()
        };
        assert_eq!(future_uses(body)[&view], 2);
        assert!(reference_uses(body).contains(&view));
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let original = c.handles[&view];
        install_continue_target(&mut c, body, FutureUses::new());
        assert_eq!(c.recurrent[0], BTreeSet::from([original]));
    }
    #[test]
    fn no_value_bare_return_summary_excludes_every_finite_continuation() {
        let p = no_value_typed("fn f(flag: bool) { return; }");
        let f = return_function(&p);
        let outside = BTreeMap::from([(Place::Parameter(f.parameters[0]), 10)]);
        assert!(statement_future(&f.body[0], &outside, &outside).is_empty());
        assert!(block_future(&f.body, &outside, &outside).is_empty());
    }
    #[test]
    fn no_value_bare_return_failed_hold_validation_keeps_prepared_not_old_summary_state() {
        let p = no_value_typed(
            "fn f(flag: bool) { let view = &flag; while flag { return; } observe(view); }",
        );
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..1], f.id).unwrap();
        let ValueStatement::While { body, .. } = &f.body[1] else {
            panic!()
        };
        let ValueStatement::ReturnNoValue { span } = body[0] else {
            panic!()
        };
        let held = c.add_loan(None, BorrowKind::Mutable, span);
        c.loans.get_mut(&held).unwrap().held = 1;
        let old = c.clone();
        let mut entry = c.clone();
        entry.remaining.clear();
        entry.expire();
        assert!(entry.loans.len() < old.loans.len());
        assert!(c
            .bare_return(span)
            .unwrap_err()
            .message
            .contains("balanced operation holds"));
        assert_atomic(&c, &entry);
        let mut wrapper = old;
        let after = wrapper.remaining.clone();
        assert!(wrapper.statements_with(body, f.id, &after).is_err());
        assert_atomic(&wrapper, &entry);
        assert!(!c.terminal_return);
        assert_eq!(c.loans[&held].held, 1);
    }
    #[test]
    fn no_value_bare_return_preserves_original_provenance_all_frames_and_allocator() {
        let p=no_value_typed("fn f(flag: bool) { let value = false; let view = &value; while flag { while flag { return; } observe(view); } }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.statements(&f.body[..2], f.id).unwrap();
        let ValueStatement::While { body: outer, .. } = &f.body[2] else {
            panic!()
        };
        install_continue_target(&mut c, outer, FutureUses::new());
        let ValueStatement::While { body: inner, .. } = &outer[0] else {
            panic!()
        };
        install_continue_target(&mut c, inner, FutureUses::new());
        assert!(!c.recurrent[0].is_empty());
        assert!(c.recurrent[1].is_empty());
        let ValueStatement::ReturnNoValue { span } = inner[0] else {
            panic!()
        };
        let mut entry = c.clone();
        entry.remaining.clear();
        entry.expire();
        assert_eq!(c.bare_return(span).unwrap(), Flow::Return);
        assert_atomic(&c, &entry);
        assert!(!c.terminal_return);
    }
    #[test]
    fn no_value_normal_completion_rejects_unbalanced_holds_without_normalization() {
        let p = no_value_typed("fn f(ticket: Ticket) {}");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let held = c.add_loan(None, BorrowKind::Shared, f.span);
        c.loans.get_mut(&held).unwrap().held = 1;
        let entry = c.clone();
        assert!(c
            .complete_no_value(f.span)
            .unwrap_err()
            .message
            .contains("balanced operation holds"));
        assert_atomic(&c, &entry);
        // The function-body path must invoke the fallible completion check too.
        let mut expected = c.clone();
        expected.remaining.clear();
        expected.expire();
        assert!(c.body(f).is_err());
        assert_atomic(&c, &expected);
    }
    #[test]
    fn no_value_normal_completion_does_not_consume_unused_owner_or_normalize_state() {
        let p = no_value_typed("fn f(ticket: Ticket) {}");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let entry = c.clone();
        c.complete_no_value(f.span).unwrap();
        assert_atomic(&c, &entry);
        assert_eq!(
            c.states[&Place::Parameter(f.parameters[0])],
            State::Available
        );
        c.body(f).unwrap();
        assert_eq!(
            c.states[&Place::Parameter(f.parameters[0])],
            State::Available
        );
    }
    #[test]
    fn no_value_terminated_move_does_not_enter_fallthrough_sibling() {
        let p=no_value_typed("fn f(flag: bool, ticket: Ticket) { if flag { consume(ticket); return; } consume(ticket); }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        let after = future_uses(&f.body[1..]);
        assert_eq!(
            c.statements_with(&f.body[..1], f.id, &after).unwrap(),
            Flow::Fallthrough
        );
        assert_eq!(
            c.states[&Place::Parameter(f.parameters[1])],
            State::Available
        );
        assert!(!c.terminal_return);
        c.statements_with(&f.body[1..], f.id, &FutureUses::new())
            .unwrap();
        assert!(matches!(
            c.states[&Place::Parameter(f.parameters[1])],
            State::Moved { .. }
        ));
    }
    #[test]
    fn no_value_top_level_completion_keeps_conditional_move_and_local_suffix_still_checks_it() {
        let p = no_value_typed("fn f(flag: bool, ticket: Ticket) { if flag { consume(ticket); } }");
        let f = return_function(&p);
        let mut c = Checker::new(&p, f);
        c.body(f).unwrap();
        assert!(matches!(
            c.states[&Place::Parameter(f.parameters[1])],
            State::ConditionalMove { .. }
        ));
        let entry = c.clone();
        c.complete_no_value(f.span).unwrap();
        assert_atomic(&c, &entry);
    }
    #[test]
    fn no_value_call_and_bare_return_outcomes_ignore_all_display_names() {
        fn rename(p: &mut Program) {
            for range in &mut p.ranges {
                range.name = "display".into();
            }
            for record in &mut p.records {
                record.name = "display".into();
            }
            for parameter in &mut p.parameters {
                parameter.name = "display".into();
            }
            for local in &mut p.locals {
                local.name = "display".into();
            }
            for f in &mut p.functions {
                if let crate::hir::Function::Ordinary(f) = f {
                    f.name = "display".into();
                }
            }
        }
        let mut p=no_value_typed("fn f(flag: bool) { let value = false; let view = &value; while flag { observe(view); if flag { return; } } }");
        check(&p).unwrap();
        let original = crate::mir::lower(&p);
        rename(&mut p);
        check(&p).unwrap();
        assert_eq!(original, crate::mir::lower(&p));
        let mut p =
            no_value_typed("fn f(ticket: Ticket, flag: bool) { receive(ticket, &flag, ticket); }");
        let original = check(&p).unwrap_err();
        rename(&mut p);
        let renamed = check(&p).unwrap_err();
        assert_eq!(original[0].span, renamed[0].span);
        assert_eq!(original[0].required, renamed[0].required);
        assert!(renamed[0].message.contains("display"));
    }
}
