//! Bounded views of the existing opaque execution authority. No second IR,
//! ownership solver, native transport or source execution authority is created.
use super::*;
use crate::hir::{ExprKind, TypedExpr};
use crate::mir::{BasicBlockId, Statement, Terminator};
use std::mem::size_of;

/// The versions are one sealed unit: changing a schedule changes the contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schema {
    semantic: u32,
    accounting: u32,
    cuts: u32,
    observation: u32,
    replay: u32,
    fuel_unit: u64,
}
impl Schema {
    pub const V1: Self = Self {
        semantic: 1,
        accounting: 1,
        cuts: 1,
        observation: 1,
        replay: 1,
        fuel_unit: 1,
    };
    pub(crate) fn validate(self) -> Result<(), ExecutionError> {
        if self != Self::V1 {
            return Err(ExecutionError::integrity(
                "unsupported execution contract schema",
            ));
        }
        Ok(())
    }
    pub(crate) fn fuel_unit(self) -> u64 {
        self.fuel_unit
    }
}

/// Explicit abstract compatibility quantities from the reference interpreter
/// build. A future engine consumes these sealed integers, never its own layout.
/// Capturing the reference geometry does not select a native ABI or target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub(super) machine: [usize; 8],
    pub(super) cleanup: [usize; 9],
    pub(super) results: [usize; 5],
    pub(super) temporaries: [usize; 4],
    pub(super) trusted: [usize; 8],
}
impl Schedule {
    pub(crate) fn reference() -> Self {
        machine::compatibility_schedule()
    }
    pub(crate) fn validate(self) -> Result<(), ExecutionError> {
        if self != Self::reference() {
            return Err(ExecutionError::integrity(
                "execution compatibility schedule mismatch",
            ));
        }
        Ok(())
    }
    pub fn quantities(&self) -> impl Iterator<Item = usize> + '_ {
        self.machine
            .iter()
            .chain(&self.cleanup)
            .chain(&self.results)
            .chain(&self.temporaries)
            .chain(&self.trusted)
            .copied()
    }
}

/// Domains remain distinct even when they share a byte/step representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    Preparation,
    LogicalExecution,
    BackendProduction,
    PhysicalEngine,
    Observation,
    AuthorityAudit,
    Presentation,
}

/// A separately admitted extraction budget, never an ordinary execution limit.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub nodes: usize,
    pub work: usize,
    pub storage: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 16_384,
            work: 1_000_000,
            storage: 4_194_304,
        }
    }
}

/// Static identities remain exact under the same approved compilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Statement,
    Terminator,
    Expression,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Site {
    pub function: FunctionId,
    pub block: BasicBlockId,
    pub statement: usize,
    pub ordinal: usize,
    pub role: Role,
    pub origin: Span,
}

