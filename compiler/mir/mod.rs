//! Deterministic control-flow lowering of checked ordinary HIR. No execution or proof.
use crate::hir::{self, FunctionId, LocalId, Place, TypedExpr, ValueStatement};
use crate::lexer::Span;
pub(crate) mod cleanup;
pub(crate) mod temporaries;

/// Compilation-local CFG identity, unrelated to source declaration identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BasicBlockId(pub(crate) usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    BindResult {
        result: hir::ResultTypeId,
        tag: hir::ResultTag,
        local: LocalId,
        match_origin: Span,
        pattern_origin: Span,
    },
    CallNoValue {
        function: FunctionId,
        arguments: Vec<TypedExpr>,
        span: Span,
    },
    CopyReferenceFieldAssign {
        reference: Place,
        record: hir::RecordId,
        field: hir::FieldId,
        value: TypedExpr,
        reference_span: Span,
        field_span: Span,
        span: Span,
    },
    CopyFieldAssign {
        owner: LocalId,
        record: hir::RecordId,
        field: hir::FieldId,
        owner_span: Span,
        field_span: Span,
        value: TypedExpr,
        span: Span,
    },
    Assign {
        local: LocalId,
        value: TypedExpr,
        span: Span,
    },
    /// One indivisible source operation; execution protects the RHS, discharges
    /// the displaced owner, then publishes and registers its successor.
    ReplaceOwned {
        local: LocalId,
        record: hir::RecordId,
        owning_scope: usize,
        cleanup_plan: Option<hir::RecordId>,
        value: TypedExpr,
        origin: Origin,
        span: Span,
    },
    Let {
        local: LocalId,
        initializer: TypedExpr,
        span: Span,
    },
    DerefAssign {
        reference: Place,
        value: TypedExpr,
        span: Span,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Propagate {
        operation: hir::Propagation,
        local: LocalId,
        owning_scope: usize,
        initializer: TypedExpr,
        span: Span,
        ok_target: BasicBlockId,
        error_target: BasicBlockId,
    },
    ForwardError {
        operation: hir::Propagation,
    },
    Match {
        result: hir::ResultTypeId,
        scrutinee: TypedExpr,
        ok_target: BasicBlockId,
        error_target: BasicBlockId,
    },
    ReturnNoValue,
    Goto {
        target: BasicBlockId,
    },
    Branch {
        condition: TypedExpr,
        then_target: BasicBlockId,
        else_target: BasicBlockId,
    },
    Return {
        value: TypedExpr,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlock {
    statements: Vec<Statement>,
    terminator: Terminator,
}
impl BasicBlock {
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }
    pub fn terminator(&self) -> &Terminator {
        &self.terminator
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFunction {
    pub(crate) cleanup_effect_sites: Vec<crate::effects::custody::Site>,
    pub(crate) direct_effects: Vec<crate::effects::calls::CallSite>,
    effects: crate::effects::Declaration,
    pub(crate) temporaries: Option<temporaries::Flow>,
    pub(crate) cleanup: Option<cleanup::Flow>,
    function: FunctionId,
    entry: BasicBlockId,
    blocks: Vec<BasicBlock>,
}
impl MirFunction {
    pub fn effects(&self) -> &crate::effects::Declaration {
        &self.effects
    }
    pub fn function(&self) -> FunctionId {
        self.function
    }
    pub fn entry(&self) -> BasicBlockId {
        self.entry
    }
    pub fn blocks(&self) -> impl ExactSizeIterator<Item = (BasicBlockId, &BasicBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .map(|(i, block)| (BasicBlockId(i), block))
    }
    pub fn block(&self, id: BasicBlockId) -> &BasicBlock {
        &self.blocks[id.0]
    }
    /// Validate compiler structure, not source semantics or numerical obligations.
    pub fn validate(&self) -> Result<(), &'static str> {
        self.validate_bounded()
            .map_err(hir::resources::PlanError::message)
    }
    pub(crate) fn validate_bounded(&self) -> Result<(), hir::resources::PlanError> {
        if !self.effects.validate() {
            return Err("invalid MIR effect declaration".into());
        }
        for site in &self.cleanup_effect_sites {
            if site.function != self.function
                || !site.effects.is_subset(self.effects.set())
                || site.effects == crate::effects::EffectSet::EMPTY
                || site.origin.line == 0
                || site.origin.column == 0
                || site.origin.start > site.origin.end
            {
                return Err("invalid MIR cleanup effect site".into());
            }
        }
        for (index, site) in self.direct_effects.iter().enumerate() {
            if site.function != self.function
                || site.effects == crate::effects::EffectSet::EMPTY
                || !site.effects.is_subset(self.effects.set())
                || site.origin.line == 0
                || site.origin.column == 0
                || site.origin.start > site.origin.end
                || index > 0 && self.direct_effects[index - 1].ordinal >= site.ordinal
            {
                return Err("invalid MIR direct effect site".into());
            }
        }
        fn visit(
            f: &MirFunction,
            entry: BasicBlockId,
            marks: &mut [u8],
        ) -> Result<(), hir::resources::PlanError> {
            let mut pending = Vec::new();
            #[cfg(test)]
            temporaries::allocation()?;
            pending
                .try_reserve_exact(f.blocks.len().saturating_add(1))
                .map_err(|_| hir::resources::PlanError::Host("MIR validation allocation"))?;
            pending.push(entry);
            while let Some(id) = pending.pop() {
                let mark = marks.get_mut(id.0).ok_or("invalid block target")?;
                if *mark != 0 {
                    continue;
                }
                *mark = 1;
                match &f.block(id).terminator {
                    Terminator::Propagate {
                        operation,
                        local,
                        initializer,
                        ok_target,
                        error_target,
                        ..
                    } => {
                        if initializer.ty != hir::ExprType::Result(operation.inner)
                            || ok_target == error_target
                            || !matches!(f.blocks.get(ok_target.0).and_then(|b|b.statements.first()),Some(Statement::BindResult {result,tag:hir::ResultTag::Ok,local:l,match_origin,pattern_origin}) if *result==operation.inner && l==local && *match_origin==operation.span && *pattern_origin==operation.syntax.binding)
                            || !matches!(f.blocks.get(error_target.0),Some(BasicBlock {statements,terminator:Terminator::ForwardError {operation:out}}) if statements.is_empty() && out==operation)
                        {
                            return Err("invalid propagation dispatch or transfer".into());
                        }
                        pending.push(*error_target);
                        pending.push(*ok_target);
                    }
                    Terminator::ForwardError { .. } => {}
                    Terminator::Match {
                        result,
                        scrutinee,
                        ok_target,
                        error_target,
                    } => {
                        if scrutinee.ty != hir::ExprType::Result(*result)
                            || ok_target == error_target
                        {
                            return Err("invalid Result dispatch".into());
                        }
                        for (target, tag) in [
                            (*ok_target, hir::ResultTag::Ok),
                            (*error_target, hir::ResultTag::Error),
                        ] {
                            if !matches!(f.blocks.get(target.0).and_then(|b| b.statements.first()), Some(Statement::BindResult { result: r, tag: t, .. }) if r == result && *t == tag)
                            {
                                return Err(
                                    "Result arm does not begin with its canonical payload transfer"
                                        .into(),
                                );
                            }
                            pending.push(target);
                        }
                    }
                    Terminator::Goto { target } => pending.push(*target),
                    Terminator::Branch {
                        condition,
                        then_target,
                        else_target,
                    } => {
                        if condition.ty != hir::ExprType::Bool {
                            return Err("non-Bool branch condition".into());
                        }
                        pending.push(*else_target);
                        pending.push(*then_target);
                    }
                    Terminator::Return { .. } | Terminator::ReturnNoValue => {}
                }
            }
            Ok(())
        }
        fn expression_valid(expression: &TypedExpr) -> Result<(), hir::resources::PlanError> {
            match &expression.kind {
                hir::ExprKind::ResultConstruct { result, payload, .. } => {
                    if expression.ty != hir::ExprType::Result(*result) { return Err("invalid Result constructor type".into()); }
                    expression_valid(payload)?;
                }
                hir::ExprKind::IntegerLiteral(_) => {
                    if expression.ty != hir::ExprType::IntegerLiteral { return Err("raw integer literal has invalid type".into()); }
                }
                hir::ExprKind::FormedRangeLiteral(_) => {
                    if !matches!(expression.ty, hir::ExprType::Range(_)) { return Err("formed range literal has non-range type".into()); }
                }
                hir::ExprKind::CopyReferenceFieldRead { .. } => {
                    if !matches!(expression.ty, hir::ExprType::Range(_)) { return Err("reference Copy-field read has non-range result".into()); }
                }
                hir::ExprKind::IfValue { .. } => return Err("conditional value must lower to control flow".into()),
                hir::ExprKind::RecordConstruct { fields, .. } => { for entry in fields { expression_valid(&entry.value)?; } },
                hir::ExprKind::Call { arguments, .. } => for argument in arguments { expression_valid(argument)?; },
                hir::ExprKind::Unary { operand, .. } => expression_valid(operand)?,
                hir::ExprKind::Binary { left, right, .. } => { expression_valid(left)?; expression_valid(right)?; },
                hir::ExprKind::FieldAccess(_) | hir::ExprKind::OldField(_) => return Err("verified-state or residual projection representation is ineligible in ordinary MIR".into()),
                hir::ExprKind::CopyFieldRead { .. } | hir::ExprKind::Parameter(_) | hir::ExprKind::Local(_) | hir::ExprKind::Deref { .. } | hir::ExprKind::Borrow { .. } | hir::ExprKind::BoolLiteral(_) => {}
            }
            Ok(())
        }
        for block in &self.blocks {
            for statement in &block.statements {
                match statement {
                    Statement::BindResult {
                        match_origin,
                        pattern_origin,
                        ..
                    } => {
                        if pattern_origin.start < match_origin.start
                            || pattern_origin.end > match_origin.end
                        {
                            return Err("invalid Result transfer origin".into());
                        }
                    }
                    Statement::CallNoValue { arguments, .. } => {
                        for argument in arguments {
                            expression_valid(argument)?;
                        }
                    }
                    Statement::CopyReferenceFieldAssign { value, .. }
                    | Statement::CopyFieldAssign { value, .. } => {
                        if !matches!(value.ty, hir::ExprType::Range(_)) {
                            return Err("Copy-field assignment requires a named-range RHS".into());
                        }
                        expression_valid(value)?;
                    }
                    Statement::Let { initializer, .. } => expression_valid(initializer)?,
                    Statement::ReplaceOwned {
                        local,
                        record,
                        owning_scope,
                        cleanup_plan,
                        value,
                        origin,
                        span,
                    } => {
                        if value.ty != hir::ExprType::Record(*record)
                            || cleanup_plan.is_some_and(|id| id != *record)
                            || origin.kind != OriginKind::Replacement
                            || origin.span != *span
                            || !self.cleanup.as_ref().is_some_and(|flow| {
                                flow.scopes
                                    .get(*owning_scope)
                                    .is_some_and(|s| s.owners.contains(local))
                            })
                        {
                            return Err("malformed owned replacement authority".into());
                        }
                        expression_valid(value)?;
                    }
                    Statement::DerefAssign { value, .. } | Statement::Assign { value, .. } => {
                        expression_valid(value)?
                    }
                }
            }
            match &block.terminator {
                Terminator::Branch { condition, .. } => expression_valid(condition)?,
                Terminator::Match { scrutinee, .. } => expression_valid(scrutinee)?,
                Terminator::Propagate { initializer, .. } => expression_valid(initializer)?,
                Terminator::ForwardError { .. } => {}
                Terminator::Return { value } => expression_valid(value)?,
                Terminator::Goto { .. } | Terminator::ReturnNoValue => {}
            }
        }
        if let Some(flow) = &self.cleanup {
            flow.validate_bounded(self)?;
        }
        let mut marks = Vec::new();
        #[cfg(test)]
        temporaries::allocation()?;
        marks
            .try_reserve_exact(self.blocks.len())
            .map_err(|_| hir::resources::PlanError::Host("MIR validation allocation"))?;
        marks.resize(self.blocks.len(), 0);
        visit(self, self.entry, &mut marks)?;
        if marks.contains(&0) {
            return Err("unreachable block".into());
        }
        // Indexed storage gives unique IDs and one terminator per block. Cycles
        // are legal; structural validity makes no claim that Return is reached.
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirProgram {
    functions: Vec<MirFunction>,
}
impl MirProgram {
    pub fn functions(&self) -> &[MirFunction] {
        &self.functions
    }
}

/// Lower canonical ordinary HIR after frontend/ownership checks. Verified
/// functions retain their specialized proof path. Unsupported valid HIR is a
/// compiler defect, never a new user diagnostic phase.
pub fn lower(program: &hir::Program) -> MirProgram {
    // Public source HIR cannot contain the private trusted catalog. Synthetic
    // preparation uses the fallible lower_with_origins entry below.
    lower_internal(program, true, &[], Vec::new())
        .expect("MIR metadata allocation failed")
        .0
}

/// Attach only the successful checker's immutable cleanup realization evidence.
/// This performs no ownership or effect inference.
pub(crate) fn attach_cleanup_effects(
    program: &mut MirProgram,
    sites: &[crate::effects::custody::Site],
) -> Result<(), hir::resources::PlanError> {
    if sites
        .iter()
        .any(|s| !program.functions.iter().any(|f| f.function == s.function))
    {
        return Err(hir::resources::PlanError::Integrity(
            "foreign cleanup effect witness function",
        ));
    }
    for function in &mut program.functions {
        let count = sites
            .iter()
            .filter(|s| s.function == function.function)
            .count();
        function
            .cleanup_effect_sites
            .try_reserve_exact(count)
            .map_err(|_| {
                hir::resources::PlanError::Host("MIR cleanup effect witness allocation")
            })?;
        function.cleanup_effect_sites.extend(
            sites
                .iter()
                .filter(|s| s.function == function.function)
                .cloned(),
        );
    }
    Ok(())
}

/// Produced by the same lowering, never reconstructed by the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginKind {
    ResultMatch,
    ResultPropagation,
    ResultPropagationReturn,
    ResultPayloadTransfer,
    TemporaryRegion,
    TemporaryCreation,
    TemporaryTransfer,
    TemporaryCleanup,
    TemporaryRetirement,
    DestinationPublication,
    ScopeEntry,
    OwnerInitialization,
    Replacement,
    ScopeExit,
    Statement,
    Initializer,
    If,
    While,
    FunctionBoundary,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Origin {
    pub span: Span,
    pub kind: OriginKind,
}
pub(crate) fn lower_with_origins(
    program: &hir::Program,
) -> Result<(MirProgram, Vec<Vec<Origin>>), hir::resources::PlanError> {
    let plans = if program.resources.leaves.is_empty() {
        Vec::new()
    } else {
        program.resources.plans(program)?
    };
    let mut flows = Vec::new();
    for function in program
        .functions()
        .iter()
        .filter_map(|f| f.as_ordinary())
        .filter(|_| plans.iter().any(Option::is_some) || !program.result_types().is_empty())
    {
        #[cfg(test)]
        temporaries::allocation()?;
        flows
            .try_reserve_exact(1)
            .map_err(|_| hir::resources::PlanError::Host("temporary function allocation"))?;
        flows.push(temporaries::Flow::lower(program, &function.body, &plans)?);
    }
    lower_internal(program, false, &plans, flows)
}
fn lower_internal(
    program: &hir::Program,
    legacy_assert: bool,
    plans: &[Option<hir::resources::CleanupPlan>],
    flows: Vec<Option<temporaries::Flow>>,
) -> Result<(MirProgram, Vec<Vec<Origin>>), hir::resources::PlanError> {
    let result_plans = if program.result_types().is_empty() {
        Vec::new()
    } else {
        let record_effects = program.resources.summaries(program, plans)?;
        program.variant_plans(plans, &record_effects)?
    };
    fn replacements(body: &[ValueStatement]) -> bool {
        body.iter().any(|s| match s {
            ValueStatement::Assign { value, .. } => matches!(value.ty, hir::ExprType::Record(_)),
            ValueStatement::If {
                then_body,
                else_body,
                ..
            } => replacements(then_body) || replacements(else_body),
            ValueStatement::While { body, .. } => replacements(body),
            ValueStatement::Match { arms, .. } => arms.iter().any(|a| replacements(&a.body)),
            _ => false,
        })
    }
    let mut flows = flows.into_iter();
    let pairs: Vec<_> = program
        .functions()
        .iter()
        .filter_map(|f| f.as_ordinary())
        .map(|f| {
            let bearing = |ty| match ty {
                hir::ValueType::Record(id) => plans.get(id.0).is_some_and(Option::is_some),
                hir::ValueType::Resource(_) => true,
                hir::ValueType::Result(id) => result_plans.get(id.0).is_some_and(hir::results::VariantPlan::bearing),
                _ => false,
            };
            let resource_locals:Vec<_> = program.locals.iter().filter(|l| l.function==f.id && bearing(l.ty)).map(|l|l.id).collect();
            let parameters:Vec<_> = f.parameters.iter().rev().copied().filter(|id| matches!(program.parameter(*id).ty,hir::ParameterType::Value(ty) if bearing(ty))).collect();
            let protected_return=f.return_type.is_some_and(bearing);
            let record_locals = if replacements(&f.body) { program.locals.iter().filter(|l| l.function == f.id && matches!(l.ty, hir::ValueType::Record(_))).map(|l| l.id).collect() } else { resource_locals.clone() };
            let cleanup=(!resource_locals.is_empty() || !parameters.is_empty() || protected_return || replacements(&f.body)).then_some(cleanup::Flow {parameters,protected_return,..cleanup::Flow::default()});
            let mut builder = Builder {temporaries: flows.next().flatten(), next_evaluation: 0, next_scope: 0, blocks:Vec::new(),loop_targets:Vec::new(),origins:Vec::new(),cleanup,resource_locals,record_locals,active_scopes:Vec::new()};
            let entry = builder.reserve();
            if let Some((tail, statements)) = builder.body(entry, &f.body, f.span) {
                assert!(
                    f.return_type.is_none(),
                    "value functions have no closing-brace fallthrough"
                );
                builder.finish(
                    tail,
                    statements,
                    Terminator::ReturnNoValue,
                    Origin {
                        span: f.span,
                        kind: OriginKind::FunctionBoundary,
                    },
                );
            }
            let function = MirFunction {
                cleanup_effect_sites: Vec::new(),
                direct_effects: crate::effects::calls::sites(program, f)?,
                effects: f.effects.clone(),
                temporaries: builder.temporaries,
                cleanup: builder.cleanup,
                function: f.id,
                entry,
                blocks: builder
                    .blocks
                    .into_iter()
                    .map(|b| b.expect("all reserved blocks finished"))
                    .collect(),
            };
            if legacy_assert {
                debug_assert_eq!(function.validate(), Ok(()));
            }
            Ok((
                function,
                builder
                    .origins
                    .into_iter()
                    .map(|o| o.expect("lowering origin finished"))
                    .collect::<Vec<_>>(),
            ))
        })
        .collect::<Result<_,hir::resources::PlanError>>()?;
    let (functions, origins) = pairs.into_iter().unzip();
    Ok((MirProgram { functions }, origins))
}
fn value_statement_span(statement: &ValueStatement) -> Span {
    match statement {
        ValueStatement::ReturnNoValue { span }
        | ValueStatement::Break { span }
        | ValueStatement::Continue { span }
        | ValueStatement::CallNoValue { span, .. }
        | ValueStatement::Let { span, .. }
        | ValueStatement::Return { span, .. }
        | ValueStatement::Assign { span, .. }
        | ValueStatement::DerefAssign { span, .. }
        | ValueStatement::CopyFieldAssign { span, .. }
        | ValueStatement::CopyReferenceFieldAssign { span, .. }
        | ValueStatement::If { span, .. }
        | ValueStatement::Match { span, .. }
        | ValueStatement::While { span, .. } => *span,
    }
}
#[derive(Clone, Copy)]
struct InitializerDestination {
    local: LocalId,
    span: Span,
    join: BasicBlockId,
}

struct Builder {
    temporaries: Option<temporaries::Flow>,
    next_evaluation: usize,
    next_scope: usize,
    blocks: Vec<Option<BasicBlock>>,
    loop_targets: Vec<(BasicBlockId, BasicBlockId, usize)>,
    cleanup: Option<cleanup::Flow>,
    active_scopes: Vec<cleanup::ScopeId>,
    resource_locals: Vec<LocalId>,
    record_locals: Vec<LocalId>,
    origins: Vec<Option<Origin>>,
}
impl Builder {
    fn temporary_region(&mut self, statement: &ValueStatement) -> Option<temporaries::RegionId> {
        if matches!(
            statement,
            ValueStatement::ReturnNoValue { .. }
                | ValueStatement::Break { .. }
                | ValueStatement::Continue { .. }
        ) {
            return None;
        }
        let evaluation = self.next_evaluation;
        self.next_evaluation += 1;
        self.temporaries
            .as_ref()?
            .regions
            .iter()
            .find(|r| r.evaluation == Some(evaluation))
            .map(|r| r.id)
    }
    fn temporary_point(
        &mut self,
        block: BasicBlockId,
        statement: usize,
        region: Option<temporaries::RegionId>,
        site: Option<temporaries::SiteId>,
        begin: bool,
        complete: bool,
    ) {
        if let (Some(flow), Some(region)) = (&mut self.temporaries, region) {
            assert!(
                flow.points.len() < flow.points.capacity(),
                "reserved source points"
            );
            flow.points.push(temporaries::Point {
                block,
                statement,
                region,
                site,
                begin,
                complete,
            });
        }
    }
    fn reserve(&mut self) -> BasicBlockId {
        let id = BasicBlockId(self.blocks.len());
        self.blocks.push(None);
        self.origins.push(None);
        if let Some(flow) = &mut self.cleanup {
            flow.blocks.push(cleanup::Block::default());
        }
        id
    }
    fn finish(
        &mut self,
        id: BasicBlockId,
        statements: Vec<Statement>,
        terminator: Terminator,
        origin: Origin,
    ) {
        self.origins[id.0] = Some(origin);
        assert!(self.blocks[id.0].is_none(), "block finished once");
        self.blocks[id.0] = Some(BasicBlock {
            statements,
            terminator,
        });
    }
    // Each value leaf initializes the canonical destination once on its path.
    // Nested value branches share the destination's control-flow-only join.
    fn initialize(
        &mut self,
        current: BasicBlockId,
        mut statements: Vec<Statement>,
        value: &TypedExpr,
        target: InitializerDestination,
        region_site: Option<(temporaries::RegionId, temporaries::SiteId)>,
        begin: bool,
    ) {
        let InitializerDestination { local, span, join } = target;
        let origin = Origin {
            span,
            kind: OriginKind::Initializer,
        };
        if let hir::ExprKind::IfValue {
            condition,
            then_value,
            else_value,
        } = &value.kind
        {
            let then_target = self.reserve();
            let else_target = self.reserve();
            let child = |index| {
                region_site.map(|(r, s)| {
                    (
                        r,
                        self.temporaries.as_ref().unwrap().sites[s.0].children[index],
                    )
                })
            };
            let condition_site = child(0);
            let then_site = child(1);
            let else_site = child(2);
            self.temporary_point(
                current,
                statements.len(),
                region_site.map(|p| p.0),
                condition_site.map(|p| p.1),
                begin,
                false,
            );
            self.finish(
                current,
                statements,
                Terminator::Branch {
                    condition: (**condition).clone(),
                    then_target,
                    else_target,
                },
                origin,
            );
            self.initialize(
                then_target,
                Vec::new(),
                then_value,
                target,
                then_site,
                false,
            );
            self.initialize(
                else_target,
                Vec::new(),
                else_value,
                target,
                else_site,
                false,
            );
        } else {
            self.temporary_point(
                current,
                statements.len(),
                region_site.map(|p| p.0),
                region_site.map(|p| p.1),
                begin,
                true,
            );
            self.initialization(current, statements.len(), local, span);
            statements.push(Statement::Let {
                local,
                initializer: value.clone(),
                span,
            });
            self.finish(
                current,
                statements,
                Terminator::Goto { target: join },
                origin,
            );
        }
    }
    fn body(
        &mut self,
        current: BasicBlockId,
        body: &[ValueStatement],
        scope_span: Span,
    ) -> Option<(BasicBlockId, Vec<Statement>)> {
        self.body_with_binding(current, body, scope_span, None)
    }
    fn body_with_binding(
        &mut self,
        current: BasicBlockId,
        body: &[ValueStatement],
        scope_span: Span,
        binding: Option<Statement>,
    ) -> Option<(BasicBlockId, Vec<Statement>)> {
        let lexical_scope = cleanup::ScopeId(self.next_scope);
        self.next_scope += 1;
        let scope = self.cleanup.as_mut().map(|flow| {
            let id = lexical_scope;
            flow.scopes.push(cleanup::Scope {
                id,
                span: scope_span,
                parent: self.active_scopes.last().copied(),
                owners: Vec::new(),
            });
            let span = scope_span;
            flow.blocks[current.0].enter = Some((
                id,
                Origin {
                    span,
                    kind: OriginKind::ScopeEntry,
                },
            ));
            id
        });
        self.active_scopes.push(lexical_scope);
        let mut prefix = Vec::new();
        if let Some(binding) = binding {
            if let Statement::BindResult {
                local,
                pattern_origin,
                ..
            } = &binding
            {
                self.initialization(current, 0, *local, *pattern_origin);
            }
            prefix.push(binding);
        }
        let result = self.body_inner(current, body, prefix);
        if let Some(scope) = scope {
            if let Some((tail, _)) = &result {
                let flow = self.cleanup.as_mut().unwrap();
                flow.blocks[tail.0].exit.push(scope);
                flow.blocks[tail.0].exit_origin = Some(Origin {
                    span: body.last().map(value_statement_span).unwrap_or(scope_span),
                    kind: OriginKind::ScopeExit,
                });
            }
        }
        assert_eq!(self.active_scopes.pop(), Some(lexical_scope));
        result
    }
    fn exiting(&mut self, current: BasicBlockId, depth: usize, span: Span) {
        if let Some(flow) = &mut self.cleanup {
            flow.blocks[current.0].exit =
                self.active_scopes[depth..].iter().rev().copied().collect();
            flow.blocks[current.0].exit_origin = Some(Origin {
                span,
                kind: OriginKind::ScopeExit,
            });
        }
    }
    fn initialization(
        &mut self,
        current: BasicBlockId,
        statement: usize,
        local: LocalId,
        span: Span,
    ) {
        if !self.record_locals.contains(&local) {
            return;
        }
        if let Some(flow) = &mut self.cleanup {
            let scope = *self.active_scopes.last().expect("lexical initialization");
            if !flow.scopes[scope.0].owners.contains(&local) {
                flow.scopes[scope.0].owners.push(local);
            }
            if !self.resource_locals.contains(&local) {
                return;
            }
            flow.blocks[current.0]
                .initialize
                .push(cleanup::Initialization {
                    statement,
                    local,
                    scope,
                    origin: Origin {
                        span,
                        kind: OriginKind::OwnerInitialization,
                    },
                });
        }
    }
    fn body_inner(
        &mut self,
        mut current: BasicBlockId,
        body: &[ValueStatement],
        mut statements: Vec<Statement>,
    ) -> Option<(BasicBlockId, Vec<Statement>)> {
        for statement in body {
            let span = match statement {
                ValueStatement::ReturnNoValue { span }
                | ValueStatement::CallNoValue { span, .. }
                | ValueStatement::Break { span }
                | ValueStatement::Continue { span }
                | ValueStatement::Let { span, .. }
                | ValueStatement::CopyReferenceFieldAssign { span, .. }
                | ValueStatement::CopyFieldAssign { span, .. }
                | ValueStatement::Assign { span, .. }
                | ValueStatement::DerefAssign { span, .. }
                | ValueStatement::While { span, .. }
                | ValueStatement::If { span, .. }
                | ValueStatement::Match { span, .. }
                | ValueStatement::Return { span, .. } => *span,
            };
            let origin = Origin {
                span,
                kind: OriginKind::Statement,
            };
            let region = self.temporary_region(statement);
            let root = region.and_then(|r| {
                self.temporaries.as_ref().unwrap().regions[r.0]
                    .roots
                    .first()
                    .copied()
            });
            if !matches!(
                statement,
                ValueStatement::Let {
                    initializer: TypedExpr {
                        kind: hir::ExprKind::IfValue { .. },
                        ..
                    },
                    ..
                } | ValueStatement::If { .. }
                    | ValueStatement::While { .. }
            ) {
                self.temporary_point(
                    current,
                    statements.len(),
                    region,
                    if matches!(statement, ValueStatement::CallNoValue { .. }) {
                        None
                    } else {
                        root
                    },
                    true,
                    true,
                );
            }
            match statement {
                ValueStatement::Match {
                    scrutinee, arms, ..
                } => {
                    let hir::ExprType::Result(result) = scrutinee.ty else {
                        unreachable!("checked Result match");
                    };
                    let targets = [self.reserve(), self.reserve()];
                    let join = arms
                        .iter()
                        .any(|a| falls_through(&a.body))
                        .then(|| self.reserve());
                    let ok = arms
                        .iter()
                        .position(|a| a.tag == hir::ResultTag::Ok)
                        .expect("checked Ok arm");
                    let error = 1 - ok;
                    let origin = Origin {
                        span,
                        kind: OriginKind::ResultMatch,
                    };
                    self.finish(
                        current,
                        std::mem::take(&mut statements),
                        Terminator::Match {
                            result,
                            scrutinee: scrutinee.clone(),
                            ok_target: targets[ok],
                            error_target: targets[error],
                        },
                        origin,
                    );
                    for (arm, target) in arms.iter().zip(targets) {
                        let binding = Statement::BindResult {
                            result,
                            tag: arm.tag,
                            local: arm.binding,
                            match_origin: span,
                            pattern_origin: arm.pattern_span,
                        };
                        if let Some((end, body)) =
                            self.body_with_binding(target, &arm.body, arm.span, Some(binding))
                        {
                            self.finish(
                                end,
                                body,
                                Terminator::Goto {
                                    target: join.expect("match arm fallthrough"),
                                },
                                origin,
                            );
                        }
                    }
                    current = join?;
                }
                ValueStatement::ReturnNoValue { .. } => {
                    self.exiting(current, 0, span);
                    self.finish(current, statements, Terminator::ReturnNoValue, origin);
                    return None;
                }
                ValueStatement::CallNoValue {
                    function,
                    arguments,
                    span,
                } => {
                    statements.push(Statement::CallNoValue {
                        function: *function,
                        arguments: arguments.clone(),
                        span: *span,
                    });
                }
                ValueStatement::Break { .. } => {
                    let (_, target, depth) =
                        *self.loop_targets.last().expect("checked innermost loop");
                    self.exiting(current, depth, span);
                    self.finish(current, statements, Terminator::Goto { target }, origin);
                    return None;
                }
                ValueStatement::Continue { .. } => {
                    let (target, _, depth) =
                        *self.loop_targets.last().expect("checked innermost loop");
                    self.exiting(current, depth, span);
                    self.finish(current, statements, Terminator::Goto { target }, origin);
                    return None;
                }
                ValueStatement::Let {
                    local,
                    initializer,
                    span,
                    propagation,
                } => {
                    if let Some(operation) = propagation {
                        let success = self.reserve();
                        let failure = self.reserve();
                        let owning_scope =
                            self.active_scopes.last().expect("actual declaring scope").0;
                        self.finish(
                            current,
                            std::mem::take(&mut statements),
                            Terminator::Propagate {
                                operation: *operation,
                                local: *local,
                                owning_scope,
                                initializer: initializer.clone(),
                                span: *span,
                                ok_target: success,
                                error_target: failure,
                            },
                            Origin {
                                span: *span,
                                kind: OriginKind::ResultPropagation,
                            },
                        );
                        self.exiting(failure, 0, operation.syntax.phrase);
                        self.finish(
                            failure,
                            Vec::new(),
                            Terminator::ForwardError {
                                operation: *operation,
                            },
                            Origin {
                                span: operation.syntax.phrase,
                                kind: OriginKind::ResultPropagationReturn,
                            },
                        );
                        current = success;
                        self.initialization(current, 0, *local, operation.syntax.binding);
                        statements.push(Statement::BindResult {
                            result: operation.inner,
                            tag: hir::ResultTag::Ok,
                            local: *local,
                            match_origin: *span,
                            pattern_origin: operation.syntax.binding,
                        });
                    } else if matches!(initializer.kind, hir::ExprKind::IfValue { .. }) {
                        let join = self.reserve();
                        self.initialize(
                            current,
                            std::mem::take(&mut statements),
                            initializer,
                            InitializerDestination {
                                local: *local,
                                span: *span,
                                join,
                            },
                            region.zip(root),
                            true,
                        );
                        current = join;
                    } else {
                        self.initialization(current, statements.len(), *local, *span);
                        statements.push(Statement::Let {
                            local: *local,
                            initializer: initializer.clone(),
                            span: *span,
                        });
                    }
                }
                ValueStatement::CopyReferenceFieldAssign {
                    reference,
                    record,
                    field,
                    value,
                    reference_span,
                    field_span,
                    span,
                } => {
                    statements.push(Statement::CopyReferenceFieldAssign {
                        reference: *reference,
                        record: *record,
                        field: *field,
                        value: value.clone(),
                        reference_span: *reference_span,
                        field_span: *field_span,
                        span: *span,
                    });
                }
                ValueStatement::CopyFieldAssign {
                    owner,
                    record,
                    field,
                    owner_span,
                    field_span,
                    value,
                    span,
                } => {
                    statements.push(Statement::CopyFieldAssign {
                        owner: *owner,
                        record: *record,
                        field: *field,
                        owner_span: *owner_span,
                        field_span: *field_span,
                        value: value.clone(),
                        span: *span,
                    });
                }
                ValueStatement::Assign { local, value, span } => {
                    if let hir::ExprType::Record(record) = value.ty {
                        let owning_scope = self
                            .cleanup
                            .as_ref()
                            .unwrap()
                            .scopes
                            .iter()
                            .find(|s| s.owners.contains(local))
                            .expect("declared replacement target")
                            .id
                            .0;
                        statements.push(Statement::ReplaceOwned {
                            local: *local,
                            record,
                            owning_scope,
                            cleanup_plan: self.resource_locals.contains(local).then_some(record),
                            value: value.clone(),
                            origin: Origin {
                                span: *span,
                                kind: OriginKind::Replacement,
                            },
                            span: *span,
                        });
                    } else {
                        statements.push(Statement::Assign {
                            local: *local,
                            value: value.clone(),
                            span: *span,
                        });
                    }
                }
                ValueStatement::DerefAssign {
                    reference,
                    value,
                    span,
                } => statements.push(Statement::DerefAssign {
                    reference: *reference,
                    value: value.clone(),
                    span: *span,
                }),
                ValueStatement::While {
                    condition, body, ..
                } => {
                    let origin = Origin {
                        span,
                        kind: OriginKind::While,
                    };
                    let header = self.reserve();
                    let body_target = self.reserve();
                    let exit = self.reserve();
                    self.finish(
                        current,
                        std::mem::take(&mut statements),
                        Terminator::Goto { target: header },
                        origin,
                    );
                    self.temporary_point(header, 0, region, root, true, true);
                    self.finish(
                        header,
                        Vec::new(),
                        Terminator::Branch {
                            condition: condition.clone(),
                            then_target: body_target,
                            else_target: exit,
                        },
                        origin,
                    );
                    self.loop_targets
                        .push((header, exit, self.active_scopes.len()));
                    if let Some((end, body)) = self.body(body_target, body, span) {
                        self.finish(end, body, Terminator::Goto { target: header }, origin);
                    }
                    assert_eq!(
                        self.loop_targets.pop().map(|(h, e, _)| (h, e)),
                        Some((header, exit))
                    );
                    current = exit;
                }
                ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    let origin = Origin {
                        span,
                        kind: OriginKind::If,
                    };
                    self.temporary_point(current, statements.len(), region, root, true, true);
                    let then_target = self.reserve();
                    let else_target = if else_body.is_empty() {
                        None
                    } else {
                        Some(self.reserve())
                    };
                    let join = if falls_through(then_body) || falls_through(else_body) {
                        Some(self.reserve())
                    } else {
                        None
                    };
                    self.finish(
                        current,
                        std::mem::take(&mut statements),
                        Terminator::Branch {
                            condition: condition.clone(),
                            then_target,
                            else_target: else_target.or(join).expect("false edge"),
                        },
                        origin,
                    );
                    if let Some((end, body)) = self.body(then_target, then_body, span) {
                        self.finish(
                            end,
                            body,
                            Terminator::Goto {
                                target: join.expect("then fallthrough"),
                            },
                            origin,
                        );
                    }
                    if let Some(target) = else_target {
                        if let Some((end, body)) = self.body(target, else_body, span) {
                            self.finish(
                                end,
                                body,
                                Terminator::Goto {
                                    target: join.expect("else fallthrough"),
                                },
                                origin,
                            );
                        }
                    }
                    let join = join?;
                    current = join;
                }
                ValueStatement::Return { value, .. } => {
                    self.exiting(current, 0, span);
                    self.finish(
                        current,
                        statements,
                        Terminator::Return {
                            value: value.clone(),
                        },
                        origin,
                    );
                    return None;
                }
            }
        }
        Some((current, statements))
    }
}

// Structural source fallthrough only; while retains its real false exit.
fn falls_through(body: &[ValueStatement]) -> bool {
    match body.last() {
        Some(
            ValueStatement::Continue { .. }
            | ValueStatement::Break { .. }
            | ValueStatement::Return { .. }
            | ValueStatement::ReturnNoValue { .. },
        ) => false,
        Some(ValueStatement::Match { arms, .. }) => arms.iter().any(|a| falls_through(&a.body)),
        Some(ValueStatement::If {
            then_body,
            else_body,
            ..
        }) => falls_through(then_body) || falls_through(else_body),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile_source, ownership, parse_source, resolve, types};
    #[test]
    fn assignment_identity_shape_and_loan_outcomes_ignore_metadata() {
        let source = "type Percent = range 0..100; fn next(value: Percent) -> Percent { return value; } fn f(flag: bool, value: Percent) -> Percent { let mut current = if flag { value } else { value }; let view = &current; if flag { let observed = *view; } else { current = next(current); } return current; }";
        let mut program = compile_source(source).unwrap();
        let original = lower(&program);
        for range in &mut program.ranges {
            range.name = "display".into();
        }
        for parameter in &mut program.parameters {
            parameter.name = "display".into();
        }
        for local in &mut program.locals {
            local.name = "display".into();
        }
        for function in &mut program.functions {
            if let hir::Function::Ordinary(f) = function {
                f.name = "display".into();
            }
        }
        ownership::check(&program).unwrap();
        assert_eq!(original, lower(&program));
        let source = source.replace("return current;", "return *view;");
        let mut program =
            types::check(resolve::resolve(&parse_source(&source).unwrap()).unwrap()).unwrap();
        let original = ownership::check(&program).unwrap_err();
        for local in &mut program.locals {
            local.name = "display".into();
        }
        let renamed = ownership::check(&program).unwrap_err();
        assert_eq!(original[0].span, renamed[0].span);
        assert!(renamed[0].message.contains("cannot assign"));
    }
    #[test]
    fn validator_rejects_hidden_if_value_in_assignment_rhs() {
        let program = compile_source("fn f(flag: bool) -> bool { let mut value = if flag { true } else { false }; value = true; return value; }").unwrap();
        let hir::ValueStatement::Let { initializer, .. } =
            &program.functions()[0].as_ordinary().unwrap().body[0]
        else {
            panic!()
        };
        let mut mir = lower(&program);
        for block in &mut mir.functions[0].blocks {
            for statement in &mut block.statements {
                if let Statement::Assign { value, .. } = statement {
                    *value = initializer.clone();
                }
            }
        }
        assert_eq!(
            mir.functions[0].validate(),
            Err("conditional value must lower to control flow")
        );
    }

    #[test]
    fn conditional_value_identity_and_ownership_ignore_display_names() {
        let source = "type Percent = range 0..100; record Ticket { value: Percent } fn id(a: Ticket) -> Ticket { return a; } fn f(flag: bool, a: Ticket, b: Ticket) -> Ticket { let chosen = if flag { id(a) } else { if flag { b } else { a } }; return chosen; }";
        let mut p = compile_source(source).unwrap();
        let original = lower(&p);
        for range in &mut p.ranges {
            range.name = "display".into();
        }
        for record in &mut p.records {
            record.name = "display".into();
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
            if let hir::Function::Ordinary(f) = function {
                f.name = "display".into();
            }
        }
        ownership::check(&p).unwrap();
        assert_eq!(original, lower(&p));
        let failed = source.replace("return chosen;", "return a;");
        let mut p =
            types::check(resolve::resolve(&parse_source(&failed).unwrap()).unwrap()).unwrap();
        let original = ownership::check(&p).unwrap_err();
        for parameter in &mut p.parameters {
            parameter.name = "display".into();
        }
        for local in &mut p.locals {
            local.name = "display".into();
        }
        let renamed = ownership::check(&p).unwrap_err();
        assert_eq!(original[0].span, renamed[0].span);
        assert!(original[0].message.contains("may have been moved"));
        assert!(renamed[0].message.contains("may have been moved"));
    }
    #[test]
    fn validator_rejects_hidden_conditional_value() {
        let p = compile_source("fn f(flag: bool) -> bool { let chosen = if flag { true } else { false }; return chosen; }").unwrap();
        let hir::ValueStatement::Let { initializer, .. } =
            &p.functions()[0].as_ordinary().unwrap().body[0]
        else {
            panic!()
        };
        let mut m = lower(&p);
        let f = &mut m.functions[0];
        let statement = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.statements)
            .next()
            .unwrap();
        let Statement::Let {
            initializer: value, ..
        } = statement
        else {
            panic!()
        };
        *value = initializer.clone();
        assert_eq!(
            f.validate(),
            Err("conditional value must lower to control flow")
        );
    }
    fn program() -> hir::Program {
        compile_source("type Percent = range 0..100; record Ticket { value: Percent } fn identity(value: Percent) -> Percent { return value; } fn example(flag: bool, value: Percent) -> Percent { if flag { let observed = identity(value); if flag { let copied = observed; } } else { let observed = value; } return value; }").unwrap()
    }
    #[test]
    fn canonical_ids_and_cfg_ignore_colliding_display_names() {
        let mut p = program();
        let original = lower(&p);
        for r in &mut p.ranges {
            r.name = "display".into();
        }
        for r in &mut p.records {
            r.name = "display".into();
        }
        for f in &mut p.fields {
            f.name = "display".into();
        }
        for parameter in &mut p.parameters {
            parameter.name = "display".into();
        }
        for local in &mut p.locals {
            local.name = "display".into();
        }
        for f in &mut p.functions {
            if let hir::Function::Ordinary(f) = f {
                f.name = "display".into();
            }
        }
        ownership::check(&p).unwrap();
        assert_eq!(original, lower(&p));
        assert_ne!(p.locals[0].id, p.locals[2].id);
        let f = &original.functions()[1];
        assert_eq!(f.function(), p.functions[1].id());
        let call = f
            .blocks()
            .flat_map(|(_, b)| b.statements())
            .find_map(|s| match s {
                Statement::Let {
                    initializer:
                        TypedExpr {
                            kind: hir::ExprKind::Call { function, .. },
                            ..
                        },
                    ..
                } => Some(*function),
                _ => None,
            })
            .unwrap();
        assert_eq!(call, p.functions[0].id());
    }
    #[test]
    fn conditional_move_diagnostic_outcome_ignores_names() {
        let ast = parse_source("type Percent = range 0..100; record Ticket { value: Percent } fn bad(flag: bool, ticket: Ticket) -> Ticket { if flag { let moved = ticket; } return ticket; }").unwrap();
        let mut p = types::check(resolve::resolve(&ast).unwrap()).unwrap();
        let before = ownership::check(&p).unwrap_err();
        for parameter in &mut p.parameters {
            parameter.name = "display".into();
        }
        for local in &mut p.locals {
            local.name = "display".into();
        }
        let after = ownership::check(&p).unwrap_err();
        assert_eq!(before[0].span, after[0].span);
        assert_eq!(before[0].known, after[0].known);
        assert!(after[0].message.contains("`display` may have been moved"));
    }
    #[test]
    fn validator_rejects_invalid_entry_target_and_unreachable_block() {
        let original = lower(&program()).functions()[1].clone();
        let mut f = original.clone();
        f.entry = BasicBlockId(f.blocks.len());
        assert_eq!(f.validate(), Err("invalid block target"));
        let mut f = original.clone();
        f.blocks[0].terminator = Terminator::Goto {
            target: BasicBlockId(f.blocks.len()),
        };
        assert_eq!(f.validate(), Err("invalid block target"));
        let mut f = original.clone();
        f.blocks.push(original.blocks.last().unwrap().clone());
        assert_eq!(f.validate(), Err("unreachable block"));
    }
    #[test]
    fn validator_rejects_non_boolean_branch_conditions() {
        let mut f = lower(&program()).functions()[1].clone();
        let Terminator::Branch { condition, .. } = &mut f.blocks[0].terminator else {
            panic!()
        };
        condition.ty = hir::ExprType::IntegerLiteral;
        assert_eq!(f.validate(), Err("non-Bool branch condition"));
    }
    #[test]
    fn validator_accepts_cycles_without_claiming_termination() {
        let p = compile_source("fn f() -> bool { return true; }").unwrap();
        let mut f = lower(&p).functions()[0].clone();
        f.blocks[0].terminator = Terminator::Goto { target: f.entry };
        assert_eq!(f.validate(), Ok(())); // No Return is required for structural validity.
        f.blocks[0].terminator = Terminator::Goto {
            target: BasicBlockId(1),
        };
        assert_eq!(f.validate(), Err("invalid block target"));
    }
    #[test]
    fn validator_checks_structure_of_nested_cyclic_cfgs() {
        let p = compile_source("fn f(flag: bool) -> bool { let mut current = flag; while current { while flag { current = flag; } current = false; } return current; }").unwrap();
        let original = lower(&p).functions()[0].clone();
        assert_eq!(original.validate(), Ok(()));
        let mut f = original.clone();
        f.blocks.push(original.blocks.last().unwrap().clone());
        assert_eq!(f.validate(), Err("unreachable block"));
        let mut f = original;
        let branch = f
            .blocks
            .iter_mut()
            .find_map(|b| match &mut b.terminator {
                Terminator::Branch { condition, .. } => Some(condition),
                _ => None,
            })
            .unwrap();
        branch.ty = hir::ExprType::IntegerLiteral;
        assert_eq!(f.validate(), Err("non-Bool branch condition"));
    }

    #[test]
    fn validator_inspects_each_no_value_argument_tree_for_hidden_conditional_values() {
        let p=compile_source("fn consume(first: bool, second: bool) {} fn f(flag: bool) { let chosen = if flag { true } else { false }; consume(flag, chosen); }").unwrap();
        let hir::ValueStatement::Let { initializer, .. } =
            &p.functions()[1].as_ordinary().unwrap().body[0]
        else {
            panic!()
        };
        for position in 0..2 {
            let mut m = lower(&p);
            let f = &mut m.functions[1];
            let statement = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.statements)
                .find(|s| matches!(s, Statement::CallNoValue { .. }))
                .unwrap();
            let Statement::CallNoValue { arguments, .. } = statement else {
                panic!()
            };
            arguments[position] = TypedExpr {
                kind: hir::ExprKind::Call {
                    function: p.functions()[0].id(),
                    arguments: vec![initializer.clone()],
                },
                ty: hir::ExprType::Bool,
                span: initializer.span,
            };
            assert_eq!(
                f.validate(),
                Err("conditional value must lower to control flow")
            );
        }
    }
    #[test]
    fn validator_no_value_completions_keep_target_reachability_and_bool_checks() {
        let p = compile_source("fn f(flag: bool) { if flag { return; } }").unwrap();
        let original = lower(&p);
        original.functions[0].validate().unwrap();
        let mut m = original.clone();
        m.functions[0].blocks[0].terminator = Terminator::Goto {
            target: BasicBlockId(999),
        };
        assert_eq!(m.functions[0].validate(), Err("invalid block target"));
        let mut m = original.clone();
        m.functions[0].blocks[0].terminator = Terminator::ReturnNoValue;
        assert_eq!(m.functions[0].validate(), Err("unreachable block"));
        let mut m = original;
        let Terminator::Branch { condition, .. } = &mut m.functions[0].blocks[0].terminator else {
            panic!()
        };
        condition.ty = hir::ExprType::IntegerLiteral;
        assert_eq!(m.functions[0].validate(), Err("non-Bool branch condition"));
    }

    #[test]
    fn validator_descends_construction_at_multiple_depths_in_every_mir_container() {
        let p=compile_source("type Percent = range 0..100; record Battery { charge: Percent } fn f(flag: bool,p: Percent) -> Battery { let chosen = if flag { p } else { p }; return Battery { charge: chosen }; }").unwrap();
        let f = p.functions()[0].as_ordinary().unwrap();
        let hir::ValueStatement::Let {
            initializer: hidden,
            local,
            span,
            ..
        } = &f.body[0]
        else {
            panic!()
        };
        let hir::ValueStatement::Return { value: record, .. } = &f.body[1] else {
            panic!()
        };
        for depth in 0..4 {
            let mut nested = hidden.clone();
            for _ in 0..depth {
                let mut construct = record.clone();
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut construct.kind else {
                    panic!()
                };
                fields[0].value = nested;
                nested = TypedExpr {
                    kind: hir::ExprKind::Call {
                        function: f.id,
                        arguments: vec![construct],
                    },
                    ..hidden.clone()
                };
            }
            let mut construct = record.clone();
            let hir::ExprKind::RecordConstruct { fields, .. } = &mut construct.kind else {
                panic!()
            };
            fields[0].value = nested;
            for statement in [
                Statement::Let {
                    local: *local,
                    initializer: construct.clone(),
                    span: *span,
                },
                Statement::Assign {
                    local: *local,
                    value: construct.clone(),
                    span: *span,
                },
                Statement::DerefAssign {
                    reference: Place::Local(*local),
                    value: construct.clone(),
                    span: *span,
                },
                Statement::CallNoValue {
                    function: f.id,
                    arguments: vec![construct.clone()],
                    span: *span,
                },
            ] {
                let m = MirFunction {
                    cleanup_effect_sites: Vec::new(),
                    direct_effects: Vec::new(),
                    effects: crate::effects::Declaration::default(),
                    temporaries: None,
                    cleanup: None,
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![statement],
                        terminator: Terminator::ReturnNoValue,
                    }],
                };
                assert_eq!(
                    m.validate(),
                    Err("conditional value must lower to control flow")
                );
            }
            for terminator in [
                Terminator::Return {
                    value: construct.clone(),
                },
                Terminator::Branch {
                    condition: TypedExpr {
                        ty: hir::ExprType::Bool,
                        ..construct.clone()
                    },
                    then_target: BasicBlockId(1),
                    else_target: BasicBlockId(1),
                },
            ] {
                let m = MirFunction {
                    cleanup_effect_sites: Vec::new(),
                    direct_effects: Vec::new(),
                    effects: crate::effects::Declaration::default(),
                    temporaries: None,
                    cleanup: None,
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![
                        BasicBlock {
                            statements: vec![],
                            terminator,
                        },
                        BasicBlock {
                            statements: vec![],
                            terminator: Terminator::ReturnNoValue,
                        },
                    ],
                };
                assert_eq!(
                    m.validate(),
                    Err("conditional value must lower to control flow")
                );
            }
        }
    }
    #[test]
    fn validator_rejects_residual_verified_projection_under_nested_construction() {
        let p=compile_source("type Percent = range 0..100; record Battery { charge: Percent } fn f(b: Battery) -> Battery { return Battery { charge: b.charge }; }").unwrap();
        let f = p.functions()[0].as_ordinary().unwrap();
        let hir::ValueStatement::Return { value: record, .. } = &f.body[0] else {
            panic!()
        };
        for old in [false, true] {
            let mut construct = record.clone();
            let hir::ExprKind::RecordConstruct { fields, .. } = &mut construct.kind else {
                panic!()
            };
            let access = hir::FieldAccess {
                parameter: f.parameters[0],
                field: p.fields()[0].id,
                ty: p.ranges()[0].id,
                span: fields[0].value.span,
            };
            fields[0].value.kind = if old {
                hir::ExprKind::OldField(access)
            } else {
                hir::ExprKind::FieldAccess(access)
            };
            let m = MirFunction {
                cleanup_effect_sites: Vec::new(),
                direct_effects: Vec::new(),
                effects: crate::effects::Declaration::default(),
                temporaries: None,
                cleanup: None,
                function: f.id,
                entry: BasicBlockId(0),
                blocks: vec![BasicBlock {
                    statements: vec![],
                    terminator: Terminator::Return { value: construct },
                }],
            };
            assert!(m.validate().unwrap_err().contains("residual projection"));
        }
    }
}

