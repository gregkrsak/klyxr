use klyxr_compiler::{
    compile_source,
    verify::{verify_report, VerificationReport},
};

const PREFIX: &str = "\
type Percent = range 0..100;
record Battery { charge: Percent }
verified fn consume(battery: &mut Battery, amount: Percent)
    requires amount <= battery.charge
    ensures battery.charge == old(battery.charge) - amount
{ battery.charge -= amount; }
";

fn report(source: &str) -> VerificationReport {
    verify_report(&compile_source(source).expect("fixture should parse"))
}

fn fails(source: &str, message: &str) {
    let result = report(source);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|error| error.message.contains(message)),
        "expected {message:?}, got {:?}",
        result.diagnostics
    );
}

#[test]
fn canonical_examples_and_keyword_aliases() {
    let result = report(include_str!("../../examples/battery_ok.klx"));
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.functions_proven, 1);
    assert_eq!(result.calls_checked, 1);
    assert_eq!(result.final_values["battery"], 30);
    fails(
        include_str!("../../examples/battery_fail.klx"),
        "precondition cannot be established",
    );
    let compact = report(&format!(
        "{PREFIX}let mut battery = Battery {{ charge: 80 }}; consume(&mut battery, 50);"
    ));
    assert_eq!(compact.final_values, result.final_values);
}

#[test]
fn calls_use_updated_state_and_stop_after_failure() {
    let result = report(&format!(
        "{PREFIX}let mut battery = Battery {{ charge: 80 }};
        consume(&mut battery, 50); consume(&mut battery, 50); consume(&mut battery, 0);"
    ));
    assert_eq!(result.calls_checked, 1);
    assert_eq!(result.final_values["battery"], 30);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].conclusion, "50 <= 30 is false");
    assert!(result.diagnostics[0]
        .known
        .contains(&"battery.charge == 30".into()));
}

#[test]
fn independent_records_and_interleaved_construction() {
    let result = report(&format!(
        "{PREFIX}let mut first = Battery {{ charge: 80 }};
        consume(&mut first, 50); let mutable second = Battery {{ charge: 90 }};
        consume(&mutable second, 90); consume(&mut first, 30);"
    ));
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.calls_checked, 3);
    assert_eq!(result.final_values["first"], 0);
    assert_eq!(result.final_values["second"], 0);
}

#[test]
fn body_is_proven_without_call_sites() {
    assert_eq!(report(PREFIX).functions_proven, 1);
    fails(
        &PREFIX.replace("battery.charge -= amount;", ""),
        "postcondition cannot be established",
    );
    fails(
        &PREFIX.replace("battery.charge -= amount;", "battery.charge -= 1;"),
        "field range preservation",
    );
    fails(
        &PREFIX.replace(
            "battery.charge -= amount;",
            "battery.charge -= amount; battery.charge -= amount;",
        ),
        "field range preservation",
    );
}

#[test]
fn unknown_declarations_and_references_never_succeed() {
    for (source, message) in [
        (
            PREFIX.replace("charge: Percent", "charge: Missing"),
            "unknown range type",
        ),
        (
            PREFIX.replace("&mut Battery", "&mut Missing"),
            "unknown record type",
        ),
        (
            PREFIX.replace("amount: Percent", "amount: Missing"),
            "unknown range type",
        ),
        (
            PREFIX.replace(
                "requires amount <= battery.charge",
                "requires amount <= battery.missing",
            ),
            "unknown precondition field",
        ),
        (
            PREFIX.replace("old(battery.charge)", "old(other.charge)"),
            "invalid field reference",
        ),
        (
            PREFIX.replace("old(battery.charge)", "old(battery.missing)"),
            "invalid field reference",
        ),
        (
            PREFIX.replace("ensures battery.charge", "ensures other.charge"),
            "invalid field reference",
        ),
        (
            PREFIX.replace("- amount\n", "- missing\n"),
            "unknown postcondition parameter",
        ),
        (
            PREFIX.replace("battery.charge -= amount", "battery.missing -= amount"),
            "invalid field reference",
        ),
        (
            PREFIX.replace("battery.charge -= amount", "other.charge -= amount"),
            "invalid field reference",
        ),
        (
            PREFIX.replace("battery.charge -= amount", "battery.charge -= missing"),
            "unknown subtraction parameter",
        ),
    ] {
        fails(&source, message);
    }
    for (suffix, message) in [
        ("let mut b = Missing { charge: 80 };", "unknown record type"),
        (
            "let mut b = Battery { missing: 80 };",
            "unknown initializer field",
        ),
        ("consume(&mut missing, 50);", "unknown binding"),
        (
            "let mut b = Battery { charge: 80 }; missing(&mut b, 50);",
            "unknown function",
        ),
        (
            "consume(&mut b, 50); let mut b = Battery { charge: 80 };",
            "unknown binding",
        ),
    ] {
        fails(&format!("{PREFIX}{suffix}"), message);
    }
}

