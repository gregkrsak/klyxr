//! Core owned-value moves for canonical typed ordinary functions only.
//! This straight-line pass has no borrowing, destruction, execution, or proof semantics.
use std::collections::BTreeMap;

use crate::{
    diagnostics::Diagnostic,
    hir::{
        ExprKind, FunctionId, LocalId, ParameterId, ParameterType, Program, TypedExpr,
        ValueFunction, ValueStatement, ValueType,
    },
    lexer::Span,
};

type OwnershipResult = Result<(), Box<Diagnostic>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnershipKind {
    Copy,
    Move,
}
fn ownership_kind(ty: ValueType) -> OwnershipKind {
    match ty {
        ValueType::Bool | ValueType::Range(_) => OwnershipKind::Copy,
        ValueType::Record(_) => OwnershipKind::Move,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Parameter(ParameterId),
    Local(LocalId),
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

/// Inspect typed HIR without rebuilding it. Report the first precise ownership error.
/// Callers must first resolve and type-check; compile_source runs the full pipeline.
pub fn check(program: &Program) -> Result<(), Vec<Diagnostic>> {
    for function in program.functions().iter().filter_map(|f| f.as_ordinary()) {
        let mut checker = Checker::new(program, function);
        checker.body(function).map_err(|error| vec![*error])?;
    }
    Ok(())
}

struct Checker<'a> {
    program: &'a Program,
    states: BTreeMap<Place, State>,
}
impl<'a> Checker<'a> {
    fn new(program: &'a Program, function: &ValueFunction) -> Self {
        let mut checker = Self {
            program,
            states: BTreeMap::new(),
        };
        for id in &function.parameters {
            if let ParameterType::Value(ty) = program.parameter(*id).ty {
                if ownership_kind(ty) == OwnershipKind::Move {
                    checker
                        .states
                        .insert(Place::Parameter(*id), State::Available);
                }
            }
        }
        checker
    }
    fn body(&mut self, function: &ValueFunction) -> OwnershipResult {
        for statement in &function.body {
            match statement {
                ValueStatement::Let {
                    local, initializer, ..
                } => {
                    self.expression(initializer, Transfer::Local(*local))?;
                    if ownership_kind(self.program.local(*local).ty) == OwnershipKind::Move {
                        self.states.insert(Place::Local(*local), State::Available);
                    }
                }
                ValueStatement::Return { value, .. } => {
                    self.expression(value, Transfer::Return(function.id))?;
                }
            }
        }
        Ok(())
    }
    fn expression(&mut self, expression: &TypedExpr, transfer: Transfer) -> OwnershipResult {
        match &expression.kind {
            ExprKind::Parameter(id) => {
                self.consume(Place::Parameter(*id), expression.span, transfer)
            }
            ExprKind::Local(id) => self.consume(Place::Local(*id), expression.span, transfer),
            ExprKind::Call {
                function,
                arguments,
            } => {
                // Typing already established ordinary callee identity and exact arity.
                let callee = self
                    .program
                    .function(*function)
                    .as_ordinary()
                    .expect("typed call expressions target ordinary functions");
                for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
                    self.expression(argument, Transfer::Argument(*parameter))?;
                }
                // The result is a fresh value, not an independently reusable place.
                Ok(())
            }
            ExprKind::Unary { operand, .. } => self.expression(operand, transfer),
            ExprKind::Binary { left, right, .. } => {
                // Deterministic diagnostic traversal, not a runtime evaluation-order rule.
                self.expression(left, transfer)?;
                self.expression(right, transfer)
            }
            ExprKind::BoolLiteral(_)
            | ExprKind::IntegerLiteral(_)
            | ExprKind::FieldAccess(_)
            | ExprKind::OldField(_) => Ok(()),
        }
    }
    fn consume(&mut self, place: Place, span: Span, transfer: Transfer) -> OwnershipResult {
        let (name, ty) = match place {
            Place::Parameter(id) => {
                let parameter = self.program.parameter(id);
                let ParameterType::Value(ty) = parameter.ty else {
                    // Verified reference/range parameters are outside this owned-value model.
                    return Ok(());
                };
                (&parameter.name, ty)
            }
            Place::Local(id) => {
                let local = self.program.local(id);
                (&local.name, local.ty)
            }
        };
        if ownership_kind(ty) == OwnershipKind::Copy {
            return Ok(());
        }
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
                    required:
                        "a non-Copy owned value cannot be reused after transfer; use its new owner"
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
        self.states.insert(place, State::Moved { span, transfer });
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
}
