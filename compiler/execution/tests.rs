fn seed(s: &str) -> String {
    if (s.contains("type P=") || s.contains("type P =")) && !s.contains("fn seed(") {
        format!("{s} fn seed(v:P)->P{{return v;}}")
    } else {
        s.into()
    }
}
use super::*;
fn prepare(s: &str) -> CheckedExecutable {
    prepare_execution(&seed(s), AdmissionLimits::default()).unwrap_or_else(|e| panic!("{e}: {s}"))
}
fn rendered(s: &str) -> String {
    let e = prepare(s);
    let r = execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
    e.render_result(&r, 65536).unwrap()
}
const P: &str = "type P = range 0..100; record Pair { first: P, second: P } ";
const TICK: &str =
    "fn tick(p: &mut P) -> P { let previous = *p; *p = previous - 1; return previous; } ";
#[test]
fn typed_results_and_all_basic_expression_forms() {
    for (s, want) in [
        (
            "fn demo()->bool{return !(false || false) && (true == true);}",
            "Bool(true)",
        ),
        ("fn demo(){}", "NoValue"),
        ("fn demo(){return;}", "NoValue"),
        ("type P = range 0..100; fn demo()->P{return 80;}", "P(80)"),
    ] {
        assert_eq!(rendered(s), want);
    }
}
#[test]
fn every_statement_including_reference_and_owned_field_updates() {
    let s=format!("{P}{TICK} fn set(p: &mut Pair) {{ p.first = 60; }} fn demo()->P {{ let mut p=Pair {{first:80,second:90}}; p.second=70; let r=&mut p; set(r); let s=&p; let a=s.first; let b=p.second; let mut x=a; x=b; let t=&mut x; *t=50; return x; }}");
    assert_eq!(rendered(&s), "P(50)");
}
#[test]
fn arguments_evaluate_in_written_order_with_repeated_mutation() {
    let s=format!("{P}{TICK} fn difference(a:P,b:P)->P{{return a-b;}} fn demo()->P{{let mut p=seed(90);return difference(tick(&mut p),tick(&mut p));}}");
    assert_eq!(rendered(&s), "P(1)");
}
#[test]
fn record_initializers_execute_written_order_and_render_declaration_order() {
    let s=format!("{P}{TICK} fn demo()->Pair{{let mut p=seed(90);return Pair {{second:tick(&mut p),first:tick(&mut p)}};}}");
    assert_eq!(rendered(&s), "Pair { first: P(89), second: P(90) }");
}
#[test]
fn owned_field_commit_preserves_rhs_mutation_of_other_field() {
    let s=format!("{P} fn change(p:&mut Pair)->P{{let previous=p.second;p.second=previous-1;return previous;}} fn demo()->Pair{{let mut p=Pair{{first:90,second:80}};p.first=change(&mut p);return p;}}");
    assert_eq!(rendered(&s), "Pair { first: P(80), second: P(79) }");
}
#[test]
fn short_circuit_skips_mutating_call_and_trap() {
    for expr in ["false && mutate(&mut p)", "true || mutate(&mut p)"] {
        let s=format!("{P} fn mutate(p:&mut P)->bool{{*p=*p-1;return true;}} fn demo()->P{{let mut p=seed(80);let b={expr};return p;}}");
        assert_eq!(rendered(&s), "P(80)");
    }
    assert_eq!(
        rendered("type P=range 0..100; fn demo()->bool{let p=seed(0);return false && (p-1 == 0);}"),
        "Bool(false)"
    );
}
#[test]
fn evolving_mir_backedges_continue_break_and_loop_local_reinitialization() {
    let s=format!("{P} fn demo()->P{{let mut p=seed(4);while 0<=p {{let x=p;p=x-1;if p==2 {{continue;}} if p==0 {{break;}} }} return p;}}");
    assert_eq!(rendered(&s), "P(0)");
}
#[test]
fn conditional_initializer_selected_branch_and_early_return() {
    let s=format!("{P}{TICK} fn demo()->P{{let mut p=seed(80);let x=if true {{tick(&mut p)}} else {{tick(&mut p)}};if x==80 {{return p;}} else {{return x;}} }}");
    assert_eq!(rendered(&s), "P(79)");
}
#[test]
fn recursive_activations_nested_caller_references_and_growth() {
    let s=format!("{P} fn recur(n:P,p:&mut P)->P{{if n==0 {{return *p;}} else {{let previous=*p;*p=previous-1;return recur(n-1,p);}} }} fn demo()->P{{let mut p=seed(90);return recur(20,&mut p);}}");
    assert_eq!(rendered(&s), "P(70)");
}
#[test]
fn shared_aliases_copy_and_exclusive_handles_transfer() {
    let s=format!("{P} fn read(a:&P,b:&P)->P{{return *a-*b;}} fn demo()->P{{let mut p=seed(80);let r=&p;let alias=r;let equal=read(r,alias);let exclusive=&mut p;let moved=exclusive;*moved=70;return p-equal;}}");
    assert_eq!(rendered(&s), "P(70)");
}
#[test]
fn record_moves_arguments_and_returns_do_not_consume_field_read_root() {
    let s=format!("{P} fn id(p:Pair)->Pair{{let a=p.first;let b=p.second;return p;}} fn demo()->Pair{{let p=Pair{{first:80,second:90}};let moved=id(p);return moved;}}");
    assert_eq!(rendered(&s), "Pair { first: P(80), second: P(90) }");
}
#[test]
fn fresh_independent_runs() {
    let e = prepare(&format!(
        "{P}{TICK} fn demo()->P{{let mut p=seed(80);return tick(&mut p);}}"
    ));
    let id = e.entry("demo").unwrap();
    assert_eq!(
        execute(&e, id, ExecutionLimits::default()).unwrap(),
        execute(&e, id, ExecutionLimits::default()).unwrap()
    );
}
#[test]
fn entry_rejects_unknown_and_parameterized() {
    let e = prepare("fn parameter(p:bool)->bool{return p;} fn demo()->bool{return true;}");
    assert!(e.entry("missing").is_err());
    assert!(e.entry("parameter").is_err());
}
#[test]
fn mixed_verified_ids_and_failing_proof_are_isolated() {
    let s = format!(
        "{}\nfn demo()->Percent{{return 80;}}",
        include_str!("../../examples/battery_fail.klx")
    );
    let e = prepare(&s);
    let id = e.entry("demo").unwrap();
    assert_ne!(id.0, 0);
    assert!(e.entry("consume").is_err());
    assert!(
        !crate::verify::verify_report(&crate::compile_source(&s).unwrap())
            .diagnostics
            .is_empty()
    );
    assert_eq!(
        e.render_result(&execute(&e, id, ExecutionLimits::default()).unwrap(), 65536)
            .unwrap(),
        "Percent(80)"
    );
}
#[test]
fn whole_input_frontend_errors_block_preparation() {
    for s in [
        "fn demo()->bool{return true;} fn bad()->bool{return missing;}",
        "fn demo()->bool{return true;} fn bad()->bool{return 80;}",
        "record R{x:P} type P=range 0..100; fn bad(a:R)->R{let moved=a;return a;} fn demo(){}",
    ] {
        assert!(matches!(
            prepare_execution(s, AdmissionLimits::default()),
            Err(ExecutionError {
                kind: ErrorKind::Frontend { .. },
                ..
            })
        ));
    }
}
#[test]
fn named_range_bounds_and_invalid_intermediates() {
    assert_eq!(
        rendered("type P=range -100..100; fn demo()->P{let p=seed(-99);return p-1;}"),
        "P(-100)"
    );
    for body in [
        "return p-101;",
        "return (p-101)- -101;",
        "let b=(p-101)<=100;return p;",
    ] {
        let e = prepare(&format!(
            "type P=range 0..100; fn demo()->P{{let p=seed(0);{body}}}"
        ));
        let error = execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap_err();
        assert!(matches!(
            error.kind,
            ErrorKind::RangeTrap { computed: -101, .. }
        ));
        assert!(error.expression.is_some());
        assert!(error.origin.end > error.origin.start);
    }
}
#[test]
fn subtraction_preserves_both_wide_signed_extremes() {
    for (a, b, want) in [
        (i64::MAX, i64::MIN, 18446744073709551615i128),
        (i64::MIN, i64::MAX, -18446744073709551615i128),
    ] {
        let e=prepare(&format!("type P=range -9223372036854775808..9223372036854775807; fn demo()->P{{let p=seed({a});return p - {b};}}"));
        let error = execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap_err();
        assert!(matches!(error.kind,ErrorKind::RangeTrap{computed,..} if computed==want));
        assert!(e
            .render_error(&error, 65536)
            .unwrap()
            .contains(&want.to_string()));
    }
}
#[test]
fn exact_fuel_and_one_short() {
    let e = prepare("fn demo()->bool{return true;}");
    let id = e.entry("demo").unwrap();
    let l = ExecutionLimits {
        steps: 3,
        ..ExecutionLimits::default()
    };
    assert_eq!(
        execute(&e, id, l).unwrap(),
        ExecutionResult::Value(RuntimeValue::Bool(true))
    );
    assert!(matches!(
        execute(&e, id, ExecutionLimits { steps: 2, ..l }),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit("steps"),
            ..
        })
    ));
}
#[test]
fn limit_precedence_is_fuel_before_depth() {
    let e = prepare("fn demo()->bool{return !true;}");
    let id = e.entry("demo").unwrap();
    assert!(matches!(
        execute(
            &e,
            id,
            ExecutionLimits {
                steps: 3,
                expression_depth: 1,
                ..ExecutionLimits::default()
            }
        ),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit("steps"),
            ..
        })
    ));
    assert!(matches!(
        execute(
            &e,
            id,
            ExecutionLimits {
                steps: 4,
                expression_depth: 1,
                ..ExecutionLimits::default()
            }
        ),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit("expression depth"),
            ..
        })
    ));
}
#[test]
fn unsupported_and_overflowing_limits_reject() {
    let mut a = AdmissionLimits::MAX;
    a.tokens += 1;
    assert!(a.validate().is_err());
    a = AdmissionLimits::default();
    a.alias_work = 0;
    assert!(a.validate().is_err());
    let mut e = ExecutionLimits {
        steps: u64::MAX,
        ..ExecutionLimits::default()
    };
    assert!(e.validate().is_err());
    e.steps = 1;
    e.working_storage = usize::MAX;
    assert!(e.validate().is_err());
}
#[test]
fn bounded_ingestion_reads_only_one_lookahead_byte() {
    struct Endless {
        read: usize,
    }
    impl Read for Endless {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            self.read += b.len();
            b.fill(b' ');
            Ok(b.len())
        }
    }
    let mut input = Endless { read: 0 };
    assert!(matches!(
        prepare_execution_from_reader(
            &mut input,
            AdmissionLimits {
                source_bytes: 8,
                ..AdmissionLimits::default()
            }
        ),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit("source bytes"),
            ..
        })
    ));
    assert_eq!(input.read, 9);
}
#[test]
fn detected_reader_and_writer_failure_remain_host_failures() {
    struct Fail;
    impl Read for Fail {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::Other))
        }
    }
    impl std::io::Write for Fail {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(
        prepare_execution_from_reader(Fail, AdmissionLimits::default()),
        Err(ExecutionError {
            kind: ErrorKind::Host(_),
            ..
        })
    ));
    assert!(matches!(
        write_output(Fail, "Bool(true)"),
        Err(ExecutionError {
            kind: ErrorKind::Host(_),
            ..
        })
    ));
}
#[test]
fn token_identifier_and_work_storage_diagnostic_reservations() {
    for (limits, want) in [
        (
            AdmissionLimits {
                tokens: 2,
                ..AdmissionLimits::default()
            },
            "token storage",
        ),
        (
            AdmissionLimits {
                identifier_bytes: 2,
                ..AdmissionLimits::default()
            },
            "identifier bytes",
        ),
        (
            AdmissionLimits {
                frontend_work: 30,
                ..AdmissionLimits::default()
            },
            "frontend work reservation",
        ),
        (
            AdmissionLimits {
                working_storage: 8192,
                ..AdmissionLimits::default()
            },
            "frontend storage reservation",
        ),
        (
            AdmissionLimits {
                diagnostic_storage: 64,
                ..AdmissionLimits::default()
            },
            "diagnostic storage reservation",
        ),
    ] {
        assert!(
            matches!(prepare_execution("fn demo()->bool{return true;}",limits),Err(ExecutionError{kind:ErrorKind::ConfiguredLimit(s),..}) if s==want)
        );
    }
}
#[test]
fn nested_parser_and_flat_expression_depth_rejected_before_deeper_growth() {
    let s = format!(
        "fn demo()->bool{{return {}true{};}}",
        "(".repeat(1000),
        ")".repeat(1000)
    );
    let l = AdmissionLimits::MAX;
    assert!(matches!(
        prepare_execution(&s, l),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit(_),
            ..
        })
    ));
    let s = format!(
        "type P=range 0..100; fn demo()->P{{let p=seed(80);return p{};}}",
        "-0".repeat(50)
    );
    assert!(matches!(
        prepare_execution(&s, l),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit(_),
            ..
        })
    ));
}
#[test]
fn alias_lineage_depth_and_total_work_are_independent() {
    let s="type P=range 0..100;fn seed(v:P)->P{return v;} fn demo()->P{let p=seed(80);let a=&p;let b=a;let c=b;return *c;}";
    assert_eq!(rendered(s), "P(80)");
    for limits in [
        AdmissionLimits {
            alias_depth: 1,
            ..AdmissionLimits::default()
        },
        AdmissionLimits {
            alias_work: 1,
            ..AdmissionLimits::default()
        },
    ] {
        assert!(matches!(
            prepare_execution(s, limits),
            Err(ExecutionError {
                kind: ErrorKind::ConfiguredLimit(_),
                ..
            })
        ));
    }
}
#[test]
fn result_rendering_and_diagnostic_truncation_are_bounded() {
    let e = prepare("fn demo()->bool{return true;}");
    let r = execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
    assert!(matches!(
        e.render_result(&r, 4),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit("rendered output"),
            ..
        })
    ));
    let error =
        ExecutionError::integrity("a long integrity diagnostic that exceeds a small allowance");
    let text = e.render_error(&error, 48).unwrap();
    assert!(text.contains("truncated"));
    assert!(text.len() <= 48);
    assert_eq!(text, e.render_error(&error, 48).unwrap());
}
#[test]
fn cross_compilation_mir_and_origins_cannot_authorize_execution() {
    let mut a = prepare("fn demo()->bool{return true;}");
    let b = prepare("fn demo()->bool{return true;}");
    a.mir = b.mir;
    a.mir_approval = b.mir_approval;
    assert!(matches!(
        execute(&a, a.entry("demo").unwrap(), ExecutionLimits::default()),
        Err(ExecutionError {
            kind: ErrorKind::Integrity(_),
            ..
        })
    ));
    let mut a = prepare("fn demo()->bool{return true;}");
    let b = prepare("fn demo()->bool{return true;}");
    a.origins = b.origins;
    a.origin_approval = b.origin_approval;
    assert!(a.validate().is_err());
}
#[test]
fn missing_invalid_and_mismatched_origins_reject() {
    for mode in 0..3 {
        let mut a = prepare("fn demo()->bool{return true;}");
        match mode {
            0 => a.origins[0].clear(),
            1 => a.origins[0][0].span.line = 0,
            _ => a.origins[0][0].kind = mir::OriginKind::While,
        }
        assert!(matches!(
            a.validate(),
            Err(ExecutionError {
                kind: ErrorKind::Integrity(_),
                ..
            })
        ));
    }
}
#[test]
fn invalid_canonical_metadata_fails_before_indexing() {
    let mut e = prepare("type P=range 0..100;fn demo()->P{return 80;}");
    e.program.ranges[0].id = hir::RangeTypeId(usize::MAX);
    assert!(e.validate().is_err());
}
#[test]
fn typed_only_ownership_invalid_replacement_has_no_execution_authority() {
    let mut e = prepare("type P=range 0..100;record R{x:P}fn demo()->R{let r=R{x:80};return r;}");
    let ast = crate::parse_source(
        "type P=range 0..100;record R{x:P}fn demo()->R{let r=R{x:80};let moved=r;return r;}",
    )
    .unwrap();
    let typed = crate::types::check(crate::resolve::resolve(&ast).unwrap()).unwrap();
    assert!(crate::ownership::check(&typed).is_err());
    let (m, o) = mir::lower_with_origins(&typed);
    e.program = typed;
    e.mir = m;
    e.origins = o;
    assert!(matches!(
        e.validate(),
        Err(ExecutionError {
            kind: ErrorKind::Integrity("frontend ownership approval mismatch"),
            ..
        })
    ));
}
#[test]
fn residual_conditional_and_verified_forms_rejected_at_mir_admission() {
    for mode in 0..3 {
        let mut e = prepare("fn demo()->bool{return true;}");
        let mir::Terminator::Return { value } =
            e.mir.test_functions_mut()[0].test_blocks_mut()[0].test_terminator_mut()
        else {
            panic!()
        };
        let literal = value.clone();
        value.kind = match mode {
            0 => hir::ExprKind::IfValue {
                condition: Box::new(literal.clone()),
                then_value: Box::new(literal.clone()),
                else_value: Box::new(literal),
            },
            1 => hir::ExprKind::FieldAccess(hir::FieldAccess {
                parameter: hir::ParameterId(0),
                field: hir::FieldId(0),
                ty: hir::RangeTypeId(0),
                span: START,
            }),
            _ => hir::ExprKind::OldField(hir::FieldAccess {
                parameter: hir::ParameterId(0),
                field: hir::FieldId(0),
                ty: hir::RangeTypeId(0),
                span: START,
            }),
        };
        assert!(matches!(
            e.validate(),
            Err(ExecutionError {
                kind: ErrorKind::Integrity(_),
                ..
            })
        ));
    }
}
#[test]
fn injected_deep_mir_stops_before_recursive_validator() {
    let mut e = prepare("fn demo()->bool{return true;}");
    let mir::Terminator::Return { value } =
        e.mir.test_functions_mut()[0].test_blocks_mut()[0].test_terminator_mut()
    else {
        panic!()
    };
    for _ in 0..40 {
        value.kind = hir::ExprKind::Unary {
            op: hir::UnaryOp::Not,
            operand: Box::new(value.clone()),
        };
    }
    assert!(matches!(
        e.validate(),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit("admission expression depth"),
            ..
        })
    ));
}
#[test]
fn flat_cfg_preparation_and_execution_use_iterative_reachability() {
    let s = format!(
        "fn demo()->bool{{{}return true;}}",
        "if true{let value=true;}".repeat(35)
    );
    let e = prepare_execution(&s, AdmissionLimits::MAX).unwrap();
    assert!(e.mir.functions()[0].blocks().len() > 60);
    assert_eq!(
        execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap(),
        ExecutionResult::Value(RuntimeValue::Bool(true))
    );
}
#[test]
fn recursive_large_local_tables_hit_storage_before_growth() {
    let locals = (0..20).fold(String::new(), |mut s, i| {
        use std::fmt::Write;
        write!(&mut s, "let value{i}=true;").unwrap();
        s
    });
    let e = prepare(&format!(
        "fn recurse()->bool{{{locals}return recurse();}}fn demo()->bool{{return recurse();}}"
    ));
    let error = execute(
        &e,
        e.entry("demo").unwrap(),
        ExecutionLimits {
            working_storage: 4096,
            ..ExecutionLimits::default()
        },
    )
    .unwrap_err();
    assert!(matches!(
        error.kind,
        ErrorKind::ConfiguredLimit("working storage")
    ));
    assert!(!error.trace.is_empty());
}
#[test]
fn wide_pending_records_and_arguments_are_storage_bounded() {
    let fields = (0..20)
        .map(|i| format!("f{i}:P"))
        .collect::<Vec<_>>()
        .join(",");
    let init = (0..20)
        .map(|i| format!("f{i}:80"))
        .collect::<Vec<_>>()
        .join(",");
    let e = prepare_execution(
        &format!("type P=range 0..100;record R{{{fields}}}fn demo()->R{{return R{{{init}}};}}"),
        AdmissionLimits::MAX,
    )
    .unwrap();
    assert!(matches!(
        execute(
            &e,
            e.entry("demo").unwrap(),
            ExecutionLimits {
                working_storage: 1024,
                ..ExecutionLimits::default()
            }
        ),
        Err(ExecutionError {
            kind: ErrorKind::ConfiguredLimit("working storage"),
            ..
        })
    ));
}
#[test]
fn structural_origin_kinds_include_all_synthetic_transfers() {
    let e=prepare("fn demo(){let value=if true{true}else{false};while value{if false{continue;}else{break;}}return;}");
    let origins = &e.origins[0];
    assert!(origins
        .iter()
        .any(|o| o.kind == mir::OriginKind::Initializer));
    assert!(origins.iter().any(|o| o.kind == mir::OriginKind::While));
    assert!(origins.iter().any(|o| o.kind == mir::OriginKind::If));
    assert!(origins.iter().any(|o| o.kind == mir::OriginKind::Statement));
    for o in origins {
        assert!(o.span.line > 0);
        assert!(o.span.end > o.span.start);
    }
}
#[test]
fn empty_infinite_loop_exhausts_terminator_fuel() {
    let e = prepare("fn demo(){while true{}}");
    let error = execute(
        &e,
        e.entry("demo").unwrap(),
        ExecutionLimits {
            steps: 30,
            ..ExecutionLimits::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error.kind, ErrorKind::ConfiguredLimit("steps")));
    assert!(error.origin.end > error.origin.start);
}
#[test]
fn function_entry_fuel_precedes_call_depth() {
    let e = prepare("fn callee()->bool{return true;}fn demo()->bool{return callee();}");
    let id = e.entry("demo").unwrap();
    for (steps, want) in [(3, "steps"), (4, "call depth")] {
        assert!(
            matches!(execute(&e,id,ExecutionLimits{steps,call_depth:1,..ExecutionLimits::default()}),Err(ExecutionError{kind:ErrorKind::ConfiguredLimit(s),..})if s==want)
        );
    }
}
