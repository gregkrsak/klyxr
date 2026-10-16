//! Structural call contracts, without interprocedural body inference.
use super::EffectSet;
use crate::{
    diagnostics::Diagnostic,
    hir::{ExprKind, FunctionId, Program, TypedExpr, ValueFunction, ValueStatement},
    lexer::Span,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallSite {
    pub(crate) ordinal: usize,
    pub(crate) function: FunctionId,
    pub(crate) callee: FunctionId,
    pub(crate) effects: EffectSet,
    pub(crate) origin: Span,
}

struct Collector<'a> {
    program: &'a Program,
    function: FunctionId,
    ordinal: usize,
    sites: Vec<CallSite>,
}

impl Collector<'_> {
    fn call(&mut self, callee: FunctionId, origin: Span, arguments: &[TypedExpr]) {
        let ordinal = self.ordinal;
        self.ordinal += 1;
        let contract = self.program.function(callee).as_ordinary().expect("typed ordinary callee");
        if contract.effects.set() != EffectSet::EMPTY {
            self.sites.push(CallSite {
                ordinal,
                function: self.function,
                callee,
                effects: contract.effects.set(),
                origin,
            });
        }
        for argument in arguments {
            self.expression(argument);
        }
    }

    fn expression(&mut self, expression: &TypedExpr) {
        match &expression.kind {
            ExprKind::Call { function, arguments } => {
                self.call(*function, expression.span, arguments);
            }
            ExprKind::RecordConstruct { fields, .. } => {
                for field in fields {
                    self.expression(&field.value);
                }
            }
            ExprKind::IfValue { condition, then_value, else_value } => {
                self.expression(condition);
                self.expression(then_value);
                self.expression(else_value);
            }
            ExprKind::Unary { operand, .. } => self.expression(operand),
            ExprKind::Binary { left, right, .. } => {
                self.expression(left);
                self.expression(right);
            }
            _ => {}
        }
    }

    fn statements(&mut self, statements: &[ValueStatement]) {
        for statement in statements {
            match statement {
                ValueStatement::CallNoValue { function, arguments, span } => {
                    self.call(*function, *span, arguments);
                }
                ValueStatement::Let { initializer, .. } => self.expression(initializer),
                ValueStatement::Assign { value, .. }
                | ValueStatement::DerefAssign { value, .. }
                | ValueStatement::CopyFieldAssign { value, .. }
                | ValueStatement::CopyReferenceFieldAssign { value, .. }
                | ValueStatement::Return { value, .. } => self.expression(value),
                ValueStatement::If { condition, then_body, else_body, .. } => {
                    self.expression(condition);
                    self.statements(then_body);
                    self.statements(else_body);
                }
                ValueStatement::While { condition, body, .. } => {
                    self.expression(condition);
                    self.statements(body);
                }
                _ => {}
            }
        }
    }
}

/// The existing parser/type structural-reachability rules remain authoritative.
/// In particular both short-circuit operands and terminal branch arms contribute.
pub(crate) fn sites(program: &Program, function: &ValueFunction) -> Vec<CallSite> {
    let mut collector = Collector {
        program,
        function: function.id,
        ordinal: 0,
        sites: Vec::new(),
    };
    collector.statements(&function.body);
    collector.sites
}

pub(crate) fn check(program: &Program) -> Result<(), Vec<Diagnostic>> {
    for function in program.functions().iter().filter_map(|f| f.as_ordinary()) {
        for site in sites(program, function) {
            if site.effects.is_subset(function.effects.set()) {
                continue;
            }
            let callee = program.function(site.callee).as_ordinary().expect("typed ordinary callee");
            let missing = site.effects.iter().find(|e| !function.effects.set().contains(*e)).unwrap();
            let declaration = callee.effects.origin(missing).unwrap_or(callee.span);
            return Err(vec![Diagnostic::semantic(
                site.origin,
                format!(
                    "unadmitted effect `{}` in `{}`: call to `{}` uses its complete declared contract (declaration at {}:{})",
                    missing.name(), function.name, callee.name, declaration.line, declaration.column
                ),
                "admit the callee's complete declared effect set; unused declared members remain caller-visible",
            )]);
        }
    }
    Ok(())
}
