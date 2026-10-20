//! M2 controls exercise the real opaque validation boundary without dispatch.
use super::*;
use crate::effects::custody::{Holder, Kind};
use crate::trusted::{
    approval::{CompletionHolder, CompletionPhase, ProtectedOutcome},
    Grants, Scenario, Selection,
};

const SOURCE: &str = "fn relay(sensor:Sensor)->Sensor{return sensor;}fn demo()->Result<success:FinishReceipt,failure:SensorError>effects io{let sensor=sensor_open()or return;return sensor_finish(relay(sensor));}";

fn artifact() -> CheckedExecutable {
    let selection = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap();
    prepare_execution_with_profile(SOURCE, AdmissionLimits::default(), &selection).unwrap()
}

fn rejected(e: &CheckedExecutable) {
    assert!(matches!(
        e.validate().unwrap_err().kind,
        ErrorKind::Integrity(_)
    ));
}

#[test]
fn completion_checker_witness_names_invocation_without_a_source_place() {
    let e = artifact();
    e.validate().unwrap();
    let trusted = e.trusted.as_ref().unwrap();
    let invocation = trusted
        .invocations
        .iter()
        .find(|i| i.completion.is_some())
        .unwrap();
    let plan = invocation.completion.as_ref().unwrap();
    let witnesses: Vec<_> = e
        .approval
        .cleanup_sites
        .iter()
        .filter(|s| s.kind == Kind::CompletionRelease)
        .collect();
    assert_eq!(witnesses.len(), 1);
    let witness = witnesses[0];
    assert_eq!(witness.function, invocation.function);
    assert_eq!(witness.holder, Holder::Invocation(invocation.operation));
    assert_eq!(witness.origin, invocation.source);
    assert_eq!(witness.effects, plan.release_effects);
    assert_eq!(plan.holder, CompletionHolder(invocation.site));
    assert_eq!(plan.protected, ProtectedOutcome(invocation.site));
    assert_eq!(plan.phases, CompletionPhase::PROTOCOL);
    let flow = e
        .function(invocation.function)
        .unwrap()
        .temporaries
        .as_ref()
        .unwrap();
    assert_eq!(plan.transfer, flow.sites[plan.input.0].transfer);
    assert_eq!(plan.protection, flow.sites[invocation.site.0].origin);
    assert_eq!(plan.cleanup, flow.sites[invocation.site.0].cleanup);
    assert_eq!(plan.delivery, flow.sites[invocation.site.0].transfer);
    // Relay transfers its parameter, so it has no cleanup realization.
    assert!(e
        .approval
        .cleanup_sites
        .iter()
        .all(|s| s.function != FunctionId(0)));
}

#[test]
fn completion_approval_rejects_independent_holder_and_continuation_mutations() {
    for mutation in 0..9 {
        let mut e = artifact();
        let invocation = e
            .trusted
            .as_mut()
            .unwrap()
            .invocations
            .iter_mut()
            .find(|i| i.completion.is_some())
            .unwrap();
        let plan = invocation.completion.as_mut().unwrap();
        match mutation {
            0 => plan.input.0 += 1,
            1 => plan.holder.0 .0 += 1,
            2 => plan.protected.0 .0 += 1,
            3 => plan.transfer.span.start += 1,
            4 => plan.protection.span.start += 1,
            5 => plan.cleanup.span.start += 1,
            6 => plan.delivery.span.start += 1,
            7 => plan.phases.swap(3, 6),
            _ => invocation.completion = None,
        }
        rejected(&e);
    }
}

#[test]
fn completion_approval_rejects_independent_contract_and_pair_mutations() {
    for mutation in 0..10 {
        let mut e = artifact();
        let trusted = e.trusted.as_mut().unwrap();
        let read = trusted.catalog.operations[1].id;
        let plan = trusted
            .invocations
            .iter_mut()
            .find_map(|i| i.completion.as_mut())
            .unwrap();
        match mutation {
            0 => plan.resource.0 += 1,
            1 => plan.parameter = crate::trusted::Passing::Shared(plan.resource),
            2 => plan.result.0 += 1,
            3 => plan.success.0 += 1,
            4 => plan.failure.0 += 1,
            5 => plan.errors |= 0b10,
            6 => plan.release = read,
            7 => plan.direct_effects = crate::effects::EffectSet::EMPTY,
            8 => plan.release_effects = crate::effects::EffectSet::EMPTY,
            _ => plan.destination = mir::temporaries::Destination::Operand,
        }
        rejected(&e);
    }
}

#[test]
fn completion_all_authority_copies_still_require_exact_mir_derivation() {
    let mut e = artifact();
    e.trusted
        .as_mut()
        .unwrap()
        .invocations
        .iter_mut()
        .find_map(|i| i.completion.as_mut())
        .unwrap()
        .delivery
        .span
        .start += 1;
    let approved = Rc::new(Approval {
        outcomes: e.approval.outcomes.clone(),
        program: e.approval.program.clone(),
        cleanup_sites: e.approval.cleanup_sites.clone(),
        domain: e.approval.domain.clone(),
        trusted: e.trusted.clone(),
    });
    e.approval = approved.clone();
    e.mir_approval = approved.clone();
    e.origin_approval = approved;
    rejected(&e);
}

#[test]
fn completion_mir_cleanup_witness_omission_duplicate_wrong_holder_and_kind_reject() {
    for mutation in 0..5 {
        let mut e = artifact();
        let sites = &mut e.mir.test_functions_mut()[1].cleanup_effect_sites;
        let index = sites
            .iter()
            .position(|s| s.kind == Kind::CompletionRelease)
            .unwrap();
        match mutation {
            0 => {
                sites.remove(index);
            }
            1 => sites.push(sites[index].clone()),
            2 => sites[index].holder = Holder::Place(hir::Place::Parameter(hir::ParameterId(0))),
            3 => sites[index].kind = Kind::Return,
            _ => sites[index].origin.start += 1,
        }
        rejected(&e);
    }
}

