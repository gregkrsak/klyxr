//! Structural call contracts, without interprocedural body inference.
use super::EffectSet;
use crate::{
    diagnostics::Diagnostic,
    hir::{ExprKind, FunctionId, Program, TypedExpr, ValueFunction, ValueStatement},
    lexer::Span,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Callee {
    Source(FunctionId),
    Trusted(crate::hir::TrustedOperationId),
}
impl Callee {
    pub(crate) fn effects(self, program: &Program) -> Option<EffectSet> {
        match self {
            Self::Source(id) => program
                .functions()
                .get(id.0)
                .filter(|f| f.id() == id)
                .and_then(|f| f.as_ordinary())
                .map(|f| f.effects.set()),
            Self::Trusted(id) => {
                let catalog = program.trusted.as_ref()?;
                catalog
                    .operation(id)
                    .filter(|op| {
                        op.source_name.is_some()
                            && catalog.granted(id)
                            && (!matches!(op.class, crate::trusted::OperationClass::Acquisition | crate::trusted::OperationClass::Completion)
                                || catalog.granted(catalog.resource.release))
                    })
                    .map(|op| op.effects)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallSite {
    pub(crate) ordinal: usize,
    pub(crate) function: FunctionId,
    pub(crate) callee: Callee,
    pub(crate) effects: EffectSet,
    pub(crate) origin: Span,
}

struct Collector<'a> {
    program: &'a Program,
    function: FunctionId,
    ordinal: usize,
    sites: Vec<CallSite>,
    checking: bool,
    violation: Option<CallSite>,
    failure: Option<crate::hir::resources::PlanError>,
}

#[cfg(test)]
thread_local! {
    pub(crate) static FAIL_ALLOCATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Collector<'_> {
    fn call(&mut self, callee: Callee, origin: Span, arguments: &[TypedExpr]) {
        let ordinal = self.ordinal;
        self.ordinal += 1;
        let contract = callee
            .effects(self.program)
            .expect("validated canonical call target");
        if contract != EffectSet::EMPTY {
            let site = CallSite {
                ordinal,
                function: self.function,
                callee,
                effects: contract,
                origin,
            };
            if self.checking {
                let declared = self
                    .program
                    .function(self.function)
                    .as_ordinary()
                    .unwrap()
                    .effects
                    .set();
                if !site.effects.is_subset(declared) && self.violation.is_none() {
                    self.violation = Some(site);
                }
            } else {
                #[cfg(test)]
                if FAIL_ALLOCATION.with(std::cell::Cell::get) {
                    self.failure = Some(crate::hir::resources::PlanError::Host(
                        "direct effect site allocation",
                    ));
                    return;
                }
                if self.sites.try_reserve_exact(1).is_err() {
                    self.failure = Some(crate::hir::resources::PlanError::Host(
                        "direct effect site allocation",
                    ));
                    return;
                }
                self.sites.push(site);
            }
        }
        for argument in arguments {
            self.expression(argument);
        }
    }

    fn expression(&mut self, expression: &TypedExpr) {
        if self.failure.is_some() || self.violation.is_some() {
            return;
        }
        match &expression.kind {
            ExprKind::ResultConstruct { payload, .. } => self.expression(payload),
            ExprKind::Call {
                function,
                arguments,
            } => {
                self.call(Callee::Source(*function), expression.span, arguments);
            }
            ExprKind::TrustedCall {
                operation,
                arguments,
            } => self.call(Callee::Trusted(*operation), expression.span, arguments),
            ExprKind::RecordConstruct { fields, .. } => {
                for field in fields {
                    self.expression(&field.value);
                }
            }
            ExprKind::IfValue {
                condition,
                then_value,
                else_value,
            } => {
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
            if self.failure.is_some() || self.violation.is_some() {
                break;
            }
            match statement {
                ValueStatement::Match {
                    scrutinee, arms, ..
                } => {
                    self.expression(scrutinee);
                    for arm in arms {
                        self.statements(&arm.body);
                    }
                }
                ValueStatement::CallNoValue {
                    function,
                    arguments,
                    span,
                } => {
                    self.call(Callee::Source(*function), *span, arguments);
                }
                ValueStatement::Let { initializer, .. } => self.expression(initializer),
                ValueStatement::Assign { value, .. }
                | ValueStatement::DerefAssign { value, .. }
                | ValueStatement::CopyFieldAssign { value, .. }
                | ValueStatement::CopyReferenceFieldAssign { value, .. }
                | ValueStatement::Return { value, .. } => self.expression(value),
                ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.expression(condition);
                    self.statements(then_body);
                    self.statements(else_body);
                }
                ValueStatement::While {
                    condition, body, ..
                } => {
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
pub(crate) fn sites(
    program: &Program,
    function: &ValueFunction,
) -> Result<Vec<CallSite>, crate::hir::resources::PlanError> {
    let mut collector = Collector {
        program,
        function: function.id,
        ordinal: 0,
        sites: Vec::new(),
        checking: false,
        violation: None,
        failure: None,
    };
    collector.statements(&function.body);
    match collector.failure {
        Some(error) => Err(error),
        None => Ok(collector.sites),
    }
}

pub(crate) fn check(program: &Program) -> Result<(), Vec<Diagnostic>> {
    for function in program.functions().iter().filter_map(|f| f.as_ordinary()) {
        // Checking needs only the first illegal edge, not a retained site table.
        // MIR retention below uses fallible allocation and typed Host failures.
        let mut collector = Collector {
            program,
            function: function.id,
            ordinal: 0,
            sites: Vec::new(),
            checking: true,
            violation: None,
            failure: None,
        };
        collector.statements(&function.body);
        if let Some(site) = collector.violation {
            if let Callee::Trusted(id) = site.callee {
                let catalog = program.trusted.as_ref().expect("checked selected catalog");
                let op = catalog.operation(id).expect("checked trusted call");
                let missing = site
                    .effects
                    .iter()
                    .find(|e| !function.effects.set().contains(*e))
                    .unwrap();
                return Err(vec![Diagnostic::semantic(site.origin,
                    format!("unadmitted effect `{}` in `{}`: trusted call to `{}` uses its complete canonical contract (catalog {} row {}; no source declaration)",missing.name(),function.name,op.source_name.unwrap(),crate::trusted::CATALOG,op.origin.row),
                    "admit the complete operation effect set; profile grants do not replace source effects")]);
            }
            let Callee::Source(id) = site.callee else {
                unreachable!()
            };
            let callee = program
                .function(id)
                .as_ordinary()
                .expect("typed ordinary callee");
            let missing = site
                .effects
                .iter()
                .find(|e| !function.effects.set().contains(*e))
                .unwrap();
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

#[cfg(test)]
mod tests {
    #[test]
    fn retained_direct_site_allocation_failure_is_host_and_publishes_no_executable() {
        use crate::execution::{self, AdmissionLimits, ErrorKind};
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                super::FAIL_ALLOCATION.with(|f| f.set(false));
            }
        }
        let _restore = Restore;
        super::FAIL_ALLOCATION.with(|f| f.set(true));
        let source = "fn leaf() effects io {} fn demo() effects io {leaf();}";
        // The checking traversal retains no table and grants no execution authority.
        assert!(crate::compile_source(source).is_ok());
        let error = execution::prepare_execution(source, AdmissionLimits::default())
            .err()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::Host("direct effect site allocation"));
    }
}
