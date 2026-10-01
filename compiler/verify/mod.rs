use std::collections::{BTreeMap, HashSet};

use crate::ast::{
    FieldAccess, Operand, Program, RangeType, RecordBinding, Statement, VerifiedFunction,
};
use crate::diagnostics::Diagnostic;
use crate::lexer::Span;

#[derive(Debug, Default)]
pub struct VerificationReport {
    pub diagnostics: Vec<Diagnostic>,
    pub functions_proven: usize,
    pub calls_checked: usize,
    pub final_values: BTreeMap<String, i64>,
}

type VerificationResult<T = ()> = Result<T, Box<Diagnostic>>;

/// Convenience API for callers that only need diagnostics.
pub fn verify(program: &Program) -> Vec<Diagnostic> {
    verify_report(program).diagnostics
}

/// Establish only the documented straight-line integer prototype obligations.
/// Declaration errors stop verification before executable statements run.
pub fn verify_report(program: &Program) -> VerificationReport {
    let mut report = VerificationReport::default();
    let mut type_names = HashSet::new();
    let mut function_names = HashSet::new();
    for range in &program.ranges {
        if !type_names.insert(&range.name) {
            report.diagnostics.push(*duplicate(range.span, &range.name));
        }
        if range.min > range.max {
            report.diagnostics.push(Diagnostic::semantic(
                range.span,
                "invalid range declaration",
                format!(
                    "lower bound {} must not exceed upper bound {}",
                    range.min, range.max
                ),
            ));
        }
    }
    for record in &program.records {
        if !type_names.insert(&record.name) {
            report
                .diagnostics
                .push(*duplicate(record.span, &record.name));
        }
        if let Err(error) = find_range(program, &record.field_type, record.span) {
            report.diagnostics.push(*error);
        }
    }
    for function in &program.functions {
        if !function_names.insert(&function.name) {
            report
                .diagnostics
                .push(*duplicate(function.span, &function.name));
        }
        if let Err(error) = validate_function(program, function) {
            report.diagnostics.push(*error);
        }
    }
    if !report.diagnostics.is_empty() {
        return report;
    }

    // Prove every declared body, even when it has no call sites.
    for function in &program.functions {
        match prove_function(program, function) {
            Ok(()) => report.functions_proven += 1,
            Err(error) => report.diagnostics.push(*error),
        }
    }
    if !report.diagnostics.is_empty() {
        return report;
    }

    let mut bindings = BTreeMap::<String, &RecordBinding>::new();
    for statement in &program.statements {
        let result = match statement {
            Statement::Binding(binding) => {
                validate_binding(program, binding, &bindings).map(|()| {
                    bindings.insert(binding.name.clone(), binding);
                    report
                        .final_values
                        .insert(binding.name.clone(), binding.value);
                })
            }
            Statement::Call(call) => check_call(program, call, &bindings, &mut report.final_values)
                .map(|()| {
                    report.calls_checked += 1;
                }),
        };
        if let Err(error) = result {
            report.diagnostics.push(*error);
            // Do not reason about later calls with stale state after a failure.
            break;
        }
    }
    report
}

fn semantic_error(
    span: Span,
    message: impl Into<String>,
    detail: impl Into<String>,
) -> Box<Diagnostic> {
    Diagnostic::semantic(span, message, detail).into()
}

fn duplicate(span: Span, name: &str) -> Box<Diagnostic> {
    semantic_error(
        span,
        format!("duplicate declaration `{name}`"),
        "names must be unique in this prototype scope",
    )
}

fn find_range<'a>(
    program: &'a Program,
    name: &str,
    span: Span,
) -> VerificationResult<&'a RangeType> {
    program
        .ranges
        .iter()
        .find(|range| range.name == name)
        .ok_or_else(|| {
            semantic_error(
                span,
                format!("unknown range type `{name}`"),
                "declare a signed i64 range type",
            )
        })
}

