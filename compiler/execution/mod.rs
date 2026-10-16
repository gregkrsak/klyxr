//! Deterministic ordinary MIR execution. The only public preparation starts at
//! bounded source; public HIR and MIR inspection never confer execution authority.
// Keep immediate origins and the wide trap value inline: exhaustion reporting
// must not require an additional allocation merely to return the error itself.
#![allow(clippy::result_large_err)]
mod machine;
mod render;

pub use crate::bounded::{AdmissionLimits, ResourceFailure, ResourceKind};
use crate::{
    bounded::{Context, START},
    hir::{self, FunctionId, ValueType},
    lexer::Span,
    mir::{self, Origin},
};
pub use render::write_output;
use std::{io::Read, rc::Rc};
pub fn render_admission_error(
    error: &ExecutionError,
    allowance: usize,
) -> Result<String, ExecutionError> {
    render::error(None, error, allowance)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLimits {
    pub steps: u64,
    pub call_depth: usize,
    pub expression_depth: usize,
    pub working_storage: usize,
    pub rendered_output: usize,
    pub trace_depth: usize,
}
impl ExecutionLimits {
    pub const MAX: Self = Self {
        steps: 100_000_000,
        call_depth: 256,
        expression_depth: 128,
        working_storage: 268_435_456,
        rendered_output: 1_048_576,
        trace_depth: 64,
    };
    pub fn validate(self) -> Result<(), ResourceFailure> {
        let m = Self::MAX;
        if self.steps == 0 || self.steps > m.steps {
            return Err(ResourceFailure::limit("steps override", START));
        }
        for (v, max, name) in [
            (self.call_depth, m.call_depth, "call depth override"),
            (
                self.expression_depth,
                m.expression_depth,
                "expression depth override",
            ),
            (
                self.working_storage,
                m.working_storage,
                "working storage override",
            ),
            (
                self.rendered_output,
                m.rendered_output,
                "rendered output override",
            ),
            (self.trace_depth, m.trace_depth, "trace depth override"),
        ] {
            if v == 0 || v > max {
                return Err(ResourceFailure::limit(name, START));
            }
        }
        Ok(())
    }
}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            steps: 1_000_000,
            call_depth: 64,
            expression_depth: 64,
            working_storage: 16_777_216,
            rendered_output: 65_536,
            trace_depth: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeValue {
    pub ty: hir::RangeTypeId,
    pub value: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeValue {
    Bool(bool),
    Range(RangeValue),
    Record {
        record: hir::RecordId,
        fields: Vec<(hir::FieldId, RangeValue)>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    Value(RuntimeValue),
    NoValue,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Frontend {
        phase: &'static str,
        message: String,
    },
    RangeTrap {
        ty: hir::RangeTypeId,
        min: i64,
        max: i64,
        computed: i128,
    },
    ConfiguredLimit(&'static str),
    Integrity(&'static str),
    Host(&'static str),
    /// Private synthetic construction control, not a recoverable source error.
    SyntheticConstructionFailure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallTrace {
    pub function: FunctionId,
    pub call_site: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionError {
    pub kind: ErrorKind,
    pub origin: Span,
    pub expression: Option<Span>,
    pub trace: Vec<CallTrace>,
    pub trace_truncated: bool,
}
impl ExecutionError {
    pub(crate) fn new(kind: ErrorKind, origin: Span) -> Self {
        Self {
            kind,
            origin,
            expression: None,
            trace: Vec::new(),
            trace_truncated: false,
        }
    }
    pub(crate) fn integrity(message: &'static str) -> Self {
        Self::new(ErrorKind::Integrity(message), START)
    }
}
impl From<ResourceFailure> for ExecutionError {
    fn from(f: ResourceFailure) -> Self {
        Self::new(
            match f.kind {
                ResourceKind::Limit => ErrorKind::ConfiguredLimit(f.resource),
                ResourceKind::Host => ErrorKind::Host(f.resource),
            },
            f.span,
        )
    }
}
impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} at {}:{}",
            self.kind, self.origin.line, self.origin.column
        )
    }
}
impl std::error::Error for ExecutionError {}

// An immutable witness retained only after ownership succeeds. Exact comparison
// catches compiler-internal replacement with a merely typed program as well as
// numeric-ID-aligned substitutions; this never re-runs ownership at execution.
struct Approval {
    program: hir::Program,
}

/// No public fields, constructors, mutation, or independent HIR/MIR pairing.
/// ```compile_fail
/// use klyxr_compiler::{compile_source, execution::CheckedExecutable};
/// let program = compile_source("fn demo() {}").unwrap();
/// let executable = CheckedExecutable { program };
/// ```
pub struct CheckedExecutable {
    program: hir::Program,
    mir: mir::MirProgram,
    origins: Vec<Vec<Origin>>,
    // Private preparation identity binds three independently stored components.
    // It is neither a numeric source identity nor global runtime state.
    approval: Rc<Approval>,
    mir_approval: Rc<Approval>,
    origin_approval: Rc<Approval>,
    limits: AdmissionLimits,
    cleanup_plans: Vec<Option<hir::resources::CleanupPlan>>,
}
impl CheckedExecutable {
    pub fn entry(&self, name: &str) -> Result<FunctionId, ExecutionError> {
        let f = self
            .program
            .functions()
            .iter()
            .find(|f| f.name() == name)
            .ok_or_else(|| ExecutionError::integrity("selected entry does not exist"))?;
        let ordinary = f.as_ordinary().ok_or_else(|| {
            ExecutionError::new(
                ErrorKind::Integrity("selected entry is verified, not ordinary"),
                f.span(),
            )
        })?;
        if !ordinary.parameters.is_empty() {
            return Err(ExecutionError::new(
                ErrorKind::Integrity("selected entry must have zero parameters"),
                ordinary.span,
            ));
        }
        Ok(ordinary.id)
    }
    fn function(&self, id: FunctionId) -> Result<&mir::MirFunction, ExecutionError> {
        let mut found = self.mir.functions().iter().filter(|f| f.function() == id);
        let f = found.next().ok_or_else(|| {
            ExecutionError::integrity("canonical ordinary MIR function is missing")
        })?;
        if found.next().is_some() {
            return Err(ExecutionError::integrity(
                "duplicate canonical MIR function",
            ));
        }
        Ok(f)
    }
    fn origin(&self, id: FunctionId, block: mir::BasicBlockId) -> Result<Origin, ExecutionError> {
        let index = self
            .mir
            .functions()
            .iter()
            .position(|f| f.function() == id)
            .ok_or_else(|| ExecutionError::integrity("origin function missing"))?;
        let position = self.mir.functions()[index]
            .blocks()
            .position(|(b, _)| b == block)
            .ok_or_else(|| ExecutionError::integrity("origin block missing"))?;
        self.origins
            .get(index)
            .and_then(|v| v.get(position))
            .copied()
            .ok_or_else(|| ExecutionError::integrity("lowering origin missing"))
    }
    fn validate(&self) -> Result<(), ExecutionError> {
        if !Rc::ptr_eq(&self.approval, &self.mir_approval)
            || !Rc::ptr_eq(&self.approval, &self.origin_approval)
        {
            return Err(ExecutionError::integrity(
                "cross-compilation execution authority",
            ));
        }
        let c = Context::new(self.limits);
        // Admission already bounds this private immutable representation. Check
        // shape before any recursive comparison/re-lowering, including injections.
        validate::program_shape(&self.program, &c)?;
        let plans = if self.program.resources.leaves.is_empty() {
            Vec::new()
        } else {
            self.program
                .resources
                .plans(&self.program)
                .map_err(ExecutionError::from)?
        };
        if self.cleanup_plans != plans {
            return Err(ExecutionError::integrity(
                "canonical cleanup plan correspondence",
            ));
        }
        if self.program != self.approval.program {
            return Err(ExecutionError::integrity(
                "frontend ownership approval mismatch",
            ));
        }
        if self.mir.functions().len() != self.origins.len() {
            return Err(ExecutionError::integrity("origin/function correspondence"));
        }
        validate::mir_shape(&self.program, &self.mir, &c)?;
        for f in self.mir.functions() {
            f.validate().map_err(|s| {
                if s == "MIR validation allocation failed" {
                    ExecutionError::new(ErrorKind::Host("MIR validation allocation"), START)
                } else {
                    ExecutionError::integrity(s)
                }
            })?;
        }
        // Same deterministic lowerer, not a second semantic or ownership pipeline.
        let (expected, origins) =
            mir::lower_with_origins(&self.program).map_err(ExecutionError::from)?;
        if self.mir != expected || self.origins != origins {
            return Err(ExecutionError::integrity(
                "exact HIR/MIR/origin correspondence",
            ));
        }
        Ok(())
    }
    pub fn render_result(
        &self,
        result: &ExecutionResult,
        allowance: usize,
    ) -> Result<String, ExecutionError> {
        render::result(self, result, allowance)
    }
    pub fn render_error(
        &self,
        error: &ExecutionError,
        allowance: usize,
    ) -> Result<String, ExecutionError> {
        render::error(Some(self), error, allowance)
    }
}

pub fn prepare_execution(
    source: &str,
    limits: AdmissionLimits,
) -> Result<CheckedExecutable, ExecutionError> {
    limits.validate()?;
    if source.len() > limits.source_bytes {
        return Err(ResourceFailure::limit("source bytes", START).into());
    }
    // Reserve lexer input/token capacity before its first source-directed growth.
    if source
        .len()
        .checked_mul(128)
        .map_or(true, |v| v > limits.working_storage)
    {
        return Err(ResourceFailure::limit("lexer storage reservation", START).into());
    }
    if source.len() as u64 > limits.frontend_work {
        return Err(ResourceFailure::limit("lexing work", START).into());
    }
    let context = Context::new(limits);
    let tokens = crate::lexer::lex_bounded(source, &context).map_err(|e| {
        context
            .failure
            .get()
            .map(ExecutionError::from)
            .unwrap_or_else(|| {
                ExecutionError::new(
                    ErrorKind::Frontend {
                        phase: "lex",
                        message: e.message,
                    },
                    e.span,
                )
            })
    })?;
    context.reserve_frontend(tokens.len(), source.len())?;
    let ast = crate::parser::parse_bounded(&tokens, &context).map_err(|e| {
        context
            .failure
            .get()
            .map(ExecutionError::from)
            .unwrap_or_else(|| {
                ExecutionError::new(
                    ErrorKind::Frontend {
                        phase: "parse",
                        message: e.message,
                    },
                    e.span,
                )
            })
    })?;
    let resolved = crate::resolve::resolve(&ast).map_err(|e| frontend("resolve", e, &context))?;
    let program = crate::types::check(resolved).map_err(|e| frontend("type", e, &context))?;
    crate::ownership::check_bounded(&program, &context)
        .map_err(|e| frontend("ownership", e, &context))?;
    if let Some(e) = context.failure.get() {
        return Err(e.into());
    }
    crate::effects::calls::check(&program)
        .map_err(|e| frontend("effects", e, &context))?;
    seal(program, limits)
}
impl From<hir::resources::PlanError> for ExecutionError {
    fn from(error: hir::resources::PlanError) -> Self {
        use hir::resources::PlanError;
        match error {
            PlanError::Integrity(m) => Self::integrity(m),
            PlanError::Limit(m) => Self::new(
                ErrorKind::ConfiguredLimit(if m == "cleanup type graph depth exceeded" {
                    "cleanup type graph depth"
                } else {
                    m
                }),
                START,
            ),
            PlanError::Host(m) => Self::new(ErrorKind::Host(m), START),
        }
    }
}
fn seal(
    program: hir::Program,
    limits: AdmissionLimits,
) -> Result<CheckedExecutable, ExecutionError> {
    let (mir, origins) = mir::lower_with_origins(&program).map_err(ExecutionError::from)?;
    let cleanup_plans = if program.resources.leaves.is_empty() {
        Vec::new()
    } else {
        program
            .resources
            .plans(&program)
            .map_err(ExecutionError::from)?
    };
    let approval = Rc::new(Approval {
        program: program.clone(),
    });
    let executable = CheckedExecutable {
        program,
        mir,
        origins,
        mir_approval: approval.clone(),
        origin_approval: approval.clone(),
        approval,
        limits,
        cleanup_plans,
    };
    executable.validate()?;
    Ok(executable)
}
fn frontend(
    phase: &'static str,
    errors: Vec<crate::diagnostics::Diagnostic>,
    context: &Context,
) -> ExecutionError {
    if let Some(e) = context.failure.get() {
        return e.into();
    }
    match errors.into_iter().next() {
        Some(e) => ExecutionError::new(
            ErrorKind::Frontend {
                phase,
                message: e.message,
            },
            e.span,
        ),
        None => ExecutionError::integrity("frontend returned no diagnostic"),
    }
}

/// Reads at most source_bytes + one lookahead byte; oversized input is never
/// accumulated. The fixed read buffer also bounds rejected-input housekeeping.
pub fn prepare_execution_from_reader(
    mut input: impl Read,
    limits: AdmissionLimits,
) -> Result<CheckedExecutable, ExecutionError> {
    limits.validate()?;
    let mut source = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let available = limits
            .source_bytes
            .checked_sub(source.len())
            .ok_or_else(|| ExecutionError::integrity("input accounting"))?;
        let count = input
            .read(&mut buffer[..available.saturating_add(1).min(4096)])
            .map_err(|_| ExecutionError::new(ErrorKind::Host("source input"), START))?;
        if count == 0 {
            break;
        }
        if count > available {
            return Err(ResourceFailure::limit("source bytes", START).into());
        }
        let needed = source
            .len()
            .checked_add(count)
            .and_then(|n| n.checked_mul(2));
        if needed.map_or(true, |n| n > limits.working_storage) {
            return Err(ResourceFailure::limit("source storage", START).into());
        }
        source
            .try_reserve_exact(count)
            .map_err(|_| ExecutionError::new(ErrorKind::Host("source allocation"), START))?;
        source.extend_from_slice(&buffer[..count]);
    }
    let source = std::str::from_utf8(&source)
        .map_err(|_| ExecutionError::new(ErrorKind::Host("source UTF-8 decoding"), START))?;
    prepare_execution(source, limits)
}

pub fn execute(
    executable: &CheckedExecutable,
    entry: FunctionId,
    limits: ExecutionLimits,
) -> Result<ExecutionResult, ExecutionError> {
    limits.validate()?;
    executable.validate()?;
    let f = executable
        .program
        .functions()
        .get(entry.0)
        .filter(|f| f.id() == entry)
        .and_then(|f| f.as_ordinary())
        .ok_or_else(|| ExecutionError::integrity("selected entry is not canonical ordinary"))?;
    if !f.parameters.is_empty() {
        return Err(ExecutionError::new(
            ErrorKind::Integrity("selected entry must have zero parameters"),
            f.span,
        ));
    }
    if matches!(
        f.return_type,
        Some(ValueType::SharedRef(_) | ValueType::MutableRef(_))
    ) {
        return Err(ExecutionError::new(
            ErrorKind::Integrity("reference result cannot escape"),
            f.span,
        ));
    }
    if let Some(ValueType::Record(id)) = f.return_type {
        if executable
            .cleanup_plans
            .get(id.0)
            .is_some_and(Option::is_some)
        {
            return Err(ExecutionError::integrity(
                "live resource result cannot cross public boundary",
            ));
        }
    }
    machine::Machine::new(executable, limits).run(entry)
}

#[cfg(test)]
mod tests;
mod validate;

#[cfg(test)]
fn prepare_synthetic(
    program: hir::Program,
    limits: AdmissionLimits,
) -> Result<CheckedExecutable, ExecutionError> {
    limits.validate()?;
    let context = Context::new(limits);
    context.reserve_frontend(
        program.functions.len()
            + program.fields.len()
            + program.locals.len()
            + program.parameters.len()
            + 16,
        0,
    )?;
    validate::program_shape(&program, &context)?;
    crate::ownership::check_bounded(&program, &context)
        .map_err(|e| frontend("ownership", e, &context))?;
    if let Some(e) = context.failure.get() {
        return Err(e.into());
    }
    crate::effects::calls::check(&program)
        .map_err(|e| frontend("effects", e, &context))?;
    seal(program, limits)
}

#[cfg(test)]
mod effect_authority_tests {
    use super::*;
    use crate::effects::{Effect, EffectSet};

    #[test]
    fn direct_effect_sites_reject_independent_authority_mutations() {
        for mutation in 0..9 {
            let mut e = prepare_execution(
                "fn leaf() effects clock {} fn demo() effects io,clock {leaf();leaf();}",
                AdmissionLimits::default(),
            )
            .unwrap();
            let sites = &mut e.mir.test_functions_mut()[1].direct_effects;
            match mutation {
                0 => {
                    sites.pop();
                }
                1 => sites.push(sites[0].clone()),
                2 => sites.swap(0, 1),
                3 => sites[0].function = FunctionId(0),
                4 => sites[0].effects = EffectSet::EMPTY,
                5 => {
                    sites[0].effects.insert(Effect::Io);
                }
                6 => sites[0].origin.start += 1,
                7 => sites[0].ordinal += 10,
                _ => sites[0].callee = FunctionId(1),
            }
            assert!(e.validate().is_err(), "mutation {mutation}");
        }
    }
}

#[cfg(test)]
mod temporary_authority_tests {
    use super::*;
    use crate::mir::temporaries::{Destination, RegionId, SiteId};

    fn artifact() -> CheckedExecutable {
        prepare_synthetic(
            hir::resources::fixture_with(
                "fn take(a:Leaf,b:Leaf){}fn demo(){let a=acquire();let b=acquire();take(a,b);}",
            ),
            AdmissionLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn each_independent_temporary_authority_mutation_is_rejected() {
        for fault in 0..37 {
            let mut e = artifact();
            let f = e.mir.test_functions_mut().last_mut().unwrap();
            if fault == 0 {
                f.temporaries = None;
            } else {
                let flow = f.temporaries.as_mut().unwrap();
                match fault {
                    1 => flow.regions[0].roots.clear(),
                    2 => {
                        let root = flow.regions[0].roots[0];
                        flow.regions[0].roots.push(root);
                    }
                    3 => flow.sites[0].id = SiteId(usize::MAX),
                    4 => flow.sites[0].plan = Some(hir::RecordId(999)),
                    5 => flow.sites[0].destination = Destination::Parameter(hir::ParameterId(999)),
                    6 => flow.regions[0].parent = Some(RegionId(0)),
                    7 => flow.regions[0].origin.kind = mir::OriginKind::FunctionBoundary,
                    8 => flow.sites[0].transfer.kind = mir::OriginKind::Statement,
                    9 => flow.sites[0].cleanup.kind = mir::OriginKind::Statement,
                    10 => flow.regions.last_mut().unwrap().roots.swap(0, 1),
                    11 => {
                        flow.points.remove(0);
                    }
                    12 => flow.points.push(flow.points[0].clone()),
                    13 => flow.points[0].block = mir::BasicBlockId(usize::MAX),
                    14 => flow.points[0].statement = usize::MAX,
                    15 => flow.points[0].begin = false,
                    16 => flow.points[0].complete = false,
                    17 => flow.regions[0].protect_result = false,
                    18 => flow.regions[0].retire_before_publication = false,
                    19 => flow.regions[0].destination = Destination::Return,
                    20 => flow.sites.push(flow.sites[0].clone()),
                    21 => flow.regions.push(flow.regions[0].clone()),
                    22 => flow.sites[0].children.push(SiteId(0)),
                    23 => flow.regions[0].evaluation = Some(1),
                    24 => flow.sites[0].origin.span.line = 0,
                    25 => flow.sites[0].plan = None,
                    26 => flow.sites[0].cleanup.span = flow.sites[1].cleanup.span,
                    27 => flow.sites[0].transfer.span = flow.sites[1].transfer.span,
                    28 => flow.regions[0].publication.span = flow.regions[1].publication.span,
                    29 => flow.points[0].region = RegionId(1),
                    30 => flow.regions[1].parent = Some(RegionId(0)),
                    31 => flow.points[0].site = Some(SiteId(999)),
                    32 => flow.sites[0].ty = hir::ExprType::Bool,
                    33 => flow.regions[0].roots[0] = SiteId(999),
                    34 => flow.regions[0].retirement.kind = mir::OriginKind::Statement,
                    35 => flow.regions[0].publication.kind = mir::OriginKind::Statement,
                    36 => flow.sites[0].children.push(SiteId(1)),
                    _ => unreachable!(),
                }
            }
            assert!(
                matches!(e.validate().unwrap_err().kind, ErrorKind::Integrity(_)),
                "fault {fault}"
            );
        }
    }

    #[test]
    fn initializer_points_begin_once_and_complete_only_on_selected_leaf() {
        let e = prepare_synthetic(
            hir::resources::fixture_with(
                "fn demo(){let a=acquire();let chosen=if true{a}else{acquire()};}",
            ),
            AdmissionLimits::default(),
        )
        .unwrap();
        let flow = e
            .mir
            .functions()
            .last()
            .unwrap()
            .temporaries
            .as_ref()
            .unwrap();
        let points: Vec<_> = flow
            .points
            .iter()
            .filter(|p| p.region == RegionId(1))
            .collect();
        assert_eq!(points.len(), 3);
        assert_eq!(points.iter().filter(|p| p.begin).count(), 1);
        assert_eq!(points.iter().filter(|p| p.complete).count(), 2);
        assert!(points[0].begin && !points[0].complete);
        assert!(points[1..].iter().all(|p| !p.begin && p.complete));
        assert_ne!(points[0].block, points[1].block);
    }

    #[test]
    fn source_evaluation_ordinals_include_resource_free_predecessors() {
        let e = prepare_synthetic(
            hir::resources::fixture_with(
                "fn demo(){let flag=true;let a=acquire();let unused=false;release(a);}",
            ),
            AdmissionLimits::default(),
        )
        .unwrap();
        let f = e.mir.functions().last().unwrap();
        let flow = f.temporaries.as_ref().unwrap();
        assert_eq!(
            flow.regions
                .iter()
                .map(|r| r.evaluation)
                .collect::<Vec<_>>(),
            vec![Some(1), Some(3)]
        );
        assert_eq!(
            flow.points.iter().map(|p| p.statement).collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn each_temporary_authority_allocation_failure_preserves_host_classification() {
        let p = hir::resources::fixture_with(
            "fn take(a:Leaf,b:Leaf){}fn demo(){take(acquire(),acquire());}",
        );
        let mut success = None;
        for cut in 0..512 {
            mir::temporaries::FAIL_ALLOCATION.with(|f| f.set(Some(cut)));
            let result = prepare_synthetic(p.clone(), AdmissionLimits::default());
            mir::temporaries::FAIL_ALLOCATION.with(|f| f.set(None));
            match result {
                Ok(_) => {
                    success = Some(cut);
                    break;
                }
                Err(e) => assert!(matches!(e.kind, ErrorKind::Host(_)), "cut {cut}: {e:?}"),
            }
        }
        assert!(success.unwrap() > 10);
    }

    #[test]
    fn exact_admission_work_and_storage_neighbors_keep_typed_limits() {
        let p = hir::resources::fixture_with(
            "fn take(a:Leaf,b:Leaf){}fn demo(){take(acquire(),acquire());}",
        );
        for work in [false, true] {
            let default = AdmissionLimits::default();
            let mut low = 1u64;
            let mut high = if work {
                default.frontend_work
            } else {
                default.working_storage as u64
            };
            while low < high {
                let mid = low + (high - low) / 2;
                let mut limits = default;
                if work {
                    limits.frontend_work = mid;
                } else {
                    limits.working_storage = mid as usize;
                }
                match prepare_synthetic(p.clone(), limits) {
                    Ok(_) => high = mid,
                    Err(e) => {
                        assert!(matches!(e.kind, ErrorKind::ConfiguredLimit(_)), "{e:?}");
                        low = mid + 1;
                    }
                }
            }
            let mut limits = default;
            if work {
                limits.frontend_work = low;
            } else {
                limits.working_storage = low as usize;
            }
            assert!(prepare_synthetic(p.clone(), limits).is_ok());
            if work {
                limits.frontend_work -= 1;
            } else {
                limits.working_storage -= 1;
            }
            assert!(matches!(
                prepare_synthetic(p.clone(), limits).err().unwrap().kind,
                ErrorKind::ConfiguredLimit(_)
            ));
        }
    }
}
