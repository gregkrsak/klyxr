use klyxr_compiler::{
    compile_source,
    effects::{Effect, EffectSet},
    execution::{self, AdmissionLimits, ExecutionLimits, ExecutionResult},
    lexer::{lex, TokenKind},
    mir, parse_source, FrontendError,
};

#[test]
fn ordinary_result_kinds_and_keyword_spellings_preserve_contracts() {
    for keyword in ["fn", "function"] {
        for (result, body) in [("", ""), (" -> bool", "return true;")] {
            let no_value = result.is_empty();
            let source = format!("{keyword} demo(){result} effects clock, io {{ {body} }}");
            let p = compile_source(&source).unwrap();
            let f = p.functions()[0].as_ordinary().unwrap();
            assert_eq!(f.effects.set().to_string(), "io, clock");
            assert!(f.effects.origin(Effect::Clock).is_some());
            assert!(f.effects.origin(Effect::Io).is_some());
            assert!(f.effects.origin(Effect::Unsafe).is_none());
            let m = mir::lower(&p);
            assert_eq!(m.functions()[0].effects(), &f.effects);
            m.functions()[0].validate().unwrap();
            let e = execution::prepare_execution(&source, AdmissionLimits::default()).unwrap();
            let result =
                execution::execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
            if no_value {
                assert_eq!(result, ExecutionResult::NoValue);
            } else {
                assert_eq!(
                    result,
                    ExecutionResult::Value(execution::RuntimeValue::Bool(true))
                );
            }
        }
    }
}

#[test]
fn all_new_spellings_remain_identifiers_outside_the_clause() {
    for name in [
        "effects", "io", "heap", "blocking", "network", "filesystem", "clock", "random", "unsafe",
    ] {
        assert!(matches!(&lex(name).unwrap()[0].kind, TokenKind::Ident(n) if n == name));
        let source = format!("fn {name}({name}: bool) -> bool {{ return {name}; }} fn demo() -> bool {{ return {name}(true); }}");
        let p = compile_source(&source).unwrap();
        assert!(p
            .functions()
            .iter()
            .all(|f| f.as_ordinary().unwrap().effects.set() == EffectSet::EMPTY));
    }
    compile_source("type N=range 0..1; record effects{x:N} fn f()->effects effects clock {return effects{x:0};}").unwrap();
}

#[test]
fn absent_clause_is_empty_and_declarations_collect_before_bodies() {
    let p = compile_source("fn first() effects io {second();} function second() effects io {first();} fn plain() {}").unwrap();
    assert_eq!(
        p.functions()[0]
            .as_ordinary()
            .unwrap()
            .effects
            .set()
            .to_string(),
        "io"
    );
    assert_eq!(
        p.functions()[1]
            .as_ordinary()
            .unwrap()
            .effects
            .set()
            .to_string(),
        "io"
    );
    let plain = p.functions()[2].as_ordinary().unwrap();
    assert_eq!(plain.effects.set(), EffectSet::EMPTY);
    assert_eq!(plain.effects.clause(), None);
    compile_source("fn self_call() effects random {self_call();}").unwrap();
}

#[test]
fn unknown_duplicate_and_malformed_clauses_reject() {
    for (names, expected) in [
        ("io, io", "duplicate"),
        ("mystery", "unknown"),
        ("IO", "unknown"),
    ] {
        let error = compile_source(&format!("fn f() effects {names} {{}}")).unwrap_err();
        assert!(matches!(error, FrontendError::Resolve(_)));
        assert!(error.to_string().contains(expected));
    }
    for source in [
        "fn f() effects {}",
        "fn f() effects io, {}",
        "fn f() effects io effects heap {}",
        "fn f() effects io -> bool {return true;}",
        "fn f() effects io,heap,blocking,network,filesystem,clock,random,unsafe,io {}",
        "fn f(){ effects io; }",
    ] {
        assert!(
            matches!(parse_source(source), Err(FrontendError::Parse(_))),
            "{source}"
        );
    }
}

#[test]
fn verified_and_harness_effect_clauses_remain_excluded() {
    let source = "type N=range 0..100; record Cell{x:N} verified fn consume(c:&mut Cell,n:N) requires n<=c.x ensures c.x==old(c.x)-n {c.x-=n;} let mut cell=Cell{x:80}; consume(&mut cell,50);";
    let p = compile_source(source).unwrap();
    let control = klyxr_compiler::verify::verify_report(&p);
    assert!(control.diagnostics.is_empty());
    assert_eq!(control.functions_proven, 1);
    for malformed in [
        source.replace(") requires", ") effects io requires"),
        source.replace(" requires", " effects unsafe requires"),
        source.replace("let mut cell=", "let mut cell effects io="),
        source.replace("consume(&mut cell,50);", "consume(&mut cell,50) effects io;"),
    ] {
        assert!(matches!(
            parse_source(&malformed),
            Err(FrontendError::Parse(_))
        ));
    }
}
