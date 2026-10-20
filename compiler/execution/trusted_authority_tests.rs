//! Private mutations enter the real opaque validation seam; no fixture executes.
use super::*;
use crate::{
    effects::calls::Callee,
    trusted::{self, approval::Allocation, Grants, Scenario, Selection},
};

const SOURCE:&str="fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=sensor_open()or return;let reading=sensor_read(&sensor)or return;return Ok(reading);}fn unused()->bool{return true;}";
fn selection() -> Selection {
    Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap()
}
fn artifact() -> CheckedExecutable {
    prepare_execution_with_profile(SOURCE, AdmissionLimits::default(), &selection()).unwrap()
}
fn integrity(e: &CheckedExecutable) {
    assert!(matches!(
        e.validate().unwrap_err().kind,
        ErrorKind::Integrity(_)
    ));
}

#[test]
fn trusted_approval_seals_explicit_invocations_roots_signatures_and_receivers() {
    let e = artifact();
    let t = e.trusted.as_ref().unwrap();
    assert_eq!(
        (t.target, t.compiler, t.catalog_identity, t.profile_identity),
        (
            trusted::TARGET,
            trusted::COMPILER,
            trusted::CATALOG,
            trusted::PROFILE
        )
    );
    assert_eq!(t.roots, [FunctionId(0)]);
    assert_eq!(t.invocations.len(), 2);
    for (index, i) in t.invocations.iter().enumerate() {
        assert_eq!(i.operation, t.catalog.operations[index].id);
        assert_eq!(Some(i.result), t.catalog.operations[index].result);
        assert_eq!(i.catalog, t.catalog.operations[index].origin);
        assert_eq!(i.arguments.len(), index);
        let f = e.function(i.function).unwrap();
        let s = &f.temporaries.as_ref().unwrap().sites[i.site.0];
        assert_eq!(s.origin.span, i.source);
        assert_eq!(s.destination, i.destination);
        assert_eq!(s.region, i.region);
    }
    assert!(e.entry("unused").is_err());
    e.validate().unwrap();
}

#[test]
fn trusted_roots_reject_missing_parameterized_and_verified_configuration() {
    for root in ["missing", "with_arg", "proof"] {
        let source="type P=range 0..100;record Battery{charge:P}fn demo(){}fn with_arg(value:bool){}verified fn proof(battery:&mut Battery,amount:P)requires amount<=battery.charge ensures battery.charge==old(battery.charge)-amount{battery.charge-=amount;}";
        let s = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &[root]).unwrap();
        let e = prepare_execution_with_profile(source, AdmissionLimits::default(), &s)
            .err()
            .unwrap();
        assert!(matches!(e.kind, ErrorKind::Integrity(_)), "{e:?}");
    }
}

#[test]
fn structural_call_inventory_includes_empty_effect_helpers_and_both_alternatives() {
    let source="fn flag()->bool{return true;}fn demo()->Result<success:Reading,failure:SensorError>effects io{let condition=flag();let sensor=sensor_open()or return;if condition{return sensor_read(&sensor);}else{return sensor_read(&sensor);}}";
    let e =
        prepare_execution_with_profile(source, AdmissionLimits::default(), &selection()).unwrap();
    let t = e.trusted.as_ref().unwrap();
    assert!(t
        .edges
        .iter()
        .any(|edge| edge.caller == FunctionId(1) && edge.callee == Callee::Source(FunctionId(0))));
    let f = e.function(FunctionId(1)).unwrap();
    assert!(f
        .direct_effects
        .iter()
        .all(|s| s.callee != Callee::Source(FunctionId(0))));
    assert_eq!(
        t.invocations
            .iter()
            .filter(|i| i.operation == t.catalog.operations[1].id)
            .count(),
        2
    );
}

#[test]
fn independent_target_compiler_catalog_profile_and_configuration_mutations_reject() {
    for mutation in 0..5 {
        let mut e = artifact();
        let t = e.trusted.as_mut().unwrap();
        match mutation {
            0 => t.target = "foreign",
            1 => t.compiler = "foreign",
            2 => t.catalog_identity = "foreign",
            3 => t.profile_identity = "foreign",
            _ => {
                t.catalog.selection =
                    Selection::sensor_demo(Scenario::NoSample, Grants::ALL, &["demo"]).unwrap()
            }
        };
        integrity(&e);
    }
}

