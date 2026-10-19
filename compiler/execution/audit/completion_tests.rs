use super::*;
use crate::trusted::{Grants, Scenario, Selection};

fn artifact(source: &str) -> CheckedExecutable {
    let selection =
        Selection::sensor_demo(Scenario::FinishPartialIoFailure, Grants::ALL, &["demo"]).unwrap();
    prepare_execution_with_profile(source, AdmissionLimits::default(), &selection).unwrap()
}

#[test]
fn rooted_completion_audit_lists_roles_effect_origins_and_both_release_paths_without_execution() {
    let source="fn status()->FinishReceipt{return 1;}fn flag()->bool{return true;}fn demo()->Result<success:FinishReceipt,failure:SensorError>effects io{let sensor=sensor_open()or return;if flag(){return sensor_finish(sensor);}else{return Ok(status());}}";
    let e = artifact(source);
    let before = e.render_authority(ExecutionLimits::default()).unwrap();
    for text in [
        "possible finish protocol:",
        "completion-holder=",
        "protected-result=",
        "direct-finish-effects=io",
        "cleanup-derived-release-effects=io",
        "event-identities=",
        "possible completion cleanup:",
        "possible release-only cleanup:",
        "Copy status data, source-formable and not proof or authority",
        "terminal non-delivery",
        "source-callable",
    ] {
        assert!(before.contains(text), "{text}");
    }
    let mut session =
        ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
    assert!(session.events().is_empty());
    assert_eq!(
        session
            .render_authority(ExecutionLimits::default())
            .unwrap(),
        before
    );
    assert!(session.events().is_empty());
    session.advance().unwrap();
    assert_eq!(
        session
            .render_authority(ExecutionLimits::default())
            .unwrap(),
        before
    );
}

#[test]
fn unused_finish_grant_and_source_status_never_become_rooted_finish_use() {
    let e=artifact("fn finish_unused(sensor:Sensor)->Result<success:FinishReceipt,failure:SensorError>effects io{return sensor_finish(sensor);}fn demo()->FinishReceipt{return 1;}");
    let text = e.render_authority(ExecutionLimits::default()).unwrap();
    let finish_line = text
        .lines()
        .find(|line| line.contains("binding=SensorFinishV1"))
        .unwrap();
    assert!(finish_line.contains("granted=true; structurally-used=false"));
    assert!(!text.contains("possible finish protocol:"));
    let mut session =
        ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
    session.advance().unwrap();
    assert!(session.events().is_empty());
}

#[test]
fn recursive_structural_edges_terminate_without_running_completion() {
    let e =
        artifact("fn recursive()->bool{return recursive();}fn demo()->bool{return recursive();}");
    let text = e.render_authority(ExecutionLimits::default()).unwrap();
    assert!(text.contains("reachable edge: caller=0"));
    assert!(!text.contains("possible finish protocol:"));
}
