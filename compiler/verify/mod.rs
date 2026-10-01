use crate::ast::Program;
use crate::diagnostics::Diagnostic;

pub fn verify(program: &Program) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for binding in &program.bindings {
        let Some(record) = program
            .records
            .iter()
            .find(|record| record.name == binding.record_type)
        else {
            continue;
        };

        let Some(range) = program
            .ranges
            .iter()
            .find(|range| range.name == record.field_type)
        else {
            continue;
        };

        if binding.field_name == record.field_name
            && (binding.value < range.min || binding.value > range.max)
        {
            diagnostics.push(Diagnostic {
                message: "range constraint cannot be established".into(),
                span: binding.span,
                required: format!(
                    "{} <= {}.{} <= {}",
                    range.min, binding.name, binding.field_name, range.max
                ),
                known: vec![format!(
                    "{}.{} == {}",
                    binding.name, binding.field_name, binding.value
                )],
                conclusion: format!(
                    "{} is outside {}..{}",
                    binding.value, range.min, range.max
                ),
            });
        }
    }

    for call in &program.calls {
        let Some(function) = program
            .functions
            .iter()
            .find(|function| function.name == call.function)
        else {
            continue;
        };

        let Some(binding) = program
            .bindings
            .iter()
            .find(|binding| binding.name == call.binding)
        else {
            continue;
        };

        let Some(record) = program
            .records
            .iter()
            .find(|record| record.name == binding.record_type)
        else {
            continue;
        };

        let Some(amount_range) = program
            .ranges
            .iter()
            .find(|range| range.name == function.amount_type)
        else {
            continue;
        };

        if call.amount < amount_range.min || call.amount > amount_range.max {
            diagnostics.push(Diagnostic {
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
            });
            continue;
        }

        if function.state_type != binding.record_type
            || function.required_field != record.field_name
        {
            continue;
        }

        if call.amount > binding.value {
            diagnostics.push(Diagnostic {
                message: "precondition cannot be established".into(),
                span: call.span,
                required: format!(
                    "{} <= {}.{}",
                    function.amount_param, binding.name, function.required_field
                ),
                known: vec![
                    format!("{} == {}", function.amount_param, call.amount),
                    format!(
                        "{}.{} == {}",
                        binding.name, function.required_field, binding.value
                    ),
                ],
                conclusion: format!("{} <= {} is false", call.amount, binding.value),
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_source;

    const PREFIX: &str = r#"
type Percent = range 0..100;

record Battery {
    charge: Percent
}

verified fn consume(
    battery: &mut Battery,
    amount: Percent
)
    requires amount <= battery.charge
    ensures battery.charge == old(battery.charge) - amount
{
    battery.charge -= amount;
}
"#;

    fn program_with_call(amount: i64) -> Program {
        compile_source(&format!(
            "{PREFIX}\nlet mut battery = Battery {{ charge: 80 }};\n\
             consume(&mut battery, {amount});\n"
        ))
        .unwrap()
    }

    #[test]
    fn rejects_unprovable_battery_precondition() {
        let program = program_with_call(90);
        let diagnostics = verify(&program);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "precondition cannot be established"
        );
        assert_eq!(diagnostics[0].conclusion, "90 <= 80 is false");
    }

    #[test]
    fn accepts_provable_battery_precondition() {
        let program = program_with_call(50);
        assert!(verify(&program).is_empty());
    }
}
