//! Ordinary source custody and separately admitted private abort/protocol evidence.
//! The ordinary runtime controls use fixed descriptors without state injection.
use super::*;
use crate::effects::{Effect, EffectSet};

fn set(effect: Effect) -> EffectSet {
    let mut set = EffectSet::EMPTY;
    set.insert(effect);
    set
}
fn artifact(source: &str) -> CheckedExecutable {
    crate::execution::prepare_ordinary_synthetic(
        hir::resources::fixture_effects(source, set(Effect::Io), set(Effect::Filesystem)),
        AdmissionLimits::default(),
    )
    .unwrap()
}
fn events<'m>(m: &'m Machine<'_>) -> Vec<&'m Event> {
    m.resources
        .as_ref()
        .unwrap()
        .events
        .iter()
        .filter(|e| !matches!(e, Event::Retire(_)))
        .collect()
}
fn completed(m: &Machine<'_>) -> Vec<u64> {
    events(m)
        .into_iter()
        .filter_map(|e| match e {
            Event::Complete(id) => Some(*id),
            _ => None,
        })
        .collect()
}
fn run(e: &CheckedExecutable) -> Machine<'_> {
    let mut m = Machine::new(e, ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    while let Some(task) = m.tasks.pop() {
        m.step(task).unwrap();
        super::conservation_tests::conserved(&m);
    }
    assert_eq!(m.resume().unwrap(), ExecutionResult::NoValue);
    m
}

#[test]
fn nonempty_contracts_preserve_replacement_generations_and_argument_deferral() {
    let e = artifact("fn pass(x:Leaf)->Leaf{return x;}fn take(x:Leaf) effects filesystem{}fn demo() effects io,filesystem{let mut current=acquire();current=pass(acquire());take(pass(acquire()));}");
    let pass = e
        .program
        .functions()
        .iter()
        .find(|f| f.name() == "pass")
        .unwrap()
        .id();
    assert!(!e.approval.cleanup_sites.iter().any(|s| s.function == pass));
    let m = run(&e);
    assert_eq!(
        events(&m),
        vec![
            &Event::Acquire(0),
            &Event::Acquire(1),
            &Event::Attempt(0),
            &Event::Complete(0),
            &Event::Acquire(2),
            &Event::Attempt(2),
            &Event::Complete(2),
            &Event::Attempt(1),
            &Event::Complete(1)
        ]
    );
    assert!(!m
        .temporaries
        .as_ref()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, super::super::temporaries::Event::Cleanup(_))));
}

#[test]
fn protected_result_and_other_parameter_keep_distinct_cleanup_owners() {
    let e = artifact("fn choose(a:Leaf,b:Leaf)->Leaf effects filesystem{return a;}fn take(x:Leaf) effects filesystem{}fn demo() effects io,filesystem{take(choose(acquire(),acquire()));}");
    let f = e
        .program
        .functions()
        .iter()
        .find(|f| f.name() == "choose")
        .unwrap()
        .as_ordinary()
        .unwrap();
    let sites: Vec<_> = e
        .approval
        .cleanup_sites
        .iter()
        .filter(|s| s.function == f.id)
        .collect();
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].holder, Place::Parameter(f.parameters[1]));
    assert_eq!(completed(&run(&e)), vec![1, 0]);
}

#[test]
fn short_circuit_skip_and_execution_release_the_actual_owner_once() {
    for flag in [false, true] {
        let e = artifact(&format!("fn take(x:Leaf)->bool effects filesystem{{return true;}}fn skip(x:Leaf) effects filesystem{{let observed={flag}&&take(x);}}fn demo() effects io,filesystem{{skip(acquire());}}"));
        let m = run(&e);
        assert_eq!(completed(&m), vec![0]);
        assert_eq!(
            events(&m),
            vec![&Event::Acquire(0), &Event::Attempt(0), &Event::Complete(0)]
        );
    }
}

