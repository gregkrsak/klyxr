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