#[test]
fn rejects_immutable_and_distinct_record_arguments() {
    fails(
        &format!("{PREFIX}let b = Battery {{ charge: 80 }}; consume(&mut b, 50);"),
        "immutable binding",
    );
    fails(
        &format!(
            "{PREFIX}record Other {{ charge: Percent }}
        let mut b = Other {{ charge: 80 }}; consume(&mut b, 50);"
        ),
        "record argument type mismatch",
    );
}

#[test]
fn rejects_duplicate_names_and_invalid_ranges() {
    for suffix in [
        "type Percent = range 0..100;",
        "record Battery { charge: Percent }",
        "record Percent { charge: Percent }",
    ] {
        fails(&format!("{PREFIX}{suffix}"), "duplicate declaration");
    }
    fails(&format!("{PREFIX}{PREFIX}"), "duplicate declaration");
    fails(
        &format!("{PREFIX}let b = Battery {{ charge: 80 }}; let b = Battery {{ charge: 90 }};"),
        "duplicate declaration",
    );
    fails(
        &PREFIX.replace("amount", "battery"),
        "duplicate declaration",
    );
    fails(
        &PREFIX.replace("0..100", "100..0"),
        "invalid range declaration",
    );
}

#[test]
fn validates_initial_values_and_argument_boundaries() {
    fails(
        &format!("{PREFIX}let mut b = Battery {{ charge: 101 }};"),
        "range constraint",
    );
    for amount in [-1, 101] {
        fails(
            &format!("{PREFIX}let mut b = Battery {{ charge: 80 }}; consume(&mut b, {amount});"),
            "argument violates range",
        );
    }
    for (initial, amount) in [(0, 0), (100, 100), (100, 0)] {
        let result = report(&format!(
            "{PREFIX}let mut b = Battery {{ charge: {initial} }}; consume(&mut b, {amount});"
        ));
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.final_values["b"], initial - amount);
    }
}

#[test]
fn unsupported_and_malformed_syntax_is_not_skipped() {
    for source in [
        PREFIX.replace(
            "battery.charge -= amount;",
            "if amount { battery.charge -= amount; }",
        ),
        PREFIX.replace("battery.charge -= amount;", "unknown(&mut battery, 1);"),
        PREFIX.replace("battery.charge -= amount;", "battery.charge = 0;"),
        PREFIX.replace("battery.charge -= amount;", "{ battery.charge -= amount; }"),
        PREFIX.replace("== old(battery.charge) - amount", "<= old(battery.charge)"),
        PREFIX.replace(
            "== old(battery.charge) - amount",
            "= old(battery.charge) - amount",
        ),
        PREFIX.replace("ensures battery.charge == old(battery.charge) - amount", ""),
        PREFIX.replace("- amount\n", "- amount - 1\n"),
        PREFIX.replace(
            "requires amount <= battery.charge",
            "requires wrong <= battery.charge",
        ),
        PREFIX.replace(
            "requires amount <= battery.charge",
            "requires amount <= other.charge",
        ),
        PREFIX.replace(
            "{ battery.charge -= amount; }",
            "{ battery.charge -= amount;",
        ),
        format!("{PREFIX}unsafe {{}}"),
    ] {
        assert!(
            compile_source(&source).is_err(),
            "silently accepted: {source}"
        );
    }
    assert!(compile_source(include_str!("../../examples/effects.klx")).is_err());
}