#[test]
fn nonempty_effects_conserve_partial_call_entry_at_every_fuel_stop() {
    let e = artifact("fn choose(a:Leaf,b:Leaf)->Leaf effects filesystem{return a;}fn demo() effects io,filesystem{let predecessor_result=choose(acquire(),acquire());}");
    let full = run(&e);
    let mut partial_entry = false;
    for steps in 1..full.steps {
        let mut m = Machine::new(
            &e,
            ExecutionLimits {
                steps,
                ..ExecutionLimits::default()
            },
        );
        if m.start(e.entry("demo").unwrap()).is_err() {
            continue;
        }
        let error = m.resume().unwrap_err();
        assert_eq!(error.kind, ErrorKind::ConfiguredLimit("steps"));
        if let Some(frame) = m.frames.last() {
            let ready = frame
                .slots
                .iter()
                .filter(|(p, s)| {
                    matches!(p, Place::Parameter(_)) && matches!(s, Slot::Available(_))
                })
                .count();
            // Entry publishes receiving slots one at a time. The second slot
            // does not exist yet when transfer of the second argument stops.
            partial_entry |=
                frame.function == FunctionId(2) && ready == 1 && frame.slots.len() == 1;
        }
        super::conservation_tests::conserved(&m);
        let before = format!("{:?}", m.resources.as_ref().map(|s| &s.events));
        assert!(m.resume().is_err());
        assert_eq!(
            format!("{:?}", m.resources.as_ref().map(|s| &s.events)),
            before
        );
    }
    assert!(
        partial_entry,
        "fuel matrix must actually stop during parameter entry"
    );
    assert_eq!(completed(&full), vec![1, 0]);
}

