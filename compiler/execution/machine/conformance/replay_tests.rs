//! Private interpreter-contract evidence; no public controlled-input ABI.
use super::*;
use replay::{Address, Birth, Input, Item, Phase, Response, Site};
fn execution()->ExecutionLimits {ExecutionLimits{steps:256,..ExecutionLimits::default()}}
fn run<'a>(c:&'a contract::Contract<'a>,responses:Option<&[Item]>,failures:&[Address],execution:ExecutionLimits)->Session<'a> {
    let input=Input{responses,failures,limits:replay::Limits{work:1_000_000_000,..replay::Limits::default()}};
    let mut session=Session::new_inner(c,execution,Limits{records:262_144,work:10_000_000,storage:67_108_864,lineage:128},Some(input)).unwrap();
    let _=session.advance();session
}
fn compare_pair(a:&Session<'_>,b:&Session<'_>)->Comparison {compare(a,b,ComparisonLimits{work:100_000_000,..ComparisonLimits::default()})}
fn selected()->CheckedExecutable {
    let selection=crate::trusted::Selection::sensor_demo(crate::trusted::Scenario::Sample,crate::trusted::Grants::ALL,&["sample"]).unwrap();
    prepare_execution_with_profile(include_str!("../../../../examples/sensor_finish.klx"),AdmissionLimits::default(),&selection).unwrap()
}
fn call(plan:usize,phase:Phase)->Address {Address{site:Site::Invocation(plan),activation:0,occurrence:1,phase}}
fn release(c:&contract::Contract<'_>,phase:Phase)->Address {
    let approved=c.executable.trusted.as_ref().unwrap();
    let span=approved.invocations[2].completion.as_ref().unwrap().cleanup.span;
    Address{site:Site::Release(c.entry(),span),activation:0,occurrence:1,phase}
}
fn events<'a>(session:&'a Session<'_>)->&'a [trusted::Event] {
    let Engine::Selected(session)=&session.engine else {unreachable!()};session.events()
}
fn live(session:&Session<'_>)->usize {
    let Engine::Selected(session)=&session.engine else {unreachable!()};session.live_sensors()
}
#[test]
fn ordinary_modeled_capacity_host_is_equal_and_storage_precedes_consumption() {
    let e=prepare_execution("fn demo()->bool{return true;}",AdmissionLimits::default()).unwrap();
    let c=contract::Contract::extract(&e,e.entry("demo").unwrap(),contract::Limits::default()).unwrap();
    let address=Address{site:Site::Capacity(c.entry(),e.program.function(c.entry()).span()),activation:0,occurrence:1,phase:Phase::Reservation};
    let failures=[address];
    let a=run(&c,None,&failures,execution());let b=run(&c,None,&failures,execution());
    assert_eq!(a.primary().unwrap().unwrap_err().kind,ErrorKind::Host("modeled abstract Host"));
    assert_eq!(compare_pair(&a,&b),Comparison::Equal);
    assert_eq!(a.machine().steps,1);
    let low=ExecutionLimits{working_storage:c.schedule().machine[0]-1,..execution()};
    let a=run(&c,None,&failures,low);let b=run(&c,None,&failures,low);
    assert_eq!(a.primary().unwrap().unwrap_err().kind,ErrorKind::ConfiguredLimit("working storage"));
    assert!(a.machine().control.borrow().as_ref().unwrap().stopped().is_none());
    assert_eq!(compare_pair(&a,&b),Comparison::Equal);
}
#[test]
fn addressed_completion_phases_preserve_manual_prefix_custody_and_non_delivery() {
    let e=selected();let c=contract::Contract::extract(&e,e.entry("sample").unwrap(),contract::Limits::default()).unwrap();
    let approved=e.trusted.as_ref().unwrap();let finish=approved.invocations[2].operation;
    for phase in [Phase::BeforeHandoff,Phase::HandoffAccepted,Phase::BeforeAttempt,Phase::Attempt,Phase::Response,Phase::Validation,Phase::Protection,Phase::Delivery] {
        let failures=[call(2,phase)];
        let mut a=run(&c,None,&failures,execution());let b=run(&c,None,&failures,execution());
        assert_eq!(a.primary().unwrap().unwrap_err().kind,ErrorKind::Host("modeled abstract Host"),"{phase:?}");
        let attempted=matches!(phase,Phase::Attempt|Phase::Response|Phase::Validation|Phase::Protection|Phase::Delivery);
        assert_eq!(events(&a).contains(&trusted::Event::Attempt(2,finish)),attempted,"{phase:?}");
        assert!(!events(&a).iter().any(|event|matches!(event,trusted::Event::Published(2,_))));
        assert_eq!(live(&a),usize::from(phase!=Phase::Delivery));
        assert_eq!(events(&a).contains(&trusted::Event::ReleaseComplete(0)),phase==Phase::Delivery);
        assert_eq!(compare_pair(&a,&b),Comparison::Equal,"{phase:?}");
        let count=a.record_count();let fuel=a.machine().steps;let prefix=events(&a).to_vec();
        assert!(a.advance().is_err());assert_eq!(a.record_count(),count);assert_eq!(a.machine().steps,fuel);assert_eq!(events(&a),prefix);
    }
}
#[test]
fn strict_typed_replay_commits_partial_error_then_exact_fixed_release() {
    let e=selected();let c=contract::Contract::extract(&e,e.entry("sample").unwrap(),contract::Limits::default()).unwrap();
    let catalog=&e.trusted.as_ref().unwrap().catalog;
    let responses=[
        Item{address:call(0,Phase::Response),response:Response::Open(catalog.sensor)},
        Item{address:call(1,Phase::Response),response:Response::Value(catalog.reading,37,0)},
        Item{address:call(2,Phase::Response),response:Response::Error(catalog.error,3,1)},
        Item{address:release(&c,Phase::ReleaseAttempt),response:Response::Release(Birth{plan:0,activation:0,occurrence:1},true)},
    ];
    let a=run(&c,Some(&responses),&[],execution());let b=run(&c,Some(&responses),&[],execution());
    let ExecutionResult::Value(RuntimeValue::Result(layers))=a.primary().unwrap().unwrap() else {panic!("ordinary Error required")};
    assert_eq!(layers[0].tag,hir::ResultTag::Error);
    assert_eq!(layers[0].payload,Some(ResultPayload::Range(RangeValue{ty:catalog.error,value:3})));
    assert_eq!(live(&a),0);
    let finish=e.trusted.as_ref().unwrap().invocations[2].operation;
    let expected=[trusted::Event::Attempt(2,finish),trusted::Event::FinishPrefix(2,1),trusted::Event::Received(2,1),trusted::Event::FinishProtected(2,hir::ResultTag::Error),trusted::Event::ReleaseAttempt(0,catalog.resource.release),trusted::Event::ReleaseComplete(0),trusted::Event::Published(2,hir::ResultTag::Error)];
    let start=events(&a).iter().position(|event|*event==expected[0]).unwrap();
    assert_eq!(&events(&a)[start..],&expected);
    assert_eq!(a.machine().control.borrow().as_ref().unwrap().used_responses(),4);
    assert_eq!(compare_pair(&a,&b),Comparison::Equal);
}
#[test]
fn release_attempt_and_completed_release_hosts_have_opposite_live_duties() {
    let e=selected();let c=contract::Contract::extract(&e,e.entry("sample").unwrap(),contract::Limits::default()).unwrap();
    for phase in [Phase::BeforeAttempt,Phase::ReleaseAttempt,Phase::ReleaseComplete] {
        let failures=[release(&c,phase)];let a=run(&c,None,&failures,execution());let b=run(&c,None,&failures,execution());
        assert_eq!(a.primary().unwrap().unwrap_err().kind,ErrorKind::Host("modeled abstract Host"),"{phase:?}");
        assert_eq!(live(&a),usize::from(phase!=Phase::ReleaseComplete));
        assert_eq!(events(&a).contains(&trusted::Event::ReleaseFailure(0)),phase==Phase::ReleaseAttempt);
        assert_eq!(events(&a).contains(&trusted::Event::ReleaseComplete(0)),phase==Phase::ReleaseComplete);
        assert!(!events(&a).iter().any(|event|matches!(event,trusted::Event::Published(2,_))));
        assert_eq!(compare_pair(&a,&b),Comparison::Equal,"{phase:?}");
    }
}
#[test]
fn earlier_fuel_leaves_future_replay_and_host_items_unconsumed() {
    let e=selected();let c=contract::Contract::extract(&e,e.entry("sample").unwrap(),contract::Limits::default()).unwrap();
    let failures=[call(2,Phase::Protection)];
    let low=ExecutionLimits{steps:1,..execution()};
    let a=run(&c,Some(&[]),&failures,low);let b=run(&c,Some(&[]),&failures,low);
    assert_eq!(a.primary().unwrap().unwrap_err().kind,ErrorKind::ConfiguredLimit("steps"));
    assert!(events(&a).is_empty());
    let controller=a.machine().control.borrow();let controller=controller.as_ref().unwrap();
    assert_eq!(controller.used_responses(),0);assert!(controller.stopped().is_none());
    assert_eq!(compare_pair(&a,&b),Comparison::Equal);
}
