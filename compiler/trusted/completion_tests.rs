use super::*;
use crate::execution::{
    prepare_execution, prepare_execution_with_profile, AdmissionLimits, ErrorKind, ExecutionLimits,
    ExecutionSession,
};

fn selection(grants: Grants) -> Selection {
    Selection::sensor_demo(Scenario::Sample, grants, &["demo"]).unwrap()
}

fn prepare(source: &str, grants: Grants) -> Result<crate::execution::CheckedExecutable, crate::execution::ExecutionError> {
    prepare_execution_with_profile(source, AdmissionLimits::default(), &selection(grants))
}

#[test]
fn finish_contract_is_exact_owned_nominal_and_operation_specific() {
    let context = Context::new(AdmissionLimits::default());
    let ast = crate::parse_source("fn demo(){}").unwrap();
    let resolved = crate::resolve::resolve_profile(&ast, &context, &selection(Grants::ALL)).unwrap();
    let program = crate::types::check(resolved).unwrap();
    let catalog = program.trusted.as_ref().unwrap();
    catalog.validate(&program).unwrap();
    let finish = &catalog.operations[3];
    assert_eq!(finish.parameter, Some(Passing::Owned(catalog.sensor)));
    assert_eq!(finish.class, OperationClass::Completion);
    assert_eq!(finish.binding, Binding::SensorFinishV1);
    assert_eq!(finish.errors, 0b1000);
    assert_eq!(finish.failure, Some(catalog.error));
    let receipt = &program.ranges[catalog.finish_receipt.0];
    assert_eq!((receipt.name.as_str(), receipt.min, receipt.max), ("FinishReceipt", 1, 1));
    let result = program.result_type(finish.result.unwrap()).unwrap();
    assert_eq!(result.success, hir::ValueType::Range(catalog.finish_receipt));
    assert_eq!(result.failure, hir::ValueType::Range(catalog.error));
    let release = &catalog.operations[2];
    assert_eq!(release.source_name, None);
    assert_ne!(release.id, finish.id);
    assert_eq!(release.effects, finish.effects);
}

#[test]
fn finish_direct_and_relay_arguments_prepare_without_reference_coercion() {
    for argument in ["sensor", "relay(sensor)"] {
        let source = format!("fn relay(sensor:Sensor)->Sensor{{return sensor;}} fn demo()->Result<success:FinishReceipt,failure:SensorError> effects io{{let sensor=sensor_open()or return;return sensor_finish({argument});}}");
        prepare(&source, Grants::ALL).unwrap();
    }
}

#[test]
fn finish_match_and_local_propagation_reuse_existing_outcomes() {
    for body in [
        "let finished=sensor_finish(sensor)or return;return Ok(finished);",
        "match sensor_finish(sensor){Ok(finished)=>{return Ok(finished);}Err(problem)=>{return Error(problem);}}",
    ] {
        prepare(&format!("fn demo()->Result<success:FinishReceipt,failure:SensorError>effects io{{let sensor=sensor_open()or return;{body}}}"), Grants::ALL).unwrap();
    }
}

#[test]
fn finish_and_receipt_names_obey_existing_namespace_selection() {
    for source in [
        "fn sensor_finish()->bool{return true;}fn demo(){}",
        "type FinishReceipt=range 1..1;fn demo(){}",
        "record FinishReceipt{x:Reading}fn demo(){}",
    ] {
        assert!(prepare(source, Grants::ALL).is_err());
    }
    prepare_execution("fn sensor_finish()->bool{return true;}fn demo()->bool{return sensor_finish();}", AdmissionLimits::default()).unwrap();
    prepare("record Sample{FinishReceipt:FinishReceipt,sensor_finish:Reading}fn demo()->Sample{let sensor_finish=true;return Sample{FinishReceipt:1,sensor_finish:42};}", Grants::ALL).unwrap();
}

