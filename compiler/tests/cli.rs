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
