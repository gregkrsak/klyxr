//! Real source and private boundary faults run only in GitHub Actions.
use super::*;
use crate::trusted::{Grants, Selection};

const SOURCE: &str = "fn demo()->Result<success:FinishReceipt,failure:SensorError>effects io{let sensor=sensor_open()or return;return sensor_finish(sensor);}";

fn artifact(scenario: Scenario) -> CheckedExecutable {
    let selection = Selection::sensor_demo(scenario, Grants::ALL, &["demo"]).unwrap();
    prepare_execution_with_profile(SOURCE, AdmissionLimits::default(), &selection).unwrap()
}

#[test]
fn m4_success_error_and_partial_error_release_before_inner_delivery() {
    for (scenario, expected, tag, prefix) in [
        (
            Scenario::Sample,
            "Ok(FinishReceipt(1))",
            hir::ResultTag::Ok,
            false,
        ),
        (
            Scenario::FinishIoFailure,
            "Error(SensorError(3))",
            hir::ResultTag::Error,
            false,
        ),
        (
            Scenario::FinishPartialIoFailure,
            "Error(SensorError(3))",
            hir::ResultTag::Error,
            true,
        ),
    ] {
        let e = artifact(scenario);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
            .unwrap();
        assert_eq!(
            e.render_result(s.advance().unwrap(), 4096).unwrap(),
            expected
        );
        assert_eq!(s.live_sensors(), 0);
        let events = s.events();
        let finish = e.trusted.as_ref().unwrap().catalog.operations[3].id;
        let release = e.trusted.as_ref().unwrap().catalog.resource.release;
        let start = events
            .iter()
            .position(|e| *e == Event::Attempt(1, finish))
            .unwrap();
        let mut expected = vec![Event::Attempt(1, finish)];
        if prefix {
            expected.push(Event::FinishPrefix(1, 1));
        }
        expected.extend([
            Event::Received(1, u8::from(tag == hir::ResultTag::Error)),
            Event::FinishProtected(1, tag),
            Event::ReleaseAttempt(0, release),
            Event::ReleaseComplete(0),
            Event::Published(1, tag),
        ]);
        assert_eq!(&events[start..], expected);
        let before = events.to_vec();
        s.advance().unwrap();
        assert_eq!(s.events(), before);
    }
}

#[test]
fn m4_both_outcomes_survive_release_failure_with_exact_prefix_and_no_delivery() {
    for scenario in [
        Scenario::Sample,
        Scenario::FinishIoFailure,
        Scenario::FinishPartialIoFailure,
    ] {
        let e = artifact(scenario);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
            .unwrap();
        s.machine.trusted.as_mut().unwrap().fault = Some((Fault::ReleaseFailure, 1));
        assert_eq!(
            s.advance().unwrap_err().kind,
            ErrorKind::Host("injected fixed Sensor release")
        );
        assert_eq!(s.live_sensors(), 1);
        let state = s.machine.trusted.as_ref().unwrap();
        let pending = state.completion.as_ref().unwrap();
        assert_eq!(
            pending.phase,
            crate::trusted::approval::CompletionPhase::Releasing
        );
        let owner = pending.protected.unwrap();
        let cell = &s.machine.results.as_ref().unwrap().cells[owner.0];
        assert_eq!(cell.phase, results::Phase::CompletionProtected);
        assert!(matches!(
            cell.payload,
            Some(Value::Public(RuntimeValue::Range(_)))
        ));
        assert!(s.machine.resources.as_ref().unwrap().current.is_some());
        assert!(!s
            .events()
            .iter()
            .any(|e| matches!(e, Event::Published(1, _) | Event::ReleaseComplete(_))));
        let before = s.events().to_vec();
        let steps = s.machine.steps;
        assert!(s.advance().is_err());
        assert_eq!(s.events(), before);
        assert_eq!(s.machine.steps, steps);
    }
}

