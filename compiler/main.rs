use std::{env, fs, process};

use klyxr_compiler::{compile_source, verify};

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
            eprintln!("error: {error}");
            return Err(1);
        }
    };

    let diagnostics = verify::verify(&program);

    if diagnostics.is_empty() {
        println!("verified: {path}");
        println!("  ranges: {}", program.ranges.len());
        println!("  records: {}", program.records.len());
        println!("  verified functions: {}", program.functions.len());
        println!("  calls checked: {}", program.calls.len());
        return Ok(());
    }

    for (index, diagnostic) in diagnostics.iter().enumerate() {
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