#[test]
fn independent_grant_binding_release_and_resource_descriptor_mutations_reject() {
    for mutation in 0..6 {
        let mut e = artifact();
        let t = e.trusted.as_mut().unwrap();
        match mutation {
            0 => {
                t.catalog.selection = Selection::sensor_demo(
                    Scenario::Sample,
                    Grants {
                        release: false,
                        ..Grants::ALL
                    },
                    &["demo"],
                )
                .unwrap()
            }
            1 => t.catalog.operations[0].binding = trusted::Binding::SensorReadV1,
            2 => t.catalog.resource.release = t.catalog.operations[1].id,
            3 => t.catalog.resource.payload_bytes += 1,
            4 => t.catalog.resource.id = trusted::ResourceTypeId(0),
            _ => t.catalog.operations[2].source_name = Some("release"),
        };
        integrity(&e);
    }
}

#[test]
fn independently_altered_operation_signature_effect_and_catalog_origin_reject() {
    for mutation in 0..5 {
        let mut e = artifact();
        let t = e.trusted.as_mut().unwrap();
        match mutation {
            0 => {
                t.catalog.operations[1].parameter = Some(trusted::Passing::Owned(t.catalog.sensor))
            }
            1 => t.catalog.operations[0].result = t.catalog.operations[1].result,
            2 => t.catalog.operations[0].effects = crate::effects::EffectSet::EMPTY,
            3 => t.catalog.operations[0].origin.row += 1,
            _ => t.catalog.operations[0].success = Some(trusted::Payload::Range(t.catalog.reading)),
        };
        integrity(&e);
    }
}

#[test]
fn runtime_imported_range_identity_bounds_origin_and_assumptions_mutations_reject() {
    for mutation in 0..5 {
        let mut e = artifact();
        let t = e.trusted.as_mut().unwrap();
        match mutation {
            0 => t.ranges[0].id = t.ranges[1].id,
            1 => t.ranges[0].max += 1,
            2 => t.ranges[1].min -= 1,
            3 => t.ranges[0].origin = hir::DeclarationOrigin::Source(START),
            _ => t.assumptions = "foreign",
        };
        integrity(&e);
    }
}

#[test]
fn invocation_omission_duplication_reordering_and_foreign_numeric_id_reject() {
    let foreign = artifact().trusted.unwrap().catalog.operations[0].id;
    for mutation in 0..4 {
        let mut e = artifact();
        let t = e.trusted.as_mut().unwrap();
        match mutation {
            0 => {
                t.invocations.pop();
            }
            1 => t.invocations.push(t.invocations[0].clone()),
            2 => t.invocations.swap(0, 1),
            _ => t.invocations[0].operation = foreign,
        };
        integrity(&e);
    }
}

#[test]
fn invocation_origin_result_receiver_region_and_argument_site_mutations_reject() {
    for mutation in 0..6 {
        let mut e = artifact();
        let t = e.trusted.as_mut().unwrap();
        match mutation {
            0 => t.invocations[0].source.start += 1,
            1 => t.invocations[0].catalog.row += 1,
            2 => t.invocations[0].result = t.invocations[1].result,
            3 => t.invocations[0].destination = mir::temporaries::Destination::Return,
            4 => t.invocations[0].region = mir::temporaries::RegionId(999),
            _ => t.invocations[1].arguments[0] = mir::temporaries::SiteId(999),
        };
        integrity(&e);
    }
}

#[test]
fn independent_root_edge_and_authority_copy_mutations_reject() {
    for mutation in 0..5 {
        let mut e = artifact();
        match mutation {
            0 => e.trusted.as_mut().unwrap().roots[0] = FunctionId(1),
            1 => {
                e.trusted.as_mut().unwrap().edges.pop();
            }
            2 => e.trusted.as_mut().unwrap().edges[0].callee = Callee::Source(FunctionId(1)),
            3 => e.trusted = None,
            _ => {
                let foreign = artifact();
                e.origin_approval = foreign.approval;
            }
        };
        integrity(&e);
    }
}

