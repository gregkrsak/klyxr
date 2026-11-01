use super::*;
use crate::execution::{
    prepare_execution, prepare_execution_with_profile, AdmissionLimits, ErrorKind,
};

fn selection(grants: Grants) -> Selection {
    Selection::sensor_demo(Scenario::Sample, grants, &["demo"]).unwrap()
}
fn program(source: &str, grants: Grants) -> hir::Program {
    let context = Context::new(AdmissionLimits::default());
    let ast = crate::parse_source(source).unwrap();
    let resolved = crate::resolve::resolve_profile(&ast, &context, &selection(grants)).unwrap();
    crate::types::check(resolved).unwrap()
}
fn rejected(source: &str, grants: Grants) -> String {
    prepare_execution_with_profile(source, AdmissionLimits::default(), &selection(grants))
        .err()
        .expect("source must reject")
        .kind
        .frontend_message()
}

trait FrontendMessage {
    fn frontend_message(self) -> String;
}
impl FrontendMessage for ErrorKind {
    fn frontend_message(self) -> String {
        match self {
            ErrorKind::Frontend { message, .. } => message,
            other => panic!("expected frontend error, got {other:?}"),
        }
    }
}

#[test]
fn selected_ranges_precede_signatures_and_have_honest_catalog_origins() {
    let p = program(
        "fn demo()->Reading{return 4095;} fn failure_value()->SensorError{return 3;}",
        Grants::ALL,
    );
    let c = p.trusted.as_ref().unwrap();
    assert_eq!(p.ranges.len(), 3);
    for (index, (name, min, max)) in [
        ("Reading", 0, 4095),
        ("SensorError", 1, 3),
        ("FinishReceipt", 1, 1),
    ]
    .into_iter()
    .enumerate()
    {
        let r = &p.ranges[index];
        assert_eq!(
            (r.id, r.name.as_str(), r.min, r.max),
            (RangeTypeId(index), name, min, max)
        );
        assert_eq!(
            r.origin,
            hir::DeclarationOrigin::Catalog(c.origin(if index == 2 { 6 } else { index }))
        );
    }
    c.validate(&p).unwrap();
}

#[test]
fn imported_ranges_keep_existing_arithmetic_comparison_copy_and_field_rules() {
    let source = "record Box { value: Reading } fn demo()->bool { let a=reading(); let b=a; let boxed=Box { value:b }; return boxed.value==a-reading(); } fn reading()->Reading{return 0;}";
    prepare_execution_with_profile(source, AdmissionLimits::default(), &selection(Grants::ALL))
        .unwrap();
}

#[test]
fn imported_range_literal_neighbors_preserve_existing_frontend_checks() {
    for (ty, good, bad) in [
        ("Reading", 4095, 4096),
        ("SensorError", 1, 0),
        ("SensorError", 3, 4),
    ] {
        prepare_execution_with_profile(
            &format!("fn demo()->{ty}{{return {good};}}"),
            AdmissionLimits::default(),
            &selection(Grants::ALL),
        )
        .unwrap();
        assert!(
            rejected(&format!("fn demo()->{ty}{{return {bad};}}"), Grants::ALL).contains("range")
        );
    }
}

#[test]
fn equal_source_range_bounds_do_not_establish_imported_nominal_compatibility() {
    assert!(rejected(
        "type Same=range 0..4095; fn same()->Same{return 1;} fn demo()->Reading{return same();}",
        Grants::ALL
    )
    .contains("type"));
}

#[test]
fn selected_type_namespace_collisions_reject_before_signatures() {
    for name in ["Reading", "SensorError", "Sensor"] {
        assert_eq!(
            rejected(
                &format!("type {name}=range 0..1; fn demo(){{}}"),
                Grants::ALL
            ),
            format!("duplicate declaration `{name}`")
        );
        assert_eq!(
            rejected(
                &format!("record {name} {{ value: Reading }} fn demo(){{}}"),
                Grants::ALL
            ),
            format!("duplicate declaration `{name}`")
        );
    }
}

