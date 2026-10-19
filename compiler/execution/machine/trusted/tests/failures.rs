use super::*;

fn terminal(s: &mut ExecutionSession<'_>, kind: ErrorKind, live: usize) {
    assert_eq!(s.advance().unwrap_err().kind, kind);
    assert!(s.is_stopped());
    assert_eq!(s.live_sensors(), live);
    let prefix = s.events().to_vec();
    let steps = s.machine.steps;
    assert_eq!(s.advance().unwrap_err().kind, kind);
    assert_eq!(s.events(), prefix);
    assert_eq!(s.machine.steps, steps);
}

#[test]
fn each_actual_receiving_allocation_failure_is_host_before_any_external_attempt() {
    for site in [
        Allocation::Identity,
        Allocation::Provenance,
        Allocation::Audit,
        Allocation::Operand,
        Allocation::ResultCell,
        Allocation::TemporaryOwner,
        Allocation::TemporaryEvent,
    ] {
        let e = artifact(Scenario::Sample);
        let mut s = session(&e);
        s.machine.trusted.as_mut().unwrap().fault = Some((Fault::Allocation(site), 0));
        terminal(
            &mut s,
            ErrorKind::Host("injected trusted allocation boundary"),
            0,
        );
        assert!(s.events().is_empty());
    }
}

#[test]
fn foreign_fresh_identity_is_integrity_while_receipt_owns_the_actual_token() {
    stopped(
        Fault::ForeignSensor,
        0,
        Scenario::Sample,
        ErrorKind::Integrity("duplicate or foreign fresh Sensor identity"),
        1,
    );
}

#[test]
fn error_carrying_sensor_is_integrity_and_cannot_become_no_owner_error() {
    stopped(
        Fault::ErrorWithSensor,
        0,
        Scenario::Sample,
        ErrorKind::Integrity("trusted receipt tag or active payload mismatch"),
        1,
    );
}

#[test]
fn wrong_imported_nominal_range_identity_is_integrity_without_conversion() {
    stopped(
        Fault::WrongRangeIdentity,
        1,
        Scenario::Sample,
        ErrorKind::Integrity("imported trusted range or operation Error contract mismatch"),
        1,
    );
}

#[test]
fn in_bounds_but_unenumerated_operation_errors_are_integrity() {
    stopped(
        Fault::WrongErrorCode,
        0,
        Scenario::NoSensor,
        ErrorKind::Integrity("imported trusted range or operation Error contract mismatch"),
        0,
    );
    stopped(
        Fault::WrongErrorCode,
        1,
        Scenario::NoSample,
        ErrorKind::Integrity("imported trusted range or operation Error contract mismatch"),
        1,
    );
}

#[test]
fn receipt_operation_provenance_rejects_before_result_publication() {
    stopped(
        Fault::WrongInvocation,
        0,
        Scenario::Sample,
        ErrorKind::Integrity("trusted receipt operation provenance mismatch"),
        1,
    );
}

#[test]
fn duplicate_identity_after_an_earlier_success_retains_both_actual_responsibilities() {
    let source="fn demo()->Result<success:bool,failure:SensorError>effects io{let first=sensor_open()or return;let second=sensor_open()or return;return Ok(true);}";
    let selection = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap();
    let e = prepare_execution_with_profile(source, AdmissionLimits::default(), &selection).unwrap();
    let mut s = session(&e);
    s.machine.trusted.as_mut().unwrap().fault = Some((Fault::DuplicateSensor, 1));
    terminal(
        &mut s,
        ErrorKind::Integrity("duplicate or foreign fresh Sensor identity"),
        2,
    );
    assert_eq!(
        s.events()
            .iter()
            .filter(|v| matches!(v, Event::Published(..)))
            .count(),
        1
    );
    assert!(!s
        .events()
        .iter()
        .any(|v| matches!(v, Event::ReleaseAttempt(..))));
}