#[test]
fn denied_finish_keeps_occupancy_without_source_fallback() {
    let grants = Grants { finish: false, ..Grants::ALL };
    let failure = prepare("fn demo(sensor:Sensor)->Result<success:FinishReceipt,failure:SensorError>effects io{return sensor_finish(sensor);}", grants).err().unwrap();
    assert!(matches!(failure.kind, ErrorKind::Frontend { .. }));
    assert!(prepare("fn sensor_finish(sensor:Sensor)->bool{return true;}fn demo(){}", grants).is_err());
}

#[test]
fn finish_requires_release_even_on_unrooted_owned_helper() {
    assert!(prepare("fn finish_owned(sensor:Sensor)->Result<success:FinishReceipt,failure:SensorError>effects io{return sensor_finish(sensor);}fn demo(){}", Grants { release: false, ..Grants::ALL }).is_err());
}

#[test]
fn receipt_is_source_formable_copy_field_status_without_finish() {
    prepare("record Sample{finished:FinishReceipt,again:FinishReceipt}fn status()->FinishReceipt{return 1;}fn demo()->Sample{let first=status();let second=first;return Sample{finished:first,again:second};}", Grants::NONE).unwrap();
}

#[test]
fn receipt_literal_neighbors_and_foreign_nominal_values_reject() {
    for source in [
        "fn demo()->FinishReceipt{return 0;}",
        "fn demo()->FinishReceipt{return 2;}",
        "type Same=range 1..1;fn same()->Same{return 1;}fn demo()->FinishReceipt{return same();}",
        "fn demo()->Result<success:FinishReceipt,failure:SensorError>{return Ok(1);}",
    ] {
        assert!(prepare(source, Grants::ALL).is_err());
    }
}

#[test]
fn finish_rejects_reference_wrong_arity_and_unselected_payload() {
    for expression in ["sensor_finish(&sensor)", "sensor_finish()", "sensor_finish(sensor,sensor)", "sensor_finish(sensor_open())"] {
        assert!(prepare(&format!("fn demo()->Result<success:FinishReceipt,failure:SensorError>effects io{{let sensor=sensor_open()or return;return {expression};}}"), Grants::ALL).is_err());
    }
}

#[test]
fn finish_complete_direct_effect_is_required() {
    assert!(prepare("fn finish_owned(sensor:Sensor)->Result<success:FinishReceipt,failure:SensorError>{return sensor_finish(sensor);}fn demo(){}", Grants::ALL).is_err());
}

#[test]
fn unused_finish_descriptor_and_error_subset_are_independently_validated() {
    let context = Context::new(AdmissionLimits::default());
    let ast = crate::parse_source("fn demo(){}").unwrap();
    let resolved = crate::resolve::resolve_profile(&ast, &context, &selection(Grants::NONE)).unwrap();
    let program = crate::types::check(resolved).unwrap();
    for mutation in 0..4 {
        let mut changed = program.clone();
        let catalog = changed.trusted.as_mut().unwrap();
        match mutation {
            0 => catalog.operations[3].errors = 0b1110,
            1 => catalog.operations[3].parameter = Some(Passing::Shared(catalog.sensor)),
            2 => catalog.operations[3].binding = Binding::SensorReadV1,
            _ => changed.ranges[2].max = 2,
        }
        assert!(changed.validate_record_expressions().is_err());
    }
}

#[test]
fn m1_completion_dispatch_is_disabled_and_retains_the_sensor() {
    let executable = prepare("fn demo()->Result<success:FinishReceipt,failure:SensorError>effects io{let sensor=sensor_open()or return;return sensor_finish(sensor);}", Grants::ALL).unwrap();
    let mut session = ExecutionSession::new(&executable, executable.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
    assert!(matches!(session.advance().unwrap_err().kind, ErrorKind::Integrity(_)));
    assert_eq!(session.live_sensors(), 1);
    let prefix = session.events().to_vec();
    assert_eq!(prefix.iter().filter(|e| matches!(e, crate::execution::TrustedEvent::Attempt(..))).count(), 1);
    assert!(session.advance().is_err());
    assert_eq!(session.events(), prefix);
}