/// Only extraction from a checked executable can create this borrowed view.
/// The executable, its plans, witnesses and trusted contracts remain immutable.
pub struct Contract<'a> {
    pub(super) executable: &'a CheckedExecutable,
    schema: Schema,
    schedule: Schedule,
    entry: FunctionId,
    functions: Vec<FunctionId>,
    sites: Vec<Site>,
    // Private lookup keys only. Addresses never enter semantic comparison;
    // runtime operands are projected to their exact approved static ordinal.
    expressions: Vec<(usize, usize)>,
    limits: Limits,
}
impl<'a> Contract<'a> {
    pub fn extract(
        executable: &'a CheckedExecutable,
        entry: FunctionId,
        limits: Limits,
    ) -> Result<Self, ExecutionError> {
        // Preserve the original canonical/approval and public-entry boundaries.
        checked_entry(executable, entry, ExecutionLimits::default())?;
        if limits.nodes == 0
            || limits.nodes > 65_536
            || limits.work == 0
            || limits.work > 100_000_000
            || limits.storage == 0
            || limits.storage > 67_108_864
        {
            return Err(ResourceFailure::limit("contract extraction override", START).into());
        }
        let bytes = limits
            .nodes
            .checked_mul(size_of::<Site>() + size_of::<FunctionId>() + size_of::<(usize, usize)>())
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(2 * size_of::<Self>()))
            .ok_or_else(|| ExecutionError::integrity("contract storage overflow"))?;
        if bytes > limits.storage {
            return Err(ResourceFailure::limit("contract extraction storage", START).into());
        }
        let mut contract = Self {
            executable,
            schema: executable.approval.schema,
            schedule: executable.approval.schedule,
            entry,
            functions: Vec::new(),
            sites: Vec::new(),
            expressions: Vec::new(),
            limits,
        };
        contract
            .functions
            .try_reserve_exact(limits.nodes)
            .map_err(|_| {
                ExecutionError::new(ErrorKind::Host("contract closure allocation"), START)
            })?;
        contract
            .sites
            .try_reserve_exact(limits.nodes)
            .map_err(|_| ExecutionError::new(ErrorKind::Host("contract site allocation"), START))?;
        contract.expressions.try_reserve_exact(limits.nodes)
            .map_err(|_| ExecutionError::new(ErrorKind::Host("contract operand index allocation"), START))?;
        contract.functions.push(entry);
        let mut work = limits.work;
        let mut cursor = 0;
        // One visited entry per canonical function, even with recursive calls.
        while cursor < contract.functions.len() {
            let function = contract.functions[cursor];
            let definition = executable.function(function)?;
            for (block, body) in definition.blocks() {
                for (index, statement) in body.statements().iter().enumerate() {
                    let span = statement_span(statement);
                    contract.add(function, block, index, Role::Statement, span, &mut work)?;
                    match statement {
                        Statement::CallNoValue {
                            function: callee,
                            arguments,
                            ..
                        } => {
                            contract.call(*callee, &mut work)?;
                            for argument in arguments {
                                contract.expression(function, block, index, argument, &mut work)?;
                            }
                        }
                        Statement::Let {
                            initializer: value, ..
                        }
                        | Statement::Assign { value, .. }
                        | Statement::ReplaceOwned { value, .. }
                        | Statement::CopyFieldAssign { value, .. }
                        | Statement::CopyReferenceFieldAssign { value, .. }
                        | Statement::DerefAssign { value, .. } => {
                            contract.expression(function, block, index, value, &mut work)?;
                        }
                        Statement::BindResult { .. } => {}
                    }
                }
                let index = body.statements().len();
                let origin = executable.origin(function, block)?.span;
                contract.add(function, block, index, Role::Terminator, origin, &mut work)?;
                match body.terminator() {
                    Terminator::Return { value }
                    | Terminator::Branch {
                        condition: value, ..
                    }
                    | Terminator::Match {
                        scrutinee: value, ..
                    }
                    | Terminator::Propagate {
                        initializer: value, ..
                    } => {
                        contract.expression(function, block, index, value, &mut work)?;
                    }
                    _ => {}
                }
            }
            cursor += 1;
        }
        Ok(contract)
    }
    pub fn schema(&self) -> Schema {
        self.schema
    }
    pub fn schedule(&self) -> Schedule {
        self.schedule
    }
    pub fn entry(&self) -> FunctionId {
        self.entry
    }
    pub fn functions(&self) -> &[FunctionId] {
        &self.functions
    }
    pub fn sites(&self) -> &[Site] {
        &self.sites
    }
    pub(crate) fn expressions(&self) -> &[(usize, usize)] { &self.expressions }
    /// Identity comparison does not equate independent source compilations.
    pub fn same_approval(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.executable.approval, &other.executable.approval)
            && self.schema == other.schema
    }
    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.schema.validate()?;
        self.schedule.validate()?;
        self.executable.validate()?;
        let expected = Self::extract(self.executable, self.entry, self.limits)?;
        if self.schema != self.executable.approval.schema
            || self.schedule != self.executable.approval.schedule
            || self.functions != expected.functions
            || self.sites != expected.sites
            || self.expressions != expected.expressions
        {
            return Err(ExecutionError::integrity(
                "execution contract correspondence",
            ));
        }
        Ok(())
    }
    fn spend(work: &mut usize) -> Result<(), ExecutionError> {
        *work = work.checked_sub(1).ok_or_else(|| {
            ExecutionError::from(ResourceFailure::limit("contract extraction work", START))
        })?;
        Ok(())
    }
    fn call(&mut self, function: FunctionId, work: &mut usize) -> Result<(), ExecutionError> {
        for id in &self.functions {
            Self::spend(work)?;
            if *id == function {
                return Ok(());
            }
        }
        if self.functions.len() == self.limits.nodes {
            return Err(ResourceFailure::limit("contract closure nodes", START).into());
        }
        self.executable.function(function)?;
        self.functions.push(function);
        Ok(())
    }
    fn add(
        &mut self,
        function: FunctionId,
        block: BasicBlockId,
        statement: usize,
        role: Role,
        origin: Span,
        work: &mut usize,
    ) -> Result<(), ExecutionError> {
        Self::spend(work)?;
        if self.sites.len() == self.limits.nodes {
            return Err(ResourceFailure::limit("contract site nodes", origin).into());
        }
        self.sites.push(Site {
            function,
            block,
            statement,
            ordinal: self.sites.len(),
            role,
            origin,
        });
        Ok(())
    }
    fn expression(
        &mut self,
        function: FunctionId,
        block: BasicBlockId,
        statement: usize,
        value: &TypedExpr,
        work: &mut usize,
    ) -> Result<(), ExecutionError> {
        self.add(
            function,
            block,
            statement,
            Role::Expression,
            value.span,
            work,
        )?;
        self.expressions.push((value as *const TypedExpr as usize, self.sites.len() - 1));
        // Executable shape validation already bounded expression depth.
        match &value.kind {
            ExprKind::Call {
                function: callee,
                arguments,
            } => {
                self.call(*callee, work)?;
                for child in arguments {
                    self.expression(function, block, statement, child, work)?;
                }
            }
            ExprKind::TrustedCall { arguments, .. } => {
                for child in arguments {
                    self.expression(function, block, statement, child, work)?;
                }
            }
            ExprKind::RecordConstruct { fields, .. } => {
                for child in fields {
                    self.expression(function, block, statement, &child.value, work)?;
                }
            }
            ExprKind::ResultConstruct { payload, .. }
            | ExprKind::Unary {
                operand: payload, ..
            } => {
                self.expression(function, block, statement, payload, work)?;
            }
            ExprKind::Binary { left, right, .. } => {
                self.expression(function, block, statement, left, work)?;
                self.expression(function, block, statement, right, work)?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn statement_span(statement: &Statement) -> Span {
    match statement {
        Statement::BindResult { match_origin, .. } => *match_origin,
        Statement::CallNoValue { span, .. }
        | Statement::CopyReferenceFieldAssign { span, .. }
        | Statement::CopyFieldAssign { span, .. }
        | Statement::Assign { span, .. }
        | Statement::ReplaceOwned { span, .. }
        | Statement::Let { span, .. }
        | Statement::DerefAssign { span, .. } => *span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_schedule_is_sealed_and_independent_mutation_rejects() {
        let e =
            prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        let mut c = Contract::extract(&e, e.entry("demo").unwrap(), Limits::default()).unwrap();
        assert_eq!(c.schedule().quantities().count(), 34);
        assert!(c.schedule().quantities().all(|n| n > 0));
        c.schedule.machine[2] += 1;
        assert!(matches!(
            c.validate().unwrap_err().kind,
            ErrorKind::Integrity(_)
        ));
    }

    #[test]
    fn schema_is_consumed_before_execution_and_cannot_change_fuel() {
        let mut e =
            prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        let mut schema = e.approval.schema;
        schema.fuel_unit = 0;
        let approval = Rc::new(Approval {
            schema,
            schedule: e.approval.schedule,
            outcomes: e.approval.outcomes.clone(),
            program: e.approval.program.clone(),
            cleanup_sites: e.approval.cleanup_sites.clone(),
            domain: e.approval.domain.clone(),
            trusted: e.approval.trusted.clone(),
        });
        e.approval = approval.clone();
        e.mir_approval = approval.clone();
        e.origin_approval = approval;
        assert!(matches!(
            execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
                .unwrap_err()
                .kind,
            ErrorKind::Integrity(_)
        ));
    }

    #[test]
    fn logical_fuel_neighbors_and_extraction_storage_are_separate() {
        let e =
            prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        let entry = e.entry("demo").unwrap();
        assert_eq!(
            execute(
                &e,
                entry,
                ExecutionLimits {
                    steps: 3,
                    ..ExecutionLimits::default()
                }
            )
            .unwrap(),
            ExecutionResult::Value(RuntimeValue::Bool(true))
        );
        assert_eq!(
            execute(
                &e,
                entry,
                ExecutionLimits {
                    steps: 2,
                    ..ExecutionLimits::default()
                }
            )
            .unwrap_err()
            .kind,
            ErrorKind::ConfiguredLimit("steps")
        );
        let limits = Limits {
            nodes: 2,
            work: 100,
            storage: 2 * (size_of::<Site>() + size_of::<FunctionId>() + size_of::<(usize, usize)>()) * 2
                + 2 * size_of::<Contract<'_>>(),
        };
        Contract::extract(&e, entry, limits)
            .unwrap()
            .validate()
            .unwrap();
        assert!(matches!(
            Contract::extract(
                &e,
                entry,
                Limits {
                    storage: limits.storage - 1,
                    ..limits
                }
            )
            .err()
            .unwrap()
            .kind,
            ErrorKind::ConfiguredLimit("contract extraction storage")
        ));
        assert_eq!(
            execute(&e, entry, ExecutionLimits::default()).unwrap(),
            ExecutionResult::Value(RuntimeValue::Bool(true))
        );
    }

    #[test]
    fn ordinary_empty_effect_closure_is_real_and_recursion_is_finite() {
        let e = prepare_execution(
            "fn first(){second();}fn second(){first();}fn demo(){first();}",
            AdmissionLimits::default(),
        )
        .unwrap();
        let contract = Contract::extract(&e, e.entry("demo").unwrap(), Limits::default()).unwrap();
        assert_eq!(
            contract.functions(),
            &[FunctionId(2), FunctionId(0), FunctionId(1)]
        );
        assert_eq!(contract.sites().len(), 6);
        contract.validate().unwrap();
    }

    #[test]
    fn source_similarity_cannot_replace_opaque_approval() {
        let a =
            prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        let b =
            prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        let first = Contract::extract(&a, a.entry("demo").unwrap(), Limits::default()).unwrap();
        let second = Contract::extract(&b, b.entry("demo").unwrap(), Limits::default()).unwrap();
        assert!(!first.same_approval(&second));
        assert!(first.same_approval(&first));
    }

    #[test]
    fn independent_contract_schema_site_and_closure_mutations_reject() {
        let e =
            prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        for mutation in 0..4 {
            let mut c = Contract::extract(&e, e.entry("demo").unwrap(), Limits::default()).unwrap();
            match mutation {
                0 => c.schema.accounting += 1,
                1 => c.sites[0].origin.start += 1,
                2 => c.sites.pop().map(|_| ()).unwrap(),
                _ => c.functions.clear(),
            }
            assert!(c.validate().is_err());
        }
    }

    #[test]
    fn extraction_limits_never_change_the_ordinary_result() {
        let e =
            prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        assert!(Contract::extract(
            &e,
            e.entry("demo").unwrap(),
            Limits {
                work: 1,
                ..Limits::default()
            }
        )
        .is_err());
        assert_eq!(
            execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap(),
            ExecutionResult::Value(RuntimeValue::Bool(true))
        );
    }
}