#[test]
fn live_resource_limit_exact_neighbor_stops_before_the_excess_attempt() {
    for (count, success) in [(2, true), (3, false)] {
        let mut source =
            "fn demo()->Result<success:bool,failure:SensorError>effects io{".to_owned();
        for i in 0..count {
            source.push_str(&format!("let sensor{i}=sensor_open()or return;"));
        }
        source.push_str("return Ok(true);}");
        let selection = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"])
            .unwrap()
            .with_sensor_limit(2)
            .unwrap();
        let e = prepare_execution_with_profile(&source, AdmissionLimits::default(), &selection)
            .unwrap();
        let mut s = session(&e);
        if success {
            s.advance().unwrap();
            assert_eq!(s.live_sensors(), 0);
        } else {
            terminal(&mut s, ErrorKind::ConfiguredLimit("live Sensor count"), 2);
        }
        assert_eq!(
            s.events()
                .iter()
                .filter(|v| matches!(v, Event::Attempt(..)))
                .count(),
            2
        );
    }
}

#[test]
fn exact_combined_runtime_storage_and_fuel_neighbors_are_sufficient_and_insufficient() {
    let e = artifact(Scenario::Sample);
    let mut normal = session(&e);
    normal.advance().unwrap();
    let peak = normal.machine.max_storage.get();
    let steps = normal.machine.steps;
    for (storage, fuel, success) in [
        (peak, steps, true),
        (peak - 1, steps, false),
        (peak, steps - 1, false),
    ] {
        let mut s = ExecutionSession::new(
            &e,
            e.entry("demo").unwrap(),
            ExecutionLimits {
                working_storage: storage,
                steps: fuel,
                ..ExecutionLimits::default()
            },
        )
        .unwrap();
        if success {
            s.advance().unwrap();
            assert_eq!(s.live_sensors(), 0);
        } else {
            assert!(matches!(
                s.advance().unwrap_err().kind,
                ErrorKind::ConfiguredLimit(_)
            ));
            let prefix = s.events().to_vec();
            assert!(s.advance().is_err());
            assert_eq!(s.events(), prefix);
        }
    }
}

#[test]
fn tiny_profile_session_header_and_stopped_diagnostics_are_charged_exactly() {
    let selection = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap();
    let e = prepare_execution_with_profile("fn demo(){}", AdmissionLimits::default(), &selection)
        .unwrap();
    let s = session(&e);
    let header = s.machine.storage().unwrap();
    assert!(
        header
            >= size_of::<State>() + ExecutionLimits::default().trace_depth * size_of::<CallTrace>()
    );
    assert!(ExecutionSession::new(
        &e,
        e.entry("demo").unwrap(),
        ExecutionLimits {
            working_storage: header,
            ..ExecutionLimits::default()
        }
    )
    .is_ok());
    assert!(matches!(
        ExecutionSession::new(
            &e,
            e.entry("demo").unwrap(),
            ExecutionLimits {
                working_storage: header - 1,
                ..ExecutionLimits::default()
            }
        )
        .err()
        .unwrap()
        .kind,
        ErrorKind::ConfiguredLimit("working storage")
    ));
}

#[test]
fn legitimate_error_then_fuel_limit_retains_protected_error_and_completed_prefix() {
    let e = artifact(Scenario::NoSample);
    let mut normal = session(&e);
    normal.advance().unwrap();
    let mut witnessed = false;
    for fuel in 1..normal.machine.steps {
        let mut s = ExecutionSession::new(
            &e,
            e.entry("demo").unwrap(),
            ExecutionLimits {
                steps: fuel,
                ..ExecutionLimits::default()
            },
        )
        .unwrap();
        assert!(s.advance().is_err());
        if s.events()
            .iter()
            .any(|v| matches!(v, Event::Published(1, hir::ResultTag::Error)))
            && s.machine
                .resources
                .as_ref()
                .is_some_and(|r| r.protected.is_some())
        {
            witnessed = true;
            assert_eq!(
                s.advance().unwrap_err().kind,
                ErrorKind::ConfiguredLimit("steps")
            );
            let prefix = s.events().to_vec();
            assert!(s.advance().is_err());
            assert_eq!(s.events(), prefix);
        }
    }
    assert!(witnessed);
}

