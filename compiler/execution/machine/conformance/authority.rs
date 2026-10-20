//! Structured static authority in its own evidence account. This does not call
//! the public text audit, inspect runtime events or confer execution authority.
use super::*;
use crate::effects::calls::Callee;
use crate::trusted::{approval, Catalog, Operation};

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub rows: usize,
    pub work: usize,
    pub storage: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            rows: 65_536,
            work: 1_000_000,
            storage: 16_777_216,
        }
    }
}
#[derive(Clone, Copy)]
enum Row<'a> {
    Schema(contract::Schema),
    Types(&'a hir::Program),
    Catalog(Option<&'a Catalog>),
    Root(FunctionId),
    Function(&'a crate::mir::MirFunction),
    Operation(&'a Operation, bool, bool),
    Edge(&'a approval::Edge),
    Invocation(&'a approval::Invocation),
    Cleanup(&'a crate::effects::custody::Site),
    Temporary(FunctionId, &'a crate::mir::temporaries::Site),
}
impl Row<'_> {
    fn same(self, other: Self) -> bool {
        match (self, other) {
            (Self::Schema(a), Self::Schema(b)) => a == b,
            (Self::Types(a), Self::Types(b)) => std::ptr::eq(a, b),
            (Self::Catalog(a), Self::Catalog(b)) => match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => std::ptr::eq(a, b),
                _ => false,
            },
            (Self::Root(a), Self::Root(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => std::ptr::eq(a, b),
            (Self::Operation(a, x, y), Self::Operation(b, u, v)) => {
                std::ptr::eq(a, b) && x == u && y == v
            }
            (Self::Edge(a), Self::Edge(b)) => std::ptr::eq(a, b),
            (Self::Invocation(a), Self::Invocation(b)) => std::ptr::eq(a, b),
            (Self::Cleanup(a), Self::Cleanup(b)) => std::ptr::eq(a, b),
            (Self::Temporary(a, x), Self::Temporary(b, y)) => a == b && std::ptr::eq(x, y),
            _ => false,
        }
    }
}
pub(super) struct View<'a> {
    contract: &'a contract::Contract<'a>,
    rows: Vec<Row<'a>>,
    limits: Limits,
}
struct Builder<'a> {
    view: View<'a>,
    work: usize,
}
impl<'a> Builder<'a> {
    fn spend(&mut self, n: usize) -> Result<(), Incomplete> {
        self.work = self.work.checked_sub(n).ok_or(Incomplete::Work)?;
        Ok(())
    }
    fn push(&mut self, row: Row<'a>) -> Result<(), Incomplete> {
        self.spend(1)?;
        if self.view.rows.len() == self.view.limits.rows {
            return Err(Incomplete::Records);
        }
        self.view.rows.push(row);
        Ok(())
    }
}
impl<'a> View<'a> {
    pub(super) fn extract(
        contract: &'a contract::Contract<'a>,
        limits: Limits,
    ) -> Result<Self, Incomplete> {
        contract.validate().map_err(|error| match error.kind {
            ErrorKind::Host(_) => Incomplete::Allocation,
            ErrorKind::ConfiguredLimit(_) => Incomplete::Admission,
            _ => Incomplete::Authority,
        })?;
        let e = contract.executable;
        let count = e.program.functions().len();
        let bytes = limits
            .rows
            .checked_mul(size_of::<Row<'_>>())
            .and_then(|n| n.checked_add(count))
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .and_then(|n| n.checked_mul(2));
        if limits.rows == 0
            || limits.rows > 65_536
            || limits.work == 0
            || limits.work > 100_000_000
            || limits.storage > 67_108_864
            || bytes.map_or(true, |bytes| bytes > limits.storage)
        {
            return Err(Incomplete::Admission);
        }
        let mut b = Builder {
            view: Self {
                contract,
                rows: Vec::new(),
                limits,
            },
            work: limits.work,
        };
        b.spend(
            limits
                .rows
                .checked_mul(2)
                .and_then(|n| n.checked_add(count))
                .ok_or(Incomplete::Work)?,
        )?;
        b.view
            .rows
            .try_reserve_exact(limits.rows)
            .map_err(|_| Incomplete::Allocation)?;
        let mut reachable = Vec::new();
        reachable
            .try_reserve_exact(count)
            .map_err(|_| Incomplete::Allocation)?;
        reachable.resize(count, 0u8);
        b.push(Row::Schema(contract.schema()))?;
        b.push(Row::Types(&e.program))?;
        b.push(Row::Catalog(e.trusted.as_ref().map(|t| &t.catalog)))?;
        if let Some(approved) = &e.trusted {
            for root in &approved.roots {
                b.push(Row::Root(*root))?;
                reachable[root.0] = 1;
            }
            for _ in 0..count {
                let mut changed = false;
                for edge in &approved.edges {
                    b.spend(1)?;
                    if reachable[edge.caller.0] != 0 {
                        if let Callee::Source(callee) = edge.callee {
                            if reachable[callee.0] == 0 {
                                reachable[callee.0] = 1;
                                changed = true;
                            }
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            let mut used = [false; 4];
            for edge in &approved.edges {
                b.spend(1)?;
                if reachable[edge.caller.0] != 0 {
                    b.push(Row::Edge(edge))?;
                    if let Callee::Trusted(operation) = edge.callee {
                        for (index, description) in approved.catalog.operations.iter().enumerate() {
                            b.spend(1)?;
                            used[index] |= description.id == operation;
                        }
                    }
                }
            }
            for (index, operation) in approved.catalog.operations.iter().enumerate() {
                b.push(Row::Operation(
                    operation,
                    approved.catalog.granted(operation.id),
                    used[index],
                ))?;
            }
            for invocation in &approved.invocations {
                b.spend(1)?;
                if reachable[invocation.function.0] != 0 {
                    b.push(Row::Invocation(invocation))?;
                }
            }
        } else {
            b.push(Row::Root(contract.entry()))?;
            for function in contract.functions() {
                b.spend(1)?;
                reachable[function.0] = 1;
            }
        }
        for function in e.mir.functions() {
            b.spend(1)?;
            if reachable[function.function().0] == 0 {
                continue;
            }
            b.push(Row::Function(function))?;
            for site in &function.cleanup_effect_sites {
                b.push(Row::Cleanup(site))?;
            }
            if let Some(flow) = &function.temporaries {
                for site in &flow.sites {
                    b.spend(1)?;
                    if site.plan.is_some() || site.result_plan.is_some() {
                        b.push(Row::Temporary(function.function(), site))?;
                    }
                }
            }
        }
        Ok(b.view)
    }
    pub(super) fn validate(&self) -> Result<(), Incomplete> {
        let expected = Self::extract(self.contract, self.limits)?;
        if self.rows.len() != expected.rows.len()
            || !self
                .rows
                .iter()
                .copied()
                .zip(expected.rows)
                .all(|(a, b)| a.same(b))
        {
            return Err(Incomplete::InvalidState);
        }
        Ok(())
    }
    pub(super) fn equivalent(&self, other: &Self) -> Result<bool, Incomplete> {
        if !self.contract.same_approval(other.contract) {
            return Err(Incomplete::Authority);
        }
        self.validate()?;
        other.validate()?;
        Ok(self.rows.len() == other.rows.len()
            && self
                .rows
                .iter()
                .copied()
                .zip(other.rows.iter().copied())
                .all(|(a, b)| a.same(b)))
    }
    pub(super) fn comparison_work(&self) -> usize {
        self.rows.len() + 1
    }
    pub(super) fn render_summary(
        &self,
        allowance: usize,
        verbose: bool,
    ) -> Result<String, ExecutionError> {
        use std::fmt::Write;
        self.validate().map_err(|reason| match reason {
            Incomplete::Allocation => ExecutionError::new(
                ErrorKind::Host("structured audit evidence allocation"),
                START,
            ),
            Incomplete::Admission | Incomplete::Records | Incomplete::Work => ExecutionError::new(
                ErrorKind::ConfiguredLimit("structured audit evidence"),
                START,
            ),
            _ => ExecutionError::integrity("structured audit correspondence"),
        })?;
        let mut output = crate::execution::render::Output::new(allowance)?;
        let _ = writeln!(output, "structured authority v1: rows={}", self.rows.len());
        if verbose {
            for row in &self.rows {
                if let Row::Root(function) = row {
                    let _ = writeln!(output, "root {}", function.0);
                }
            }
        }
        output.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_profile_structural_content_includes_empty_effect_calls_without_catalog_authority() {
        let e=prepare_execution("fn helper()->bool{return true;}fn demo()->bool{return helper();}fn uncalled()->bool{return false;}",AdmissionLimits::default()).unwrap();
        let c =
            contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default())
                .unwrap();
        let a = View::extract(&c, Limits::default()).unwrap();
        let b = View::extract(&c, Limits::default()).unwrap();
        assert!(a.equivalent(&b).unwrap());
        let functions: Vec<_> = a
            .rows
            .iter()
            .filter_map(|row| {
                if let Row::Function(function) = row {
                    Some(function.function())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            functions,
            vec![e.program.functions()[0].id(), e.program.functions()[1].id()]
        );
        assert!(a.rows.iter().any(|row| matches!(row, Row::Catalog(None))));
        assert!(!a.rows.iter().any(|row| matches!(row, Row::Operation(..))));
        assert!(e.render_authority(ExecutionLimits::default()).is_err());
    }
    #[test]
    fn selected_static_possible_protocols_survive_runtime_skip_and_exact_row_corruption_rejects() {
        let source="fn unused()->Result<success:bool,failure:SensorError>effects io{let sensor=sensor_open()or return;return Ok(true);}fn demo()->Result<success:bool,failure:SensorError>effects io{if false{let sensor=sensor_open()or return;}return Ok(true);}";
        let selection = crate::trusted::Selection::sensor_demo(
            crate::trusted::Scenario::Sample,
            crate::trusted::Grants::ALL,
            &["demo"],
        )
        .unwrap();
        let e =
            prepare_execution_with_profile(source, AdmissionLimits::default(), &selection).unwrap();
        let c =
            contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default())
                .unwrap();
        let mut session = Session::new(
            &c,
            ExecutionLimits::default(),
            super::super::Limits::default(),
        )
        .unwrap();
        session.advance().unwrap();
        let Engine::Selected(actual) = &session.engine else {
            unreachable!()
        };
        assert!(actual.events().is_empty());
        let view = session.authority.as_ref().unwrap();
        assert_eq!(
            view.rows
                .iter()
                .filter(|row| matches!(row, Row::Operation(..)))
                .count(),
            4
        );
        assert_eq!(
            view.rows
                .iter()
                .filter(|row| matches!(row, Row::Invocation(..)))
                .count(),
            1
        );
        let mut malformed = View::extract(&c, Limits::default()).unwrap();
        let operation = malformed
            .rows
            .iter_mut()
            .find(|row| matches!(row, Row::Operation(..)))
            .unwrap();
        let Row::Operation(_, granted, _) = operation else {
            unreachable!()
        };
        *granted = !*granted;
        assert_eq!(malformed.validate().err(), Some(Incomplete::InvalidState));
        assert!(matches!(
            view.equivalent(&malformed),
            Err(Incomplete::InvalidState)
        ));
    }
    #[test]
    fn structured_evidence_limits_and_bounded_presentation_never_replace_primary_execution() {
        let e =
            prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        let c =
            contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default())
                .unwrap();
        let view = View::extract(&c, Limits::default()).unwrap();
        let count = view.rows.len();
        let bytes = 2
            * (count * size_of::<Row<'_>>() + e.program.functions().len() + size_of::<View<'_>>());
        let exact = Limits {
            rows: count,
            storage: bytes,
            ..Limits::default()
        };
        assert!(View::extract(&c, exact).is_ok());
        assert_eq!(
            View::extract(
                &c,
                Limits {
                    storage: bytes - 1,
                    ..exact
                }
            )
            .err()
            .unwrap(),
            Incomplete::Admission
        );
        assert_eq!(
            View::extract(
                &c,
                Limits {
                    rows: count - 1,
                    ..Limits::default()
                }
            )
            .err()
            .unwrap(),
            Incomplete::Records
        );
        let mut a = Session::new(
            &c,
            ExecutionLimits::default(),
            super::super::Limits::default(),
        )
        .unwrap();
        a.advance().unwrap();
        let text = a.render_authority_summary(4096, false).unwrap();
        let verbose = a.render_authority_summary(4096, true).unwrap();
        assert_ne!(text, verbose);
        assert_eq!(a.render_authority_summary(text.len(), false).unwrap(), text);
        assert!(a.render_authority_summary(text.len() - 1, false).is_err());
        let error = crate::execution::render::with_allocation_failure(|| {
            a.render_authority_summary(4096, false)
        })
        .unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Host(_)));
        assert_eq!(
            a.primary().unwrap().unwrap(),
            &ExecutionResult::Value(RuntimeValue::Bool(true))
        );
        let mut b = Session::new(
            &c,
            ExecutionLimits::default(),
            super::super::Limits::default(),
        )
        .unwrap();
        b.advance().unwrap();
        assert_eq!(
            compare(&a, &b, ComparisonLimits::default()),
            Comparison::Equal
        );
        b.authority = None;
        assert!(matches!(
            compare(&a, &b, ComparisonLimits::default()),
            Comparison::Inconclusive(_)
        ));
        assert_eq!(
            b.primary().unwrap().unwrap(),
            &ExecutionResult::Value(RuntimeValue::Bool(true))
        );
    }
}
