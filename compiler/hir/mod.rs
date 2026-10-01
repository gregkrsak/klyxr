//! Resolved, typed representation of the one-field integer prototype only.
//! IDs are compilation-local indices into the canonical tables owned by Program.
//! Public inspection is read-only; compiler-internal mutation must preserve identity
//! and reference invariants. Names and spans are diagnostic metadata.
use crate::lexer::Span;

macro_rules! id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub(crate) usize);
    };
}
id!(RangeTypeId);
id!(RecordId);
id!(FieldId);
id!(FunctionId);
id!(ParameterId);
id!(BindingId);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Program {
    pub(crate) ranges: Vec<RangeType>,
    pub(crate) records: Vec<Record>,
    pub(crate) fields: Vec<Field>,
    pub(crate) functions: Vec<VerifiedFunction>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) bindings: Vec<RecordBinding>,
    pub(crate) statements: Vec<Statement>,
}

impl Program {
    pub fn ranges(&self) -> &[RangeType] {
        &self.ranges
    }
    pub fn records(&self) -> &[Record] {
        &self.records
    }
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
    pub fn functions(&self) -> &[VerifiedFunction] {
        &self.functions
    }
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }
    pub fn bindings(&self) -> &[RecordBinding] {
        &self.bindings
    }
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }

    pub fn range(&self, id: RangeTypeId) -> &RangeType {
        &self.ranges[id.0]
    }
    pub fn record(&self, id: RecordId) -> &Record {
        &self.records[id.0]
    }
    pub fn field(&self, id: FieldId) -> &Field {
        &self.fields[id.0]
    }
    pub fn function(&self, id: FunctionId) -> &VerifiedFunction {
        &self.functions[id.0]
    }
    pub fn parameter(&self, id: ParameterId) -> &Parameter {
        &self.parameters[id.0]
    }
    pub fn binding(&self, id: BindingId) -> &RecordBinding {
        &self.bindings[id.0]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeType {
    pub id: RangeTypeId,
    pub name: String,
    pub min: i64,
    pub max: i64,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: RecordId,
    pub name: String,
    pub field: FieldId,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub id: FieldId,
    pub record: RecordId,
    pub name: String,
    pub ty: RangeTypeId,
    pub span: Span,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterType {
    MutableRecord(RecordId),
    Range(RangeTypeId),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub id: ParameterId,
    pub function: FunctionId,
    pub name: String,
    pub ty: ParameterType,
    pub span: Span,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeParameter {
    pub parameter: ParameterId,
    pub ty: RangeTypeId,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAccess {
    pub parameter: ParameterId,
    pub field: FieldId,
    pub ty: RangeTypeId,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Precondition {
    pub amount: RangeParameter,
    pub state: FieldAccess,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Postcondition {
    pub target: FieldAccess,
    pub old: FieldAccess,
    pub amount: RangeParameter,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Parameter(RangeParameter),
    Literal(i64),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtract {
    pub target: FieldAccess,
    pub operand: Operand,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFunction {
    pub id: FunctionId,
    pub name: String,
    pub span: Span,
    pub state_param: ParameterId,
    pub state_type: RecordId,
    pub amount_param: RangeParameter,
    pub precondition: Precondition,
    pub postcondition: Postcondition,
    pub body: Vec<Subtract>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordBinding {
    pub id: BindingId,
    pub name: String,
    pub mutable: bool,
    pub record_type: RecordId,
    pub field: FieldId,
    pub value: i64,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub function: FunctionId,
    pub binding: BindingId,
    pub amount: i64,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Binding(BindingId),
    Call(Call),
}

#[cfg(test)]
mod tests {
    use crate::{compile_source, verify::verify_report};

    const SOURCE: &str = "\
let mut first = Battery { charge: 80 };
consume(&mut first, 50);
let mut second = Other { charge: 70 };
drain(&mut second, 20);
record Battery { charge: Percent }
record Other { charge: Percent }
type Unused = range 0..100;
type Percent = range 0..100;
verified fn consume(battery: &mut Battery, amount: Percent)
 requires amount <= battery.charge
 ensures battery.charge == old(battery.charge) - amount
 { battery.charge -= amount; }
verified function drain(battery: &mutable Other, amount: Percent)
 requires amount <= battery.charge
 ensures battery.charge == old(battery.charge) - amount
 { battery.charge -= amount; }
";

    #[test]
    fn verifier_uses_identities_even_when_display_names_collide() {
        let mut program = compile_source(SOURCE).unwrap();
        // Metadata changes cannot change which entities the references denote.
        for range in &mut program.ranges {
            range.name = "display".into();
        }
        for record in &mut program.records {
            record.name = "display".into();
        }
        for field in &mut program.fields {
            field.name = "display".into();
        }
        for function in &mut program.functions {
            function.name = "display".into();
        }
        for parameter in &mut program.parameters {
            parameter.name = "display".into();
        }
        program.bindings[0].name = "renamed_first".into();
        program.bindings[1].name = "renamed_second".into();
        let report = verify_report(&program);
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.functions_proven, 2);
        assert_eq!(report.calls_checked, 2);
        assert_eq!(report.final_values["renamed_first"], 30);
        assert_eq!(report.final_values["renamed_second"], 50);

        let wrong_record = SOURCE.replace("drain(&mut second, 20)", "drain(&mut first, 20)");
        let mut program = compile_source(&wrong_record).unwrap();
        for record in &mut program.records {
            record.name = "display".into();
        }
        let report = verify_report(&program);
        assert_eq!(report.calls_checked, 1);
        assert_eq!(
            report.diagnostics[0].message,
            "record argument type mismatch"
        );
    }
}