fn record_range<'a>(
    program: &'a Program,
    name: &str,
    span: Span,
) -> VerificationResult<(&'a str, &'a RangeType)> {
    let record = program
        .records
        .iter()
        .find(|record| record.name == name)
        .ok_or_else(|| {
            semantic_error(
                span,
                format!("unknown record type `{name}`"),
                "declare a one-field record",
            )
        })?;
    Ok((
        &record.field_name,
        find_range(program, &record.field_type, span)?,
    ))
}

fn validate_access(
    access: &FieldAccess,
    function: &VerifiedFunction,
    field: &str,
) -> VerificationResult {
    if access.binding != function.state_param || access.field != field {
        return Err(semantic_error(
            access.span,
            format!(
                "invalid field reference `{}.{}`",
                access.binding, access.field
            ),
            format!(
                "the supported state field is `{}.{field}`",
                function.state_param
            ),
        ));
    }
    Ok(())
}

fn validate_function(program: &Program, function: &VerifiedFunction) -> VerificationResult {
    if function.state_param == function.amount_param {
        return Err(duplicate(function.span, &function.state_param));
    }
    let (field, state_range) = record_range(program, &function.state_type, function.span)?;
    let amount_range = find_range(program, &function.amount_type, function.span)?;
    if function.required_field != field {
        return Err(semantic_error(
            function.requires_span,
            format!("unknown precondition field `{}`", function.required_field),
            format!("record `{}` has field `{field}`", function.state_type),
        ));
    }
    validate_access(&function.postcondition.target, function, field)?;
    validate_access(&function.postcondition.old, function, field)?;
    if function.postcondition.amount != function.amount_param {
        return Err(semantic_error(
            function.postcondition.span,
            format!(
                "unknown postcondition parameter `{}`",
                function.postcondition.amount
            ),
            format!(
                "the supported amount parameter is `{}`",
                function.amount_param
            ),
        ));
    }
    for statement in &function.body {
        validate_access(&statement.target, function, field)?;
        if let Operand::Parameter(name) = &statement.operand {
            if name != &function.amount_param {
                return Err(semantic_error(
                    statement.span,
                    format!("unknown subtraction parameter `{name}`"),
                    format!(
                        "use `{}` or a signed integer literal",
                        function.amount_param
                    ),
                ));
            }
        }
    }
    // The mandatory postcondition also subtracts amount, even for an empty or
    // literal-only body. Reject nominal mixing before any numerical proof.
    if state_range.name != amount_range.name {
        return Err(semantic_error(
            function.postcondition.span,
            format!(
                "incompatible named range types for subtraction: `{}` and `{}`",
                state_range.name, amount_range.name
            ),
            format!(
                "state field `{}.{field}` has type `{}`; parameter `{}` has type `{}`. \
                 This prototype does not permit implicit arithmetic between distinct named range types; \
                 both must use the same declared range type",
                function.state_param, state_range.name, function.amount_param, amount_range.name
            ),
        ));
    }
    Ok(())
}

/// The input domain is a rectangle clipped by amount <= state. Its vertices
/// are rectangle corners and intersections of amount == state with its edges.
/// All supported expressions are affine. An affine bound/equality holds over
/// the entire polygon iff it holds at every vertex. These integral vertices
/// establish the obligations for ALL admissible integer inputs, without
/// enumerating a range or sampling a few calls.
fn input_vertices(state: &RangeType, amount: &RangeType) -> Vec<(i128, i128)> {
    let mut points = Vec::new();
    for x in [state.min, state.max] {
        for y in [amount.min, amount.max] {
            if y <= x {
                points.push((x as i128, y as i128));
            }
        }
    }
    for value in [state.min, state.max, amount.min, amount.max] {
        if value >= state.min && value <= state.max && value >= amount.min && value <= amount.max {
            points.push((value as i128, value as i128));
        }
    }
    points.sort_unstable();
    points.dedup();
    points
}

fn prove_function(program: &Program, function: &VerifiedFunction) -> VerificationResult {
    let (_, state) = record_range(program, &function.state_type, function.span)?;
    let amount = find_range(program, &function.amount_type, function.span)?;
    prove_affine_function(function, state, amount)
}