#[test]
fn selected_denied_operations_still_occupy_callable_namespace() {
    for name in ["sensor_open", "sensor_read"] {
        assert_eq!(
            rejected(&format!("fn {name}(){{}} fn demo(){{}}"), Grants::NONE),
            format!("duplicate declaration `{name}`")
        );
    }
}

#[test]
fn selected_denied_call_resolves_to_grant_failure_without_fallback() {
    assert_eq!(
        rejected(
            "fn demo()->bool{let unused=sensor_open(); return true;}",
            Grants::NONE
        ),
        "trusted operation `sensor_open` is not granted"
    );
}

#[test]
fn acquisition_requires_its_exact_cleanup_only_release_grant() {
    let grants = Grants {
        release: false,
        ..Grants::ALL
    };
    assert_eq!(
        rejected(
            "fn demo()->bool{let unused=sensor_open(); return true;}",
            grants
        ),
        "trusted operation `sensor_open` is not granted"
    );
}

#[test]
fn cross_namespace_equalities_keep_value_field_and_callable_lookup_separate() {
    let source="type sensor_open=range 0..1; record Box { Reading: Reading, sensor_open: sensor_open } fn Reading()->Reading{return 1;} fn demo()->bool {let sensor_open=true; let Reading=sensor_open; let boxed=Box{Reading:Reading(),sensor_open:0}; return Reading;}";
    prepare_execution_with_profile(source, AdmissionLimits::default(), &selection(Grants::ALL))
        .unwrap();
}

#[test]
fn catalog_names_preserve_case_and_unselected_catalog_reserves_nothing() {
    let source="type Reading=range 0..1; type Sensor=range 0..1; fn sensor_open()->Reading{return 0;} fn demo()->Reading{return sensor_open();}";
    prepare_execution(source, AdmissionLimits::default()).unwrap();
    prepare_execution_with_profile(
        "type reading=range 0..1; fn Sensor_Open(){} fn demo(){}",
        AdmissionLimits::default(),
        &selection(Grants::ALL),
    )
    .unwrap();
    assert!(rejected(
        "fn demo()->bool{let x=Sensor_Open();return true;}",
        Grants::ALL
    )
    .contains("unknown function `Sensor_Open`"));
}

#[test]
fn release_has_no_source_callable_alias_or_record_representation() {
    let p = program("fn demo(){}", Grants::ALL);
    let c = p.trusted.as_ref().unwrap();
    assert_eq!(c.operations[2].source_name, None);
    assert_eq!(c.operations[2].class, OperationClass::Release);
    assert!(p.records.is_empty());
    assert!(p.resources.leaves.is_empty());
    assert!(rejected("fn demo(){sensor_release();}", Grants::ALL).contains("unknown function"));
}

#[test]
fn distinct_collections_have_foreign_resource_operation_and_origin_identities() {
    let a = program("fn demo(){}", Grants::ALL);
    let b = program("fn demo(){}", Grants::ALL);
    let (a, b) = (a.trusted.unwrap(), b.trusted.unwrap());
    assert_ne!(a.sensor, b.sensor);
    assert_ne!(a.operations[0].id, b.operations[0].id);
    assert_ne!(a.origin(0), b.origin(0));
}

#[test]
fn every_unused_and_denied_catalog_row_is_validated_independently() {
    let p = program("fn demo(){}", Grants::NONE);
    for index in 0..3 {
        let mut altered = p.clone();
        altered.trusted.as_mut().unwrap().operations[index].effects = EffectSet::EMPTY;
        assert!(altered.validate_record_expressions().is_err());
    }
    for index in 0..2 {
        let mut altered = p.clone();
        altered.ranges[index].max += 1;
        assert!(altered.validate_record_expressions().is_err());
    }
    let mut altered = p.clone();
    altered.ranges[0].origin = hir::DeclarationOrigin::Source(START);
    assert!(altered.validate_record_expressions().is_err());
}

