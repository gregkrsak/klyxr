use std::{path::PathBuf, process::Command};

fn example(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_klyxr"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn success_reports_only_supported_proofs() {
    for (command, status) in [("verify", "verified"), ("check", "checked")] {
        let output = run(&[command, &example("battery_ok.klx")]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.starts_with(&format!("{status} supported integer contracts:")));
        assert!(text.contains("function bodies proven: 1"));
        assert!(text.contains("calls checked: 1"));
        assert!(
            text.contains("general ownership, effects, and code generation are not implemented")
        );
    }
}

#[test]
fn rejected_programs_return_one_and_no_success_message() {
    for (name, message) in [
        ("battery_fail.klx", "90 <= 80 is false"),
        ("battery_sequence_fail.klx", "50 <= 30 is false"),
        (
            "battery_body_fail.klx",
            "postcondition cannot be established",
        ),
        ("effects.klx", "supported grammar"),
    ] {
        for command in ["verify", "check"] {
            let output = run(&[command, &example(name)]);
            assert_eq!(output.status.code(), Some(1), "{name}");
            assert!(output.stdout.is_empty());
            let text = String::from_utf8(output.stderr).unwrap();
            assert!(text.contains(message), "{text}");
            assert!(text.contains("-->"));
            assert!(text.contains(name));
        }
    }
}

#[test]
fn usage_and_io_errors_return_two() {
    for args in [
        vec![],
        vec!["build"],
        vec!["verify"],
        vec!["--version", "extra"],
        vec!["verify", "one.klx", "two.klx"],
        vec!["verify", "file-that-does-not-exist.klx"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
    }
    assert!(run(&["--version"]).status.success());
}

#[test]
fn ordinary_functions_report_typing_without_execution_or_proof() {
    for command in ["check", "verify"] {
        let output = run(&[command, &example("value_flow.klx")]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(
            text.starts_with("checked ordinary value functions; no verified function contracts:")
        );
        assert!(text.contains("ordinary value functions type-checked: 6"));
        assert!(text.contains("not executed or verified; constrained results are not proven"));
        assert!(text.contains("function bodies proven: 0"));
        assert!(text.contains("calls checked: 0"));
        assert!(!text.contains("verified supported integer contracts"));
    }
}

#[test]
fn record_moves_are_checked_without_claiming_execution_or_verification() {
    for command in ["check", "verify"] {
        let output = run(&[command, &example("move_values.klx")]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("ordinary value functions type-checked: 8"));
        assert!(text.contains("core Copy/move checking passed"));
        assert!(text.contains("whole-value loans checked; Copy-safe borrowed access checked; destruction is not implemented"));
        assert!(text.contains("not executed or verified"));
        assert!(text.contains("function bodies proven: 0"));
    }
}

#[test]
fn ownership_failure_exits_one_and_reports_both_move_sites() {
    for command in ["check", "verify"] {
        let output = run(&[command, &example("move_fail.klx")]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(text.contains("use of moved value `ticket`"));
        assert!(text.contains("move_fail.klx:7:12"));
        assert!(text.contains("into local `next` at line 6, column 16"));
        assert!(text.contains("ownership checking stopped"));
    }
}

#[test]
fn borrowing_success_reports_loans_without_execution_or_proof() {
    for command in ["check", "verify"] {
        let output = run(&[command, &example("borrow_values.klx")]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("whole-value loans checked"));
        assert!(text.contains("not executed or verified"));
        assert!(text.contains("function bodies proven: 0"));
        assert!(text.contains("calls checked: 0"));
        assert!(text.contains("destruction is not implemented"));
    }
}

#[test]
fn borrowing_failure_reports_the_conflict_and_earlier_loan_location() {
    for command in ["check", "verify"] {
        let output = run(&[command, &example("borrow_fail.klx")]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(text.contains("cannot move `ticket` while it is shared-borrowed"));
        assert!(text.contains("borrow_fail.klx:11:17"));
        assert!(text.contains("borrow began at line 10, column 16"));
        assert!(text.contains("ownership checking stopped"));
        assert!(!text.contains("LoanId"));
    }
}

#[test]
fn copy_safe_mutation_reports_frontend_acceptance_without_runtime_claims() {
    for command in ["check", "verify"] {
        let output = run(&[command, &example("deref_mutation.klx")]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Copy-safe borrowed access checked"));
        assert!(text.contains("not executed or verified"));
        assert!(text.contains("function bodies proven: 0"));
        assert!(text.contains("calls checked: 0"));
        assert!(text.contains("destruction is not implemented"));
    }
}
#[test]
fn non_copy_mutation_failure_explains_the_replacement_boundary() {
    for command in ["check", "verify"] {
        let output = run(&[command, &example("deref_mutation_fail.klx")]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(text.contains("cannot replace non-Copy value `Ticket`"));
        assert!(text.contains("before destruction semantics are defined"));
        assert!(text.contains("deref_mutation_fail.klx:6:5"));
        assert!(text.contains("ownership checking stopped"));
    }
}
