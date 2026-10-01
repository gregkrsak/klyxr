use crate::lexer::Span;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Program {
    pub ranges: Vec<RangeType>,
    pub records: Vec<RecordDef>,
    pub functions: Vec<VerifiedFunction>,
    pub bindings: Vec<RecordBinding>,
    pub calls: Vec<Call>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeType {
    pub name: String,
    pub min: i64,
    pub max: i64,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDef {
    pub name: String,
    pub field_name: String,
    pub field_type: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFunction {
    pub name: String,
    pub state_param: String,
    pub state_type: String,
    pub amount_param: String,
    pub amount_type: String,
    pub required_field: String,
    pub requires_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordBinding {
    pub name: String,
    pub record_type: String,
    pub field_name: String,
    pub value: i64,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub function: String,
    pub binding: String,
    pub amount: i64,
    pub span: Span,
}
