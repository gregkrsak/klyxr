use std::{env, fs, process};

use klyxr_compiler::{compile_source, diagnostics::Diagnostic, execution, verify, FrontendError};
mod run_reporting;

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

    if command == "run" {
        return run_execution(args);
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
                | FrontendError::Ownership(diagnostics)
                | FrontendError::Effects(diagnostics) => {
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
        let no_value = program
            .functions()
            .iter()
            .filter_map(|f| f.as_ordinary())
            .filter(|f| f.return_type.is_none())
            .count();
        let status = if report.functions_proven == 0 && ordinary > 0 {
            if no_value == 0 {
                "checked ordinary value functions; no verified function contracts"
            } else {
                "checked ordinary functions; no verified function contracts"
            }
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
            let value_functions = ordinary - no_value;
            if no_value > 0 {
                println!("  ordinary no-value functions type-checked: {no_value} (not executed or verified)");
            }
            println!("  ordinary value functions type-checked: {value_functions} (not executed or verified; constrained results are not proven)");
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
        println!("  scope: ordinary control flow with loop-stable Copy state and iteration-local Move/borrowing plus stable carried references and specialized signed i64 contract prototype; general ownership, effects, and code generation are not implemented");
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

fn run_execution(mut args: impl Iterator<Item = String>) -> Result<(), i32> {
    use execution::{AdmissionLimits, ExecutionLimits};
    let Some(path) = args.next() else {
        eprintln!("usage: klyxr run <file.klx> --function <name> [limit overrides]");
        return Err(2);
    };
    let mut admission = AdmissionLimits::default();
    let mut execution = ExecutionLimits::default();
    let mut entry = None;
    let mut flags = std::collections::BTreeSet::new();
    for _ in 0..32 {
        let Some(flag) = args.next() else {
            break;
        };
        if flag.len() > 64 || !flags.insert(flag.clone()) {
            eprintln!("error: invalid or repeated run option");
            return Err(2);
        }
        let Some(value) = args.next() else {
            eprintln!("error: missing run option value");
            return Err(2);
        };
        if flag == "--function" {
            if value.len() > AdmissionLimits::MAX.identifier_bytes {
                eprintln!("error: selected function name exceeds supported maximum");
                return Err(2);
            }
            entry = Some(value);
            continue;
        }
        let n: u64 = match value.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("error: invalid or overflowing run limit");
                return Err(2);
            }
        };
        let size = match usize::try_from(n) {
            Ok(n) => n,
            Err(_) => {
                eprintln!("error: overflowing run limit");
                return Err(2);
            }
        };
        match flag.as_str() {
            "--max-source-bytes" => admission.source_bytes = size,
            "--max-tokens" => admission.tokens = size,
            "--max-identifier-bytes" => admission.identifier_bytes = size,
            "--max-frontend-work" => admission.frontend_work = n,
            "--max-traversal-depth" => admission.traversal_depth = size,
            "--max-alias-depth" => admission.alias_depth = size,
            "--max-alias-work" => admission.alias_work = n,
            "--max-admission-storage" => admission.working_storage = size,
            "--max-diagnostic-storage" => admission.diagnostic_storage = size,
            "--max-output-bytes" => {
                admission.rendered_output = size;
                execution.rendered_output = size;
            }
            "--max-steps" => execution.steps = n,
            "--max-call-depth" => execution.call_depth = size,
            "--max-expression-depth" => execution.expression_depth = size,
            "--max-working-storage" => execution.working_storage = size,
            "--max-trace-depth" => execution.trace_depth = size,
            _ => {
                eprintln!("error: unknown run option");
                return Err(2);
            }
        }
    }
    if args.next().is_some() {
        eprintln!("error: too many run options");
        return Err(2);
    }
    let Some(entry) = entry else {
        eprintln!("error: run requires --function <name>");
        return Err(2);
    };
    if admission.validate().is_err() || execution.validate().is_err() {
        eprintln!("error: configured run limit is zero or exceeds supported maximum");
        return Err(2);
    }
    let file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("error: detected host/tool failure: source input");
            return Err(2);
        }
    };
    let artifact = match execution::prepare_execution_from_reader(file, admission) {
        Ok(e) => e,
        Err(e) => {
            return run_reporting::preparation_error(
                &e,
                admission.rendered_output,
                std::io::stderr().lock(),
            );
        }
    };
    let result = artifact
        .entry(&entry)
        .and_then(|entry| execution::execute(&artifact, entry, execution))
        .and_then(|r| artifact.render_result(&r, execution.rendered_output));
    match result {
        Ok(text) => execution::write_output(std::io::stdout().lock(), &text).map_err(|_| 2),
        Err(e) => run_reporting::execution_error(
            &artifact,
            &e,
            execution.rendered_output,
            std::io::stderr().lock(),
        ),
    }
}
