//! Expression type legality. Input references are resolved IDs; no name lookup occurs here.
use crate::{
    diagnostics::Diagnostic,
    hir::{
        BinaryOp, ExprKind, ExprType, ParameterType, Program, Subtract, TypedExpr, VerifiedFunction,
    },
    resolve::{ResolvedExpr, ResolvedExprKind, ResolvedProgram},
};

type TypeResult<T> = Result<T, Box<Diagnostic>>;

/// Type-check resolved expressions and publish canonical, read-only typed HIR.
pub fn check(resolved: ResolvedProgram) -> Result<Program, Vec<Diagnostic>> {
    let mut program = resolved.declarations;
    let mut diagnostics = Vec::new();
    for function in resolved.functions {
        let result = (|| {
            let requires = expression(&program, function.requires)?;
            contract(&requires, "requires")?;
            let ensures = expression(&program, function.ensures)?;
            contract(&ensures, "ensures")?;
            let mut body = Vec::new();
            for statement in function.body {
                let operand = expression(&program, statement.operand)?;
                let target = ExprType::Range(statement.target.ty);
                if operand.ty != target && operand.ty != ExprType::IntegerLiteral {
                    return Err(incompatible(
                        &program,
                        statement.span,
                        "subtract-assignment",
                        target,
                        operand.ty,
                    ));
                }
                body.push(Subtract {
                    target: statement.target,
                    operand,
                    span: statement.span,
                });
            }
            Ok(VerifiedFunction {
                id: function.id,
                name: function.name,
                span: function.span,
                state_param: function.state_param,
                state_type: function.state_type,
                amount_param: function.amount_param,
                requires,
                ensures,
                body,
            })
        })();
        match result {
            Ok(function) => program.functions.push(function),
            Err(error) => diagnostics.push(*error),
        }
    }
    if diagnostics.is_empty() {
        Ok(program)
    } else {
        Err(diagnostics)
    }
}
fn contract(expression: &TypedExpr, name: &str) -> TypeResult<()> {
    if expression.ty != ExprType::Bool {
        return Err(Diagnostic::semantic(
            expression.span,
            format!("{name} expression must have type Bool"),
            "contracts must be Boolean expressions",
        )
        .into());
    }
    Ok(())
}
fn display_type(program: &Program, ty: ExprType) -> String {
    match ty {
        ExprType::Bool => "Bool".into(),
        ExprType::IntegerLiteral => "IntegerLiteral".into(),
        ExprType::Range(id) => format!("range `{}`", program.range(id).name),
    }
}
fn incompatible(
    program: &Program,
    span: crate::lexer::Span,
    operator: &str,
    left: ExprType,
    right: ExprType,
) -> Box<Diagnostic> {
    let message = match (left, right) {
        (ExprType::Range(a), ExprType::Range(b)) if a != b => format!(
            "incompatible named range types for {operator}: `{}` and `{}`",
            program.range(a).name,
            program.range(b).name
        ),
        _ => format!("incompatible operand types for `{operator}`"),
    };
    let rule = match operator {
        "&&" | "||" => "both operands must be Bool",
        "==" => "use two Bool operands, the same named range type, or compatible range/literal operands",
        "<=" => "use the same named range type or range/literal operands",
        "subtraction" => "the left operand must be a named range; the right must be that same range or an integer literal",
        "subtract-assignment" => "the right operand must be the target's named range or an integer literal",
        _ => "operands must satisfy the documented operator type rule",
    };
    let nominal = matches!((left, right), (ExprType::Range(a), ExprType::Range(b)) if a != b);
    let restriction = if nominal {
        " This prototype does not permit implicit arithmetic or comparison between distinct named range types."
    } else {
        ""
    };
    Diagnostic::semantic(
        span,
        message,
        format!(
            "found {} and {}; {rule}.{restriction}",
            display_type(program, left),
            display_type(program, right)
        ),
    )
    .into()
}
fn binary_type(
    program: &Program,
    span: crate::lexer::Span,
    op: BinaryOp,
    left: ExprType,
    right: ExprType,
) -> TypeResult<ExprType> {
    use ExprType::*;
    match (op, left, right) {
        (BinaryOp::And | BinaryOp::Or, Bool, Bool) | (BinaryOp::Equal, Bool, Bool) => Ok(Bool),
        (BinaryOp::Equal | BinaryOp::LessEqual, Range(a), Range(b)) if a == b => Ok(Bool),
        (BinaryOp::Equal | BinaryOp::LessEqual, Range(_), IntegerLiteral)
        | (BinaryOp::Equal | BinaryOp::LessEqual, IntegerLiteral, Range(_))
        | (BinaryOp::Equal | BinaryOp::LessEqual, IntegerLiteral, IntegerLiteral) => Ok(Bool),
        (BinaryOp::Subtract, Range(a), Range(b)) if a == b => Ok(Range(a)),
        (BinaryOp::Subtract, Range(a), IntegerLiteral) => Ok(Range(a)),
        _ => Err(incompatible(
            program,
            span,
            if op == BinaryOp::Subtract {
                "subtraction"
            } else {
                op.spelling()
            },
            left,
            right,
        )),
    }
}
fn expression(program: &Program, input: ResolvedExpr) -> TypeResult<TypedExpr> {
    let span = input.span;
    let (kind, ty) = match input.kind {
        ResolvedExprKind::BoolLiteral(value) => (ExprKind::BoolLiteral(value), ExprType::Bool),
        ResolvedExprKind::IntegerLiteral(value) => {
            (ExprKind::IntegerLiteral(value), ExprType::IntegerLiteral)
        }
        ResolvedExprKind::Parameter(id) => {
            let parameter = program.parameter(id);
            let ParameterType::Range(ty) = parameter.ty else {
                return Err(Diagnostic::semantic(
                    span,
                    format!(
                        "record parameter `{}` is not a numeric expression",
                        parameter.name
                    ),
                    "use its declared constrained field",
                )
                .into());
            };
            (ExprKind::Parameter(id), ExprType::Range(ty))
        }
        ResolvedExprKind::FieldAccess(access) => {
            let ty = ExprType::Range(access.ty);
            (ExprKind::FieldAccess(access), ty)
        }
        ResolvedExprKind::OldField(access) => {
            let ty = ExprType::Range(access.ty);
            (ExprKind::OldField(access), ty)
        }
        ResolvedExprKind::Unary { op, operand } => {
            let operand = expression(program, *operand)?;
            if operand.ty != ExprType::Bool {
                return Err(Diagnostic::semantic(
                    span,
                    "Boolean negation requires Bool",
                    format!("found {}", display_type(program, operand.ty)),
                )
                .into());
            }
            (
                ExprKind::Unary {
                    op,
                    operand: Box::new(operand),
                },
                ExprType::Bool,
            )
        }
        ResolvedExprKind::Binary { op, left, right } => {
            let left = expression(program, *left)?;
            let right = expression(program, *right)?;
            let ty = binary_type(program, span, op, left.ty, right.ty)?;
            (
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                ty,
            )
        }
    };
    Ok(TypedExpr { kind, ty, span })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_source, resolve, verify::verify_report};

    #[test]
    fn expression_typing_uses_ids_when_diagnostic_names_change() {
        let ast = parse_source(include_str!("../../examples/battery_ok.klx")).unwrap();
        let mut resolved = resolve::resolve(&ast).unwrap();
        let range = resolved.declarations.ranges[0].id;
        for declaration in &mut resolved.declarations.ranges {
            declaration.name = "display".into();
        }
        for parameter in &mut resolved.declarations.parameters {
            parameter.name = "display".into();
        }
        for field in &mut resolved.declarations.fields {
            field.name = "display".into();
        }
        let program = check(resolved).unwrap();
        assert_eq!(
            program.functions()[0].body[0].operand.ty,
            ExprType::Range(range)
        );
        assert!(verify_report(&program).diagnostics.is_empty());

        for expression in [
            "amount <= battery.charge",
            "battery.charge == amount",
            "(battery.charge - amount) == 0",
        ] {
            let text = format!(
                "type Other = range 0..100;\n{}",
                include_str!("../../examples/battery.klx")
            )
            .replace("amount: Percent", "amount: Other")
            .replace(
                "requires amount <= battery.charge",
                &format!("requires {expression}"),
            );
            let mut resolved = resolve::resolve(&parse_source(&text).unwrap()).unwrap();
            assert_ne!(
                resolved.declarations.ranges[0].id,
                resolved.declarations.ranges[1].id
            );
            for range in &mut resolved.declarations.ranges {
                range.name = "display".into();
            }
            let errors = check(resolved).unwrap_err();
            assert!(errors[0].message.contains("incompatible named range types"));
            assert!(errors[0].message.contains("`display` and `display`"));
        }
    }
}