/// Private numerical kernel. Source-language nominal compatibility is checked
/// before this is reached; unit tests can vary the numeric domains independently.
fn prove_affine_function(
    function: &VerifiedFunction,
    state: &RangeType,
    amount: &RangeType,
) -> VerificationResult {
    let vertices = input_vertices(state, amount);
    if vertices.is_empty() {
        return Err(semantic_error(
            function.requires_span,
            "precondition has no admissible inputs",
            "this prototype requires a nonempty input domain rather than reporting a vacuous proof",
        ));
    }
    let mut current: Vec<i128> = vertices.iter().map(|(x, _)| *x).collect();
    for statement in &function.body {
        for (index, &(initial, input_amount)) in vertices.iter().enumerate() {
            let operand = match &statement.operand {
                Operand::Parameter(_) => input_amount,
                Operand::Literal(value) => *value as i128,
            };
            // Both operands fit i64; i128 subtraction cannot overflow.
            let next = current[index] - operand;
            if next < i64::MIN as i128 || next > i64::MAX as i128 {
                return Err(proof_failure(
                    function,
                    statement.span,
                    "subtraction overflow cannot be excluded",
                    initial,
                    input_amount,
                    next,
                ));
            }
            if next < state.min as i128 || next > state.max as i128 {
                return Err(proof_failure(
                    function,
                    statement.span,
                    "field range preservation cannot be established",
                    initial,
                    input_amount,
                    next,
                ));
            }
            current[index] = next;
        }
    }
    for (index, &(initial, input_amount)) in vertices.iter().enumerate() {
        if current[index] != initial - input_amount {
            return Err(proof_failure(
                function,
                function.postcondition.span,
                "postcondition cannot be established",
                initial,
                input_amount,
                current[index],
            ));
        }
    }
    Ok(())
}

fn proof_failure(
    function: &VerifiedFunction,
    span: Span,
    message: &str,
    state: i128,
    amount: i128,
    actual: i128,
) -> Box<Diagnostic> {
    Diagnostic {
        message: message.into(), span,
        required: format!("`{}` must preserve its field range and establish its postcondition for every input satisfying requires", function.name),
        known: vec![
            format!("old({}.{}) == {state}", function.state_param, function.required_field),
            format!("{} == {amount}", function.amount_param),
            format!("{amount} <= {state} is true"),
            format!("computed field value == {actual}"),
        ],
        conclusion: "admissible counterexample; this function has not been verified".into(),
    }.into()
}

fn validate_binding(
    program: &Program,
    binding: &RecordBinding,
    bindings: &BTreeMap<String, &RecordBinding>,
) -> VerificationResult {
    if bindings.contains_key(&binding.name) {
        return Err(duplicate(binding.span, &binding.name));
    }
    let (field, range) = record_range(program, &binding.record_type, binding.span)?;
    if binding.field_name != field {
        return Err(semantic_error(
            binding.span,
            format!("unknown initializer field `{}`", binding.field_name),
            format!("record `{}` requires field `{field}`", binding.record_type),
        ));
    }
    if binding.value < range.min || binding.value > range.max {
        return Err(Diagnostic {
            message: "range constraint cannot be established".into(),
            span: binding.span,
            required: format!(
                "{} <= {}.{} <= {}",
                range.min, binding.name, field, range.max
            ),
            known: vec![format!("{}.{} == {}", binding.name, field, binding.value)],
            conclusion: format!("{} is outside {}..{}", binding.value, range.min, range.max),
        }
        .into());
    }
    Ok(())
}