#[cfg(test)]
mod multiple_field_tests {
    use super::*;
    use crate::{compile_source, hir};
    #[test]
    fn mir_traverses_all_initializers_at_depth_in_every_expression_container() {
        let p=compile_source("type P = range 0..100; record Triple { a: P, b: P, c: P } fn f(flag: bool,p: P) -> Triple { let chosen = if flag { p } else { p }; return Triple { c: chosen, a: p, b: p }; }").unwrap();
        let f = p.functions()[0].as_ordinary().unwrap();
        let hir::ValueStatement::Let {
            local,
            initializer: hidden,
            span,
            ..
        } = &f.body[0]
        else {
            panic!()
        };
        let hir::ValueStatement::Return { value: record, .. } = &f.body[1] else {
            panic!()
        };
        for position in 0..3 {
            for depth in 0..3 {
                let mut nested = hidden.clone();
                for _ in 0..depth {
                    nested = TypedExpr {
                        kind: hir::ExprKind::Call {
                            function: f.id,
                            arguments: vec![nested],
                        },
                        ..hidden.clone()
                    };
                }
                let mut e = record.clone();
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut e.kind else {
                    panic!()
                };
                fields[position].value = nested;
                let statements = [
                    Statement::Let {
                        local: *local,
                        initializer: e.clone(),
                        span: *span,
                    },
                    Statement::Assign {
                        local: *local,
                        value: e.clone(),
                        span: *span,
                    },
                    Statement::DerefAssign {
                        reference: hir::Place::Local(*local),
                        value: e.clone(),
                        span: *span,
                    },
                    Statement::CallNoValue {
                        function: f.id,
                        arguments: vec![e.clone()],
                        span: *span,
                    },
                ];
                for statement in statements {
                    let m = MirFunction {
                        cleanup_effect_sites: Vec::new(),
                        direct_effects: Vec::new(),
                        effects: crate::effects::Declaration::default(),
                        temporaries: None,
                        cleanup: None,
                        function: f.id,
                        entry: BasicBlockId(0),
                        blocks: vec![BasicBlock {
                            statements: vec![statement],
                            terminator: Terminator::ReturnNoValue,
                        }],
                    };
                    assert_eq!(
                        m.validate(),
                        Err("conditional value must lower to control flow")
                    );
                }
                let m = MirFunction {
                    cleanup_effect_sites: Vec::new(),
                    direct_effects: Vec::new(),
                    effects: crate::effects::Declaration::default(),
                    temporaries: None,
                    cleanup: None,
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![],
                        terminator: Terminator::Return { value: e.clone() },
                    }],
                };
                assert_eq!(
                    m.validate(),
                    Err("conditional value must lower to control flow")
                );
                e.ty = hir::ExprType::Bool;
                let m = MirFunction {
                    cleanup_effect_sites: Vec::new(),
                    direct_effects: Vec::new(),
                    effects: crate::effects::Declaration::default(),
                    temporaries: None,
                    cleanup: None,
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![
                        BasicBlock {
                            statements: vec![],
                            terminator: Terminator::Branch {
                                condition: e,
                                then_target: BasicBlockId(1),
                                else_target: BasicBlockId(1),
                            },
                        },
                        BasicBlock {
                            statements: vec![],
                            terminator: Terminator::ReturnNoValue,
                        },
                    ],
                };
                assert_eq!(
                    m.validate(),
                    Err("conditional value must lower to control flow")
                );
            }
        }
    }
    #[test]
    fn mir_rejects_residual_projections_in_first_middle_and_final_children() {
        let p=compile_source("type P = range 0..100; record Triple { a: P, b: P, c: P } fn f(p: P) -> Triple { return Triple { c: p, a: p, b: p }; }").unwrap();
        let f = p.functions()[0].as_ordinary().unwrap();
        let hir::ValueStatement::Return { value: record, .. } = &f.body[0] else {
            panic!()
        };
        for position in 0..3 {
            for old in [false, true] {
                let mut e = record.clone();
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut e.kind else {
                    panic!()
                };
                let access = hir::FieldAccess {
                    parameter: f.parameters[0],
                    field: p.records()[0].fields[position],
                    ty: p.ranges()[0].id,
                    span: e.span,
                };
                fields[position].value.kind = if old {
                    hir::ExprKind::OldField(access)
                } else {
                    hir::ExprKind::FieldAccess(access)
                };
                let m = MirFunction {
                    cleanup_effect_sites: Vec::new(),
                    direct_effects: Vec::new(),
                    effects: crate::effects::Declaration::default(),
                    temporaries: None,
                    cleanup: None,
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![],
                        terminator: Terminator::Return { value: e },
                    }],
                };
                assert_eq!(m.validate(),Err("verified-state or residual projection representation is ineligible in ordinary MIR"));
            }
        }
    }
    #[test]
    fn literal_discriminator_is_preserved_and_validator_traverses_each_child() {
        let p=compile_source("type P = range 0..100; record R {a: P,b: P,c: P} fn id(p: P) -> P {return p;} fn f() -> R {return R {c: id(80),a: id(80),b: id(80)};}").unwrap();
        let f = p.functions().last().unwrap().as_ordinary().unwrap();
        let hir::ValueStatement::Return { value, .. } = &f.body[0] else {
            panic!()
        };
        let m = lower(&p);
        let lowered = m.functions().last().unwrap();
        assert_eq!(
            &lowered.blocks[0].terminator,
            &Terminator::Return {
                value: value.clone()
            }
        );
        lowered.validate().unwrap();
        for position in 0..3 {
            for raw in [false, true] {
                let mut e = value.clone();
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut e.kind else {
                    panic!()
                };
                let hir::ExprKind::Call { arguments, .. } = &mut fields[position].value.kind else {
                    panic!()
                };
                if raw {
                    arguments[0].kind = hir::ExprKind::IntegerLiteral(80);
                } else {
                    arguments[0].ty = hir::ExprType::Bool;
                }
                let m = MirFunction {
                    cleanup_effect_sites: Vec::new(),
                    direct_effects: Vec::new(),
                    effects: crate::effects::Declaration::default(),
                    temporaries: None,
                    cleanup: None,
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![],
                        terminator: Terminator::Return { value: e },
                    }],
                };
                assert_eq!(
                    m.validate(),
                    Err(if raw {
                        "raw integer literal has invalid type"
                    } else {
                        "formed range literal has non-range type"
                    })
                );
            }
        }
        // MIR has no canonical tables: shape-valid range tags are not bounds proofs.
        let e = hir::TypedExpr {
            kind: hir::ExprKind::FormedRangeLiteral(101),
            ty: hir::ExprType::Range(hir::RangeTypeId(usize::MAX)),
            span: value.span,
        };
        let m = MirFunction {
            cleanup_effect_sites: Vec::new(),
            direct_effects: Vec::new(),
            effects: crate::effects::Declaration::default(),
            temporaries: None,
            cleanup: None,
            function: f.id,
            entry: BasicBlockId(0),
            blocks: vec![BasicBlock {
                statements: vec![],
                terminator: Terminator::Return { value: e },
            }],
        };
        m.validate().unwrap();
    }

    #[test]
    fn raw_and_formed_validation_covers_expression_containers_and_all_mir_positions() {
        let p=compile_source("type P = range 0..100; fn id(p: P) -> P {return p;} fn f(p: P,r: &mut P) -> P {let mut x=p;return x;}").unwrap();
        let f = p.functions()[1].as_ordinary().unwrap();
        let span = f.span;
        for raw in [false, true] {
            let leaf = hir::TypedExpr {
                kind: if raw {
                    hir::ExprKind::IntegerLiteral(80)
                } else {
                    hir::ExprKind::FormedRangeLiteral(80)
                },
                ty: if raw {
                    hir::ExprType::Range(p.ranges()[0].id)
                } else {
                    hir::ExprType::IntegerLiteral
                },
                span,
            };
            let zero = hir::TypedExpr {
                kind: hir::ExprKind::IntegerLiteral(0),
                ty: hir::ExprType::IntegerLiteral,
                span,
            };
            let compare = hir::TypedExpr {
                kind: hir::ExprKind::Binary {
                    op: hir::BinaryOp::LessEqual,
                    left: Box::new(leaf.clone()),
                    right: Box::new(zero),
                },
                ty: hir::ExprType::Bool,
                span,
            };
            let unary = hir::TypedExpr {
                kind: hir::ExprKind::Unary {
                    op: hir::UnaryOp::Not,
                    operand: Box::new(compare.clone()),
                },
                ty: hir::ExprType::Bool,
                span,
            };
            let call = hir::TypedExpr {
                kind: hir::ExprKind::Call {
                    function: p.functions()[0].id(),
                    arguments: vec![leaf.clone()],
                },
                ty: hir::ExprType::Range(p.ranges()[0].id),
                span,
            };
            let error = if raw {
                "raw integer literal has invalid type"
            } else {
                "formed range literal has non-range type"
            };
            for e in [leaf, compare, unary, call] {
                for statement in [
                    Statement::Let {
                        local: p.locals()[0].id,
                        initializer: e.clone(),
                        span,
                    },
                    Statement::Assign {
                        local: p.locals()[0].id,
                        value: e.clone(),
                        span,
                    },
                    Statement::DerefAssign {
                        reference: hir::Place::Parameter(f.parameters[1]),
                        value: e.clone(),
                        span,
                    },
                    Statement::CallNoValue {
                        function: f.id,
                        arguments: vec![e.clone()],
                        span,
                    },
                ] {
                    let m = MirFunction {
                        cleanup_effect_sites: Vec::new(),
                        direct_effects: Vec::new(),
                        effects: crate::effects::Declaration::default(),
                        temporaries: None,
                        cleanup: None,
                        function: f.id,
                        entry: BasicBlockId(0),
                        blocks: vec![BasicBlock {
                            statements: vec![statement],
                            terminator: Terminator::ReturnNoValue,
                        }],
                    };
                    assert_eq!(m.validate(), Err(error));
                }
                let m = MirFunction {
                    cleanup_effect_sites: Vec::new(),
                    direct_effects: Vec::new(),
                    effects: crate::effects::Declaration::default(),
                    temporaries: None,
                    cleanup: None,
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![],
                        terminator: Terminator::Return { value: e.clone() },
                    }],
                };
                assert_eq!(m.validate(), Err(error));
                if e.ty == hir::ExprType::Bool {
                    let m = MirFunction {
                        cleanup_effect_sites: Vec::new(),
                        direct_effects: Vec::new(),
                        effects: crate::effects::Declaration::default(),
                        temporaries: None,
                        cleanup: None,
                        function: f.id,
                        entry: BasicBlockId(0),
                        blocks: vec![
                            BasicBlock {
                                statements: vec![],
                                terminator: Terminator::Branch {
                                    condition: e,
                                    then_target: BasicBlockId(1),
                                    else_target: BasicBlockId(1),
                                },
                            },
                            BasicBlock {
                                statements: vec![],
                                terminator: Terminator::ReturnNoValue,
                            },
                        ],
                    };
                    assert_eq!(m.validate(), Err(error));
                }
            }
        }
    }
    #[test]
    fn formed_type_validation_reaches_first_middle_final_call_arguments() {
        let p=compile_source("type P = range 0..100; fn tri(a: P,b: P,c: P) -> P {return a;} fn f() -> P {return tri(80,80,80);}").unwrap();
        let mut m = lower(&p);
        let f = m.functions.last_mut().unwrap();
        for position in 0..3 {
            let mut candidate = f.clone();
            let Terminator::Return { value } = &mut candidate.blocks[0].terminator else {
                panic!()
            };
            let hir::ExprKind::Call { arguments, .. } = &mut value.kind else {
                panic!()
            };
            arguments[position].ty = hir::ExprType::Bool;
            assert_eq!(
                candidate.validate(),
                Err("formed range literal has non-range type")
            );
        }
    }
    #[test]
    fn copy_field_write_mir_checks_rhs_shape_and_every_deep_child() {
        let p=crate::compile_source("type Percent = range 0..100; record Battery {charge: Percent} fn id(p: Percent) -> Percent {return p;} fn f(b: Battery) {let mut owned = b; owned.charge = id(80);}").unwrap();
        let original = lower(&p).functions.last().unwrap().clone();
        original.validate().unwrap();
        for fault in 0..5 {
            for position in 0..3 {
                let mut f = original.clone();
                let Statement::CopyFieldAssign { value, .. } = &mut f.blocks[0].statements[1]
                else {
                    panic!()
                };
                let mut leaf = value.clone();
                leaf.kind = match fault {
                    0 => hir::ExprKind::IfValue {
                        condition: Box::new(leaf.clone()),
                        then_value: Box::new(leaf.clone()),
                        else_value: Box::new(leaf.clone()),
                    },
                    1 => hir::ExprKind::FormedRangeLiteral(80),
                    2 => hir::ExprKind::IntegerLiteral(80),
                    3 => hir::ExprKind::OldField(hir::FieldAccess {
                        parameter: hir::ParameterId(0),
                        field: hir::FieldId(0),
                        ty: hir::RangeTypeId(0),
                        span: leaf.span,
                    }),
                    4 => hir::ExprKind::BoolLiteral(true),
                    _ => unreachable!(),
                };
                if fault == 1 {
                    leaf.ty = hir::ExprType::Bool;
                }
                if fault == 4 {
                    value.ty = hir::ExprType::Bool;
                }
                let mut children = vec![
                    TypedExpr {
                        kind: hir::ExprKind::FormedRangeLiteral(80),
                        ..value.clone()
                    };
                    3
                ];
                children[position] = leaf;
                value.kind = hir::ExprKind::Call {
                    function: hir::FunctionId(0),
                    arguments: vec![TypedExpr {
                        kind: hir::ExprKind::RecordConstruct {
                            record: hir::RecordId(0),
                            fields: children
                                .into_iter()
                                .enumerate()
                                .map(|(i, value)| hir::RecordFieldInit {
                                    field: hir::FieldId(i),
                                    span: value.span,
                                    value,
                                })
                                .collect(),
                        },
                        ty: hir::ExprType::Record(hir::RecordId(0)),
                        span: value.span,
                    }],
                };
                assert!(f.validate().is_err(), "fault {fault}, child {position}");
            }
        }
        // MIR honestly knows leaf shape, not canonical bounds/table membership.
        let mut f = original;
        let Statement::CopyFieldAssign { value, .. } = &mut f.blocks[0].statements[1] else {
            panic!()
        };
        *value = TypedExpr {
            kind: hir::ExprKind::FormedRangeLiteral(101),
            ty: hir::ExprType::Range(hir::RangeTypeId(usize::MAX)),
            span: value.span,
        };
        f.validate().unwrap();
    }
    #[test]
    fn reference_field_mir_retains_distinct_leaf_and_canonical_ids() {
        let p=compile_source("type Percent = range 0..100; record Battery {charge: Percent} fn f(v: &Battery) -> Percent {return v.charge;}").unwrap();
        let m = lower(&p);
        let f = &m.functions[0];
        f.validate().unwrap();
        let Terminator::Return { value } = &f.blocks[0].terminator else {
            panic!()
        };
        let hir::ExprKind::CopyReferenceFieldRead {
            reference,
            record,
            field,
        } = value.kind
        else {
            panic!()
        };
        assert_eq!(
            reference,
            hir::Place::Parameter(p.functions()[0].as_ordinary().unwrap().parameters[0])
        );
        assert_eq!(record, p.records()[0].id);
        assert_eq!(field, p.records()[0].fields[0]);
        assert_eq!(
            value.ty,
            hir::ExprType::Range(p.field(field).range_type().unwrap())
        );
        assert_eq!(m, lower(&p));
    }
    #[test]
    fn reference_field_mir_validation_inspects_first_middle_final_nested_children_honestly() {
        let p=compile_source("type Percent = range 0..100; record Battery {charge: Percent} record Triple {first: Percent,middle: Percent,last: Percent} fn three(a: Percent,b: Percent,c: Percent) -> Percent {return a;} fn f(v: &Battery) -> Percent {return three(v.charge,v.charge,v.charge);}").unwrap();
        let original = lower(&p).functions[1].clone();
        let Terminator::Return { value } = &original.blocks[0].terminator else {
            panic!()
        };
        let hir::ExprKind::Call { arguments, .. } = &value.kind else {
            panic!()
        };
        let leaf = arguments[0].clone();
        for container in ["call", "construction", "unary", "binary"] {
            for position in 0..3 {
                let mut f = original.clone();
                let Terminator::Return { value } = &mut f.blocks[0].terminator else {
                    panic!()
                };
                let hir::ExprKind::Call { arguments, .. } = &mut value.kind else {
                    panic!()
                };
                let mut bad = leaf.clone();
                bad.ty = hir::ExprType::Bool;
                let kind = match container {
                    "call" => hir::ExprKind::Call {
                        function: p.functions()[0].id(),
                        arguments: vec![bad],
                    },
                    "construction" => hir::ExprKind::RecordConstruct {
                        record: p.records()[1].id,
                        fields: vec![hir::RecordFieldInit {
                            field: p.records()[1].fields[0],
                            value: bad,
                            span: leaf.span,
                        }],
                    },
                    "unary" => hir::ExprKind::Unary {
                        op: crate::ast::UnaryOp::Not,
                        operand: Box::new(bad),
                    },
                    "binary" => hir::ExprKind::Binary {
                        op: crate::ast::BinaryOp::Equal,
                        left: Box::new(leaf.clone()),
                        right: Box::new(bad),
                    },
                    _ => unreachable!(),
                };
                arguments[position] = TypedExpr {
                    kind,
                    ty: leaf.ty,
                    span: leaf.span,
                };
                assert_eq!(
                    f.validate().unwrap_err(),
                    "reference Copy-field read has non-range result"
                );
            }
        }
        // No HIR tables at MIR: structural validity makes no membership/bounds claim.
        let mut f = original;
        let Terminator::Return { value } = &mut f.blocks[0].terminator else {
            panic!()
        };
        let hir::ExprKind::Call { arguments, .. } = &mut value.kind else {
            panic!()
        };
        let hir::ExprKind::CopyReferenceFieldRead { record, field, .. } = &mut arguments[0].kind
        else {
            panic!()
        };
        *record = hir::RecordId(usize::MAX);
        *field = hir::FieldId(usize::MAX);
        f.validate().unwrap();
    }
    #[test]
    fn reference_write_mir_checks_every_rhs_child_and_only_structural_authority() {
        for construction in [false, true] {
            for pos in 0..3 {
                let mut args = ["80", "80", "80"];
                args[pos] = "v.charge";
                let rhs = if construction {
                    format!(
                        "triple(Triple {{last:{},first:{},middle:{}}})",
                        args[0], args[1], args[2]
                    )
                } else {
                    format!("three({})", args.join(","))
                };
                let p=crate::compile_source(&format!("type Percent=range 0..100; record Battery {{charge:Percent}} record Triple {{first:Percent,middle:Percent,last:Percent}} fn three(a:Percent,b:Percent,c:Percent)->Percent {{return a;}} fn triple(t:Triple)->Percent {{return t.first;}} fn f(v:&mut Battery) {{v.charge={rhs};}}")).unwrap();
                let mut m = lower(&p);
                let f = m.functions.last_mut().unwrap();
                f.validate().unwrap();
                let Statement::CopyReferenceFieldAssign { value, .. } =
                    &mut f.blocks[0].statements[0]
                else {
                    panic!()
                };
                fn damage(e: &mut hir::TypedExpr) {
                    match &mut e.kind {
                        hir::ExprKind::CopyReferenceFieldRead { .. } => e.ty = hir::ExprType::Bool,
                        hir::ExprKind::Call { arguments, .. } => {
                            for a in arguments {
                                damage(a)
                            }
                        }
                        hir::ExprKind::RecordConstruct { fields, .. } => {
                            for a in fields {
                                damage(&mut a.value)
                            }
                        }
                        _ => {}
                    }
                }
                damage(value);
                assert!(f.validate().is_err(), "position {pos}");
            }
        }
        let p=crate::compile_source("type Percent=range 0..100; record Battery {charge:Percent} fn f(v:&mut Battery) {v.charge=80;}").unwrap();
        let mut m = lower(&p);
        let f = &mut m.functions[0];
        let Statement::CopyReferenceFieldAssign {
            reference,
            record,
            field,
            ..
        } = &mut f.blocks[0].statements[0]
        else {
            panic!()
        };
        *reference = hir::Place::Local(hir::LocalId(usize::MAX));
        *record = hir::RecordId(usize::MAX);
        *field = hir::FieldId(usize::MAX);
        f.validate().unwrap(); // MIR has no canonical tables or live provenance authority.
        let Statement::CopyReferenceFieldAssign { value, .. } = &mut f.blocks[0].statements[0]
        else {
            panic!()
        };
        value.ty = hir::ExprType::Bool;
        assert!(f.validate().is_err());
    }
    #[test]
    fn reference_write_mir_recurses_unary_binary_and_rejects_hidden_conditionals() {
        let p=crate::compile_source("type Percent=range 0..100; record Battery {charge:Percent} fn f(v:&mut Battery) {v.charge=80;}").unwrap();
        for defect in 0..4 {
            let mut m = lower(&p);
            let f = &mut m.functions[0];
            let Statement::CopyReferenceFieldAssign { value, .. } = &mut f.blocks[0].statements[0]
            else {
                panic!()
            };
            let mut bad = value.clone();
            bad.ty = hir::ExprType::Bool;
            value.kind = match defect {
                0 => hir::ExprKind::Unary {
                    op: crate::ast::UnaryOp::Not,
                    operand: Box::new(bad),
                },
                1 => hir::ExprKind::Binary {
                    op: crate::ast::BinaryOp::Subtract,
                    left: Box::new(bad),
                    right: Box::new(value.clone()),
                },
                2 => hir::ExprKind::Binary {
                    op: crate::ast::BinaryOp::Subtract,
                    left: Box::new(value.clone()),
                    right: Box::new(bad),
                },
                3 => hir::ExprKind::IfValue {
                    condition: Box::new(bad.clone()),
                    then_value: Box::new(value.clone()),
                    else_value: Box::new(value.clone()),
                },
                _ => unreachable!(),
            };
            assert!(f.validate().is_err());
        }
    }
}