#[test]
fn m4_post_response_host_preserves_protected_outcome_and_completion_owner() {
    let e = artifact(Scenario::FinishPartialIoFailure);
    let mut s =
        ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
    s.machine.trusted.as_mut().unwrap().fault = Some((Fault::AfterReceiptHost, 1));
    assert_eq!(
        s.advance().unwrap_err().kind,
        ErrorKind::Host("injected trusted receipt boundary")
    );
    let pending = s
        .machine
        .trusted
        .as_ref()
        .unwrap()
        .completion
        .as_ref()
        .unwrap();
    assert!(pending.value.is_some() && pending.protected.is_some());
    assert_eq!(s.live_sensors(), 1);
    assert!(s.events().contains(&Event::FinishPrefix(1, 1)));
    assert!(s
        .events()
        .contains(&Event::FinishProtected(1, hir::ResultTag::Error)));
    assert!(!s
        .events()
        .iter()
        .any(|e| matches!(e, Event::ReleaseAttempt(..) | Event::Published(1, _))));
}

#[test]
fn m4_malformed_completion_response_retains_sensor_and_real_prefix() {
    for fault in [
        Fault::BadTag,
        Fault::BadRange,
        Fault::WrongRangeIdentity,
        Fault::WrongErrorCode,
        Fault::WrongInvocation,
    ] {
        let e = artifact(Scenario::FinishPartialIoFailure);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
            .unwrap();
        s.machine.trusted.as_mut().unwrap().fault = Some((fault, 1));
        assert_eq!(
            s.advance().unwrap_err().kind,
            ErrorKind::Integrity("completion response tag/payload/provenance contract")
        );
        assert_eq!(s.live_sensors(), 1);
        let state = s.machine.trusted.as_ref().unwrap();
        assert!(state.receipt.is_some());
        assert!(state.completion.as_ref().unwrap().value.is_some());
        assert!(s.events().contains(&Event::FinishPrefix(1, 1)));
        assert!(s.events().contains(&Event::Received(1, 1)));
        assert!(!s
            .events()
            .iter()
            .any(|e| matches!(e, Event::FinishProtected(..) | Event::Published(1, _))));
    }
}

#[test]
fn m4_every_fuel_cut_preserves_a_lawful_prefix_without_repeat_effects() {
    for scenario in [
        Scenario::Sample,
        Scenario::FinishIoFailure,
        Scenario::FinishPartialIoFailure,
    ] {
        let e = artifact(scenario);
        let mut full =
            ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
                .unwrap();
        full.advance().unwrap();
        let expected = full.events().to_vec();
        let mut protected = false;
        let mut released_before_delivery = false;
        for fuel in 1..full.machine.steps {
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
            assert!(expected.starts_with(s.events()), "fuel={fuel}");
            let acquired = usize::from(s.events().contains(&Event::Received(0, 0)));
            let released = usize::from(s.events().contains(&Event::ReleaseComplete(0)));
            assert_eq!(s.live_sensors(), acquired - released);
            super::completion_bound_tests::conserved(&s.machine);
            if let Some(pending) = &s.machine.trusted.as_ref().unwrap().completion {
                if let Some(owner) = pending.protected {
                    protected = true;
                    assert!(s.machine.results.as_ref().unwrap().cells[owner.0]
                        .payload
                        .is_some());
                }
                if pending.phase == crate::trusted::approval::CompletionPhase::Released {
                    released_before_delivery = true;
                    assert!(pending.value.is_none() && pending.protected.is_some());
                    assert!(!s
                        .events()
                        .iter()
                        .any(|e| matches!(e, Event::Published(1, _))));
                }
            }
            let before = s.events().to_vec();
            let steps = s.machine.steps;
            assert!(s.advance().is_err());
            assert_eq!(s.events(), before);
            assert_eq!(s.machine.steps, steps);
        }
        assert!(protected && released_before_delivery);
    }
}

#[test]
fn m6_bad_ticket_realm_fresh_identity_and_frame_reject_before_finish_attempt() {
    for fault in [
        Fault::CompletionTicket,
        Fault::CompletionRealm,
        Fault::CompletionIdentity,
        Fault::CompletionFrame,
    ] {
        let e = artifact(Scenario::Sample);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
            .unwrap();
        s.machine.trusted.as_mut().unwrap().fault = Some((fault, 1));
        assert!(matches!(
            s.advance().unwrap_err().kind,
            ErrorKind::Integrity(_)
        ));
        assert_eq!(s.live_sensors(), 1);
        let state = s.machine.trusted.as_ref().unwrap();
        assert!(state.completion.as_ref().unwrap().value.is_some());
        assert_eq!(
            s.events()
                .iter()
                .filter(|e| matches!(e, Event::Attempt(..)))
                .count(),
            1
        );
        assert!(!s
            .events()
            .iter()
            .any(|e| matches!(e, Event::ReleaseAttempt(..))));
        let prefix = s.events().to_vec();
        assert!(s.advance().is_err());
        assert_eq!(s.events(), prefix);
    }
}

