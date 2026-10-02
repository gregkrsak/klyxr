//! Straight-line owned moves and whole-value, non-escaping loans in ordinary HIR.
//! Availability and active loans are orthogonal private state; no runtime behavior.
use std::collections::BTreeMap;

use crate::{
    diagnostics::Diagnostic,
    hir::{
        BorrowKind, ExprKind, FunctionId, LocalId, ParameterId, ParameterType, Place, Program,
        TypedExpr, ValueFunction, ValueStatement, ValueType,
    },
    lexer::Span,
};

type OwnershipResult<T = ()> = Result<T, Box<Diagnostic>>;

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
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Available,
    Moved { span: Span, transfer: Transfer },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LoanId(usize);
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
struct Checker<'a> {
    program: &'a Program,
    states: BTreeMap<Place, State>,
    loans: BTreeMap<LoanId, Loan>,
    handles: BTreeMap<Place, LoanId>,
    remaining: BTreeMap<Place, usize>,
    next_loan: usize,
}
impl<'a> Checker<'a> {
    fn new(program: &'a Program, function: &ValueFunction) -> Self {
        let mut checker = Self {
            program,
            states: BTreeMap::new(),
            loans: BTreeMap::new(),
            handles: BTreeMap::new(),
            remaining: BTreeMap::new(),
            next_loan: 0,
        };
        // The source subset is straight-line. Count syntactic handle uses once;
        // aliases inherit provenance during traversal, not through source names.
        for statement in &function.body {
            match statement {
                ValueStatement::Let { initializer, .. } => checker.count_uses(initializer),
                ValueStatement::Return { value, .. } => checker.count_uses(value),
            }
        }
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
    fn count_uses(&mut self, expression: &TypedExpr) {
        match &expression.kind {
            ExprKind::Parameter(id) => {
                *self.remaining.entry(Place::Parameter(*id)).or_default() += 1;
            }
            ExprKind::Local(id) => {
                *self.remaining.entry(Place::Local(*id)).or_default() += 1;
            }
            ExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    self.count_uses(argument);
                }
            }
            ExprKind::Unary { operand, .. } => self.count_uses(operand),
            ExprKind::Binary { left, right, .. } => {
                self.count_uses(left);
                self.count_uses(right);
            }
            _ => {}
        }
    }
    fn body(&mut self, function: &ValueFunction) -> OwnershipResult {
        for statement in &function.body {
            match statement {
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
                ValueStatement::Return { value, .. } => {
                    self.expression(value, Transfer::Return(function.id))?;
                }
            }
            self.expire();
        }
        Ok(())
    }
    fn expression(
        &mut self,
        expression: &TypedExpr,
        transfer: Transfer,
    ) -> OwnershipResult<Option<LoanId>> {
        match &expression.kind {
            ExprKind::Parameter(id) => {
                self.consume(Place::Parameter(*id), expression.span, transfer)
            }
            ExprKind::Local(id) => self.consume(Place::Local(*id), expression.span, transfer),
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
                        && !matches!(states.get(place), Some(State::Moved { .. }))
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
        if let Some(State::Moved {
            span: earlier,
            transfer: previous,
        }) = self.states.get(&place)
        {
            let destination = match previous {
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
}
