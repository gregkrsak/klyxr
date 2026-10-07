use crate::lexer::Span;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Program {
    pub ranges: Vec<RangeType>,
    pub records: Vec<RecordDef>,
    pub functions: Vec<FunctionDecl>,
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

/// Both function forms share source declaration order and a name namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionDecl {
    Ordinary(ValueFunction),
    Verified(VerifiedFunction),
}
impl FunctionDecl {
    pub fn name(&self) -> &str {
        match self {
            Self::Ordinary(f) => &f.name,
            Self::Verified(f) => &f.name,
        }
    }
    pub fn span(&self) -> Span {
        match self {
            Self::Ordinary(f) => f.span,
            Self::Verified(f) => f.span,
        }
    }
    pub fn as_verified(&self) -> Option<&VerifiedFunction> {
        match self {
            Self::Verified(f) => Some(f),
            Self::Ordinary(_) => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueFunction {
    pub name: String,
    pub parameters: Vec<ValueParameter>,
    pub return_type: ValueType,
    pub body: Vec<ValueStatement>,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueParameter {
    pub name: String,
    pub ty: ValueType,
    pub span: Span,
}
/// Flat source reference prefixes preserve even unsupported nested signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueType {
    pub name: String,
    pub references: Vec<BorrowKind>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    Shared,
    Mutable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueStatement {
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    While {
        condition: Expr,
        body: Vec<ValueStatement>,
        span: Span,
    },
    Assign {
        target: String,
        value: Expr,
        span: Span,
    },
    If {
        condition: Expr,
        then_body: Vec<ValueStatement>,
        else_body: Option<Vec<ValueStatement>>,
        span: Span,
    },
    DerefAssign {
        reference: String,
        value: Expr,
        span: Span,
    },
    Let {
        name: String,
        mutable: bool,
        initializer: Expr,
        span: Span,
    },
    Return {
        value: Expr,
        span: Span,
    },
}

impl ValueStatement {
    // Narrow structural fallthrough only: a while always retains its false exit.
    pub(crate) fn falls_through(&self) -> bool {
        match self {
            Self::Continue { .. } | Self::Break { .. } => false,
            Self::If {
                then_body,
                else_body,
                ..
            } => {
                then_body.last().map_or(true, Self::falls_through)
                    || else_body
                        .as_ref()
                        .and_then(|b| b.last())
                        .map_or(true, Self::falls_through)
            }
            _ => true,
        }
    }
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
    IfValue {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    BoolLiteral(bool),
    IntegerLiteral(i64),
    Name(String),
    Deref {
        reference: String,
    },
    Borrow {
        kind: BorrowKind,
        target: String,
    },
    Call {
        callee: String,
        arguments: Vec<Expr>,
    },
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