#[test]
fn completion_direct_and_cleanup_effects_are_independently_sealed() {
    let e = artifact();
    let trusted = e.trusted.as_ref().unwrap();
    let finish = trusted.catalog.operations[3].id;
    let function = e.function(FunctionId(1)).unwrap();
    assert!(function
        .direct_effects
        .iter()
        .any(|s| s.callee == crate::effects::calls::Callee::Trusted(finish)));
    assert!(function
        .cleanup_effect_sites
        .iter()
        .any(|s| s.holder == Holder::Invocation(finish)));
    for mutation in 0..2 {
        let mut changed = artifact();
        let f = &mut changed.mir.test_functions_mut()[1];
        if mutation == 0 {
            f.direct_effects.clear();
        } else {
            f.cleanup_effect_sites.clear();
        }
        rejected(&changed);
    }
}

#[test]
fn m6_each_completion_event_identity_is_sealed_even_across_all_authority_copies() {
    use crate::trusted::approval::CompletionEvent;
    for index in 0..4 {
        let mut e = artifact();
        let plan = e
            .trusted
            .as_mut()
            .unwrap()
            .invocations
            .iter_mut()
            .find_map(|i| i.completion.as_mut())
            .unwrap();
        plan.events[index] = CompletionEvent::PROTOCOL[(index + 1) % 4];
        let approved = Rc::new(Approval {
            outcomes: e.approval.outcomes.clone(),
            program: e.approval.program.clone(),
            cleanup_sites: e.approval.cleanup_sites.clone(),
            domain: e.approval.domain.clone(),
            trusted: e.trusted.clone(),
        });
        e.approval = approved.clone();
        e.mir_approval = approved.clone();
        e.origin_approval = approved;
        rejected(&e);
    }
}

#[test]
fn m6_finish_specific_catalog_binding_type_subset_effect_grant_origin_and_root_matrix() {
    for mutation in 0..20 {
        let mut e = artifact();
        let t = e.trusted.as_mut().unwrap();
        match mutation {
            0 => t.catalog.operations[3].id = t.catalog.operations[1].id,
            1 => t.catalog.operations[3].binding = crate::trusted::Binding::SensorReadV1,
            2 => t.catalog_identity = "foreign",
            3 => t.profile_identity = "foreign",
            4 => t.compiler = "foreign",
            5 => t.target = "foreign",
            6 => {
                t.catalog.operations[3].parameter =
                    Some(crate::trusted::Passing::Shared(t.catalog.sensor))
            }
            7 => {
                t.catalog.operations[3].success =
                    Some(crate::trusted::Payload::Range(t.catalog.reading))
            }
            8 => t.catalog.operations[3].failure = Some(t.catalog.finish_receipt),
            9 => t.catalog.operations[3].result = t.catalog.operations[1].result,
            10 => t.catalog.operations[3].errors |= 0b10,
            11 => t.catalog.operations[3].effects = crate::effects::EffectSet::EMPTY,
            12 => t.catalog.resource.effects = crate::effects::EffectSet::EMPTY,
            13 | 14 => {
                let grants = Grants {
                    finish: mutation != 13,
                    release: mutation != 14,
                    ..Grants::ALL
                };
                t.catalog.selection =
                    Selection::sensor_demo(Scenario::Sample, grants, &["demo"]).unwrap();
            }
            15 => t.catalog.resource.release = t.catalog.operations[3].id,
            16 => t.catalog.operations[3].origin.row += 1,
            17 => t.ranges[2].id = t.ranges[0].id,
            18 => t.roots[0] = FunctionId(0),
            _ => t.edges.clear(),
        }
        rejected(&e);
        assert!(ExecutionSession::new(&e, FunctionId(1), ExecutionLimits::default()).is_err());
    }
}

#[test]
fn m6_invocation_argument_order_arity_result_source_mir_and_catalog_origins_reject() {
    for mutation in 0..9 {
        let mut e = artifact();
        let invocation = e
            .trusted
            .as_mut()
            .unwrap()
            .invocations
            .iter_mut()
            .find(|i| i.completion.is_some())
            .unwrap();
        match mutation {
            0 => invocation.arguments.push(invocation.arguments[0]),
            1 => invocation.arguments.clear(),
            2 => invocation.arguments[0].0 += 1,
            3 => invocation.function = FunctionId(0),
            4 => invocation.block = mir::BasicBlockId(999),
            5 => invocation.statement += 1,
            6 => invocation.source.start += 1,
            7 => invocation.catalog.row += 1,
            _ => {
                invocation.completion.as_mut().unwrap().delivery.kind =
                    mir::OriginKind::FunctionBoundary
            }
        }
        if mutation == 0 {
            assert_eq!(
                e.validate().unwrap_err().kind,
                ErrorKind::ConfiguredLimit("trusted approval metadata bounds")
            );
        } else {
            rejected(&e);
        }
    }
}

#[test]
fn m9_finish_only_approval_allocations_are_host_and_publish_no_executable() {
    use crate::trusted::approval::{with_failure, Allocation};
    let source="fn finish_only(sensor:Sensor)->Result<success:FinishReceipt,failure:SensorError>effects io{return sensor_finish(sensor);}fn demo(){}";
    for site in [
        Allocation::Root,
        Allocation::Edge,
        Allocation::Argument,
        Allocation::Invocation,
    ] {
        let result = with_failure(site, || {
            prepare_execution_with_profile(
                source,
                AdmissionLimits::default(),
                &Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap(),
            )
        });
        assert!(matches!(result.err().unwrap().kind, ErrorKind::Host(_)));
    }
}
