//! Ordinary successful-source custody, using private fixed-operation descriptors.
//! No state or temporary injection is used by these conformance controls.
use super::*;
use crate::effects::{Effect, EffectSet};

fn set(effect: Effect) -> EffectSet {
    let mut set = EffectSet::EMPTY;
    set.insert(effect);
    set
}
fn artifact(source: &str) -> CheckedExecutable {
    crate::execution::prepare_synthetic(
        hir::resources::fixture_effects(source, set(Effect::Io), set(Effect::Filesystem)),
        AdmissionLimits::default(),
    ).unwrap()
}
fn events<'m>(m: &'m Machine<'_>) -> Vec<&'m Event> {
    m.resources.as_ref().unwrap().events.iter()
        .filter(|e| !matches!(e, Event::Retire(_))).collect()
}
fn completed(m: &Machine<'_>) -> Vec<u64> {
    events(m).into_iter().filter_map(|e| match e {
        Event::Complete(id) => Some(*id), _ => None,
    }).collect()
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
    let pass = e.program.functions().iter().find(|f| f.name()=="pass").unwrap().id();
    assert!(!e.approval.cleanup_sites.iter().any(|s| s.function==pass));
    let m = run(&e);
    assert_eq!(events(&m), vec![&Event::Acquire(0), &Event::Acquire(1),
        &Event::Attempt(0), &Event::Complete(0), &Event::Acquire(2),
        &Event::Attempt(2), &Event::Complete(2), &Event::Attempt(1), &Event::Complete(1)]);
    assert!(!m.temporaries.as_ref().unwrap().events.iter().any(|e|
        matches!(e, super::super::temporaries::Event::Cleanup(_))));
}

#[test]
fn protected_result_and_other_parameter_keep_distinct_cleanup_owners() {
    let e = artifact("fn choose(a:Leaf,b:Leaf)->Leaf effects filesystem{return a;}fn take(x:Leaf) effects filesystem{}fn demo() effects io,filesystem{take(choose(acquire(),acquire()));}");
    let f = e.program.functions().iter().find(|f|f.name()=="choose").unwrap().as_ordinary().unwrap();
    let sites: Vec<_> = e.approval.cleanup_sites.iter().filter(|s|s.function==f.id).collect();
    assert_eq!(sites.len(),1);
    assert_eq!(sites[0].holder,Place::Parameter(f.parameters[1]));
    assert_eq!(completed(&run(&e)),vec![1,0]);
}

#[test]
fn short_circuit_skip_and_execution_release_the_actual_owner_once() {
    for flag in [false,true] {
        let e = artifact(&format!("fn take(x:Leaf)->bool effects filesystem{{return true;}}fn skip(x:Leaf) effects filesystem{{let observed={flag}&&take(x);}}fn demo() effects io,filesystem{{skip(acquire());}}"));
        let m = run(&e);
        assert_eq!(completed(&m),vec![0]);
        assert_eq!(events(&m),vec![&Event::Acquire(0),&Event::Attempt(0),&Event::Complete(0)]);
    }
}

#[test]
fn nonempty_effects_conserve_partial_call_entry_at_every_fuel_stop() {
    let e = artifact("fn choose(a:Leaf,b:Leaf)->Leaf effects filesystem{return a;}fn demo() effects io,filesystem{let result=choose(acquire(),acquire());}");
    let full = run(&e);
    let mut partial_entry = false;
    for steps in 1..full.steps {
        let mut m = Machine::new(&e,ExecutionLimits{steps,..ExecutionLimits::default()});
        if m.start(e.entry("demo").unwrap()).is_err(){continue;}
        let error=m.resume().unwrap_err();
        assert_eq!(error.kind,ErrorKind::ConfiguredLimit("steps"));
        if let Some(frame)=m.frames.last() {
            let ready=frame.slots.iter().filter(|(p,s)|matches!(p,Place::Parameter(_))&&matches!(s,Slot::Available(_))).count();
            // Entry publishes receiving slots one at a time. The second slot
            // does not exist yet when transfer of the second argument stops.
            partial_entry |= frame.function==FunctionId(2)&&ready==1&&frame.slots.len()==1;
        }
        super::conservation_tests::conserved(&m);
        let before=format!("{:?}",m.resources.as_ref().map(|s|&s.events));
        assert!(m.resume().is_err());
        assert_eq!(format!("{:?}",m.resources.as_ref().map(|s|&s.events)),before);
    }
    assert!(partial_entry,"fuel matrix must actually stop during parameter entry");
    assert_eq!(completed(&full),vec![1,0]);
}

#[test]
fn nonempty_effects_preserve_exact_runtime_storage_neighbors() {
    let e = artifact("fn pass(x:Leaf)->Leaf{return x;}fn demo() effects io,filesystem{let mut x=acquire();x=pass(acquire());}");
    let run = |bytes| {
        let mut m=Machine::new(&e,ExecutionLimits{working_storage:bytes,..ExecutionLimits::default()});
        let result=m.start(e.entry("demo").unwrap()).and_then(|()|m.resume());
        assert!(m.storage().unwrap()<=bytes);
        if m.resources.is_some(){super::conservation_tests::conserved(&m);}
        result
    };
    let(mut low,mut high)=(1,ExecutionLimits::default().working_storage);
    assert!(run(high).is_ok());
    while low<high {let middle=low+(high-low)/2;if run(middle).is_ok(){high=middle;}else{low=middle+1;}}
    assert!(run(low).is_ok());
    assert_eq!(run(low-1).unwrap_err().kind,ErrorKind::ConfiguredLimit("working storage"));
}

#[test]
fn effectful_release_failure_preserves_attempt_failure_and_never_started_suffix() {
    let e=artifact("fn demo() effects io,filesystem,network,unsafe{let a=acquire();let b=acquire();let c=acquire();}");
    let mut m=Machine::new(&e,ExecutionLimits::default());
    m.start(e.entry("demo").unwrap()).unwrap();
    m.resources.as_mut().unwrap().failure_at=Some(1);
    assert_eq!(m.resume().unwrap_err().kind,ErrorKind::Host("synthetic trusted release"));
    assert_eq!(events(&m),vec![&Event::Acquire(0),&Event::Acquire(1),&Event::Acquire(2),
        &Event::Attempt(2),&Event::Complete(2),&Event::Attempt(1),&Event::Failure(1)]);
    super::conservation_tests::conserved(&m);
    let before=format!("{:?}",events(&m));
    assert!(m.resume().is_err());
    assert_eq!(format!("{:?}",events(&m)),before);
}
