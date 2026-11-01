//! Sealed execution domains. Only Ordinary exists in production builds.
//! Private abort scenarios admit their complete cleanup before executable approval.
use super::*;

#[cfg(test)]
thread_local! {
    pub(super) static FAIL_ALLOCATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
fn allocation() -> Result<(), ExecutionError> {
    if FAIL_ALLOCATION.with(std::cell::Cell::get) {
        return Err(ExecutionError::new(
            ErrorKind::Host("private abort authority allocation"),
            START,
        ));
    }
    Ok(())
}

pub(super) enum Request {
    Ordinary,
    #[cfg(test)]
    Primitive,
    #[cfg(test)]
    Abort(Vec<AbortRequest>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Domain {
    Ordinary,
    #[cfg(test)]
    Primitive,
    #[cfg(test)]
    Abort(Vec<Abort>),
}
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AbortRequest {
    pub function: FunctionId,
    pub site: mir::temporaries::SiteId,
    pub before: hir::FieldId,
    // Exact monotonic dynamic identities and call-stack position of this private
    // bounded scenario. They cannot be selected or manufactured at runtime.
    pub frame: u64,
    pub depth: usize,
    pub pending: u64,
}
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Abort {
    pub request: AbortRequest,
    pub record: hir::RecordId,
    pub initialized: Vec<hir::FieldId>,
    pub effects: crate::effects::EffectSet,
    pub origin: Origin,
    pub failure: Span,
    pub before_site: mir::temporaries::SiteId,
    pub before_origin: Origin,
}

impl Domain {
    pub(super) fn shape(&self, _limits: AdmissionLimits) -> Result<(), ExecutionError> {
        #[cfg(test)]
        if let Self::Abort(templates) = self {
            if templates.len() > _limits.traversal_depth
                || templates
                    .iter()
                    .any(|t| t.initialized.len() > _limits.tokens)
            {
                return Err(ExecutionError::integrity("private abort metadata shape"));
            }
        }
        Ok(())
    }
    pub(super) fn primitive(&self) -> bool {
        #[cfg(test)]
        if matches!(self, Self::Primitive) {
            return true;
        }
        false
    }
    pub(super) fn permits_abort(&self) -> bool {
        #[cfg(test)]
        if matches!(self, Self::Primitive | Self::Abort(_)) {
            return true;
        }
        false
    }
    pub(super) fn admit(
        _program: &hir::Program,
        _mir: &mir::MirProgram,
        _plans: &[Option<hir::resources::CleanupPlan>],
        _summaries: &[crate::effects::EffectSet],
        _limits: AdmissionLimits,
        request: Request,
    ) -> Result<Self, ExecutionError> {
        match request {
            Request::Ordinary => Ok(Self::Ordinary),
            #[cfg(test)]
            Request::Primitive => Ok(Self::Primitive),
            #[cfg(test)]
            Request::Abort(requests) => {
                Self::bounds(_program, _limits, requests.len())?;
                if requests.is_empty() {
                    return Err(ExecutionError::integrity("empty private abort scenario"));
                }
                let mut templates = Vec::new();
                allocation()?;
                templates.try_reserve_exact(requests.len()).map_err(|_| {
                    ExecutionError::new(
                        ErrorKind::Host("private abort authority allocation"),
                        START,
                    )
                })?;
                for request in requests {
                    if templates
                        .iter()
                        .any(|t: &Abort| t.request.pending == request.pending)
                        || templates.last().is_some_and(|t| {
                            t.request.pending >= request.pending || t.request.depth > request.depth
                        })
                    {
                        return Err(ExecutionError::integrity(
                            "private abort scenario order or duplicate instance",
                        ));
                    }
                    templates.push(Abort::derive(
                        _program, _mir, _plans, _summaries, _limits, request,
                    )?);
                }
                Ok(Self::Abort(templates))
            }
        }
    }
    pub(super) fn validate(&self, _executable: &CheckedExecutable) -> Result<(), ExecutionError> {
        #[cfg(test)]
        if let Self::Abort(templates) = self {
            Self::bounds(&_executable.program, _executable.limits, templates.len())?;
            if templates.is_empty() {
                return Err(ExecutionError::integrity("empty private abort authority"));
            }
            for (index, template) in templates.iter().enumerate() {
                if index > 0
                    && (templates[index - 1].request.pending >= template.request.pending
                        || templates[index - 1].request.depth > template.request.depth)
                {
                    return Err(ExecutionError::integrity(
                        "private abort authority instance order",
                    ));
                }
                if template.initialized.len() > _executable.limits.tokens {
                    return Err(ExecutionError::integrity("private abort membership shape"));
                }
                let expected = Abort::derive(
                    &_executable.program,
                    &_executable.mir,
                    &_executable.cleanup_plans,
                    &_executable.cleanup_effects,
                    _executable.limits,
                    template.request,
                )?;
                if *template != expected {
                    return Err(ExecutionError::integrity(
                        "canonical private abort authority correspondence",
                    ));
                }
            }
        }
        Ok(())
    }
    #[cfg(test)]
    fn bounds(
        program: &hir::Program,
        limits: AdmissionLimits,
        count: usize,
    ) -> Result<(), ExecutionError> {
        if count > limits.traversal_depth || count > limits.tokens {
            return Err(ResourceFailure::limit("private abort scenario count", START).into());
        }
        // Include both immutable copies, each retained membership, and scratch
        // derivation alongside a conservative frontend envelope. No source heap.
        let extra = count
            .checked_mul(program.fields.len() + 1)
            .and_then(|n| n.checked_mul(4 * std::mem::size_of::<Abort>()));
        let extra = extra.ok_or_else(|| {
            ExecutionError::from(ResourceFailure::limit(
                "private abort authority storage",
                START,
            ))
        })?;
        let context = Context::new(limits);
        validate::program_shape(program, &context)?;
        let nodes = context.resource_nodes.get()
            + program.functions.len()
            + program.locals.len()
            + program.parameters.len();
        context.reserve_cleanup_effects_extra(
            nodes,
            crate::effects::custody::holder_bound(program),
            extra,
        )?;
        Ok(())
    }
}

#[cfg(test)]
impl Abort {
    fn derive(
        program: &hir::Program,
        mir: &mir::MirProgram,
        plans: &[Option<hir::resources::CleanupPlan>],
        summaries: &[crate::effects::EffectSet],
        limits: AdmissionLimits,
        request: AbortRequest,
    ) -> Result<Self, ExecutionError> {
        use mir::temporaries::Destination;
        let invalid = || ExecutionError::integrity("private abort canonical construction binding");
        if request.frame >= limits.tokens as u64
            || request.pending >= limits.tokens as u64
            || request.depth >= limits.traversal_depth
        {
            return Err(invalid());
        }
        let function = program
            .functions()
            .get(request.function.0)
            .and_then(|f| f.as_ordinary())
            .filter(|f| f.id == request.function)
            .ok_or_else(invalid)?;
        let flow = mir
            .functions()
            .iter()
            .find(|f| f.function() == request.function)
            .and_then(|f| f.temporaries.as_ref())
            .ok_or_else(invalid)?;
        let site = flow
            .sites
            .get(request.site.0)
            .filter(|s| s.id == request.site)
            .ok_or_else(invalid)?;
        let hir::resources::CleanupType::Record(record) = site.plan.ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let declaration = program.records().get(record.0).ok_or_else(invalid)?;
        if site.children.len() != declaration.fields.len()
            || !matches!(
                plans.get(record.0),
                Some(Some(hir::resources::CleanupPlan::Fields(_)))
            )
        {
            return Err(invalid());
        }
        let mut initialized = Vec::new();
        allocation()?;
        initialized
            .try_reserve_exact(site.children.len())
            .map_err(|_| {
                ExecutionError::new(
                    ErrorKind::Host("private abort membership allocation"),
                    site.origin.span,
                )
            })?;
        let mut effects = crate::effects::EffectSet::EMPTY;
        let mut failure = None;
        let mut before_site = None;
        let mut before_origin = None;
        for child in &site.children {
            let child = flow.sites.get(child.0).ok_or_else(invalid)?;
            let Destination::Pending(parent, field) = child.destination else {
                return Err(invalid());
            };
            if parent != record || !declaration.fields.contains(&field) {
                return Err(invalid());
            }
            if field == request.before {
                before_site = Some(child.id);
                before_origin = Some(child.origin);
                failure = Some(
                    flow.regions
                        .get(child.region.0)
                        .ok_or_else(invalid)?
                        .origin
                        .span,
                );
                break;
            }
            if initialized.contains(&field) {
                return Err(invalid());
            }
            initialized.push(field);
            if let Some(ValueType::Record(r)) = program.fields().get(field.0).map(|f| f.ty) {
                effects = effects.union(
                    summaries
                        .get(r.0)
                        .copied()
                        .unwrap_or(crate::effects::EffectSet::EMPTY),
                );
            }
        }
        let failure = failure.ok_or_else(invalid)?;
        if !effects.is_subset(function.effects.set()) {
            let missing = effects
                .iter()
                .find(|e| !function.effects.set().contains(*e))
                .unwrap();
            return Err(ExecutionError::new(
                ErrorKind::Frontend {
                    phase: "effects",
                    message: format!(
                        "approved private abort in `{}` requires declared cleanup effect `{}`",
                        function.name,
                        missing.name()
                    ),
                },
                failure,
            ));
        }
        Ok(Self {
            request,
            record,
            initialized,
            effects,
            origin: site.origin,
            failure,
            before_site: before_site.ok_or_else(invalid)?,
            before_origin: before_origin.ok_or_else(invalid)?,
        })
    }
}
