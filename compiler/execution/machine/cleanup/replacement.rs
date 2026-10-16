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
    pub(in crate::execution::machine) protected: Option<Value>,
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
    fn replacement_charge(&mut self, _phase: Phase) -> Result<(), ExecutionError> {
        self.charge()?;
        #[cfg(test)]
        if self
            .resources
            .as_ref()
            .is_some_and(|s| s.fail_replacement == Some(_phase))
        {
            return Err(self.failure(ErrorKind::Host(
                "synthetic replacement reservation/progress",
            )));
        }
        Ok(())
    }
    pub(in crate::execution::machine) fn replacement_start(
        &mut self,
        at: Location,
        ty: hir::RecordId,
        scope: usize,
        plan: Option<hir::RecordId>,
    ) -> Result<u64, ExecutionError> {
        self.replacement_charge(Phase::Rhs)?;
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
    pub(in crate::execution::machine) fn replacement_accepts_result(
        &self,
        frame: u64,
        local: hir::LocalId,
    ) -> Result<(), ExecutionError> {
        self.resources
            .as_ref()
            .and_then(|s| s.replacements.last())
            .filter(|r| {
                r.at.frame == frame
                    && r.at.place == Place::Local(local)
                    && r.phase == Phase::Rhs
                    && r.protected.is_none()
            })
            .map(|_| ())
            .ok_or_else(|| self.integrity("replacement result protection mismatch"))
    }
    pub(in crate::execution::machine) fn replacement_stage_result(&mut self, value: Value) {
        self.resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap()
            .protected = Some(value);
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
        self.replacement_charge(Phase::Rhs)?;
        if self
            .resources
            .as_ref()
            .unwrap()
            .replacements
            .last()
            .unwrap()
            .protected
            .is_none()
        {
            let value = self.pop()?;
            self.replacement_stage_result(value);
        }
        self.resources
            .as_mut()
            .unwrap()
            .replacements
            .last_mut()
            .unwrap()
            .phase = Phase::Protected;
        self.replacement_charge(Phase::Protected)?;
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
        self.replacement_charge(Phase::Prepared)?;
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
        self.replacement_charge(Phase::Detached)?;
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
        self.replacement_charge(Phase::Discharged)?;
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

#[cfg(test)]
mod conformance_tests {
    use super::super::{aggregate_tests, conservation_tests};
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
        conservation_tests::conserved(&m);
        m
    }
    fn completed(m: &Machine<'_>) -> Vec<u64> {
        m.resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Complete(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }
    fn aggregate() -> CheckedExecutable {
        let p = aggregate_tests::program(
            "fn demo(){let mut a=Pair{last:0,first:0,inert:0};a=Pair{first:0,inert:0,last:0};}",
            [true, false, true],
        );
        crate::execution::prepare_synthetic(p, AdmissionLimits::default()).unwrap()
    }
    fn nested() -> CheckedExecutable {
        let mut p = aggregate_tests::program("record Outer{left:R,right:R}fn make()->Pair{return Pair{last:0,first:0,inert:0};}fn demo(){let mut a=Outer{right:0,left:0};a=Outer{left:0,right:0};}", [true,false,true]);
        for field in &mut p.fields[3..] {
            field.ty = ValueType::Record(hir::RecordId(1));
        }
        let make = p
            .functions
            .iter()
            .find(|f| f.name() == "make")
            .unwrap()
            .id();
        for f in &mut p.functions {
            if let hir::Function::Ordinary(f) = f {
                for s in &mut f.body {
                    let value = match s {
                        hir::ValueStatement::Let { initializer, .. } => initializer,
                        hir::ValueStatement::Assign { value, .. } => value,
                        _ => continue,
                    };
                    if let hir::ExprKind::RecordConstruct { record, fields } = &mut value.kind {
                        if *record == hir::RecordId(2) {
                            for field in fields {
                                field.value.kind = hir::ExprKind::Call {
                                    function: make,
                                    arguments: Vec::new(),
                                };
                                field.value.ty = hir::ExprType::Record(hir::RecordId(1));
                            }
                        }
                    }
                }
            }
        }
        crate::execution::prepare_synthetic(p, AdmissionLimits::default()).unwrap()
    }
    fn recursive() -> CheckedExecutable {
        artifact("type N=range 0..3;fn make(n:N)->Leaf{let mut a=acquire();if n==0{return a;}a=make(n-1);return a;}fn demo(){let mut root=acquire();root=make(3);}")
    }
    fn all_fuel_cuts(e: &CheckedExecutable) {
        let full = run(e);
        for fuel in 1..full.steps {
            let mut m = Machine::new(
                e,
                ExecutionLimits {
                    steps: fuel,
                    ..ExecutionLimits::default()
                },
            );
            if m.start(e.entry("demo").unwrap()).is_ok() {
                assert!(matches!(
                    m.resume().unwrap_err().kind,
                    ErrorKind::ConfiguredLimit("steps")
                ));
                if m.resources.is_some() {
                    conservation_tests::conserved(&m);
                }
                assert!(m.resume().is_err());
            }
        }
        let mut exact = Machine::new(
            e,
            ExecutionLimits {
                steps: full.steps,
                ..ExecutionLimits::default()
            },
        );
        exact.start(e.entry("demo").unwrap()).unwrap();
        assert_eq!(exact.resume().unwrap(), ExecutionResult::NoValue);
        conservation_tests::conserved(&exact);
    }
    #[test]
    fn aggregate_displacement_uses_reverse_canonical_field_order() {
        assert_eq!(completed(&run(&aggregate())), vec![0, 1, 3, 2]);
        assert_eq!(completed(&run(&nested())), vec![0, 1, 2, 3, 6, 7, 4, 5]);
    }
    #[test]
    fn recursive_rhs_keeps_outer_contexts_and_protected_returns_distinct() {
        let e = recursive();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let mut depth = 0;
        while let Some(task) = m.tasks.pop() {
            m.step(task).unwrap();
            conservation_tests::conserved(&m);
            depth = depth.max(m.resources.as_ref().unwrap().replacements.len());
        }
        assert!(depth >= 4);
        assert_eq!(completed(&m), vec![3, 2, 1, 0, 4]);
    }
    #[test]
    fn aggregate_nested_and_recursive_rhs_conserve_at_every_fuel_cut() {
        for e in [aggregate(), nested(), recursive()] {
            all_fuel_cuts(&e);
        }
    }
    #[test]
    fn every_byte_storage_cut_preserves_all_acquired_identities() {
        for e in [aggregate(), recursive()] {
            let full = run(&e);
            let required = full.max_storage.get();
            for budget in 1..=required {
                let mut m = Machine::new(
                    &e,
                    ExecutionLimits {
                        working_storage: budget,
                        ..ExecutionLimits::default()
                    },
                );
                let result = m.start(e.entry("demo").unwrap()).and_then(|_| m.resume());
                if budget == required {
                    assert_eq!(result.unwrap(), ExecutionResult::NoValue);
                } else {
                    assert!(
                        matches!(
                            result.unwrap_err().kind,
                            ErrorKind::ConfiguredLimit("working storage")
                        ),
                        "budget {budget}"
                    );
                }
                if m.resources.is_some() {
                    conservation_tests::conserved(&m);
                }
            }
        }
    }
    #[test]
    fn host_failure_at_each_prepared_phase_retains_old_and_new_owners() {
        for phase in [
            Phase::Rhs,
            Phase::Protected,
            Phase::Prepared,
            Phase::Detached,
            Phase::Discharged,
        ] {
            let e = artifact("fn demo(){let mut a=acquire();let b=acquire();a=b;}");
            let mut m = Machine::new(&e, ExecutionLimits::default());
            m.start(e.entry("demo").unwrap()).unwrap();
            m.resources.as_mut().unwrap().fail_replacement = Some(phase);
            assert_eq!(
                m.resume().unwrap_err().kind,
                ErrorKind::Host("synthetic replacement reservation/progress")
            );
            conservation_tests::conserved(&m);
            assert!(m.resume().is_err());
            if phase != Phase::Rhs {
                assert!(matches!(
                    m.frames[0]
                        .slots
                        .iter()
                        .find(|(p, _)| *p == Place::Local(hir::LocalId(1)))
                        .unwrap()
                        .1,
                    Slot::Transferred
                ));
            }
            assert_eq!(
                completed(&m),
                if phase == Phase::Discharged {
                    vec![0]
                } else {
                    vec![]
                }
            );
        }
    }
    #[test]
    fn every_nested_release_host_failure_retains_exact_remaining_leaves() {
        for identity in 0..8 {
            let e = nested();
            let mut m = Machine::new(&e, ExecutionLimits::default());
            m.start(e.entry("demo").unwrap()).unwrap();
            m.resources.as_mut().unwrap().failure_at = Some(identity);
            assert_eq!(
                m.resume().unwrap_err().kind,
                ErrorKind::Host("synthetic trusted release")
            );
            conservation_tests::conserved(&m);
            assert!(!completed(&m).contains(&identity));
            assert!(m.resume().is_err());
        }
    }
    #[test]
    fn branch_replacement_registers_in_target_owning_activation() {
        for (body, expected) in [
            ("fn demo(){let mut a=acquire();let b=acquire();if true{let inner=acquire();a=acquire();}}", vec![0,2,3,1]),
            ("fn demo(){let mut a=acquire();let b=acquire();if false{let inner=acquire();a=acquire();}}", vec![1,0]),
            ("fn demo(){let mut a=acquire();let b=acquire();if false{a=acquire();}else{a=acquire();}let later=acquire();}", vec![0,3,2,1]),
        ] { assert_eq!(completed(&run(&artifact(body))),expected); }
    }
    #[test]
    fn fallthrough_break_continue_and_bare_return_clean_only_final_generation() {
        for exit in ["", "break;", "continue;", "return;"] {
            let e = artifact(&format!("fn demo(){{let mut flag=true;while flag{{let mut a=acquire();let b=acquire();a=acquire();flag=false;{exit}}}}}"));
            assert_eq!(completed(&run(&e)), vec![0, 2, 1]);
        }
    }
    #[test]
    fn value_return_moves_new_generation_before_cleaning_remainder() {
        let e = artifact("fn make()->Leaf{while true{let mut a=acquire();let b=acquire();a=acquire();return a;}return acquire();}fn demo(){let result=make();}");
        assert_eq!(completed(&run(&e)), vec![0, 1, 2]);
    }
    #[test]
    fn repeated_iteration_activations_and_replacements_preserve_order() {
        let e = artifact("type N=range 0..3;fn start()->N{return 3;}fn demo(){let mut n=start();while !(n==0){let mut a=acquire();let b=acquire();a=acquire();a=acquire();n=n-1;continue;}}");
        assert_eq!(
            completed(&run(&e)),
            vec![0, 2, 3, 1, 4, 6, 7, 5, 8, 10, 11, 9]
        );
    }
    #[test]
    fn later_move_and_explicit_release_never_repeat_displaced_cleanup() {
        for suffix in ["let moved=a;", "release(a);", "let moved=a;release(moved);"] {
            let e = artifact(&format!(
                "fn demo(){{let mut a=acquire();a=acquire();{suffix}}}"
            ));
            assert_eq!(completed(&run(&e)), vec![0, 1]);
        }
    }
    #[test]
    fn target_explicit_consumption_inside_rhs_remains_forbidden() {
        let source = "type R=range 0..1;record Leaf{data:R}fn acquire()->Leaf{return Leaf{data:0};}fn release(x:Leaf){}fn consume(x:Leaf)->Leaf{release(x);return acquire();}fn demo(){let mut a=acquire();a=consume(a);}";
        let error = crate::compile_source(source).unwrap_err();
        assert!(matches!(error, crate::FrontendError::Ownership(_)));
        assert!(error.to_string().contains("inside its own RHS"));
    }
    #[test]
    fn rhs_prefix_mutation_survives_trap_without_displacement_or_publication() {
        let e = crate::execution::prepare_execution("type P=range 0..3;record R{x:P}fn next(v:&mut R)->R{v.x=2;return R{x:v.x-3};}fn demo(){let mut a=R{x:0};a=next(&mut a);}", AdmissionLimits::default()).unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert!(matches!(
            m.resume().unwrap_err().kind,
            ErrorKind::RangeTrap { computed: -1, .. }
        ));
        assert!(
            matches!(m.slot(Location{frame:0,place:Place::Local(hir::LocalId(0))}).unwrap(), Value::Public(RuntimeValue::Record{fields,..}) if fields[0].1.value==2)
        );
        assert!(m.resources.as_ref().unwrap().replacements[0]
            .protected
            .is_none());
        assert!(m.resume().is_err());
    }
}

#[cfg(test)]
mod classification_controls {
    use super::*;
    #[test]
    fn copy_assignment_cannot_be_retagged_as_owned_replacement() {
        let mut e = crate::execution::prepare_execution(
            "type P=range 0..1;record R{x:P}fn demo(){let mut x=false;x=true;}",
            AdmissionLimits::default(),
        )
        .unwrap();
        e.validate().unwrap();
        let s = &mut e.mir.test_functions_mut()[0].test_blocks_mut()[0].test_statements_mut()[1];
        let Statement::Assign { local, value, span } = s.clone() else {
            panic!()
        };
        *s = Statement::ReplaceOwned {
            local,
            record: hir::RecordId(0),
            owning_scope: 0,
            cleanup_plan: None,
            value,
            origin: Origin {
                span,
                kind: OriginKind::Replacement,
            },
            span,
        };
        assert!(e.validate().is_err());
    }
}
