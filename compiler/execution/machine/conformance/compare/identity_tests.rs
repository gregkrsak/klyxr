//! Actual interpreter controls and separately labeled observation-protocol
//! falsification. None selects or executes a native engine.
use super::*;

fn observation() -> Limits {
    Limits {
        records: 262_144,
        work: 10_000_000,
        storage: 67_108_864,
        lineage: 128,
    }
}
fn comparison() -> ComparisonLimits {
    ComparisonLimits {
        work: 100_000_000,
        ..ComparisonLimits::default()
    }
}
fn ordinary<'a>(c: &'a contract::Contract<'a>, first_frame: u64) -> Session<'a> {
    let mut s = Session::new(c, ExecutionLimits::default(), observation()).unwrap();
    // Raw frame allocation is private, not a new source/runtime input.
    s.machine_mut().next_frame = first_frame;
    s.advance().unwrap();
    s
}

#[test]
fn lawful_raw_frame_changes_preserve_shared_alias_equality_and_ancestry() {
    let source="fn read(view:&bool)->bool{return *view;}fn demo()->bool{let value=true;let first=&value;let second=first;return read(second);}";
    let e = prepare_execution(source, AdmissionLimits::default()).unwrap();
    let c = contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default())
        .unwrap();
    let a = ordinary(&c, 0);
    let b = ordinary(&c, 900);
    assert_eq!(compare(&a, &b, comparison()), Comparison::Equal);
}

#[test]
fn conserving_wrong_parameter_association_is_different_despite_the_same_value() {
    use crate::trusted::{Grants, Scenario, Selection};
    let source="fn relay(sensor:Sensor)->Sensor{return sensor;}fn consume(first:Sensor,second:Sensor)->bool effects io{return true;}fn demo()->Result<success:bool,failure:SensorError>effects io{let first=sensor_open()or return;let second=sensor_open()or return;return Ok(consume(relay(second),relay(first)));}";
    let selection = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap();
    let e = prepare_execution_with_profile(source, AdmissionLimits::default(), &selection).unwrap();
    let c = contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default())
        .unwrap();
    let mut a = Session::new(&c, ExecutionLimits::default(), observation()).unwrap();
    a.advance().unwrap();
    let function = e.program.functions()[1].id();
    let params = &e
        .program
        .function(function)
        .as_ordinary()
        .unwrap()
        .parameters;
    SWAP_PARAMETERS.with(|slot| {
        slot.set(Some((
            function,
            Place::Parameter(params[0]),
            Place::Parameter(params[1]),
        )))
    });
    let mut b = Session::new(&c, ExecutionLimits::default(), observation()).unwrap();
    let result = b.advance();
    SWAP_PARAMETERS.with(|slot| slot.set(None));
    result.unwrap();
    assert_eq!(a.primary(), b.primary());
    let compared = compare(&a, &b, comparison());
    assert!(matches!(compared, Comparison::Different { .. }), "{compared:?}");
}

#[test]
fn foreign_realm_stale_generation_and_bad_payload_reject_before_projection() {
    use crate::trusted::{Grants, Scenario, Selection};
    let selection = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["sample"]).unwrap();
    let e = prepare_execution_with_profile(
        include_str!("../../../../../examples/sensor_finish.klx"),
        AdmissionLimits::default(),
        &selection,
    )
    .unwrap();
    let c =
        contract::Contract::extract(&e, e.entry("sample").unwrap(), contract::Limits::default())
            .unwrap();
    let mut a = Session::new(&c, ExecutionLimits::default(), observation()).unwrap();
    a.advance().unwrap();
    for mutation in 0..4 {
        let mut b = Session::new(&c, ExecutionLimits::default(), observation()).unwrap();
        b.advance().unwrap();
        let mut observer = b.machine().observation.borrow_mut();
        let records = &mut observer.as_mut().unwrap().records;
        match mutation {
            0 => {
                let d = records
                    .iter_mut()
                    .find_map(|r| {
                        if let Record::Meta(Part::Token, d) = r {
                            Some(d)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                d[0] += 1;
            }
            1 => {
                let d = records
                    .iter_mut()
                    .find_map(|r| {
                        if let Record::Meta(Part::GenerationSlot, d) = r {
                            Some(d)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                d[2] = 9_000_000;
            }
            2 => {
                let d = records
                    .iter_mut()
                    .find_map(|r| {
                        if let Record::Meta(Part::ResultCell, d) = r {
                            Some(d)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                d[5] = 255;
            }
            _ => {
                let d = records
                    .iter_mut()
                    .find_map(|r| {
                        if let Record::Meta(Part::TemporaryOwner, d) = r {
                            Some(d)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                d[3] = 3;
            }
        }
        drop(observer);
        assert!(
            matches!(compare(&a, &b, comparison()), Comparison::Inconclusive(_)),
            "mutation {mutation}"
        );
    }
}