#[test]
fn altered_all_runtime_approval_copies_still_require_exact_mir_derivation() {
    let mut e = artifact();
    e.trusted.as_mut().unwrap().invocations[0].source.start += 1;
    let approved = Rc::new(Approval {
        schema: e.approval.schema,
        outcomes: e.approval.outcomes.clone(),
        program: e.approval.program.clone(),
        cleanup_sites: e.approval.cleanup_sites.clone(),
        domain: e.approval.domain.clone(),
        trusted: e.trusted.clone(),
    });
    e.approval = approved.clone();
    e.mir_approval = approved.clone();
    e.origin_approval = approved;
    integrity(&e);
}

fn allocation_is_host(site: Allocation, name: &'static str) {
    let result = trusted::approval::with_failure(site, || {
        prepare_execution_with_profile(SOURCE, AdmissionLimits::default(), &selection())
    });
    assert_eq!(
        result
            .err()
            .expect("no executable after detected allocation failure")
            .kind,
        ErrorKind::Host(name)
    );
}
#[test]
fn trusted_root_reservation_failure_is_host_and_publishes_no_executable() {
    allocation_is_host(Allocation::Root, "trusted root approval allocation");
}
#[test]
fn trusted_edge_reservation_failure_is_host_and_publishes_no_executable() {
    allocation_is_host(Allocation::Edge, "trusted structural edge allocation");
}
#[test]
fn trusted_argument_reservation_failure_is_host_and_publishes_no_executable() {
    allocation_is_host(
        Allocation::Argument,
        "trusted argument destination allocation",
    );
}
#[test]
fn trusted_invocation_reservation_failure_is_host_and_publishes_no_executable() {
    allocation_is_host(Allocation::Invocation, "trusted invocation plan allocation");
}

#[test]
fn trusted_metadata_shape_bounds_precede_deep_authority_comparison() {
    let mut e = artifact();
    let t = e.trusted.as_mut().unwrap();
    let i = t.invocations[0].clone();
    t.invocations.resize(e.limits.tokens + 1, i);
    assert_eq!(
        e.validate().unwrap_err().kind,
        ErrorKind::ConfiguredLimit("trusted approval metadata bounds")
    );
}

#[test]
fn each_resource_result_child_effect_and_variant_plan_mutation_rejects_before_entry() {
    for mutation in 0..6 {
        let mut e = artifact();
        let plan = &mut e.result_plans[0];
        match mutation {
            0 => {
                plan.success = Some(hir::results::VariantChild::Resource(hir::ResourceTypeId(
                    usize::MAX,
                )))
            }
            1 => plan.success = None,
            2 => plan.failure = plan.success,
            3 => plan.result = hir::ResultTypeId(usize::MAX),
            4 => plan.effects = crate::effects::EffectSet::EMPTY,
            _ => plan.resource_bearing = false,
        }
        integrity(&e);
        assert!(matches!(
            ExecutionSession::new(&e, FunctionId(0), ExecutionLimits::default())
                .err()
                .unwrap()
                .kind,
            ErrorKind::Integrity(_)
        ));
    }
}

#[test]
fn independent_resource_cleanup_realization_and_protected_holder_mutations_reject() {
    let source="fn relay(sensor:Sensor)->Sensor{return sensor;}fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=sensor_open()or return;let moved=relay(sensor);return sensor_read(&moved);}";
    for mutation in 0..4 {
        let mut e =
            prepare_execution_with_profile(source, AdmissionLimits::default(), &selection())
                .unwrap();
        let flow = e.mir.test_functions_mut()[0].cleanup.as_mut().unwrap();
        assert!(flow.protected_return);
        assert_eq!(flow.parameters.len(), 1);
        match mutation {
            0 => flow.protected_return = false,
            1 => flow.parameters.clear(),
            2 => flow.blocks[0].exit.clear(),
            _ => flow.scopes[0].span.start += 1,
        }
        integrity(&e);
    }
}