#[test]
fn tiny_source_still_charges_catalog_storage_work_and_diagnostics() {
    // Each source-independent reservation is exercised independently. Predecessor
    // empty preparation fits all three controls without selecting the catalog.
    for limits in [
        AdmissionLimits {
            working_storage: 300_000,
            ..AdmissionLimits::default()
        },
        AdmissionLimits {
            frontend_work: 4096,
            ..AdmissionLimits::default()
        },
        AdmissionLimits {
            diagnostic_storage: 2048,
            ..AdmissionLimits::default()
        },
    ] {
        prepare_execution("", limits).unwrap();
        let e = prepare_execution_with_profile("", limits, &selection(Grants::ALL))
            .err()
            .unwrap();
        assert!(matches!(e.kind, ErrorKind::ConfiguredLimit(_)), "{e:?}");
    }
}

#[test]
fn profile_root_configuration_is_bounded_exact_and_immutable() {
    for roots in [
        &[][..],
        &[""][..],
        &["demo", "demo"][..],
        &["not-a-name"][..],
    ] {
        assert!(Selection::sensor_demo(Scenario::Sample, Grants::ALL, roots).is_err());
    }
    let s = selection(Grants::NONE);
    assert_eq!(s.roots(), ["demo"]);
    assert_eq!(s.scenario(), Scenario::Sample);
    assert_eq!(s.grants(), Grants::NONE);
}

#[test]
fn catalog_combined_envelope_exact_neighbors_are_charged_without_source() {
    let defaults = AdmissionLimits::default();
    let storage = (defaults.traversal_depth * 16 + 32) * (defaults.identifier_bytes + 512)
        + 65_536
        + 4096
        + 4 * 16;
    let work = 128 + 65_536 + 4096;
    let diagnostics = (defaults.identifier_bytes + 128) * 4 + 512 + 4096 + 4 * 16;
    let exact = AdmissionLimits {
        working_storage: storage,
        frontend_work: work,
        diagnostic_storage: diagnostics,
        ..defaults
    };
    let c = Context::new(exact);
    c.reserve_frontend(1, 0).unwrap();
    c.reserve_catalog(1, 4).unwrap();
    for limits in [
        AdmissionLimits {
            working_storage: storage - 1,
            ..exact
        },
        AdmissionLimits {
            frontend_work: work - 1,
            ..exact
        },
        AdmissionLimits {
            diagnostic_storage: diagnostics - 1,
            ..exact
        },
    ] {
        let c = Context::new(limits);
        c.reserve_frontend(1, 0).unwrap();
        assert!(c.reserve_catalog(1, 4).is_err());
    }
}

#[test]
fn sensor_is_canonical_resource_distinct_from_records_and_is_move_only() {
    let p = program(
        "fn relay(sensor:Sensor)->Sensor{return sensor;} fn demo(){}",
        Grants::ALL,
    );
    let c = p.trusted.as_ref().unwrap();
    let f = p.functions[0].as_ordinary().unwrap();
    assert_eq!(f.return_type, Some(hir::ValueType::Resource(c.sensor)));
    assert_eq!(
        p.parameter(f.parameters[0]).ty,
        hir::ParameterType::Value(hir::ValueType::Resource(c.sensor))
    );
    assert!(p.records.is_empty());
    assert!(p.resources.leaves.is_empty());
    assert!(rejected(
        "fn twice(sensor:Sensor)->Sensor{let moved=sensor;return sensor;} fn demo(){}",
        Grants::ALL
    )
    .contains("moved"));
}

#[test]
fn residual_parameter_cleanup_requires_io_but_protected_relay_does_not() {
    let absent = rejected(
        "fn retire(sensor:Sensor)->bool{return true;} fn demo(){}",
        Grants::ALL,
    );
    assert!(absent.contains("unadmitted effect `io`"), "{absent}");
    prepare_execution_with_profile("fn retire(sensor:Sensor)->bool effects io{return true;} fn relay(sensor:Sensor)->Sensor{return sensor;} fn demo(){}",AdmissionLimits::default(),&selection(Grants::ALL)).unwrap();
}

#[test]
fn sensor_shared_aliases_and_non_escaping_helpers_preserve_owner_transfer() {
    let source="fn inspect(sensor:&Sensor)->bool{return true;} fn relay(sensor:Sensor)->Sensor{let first=&sensor;let second=first;let a=inspect(first);let b=inspect(second);return sensor;} fn demo(){}";
    prepare_execution_with_profile(source, AdmissionLimits::default(), &selection(Grants::ALL))
        .unwrap();
}

