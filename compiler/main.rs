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
                FrontendError::Resolve(diagnostics)
                | FrontendError::Type(diagnostics)
                | FrontendError::Ownership(diagnostics) => {
                    for diagnostic in diagnostics {
                        eprint!("{}", diagnostic.render(&path, &source));
                    }
                    return Err(1);
                }
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

    let mir = klyxr_compiler::mir::lower(&program);
    let report = verify::verify_report(&program);

    if report.diagnostics.is_empty() {
        let ordinary = program
            .functions()
            .iter()
            .filter(|f| f.as_ordinary().is_some())
            .count();
        let status = if report.functions_proven == 0 && ordinary > 0 {
            "checked ordinary value functions; no verified function contracts"
        } else if report.functions_proven == 0 {
            "checked declarations; no function contracts to verify"
        } else if command == "verify" {
            "verified supported integer contracts"
        } else {
            "checked supported integer contracts"
        };
        println!("{status}: {path}");
        if ordinary > 0 {
            println!("  ordinary MIR control-flow lowering passed: {} functions (not executed or verified)", mir.functions().len());
            println!("  ordinary value functions type-checked: {ordinary} (not executed or verified; constrained results are not proven)");
            println!("  ordinary ownership: core Copy/move checking passed; whole-value loans checked; Copy-safe borrowed access checked; destruction is not implemented");
        }
        println!(
            "  function bodies proven: {} (subtraction safety, field ranges, postconditions)",
            report.functions_proven
        );
        println!(
            "  calls checked: {} (argument ranges, mutable access, preconditions)",
            report.calls_checked
        );
        println!("  scope: ordinary control flow with loop-stable Copy state and specialized signed i64 contract prototype; general ownership, effects, and code generation are not implemented");
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
