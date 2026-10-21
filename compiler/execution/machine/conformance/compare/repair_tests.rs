//! Discriminating I01/I02 source witnesses and private malformed-state controls.
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
fn session<'a>(contract: &'a contract::Contract<'a>, steps: u64) -> Session<'a> {
    let mut session = Session::new(
        contract,
        ExecutionLimits {
            steps,
            ..ExecutionLimits::default()
        },
        observation(),
    )
    .unwrap();
    let _ = session.advance();
    assert_eq!(session.completeness(), Ok(()));
    session
}
fn contract(e: &CheckedExecutable) -> contract::Contract<'_> {
    contract::Contract::extract(e, e.entry("demo").unwrap(), contract::Limits::default()).unwrap()
}
fn equal_bool(source: &str, expected: bool) {
    let e = prepare_execution(source, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    let b = session(&c, 1024);
    assert_eq!(
        a.primary().unwrap(),
        Ok(&ExecutionResult::Value(RuntimeValue::Bool(expected)))
    );
    assert_eq!(b.primary(), a.primary());
    assert_eq!(compare(&a, &b, comparison()), Comparison::Equal);
}
const TYPES: &str = "type Percent=range 0..100;record Battery{charge:Percent}";
const VERIFIED: &str = "verified fn consume(battery:&mut Battery,amount:Percent) requires amount<=battery.charge ensures battery.charge==old(battery.charge)-amount {battery.charge-=amount;}";

#[test]
fn i01_verified_prefix_resolves_ordinary_boundary_and_nonboundary_origins() {
    let source = format!("{TYPES}{VERIFIED}fn demo()->bool{{return true;}}");
    let e = prepare_execution(&source, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    assert_eq!(c.entry().0, 1);
    assert!(e.entry("consume").is_err());
    for steps in [1, 2, 3] {
        let a = session(&c, steps);
        let b = session(&c, steps);
        if steps == 3 {
            assert_eq!(
                a.primary().unwrap(),
                Ok(&ExecutionResult::Value(RuntimeValue::Bool(true)))
            );
        } else {
            assert_eq!(
                a.primary().unwrap().unwrap_err().kind,
                ErrorKind::ConfiguredLimit("steps")
            );
        }
        assert_eq!(compare(&a, &b, comparison()), Comparison::Equal);
        assert!(a
            .capture()
            .records
            .iter()
            .any(|r| matches!(r, Record::OriginKind(OriginKind::FunctionBoundary))));
        if steps == 3 {
            assert!(a.capture().records.iter().any(
                |r| matches!(r,Record::OriginKind(kind) if *kind!=OriginKind::FunctionBoundary)
            ));
        }
        assert!(a
            .capture()
            .records
            .iter()
            .all(|r| !matches!(r,Record::Frame(_,function,..) if function.0==0)));
    }
}
#[test]
fn i01_verified_between_caller_callee_and_failing_harness_stay_isolated() {
    let source = format!(
        "{TYPES}fn demo()->bool{{return helper();}}{VERIFIED}fn helper()->bool{{return true;}}"
    );
    equal_bool(&source, true);
    let e = prepare_execution(&source, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    assert_eq!(c.functions(), &[FunctionId(0), FunctionId(2)]);
    for steps in [1, 3, 4] {
        let a = session(&c, steps);
        let b = session(&c, steps);
        assert_eq!(
            a.primary().unwrap().unwrap_err().kind,
            ErrorKind::ConfiguredLimit("steps")
        );
        assert_eq!(compare(&a, &b, comparison()), Comparison::Equal);
    }
    // This existing mixed-file fixture has an intentionally failing proof/call.
    // Public ordinary preparation/execution must not evaluate the harness.
    let source = format!(
        "{}\nfn demo()->bool{{return true;}}",
        include_str!("../../../../../examples/battery_fail.klx")
    );
    assert!(
        !crate::verify::verify_report(&crate::compile_source(&source).unwrap())
            .diagnostics
            .is_empty()
    );
    equal_bool(&source, true);
}
#[test]
fn i01_foreign_function_and_origin_metadata_reject_without_indexing_panics() {
    let source = format!("{TYPES}{VERIFIED}fn demo()->bool{{return true;}}");
    let e = prepare_execution(&source, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    for mutation in 0..3 {
        let b = session(&c, 1024);
        let mut capture = b.machine().observation.borrow_mut();
        let records = &mut capture.as_mut().unwrap().records;
        if mutation < 2 {
            let function = records
                .iter_mut()
                .find_map(|r| {
                    if let Record::Frame(_, function, ..) = r {
                        Some(function)
                    } else {
                        None
                    }
                })
                .unwrap();
            *function = FunctionId(if mutation == 0 { 0 } else { usize::MAX });
        } else {
            let span = e.program.function(FunctionId(0)).span();
            let origin = records
                .iter_mut()
                .find_map(|r| {
                    if let Record::Cut(_, _, _, origin, _, _) = r {
                        if *origin != START {
                            Some(origin)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .unwrap();
            *origin = span;
        }
        drop(capture);
        assert_eq!(
            compare(&a, &b, comparison()),
            Comparison::Inconclusive(Incomplete::InvalidState)
        );
    }
}
const SHARED_THEN_EXCLUSIVE: &str = "fn demo()->bool{let mutable value=true;let shared=&value;let seen=*shared;let exclusive=&mut value;*exclusive=false;return value;}";
const SHARED_THEN_MOVE: &str = "type N=range 0..1;record R{x:N}fn demo()->R{let item=R{x:1};let view=&item;let observed=view.x;return item;}";

#[test]
fn i02_final_shared_use_allows_exclusive_access_with_old_bits_retained() {
    equal_bool(SHARED_THEN_EXCLUSIVE, false);
    let e = prepare_execution(SHARED_THEN_EXCLUSIVE, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    let records = &a.capture().records;
    let cuts: Vec<_> = records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| matches!(r, Record::Cut(..)).then_some(i))
        .collect();
    assert!(cuts.windows(2).any(|cut| {
        let rows = &records[cut[0]..cut[1]];
        rows.iter()
            .any(|r| matches!(r, Record::Reference(BorrowKind::Shared, ..)))
            && rows
                .iter()
                .any(|r| matches!(r, Record::Reference(BorrowKind::Mutable, ..)))
    }));
}
#[test]
fn i02_record_move_after_final_field_read_preserves_stored_reference_identity() {
    let e = prepare_execution(SHARED_THEN_MOVE, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    let b = session(&c, 1024);
    let ExecutionResult::Value(RuntimeValue::Record { record, fields }) =
        a.primary().unwrap().unwrap()
    else {
        panic!("record result")
    };
    assert_eq!(*record, e.program.records()[0].id);
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].1.value, 1);
    assert_eq!(a.primary(), b.primary());
    assert_eq!(compare(&a, &b, comparison()), Comparison::Equal);
    let item = e
        .program
        .locals()
        .iter()
        .find(|local| local.name == "item")
        .unwrap()
        .id;
    let records = &a.capture().records;
    let cuts: Vec<_> = records
        .iter()
        .enumerate()
        .filter_map(|(i, r)| matches!(r, Record::Cut(..)).then_some(i))
        .collect();
    assert!(cuts.windows(2).any(|cut| {
        let rows = &records[cut[0]..cut[1]];
        rows.contains(&Record::Slot(Place::Local(item), 2))
            && rows
                .iter()
                .any(|r| matches!(r, Record::Reference(BorrowKind::Shared, ..)))
    }));
}
#[test]
fn i02_repeated_scopes_expire_shared_handles_before_exclusive_access_and_move() {
    equal_bool("type N=range 0..2;fn seed()->N{return 2;}fn demo()->bool{let mutable item=true;let mutable count=seed();while !(count==0){let first=&mut item;let second=first;*second=false;count=count-1;}return item;}", false);
    equal_bool("type N=range 0..3;fn seed()->N{return 3;}fn demo()->bool{let mutable value=true;let mutable count=seed();while !(count==0){let shared=&value;let seen=*shared;let exclusive=&mut value;*exclusive=false;count=count-1;}return value;}",false);
    equal_bool("type N=range 0..2;record R{x:N}fn seed()->N{return 2;}fn consume(item:R){}fn demo()->bool{let mutable count=seed();while !(count==0){let item=R{x:1};let view=&item;let seen=view.x;consume(item);count=count-1;}return true;}",true);
}
#[test]
fn i02_repeated_activations_and_exclusive_transfers_keep_distinct_provenance() {
    equal_bool("fn round(value:bool)->bool{let mutable item=value;let shared=&item;let seen=*shared;let first=&mut item;let second=first;*second=false;return item;}fn demo()->bool{let first=round(true);return round(first);}",false);
    equal_bool("fn demo()->bool{let mutable item=true;let first=&mut item;*first=false;let second=&mut item;*second=true;return item;}",true);
}
#[test]
fn i02_foreign_wrong_referent_and_permission_bits_remain_invalid() {
    let source="fn demo()->bool{let value=true;let other=false;let first=&value;let alias=first;return *alias;}";
    let e = prepare_execution(source, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    let other = e
        .program
        .locals()
        .iter()
        .find(|local| local.name == "other")
        .unwrap()
        .id;
    for mutation in 0..3 {
        let b = session(&c, 1024);
        let mut observer = b.machine().observation.borrow_mut();
        // Corrupt every occurrence, so cross-snapshot agreement cannot certify it.
        for record in &mut observer.as_mut().unwrap().records {
            if let Record::Reference(kind, _, frame, place) = record {
                match mutation {
                    0 => *frame = 9_000_000,
                    1 => *place = Place::Local(other),
                    _ => *kind = BorrowKind::Mutable,
                }
            }
        }
        drop(observer);
        assert_eq!(
            compare(&a, &b, comparison()),
            Comparison::Inconclusive(Incomplete::InvalidState)
        );
    }
}
#[test]
fn i02_duplicate_mutable_transfer_provenance_rejects_even_when_both_views_agree() {
    let source="fn demo()->bool{let mutable item=true;let first=&mut item;let second=first;*second=false;return item;}";
    let e = prepare_execution(source, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    let b = session(&c, 1024);
    let first = e
        .program
        .locals()
        .iter()
        .find(|local| local.name == "first")
        .unwrap()
        .id;
    let second = e
        .program
        .locals()
        .iter()
        .find(|local| local.name == "second")
        .unwrap()
        .id;
    for session in [&a, &b] {
        let mut observer = session.machine().observation.borrow_mut();
        let records = &mut observer.as_mut().unwrap().records;
        let cuts: Vec<_> = records
            .iter()
            .enumerate()
            .filter_map(|(i, r)| matches!(r, Record::Cut(..)).then_some(i))
            .collect();
        let (at, value) = cuts
            .windows(2)
            .find_map(|cut| {
                let rows = &records[cut[0]..cut[1]];
                let a = rows
                    .iter()
                    .position(|r| *r == Record::Slot(Place::Local(first), 2))?;
                let b = rows
                    .iter()
                    .position(|r| *r == Record::Slot(Place::Local(second), 1))?;
                Some((cut[0] + a, rows[b + 1]))
            })
            .unwrap();
        assert!(matches!(value, Record::Reference(BorrowKind::Mutable, ..)));
        records[at] = Record::Slot(Place::Local(first), 1);
        records.insert(at + 1, value);
    }
    assert_eq!(
        compare(&a, &b, comparison()),
        Comparison::Inconclusive(Incomplete::InvalidState)
    );
}
#[test]
fn i02_expired_reference_use_is_not_granted_authority_by_stored_bits() {
    for source in [
        "fn demo()->bool{let mutable item=true;let shared=&item;let seen=*shared;let exclusive=&mut item;*exclusive=false;return *shared;}",
        "type N=range 0..1;record R{x:N}fn consume(item:R){}fn demo()->N{let item=R{x:1};let view=&item;let seen=view.x;consume(item);return view.x;}",
    ] {
        assert!(matches!(crate::compile_source(source), Err(crate::FrontendError::Ownership(_))));
        assert!(prepare_execution(source, AdmissionLimits::default()).is_err());
    }
    let e = prepare_execution(SHARED_THEN_MOVE, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    let b = session(&c, 1024);
    let item = e
        .program
        .locals()
        .iter()
        .find(|local| local.name == "item")
        .unwrap()
        .id;
    let (expression, ordinal) = c
        .expressions()
        .iter()
        .find(|(expression, _)| matches!(expression.kind, ExprKind::CopyReferenceFieldRead { .. }))
        .unwrap();
    let _ = expression;
    for session in [&a, &b] {
        let mut observer = session.machine().observation.borrow_mut();
        let records = &mut observer.as_mut().unwrap().records;
        let at = records
            .iter()
            .position(|r| *r == Record::Slot(Place::Local(item), 2))
            .unwrap();
        let end = records[at..]
            .iter()
            .position(|r| matches!(r, Record::Cut(..)))
            .map(|i| at + i)
            .unwrap();
        let frame = records[..at]
            .iter()
            .rev()
            .find_map(|r| {
                if let Record::Frame(frame, ..) = r {
                    Some(*frame)
                } else {
                    None
                }
            })
            .unwrap();
        let mut task = [ABSENT; 8];
        task[0] = 1;
        task[1] = 1;
        // Private stale-use artifact: attempt the old approved read after Move.
        records.splice(
            end..end,
            [
                Record::Executing,
                Record::Continuation(frame),
                Record::Meta(Part::Task, task),
                Record::Expression(*ordinal),
                Record::End,
            ],
        );
    }
    assert_eq!(
        compare(&a, &b, comparison()),
        Comparison::Inconclusive(Incomplete::InvalidState)
    );

    // A dead shared handle must not gain a new read just because the owner is
    // still available after the later exclusive write. Corrupt both histories.
    let e = prepare_execution(SHARED_THEN_EXCLUSIVE, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    let b = session(&c, 1024);
    let owner = e.program.locals().iter().find(|local| local.name == "value").unwrap().id;
    let ordinal = c.expressions().iter().find(|(e, _)| matches!(e.kind, ExprKind::Deref { .. })).unwrap().1;
    for session in [&a, &b] {
        let mut observer = session.machine().observation.borrow_mut();
        let records = &mut observer.as_mut().unwrap().records;
        let at = records.windows(2).position(|r| r[0] == Record::Slot(Place::Local(owner), 1) && r[1] == Record::Bool(false)).unwrap();
        let end = records[at..].iter().position(|r| matches!(r, Record::Cut(..))).map(|i| at + i).unwrap();
        let frame = records[..at].iter().rev().find_map(|r| if let Record::Frame(frame, ..) = r { Some(*frame) } else { None }).unwrap();
        let mut task = [ABSENT; 8];
        task[0] = 1;
        task[1] = 1;
        records.splice(end..end, [Record::Executing, Record::Continuation(frame), Record::Meta(Part::Task, task), Record::Expression(ordinal), Record::End]);
    }
    assert_eq!(compare(&a, &b, comparison()), Comparison::Inconclusive(Incomplete::InvalidState));
}

#[test]
fn repair_canonical_resolution_limits_preserve_primary_and_prevent_truncated_equality() {
    let source = "fn demo()->bool{let item=true;let first=&item;let second=first;return *second;}";
    let e = prepare_execution(source, AdmissionLimits::default()).unwrap();
    let c = contract(&e);
    let a = session(&c, 1024);
    let b = session(&c, 1024);
    assert_eq!(compare(&a, &b, comparison()), Comparison::Equal);
    assert_eq!(
        compare(
            &a,
            &b,
            ComparisonLimits {
                work: 1,
                ..comparison()
            }
        ),
        Comparison::Inconclusive(Incomplete::Work)
    );
    let mut narrow = Session::new(
        &c,
        ExecutionLimits {
            steps: 1024,
            ..ExecutionLimits::default()
        },
        Limits {
            lineage: 1,
            ..observation()
        },
    )
    .unwrap();
    narrow.advance().unwrap();
    assert_eq!(narrow.completeness(), Ok(()));
    assert_eq!(
        compare(&narrow, &narrow, comparison()),
        Comparison::Inconclusive(Incomplete::Lineage)
    );
    assert_eq!(narrow.primary(), a.primary());
    assert_eq!(
        a.primary().unwrap(),
        Ok(&ExecutionResult::Value(RuntimeValue::Bool(true)))
    );
}