#[test]
fn sensor_result_payloads_have_typed_resource_children_and_cleanup_effects() {
    let p=program("fn wrap(sensor:Sensor)->Result<success:Sensor,failure:bool>{return Ok(sensor);} fn demo(){}",Grants::ALL);
    let c = p.trusted.as_ref().unwrap();
    let plans = p.variant_plans(&[], &[]).unwrap();
    let hir::ValueType::Result(result) = p.functions[0].as_ordinary().unwrap().return_type.unwrap()
    else {
        unreachable!()
    };
    let plan = &plans[result.0];
    assert_eq!(
        plan.success,
        Some(hir::results::VariantChild::Resource(c.sensor))
    );
    assert_eq!(plan.failure, None);
    assert!(plan.bearing());
    assert!(plan.effects.contains(Effect::Io));
    prepare_execution_with_profile("fn wrap(sensor:Sensor)->Result<success:Sensor,failure:bool>{return Ok(sensor);} fn demo(){}",AdmissionLimits::default(),&selection(Grants::ALL)).unwrap();
}

#[test]
fn mutable_sensor_reference_interfaces_and_borrows_are_excluded() {
    for source in ["fn bad(sensor:&mut Sensor){} fn demo(){}","fn bad(sensor:Sensor) effects io {let mut owner=sensor;let borrow=&mut owner;} fn demo(){}"] {
        assert!(rejected(source,Grants::ALL).contains("mutable Sensor references are excluded"));
    }
}

#[test]
fn sensor_conditionals_replacement_and_public_record_fields_remain_excluded() {
    for source in [
        "fn bad(a:Sensor,b:Sensor)->Sensor{let chosen=if true{a}else{b};return chosen;} fn demo(){}",
        "fn bad(a:Sensor,b:Sensor)effects io{let mut owner=a;owner=b;} fn demo(){}",
        "record Bad{value:Sensor} fn demo(){}",
    ] {
        assert!(!rejected(source,Grants::ALL).is_empty());
    }
}

#[test]
fn sensor_representation_arithmetic_comparison_and_dereference_cannot_forge_a_value() {
    for expression in ["sensor.value", "sensor-1", "sensor==sensor", "*borrow"] {
        let source=format!("fn bad(sensor:Sensor)effects io{{let borrow=&sensor;let forged={expression};}} fn demo(){{}}");
        assert!(!rejected(&source, Grants::ALL).is_empty());
    }
}

#[test]
fn public_sensor_bearing_result_is_rejected_before_machine_entry() {
    let e = prepare_execution_with_profile(
        "fn demo()->Result<success:Sensor,failure:bool>{return Error(false);}",
        AdmissionLimits::default(),
        &selection(Grants::ALL),
    )
    .unwrap();
    let entry = e.entry("demo").unwrap();
    let error = crate::execution::execute(&e, entry, crate::execution::ExecutionLimits::default())
        .unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::Integrity("live resource Result cannot cross public boundary")
    );
}

#[test]
fn foreign_resource_type_and_mutable_referent_metadata_cannot_publish() {
    let p = program(
        "fn relay(sensor:Sensor)->Sensor{return sensor;} fn demo(){}",
        Grants::ALL,
    );
    let other = program("fn demo(){}", Grants::ALL);
    let foreign = other.trusted.unwrap().sensor;
    let mut altered = p.clone();
    altered.parameters[0].ty = hir::ParameterType::Value(hir::ValueType::Resource(foreign));
    assert!(altered.validate_record_expressions().is_err());
    let mut altered = p;
    let id = altered.trusted.as_ref().unwrap().sensor;
    altered.parameters[0].ty =
        hir::ParameterType::Value(hir::ValueType::MutableRef(hir::ReferentType::Resource(id)));
    assert!(altered.validate_record_expressions().is_err());
}

const SAMPLE: &str = "fn demo()->Result<success:Reading,failure:SensorError> effects io {let sensor=sensor_open() or return;let reading=sensor_read(&sensor) or return;return Ok(reading);}";

