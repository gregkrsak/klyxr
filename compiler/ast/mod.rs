use crate::lexer::Span;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Program {
    pub ranges: Vec<RangeType>,
    pub records: Vec<RecordDef>,
    pub functions: Vec<VerifiedFunction>,
    pub statements: Vec<Statement>,
}

/// Executable prototype statements retain source order, including construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Binding(RecordBinding),
    Call(Call),
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
    pub span: Span,
    pub state_param: String,
    pub state_type: String,
    pub amount_param: String,
    pub amount_type: String,
    pub requires: Expr,
    pub ensures: Expr,
    pub body: Vec<Subtract>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldAccess {
    pub binding: String,
    pub field: String,
    pub span: Span,
}

/// Source expression tree; identifiers are deliberately unresolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    BoolLiteral(bool),
    IntegerLiteral(i64),
    Name(String),
    FieldAccess(FieldAccess),
    OldField(FieldAccess),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Subtract,
    LessEqual,
    Equal,
    And,
    Or,
}
impl BinaryOp {
    pub fn spelling(self) -> &'static str {
        match self {
            Self::Subtract => "-",
            Self::LessEqual => "<=",
            Self::Equal => "==",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtract {
    pub target: FieldAccess,
    pub operand: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordBinding {
    pub name: String,
    pub mutable: bool,
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