fn check_call(
    program: &Program,
    call: &crate::ast::Call,
    bindings: &BTreeMap<String, &RecordBinding>,
    values: &mut BTreeMap<String, i64>,
) -> VerificationResult {
    let function = program
        .functions
        .iter()
        .find(|function| function.name == call.function)
        .ok_or_else(|| {
            semantic_error(
                call.span,
                format!("unknown function `{}`", call.function),
                "declare a supported verified function",
            )
        })?;
    let binding = bindings.get(&call.binding).ok_or_else(|| {
        semantic_error(
            call.span,
            format!("unknown binding `{}`", call.binding),
            "construct the record before this call",
        )
    })?;
    if binding.record_type != function.state_type {
        return Err(semantic_error(
            call.span,
            "record argument type mismatch",
            format!(
                "expected `{}`, found `{}`; record types are distinct",
                function.state_type, binding.record_type
            ),
        ));
    }
    if !binding.mutable {
        return Err(semantic_error(
            call.span,
            "cannot mutably borrow immutable binding",
            format!("declare `{}` with `let mut` or `let mutable`", binding.name),
        ));
    }
    let amount_range = find_range(program, &function.amount_type, call.span)?;
    if call.amount < amount_range.min || call.amount > amount_range.max {
        return Err(Diagnostic {
            message: "argument violates range constraint".into(),
            span: call.span,
            required: format!(
                "{} <= {} <= {}",
                amount_range.min, function.amount_param, amount_range.max
            ),
            known: vec![format!("{} == {}", function.amount_param, call.amount)],
            conclusion: format!(
                "{} is outside {}..{}",
                call.amount, amount_range.min, amount_range.max
            ),
        }
        .into());
    }
    let initial = values[&call.binding];
    if call.amount > initial {
        return Err(Diagnostic {
            message: "precondition cannot be established".into(),
            span: call.span,
            required: format!(
                "{} <= {}.{}",
                function.amount_param, binding.name, function.required_field
            ),
            known: vec![
                format!("{} == {}", function.amount_param, call.amount),
                format!("{}.{} == {initial}", binding.name, function.required_field),
            ],
            conclusion: format!("{} <= {initial} is false", call.amount),
        }
        .into());
    }
    // Modular reasoning uses the postcondition only AFTER the body proof.
    let next = initial.checked_sub(call.amount).ok_or_else(|| {
        semantic_error(
            call.span,
            "subtraction overflow",
            "the checked subtraction must fit signed i64",
        )
    })?;
    values.insert(call.binding.clone(), next);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent finite model checks the universal proof over many small domains.
    /// It enumerates every admissible input and every intermediate assignment,
    /// rather than reconstructing the verifier's polygon/vertex implementation.
    #[test]
    fn affine_proofs_match_exhaustive_execution_on_small_domains() {
        let prototype =
            crate::compile_source(include_str!("../../examples/battery_ok.klx")).unwrap();
        let template = &prototype.functions[0];
        let mut combinations = 0;
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
                            let state = RangeType {
                                name: "State".into(),
                                min: low,
                                max: high,
                                span: template.span,
                            };
                            let amount = RangeType {
                                name: "Amount".into(),
                                min: amount_low,
                                max: amount_high,
                                span: template.span,
                            };
                            let mut function = template.clone();
                            function.body = body
                                .iter()
                                .map(|operand| {
                                    let mut statement = template.body[0].clone();
                                    statement.operand = match operand {
                                        Some(value) => Operand::Literal(*value),
                                        None => Operand::Parameter(function.amount_param.clone()),
                                    };
                                    statement
                                })
                                .collect();
                            let case = format!("state={low}..{high}, amount={amount_low}..{amount_high}, body={body:?}");
                            assert_eq!(
                                prove_affine_function(&function, &state, &amount).is_ok(),
                                has_inputs && valid,
                                "{case}"
                            );
                            combinations += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(combinations, 1_800);
    }

    #[test]
    fn numerical_kernel_preserves_extreme_and_empty_domain_coverage() {
        let program = crate::compile_source(include_str!("../../examples/battery_ok.klx")).unwrap();
        let function = &program.functions[0];
        let full = RangeType {
            name: "Full".into(),
            min: i64::MIN,
            max: i64::MAX,
            span: function.span,
        };
        let zero = RangeType {
            name: "Zero".into(),
            min: 0,
            max: 0,
            span: function.span,
        };
        assert!(prove_affine_function(function, &full, &zero).is_ok());
        let small = RangeType {
            name: "Small".into(),
            min: 0,
            max: 100,
            span: function.span,
        };
        let too_much = RangeType {
            name: "TooMuch".into(),
            min: 101,
            max: 200,
            span: function.span,
        };
        let error = prove_affine_function(function, &small, &too_much).unwrap_err();
        assert_eq!(error.message, "precondition has no admissible inputs");
    }
}
