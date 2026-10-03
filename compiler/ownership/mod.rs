//! Owned moves, acyclic whole-value loans, and iteration-local loop ownership in ordinary HIR.
//! Availability and active loans are orthogonal private state; no runtime behavior.
use std::collections::{BTreeMap, BTreeSet};

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
            ValueStatement::While {
                condition, body, ..
            } => {
                // Finite suffix bookkeeping only: recurring handle/Move uses
                // are rejected during loop checking, never modeled by this count.
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
#[derive(Clone)]
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
#[derive(Clone)]
struct Checker<'a> {
    program: &'a Program,
    states: BTreeMap<Place, State>,
    loans: BTreeMap<LoanId, Loan>,
    handles: BTreeMap<Place, LoanId>,
    remaining: FutureUses,
    next_loan: usize,
    loop_scope: Option<LoopScope>,
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
        self.statements(&function.body, function.id)
    }
    fn statements(
        &mut self,
        statements: &[ValueStatement],
        function: FunctionId,
    ) -> OwnershipResult {
        for statement in statements {
            match statement {
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
                    self.conditional(condition, then_body, else_body, function)?;
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
                ValueStatement::Return { value, .. } => {
                    self.expression(value, Transfer::Return(function))?;
                }
            }
            self.expire();
        }
        Ok(())
    }
    fn loop_reference_error(&self, span: Span) -> Box<Diagnostic> {
        self.diagnostic(span, "reference activity is unsupported in while conditions".into(), "recurring condition reference activity requires future cyclic loan/lifetime analysis; iteration-local borrowing is permitted only in the body", None)
    }
    fn loop_handle(&self, reference: Place, span: Span) -> OwnershipResult {
        if let Some(scope) = &self.loop_scope {
            if scope.condition {
                return Err(self.loop_reference_error(span));
            }
            if scope.header_handles.contains(&reference)
                || self
                    .handles
                    .get(&reference)
                    .is_some_and(|id| scope.header_loans.contains(id))
            {
                return Err(self.diagnostic(span, format!("cannot use pre-existing reference handle `{}` inside while loop", self.metadata(reference).0), "using a header reference through a backedge requires future cyclic loan/lifetime analysis; only iteration-created handles/provenance may be used in this body", None));
            }
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
    }
    fn while_loop(
        &mut self,
        condition: &TypedExpr,
        body: &[ValueStatement],
        span: Span,
        function: FunctionId,
    ) -> OwnershipResult {
        self.expire();
        self.assert_no_holds();
        let header = self.clone();
        let mut iteration = self.clone();
        iteration.loop_scope = Some(LoopScope::new(&header, true));
        iteration.expression(condition, Transfer::Return(function))?;
        iteration.expire();
        iteration
            .loop_scope
            .as_mut()
            .expect("current loop")
            .condition = false;
        iteration.statements(body, function)?;
        iteration.loop_scope = self.loop_scope.clone();
        iteration.finish_loop_provenance(&header, span)?;
        iteration.assert_no_holds();
        if !iteration.same_loop_state(&header) {
            return Err(self.diagnostic(span, "while loop does not preserve ownership and loan state".into(), "the current loop subset requires equal header/backedge state; general cyclic ownership/lifetime analysis remains future work", None));
        }
        // Equality also covers the zero-iteration exit. Only the finite outside
        // continuation advances; no repeated reference counts or fixed point exist.
        self.remaining = iteration.remaining;
        Ok(())
    }
    fn finish_loop_provenance(&mut self, header: &Self, span: Span) -> OwnershipResult {
        self.scope_loop_locals(header);
        self.expire();
        // Never roll the allocator back until every new loan and every surviving
        // handle referring to newly allocated provenance is proven absent.
        if self.loans.keys().any(|id| !header.loans.contains_key(id))
            || self.handles.values().any(|id| id.0 >= header.next_loan)
        {
            return Err(self.diagnostic(span, "iteration-created reference state survives while backedge".into(), "all newly created handles and loans must be fully discharged before the current loop backedge; allocator reuse cannot hide surviving provenance", None));
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
    ) -> OwnershipResult {
        // Condition effects occur once, before either branch starts.
        self.expression(condition, Transfer::Return(function))?;
        self.expire();
        // The current suffix contains both possible branches and their common
        // continuation. Keep their union through the condition, then give each
        // edge only its own branch uses plus the continuation after the join.
        let then_uses = future_uses(then_body);
        let else_uses = future_uses(else_body);
        let (mut then_state, mut else_state, continuation) = self.split(&then_uses, &else_uses);
        then_state.statements(then_body, function)?;
        else_state.statements(else_body, function)?;
        self.join(then_state, else_state, continuation);
        Ok(())
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
                let callee = self
                    .program
                    .function(*function)
                    .as_ordinary()
                    .expect("typed ordinary calls target ordinary functions");
                let mut held = Vec::new();
                for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
                    if let Some(loan) = self.expression(argument, Transfer::Argument(*parameter))? {
                        // Hold BEFORE expiry, including a handle's final use or move.
                        self.loans
                            .get_mut(&loan)
                            .expect("live reference provenance")
                            .held += 1;
                        held.push(loan);
                    }
                    self.expire();
                }
                // Each nested call releases only its own holds. An outer receiving
                // call's earlier arguments remain held throughout nested arguments.
                for id in held {
                    self.loans.get_mut(&id).expect("call-held loan").held -= 1;
                }
                self.expire();
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
        if let Some(remaining) = self.remaining.get_mut(&reference) {
            *remaining -= 1;
        }
        Ok(*self
            .handles
            .get(&reference)
            .expect("typed reference handle has provenance"))
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
        self.loans.retain(|id, loan| {
            loan.held > 0
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
        }
        if self
            .loop_scope
            .as_ref()
            .is_some_and(|scope| scope.header_places.contains(&place))
            && matches!(self.metadata(place).1, ValueType::Record(_))
        {
            return Err(self.diagnostic(span, format!("cannot move pre-existing non-Copy value `{}` in while loop", self.metadata(place).0), "the transfer would change pre-existing ownership state across a backedge and requires future generalized cyclic ownership analysis; only iteration-local Move values may transfer in the loop body", None));
        }
        self.available(place, span)?;
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
            ("fn f(flag: bool, view: &Percent) -> Percent { while flag { let observed = *view; } return *view; }", "pre-existing reference handle"),
        ] {
            let p = typed(body);
            let f = p.functions()[0].as_ordinary().unwrap();
            let mut c = Checker::new(&p, f);
            c.expire();
            let header = c.clone();
            let error = c.statements(&f.body[..1], f.id).unwrap_err();
            assert!(error.message.contains(message));
            assert!(error.required.contains("future cyclic") || error.required.contains("future generalized cyclic"));
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
    fn header_handle_guards_are_atomic_for_transfer_read_and_write() {
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
            .contains("pre-existing reference handle"));
        for write in [false, true] {
            assert!(c
                .borrowed_access(reference, f.span, write)
                .unwrap_err()
                .message
                .contains("pre-existing reference handle"));
        }
        assert!(c.same_loop_state(&header));
        assert_eq!(c.remaining, header.remaining);
        assert!(c.loans.values().all(|loan| loan.held == 0));
        c.loop_scope.as_mut().unwrap().condition = true;
        assert!(c
            .borrowed_access(reference, f.span, false)
            .unwrap_err()
            .message
            .contains("while conditions"));
        assert!(c.same_loop_state(&header));
        assert_eq!(c.remaining, header.remaining);
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
        assert!(inner
            .loop_handle(outer, *span)
            .unwrap_err()
            .message
            .contains("pre-existing reference handle"));
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
            "fn f(flag: bool, view: &Percent) -> Percent { while flag { let before = *view; } return *view; }",
            "fn f(flag: bool, value: Percent) -> Percent { while flag { let outer = &value; while flag { let before = *outer; } } return value; }",
        ] {
            let mut p = typed(body);
            let before = check(&p).unwrap_err();
            rename(&mut p);
            let after = check(&p).unwrap_err();
            assert_eq!(before[0].span, after[0].span);
            assert_eq!(before[0].required, after[0].required);
            assert_eq!(before[0].known, after[0].known);
            assert!(after[0].message.contains("pre-existing reference handle `display`"));
        }
    }
}
