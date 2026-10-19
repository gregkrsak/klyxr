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
    assert_eq!(p.ranges.len(), 2);
    for (index, (name, min, max)) in [("Reading", 0, 4095), ("SensorError", 1, 3)]
        .into_iter()
        .enumerate()
    {
        let r = &p.ranges[index];
        assert_eq!(
            (r.id, r.name.as_str(), r.min, r.max),
            (RangeTypeId(index), name, min, max)
        );
        assert_eq!(r.origin, hir::DeclarationOrigin::Catalog(c.origin(index)));
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