#[test]
fn defining_source_prepares_canonical_trusted_calls_without_source_stubs() {
    prepare_execution_with_profile(SAMPLE, AdmissionLimits::default(), &selection(Grants::ALL))
        .unwrap();
    let p = program(SAMPLE, Grants::ALL);
    assert_eq!(p.functions.len(), 1);
    assert!(p.parameters.is_empty());
    assert!(p.records.is_empty());
    let c = p.trusted.as_ref().unwrap();
    let f = p.functions[0].as_ordinary().unwrap();
    for (index, statement) in f.body[..2].iter().enumerate() {
        let hir::ValueStatement::Let {
            initializer,
            propagation: Some(_),
            ..
        } = statement
        else {
            panic!("expected propagation")
        };
        let hir::ExprKind::TrustedCall {
            operation,
            arguments,
        } = &initializer.kind
        else {
            panic!("expected canonical trusted target")
        };
        assert_eq!(*operation, c.operations[index].id);
        assert_eq!(arguments.len(), index);
        assert_eq!(
            initializer.ty,
            hir::ExprType::Result(c.operations[index].result.unwrap())
        );
    }
}

#[test]
fn trusted_argument_typing_uses_exact_arity_type_and_shared_passing_mode() {
    for call in [
        "sensor_open(true)",
        "sensor_read()",
        "sensor_read(true)",
        "sensor_read(sensor)",
    ] {
        let source =
            format!("fn bad(sensor:Sensor)effects io{{let ignored={call};}} fn demo(){{}}");
        let message = rejected(&source, Grants::ALL);
        assert!(
            message.contains("argument") || message.contains("type"),
            "{message}"
        );
    }
}

#[test]
fn trusted_result_and_error_payloads_keep_nominal_identity_and_literal_rules() {
    assert!(!rejected("type Same=range 0..4095;fn bad(sensor:&Sensor)->Result<success:Same,failure:SensorError>effects io{return sensor_read(sensor);}fn demo(){}",Grants::ALL).is_empty());
    assert!(rejected(
        "fn demo()->Result<success:Reading,failure:SensorError>{return Error(2);}",
        Grants::ALL
    )
    .contains("payload"));
    prepare_execution_with_profile("fn code()->SensorError{return 2;}fn demo()->Result<success:Reading,failure:SensorError>{return Error(code());}",AdmissionLimits::default(),&selection(Grants::ALL)).unwrap();
}

#[test]
fn trusted_io_contract_is_required_independently_of_operation_grants() {
    let source="fn bad(sensor:&Sensor)->Result<success:Reading,failure:SensorError>{return sensor_read(sensor);}fn demo(){}";
    assert!(rejected(source, Grants::ALL).contains("unadmitted effect `io`"));
    let source = source.replace("SensorError>{", "SensorError>effects io{");
    assert_eq!(
        rejected(
            &source,
            Grants {
                read: false,
                ..Grants::ALL
            }
        ),
        "trusted operation `sensor_read` is not granted"
    );
}

#[test]
fn trusted_shared_alias_calls_reuse_existing_loan_and_outcome_transactions() {
    let source="fn twice(sensor:&Sensor)->Result<success:Reading,failure:SensorError>effects io{let first=sensor;let second=first;let a=sensor_read(first)or return;let b=sensor_read(second)or return;return Ok(a-b);}fn demo(){}";
    prepare_execution_with_profile(source, AdmissionLimits::default(), &selection(Grants::ALL))
        .unwrap();
}

#[test]
fn receiving_call_holds_sensor_borrow_across_nested_trusted_arguments() {
    let source="fn received(sensor:&Sensor,outcome:Result<success:Reading,failure:SensorError>)->bool{match outcome{Ok(value)=>{return true;}Error(problem)=>{return false;}}}fn bad(sensor:Sensor)->bool effects io{return received(&sensor,sensor_read(&sensor));}fn demo(){}";
    prepare_execution_with_profile(source, AdmissionLimits::default(), &selection(Grants::ALL))
        .unwrap();
    let invalid="fn received(sensor:&Sensor,value:bool)->bool{return value;}fn take(sensor:Sensor)->bool effects io{return true;}fn bad(sensor:Sensor)->bool effects io{return received(&sensor,take(sensor));}fn demo(){}";
    let message = rejected(invalid, Grants::ALL);
    assert!(
        message.contains("borrow") || message.contains("loan"),
        "{message}"
    );
}

