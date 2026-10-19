//! Every acquisition uses source, opaque preparation and the persistent session.
use super::*;
use crate::trusted::{Grants, Selection};

const SAMPLE:&str="fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=sensor_open()or return;let reading=sensor_read(&sensor)or return;return Ok(reading);}";
fn artifact(scenario: Scenario) -> CheckedExecutable {
    let selection = Selection::sensor_demo(scenario, Grants::ALL, &["demo"]).unwrap();
    prepare_execution_with_profile(SAMPLE, AdmissionLimits::default(), &selection).unwrap()
}
fn session(e: &CheckedExecutable) -> ExecutionSession<'_> {
    ExecutionSession::new(e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap()
}
fn run(scenario: Scenario, expected: &str, opens: usize, reads: usize, releases: usize) {
    let e = artifact(scenario);
    let mut s = session(&e);
    let output = e.render_result(s.advance().unwrap(), 4096).unwrap();
    assert_eq!(output, expected);
    assert_eq!(s.live_sensors(), 0);
    assert!(!s.is_stopped());
    let c = &e.trusted.as_ref().unwrap().catalog;
    assert_eq!(
        s.events()
            .iter()
            .filter(|event| matches!(event,Event::Attempt(_,op) if *op==c.operations[0].id))
            .count(),
        opens
    );
    assert_eq!(
        s.events()
            .iter()
            .filter(|event| matches!(event,Event::Attempt(_,op) if *op==c.operations[1].id))
            .count(),
        reads
    );
    assert_eq!(
        s.events()
            .iter()
            .filter(|event| matches!(event, Event::ReleaseComplete(_)))
            .count(),
        releases
    );
    let prefix = s.events().to_vec();
    assert!(s.advance().is_ok());
    assert_eq!(s.events(), prefix);
}
#[test]
fn real_source_open_read_success_releases_before_ok_delivery() {
    run(Scenario::Sample, "Ok(Reading(42))", 1, 1, 1);
}
#[test]
fn real_source_open_no_sensor_returns_error_one_without_owner_or_read() {
    run(Scenario::NoSensor, "Error(SensorError(1))", 1, 0, 0);
}
#[test]
fn real_source_open_recoverable_io_returns_error_three_without_owner() {
    run(Scenario::OpenIoFailure, "Error(SensorError(3))", 1, 0, 0);
}
#[test]
fn real_source_read_no_sample_preserves_owner_until_cleanup_before_error_two() {
    run(Scenario::NoSample, "Error(SensorError(2))", 1, 1, 1);
}
#[test]
fn real_source_read_recoverable_io_preserves_owner_until_cleanup_before_error_three() {
    run(Scenario::ReadIoFailure, "Error(SensorError(3))", 1, 1, 1);
}

fn stopped(fault: Fault, at: u64, scenario: Scenario, expected: ErrorKind, live: usize) {
    let e = artifact(scenario);
    let mut s = session(&e);
    s.machine.trusted.as_mut().unwrap().fault = Some((fault, at));
    assert_eq!(s.advance().unwrap_err().kind, expected);
    assert!(s.is_stopped());
    assert_eq!(s.live_sensors(), live);
    let prefix = s.events().to_vec();
    let steps = s.machine.steps;
    assert!(s.advance().is_err());
    assert_eq!(s.events(), prefix);
    assert_eq!(s.machine.steps, steps);
    assert!(!s
        .events()
        .iter()
        .any(|e| matches!(e, Event::ReleaseComplete(_))));
}
#[test]
fn failure_before_binding_has_no_attempt_or_responsibility() {
    stopped(
        Fault::BeforeInvocationHost,
        0,
        Scenario::Sample,
        ErrorKind::Host("injected trusted invocation boundary"),
        0,
    );
}
#[test]
fn host_after_external_success_retains_actual_receipt_and_never_resumes() {
    stopped(
        Fault::AfterReceiptHost,
        0,
        Scenario::Sample,
        ErrorKind::Host("injected trusted receipt boundary"),
        1,
    );
}
#[test]
fn malformed_success_tag_retains_actual_receipt_with_integrity() {
    stopped(
        Fault::BadTag,
        0,
        Scenario::Sample,
        ErrorKind::Integrity("trusted receipt tag or active payload mismatch"),
        1,
    );
}
#[test]
fn publication_limit_after_external_success_retains_actual_receipt() {
    stopped(
        Fault::PublicationLimit,
        0,
        Scenario::Sample,
        ErrorKind::ConfiguredLimit("trusted publication storage"),
        1,
    );
}
#[test]
fn imported_out_of_bounds_read_stops_with_live_sensor_and_no_source_error() {
    stopped(
        Fault::BadRange,
        1,
        Scenario::Sample,
        ErrorKind::Integrity("imported trusted range or operation Error contract mismatch"),
        1,
    );
}
#[test]
fn legitimate_error_then_release_failure_retains_protected_error_without_delivery() {
    let e = artifact(Scenario::NoSample);
    let mut s = session(&e);
    s.machine.trusted.as_mut().unwrap().fault = Some((Fault::ReleaseFailure, 1));
    assert_eq!(
        s.advance().unwrap_err().kind,
        ErrorKind::Host("injected fixed Sensor release")
    );
    assert_eq!(s.live_sensors(), 1);
    assert!(s.machine.resources.as_ref().unwrap().protected.is_some());
    assert!(matches!(s.events().last(), Some(Event::ReleaseFailure(0))));
    assert!(!s
        .events()
        .iter()
        .any(|e| matches!(e, Event::ReleaseComplete(_))));
    let prefix = s.events().to_vec();
    assert!(s.advance().is_err());
    assert_eq!(s.events(), prefix);
}
#[test]
fn every_fuel_stop_keeps_the_real_effect_prefix_and_all_live_responsibilities() {
    let e = artifact(Scenario::Sample);
    let mut complete = session(&e);
    complete.advance().unwrap();
    let steps = complete.machine.steps;
    let mut after_success = false;
    for fuel in 1..steps {
        let mut s = ExecutionSession::new(
            &e,
            e.entry("demo").unwrap(),
            ExecutionLimits {
                steps: fuel,
                ..ExecutionLimits::default()
            },
        )
        .unwrap();
        assert_eq!(
            s.advance().unwrap_err().kind,
            ErrorKind::ConfiguredLimit("steps")
        );
        let successes = s
            .events()
            .iter()
            .filter(|e| matches!(e, Event::Received(0, 0)))
            .count();
        let released = s
            .events()
            .iter()
            .filter(|e| matches!(e, Event::ReleaseComplete(_)))
            .count();
        assert_eq!(s.live_sensors(), successes - released, "fuel {fuel}");
        if successes > released {
            after_success = true;
        }
        let prefix = s.events().to_vec();
        assert!(s.advance().is_err());
        assert_eq!(s.events(), prefix);
    }
    assert!(after_success);
}
