//! Source-independent runtime contracts and explicit MIR invocation custody.
//! This is structural correspondence, never another ownership/effect checker.
use super::*;
use crate::{
    bounded::AdmissionLimits,
    effects::calls::Callee,
    hir::resources::PlanError,
    hir::{ExprKind, ExprType, FunctionId, TypedExpr},
    lexer::Span,
    mir::{
        self,
        temporaries::{Destination, RegionId, SiteId},
        BasicBlockId,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Allocation {
    Root,
    Edge,
    Argument,
    Invocation,
}
impl Allocation {
    fn name(self) -> &'static str {
        match self {
            Self::Root => "trusted root approval allocation",
            Self::Edge => "trusted structural edge allocation",
            Self::Argument => "trusted argument destination allocation",
            Self::Invocation => "trusted invocation plan allocation",
        }
    }
}
#[cfg(test)]
thread_local! { static FAILURE: std::cell::Cell<Option<Allocation>> = const {std::cell::Cell::new(None)}; }
#[cfg(test)]
pub(crate) fn with_failure<T>(site: Allocation, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Allocation>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAILURE.with(|s| s.set(self.0));
        }
    }
    let _restore = Restore(FAILURE.with(|s| s.replace(Some(site))));
    f()
}
fn reserve<T>(values: &mut Vec<T>, count: usize, site: Allocation) -> Result<(), PlanError> {
    #[cfg(test)]
    if count > values.capacity() - values.len() && FAILURE.with(|s| s.get() == Some(site)) {
        return Err(PlanError::Host(site.name()));
    }
    values
        .try_reserve_exact(count)
        .map_err(|_| PlanError::Host(site.name()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Invocation {
    pub ordinal: usize,
    pub function: FunctionId,
    pub block: BasicBlockId,
    pub statement: usize,
    pub site: SiteId,
    pub region: RegionId,
    pub operation: TrustedOperationId,
    pub result: hir::ResultTypeId,
    pub arguments: Vec<SiteId>,
    pub destination: Destination,
    pub source: Span,
    pub catalog: CatalogOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edge {
    pub caller: FunctionId,
    pub callee: Callee,
    pub origin: Span,
}

/// An immutable copy separate from HIR and independently retained by Approval.
/// Runtime dispatch will read this contract, not search HIR/source declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Approved {
    pub target: &'static str,
    pub compiler: &'static str,
    pub catalog_identity: &'static str,
    pub profile_identity: &'static str,
    pub assumptions: &'static str,
    pub catalog: Catalog,
    pub ranges: [hir::RangeType; 3],
    pub roots: Vec<FunctionId>,
    pub invocations: Vec<Invocation>,
    /// Every structural edge, including empty-effect ordinary calls and both
    /// alternatives; not the effects collector's intentionally filtered table.
    pub edges: Vec<Edge>,
}

impl Approved {
    pub(crate) fn derive(
        program: &hir::Program,
        mir: &mir::MirProgram,
        limits: AdmissionLimits,
    ) -> Result<Option<Self>, PlanError> {
        let Some(catalog) = &program.trusted else {
            return Ok(None);
        };
        catalog.validate(program).map_err(PlanError::Integrity)?;
        let mut approved = Self {
            target: TARGET,
            compiler: COMPILER,
            catalog_identity: CATALOG,
            profile_identity: PROFILE,
            assumptions: ASSUMPTIONS,
            catalog: catalog.clone(),
            ranges: [program.ranges[0].clone(), program.ranges[1].clone(), program.ranges[2].clone()],
            roots: Vec::new(),
            invocations: Vec::new(),
            edges: Vec::new(),
        };
        reserve(
            &mut approved.roots,
            catalog.selection.roots().len(),
            Allocation::Root,
        )?;
        for name in catalog.selection.roots() {
            let mut found = program.functions().iter().filter(|f| f.name() == name);
            let root = found
                .next()
                .and_then(|f| f.as_ordinary())
                .filter(|f| f.parameters.is_empty())
                .ok_or(PlanError::Integrity(
                    "trusted entry root must name an existing zero-argument ordinary function",
                ))?;
            if found.next().is_some() || approved.roots.contains(&root.id) {
                return Err(PlanError::Integrity("duplicate trusted entry root"));
            }
            approved.roots.push(root.id);
        }
        for function in mir.functions() {
            for (block_id, block) in function.blocks() {
                for (index, statement) in block.statements().iter().enumerate() {
                    let mut collector = Collector {
                        program,
                        approved: &mut approved,
                        function,
                        block: block_id,
                        statement: index,
                        limits,
                    };
                    match statement {
                        mir::Statement::Let { initializer, .. } => {
                            collector.expression(initializer, 1)?
                        }
                        mir::Statement::Assign { value, .. }
                        | mir::Statement::ReplaceOwned { value, .. }
                        | mir::Statement::DerefAssign { value, .. }
                        | mir::Statement::CopyFieldAssign { value, .. }
                        | mir::Statement::CopyReferenceFieldAssign { value, .. } => {
                            collector.expression(value, 1)?
                        }
                        mir::Statement::CallNoValue {
                            function,
                            arguments,
                            span,
                        } => {
                            collector.edge(Callee::Source(*function), *span)?;
                            for arg in arguments {
                                collector.expression(arg, 1)?;
                            }
                        }
                        mir::Statement::BindResult { .. } => {}
                    }
                }
                let mut collector = Collector {
                    program,
                    approved: &mut approved,
                    function,
                    block: block_id,
                    statement: block.statements().len(),
                    limits,
                };
                match block.terminator() {
                    mir::Terminator::Propagate { initializer, .. } => {
                        collector.expression(initializer, 1)?
                    }
                    mir::Terminator::Match { scrutinee, .. } => {
                        collector.expression(scrutinee, 1)?
                    }
                    mir::Terminator::Branch { condition, .. } => {
                        collector.expression(condition, 1)?
                    }
                    mir::Terminator::Return { value } => collector.expression(value, 1)?,
                    _ => {}
                }
            }
        }
        approved.shape(limits)?;
        Ok(Some(approved))
    }

    pub(crate) fn shape(&self, limits: AdmissionLimits) -> Result<(), PlanError> {
        if self.target != TARGET
            || self.compiler != COMPILER
            || self.catalog_identity != CATALOG
            || self.profile_identity != PROFILE
            || self.assumptions != ASSUMPTIONS
        {
            return Err(PlanError::Integrity(
                "trusted target/compiler/catalog/profile identity",
            ));
        }
        if self.roots.is_empty() {
            return Err(PlanError::Integrity("empty trusted root authority"));
        }
        if self.catalog.selection.sensor_limit() == 0
            || self.catalog.selection.sensor_limit() > 1024
        {
            return Err(PlanError::Integrity("invalid sealed live Sensor limit"));
        }
        if self.roots.len() > 64
            || self.ranges.iter().any(|r| r.name.len() > 128)
            || self.catalog.selection.roots().len() > 64
            || self
                .catalog
                .selection
                .roots()
                .iter()
                .any(|name| name.len() > 128)
            || self.invocations.len() > limits.tokens
            || self.edges.len() > limits.tokens.saturating_mul(4)
            || self.invocations.iter().any(|i| i.arguments.len() > 1)
        {
            return Err(PlanError::Limit("trusted approval metadata bounds"));
        }
        for (index, invocation) in self.invocations.iter().enumerate() {
            if invocation.ordinal != index
                || invocation.operation != self.catalog.operations[0].id
                    && invocation.operation != self.catalog.operations[1].id
                    && invocation.operation != self.catalog.operations[3].id
            {
                return Err(PlanError::Integrity(
                    "trusted invocation identity or cleanup-only target",
                ));
            }
        }
        Ok(())
    }
}

struct Collector<'a> {
    program: &'a hir::Program,
    approved: &'a mut Approved,
    function: &'a mir::MirFunction,
    block: BasicBlockId,
    statement: usize,
    limits: AdmissionLimits,
}
impl Collector<'_> {
    fn edge(&mut self, callee: Callee, origin: Span) -> Result<(), PlanError> {
        if self.approved.edges.len() >= self.limits.tokens.saturating_mul(4) {
            return Err(PlanError::Limit("trusted structural call edges"));
        }
        callee
            .effects(self.program)
            .ok_or(PlanError::Integrity("unapproved structural call edge"))?;
        reserve(&mut self.approved.edges, 1, Allocation::Edge)?;
        self.approved.edges.push(Edge {
            caller: self.function.function(),
            callee,
            origin,
        });
        Ok(())
    }
    fn expression(&mut self, e: &TypedExpr, depth: usize) -> Result<(), PlanError> {
        if depth > self.limits.traversal_depth {
            return Err(PlanError::Limit("trusted invocation traversal depth"));
        }
        match &e.kind {
            ExprKind::TrustedCall {
                operation,
                arguments,
            } => {
                self.edge(Callee::Trusted(*operation), e.span)?;
                let catalog = self
                    .program
                    .trusted
                    .as_ref()
                    .ok_or(PlanError::Integrity("missing trusted catalog"))?;
                let op = catalog
                    .operation(*operation)
                    .ok_or(PlanError::Integrity("foreign trusted invocation"))?;
                let result = op.result.ok_or(PlanError::Integrity(
                    "source-callable trusted NoValue target",
                ))?;
                let flow = self
                    .function
                    .temporaries
                    .as_ref()
                    .ok_or(PlanError::Integrity(
                        "trusted invocation lacks temporary custody",
                    ))?;
                let mut sites = flow
                    .sites
                    .iter()
                    .filter(|s| s.origin.span == e.span && s.ty == e.ty);
                let site = sites.next().ok_or(PlanError::Integrity(
                    "trusted invocation lacks exact MIR site",
                ))?;
                if sites.next().is_some()
                    || site.result_plan != Some(result)
                    || site.plan.is_some()
                    || e.ty != ExprType::Result(result)
                    || site.children.len() != arguments.len()
                    || arguments.len() != usize::from(op.parameter.is_some())
                    || self
                        .approved
                        .invocations
                        .iter()
                        .any(|i| i.function == self.function.function() && i.site == site.id)
                {
                    return Err(PlanError::Integrity(
                        "trusted invocation/custody correspondence",
                    ));
                }
                let mut argument_sites = Vec::new();
                reserve(&mut argument_sites, arguments.len(), Allocation::Argument)?;
                for (index, (child, arg)) in site.children.iter().zip(arguments).enumerate() {
                    let child_site = flow
                        .sites
                        .get(child.0)
                        .filter(|s| s.id == *child)
                        .ok_or(PlanError::Integrity("foreign trusted argument site"))?;
                    if child_site.destination != Destination::TrustedArgument(*operation, index)
                        || child_site.ty != arg.ty
                        || Some(arg.ty) != op.parameter.map(|p| ExprType::from(p.value_type()))
                    {
                        return Err(PlanError::Integrity(
                            "trusted argument destination/passing mode correspondence",
                        ));
                    }
                    argument_sites.push(*child);
                }
                if self.approved.invocations.len() >= self.limits.tokens {
                    return Err(PlanError::Limit("trusted invocation plans"));
                }
                reserve(&mut self.approved.invocations, 1, Allocation::Invocation)?;
                let ordinal = self.approved.invocations.len();
                self.approved.invocations.push(Invocation {
                    ordinal,
                    function: self.function.function(),
                    block: self.block,
                    statement: self.statement,
                    site: site.id,
                    region: site.region,
                    operation: *operation,
                    result,
                    arguments: argument_sites,
                    destination: site.destination,
                    source: e.span,
                    catalog: op.origin,
                });
                for arg in arguments {
                    self.expression(arg, depth + 1)?;
                }
            }
            ExprKind::Call {
                function,
                arguments,
            } => {
                self.edge(Callee::Source(*function), e.span)?;
                for arg in arguments {
                    self.expression(arg, depth + 1)?;
                }
            }
            ExprKind::ResultConstruct { payload, .. } => self.expression(payload, depth + 1)?,
            ExprKind::RecordConstruct { fields, .. } => {
                for field in fields {
                    self.expression(&field.value, depth + 1)?;
                }
            }
            ExprKind::Unary { operand, .. } => self.expression(operand, depth + 1)?,
            ExprKind::Binary { left, right, .. } => {
                self.expression(left, depth + 1)?;
                self.expression(right, depth + 1)?;
            }
            ExprKind::IfValue { .. } => {
                return Err(PlanError::Integrity(
                    "residual conditional trusted invocation plan",
                ))
            }
            _ => {}
        }
        Ok(())
    }
}