#[test]
fn trusted_result_cannot_be_discarded_or_left_as_an_unresolved_outcome() {
    assert!(rejected(
        "fn bad(sensor:&Sensor)effects io{sensor_read(sensor);}fn demo(){}",
        Grants::ALL
    )
    .contains("cannot be discarded"));
    assert!(rejected(
        "fn bad(sensor:&Sensor)effects io{let ignored=sensor_read(sensor);}fn demo(){}",
        Grants::ALL
    )
    .contains("unresolved Result outcome"));
}

#[test]
fn trusted_calls_remain_excluded_from_verified_contracts_and_harness_authority() {
    let source="type P=range 0..100;record Battery{charge:P}verified fn take(battery:&mut Battery,amount:P)requires sensor_open() ensures true {battery.charge-=amount;}fn demo(){}";
    assert!(rejected(source, Grants::ALL).contains("trusted calls require an ordinary function"));
}

#[test]
fn trusted_argument_destinations_are_canonical_and_never_parameter_ids() {
    let p = program(SAMPLE, Grants::ALL);
    let c = p.trusted.as_ref().unwrap();
    let (m, _) = crate::mir::lower_with_origins(&p).unwrap();
    let f = m.functions().first().unwrap();
    let flow = f.temporaries.as_ref().unwrap();
    assert!(flow.sites.iter().any(|s| s.destination
        == crate::mir::temporaries::Destination::TrustedArgument(c.operations[1].id, 0)));
    assert!(p.parameters.is_empty());
    assert!(flow.sites.iter().all(|s| !matches!(
        s.destination,
        crate::mir::temporaries::Destination::Parameter(_)
    )));
}

#[test]
fn trusted_call_approval_rejects_foreign_cleanup_only_type_and_arity_mutations() {
    let original = program(SAMPLE, Grants::ALL);
    let foreign = program(SAMPLE, Grants::ALL).trusted.unwrap().operations[0].id;
    for mutation in 0..4 {
        let mut p = original.clone();
        let cleanup = p.trusted.as_ref().unwrap().operations[2].id;
        let f = match &mut p.functions[0] {
            hir::Function::Ordinary(f) => f,
            _ => unreachable!(),
        };
        let hir::ValueStatement::Let { initializer, .. } = &mut f.body[0] else {
            unreachable!()
        };
        let hir::ExprKind::TrustedCall {
            operation,
            arguments,
        } = &mut initializer.kind
        else {
            unreachable!()
        };
        match mutation {
            0 => *operation = foreign,
            1 => *operation = cleanup,
            2 => initializer.ty = hir::ExprType::Bool,
            _ => arguments.push(hir::TypedExpr {
                kind: hir::ExprKind::BoolLiteral(true),
                ty: hir::ExprType::Bool,
                span: initializer.span,
            }),
        }
        assert!(
            p.validate_record_expressions().is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn trusted_propagation_cleanup_witnesses_come_from_residual_sensor_custody() {
    let p = program(SAMPLE, Grants::ALL);
    let c = p.trusted.as_ref().unwrap();
    let sites = crate::ownership::analyze(&p).unwrap();
    assert!(sites
        .iter()
        .any(|s| s.plan == hir::resources::CleanupType::Resource(c.sensor)));
    assert!(sites.iter().all(|s| s.effects == c.resource.effects));
    assert!(
        sites.len() >= 2,
        "read propagation failure and ordinary return both retire Sensor"
    );
}

#[test]
fn profile_convenience_execution_stops_before_entry_without_receipt_session() {
    let e =
        prepare_execution_with_profile(SAMPLE, AdmissionLimits::default(), &selection(Grants::ALL))
            .unwrap();
    let error = crate::execution::execute(
        &e,
        e.entry("demo").unwrap(),
        crate::execution::ExecutionLimits::default(),
    )
    .unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::Integrity("selected profile execution requires persistent session custody")
    );
}