#[cfg(test)]
impl MirProgram {
    pub(crate) fn test_functions_mut(&mut self) -> &mut Vec<MirFunction> {
        &mut self.functions
    }
}
#[cfg(test)]
impl MirFunction {
    pub(crate) fn test_blocks_mut(&mut self) -> &mut Vec<BasicBlock> {
        &mut self.blocks
    }
}
#[cfg(test)]
impl BasicBlock {
    pub(crate) fn test_terminator_mut(&mut self) -> &mut Terminator {
        &mut self.terminator
    }
}
#[cfg(test)]
impl BasicBlock {
    pub(crate) fn test_statements_mut(&mut self) -> &mut Vec<Statement> {
        &mut self.statements
    }
}

#[cfg(test)]
pub(crate) fn mutate_result_mir(program: &mut MirProgram, mutation: usize) {
    let f = program
        .functions
        .iter_mut()
        .find(|f| {
            f.blocks
                .iter()
                .any(|b| matches!(b.terminator, Terminator::Match { .. }))
        })
        .unwrap();
    match mutation {
        0..=4 => {
            let b = f
                .blocks
                .iter_mut()
                .find(|b| matches!(b.terminator, Terminator::Match { .. }))
                .unwrap();
            let Terminator::Match {
                result,
                scrutinee,
                ok_target,
                error_target,
            } = &mut b.terminator
            else {
                unreachable!()
            };
            match mutation {
                0 => std::mem::swap(ok_target, error_target),
                1 => *error_target = *ok_target,
                2 => *ok_target = BasicBlockId(999),
                3 => *result = hir::ResultTypeId(999),
                4 => scrutinee.ty = hir::ExprType::Bool,
                _ => unreachable!(),
            }
        }
        5..=12 => {
            let b = f
                .blocks
                .iter_mut()
                .find(|b| matches!(b.statements.first(), Some(Statement::BindResult { .. })))
                .unwrap();
            match mutation {
                5 => {
                    b.statements.remove(0);
                }
                6 => b.statements.insert(1, b.statements[0].clone()),
                7..=12 => {
                    let Statement::BindResult {
                        result,
                        tag,
                        local,
                        match_origin,
                        pattern_origin,
                    } = &mut b.statements[0]
                    else {
                        unreachable!()
                    };
                    match mutation {
                        7 => *result = hir::ResultTypeId(999),
                        8 => *tag = hir::ResultTag::Error,
                        9 => *local = LocalId(999),
                        10 => match_origin.start += 1,
                        11 => pattern_origin.end += 1,
                        12 => std::mem::swap(match_origin, pattern_origin),
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!(),
            }
        }
        13..=18 => {
            let flow = f.temporaries.as_mut().unwrap();
            let site = flow
                .sites
                .iter_mut()
                .find(|s| s.result_plan.is_some())
                .unwrap();
            match mutation {
                13 => site.result_plan = None,
                14 => site.result_plan = Some(hir::ResultTypeId(999)),
                15 => site.plan = Some(hir::resources::CleanupType::Record(hir::RecordId(0))),
                16 => site.destination = temporaries::Destination::Return,
                17 => site.transfer.span.start += 1,
                18 => site.region = temporaries::RegionId(999),
                _ => unreachable!(),
            }
        }
        19..=23 => {
            let flow = f.cleanup.as_mut().unwrap();
            match mutation {
                19 => flow.scopes[1].parent = None,
                20 => flow.scopes[1].parent = Some(cleanup::ScopeId(999)),
                21 => {
                    flow.blocks
                        .iter_mut()
                        .flat_map(|b| &mut b.initialize)
                        .next()
                        .unwrap()
                        .scope = cleanup::ScopeId(0)
                }
                22 => {
                    let b = flow.blocks.iter_mut().find(|b| !b.exit.is_empty()).unwrap();
                    b.exit.push(b.exit[0]);
                }
                23 => flow.protected_return = !flow.protected_return,
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
pub(crate) fn mutate_propagation_mir(m: &mut MirProgram, mutation: usize) {
    let f = m
        .functions
        .iter_mut()
        .find(|f| {
            f.blocks
                .iter()
                .any(|b| matches!(b.terminator, Terminator::Propagate { .. }))
        })
        .unwrap();
    let dispatch = f
        .blocks
        .iter()
        .position(|b| matches!(b.terminator, Terminator::Propagate { .. }))
        .unwrap();
    let (ok, error) = match f.blocks[dispatch].terminator {
        Terminator::Propagate {
            ok_target,
            error_target,
            ..
        } => (ok_target, error_target),
        _ => unreachable!(),
    };
    match mutation {
        0..=8 => {
            let Terminator::Propagate {
                operation,
                local,
                owning_scope,
                ok_target,
                error_target,
                ..
            } = &mut f.blocks[dispatch].terminator
            else {
                unreachable!()
            };
            match mutation {
                0 => operation.inner = hir::ResultTypeId(999),
                1 => operation.outer = hir::ResultTypeId(999),
                2 => operation.failure = hir::ValueType::Range(hir::RangeTypeId(999)),
                3 => *local = LocalId(999),
                4 => *owning_scope = 999,
                5 => std::mem::swap(ok_target, error_target),
                6 => *error_target = *ok_target,
                7 => operation.syntax.phrase.start += 1,
                8 => operation.syntax.binding.end += 1,
                _ => unreachable!(),
            }
        }
        9 => {
            f.blocks[ok.0].statements.remove(0);
        }
        10 => {
            let binding = f.blocks[ok.0].statements[0].clone();
            f.blocks[ok.0].statements.insert(1, binding);
        }
        11 => {
            let Statement::BindResult { tag, .. } = &mut f.blocks[ok.0].statements[0] else {
                unreachable!()
            };
            *tag = hir::ResultTag::Error;
        }
        12 => f.cleanup.as_mut().unwrap().blocks[error.0].exit.clear(),
        13 => {
            let flow = f.cleanup.as_mut().unwrap();
            let scope = flow.blocks[error.0].exit[0];
            flow.blocks[error.0].exit.push(scope);
        }
        14 => {
            f.cleanup.as_mut().unwrap().protected_return =
                !f.cleanup.as_ref().unwrap().protected_return
        }
        15 => {
            let Terminator::ForwardError { operation } = &mut f.blocks[error.0].terminator else {
                unreachable!()
            };
            operation.outer = hir::ResultTypeId(999);
        }
        16 => {
            for block in &mut f.blocks {
                for statement in &mut block.statements {
                    if let Statement::ReplaceOwned { owning_scope, .. } = statement {
                        *owning_scope = 1;
                        return;
                    }
                }
            }
            panic!("replacement fixture required");
        }
        17 => {
            let flow = f.cleanup.as_mut().unwrap();
            let scope = cleanup::ScopeId(flow.scopes.len());
            let local = match f.blocks[ok.0].statements[0] {
                Statement::BindResult { local, .. } => local,
                _ => unreachable!(),
            };
            let span = flow.scopes[0].span;
            flow.scopes.push(cleanup::Scope {
                id: scope,
                parent: Some(cleanup::ScopeId(0)),
                owners: vec![local],
                span,
            });
            flow.blocks[ok.0].enter = Some((
                scope,
                Origin {
                    span,
                    kind: OriginKind::ScopeEntry,
                },
            ));
            for init in &mut flow.blocks[ok.0].initialize {
                init.scope = scope;
            }
        }
        _ => unreachable!(),
    }
}
