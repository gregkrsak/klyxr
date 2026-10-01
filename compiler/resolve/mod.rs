//! Source-name lookup and lowering for the supported prototype subset.
use std::collections::{HashMap, HashSet};

use crate::{ast, diagnostics::Diagnostic, hir, lexer::Span};
use hir::{BindingId, FieldId, FunctionId, ParameterId, RangeTypeId, RecordId};

type ResolutionResult<T> = Result<T, Box<Diagnostic>>;

fn error(span: Span, message: impl Into<String>, detail: impl Into<String>) -> Box<Diagnostic> {
    Diagnostic::semantic(span, message, detail).into()
}
fn duplicate(span: Span, name: &str) -> Box<Diagnostic> {
    error(
        span,
        format!("duplicate declaration `{name}`"),
        "names must be unique in this prototype scope",
    )
}

/// Resolved names before expression type checking. Canonical storage remains internal.
#[derive(Debug)]
pub struct ResolvedProgram {
    pub(crate) declarations: hir::Program,
    pub(crate) functions: Vec<ResolvedFunction>,
}
#[derive(Debug)]
pub(crate) struct ResolvedFunction {
    pub id: FunctionId,
    pub name: String,
    pub span: Span,
    pub state_param: ParameterId,
    pub state_type: RecordId,
    pub amount_param: hir::RangeParameter,
    pub requires: ResolvedExpr,
    pub ensures: ResolvedExpr,
    pub body: Vec<ResolvedSubtract>,
}
#[derive(Debug)]
pub(crate) struct ResolvedSubtract {
    pub target: hir::FieldAccess,
    pub operand: ResolvedExpr,
    pub span: Span,
}
#[derive(Debug)]
pub(crate) struct ResolvedExpr {
    pub kind: ResolvedExprKind,
    pub span: Span,
}
#[derive(Debug)]
pub(crate) enum ResolvedExprKind {
    BoolLiteral(bool),
    IntegerLiteral(i64),
    Parameter(ParameterId),
    FieldAccess(hir::FieldAccess),
    OldField(hir::FieldAccess),
    Unary {
        op: ast::UnaryOp,
        operand: Box<ResolvedExpr>,
    },
    Binary {
        op: ast::BinaryOp,
        left: Box<ResolvedExpr>,
        right: Box<ResolvedExpr>,
    },
}

