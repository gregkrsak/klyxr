use std::{env, fs, process};

use klyxr_compiler::{compile_source, diagnostics::Diagnostic, verify, FrontendError};

fn main() {
    if let Err(code) = run() {
        process::exit(code);
    }
}

fn run() -> Result<(), i32> {
    let mut args = env::args().skip(1);

    let Some(command) = args.next() else {
        print_usage();
        return Err(2);
    };

    if command == "--version" || command == "-V" {
        if args.next().is_some() {
            eprintln!("error: too many arguments");
            return Err(2);
        }
        println!("klyxr 0.0.1 (vertical-slice prototype)");
        return Ok(());
    }

    if command != "check" && command != "verify" {
        eprintln!("error: unknown command `{command}`");
        print_usage();
        return Err(2);
    }

    let Some(path) = args.next() else {
        eprintln!("error: missing .klx input path");
        print_usage();
        return Err(2);
    };

    if args.next().is_some() {
        eprintln!("error: too many arguments");
        print_usage();
        return Err(2);
    }

    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: could not read `{path}`: {error}");
            return Err(2);
        }
    };

    let program = match compile_source(&source) {
        Ok(program) => program,
        Err(error) => {
            let (span, message) = match error {
                FrontendError::Lex(error) => (error.span, error.message),
                FrontendError::Parse(error) => (error.span, error.message),
            };
            eprint!(
                "{}",
                Diagnostic::semantic(
                    span,
                    message,
                    "use the supported grammar documented in compiler/README.md"
                )
                .render(&path, &source)
            );
            return Err(1);
        }
    };

    let report = verify::verify_report(&program);

    if report.diagnostics.is_empty() {
        let status = if report.functions_proven == 0 {
            "checked declarations; no function contracts to verify"
        } else if command == "verify" {
            "verified supported integer contracts"
        } else {
            "checked supported integer contracts"
        };
        println!("{status}: {path}");
        println!(
            "  function bodies proven: {} (subtraction safety, field ranges, postconditions)",
            report.functions_proven
        );
        println!(
            "  calls checked: {} (argument ranges, mutable access, preconditions)",
            report.calls_checked
        );
        println!("  scope: straight-line signed i64 prototype; general ownership, effects, and code generation are not implemented");
        return Ok(());
    }

    for (index, diagnostic) in report.diagnostics.iter().enumerate() {
        if index > 0 {
            eprintln!();
        }
        eprint!("{}", diagnostic.render(&path, &source));
    }

    Err(1)
}

fn print_usage() {
    eprintln!("usage: klyxr <check|verify> <file.klx>");
}