#[test]
fn m6_finish_error_two_is_integrity_with_actual_partial_prefix_retained() {
    let e = artifact(Scenario::FinishPartialIoFailure);
    let mut s =
        ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
    s.machine.trusted.as_mut().unwrap().fault = Some((Fault::CompletionErrorTwo, 1));
    assert!(matches!(
        s.advance().unwrap_err().kind,
        ErrorKind::Integrity(_)
    ));
    assert_eq!(s.live_sensors(), 1);
    assert!(s.events().contains(&Event::FinishPrefix(1, 1)));
    assert!(s.machine.trusted.as_ref().unwrap().receipt.is_some());
}

#[test]
fn m6_late_protected_tag_payload_realm_production_and_receiver_corruptions_keep_prefix() {
    for fault in [
        Fault::ProtectedTag,
        Fault::ProtectedPayload,
        Fault::ProtectedRealm,
        Fault::ProtectedIdentity,
        Fault::ProtectedReceiver,
    ] {
        let e = artifact(Scenario::FinishPartialIoFailure);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
            .unwrap();
        s.machine.trusted.as_mut().unwrap().fault = Some((fault, 1));
        assert!(matches!(
            s.advance().unwrap_err().kind,
            ErrorKind::Integrity(_)
        ));
        assert_eq!(s.live_sensors(), 1);
        assert!(s.events().contains(&Event::FinishPrefix(1, 1)));
        assert!(s
            .events()
            .contains(&Event::FinishProtected(1, hir::ResultTag::Error)));
        assert!(s
            .machine
            .trusted
            .as_ref()
            .unwrap()
            .completion
            .as_ref()
            .unwrap()
            .value
            .is_some());
        assert!(!s
            .events()
            .iter()
            .any(|e| matches!(e, Event::Published(1, _) | Event::ReleaseAttempt(..))));
        let prefix = s.events().to_vec();
        assert!(s.advance().is_err());
        assert_eq!(s.events(), prefix);
    }
}

#[test]
fn m9_coordinated_protected_payload_and_production_forgery_cannot_widen_finish_errors() {
    for fault in [Fault::ProtectedContractOne, Fault::ProtectedContractTwo] {
        let e = artifact(Scenario::FinishPartialIoFailure);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
            .unwrap();
        s.machine.trusted.as_mut().unwrap().fault = Some((fault, 1));
        assert_eq!(
            s.advance().unwrap_err().kind,
            ErrorKind::Integrity("trusted production violates exact operation outcome contract")
        );
        assert_eq!(s.live_sensors(), 1);
        assert!(s.events().contains(&Event::FinishPrefix(1, 1)));
        assert!(s
            .events()
            .contains(&Event::FinishProtected(1, hir::ResultTag::Error)));
        assert!(!s
            .events()
            .iter()
            .any(|event| matches!(event, Event::ReleaseAttempt(..) | Event::Published(1, _))));
        let prefix = s.events().to_vec();
        assert!(s.advance().is_err());
        assert_eq!(s.events(), prefix);
    }
}

#[test]
fn m9_input_activation_frame_corruption_rejects_before_handoff_and_attempt() {
    let e = artifact(Scenario::Sample);
    let mut session = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
    session.machine.trusted.as_mut().unwrap().fault = Some((Fault::CompletionInputFrame,1));
    assert_eq!(session.advance().unwrap_err().kind, ErrorKind::Integrity("completion receiving region mismatch"));
    assert_eq!(session.live_sensors(),1);
    super::completion_bound_tests::conserved(&session.machine);
    assert!(session.machine.trusted.as_ref().unwrap().completion.is_none());
    assert_eq!(session.events().iter().filter(|event| matches!(event,Event::Attempt(..))).count(),1);
    let prefix = session.events().to_vec();
    assert!(session.advance().is_err());
    assert_eq!(session.events(),prefix);
}