#[test]
fn cleanup_host_keeps_protected_ok_and_cli_host_exit_class_two() {
    let e = artifact(Scenario::Sample);
    let mut s = session(&e);
    s.machine.trusted.as_mut().unwrap().fault = Some((Fault::ReleaseFailure, 1));
    let error = s.advance().unwrap_err();
    let mut output = Vec::new();
    assert_eq!(
        crate::run_reporting::execution_error(&e, error, 4096, &mut output),
        Err(2)
    );
    assert!(String::from_utf8(output)
        .unwrap()
        .contains("detected host/tool failure"));
    assert_eq!(s.live_sensors(), 1);
    assert!(s.machine.resources.as_ref().unwrap().protected.is_some());
    let prefix = s.events().to_vec();
    assert!(s.advance().is_err());
    assert_eq!(s.events(), prefix);
}

#[test]
fn release_audit_allocation_failure_retains_owner_without_a_release_attempt() {
    let e = artifact(Scenario::Sample);
    let mut s = session(&e);
    s.machine.trusted.as_mut().unwrap().fault =
        Some((Fault::Allocation(Allocation::ReleaseAudit), 2));
    terminal(
        &mut s,
        ErrorKind::Host("injected trusted allocation boundary"),
        1,
    );
    assert!(!s
        .events()
        .iter()
        .any(|v| matches!(v, Event::ReleaseAttempt(..))));
}

#[test]
fn result_rendering_or_writer_failure_cannot_resume_or_drop_stopped_custody() {
    let e = artifact(Scenario::NoSample);
    let mut s = session(&e);
    s.machine.trusted.as_mut().unwrap().fault = Some((Fault::ReleaseFailure, 1));
    let error = s.advance().unwrap_err();
    assert_eq!(
        crate::run_reporting::execution_error(&e, error, 1, std::io::sink()),
        Err(2)
    );
    assert_eq!(s.live_sensors(), 1);
    let prefix = s.events().to_vec();
    assert!(s.advance().is_err());
    assert_eq!(s.events(), prefix);
}

#[test]
fn range_trap_after_acquisition_is_terminal_and_keeps_sensor_without_cleanup() {
    let source="fn zero()->Reading{return 0;}fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=sensor_open()or return;let value=zero();let invalid=value-1;return Ok(invalid);}";
    let selection = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap();
    let e = prepare_execution_with_profile(source, AdmissionLimits::default(), &selection).unwrap();
    let mut s = session(&e);
    assert!(matches!(
        s.advance().unwrap_err().kind,
        ErrorKind::RangeTrap { .. }
    ));
    assert_eq!(s.live_sensors(), 1);
    let prefix = s.events().to_vec();
    assert!(s.advance().is_err());
    assert_eq!(s.events(), prefix);
    assert!(!s
        .events()
        .iter()
        .any(|v| matches!(v, Event::ReleaseAttempt(..))));
}

#[test]
fn all_fuel_cuts_through_partial_argument_entry_and_completed_release_prefix_conserve_tokens() {
    let source="fn relay(sensor:Sensor)->Sensor{return sensor;}fn consume(a:Sensor,b:Sensor)->bool effects io{return true;}fn demo()->Result<success:bool,failure:SensorError>effects io{let a=sensor_open()or return;let b=sensor_open()or return;return Ok(consume(relay(a),relay(b)));}";
    let selection = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap();
    let e = prepare_execution_with_profile(source, AdmissionLimits::default(), &selection).unwrap();
    let mut normal = session(&e);
    normal.advance().unwrap();
    let mut partial_release = false;
    for fuel in 1..normal.machine.steps {
        let mut s = ExecutionSession::new(
            &e,
            e.entry("demo").unwrap(),
            ExecutionLimits {
                steps: fuel,
                ..ExecutionLimits::default()
            },
        )
        .unwrap();
        assert!(s.advance().is_err());
        let acquired = s
            .events()
            .iter()
            .filter(|v| matches!(v, Event::Received(_, 0)))
            .count();
        let released = s
            .events()
            .iter()
            .filter(|v| matches!(v, Event::ReleaseComplete(_)))
            .count();
        assert_eq!(s.live_sensors(), acquired - released, "fuel {fuel}");
        partial_release |= released == 1 && s.live_sensors() == 1;
        let prefix = s.events().to_vec();
        assert!(s.advance().is_err());
        assert_eq!(s.events(), prefix);
    }
    assert!(partial_release);
}