/// Resolve complete declaration sets, then executable statements in source order.
/// No partially resolved program is returned on failure.
pub fn resolve(ast: &ast::Program) -> Result<ResolvedProgram, Vec<Diagnostic>> {
    let mut resolver = Resolver::default();
    let mut diagnostics = Vec::new();
    let mut types = HashSet::new();
    for range in &ast.ranges {
        if !types.insert(&range.name) {
            diagnostics.push(*duplicate(range.span, &range.name));
        }
        if range.min > range.max {
            diagnostics.push(Diagnostic::semantic(
                range.span,
                "invalid range declaration",
                format!(
                    "lower bound {} must not exceed upper bound {}",
                    range.min, range.max
                ),
            ));
        }
        let id = RangeTypeId(resolver.program.ranges.len());
        resolver.ranges.insert(range.name.clone(), id);
        resolver.program.ranges.push(hir::RangeType {
            id,
            name: range.name.clone(),
            min: range.min,
            max: range.max,
            span: range.span,
        });
    }
    for (index, record) in ast.records.iter().enumerate() {
        if !types.insert(&record.name) {
            diagnostics.push(*duplicate(record.span, &record.name));
        }
        resolver
            .records
            .insert(record.name.clone(), RecordId(index));
    }
    for (index, function) in ast.functions.iter().enumerate() {
        if resolver
            .functions
            .insert(function.name.clone(), FunctionId(index))
            .is_some()
        {
            diagnostics.push(*duplicate(function.span, &function.name));
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    for (index, record) in ast.records.iter().enumerate() {
        match resolver.range(&record.field_type, record.span) {
            Ok(ty) => {
                let id = RecordId(index);
                let field = FieldId(resolver.program.fields.len());
                resolver.program.fields.push(hir::Field {
                    id: field,
                    record: id,
                    name: record.field_name.clone(),
                    ty,
                    span: record.span,
                });
                resolver.program.records.push(hir::Record {
                    id,
                    name: record.name.clone(),
                    field,
                    span: record.span,
                });
            }
            Err(diagnostic) => diagnostics.push(*diagnostic),
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let mut functions = Vec::new();
    for (index, function) in ast.functions.iter().enumerate() {
        match resolver.function(FunctionId(index), function) {
            Ok(function) => functions.push(function),
            Err(diagnostic) => diagnostics.push(*diagnostic),
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    for statement in &ast.statements {
        match resolver.statement(statement) {
            Ok(statement) => resolver.program.statements.push(statement),
            Err(diagnostic) => return Err(vec![*diagnostic]),
        }
    }
    Ok(ResolvedProgram {
        declarations: resolver.program,
        functions,
    })
}

#[derive(Default)]
struct Resolver {
    program: hir::Program,
    ranges: HashMap<String, RangeTypeId>,
    records: HashMap<String, RecordId>,
    functions: HashMap<String, FunctionId>,
    bindings: HashMap<String, BindingId>,
}
impl Resolver {
    fn range(&self, name: &str, span: Span) -> ResolutionResult<RangeTypeId> {
        self.ranges.get(name).copied().ok_or_else(|| {
            error(
                span,
                format!("unknown range type `{name}`"),
                "declare a signed i64 range type",
            )
        })
    }
    fn record(&self, name: &str, span: Span) -> ResolutionResult<RecordId> {
        self.records.get(name).copied().ok_or_else(|| {
            error(
                span,
                format!("unknown record type `{name}`"),
                "declare a one-field record",
            )
        })
    }
    fn parameter(
        &mut self,
        function: FunctionId,
        name: &str,
        ty: hir::ParameterType,
        span: Span,
    ) -> ParameterId {
        let id = ParameterId(self.program.parameters.len());
        self.program.parameters.push(hir::Parameter {
            id,
            function,
            name: name.into(),
            ty,
            span,
        });
        id
    }
    fn access(
        &self,
        access: &ast::FieldAccess,
        function: &ast::VerifiedFunction,
        parameter: ParameterId,
        field: FieldId,
    ) -> ResolutionResult<hir::FieldAccess> {
        let declaration = self.program.field(field);
        if access.binding != function.state_param || access.field != declaration.name {
            return Err(error(
                access.span,
                format!(
                    "invalid field reference `{}.{}`",
                    access.binding, access.field
                ),
                format!(
                    "the supported state field is `{}.{}`",
                    function.state_param, declaration.name
                ),
            ));
        }
        Ok(hir::FieldAccess {
            parameter,
            field,
            ty: declaration.ty,
            span: access.span,
        })
    }
    fn function(
        &mut self,
        id: FunctionId,
        function: &ast::VerifiedFunction,
    ) -> ResolutionResult<ResolvedFunction> {
        if function.state_param == function.amount_param {
            return Err(duplicate(function.span, &function.state_param));
        }
        let state_type = self.record(&function.state_type, function.span)?;
        let amount_type = self.range(&function.amount_type, function.span)?;
        let field = self.program.record(state_type).field;
        let state_param = self.parameter(
            id,
            &function.state_param,
            hir::ParameterType::MutableRecord(state_type),
            function.span,
        );
        let amount_id = self.parameter(
            id,
            &function.amount_param,
            hir::ParameterType::Range(amount_type),
            function.span,
        );
        let amount_param = hir::RangeParameter {
            parameter: amount_id,
            ty: amount_type,
        };
        let requires = self.expression(
            &function.requires,
            function,
            state_param,
            amount_id,
            field,
            false,
        )?;
        let ensures = self.expression(
            &function.ensures,
            function,
            state_param,
            amount_id,
            field,
            true,
        )?;
        let mut body = Vec::new();
        for statement in &function.body {
            body.push(ResolvedSubtract {
                target: self.access(&statement.target, function, state_param, field)?,
                operand: self.expression(
                    &statement.operand,
                    function,
                    state_param,
                    amount_id,
                    field,
                    false,
                )?,
                span: statement.span,
            });
        }
        Ok(ResolvedFunction {
            id,
            name: function.name.clone(),
            span: function.span,
            state_param,
            state_type,
            amount_param,
            requires,
            ensures,
            body,
        })
    }
    fn expression(
        &self,
        expression: &ast::Expr,
        function: &ast::VerifiedFunction,
        state: ParameterId,
        amount: ParameterId,
        field: FieldId,
        allow_old: bool,
    ) -> ResolutionResult<ResolvedExpr> {
        let kind = match &expression.kind {
            ast::ExprKind::BoolLiteral(value) => ResolvedExprKind::BoolLiteral(*value),
            ast::ExprKind::IntegerLiteral(value) => ResolvedExprKind::IntegerLiteral(*value),
            ast::ExprKind::Name(name) => {
                let parameter = if name == &function.amount_param {
                    amount
                } else if name == &function.state_param {
                    state
                } else {
                    return Err(error(
                        expression.span,
                        format!("unknown parameter `{name}`"),
                        format!(
                            "function `{}` declares parameters `{}` and `{}`",
                            function.name, function.state_param, function.amount_param
                        ),
                    ));
                };
                ResolvedExprKind::Parameter(parameter)
            }
            ast::ExprKind::FieldAccess(access) => {
                ResolvedExprKind::FieldAccess(self.access(access, function, state, field)?)
            }
            ast::ExprKind::OldField(access) => {
                if !allow_old {
                    return Err(error(
                        expression.span,
                        "old(...) is permitted only in ensures",
                        "this prototype snapshots only the mutable state field at function entry",
                    ));
                }
                ResolvedExprKind::OldField(self.access(access, function, state, field)?)
            }
            ast::ExprKind::Unary { op, operand } => ResolvedExprKind::Unary {
                op: *op,
                operand: Box::new(
                    self.expression(operand, function, state, amount, field, allow_old)?,
                ),
            },
            ast::ExprKind::Binary { op, left, right } => ResolvedExprKind::Binary {
                op: *op,
                left: Box::new(self.expression(left, function, state, amount, field, allow_old)?),
                right: Box::new(self.expression(right, function, state, amount, field, allow_old)?),
            },
        };
        Ok(ResolvedExpr {
            kind,
            span: expression.span,
        })
    }
    fn statement(&mut self, statement: &ast::Statement) -> ResolutionResult<hir::Statement> {
        match statement {
            ast::Statement::Binding(binding) => {
                if self.bindings.contains_key(&binding.name) {
                    return Err(duplicate(binding.span, &binding.name));
                }
                let record_type = self.record(&binding.record_type, binding.span)?;
                let field = self.program.record(record_type).field;
                if binding.field_name != self.program.field(field).name {
                    return Err(error(
                        binding.span,
                        format!("unknown initializer field `{}`", binding.field_name),
                        format!(
                            "record `{}` requires field `{}`",
                            binding.record_type,
                            self.program.field(field).name
                        ),
                    ));
                }
                let id = BindingId(self.program.bindings.len());
                self.program.bindings.push(hir::RecordBinding {
                    id,
                    name: binding.name.clone(),
                    mutable: binding.mutable,
                    record_type,
                    field,
                    value: binding.value,
                    span: binding.span,
                });
                self.bindings.insert(binding.name.clone(), id);
                Ok(hir::Statement::Binding(id))
            }
            ast::Statement::Call(call) => {
                let function = self.functions.get(&call.function).copied().ok_or_else(|| {
                    error(
                        call.span,
                        format!("unknown function `{}`", call.function),
                        "declare a supported verified function",
                    )
                })?;
                let binding = self.bindings.get(&call.binding).copied().ok_or_else(|| {
                    error(
                        call.span,
                        format!("unknown binding `{}`", call.binding),
                        "construct the record before this call",
                    )
                })?;
                Ok(hir::Statement::Call(hir::Call {
                    function,
                    binding,
                    amount: call.amount,
                    span: call.span,
                }))
            }
        }
    }
}