#[test]
fn nonempty_effects_preserve_exact_runtime_storage_neighbors() {
    let e = artifact("fn pass(x:Leaf)->Leaf{return x;}fn demo() effects io,filesystem{let mut x=acquire();x=pass(acquire());}");
    let run = |bytes| {
        let mut m = Machine::new(
            &e,
            ExecutionLimits {
                working_storage: bytes,
                ..ExecutionLimits::default()
            },
        );
        let result = m.start(e.entry("demo").unwrap()).and_then(|()| m.resume());
        assert!(m.storage().unwrap() <= bytes);
        if m.resources.is_some() {
            super::conservation_tests::conserved(&m);
        }
        result
    };
    let (mut low, mut high) = (1, ExecutionLimits::default().working_storage);
    assert!(run(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if run(middle).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(run(low).is_ok());
    assert_eq!(
        run(low - 1).unwrap_err().kind,
        ErrorKind::ConfiguredLimit("working storage")
    );
}

#[test]
fn effectful_release_failure_preserves_attempt_failure_and_never_started_suffix() {
    let e=artifact("fn demo() effects io,filesystem,network,unsafe{let a=acquire();let b=acquire();let c=acquire();}");
    let mut m = Machine::new(&e, ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    m.resources.as_mut().unwrap().failure_at = Some(1);
    assert_eq!(
        m.resume().unwrap_err().kind,
        ErrorKind::Host("synthetic trusted release")
    );
    assert_eq!(
        events(&m),
        vec![
            &Event::Acquire(0),
            &Event::Acquire(1),
            &Event::Acquire(2),
            &Event::Attempt(2),
            &Event::Complete(2),
            &Event::Attempt(1),
            &Event::Failure(1)
        ]
    );
    super::conservation_tests::conserved(&m);
    let before = format!("{:?}", events(&m));
    assert!(m.resume().is_err());
    assert_eq!(format!("{:?}", events(&m)), before);
}

// These constructors still pass the complete ordinary ownership/effect pipeline.
// Only the fixed operation catalog and the private scenario differ.
fn aggregate(source: &str) -> hir::Program {
    let mut p = super::aggregate_tests::program(source, [true, false, true]);
    for (i, effect) in [(0, Effect::Io), (1, Effect::Filesystem)] {
        let hir::Function::Ordinary(f) = &mut p.functions[i] else {
            unreachable!()
        };
        f.effects = crate::effects::Declaration::synthetic(set(effect), f.span);
        p.resources.operations[i].effects = set(effect);
    }
    p.resources.leaves[0].cleanup_effects = set(Effect::Filesystem);
    p
}
fn abort_request(
    p: &hir::Program,
    name: &str,
    frame: u64,
    depth: usize,
    pending: u64,
) -> crate::execution::domain::AbortRequest {
    let function = p
        .functions()
        .iter()
        .find(|f| f.name() == name)
        .unwrap()
        .id();
    let (mir, _) = crate::mir::lower_with_origins(p).unwrap();
    let flow = mir
        .functions()
        .iter()
        .find(|f| f.function() == function)
        .unwrap()
        .temporaries
        .as_ref()
        .unwrap();
    let site = flow
        .sites
        .iter()
        .find(|s| s.plan == Some(hir::RecordId(1)) && s.children.len() == 3)
        .unwrap()
        .id;
    crate::execution::domain::AbortRequest {
        function,
        site,
        before: hir::FieldId(1),
        frame,
        depth,
        pending,
    }
}
fn approved(
    p: hir::Program,
    requests: Vec<crate::execution::domain::AbortRequest>,
) -> Result<CheckedExecutable, ExecutionError> {
    crate::execution::prepare_in_domain(
        p,
        AdmissionLimits::default(),
        crate::execution::domain::Request::Abort(requests),
    )
}
fn abort_run(e: &CheckedExecutable) -> Machine<'_> {
    let mut m = Machine::new(e, ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
    assert_eq!(
        m.resume().unwrap_err().kind,
        ErrorKind::SyntheticConstructionFailure
    );
    super::conservation_tests::conserved(&m);
    m
}

#[test]
fn ordinary_approved_abort_and_primitive_protocol_have_separate_sealed_authority() {
    use crate::execution::{prepare_ordinary_synthetic, prepare_synthetic};
    let p=aggregate("fn build()->Pair effects io{return Pair{last:0,first:0,inert:0};}fn demo() effects io,filesystem{let predecessor_result=build();}");
    let ordinary = prepare_ordinary_synthetic(p.clone(), AdmissionLimits::default()).unwrap();
    assert!(matches!(
        ordinary.domain,
        crate::execution::domain::Domain::Ordinary
    ));
    assert_eq!(completed(&run(&ordinary)), vec![0, 1]);
    let mut m = Machine::new(&ordinary, ExecutionLimits::default());
    m.start(ordinary.entry("demo").unwrap()).unwrap();
    m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
    assert_eq!(
        m.resume().unwrap_err().kind,
        ErrorKind::Integrity("ordinary execution has no synthetic abort authority")
    );
    assert_eq!(events(&m), vec![&Event::Acquire(0), &Event::Acquire(1)]);
    super::conservation_tests::conserved(&m);
    let request = abort_request(&p, "build", 1, 1, 0);
    assert!(matches!(
        approved(p.clone(), vec![request]).err().unwrap().kind,
        ErrorKind::Frontend {
            phase: "effects",
            ..
        }
    ));
    let primitive = prepare_synthetic(p, AdmissionLimits::default()).unwrap();
    assert!(matches!(
        primitive.domain,
        crate::execution::domain::Domain::Primitive
    ));
    assert_eq!(completed(&abort_run(&primitive)), vec![0, 1]);
    let p=aggregate("fn build()->Pair effects io,filesystem{return Pair{last:0,first:0,inert:0};}fn demo() effects io,filesystem{let predecessor_result=build();}");
    let request = abort_request(&p, "build", 1, 1, 0);
    let e = approved(p, vec![request]).unwrap();
    let mut m = abort_run(&e);
    assert_eq!(completed(&m), vec![0, 1]);
    assert!(m.resources.as_ref().unwrap().pending.is_empty());
    let before = format!("{:?}", events(&m));
    assert!(m.resume().is_err());
    assert_eq!(format!("{:?}", events(&m)), before);
}

#[test]
fn ordinary_trap_during_pending_construction_adds_no_cleanup_edge() {
    let p=aggregate("fn zero()->R{return 0;}fn boom()->R{let mut n=zero();n=n-1;return n;}fn build()->Pair effects io{return Pair{last:0,first:0,inert:boom()};}fn demo() effects io,filesystem{let predecessor_result=build();}");
    let e = crate::execution::prepare_ordinary_synthetic(p, AdmissionLimits::default()).unwrap();
    let build = e
        .program
        .functions()
        .iter()
        .find(|f| f.name() == "build")
        .unwrap()
        .id();
    assert!(!e.approval.cleanup_sites.iter().any(|s| s.function == build));
    let mut m = Machine::new(&e, ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    assert!(matches!(
        m.resume().unwrap_err().kind,
        ErrorKind::RangeTrap { .. }
    ));
    assert_eq!(events(&m), vec![&Event::Acquire(0), &Event::Acquire(1)]);
    assert_eq!(
        m.resources.as_ref().unwrap().pending[0].initialized.len(),
        2
    );
    super::conservation_tests::conserved(&m);
    assert!(m.resume().is_err());
    assert_eq!(events(&m), vec![&Event::Acquire(0), &Event::Acquire(1)]);
}

#[test]
fn approved_abort_rejects_independent_template_and_domain_mutations() {
    use crate::execution::domain::Domain;
    let p=aggregate("fn build()->Pair effects io,filesystem{return Pair{last:0,first:0,inert:0};}fn demo() effects io,filesystem{let predecessor_result=build();}");
    let request = abort_request(&p, "build", 1, 1, 0);
    for mutation in 0..14 {
        let mut e = approved(p.clone(), vec![request]).unwrap();
        let Domain::Abort(templates) = &mut e.domain else {
            unreachable!()
        };
        let t = &mut templates[0];
        match mutation {
            0 => t.record = hir::RecordId(0),
            1 => t.effects = EffectSet::EMPTY,
            2 => {
                t.effects.insert(Effect::Network);
            }
            3 => t.origin.span.start += 1,
            4 => t.failure.start += 1,
            5 => {
                t.initialized.pop();
            }
            6 => t.initialized.swap(0, 1),
            7 => t.initialized.push(hir::FieldId(0)),
            8 => t.request.site = crate::mir::temporaries::SiteId(usize::MAX),
            9 => t.request.function = FunctionId(usize::MAX),
            10 => templates.push(templates[0].clone()),
            11 => templates.clear(),
            12 => t.before_site = crate::mir::temporaries::SiteId(usize::MAX),
            13 => t.before_origin.span.start += 1,
            _ => unreachable!(),
        }
        assert!(
            e.domain.validate(&e).is_err(),
            "independent canonical validator mutation {mutation}"
        );
        assert!(
            e.validate().is_err(),
            "sealed correspondence mutation {mutation}"
        );
    }
    let mut e =
        crate::execution::prepare_ordinary_synthetic(p, AdmissionLimits::default()).unwrap();
    e.domain = Domain::Primitive;
    assert!(e.validate().is_err());
}

#[test]
fn approved_abort_binds_actual_frame_pending_membership_and_origin_before_attempt() {
    let p=aggregate("fn build()->Pair effects io,filesystem{return Pair{last:0,first:0,inert:0};}fn demo() effects io,filesystem{let predecessor_result=build();}");
    let request = abort_request(&p, "build", 1, 1, 0);
    for mutation in 0..7 {
        let e = approved(p.clone(), vec![request]).unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        loop {
            let task = m.tasks.pop().unwrap();
            let stop = matches!(&task.action,Action::PendingNext(_,fields,index,_) if fields.get(*index).is_some_and(|f|f.field==hir::FieldId(1)));
            if stop {
                m.tasks.push(task);
                break;
            }
            m.step(task).unwrap();
        }
        let pending = &mut m.resources.as_mut().unwrap().pending[0];
        match mutation {
            0 => pending.id += 1,
            1 => pending.frame = 0,
            2 => pending.record = hir::RecordId(0),
            3 => pending.initialized.swap(0, 1),
            4 => pending.temporary_site = None,
            5 => pending.temporary_site.as_mut().unwrap().0 += 1,
            6 => pending.complete = true,
            _ => unreachable!(),
        }
        m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
        assert!(
            matches!(m.resume().unwrap_err().kind, ErrorKind::Integrity(_)),
            "mutation {mutation}"
        );
        assert!(!events(&m).iter().any(|e| matches!(e, Event::Attempt(_))));
    }
}

#[test]
fn ordinary_temporary_injection_is_rejected_before_acquisition() {
    let e = artifact(
        "fn take(x:Leaf) effects filesystem{}fn demo() effects io,filesystem{take(acquire());}",
    );
    let mut m = Machine::new(&e, ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    assert!(matches!(
        m.inject_temporary_leaf(0).unwrap_err().kind,
        ErrorKind::Integrity(_)
    ));
    assert!(m.resources.is_none());
    assert_eq!(completed(&run(&e)), vec![0]);
}

#[test]
fn approved_nested_abort_admits_and_binds_every_affected_frame_before_release() {
    let mut p=aggregate("fn inner()->Leaf effects io,filesystem{let pending=Pair{last:0,first:0,inert:0};return acquire();}fn outer()->Pair effects io,filesystem{return Pair{last:0,first:0,inert:0};}fn demo() effects io,filesystem{let predecessor_result=outer();}");
    let inner = p
        .functions()
        .iter()
        .find(|f| f.name() == "inner")
        .unwrap()
        .id();
    let hir::Function::Ordinary(outer) = p
        .functions
        .iter_mut()
        .find(|f| f.name() == "outer")
        .unwrap()
    else {
        unreachable!()
    };
    let hir::ValueStatement::Return { value, .. } = &mut outer.body[0] else {
        unreachable!()
    };
    let hir::ExprKind::RecordConstruct { fields, .. } = &mut value.kind else {
        unreachable!()
    };
    let value = &mut fields
        .iter_mut()
        .find(|f| f.field == hir::FieldId(0))
        .unwrap()
        .value;
    value.kind = hir::ExprKind::Call {
        function: inner,
        arguments: Vec::new(),
    };
    let mut outer = abort_request(&p, "outer", 1, 1, 0);
    outer.before = hir::FieldId(0);
    let inner = abort_request(&p, "inner", 2, 2, 1);
    assert!(approved(p.clone(), vec![inner, outer]).is_err());
    assert!(approved(p.clone(), vec![outer, outer]).is_err());
    let missing = approved(p.clone(), vec![inner]).unwrap();
    let mut m = Machine::new(&missing, ExecutionLimits::default());
    m.start(missing.entry("demo").unwrap()).unwrap();
    m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
    assert!(matches!(
        m.resume().unwrap_err().kind,
        ErrorKind::Integrity(_)
    ));
    assert_eq!(
        events(&m),
        vec![&Event::Acquire(0), &Event::Acquire(1), &Event::Acquire(2)]
    );
    super::conservation_tests::conserved(&m);
    let e = approved(p, vec![outer, inner]).unwrap();
    assert_eq!(completed(&abort_run(&e)), vec![1, 2, 0]);
    let mut m = Machine::new(&e, ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    loop {
        let task = m.tasks.pop().unwrap();
        if matches!(&task.action,Action::PendingNext(id,fields,index,_) if *id==1 && fields.get(*index).is_some_and(|f|f.field==hir::FieldId(1)))
        {
            m.tasks.push(task);
            break;
        }
        m.step(task).unwrap();
    }
    m.resources.as_mut().unwrap().pending[0].child_activation = None;
    m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
    assert_eq!(
        m.resume().unwrap_err().kind,
        ErrorKind::Integrity("private abort ancestor has no in-progress field")
    );
    assert!(!events(&m).iter().any(|e| matches!(e, Event::Attempt(_))));
    super::conservation_tests::conserved(&m);
}

#[test]
fn private_abort_metadata_has_exact_admission_neighbors_and_typed_host_failure() {
    use crate::execution::{domain, prepare_in_domain};
    let p=aggregate("fn build()->Pair effects io,filesystem{return Pair{last:0,first:0,inert:0};}fn demo() effects io,filesystem{let predecessor_result=build();}");
    let request = abort_request(&p, "build", 1, 1, 0);
    for work in [false, true] {
        let prepare = |bound| {
            let mut limits = AdmissionLimits::default();
            if work {
                limits.frontend_work = bound;
            } else {
                limits.working_storage = bound as usize;
            }
            prepare_in_domain(p.clone(), limits, domain::Request::Abort(vec![request]))
        };
        let (mut low, mut high) = (
            1,
            if work {
                AdmissionLimits::default().frontend_work
            } else {
                AdmissionLimits::default().working_storage as u64
            },
        );
        assert!(prepare(high).is_ok());
        while low < high {
            let middle = low + (high - low) / 2;
            if prepare(middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(prepare(low).is_ok());
        assert!(matches!(
            prepare(low - 1).err().unwrap().kind,
            ErrorKind::ConfiguredLimit(_)
        ));
    }
    let mut bad = request;
    bad.depth = AdmissionLimits::default().traversal_depth;
    assert!(matches!(
        approved(p.clone(), vec![bad]).err().unwrap().kind,
        ErrorKind::Integrity(_)
    ));
    assert!(approved(p.clone(), Vec::new()).is_err());
    assert!(matches!(
        approved(
            p.clone(),
            vec![request; AdmissionLimits::default().traversal_depth + 1]
        )
        .err()
        .unwrap()
        .kind,
        ErrorKind::ConfiguredLimit(_)
    ));
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            domain::FAIL_ALLOCATION.with(|f| f.set(false));
        }
    }
    let _restore = Restore;
    domain::FAIL_ALLOCATION.with(|f| f.set(true));
    assert_eq!(
        approved(p, vec![request]).err().unwrap().kind,
        ErrorKind::Host("private abort authority allocation")
    );
}

#[test]
fn approved_abort_release_failure_keeps_its_observable_prefix_and_stops() {
    let p=aggregate("fn build()->Pair effects io,filesystem{return Pair{last:0,first:0,inert:0};}fn demo() effects io,filesystem{let predecessor_result=build();}");
    let request = abort_request(&p, "build", 1, 1, 0);
    let e = approved(p, vec![request]).unwrap();
    let mut m = Machine::new(&e, ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
    m.resources.as_mut().unwrap().failure_at = Some(0);
    let error = m.resume().unwrap_err();
    assert_eq!(error.kind, ErrorKind::Host("synthetic trusted release"));
    let crate::execution::domain::Domain::Abort(templates) = &e.domain else {
        unreachable!()
    };
    assert_eq!(error.origin, templates[0].origin.span);
    assert_eq!(
        events(&m),
        vec![
            &Event::Acquire(0),
            &Event::Acquire(1),
            &Event::Attempt(0),
            &Event::Failure(0)
        ]
    );
    super::conservation_tests::conserved(&m);
    let before = format!("{:?}", events(&m));
    assert!(m.resume().is_err());
    assert_eq!(format!("{:?}", events(&m)), before);
}

#[test]
fn every_cleanup_atom_requires_its_contract_at_the_same_realization() {
    for effect in Effect::ALL {
        let p = hir::resources::fixture_effects(
            "fn demo(){let owner=acquire();}",
            EffectSet::EMPTY,
            set(effect),
        );
        let error = crate::execution::prepare_ordinary_synthetic(p, AdmissionLimits::default())
            .err()
            .unwrap();
        assert!(matches!(
            error.kind,
            ErrorKind::Frontend {
                phase: "effects",
                ..
            }
        ));
        assert!(error.to_string().contains(effect.name()));
        let p = hir::resources::fixture_effects(
            &format!(
                "fn demo() effects {}{{let owner=acquire();}}",
                effect.name()
            ),
            EffectSet::EMPTY,
            set(effect),
        );
        let e =
            crate::execution::prepare_ordinary_synthetic(p, AdmissionLimits::default()).unwrap();
        assert_eq!(completed(&run(&e)), vec![0]);
    }
}

#[test]
fn empty_cleanup_summary_preserves_owner_but_does_not_own_references_or_inert_moves() {
    let mut p=hir::resources::fixture_effects("fn inspect(x:&Leaf){}fn demo() effects io{let owner=acquire();inspect(&owner);let inert=Pair{last:0,first:0,inert:0};let moved=inert;}",set(Effect::Io),EffectSet::EMPTY);
    // fixture_with normally makes Pair's first/last fields resource-bearing.
    // This control needs the original inert Move type beside the real Leaf.
    for field in &mut p.fields {
        field.ty = ValueType::Range(hir::RangeTypeId(0));
    }
    let e = crate::execution::prepare_ordinary_synthetic(p, AdmissionLimits::default()).unwrap();
    assert!(e.cleanup_effects.is_empty());
    assert!(e.approval.cleanup_sites.is_empty());
    assert!(e.cleanup_plans[0].is_some());
    assert!(e.cleanup_plans[1].is_none());
    assert_eq!(
        events(&run(&e)),
        vec![&Event::Acquire(0), &Event::Attempt(0), &Event::Complete(0)]
    );
}

#[test]
fn ordinary_pending_limits_keep_payloads_without_adding_abort_cleanup() {
    let p=aggregate("fn build()->Pair effects io{return Pair{last:0,first:0,inert:0};}fn demo() effects io,filesystem{let predecessor_result=build();}");
    let e = crate::execution::prepare_ordinary_synthetic(p, AdmissionLimits::default()).unwrap();
    let full = run(&e);
    let mut pending_stops = 0;
    for steps in 1..full.steps {
        let mut m = Machine::new(
            &e,
            ExecutionLimits {
                steps,
                ..ExecutionLimits::default()
            },
        );
        if m.start(e.entry("demo").unwrap()).is_err() {
            continue;
        }
        assert!(matches!(
            m.resume().unwrap_err().kind,
            ErrorKind::ConfiguredLimit("steps")
        ));
        if let Some(s) = &m.resources {
            if !s.pending.is_empty() {
                pending_stops += 1;
                assert!(!s
                    .events
                    .iter()
                    .any(|e| matches!(e, Event::Attempt(_) | Event::Complete(_))));
            }
            super::conservation_tests::conserved(&m);
            let before = format!("{:?}", s.events);
            assert!(m.resume().is_err());
            assert_eq!(
                format!("{:?}", m.resources.as_ref().unwrap().events),
                before
            );
        }
    }
    assert!(pending_stops > 10);
}

#[test]
fn recursive_abort_templates_distinguish_identical_sites_in_distinct_frames() {
    let mut p=aggregate("fn recurse(n:R)->Leaf effects io,filesystem{let pending=Pair{last:0,first:0,inert:0};return acquire();}fn select(n:R)->Leaf effects io,filesystem{let chosen=if n==0{acquire()}else{recurse(n-1)};return chosen;}fn demo() effects io,filesystem{let predecessor_result=recurse(1);}");
    let select = p
        .functions()
        .iter()
        .find(|f| f.name() == "select")
        .unwrap()
        .id();
    let hir::Function::Ordinary(recurse) = p
        .functions
        .iter_mut()
        .find(|f| f.name() == "recurse")
        .unwrap()
    else {
        unreachable!()
    };
    let parameter = recurse.parameters[0];
    let hir::ValueStatement::Let { initializer, .. } = &mut recurse.body[0] else {
        unreachable!()
    };
    let hir::ExprKind::RecordConstruct { fields, .. } = &mut initializer.kind else {
        unreachable!()
    };
    let value = &mut fields
        .iter_mut()
        .find(|f| f.field == hir::FieldId(0))
        .unwrap()
        .value;
    value.kind = hir::ExprKind::Call {
        function: select,
        arguments: vec![hir::TypedExpr {
            kind: hir::ExprKind::Parameter(parameter),
            ty: hir::ExprType::Range(hir::RangeTypeId(0)),
            span: value.span,
        }],
    };
    let mut outer = abort_request(&p, "recurse", 1, 1, 0);
    outer.before = hir::FieldId(0);
    let inner = abort_request(&p, "recurse", 3, 3, 1);
    assert_eq!(outer.function, inner.function);
    assert_eq!(outer.site, inner.site);
    let e = approved(p.clone(), vec![outer, inner]).unwrap();
    assert_eq!(completed(&abort_run(&e)), vec![1, 2, 0]);
    let mut wrong = inner;
    wrong.frame = 1;
    wrong.depth = 1;
    let e = approved(p, vec![outer, wrong]).unwrap();
    let mut m = Machine::new(&e, ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
    assert!(matches!(
        m.resume().unwrap_err().kind,
        ErrorKind::Integrity(_)
    ));
    assert!(!events(&m).iter().any(|e| matches!(e, Event::Attempt(_))));
    super::conservation_tests::conserved(&m);
}
