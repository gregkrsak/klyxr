//! Hand-derived observations for one admitted ordinary MIR computation.
//! No stepping, capture, replay, runtime census or malformed builder supplies
//! the decisive expected prefix, charge, value or capacity transition.
use super::*;

fn meta(records: &mut Vec<Record>, part: Part, fields: &[u64]) {
    let mut data = [ABSENT; 8];
    data[..fields.len()].copy_from_slice(fields);
    records.push(Record::Meta(part, data));
}
fn task(records: &mut Vec<Record>, frame: u64, code: u64, expression: usize) {
    records.push(Record::Continuation(frame));
    if code == 1 {
        meta(records, Part::Task, &[1, 1]);
        records.push(Record::Expression(expression));
    } else {
        meta(records, Part::Task, &[code]);
    }
    records.push(Record::End);
}

type Row = (
    Cut,
    u64,
    bool,
    bool,
    usize,
    usize,
    usize,
    &'static [bool],
    &'static [u64],
);

fn oracle(c: &contract::Contract<'_>, execution: ExecutionLimits) -> Capture {
    // Exactly one entry, a Return terminator and a bool literal: three fuel
    // units. The table specifies the compatibility reservations independently
    // of the interpreter's selected capacity and traversal implementation.
    assert!(execution.steps == 2 || execution.steps == 3);
    assert_eq!(c.functions(), &[c.entry()]);
    assert_eq!(c.sites().len(), 2);
    let function = c.executable.program.function(c.entry()).span();
    let terminator = c.executable.origin(c.entry(), BasicBlockId(0)).unwrap();
    let literal = c
        .sites()
        .iter()
        .find(|s| s.role == contract::Role::Expression)
        .unwrap();
    let [f, t, v, ..] = c.schedule().machine;
    let table: [Row; 15] = [
        (Cut::Start, 0, false, false, 0, 0, 0, &[], &[]),
        (Cut::Fuel, 0, false, false, 0, 0, 0, &[], &[]),
        (Cut::Depth, 1, false, false, 0, 0, 0, &[], &[]),
        (Cut::Storage(f), 1, false, false, 0, 0, 0, &[], &[]),
        (Cut::Storage(t), 1, true, false, 1, 0, 0, &[], &[]),
        (Cut::Fuel, 1, true, false, 1, 1, 0, &[], &[]),
        (Cut::Storage(t), 2, true, false, 1, 1, 0, &[], &[14]),
        (Cut::Transition, 2, true, false, 1, 2, 0, &[], &[14, 1]),
        (Cut::Fuel, 2, true, true, 1, 2, 0, &[], &[14]),
        (Cut::ExpressionDepth(1), 3, true, true, 1, 2, 0, &[], &[14]),
        (Cut::Storage(v), 3, true, true, 1, 2, 0, &[], &[14]),
        (Cut::Transition, 3, true, true, 1, 2, 1, &[true], &[14]),
        (Cut::Storage(0), 3, true, false, 1, 2, 1, &[], &[]),
        (Cut::Transition, 3, false, false, 1, 2, 1, &[true], &[]),
        (Cut::Final, 3, false, false, 1, 2, 1, &[], &[]),
    ];
    let rows: Vec<Row> = if execution.steps == 2 {
        table[..9]
            .iter()
            .copied()
            .chain([
                (Cut::Transition, 2, true, true, 1, 2, 0, &[][..], &[14][..]),
                (Cut::Final, 2, true, true, 1, 2, 0, &[][..], &[14][..]),
            ])
            .collect()
    } else {
        table.to_vec()
    };
    let mut records = Vec::new();
    // Deliberately lawful different raw frame identity from the real driver.
    let frame = 700;
    for (ordinal, (cut, fuel, active, expr, fcap, tcap, vcap, values, tasks)) in
        rows.iter().copied().enumerate()
    {
        if matches!(ordinal, 5 | 8 | 12) {
            records.push(Record::Executing);
            task(
                &mut records,
                frame,
                match ordinal {
                    5 => 0,
                    8 => 1,
                    _ => 14,
                },
                literal.ordinal,
            );
        }
        let origin = if ordinal == 0 {
            Origin {
                span: START,
                kind: OriginKind::FunctionBoundary,
            }
        } else if ordinal < 5 {
            Origin {
                span: function,
                kind: OriginKind::FunctionBoundary,
            }
        } else {
            terminator
        };
        let terminal = execution.steps == 2 && ordinal >= 9;
        records.push(Record::Cut(
            cut,
            fuel,
            usize::from(active),
            origin.span,
            if expr { Some(literal.origin) } else { None },
            terminal,
        ));
        records.push(Record::OriginKind(origin.kind));
        records.push(Record::Inventory(ordinal, 0, values.len(), tasks.len(), 0));
        if active {
            records.push(Record::Frame(
                frame,
                c.entry(),
                BasicBlockId(0),
                0,
                None,
                function,
            ));
            records.push(Record::FrameStorage(0));
            records.push(Record::End);
        }
        for value in values {
            records.push(Record::Operand);
            records.push(Record::Bool(*value));
        }
        for code in tasks {
            task(&mut records, frame, *code, literal.ordinal);
        }
        records.push(Record::Accounting(
            execution,
            Some(fcap * f + tcap * t + vcap * v),
        ));
        meta(
            &mut records,
            Part::Buffers,
            &[
                fcap as u64,
                tcap as u64,
                vcap as u64,
                ABSENT,
                ABSENT,
                ABSENT,
            ],
        );
        records.push(Record::End);
    }
    if execution.steps == 2 {
        records.push(Record::Stop(2, terminator.span, Some(literal.origin)));
        records.push(Record::Limit("steps"));
    } else {
        records.push(Record::Delivered);
        records.push(Record::Bool(true));
    }
    records.push(Record::End);
    let sealed_count = Some(records.len());
    Capture {
        records,
        limits: Limits {
            records: 4096,
            ..Limits::default()
        },
        work: 0,
        incomplete: None,
        finished: true,
        expressions: Vec::new(),
        snapshots: rows.len(),
        sealed_count,
        controlled: false,
    }
}

