//! Prepared whole-owner replacement. No fallible transition loses either payload.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Rhs,
    Protected,
    Prepared,
    Detached,
    Discharged,
}
pub(in crate::execution::machine) struct Prepared {
    id: u64,
    at: Location,
    record: hir::RecordId,
    scope: ScopeId,
    activation: u64,
    old_generation: Option<u64>,
    phase: Phase,
    pub(super) protected: Option<Value>,
    pub(super) displaced: Option<Value>,
}
impl Prepared {
    pub(super) fn payload_storage(&self) -> Option<usize> {
        self.protected
            .as_ref()
            .map_or(0, Value::extra_storage)
            .checked_add(self.displaced.as_ref().map_or(0, Value::extra_storage))
    }
}
fn record(value: &Value) -> Option<hir::RecordId> {
    match value {
        Value::Leaf { record, .. }
        | Value::Aggregate { record, .. }
        | Value::Public(RuntimeValue::Record { record, .. }) => Some(*record),
        _ => None,
    }
}
impl<'a> Machine<'a> {
    pub(in crate::execution::machine) fn replacement_start(
        &mut self,
        at: Location,
        ty: hir::RecordId,
        scope: usize,
        plan: Option<hir::RecordId>,
    ) -> Result<u64, ExecutionError> {
        self.charge()?;
        if record(self.slot(at)?) != Some(ty)
            || plan
                != self
                    .executable
                    .cleanup_plans
                    .get(ty.0)
                    .and_then(Option::as_ref)
                    .map(|_| ty)
        {
            return Err(self.integrity("replacement target or canonical plan mismatch"));
        }
        let r = self.resource_frame_index(at.frame)?;
        let frame = &self.resources.as_ref().unwrap().frames[r];
        let activation = frame
            .scopes
            .iter()
            .find(|a| a.scope == ScopeId(scope))
            .ok_or_else(|| self.integrity("replacement owning activation missing"))?;
        let Place::Local(local) = at.place else {
            return Err(self.integrity("replacement target is not local"));
        };
        let old_generation = if plan.is_some() {
            let current = frame
                .generations
                .iter()
                .find(|(l, _)| *l == local)
                .map(|(_, g)| *g)
                .ok_or_else(|| self.integrity("replacement generation missing"))?;
            if !activation
                .initialized
                .iter()
                .any(|g| g.local == local && g.identity == current && !g.discharged)
            {
                return Err(self.integrity("replacement generation does not own target slot"));
            }
            Some(current)
        } else {
            None
        };
        let activation = activation.identity;
        let s = self.resources.as_ref().unwrap();
        let id = s.next_replacement;
        let next = id
            .checked_add(1)
            .ok_or_else(|| self.integrity("replacement identity exhausted"))?;
        let grow = s.replacements.len() == s.replacements.capacity();
        if grow {
            self.reserve(size_of::<Prepared>())?;
            self.resources
                .as_mut()
                .unwrap()
                .replacements
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("replacement context allocation")))?;
        }
        let s = self.resources.as_mut().unwrap();
        s.next_replacement = next;
        s.replacements.push(Prepared {
            id,
            at,
            record: ty,
            scope: ScopeId(scope),
            activation,
            old_generation,
            phase: Phase::Rhs,
            protected: None,
            displaced: None,
        });
        Ok(id)
    }
    pub(in crate::execution::machine) fn replacement_commit(
        &mut self,
        id: u64,
    ) -> Result<(), ExecutionError> {
        let context = self
            .resources
            .as_ref()
            .and_then(|s| s.replacements.last())
            .filter(|r| r.id == id && r.phase == Phase::Rhs)
            .ok_or_else(|| self.integrity("replacement continuation mismatch"))?;
        let at = context.at;
        let ty = context.record;
        let activation = context.activation;
        let scope = context.scope;
        let old_generation = context.old_generation;
        // No fallible operation between taking N from the value stack and
        // protecting it in the already-reserved context.
        let value = self.pop()?;
        self.resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap()
            .protected = Some(value);
        self.resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap()
            .phase = Phase::Protected;
        self.charge()?;
        if record(
            self.resources
                .as_ref()
                .unwrap()
                .replacements
                .last()
                .unwrap()
                .protected
                .as_ref()
                .unwrap(),
        ) != Some(ty)
            || record(self.slot(at)?) != Some(ty)
        {
            return Err(self.integrity("replacement result or displaced type mismatch"));
        }
        let r = self.resource_frame_index(at.frame)?;
        let a = self.resources.as_ref().unwrap().frames[r]
            .scopes
            .iter()
            .position(|a| a.identity == activation && a.scope == scope)
            .ok_or_else(|| self.integrity("replacement owning activation changed during RHS"))?;
        let Place::Local(local) = at.place else {
            unreachable!()
        };
        let old = if let Some(old_generation) = old_generation {
            let frame = &self.resources.as_ref().unwrap().frames[r];
            if frame
                .generations
                .iter()
                .find(|(l, _)| *l == local)
                .map(|(_, g)| *g)
                != Some(old_generation)
            {
                return Err(self.integrity("replacement slot generation changed during RHS"));
            }
            Some(
                frame.scopes[a]
                    .initialized
                    .iter()
                    .position(|g| g.identity == old_generation && g.local == local && !g.discharged)
                    .ok_or_else(|| self.integrity("replacement old generation is not live"))?,
            )
        } else {
            None
        };
        let next_generation = if old.is_some() {
            let history = &self.resources.as_ref().unwrap().frames[r].scopes[a].initialized;
            if history.len() == history.capacity() {
                self.reserve(size_of::<Generation>())?;
                self.resources.as_mut().unwrap().frames[r].scopes[a]
                    .initialized
                    .try_reserve_exact(1)
                    .map_err(|_| {
                        self.failure(ErrorKind::Host("replacement generation allocation"))
                    })?;
            }
            let g = self.resources.as_ref().unwrap().next_generation;
            g.checked_add(1)
                .ok_or_else(|| self.integrity("replacement generation exhausted"))?;
            Some(g)
        } else {
            None
        };
        self.resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap()
            .phase = Phase::Prepared;
        self.charge()?;
        let (f, slot) = self.slot_index(at)?;
        let Slot::Available(displaced) =
            std::mem::replace(&mut self.frames[f].slots[slot].1, Slot::Replacing)
        else {
            return Err(self.integrity("replacement target lost complete owner"));
        };
        let context = self
            .resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap();
        context.displaced = Some(displaced);
        context.phase = Phase::Detached;
        self.charge()?;
        // Cleanup has its own retained cursors; protected N remains independent
        // from returns, pending aggregates, detached publication and work buffers.
        let displaced = self
            .resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap()
            .displaced
            .take()
            .unwrap();
        if old.is_some() {
            self.release_value(displaced)?;
        }
        if let Some(old) = old {
            self.resources.as_mut().unwrap().frames[r].scopes[a].initialized[old].discharged = true;
        }
        self.resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap()
            .phase = Phase::Discharged;
        self.charge()?;
        self.reserve(0)?;
        // All storage and fallible validation precedes publication. Slot install,
        // association update and history append are one allocation-free transition.
        let value = self
            .resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap()
            .protected
            .take()
            .unwrap();
        self.frames[f].slots[slot].1 = Slot::Available(value);
        if let Some(identity) = next_generation {
            let s = self.resources.as_mut().unwrap();
            s.next_generation = identity + 1;
            let frame = &mut s.frames[r];
            *frame
                .generations
                .iter_mut()
                .find(|(l, _)| *l == local)
                .unwrap() = (local, identity);
            frame.scopes[a].initialized.push(Generation {
                local,
                identity,
                discharged: false,
            });
        }
        self.resources.as_mut().unwrap().replacements.pop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn artifact(body: &str) -> CheckedExecutable {
        crate::execution::prepare_synthetic(
            hir::resources::fixture_with(body),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    fn run(e: &CheckedExecutable) -> Machine<'_> {
        let mut m = Machine::new(e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert_eq!(m.resume().unwrap(), ExecutionResult::NoValue);
        super::super::conservation_tests::conserved(&m);
        m
    }
    #[test]
    fn abc_generation_order_has_exact_acquisition_and_release_identities() {
        let e = artifact("fn acquire_a()->Leaf{return acquire();}fn acquire_b()->Leaf{return acquire();}fn acquire_c()->Leaf{return acquire();}fn demo(){let mut a=acquire_a();let b=acquire_b();a=acquire_c();}");
        let m = run(&e);
        let events: Vec<_> = m
            .resources
            .unwrap()
            .events
            .into_iter()
            .filter(|e| !matches!(e, Event::Retire(_)))
            .collect();
        assert_eq!(
            events,
            vec![
                Event::Acquire(0),
                Event::Acquire(1),
                Event::Acquire(2),
                Event::Attempt(0),
                Event::Complete(0),
                Event::Attempt(2),
                Event::Complete(2),
                Event::Attempt(1),
                Event::Complete(1)
            ]
        );
    }
    #[test]
    fn inert_repeated_replacement_executes_without_release() {
        let e = crate::execution::prepare_execution("type P=range 0..3;record R{x:P}fn demo()->R{let mut a=R{x:0};a=R{x:1};let b=R{x:2};a=b;return a;}", AdmissionLimits::default()).unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert!(
            matches!(m.resume().unwrap(), ExecutionResult::Value(RuntimeValue::Record { fields, .. }) if fields[0].1.value == 2)
        );
        assert!(!m
            .resources
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::Attempt(_))));
    }
    #[test]
    fn distinct_source_transfer_and_repeated_replacement_release_once() {
        let e = artifact("fn demo(){let mut a=acquire();let b=acquire();a=b;a=acquire();}");
        let m = run(&e);
        let completed: Vec<_> = m
            .resources
            .unwrap()
            .events
            .into_iter()
            .filter_map(|e| {
                if let Event::Complete(id) = e {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(completed, vec![0, 1, 2]);
    }
    #[test]
    fn every_leaf_replacement_fuel_cut_retains_both_responsibilities() {
        let e = artifact("fn demo(){let mut a=acquire();let b=acquire();a=b;}");
        let full = run(&e);
        for fuel in 1..full.steps {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    steps: fuel,
                    ..ExecutionLimits::default()
                },
            );
            if m.start(e.entry("demo").unwrap()).is_ok() {
                assert!(m.resume().is_err());
                if m.resources.is_some() {
                    super::super::conservation_tests::conserved(&m);
                    assert!(m.resume().is_err());
                }
            }
        }
        let mut exact = Machine::new(
            &e,
            ExecutionLimits {
                steps: full.steps,
                ..ExecutionLimits::default()
            },
        );
        exact.start(e.entry("demo").unwrap()).unwrap();
        assert_eq!(exact.resume().unwrap(), ExecutionResult::NoValue);
    }
    #[test]
    fn displaced_release_host_failure_retains_protected_new_owner() {
        let e = artifact("fn demo(){let mut a=acquire();a=acquire();}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(0);
        assert_eq!(
            m.resume().unwrap_err().kind,
            ErrorKind::Host("synthetic trusted release")
        );
        assert!(matches!(
            m.resources.as_ref().unwrap().replacements[0].protected,
            Some(Value::Leaf { identity: 1, .. })
        ));
        super::super::conservation_tests::conserved(&m);
        assert!(m.resume().is_err());
    }
}
