use super::*;

fn composition(source: &str, scenario: Scenario, expected: &str, released: usize) {
    let selection = Selection::sensor_demo(scenario, Grants::ALL, &["demo"]).unwrap();
    let e = prepare_execution_with_profile(source, AdmissionLimits::default(), &selection).unwrap();
    let mut s = session(&e);
    assert_eq!(
        e.render_result(s.advance().unwrap(), 4096).unwrap(),
        expected
    );
    assert_eq!(s.live_sensors(), 0);
    assert_eq!(
        s.events()
            .iter()
            .filter(|v| matches!(v, Event::ReleaseComplete(_)))
            .count(),
        released
    );
    let prefix = s.events().to_vec();
    s.advance().unwrap();
    assert_eq!(s.events(), prefix);
}

#[test]
fn explicit_matches_normalize_err_and_keep_open_and_read_outcomes() {
    let source="fn acquire()->Result<success:Sensor,failure:SensorError>effects io{match sensor_open(){Err(problem)=>{return Err(problem);}Ok(sensor)=>{return Ok(sensor);}}}fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=acquire()or return;match sensor_read(&sensor){Ok(value)=>{return Ok(value);}Error(problem)=>{return Error(problem);}}}";
    for (scenario, output, releases) in [
        (Scenario::Sample, "Ok(Reading(42))", 1),
        (Scenario::NoSensor, "Error(SensorError(1))", 0),
        (Scenario::NoSample, "Error(SensorError(2))", 1),
    ] {
        composition(source, scenario, output, releases);
    }
}

#[test]
fn read_error_keeps_shared_alias_and_owner_valid_for_explicit_later_read() {
    let source="fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=sensor_open()or return;let alias=&sensor;let second=alias;match sensor_read(alias){Ok(value)=>{return Ok(value);}Error(problem)=>{let value=sensor_read(second)or return;return Ok(value);}}}";
    composition(source, Scenario::NoSample, "Error(SensorError(2))", 1);
    let selection = Selection::sensor_demo(Scenario::NoSample, Grants::ALL, &["demo"]).unwrap();
    let e = prepare_execution_with_profile(source, AdmissionLimits::default(), &selection).unwrap();
    let mut s = session(&e);
    s.advance().unwrap();
    assert_eq!(s.events().iter().filter(|v|matches!(v,Event::Attempt(_,op) if *op==e.trusted.as_ref().unwrap().catalog.operations[1].id)).count(),2);
}

#[test]
fn owned_parameter_retirement_and_effect_free_protected_relay_compose() {
    composition("fn relay(sensor:Sensor)->Sensor{return sensor;}fn retire(sensor:Sensor)->bool effects io{return true;}fn demo()->Result<success:bool,failure:SensorError>effects io{let sensor=sensor_open()or return;let moved=relay(sensor);return Ok(retire(moved));}",Scenario::Sample,"Ok(Bool(true))",1);
}

#[test]
fn shared_non_escaping_helper_observes_without_replacing_owner() {
    composition("fn observe(sensor:&Sensor)->Result<success:Reading,failure:SensorError>effects io{return sensor_read(sensor);}fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=sensor_open()or return;let value=observe(&sensor)or return;return Ok(value);}",Scenario::Sample,"Ok(Reading(42))",1);
}

#[test]
fn nested_result_obligations_move_sensor_through_internal_protected_returns() {
    composition("fn wrap(sensor:Sensor)->Result<success:Result<success:Sensor,failure:SensorError>,failure:SensorError>{return Ok(Ok(sensor));}fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=sensor_open()or return;let inner=wrap(sensor)or return;let moved=inner or return;return sensor_read(&moved);}",Scenario::Sample,"Ok(Reading(42))",1);
}

#[test]
fn branch_local_custody_cleans_before_protected_ok_and_error_delivery() {
    let source="fn demo()->Result<success:Reading,failure:SensorError>effects io{let sensor=sensor_open()or return;if true{let reading=sensor_read(&sensor)or return;return Ok(reading);}else{return sensor_read(&sensor);}}";
    composition(source, Scenario::Sample, "Ok(Reading(42))", 1);
    composition(source, Scenario::NoSample, "Error(SensorError(2))", 1);
}

#[test]
fn iteration_local_custody_retires_on_continue_break_and_loop_exit() {
    composition("fn count()->Reading{return 3;}fn zero()->Reading{return 0;}fn demo()->Result<success:Reading,failure:SensorError>effects io{let mutable left=count();while 1<=left{let sensor=sensor_open()or return;left=left-1;if left==2{continue;}if left==0{break;}let reading=sensor_read(&sensor)or return;}return Ok(zero());}",Scenario::Sample,"Ok(Reading(0))",3);
}

#[test]
fn early_return_from_loop_retires_iteration_local_before_error_delivery() {
    composition("fn demo()->Result<success:Reading,failure:SensorError>effects io{while true{let sensor=sensor_open()or return;let reading=sensor_read(&sensor)or return;return Ok(reading);}return demo();}",Scenario::NoSample,"Error(SensorError(2))",1);
}

#[test]
fn protected_sensor_temporary_moves_to_owned_argument_and_retires_once() {
    composition("fn relay(sensor:Sensor)->Sensor{return sensor;}fn retire(sensor:Sensor)->bool effects io{return true;}fn demo()->Result<success:bool,failure:SensorError>effects io{let sensor=sensor_open()or return;return Ok(retire(relay(sensor)));}",Scenario::Sample,"Ok(Bool(true))",1);
}

#[test]
fn written_argument_order_and_multiple_owned_parameters_preserve_release_order() {
    composition("fn relay(sensor:Sensor)->Sensor{return sensor;}fn consume(first:Sensor,second:Sensor)->bool effects io{return true;}fn demo()->Result<success:bool,failure:SensorError>effects io{let first=sensor_open()or return;let second=sensor_open()or return;return Ok(consume(relay(second),relay(first)));}",Scenario::Sample,"Ok(Bool(true))",2);
}

#[test]
fn public_resource_and_resource_bearing_result_roots_cannot_enter_session() {
    for source in ["fn demo()->Result<success:Sensor,failure:SensorError>effects io{return sensor_open();}","fn demo()->Sensor effects io{match sensor_open(){Ok(sensor)=>{return sensor;}Error(problem)=>{return demo();}}}"] {
        let selection=Selection::sensor_demo(Scenario::Sample,Grants::ALL,&["demo"]).unwrap();
        let e=prepare_execution_with_profile(source,AdmissionLimits::default(),&selection).unwrap();
        let id=e.program.functions.iter().find(|f|f.name()=="demo").unwrap().id();
        assert!(matches!(ExecutionSession::new(&e,id,ExecutionLimits::default()).err().unwrap().kind,ErrorKind::Integrity(_)));
    }
}