#[test]
fn signed_i64_limits_parse_and_arithmetic_overflow_is_rejected() {
    let bounds = "type Full = range -9223372036854775808..9223372036854775807;";
    let result = report(bounds);
    assert!(result.diagnostics.is_empty());
    for value in [
        "9223372036854775808",
        "-9223372036854775809",
        "18446744073709551616",
    ] {
        assert!(compile_source(&format!("type Bad = range {value}..0;")).is_err());
    }
    let source = PREFIX
        .replace("type Percent = range 0..100;", bounds)
        .replace("Percent", "Full");
    fails(&source, "subtraction overflow cannot be excluded");
    let zero_amount = source
        .replace("record Battery", "type Zero = range 0..0; record Battery")
        .replace("amount: Full", "amount: Zero");
    assert!(report(&zero_amount).diagnostics.is_empty());
}

#[test]
fn rejects_nonempty_range_damage_and_empty_preconditions() {
    fails(
        &PREFIX.replace("0..100", "10..100"),
        "field range preservation",
    );
    let source = PREFIX
        .replace(
            "record Battery",
            "type TooMuch = range 101..200; record Battery",
        )
        .replace("amount: Percent", "amount: TooMuch");
    fails(&source, "precondition has no admissible inputs");
}

#[test]
fn zero_only_amount_can_have_an_empty_body() {
    let source = PREFIX
        .replace("record Battery", "type Zero = range 0..0; record Battery")
        .replace("amount: Percent", "amount: Zero")
        .replace("battery.charge -= amount;", "");
    assert!(report(&source).diagnostics.is_empty());
}

/// Independent finite model checks the universal proof over many small domains.
/// It enumerates every admissible input and every intermediate assignment,
/// rather than reconstructing the verifier's polygon/vertex implementation.
#[test]
fn affine_proofs_match_exhaustive_execution_on_small_domains() {
    let bodies: &[&[Option<i64>]] = &[
        &[],
        &[None],
        &[Some(0)],
        &[Some(1)],
        &[Some(-1)],
        &[None, None],
        &[Some(1), Some(-1), None],
        &[None, Some(1), Some(-1)],
    ];
    for low in -2..=2 {
        for high in low..=2 {
            for amount_low in -2..=2 {
                for amount_high in amount_low..=2 {
                    for body in bodies {
                        let mut has_inputs = false;
                        let mut valid = true;
                        for initial in low..=high {
                            for amount in amount_low..=amount_high {
                                if amount > initial {
                                    continue;
                                }
                                has_inputs = true;
                                let mut current = initial;
                                for operand in *body {
                                    current -= operand.unwrap_or(amount);
                                    valid &= current >= low && current <= high;
                                }
                                valid &= current == initial - amount;
                            }
                        }
                        let statements: String = body
                            .iter()
                            .map(|operand| {
                                format!(
                                    "b.value -= {};",
                                    operand.map_or("a".into(), |n| n.to_string())
                                )
                            })
                            .collect();
                        let source = format!("type State = range {low}..{high}; type Amount = range {amount_low}..{amount_high};
                            record R {{ value: State }} verified fn f(b: &mut R, a: Amount)
                            requires a <= b.value ensures b.value == old(b.value) - a {{ {statements} }}");
                        assert_eq!(
                            report(&source).diagnostics.is_empty(),
                            has_inputs && valid,
                            "{source}"
                        );
                    }
                }
            }
        }
    }
}