#[test]
fn hand_derived_full_success_and_first_insufficient_observations_match() {
    let e = prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
    let c = contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default())
        .unwrap();
    for steps in [2, 3] {
        let execution = ExecutionLimits {
            steps,
            ..ExecutionLimits::default()
        };
        let expected = oracle(&c, execution);
        let mut actual = Session::new(&c, execution, Limits::default()).unwrap();
        let _ = actual.advance();
        let result = if steps == 2 {
            let term = e.origin(c.entry(), BasicBlockId(0)).unwrap();
            let mut error = ExecutionError::new(ErrorKind::ConfiguredLimit("steps"), term.span);
            error.expression = Some(c.sites()[1].origin);
            Err(error)
        } else {
            Ok(ExecutionResult::Value(RuntimeValue::Bool(true)))
        };
        let mut scratch = Scratch::new(ComparisonLimits::default(), c.schedule()).unwrap();
        let mut work = Work(20_000_000);
        validate(
            &c,
            &expected,
            &mut scratch,
            &mut work,
            execution,
            Some(result.as_ref()),
            None,
        )
        .unwrap();
        let evidence = actual.capture();
        validate(
            &c,
            &evidence,
            &mut scratch,
            &mut work,
            execution,
            actual.primary(),
            None,
        )
        .unwrap();
        assert_eq!(
            correspondence(
                &expected.records,
                &evidence.records,
                &mut scratch,
                &mut work
            )
            .unwrap(),
            Comparison::Equal
        );
    }
}

#[test]
fn independent_exact_observation_records_and_storage_neighbors_preserve_primary() {
    let e = prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
    let c = contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default())
        .unwrap();
    let execution = ExecutionLimits {
        steps: 3,
        ..ExecutionLimits::default()
    };
    let expected = oracle(&c, execution);
    let count = expected.records.len();
    let units = c.schedule();
    let exact = count * units.observation[0]
        + units.observation[1]
        + c.expressions().len() * units.extraction[2];
    for (records, storage, complete) in [
        (count, exact, true),
        (count - 1, exact, false),
        (count, exact - 1, false),
    ] {
        let mut session = Session::new(
            &c,
            execution,
            Limits {
                records,
                storage,
                ..Limits::default()
            },
        )
        .unwrap();
        assert_eq!(
            session.advance().unwrap(),
            &ExecutionResult::Value(RuntimeValue::Bool(true))
        );
        assert_eq!(session.completeness().is_ok(), complete);
    }
}
