//! Resolved, typed representation of the one-field integer prototype only.
//! IDs index declaration tables within one Program; names are diagnostic metadata.
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
    pub ranges: Vec<RangeType>,
    pub records: Vec<Record>,
    pub fields: Vec<Field>,
    pub functions: Vec<VerifiedFunction>,
    pub parameters: Vec<Parameter>,
    pub bindings: Vec<RecordBinding>,
    pub statements: Vec<Statement>,
}

impl Program {
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
