//! Cleanup may-realization evidence emitted inside the existing ownership checker.
//! This product grants no move, loan, borrowing or replacement permission.
use super::EffectSet;
use crate::hir::resources::CleanupType;
use crate::{
    diagnostics::Diagnostic,
    hir::{FunctionId, Place, Program, RecordId, ValueType},
    lexer::Span,
};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Fallthrough,
    Completion,
    BareReturn,
    Return,
    Break,
    Continue,
    Iteration,
    Replacement,
    CompletionRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Holder {
    Place(Place),
    Invocation(crate::hir::TrustedOperationId),
}

impl PartialEq<Place> for Holder {
    fn eq(&self, other: &Place) -> bool {
        matches!(self, Self::Place(place) if place == other)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site {
    pub(crate) function: FunctionId,
    pub(crate) holder: Holder,
    pub(crate) plan: CleanupType,
    pub(crate) effects: EffectSet,
    pub(crate) kind: Kind,
    pub(crate) origin: Span,
}

impl Site {
    pub(crate) fn order(&self) -> (FunctionId, usize, usize, usize, usize, Kind, Holder) {
        (
            self.function,
            self.origin.start,
            self.origin.end,
            self.origin.line,
            self.origin.column,
            self.kind,
            self.holder,
        )
    }
}

// Canonical ordering does not depend on traversal or diagnostic spellings.
type Key = (usize, usize, usize, usize, Kind, Holder);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Custody {
    function: FunctionId,
    summaries: Rc<Vec<EffectSet>>,
    live: BTreeMap<Place, CleanupType>,
    resource: Option<(crate::hir::ResourceTypeId, EffectSet)>,
    sites: BTreeMap<Key, Site>,
}

impl Custody {
    pub(crate) fn new(
        function: FunctionId,
        summaries: Rc<Vec<EffectSet>>,
        resource: Option<(crate::hir::ResourceTypeId, EffectSet)>,
    ) -> Self {
        Self {
            function,
            summaries,
            resource,
            live: BTreeMap::new(),
            sites: BTreeMap::new(),
        }
    }

    pub(crate) fn initialize(&mut self, place: Place, ty: ValueType) {
        if let ValueType::Record(record) = ty {
            if self
                .summaries
                .get(record.0)
                .is_some_and(|s| *s != EffectSet::EMPTY)
            {
                self.live.insert(place, CleanupType::Record(record));
            }
        } else if let ValueType::Resource(id) = ty {
            if self
                .resource
                .is_some_and(|(resource, effects)| resource == id && effects != EffectSet::EMPTY)
            {
                self.live.insert(place, CleanupType::Resource(id));
            }
        }
    }

    /// Called only after the existing canonical consume gate succeeds.
    pub(crate) fn transfer(&mut self, place: Place) {
        self.live.remove(&place);
    }

    pub(crate) fn retire(
        &mut self,
        mut eligible: impl FnMut(Place) -> bool,
        origin: Span,
        kind: Kind,
    ) {
        let sites = &mut self.sites;
        let function = self.function;
        let summaries = &self.summaries;
        let resource = self.resource;
        self.live.retain(|place, record| {
            if !eligible(*place) {
                return true;
            }
            let key = (
                origin.start,
                origin.end,
                origin.line,
                origin.column,
                kind,
                Holder::Place(*place),
            );
            sites.insert(
                key,
                Site {
                    function,
                    holder: Holder::Place(*place),
                    plan: *record,
                    effects: match *record {
                        CleanupType::Record(id) => summaries[id.0],
                        CleanupType::Resource(id) => {
                            resource
                                .filter(|(r, _)| *r == id)
                                .expect("custody retains checked resource summary")
                                .1
                        }
                    },
                    kind,
                    origin,
                },
            );
            false
        });
    }

    /// The existing checker calls this only after the owned argument has
    /// passed its ordinary consume/loan/hold gate. This witnesses cleanup;
    /// it confers no source availability or alternative ownership permission.
    pub(crate) fn completion(&mut self, operation: crate::hir::TrustedOperationId, origin: Span) {
        let (resource, effects) = self.resource.expect("selected completion resource");
        let holder = Holder::Invocation(operation);
        let kind = Kind::CompletionRelease;
        let key = (origin.start, origin.end, origin.line, origin.column, kind, holder);
        self.sites.insert(key, Site {
            function: self.function,
            holder,
            plan: CleanupType::Resource(resource),
            effects,
            kind,
            origin,
        });
    }

    pub(crate) fn merge_sites(&mut self, other: &Self) {
        self.sites
            .extend(other.sites.iter().map(|(k, v)| (*k, v.clone())));
    }

    pub(crate) fn branches(
        &mut self,
        yes: &Self,
        no: &Self,
        yes_continues: bool,
        no_continues: bool,
    ) {
        self.live.clear();
        if yes_continues {
            self.live.extend(yes.live.iter().map(|(p, r)| (*p, *r)));
        }
        if no_continues {
            self.live.extend(no.live.iter().map(|(p, r)| (*p, *r)));
        }
        self.merge_sites(yes);
        self.merge_sites(no);
    }

    /// Effects-only skip alternative; predecessor safety usability is unchanged.
    pub(crate) fn retain_skip(&mut self, skip: &Self) {
        self.live.extend(skip.live.iter().map(|(p, r)| (*p, *r)));
        self.merge_sites(skip);
    }

    pub(crate) fn into_sites(self) -> impl ExactSizeIterator<Item = Site> {
        self.sites.into_values()
    }
}

pub(crate) fn holder_bound(program: &Program) -> usize {
    program
        .functions()
        .iter()
        .filter_map(|f| f.as_ordinary())
        .map(|f| {
            f.parameters.len()
                + program
                    .locals()
                    .iter()
                    .filter(|l| l.function == f.id)
                    .count()
        })
        .max()
        .unwrap_or(0)
        + usize::from(program.trusted.is_some())
}

#[cfg(test)]
thread_local! {
    pub(crate) static FAIL_WITNESS_ALLOCATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn reserve_output(
    output: &mut Vec<Site>,
    count: usize,
) -> Result<(), crate::hir::resources::PlanError> {
    #[cfg(test)]
    if count > 0 && FAIL_WITNESS_ALLOCATION.with(std::cell::Cell::get) {
        return Err(crate::hir::resources::PlanError::Host(
            "cleanup effect witness allocation",
        ));
    }
    output
        .try_reserve_exact(count)
        .map_err(|_| crate::hir::resources::PlanError::Host("cleanup effect witness allocation"))
}

pub(crate) fn check(program: &Program, sites: &[Site]) -> Result<(), Vec<Diagnostic>> {
    for site in sites {
        let function = program
            .function(site.function)
            .as_ordinary()
            .expect("checked ordinary function");
        if site.effects.is_subset(function.effects.set()) {
            continue;
        }
        let missing = site
            .effects
            .iter()
            .find(|e| !function.effects.set().contains(*e))
            .unwrap();
        if let CleanupType::Resource(id) = site.plan {
            let catalog = program
                .trusted
                .as_ref()
                .filter(|c| c.sensor == id)
                .expect("checked resource cleanup witness");
            return Err(vec![Diagnostic::semantic(site.origin,
                format!("unadmitted effect `{}` in `{}`: {:?} cleanup of `Sensor` derives from its complete canonical release contract (catalog {} row {}; no source declaration)", missing.name(), function.name, site.kind, crate::trusted::CATALOG, catalog.resource.origin.row),
                "admit the canonical cleanup summary at this realization; transferred and protected results remain excluded")]);
        }
        let CleanupType::Record(record_id) = site.plan else {
            unreachable!()
        };
        let record = program.record(record_id);
        fn release_origin(
            program: &Program,
            record: RecordId,
            effect: super::Effect,
            budget: &mut usize,
        ) -> Option<(FunctionId, Span)> {
            if *budget == 0 {
                return None;
            }
            *budget -= 1;
            if let Some(leaf) = program.resources.leaves.iter().find(|l| l.record == record) {
                if leaf.cleanup_effects.contains(effect) {
                    let operation = &program.resources.operations[leaf.release.0];
                    let function = program.function(operation.function).as_ordinary()?;
                    return Some((
                        function.id,
                        function.effects.origin(effect).unwrap_or(function.span),
                    ));
                }
                return None;
            }
            for field in program.record(record).fields.iter().rev() {
                if *budget == 0 {
                    break;
                }
                *budget -= 1;
                if let ValueType::Record(child) = program.field(*field).ty {
                    if let Some(origin) = release_origin(program, child, effect, budget) {
                        return Some(origin);
                    }
                }
            }
            None
        }
        let trace = if let Some((id, origin)) = release_origin(program, record_id, missing, &mut 16)
        {
            format!(
                "release declaration `{}` at {}:{}",
                program.function(id).name(),
                origin.line,
                origin.column
            )
        } else {
            "canonical type cleanup origin; ancestry truncated at the bounded diagnostic budget"
                .into()
        };
        return Err(vec![Diagnostic::semantic(
            site.origin,
            format!(
                "unadmitted effect `{}` in `{}`: {:?} cleanup of `{}` derives from its complete canonical release contract ({trace}; type declared at {}:{})",
                missing.name(), function.name, site.kind, record.name, record.span.line, record.span.column
            ),
            "admit the canonical cleanup summary at this realization; transferred and protected results remain excluded",
        )]);
    }
    Ok(())
}
