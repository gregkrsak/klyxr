//! Source-name lookup and lowering for the supported prototype subset.
use std::collections::{HashMap, HashSet};

use crate::{ast, diagnostics::Diagnostic, hir, lexer::Span};
use hir::{BindingId, FieldId, FunctionId, LocalId, ParameterId, RangeTypeId, RecordId};

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

/// Resolved names before expression/value-flow typing. No partial program is published.
#[derive(Debug)]
pub struct ResolvedProgram {
    pub(crate) declarations: hir::Program,
    pub(crate) parameters: Vec<ResolvedParameter>,
    pub(crate) functions: Vec<ResolvedFunction>,
}
/// Source type names are resolved here; the type layer decides legal value types.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ResolvedValueType {
    Bool,
    Range(RangeTypeId),
    Record(RecordId),
    SharedRef(hir::ReferentType),
    MutableRef(hir::ReferentType),
    NestedReference,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum ResolvedParameterType {
    MutableRecord(RecordId),
    Range(RangeTypeId),
    Value(ResolvedValueType),
}
#[derive(Debug)]
pub(crate) struct ResolvedParameter {
    pub id: ParameterId,
    pub function: FunctionId,
    pub name: String,
    pub ty: ResolvedParameterType,
    pub span: Span,
}
#[derive(Debug)]
pub(crate) enum ResolvedFunction {
    Ordinary(ResolvedValueFunction),
    Verified(ResolvedVerifiedFunction),
}
#[derive(Debug)]
pub(crate) struct ResolvedValueFunction {
    pub id: FunctionId,
    pub name: String,
    pub parameters: Vec<ParameterId>,
    pub return_type: ResolvedValueType,
    pub body: Vec<ResolvedValueStatement>,
    pub span: Span,
}
#[derive(Debug)]
pub(crate) enum ResolvedValueStatement {
    If {
        condition: ResolvedExpr,
        then_body: Vec<ResolvedValueStatement>,
        else_body: Vec<ResolvedValueStatement>,
        span: Span,
    },
    DerefAssign {
        reference: hir::Place,
        value: ResolvedExpr,
        span: Span,
    },
    Let {
        id: LocalId,
        function: FunctionId,
        name: String,
        mutable: bool,
        initializer: ResolvedExpr,
        span: Span,
    },
    Return {
        value: ResolvedExpr,
        span: Span,
    },
}
#[derive(Debug)]
pub(crate) struct ResolvedVerifiedFunction {
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
    IfValue {
        condition: Box<ResolvedExpr>,
        then_value: Box<ResolvedExpr>,
        else_value: Box<ResolvedExpr>,
    },
    BoolLiteral(bool),
    IntegerLiteral(i64),
    Parameter(ParameterId),
    Local(LocalId),
    Deref {
        reference: hir::Place,
    },
    Borrow {
        kind: ast::BorrowKind,
        place: hir::Place,
    },
    Call {
        function: FunctionId,
        arguments: Vec<ResolvedExpr>,
    },
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
/// Transient resolved signatures share the same FunctionId indices as the final table.
enum Signature {
    Ordinary {
        parameters: Vec<ParameterId>,
        return_type: ResolvedValueType,
    },
    Verified {
        state_param: ParameterId,
        state_type: RecordId,
        amount_param: hir::RangeParameter,
    },
}
#[derive(Clone, Copy)]
enum Reference {
    Parameter(ParameterId),
    Local(LocalId),
}
#[derive(Default, Clone)]
struct Scope {
    values: HashMap<String, Reference>,
    state: Option<(String, ParameterId, FieldId)>,
}

/// Collect declarations/signatures first, then resolve bodies and ordered harness statements.
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
            .insert(function.name().into(), FunctionId(index))
            .is_some()
        {
            diagnostics.push(*duplicate(function.span(), function.name()));
        }
        resolver
            .ordinary
            .push(matches!(function, ast::FunctionDecl::Ordinary(_)));
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    for (index, record) in ast.records.iter().enumerate() {
        let ty = resolver
            .range(&record.field_type, record.span)
            .map_err(|e| vec![*e])?;
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
    let mut signatures = Vec::new();
    for (index, function) in ast.functions.iter().enumerate() {
        let signature = resolver
            .signature(FunctionId(index), function)
            .map_err(|e| vec![*e])?;
        signatures.push(signature);
    }
    let mut functions = Vec::new();
    for (index, (function, signature)) in ast.functions.iter().zip(signatures).enumerate() {
        match resolver.function(FunctionId(index), function, signature) {
            Ok(function) => functions.push(function),
            Err(e) => diagnostics.push(*e),
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    for statement in &ast.statements {
        let statement = resolver.statement(statement).map_err(|e| vec![*e])?;
        resolver.program.statements.push(statement);
    }
    Ok(ResolvedProgram {
        declarations: resolver.program,
        parameters: resolver.parameters,
        functions,
    })
}

#[derive(Default)]
struct Resolver {
    program: hir::Program,
    parameters: Vec<ResolvedParameter>,
    local_count: usize,
    ranges: HashMap<String, RangeTypeId>,
    records: HashMap<String, RecordId>,
    functions: HashMap<String, FunctionId>,
    ordinary: Vec<bool>,
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
    fn value_type(&self, ty: &ast::ValueType, span: Span) -> ResolutionResult<ResolvedValueType> {
        let owned = self.owned_type(&ty.name, span)?;
        if ty.references.len() > 1 {
            return Ok(ResolvedValueType::NestedReference);
        }
        let Some(kind) = ty.references.first() else {
            return Ok(owned);
        };
        let referent = match owned {
            ResolvedValueType::Bool => hir::ReferentType::Bool,
            ResolvedValueType::Range(id) => hir::ReferentType::Range(id),
            ResolvedValueType::Record(id) => hir::ReferentType::Record(id),
            _ => unreachable!("owned name resolution produces a non-reference type"),
        };
        Ok(match kind {
            ast::BorrowKind::Shared => ResolvedValueType::SharedRef(referent),
            ast::BorrowKind::Mutable => ResolvedValueType::MutableRef(referent),
        })
    }
    fn owned_type(&self, name: &str, span: Span) -> ResolutionResult<ResolvedValueType> {
        if name == "bool" {
            return Ok(ResolvedValueType::Bool);
        }
        if let Some(id) = self.ranges.get(name) {
            return Ok(ResolvedValueType::Range(*id));
        }
        if let Some(id) = self.records.get(name) {
            return Ok(ResolvedValueType::Record(*id));
        }
        Err(error(
            span,
            format!("unknown value type `{name}`"),
            "use built-in bool, a declared named range, or an existing record type",
        ))
    }
    fn parameter(
        &mut self,
        function: FunctionId,
        name: &str,
        ty: ResolvedParameterType,
        span: Span,
    ) -> ParameterId {
        let id = ParameterId(self.parameters.len());
        self.parameters.push(ResolvedParameter {
            id,
            function,
            name: name.into(),
            ty,
            span,
        });
        id
    }
    fn signature(
        &mut self,
        id: FunctionId,
        function: &ast::FunctionDecl,
    ) -> ResolutionResult<Signature> {
        match function {
            ast::FunctionDecl::Verified(f) => {
                if f.state_param == f.amount_param {
                    return Err(duplicate(f.span, &f.state_param));
                }
                let state_type = self.record(&f.state_type, f.span)?;
                let amount_type = self.range(&f.amount_type, f.span)?;
                let state_param = self.parameter(
                    id,
                    &f.state_param,
                    ResolvedParameterType::MutableRecord(state_type),
                    f.span,
                );
                let parameter = self.parameter(
                    id,
                    &f.amount_param,
                    ResolvedParameterType::Range(amount_type),
                    f.span,
                );
                Ok(Signature::Verified {
                    state_param,
                    state_type,
                    amount_param: hir::RangeParameter {
                        parameter,
                        ty: amount_type,
                    },
                })
            }
            ast::FunctionDecl::Ordinary(f) => {
                let mut names = HashSet::new();
                let mut parameters = Vec::new();
                for p in &f.parameters {
                    if !names.insert(&p.name) {
                        return Err(duplicate(p.span, &p.name));
                    }
                    let ty = self.value_type(&p.ty, p.span)?;
                    parameters.push(self.parameter(
                        id,
                        &p.name,
                        ResolvedParameterType::Value(ty),
                        p.span,
                    ));
                }
                let return_type = self.value_type(&f.return_type, f.span)?;
                Ok(Signature::Ordinary {
                    parameters,
                    return_type,
                })
            }
        }
    }
    fn access(
        &self,
        access: &ast::FieldAccess,
        scope: &Scope,
    ) -> ResolutionResult<hir::FieldAccess> {
        let Some((state_name, parameter, field)) = &scope.state else {
            return Err(error(
                access.span,
                "field access is not supported in ordinary value functions",
                "ordinary record values are indivisible owned values; field reads are not supported",
            ));
        };
        let declaration = self.program.field(*field);
        if access.binding != *state_name || access.field != declaration.name {
            return Err(error(
                access.span,
                format!(
                    "invalid field reference `{}.{}`",
                    access.binding, access.field
                ),
                format!(
                    "the supported state field is `{}.{}`",
                    state_name, declaration.name
                ),
            ));
        }
        Ok(hir::FieldAccess {
            parameter: *parameter,
            field: *field,
            ty: declaration.ty,
            span: access.span,
        })
    }
    fn value_statements(
        &mut self,
        function: FunctionId,
        statements: &[ast::ValueStatement],
        scope: &mut Scope,
    ) -> ResolutionResult<Vec<ResolvedValueStatement>> {
        let mut body = Vec::new();
        for statement in statements {
            body.push(match statement {
                ast::ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    span,
                } => {
                    let condition = self.expression(condition, scope, false)?;
                    let then_body =
                        self.value_statements(function, then_body, &mut scope.clone())?;
                    let else_body = self.value_statements(
                        function,
                        else_body.as_deref().unwrap_or(&[]),
                        &mut scope.clone(),
                    )?;
                    ResolvedValueStatement::If {
                        condition,
                        then_body,
                        else_body,
                        span: *span,
                    }
                }
                ast::ValueStatement::Let {
                    name,
                    mutable,
                    initializer,
                    span,
                } => {
                    if scope.values.contains_key(name) {
                        return Err(duplicate(*span, name));
                    }
                    // Resolve first, then introduce the local: no self-reference.
                    let initializer = self.expression(initializer, scope, false)?;
                    let local = LocalId(self.local_count);
                    self.local_count += 1;
                    scope.values.insert(name.clone(), Reference::Local(local));
                    ResolvedValueStatement::Let {
                        id: local,
                        function,
                        name: name.clone(),
                        mutable: *mutable,
                        initializer,
                        span: *span,
                    }
                }
                ast::ValueStatement::DerefAssign {
                    reference,
                    value,
                    span,
                } => ResolvedValueStatement::DerefAssign {
                    reference: self.reference_place(reference, scope, *span)?,
                    value: self.expression(value, scope, false)?,
                    span: *span,
                },
                ast::ValueStatement::Return { value, span } => ResolvedValueStatement::Return {
                    value: self.expression(value, scope, false)?,
                    span: *span,
                },
            });
        }
        Ok(body)
    }
    fn function(
        &mut self,
        id: FunctionId,
        function: &ast::FunctionDecl,
        signature: Signature,
    ) -> ResolutionResult<ResolvedFunction> {
        match (function, signature) {
            (
                ast::FunctionDecl::Verified(f),
                Signature::Verified {
                    state_param,
                    state_type,
                    amount_param,
                },
            ) => {
                let mut scope = Scope {
                    state: Some((
                        f.state_param.clone(),
                        state_param,
                        self.program.record(state_type).field,
                    )),
                    ..Scope::default()
                };
                scope
                    .values
                    .insert(f.state_param.clone(), Reference::Parameter(state_param));
                scope.values.insert(
                    f.amount_param.clone(),
                    Reference::Parameter(amount_param.parameter),
                );
                let requires = self.expression(&f.requires, &scope, false)?;
                let ensures = self.expression(&f.ensures, &scope, true)?;
                let mut body = Vec::new();
                for s in &f.body {
                    body.push(ResolvedSubtract {
                        target: self.access(&s.target, &scope)?,
                        operand: self.expression(&s.operand, &scope, false)?,
                        span: s.span,
                    });
                }
                Ok(ResolvedFunction::Verified(ResolvedVerifiedFunction {
                    id,
                    name: f.name.clone(),
                    span: f.span,
                    state_param,
                    state_type,
                    amount_param,
                    requires,
                    ensures,
                    body,
                }))
            }
            (
                ast::FunctionDecl::Ordinary(f),
                Signature::Ordinary {
                    parameters,
                    return_type,
                },
            ) => {
                let mut scope = Scope::default();
                for (p, id) in f.parameters.iter().zip(&parameters) {
                    scope
                        .values
                        .insert(p.name.clone(), Reference::Parameter(*id));
                }
                let body = self.value_statements(id, &f.body, &mut scope)?;
                Ok(ResolvedFunction::Ordinary(ResolvedValueFunction {
                    id,
                    name: f.name.clone(),
                    parameters,
                    return_type,
                    body,
                    span: f.span,
                }))
            }
            _ => unreachable!("resolver signature and AST declaration kinds agree"),
        }
    }
    fn reference_place(
        &self,
        name: &str,
        scope: &Scope,
        span: Span,
    ) -> ResolutionResult<hir::Place> {
        if scope.state.is_some() {
            return Err(error(
                span,
                "dereference is supported only in ordinary functions",
                "verified state mutation retains its restricted prototype semantics",
            ));
        }
        match scope.values.get(name) {
            Some(Reference::Parameter(id)) => Ok(hir::Place::Parameter(*id)),
            Some(Reference::Local(id)) => Ok(hir::Place::Local(*id)),
            None => Err(error(
                span,
                format!("unknown value `{name}`"),
                "dereference an existing parameter or previously declared local in this function",
            )),
        }
    }
    fn expression(
        &self,
        expression: &ast::Expr,
        scope: &Scope,
        allow_old: bool,
    ) -> ResolutionResult<ResolvedExpr> {
        let kind = match &expression.kind {
            ast::ExprKind::IfValue { condition, then_value, else_value } => ResolvedExprKind::IfValue {
                condition: Box::new(self.expression(condition, scope, allow_old)?),
                then_value: Box::new(self.expression(then_value, scope, allow_old)?),
                else_value: Box::new(self.expression(else_value, scope, allow_old)?),
            },
            ast::ExprKind::BoolLiteral(v) => ResolvedExprKind::BoolLiteral(*v),
            ast::ExprKind::IntegerLiteral(v) => ResolvedExprKind::IntegerLiteral(*v),
            ast::ExprKind::Name(name) => match scope.values.get(name) {
                Some(Reference::Parameter(id)) => ResolvedExprKind::Parameter(*id),
                Some(Reference::Local(id)) => ResolvedExprKind::Local(*id),
                None => return Err(error(expression.span, format!("unknown {} `{name}`", if scope.state.is_some() { "parameter" } else { "value" }), "parameters are visible at entry; locals must be declared before use in this function")),
            },
            ast::ExprKind::Deref { reference } => ResolvedExprKind::Deref {
                reference: self.reference_place(reference, scope, expression.span)?,
            },
            ast::ExprKind::Borrow { kind, target } => {
                if scope.state.is_some() {
                    return Err(error(expression.span, "borrow expressions are supported only in ordinary functions", "the verified state-reference prototype remains separate"));
                }
                let place = match scope.values.get(target) {
                    Some(Reference::Parameter(id)) => hir::Place::Parameter(*id),
                    Some(Reference::Local(id)) => hir::Place::Local(*id),
                    None => return Err(error(expression.span, format!("unknown value `{target}`"), "borrow an existing parameter or previously declared local")),
                };
                ResolvedExprKind::Borrow { kind: *kind, place }
            }
            ast::ExprKind::FieldAccess(a) => ResolvedExprKind::FieldAccess(self.access(a, scope)?),
            ast::ExprKind::OldField(a) => {
                if !allow_old { return Err(error(expression.span, "old(...) is permitted only in ensures", "this prototype snapshots only the mutable state field at function entry")); }
                ResolvedExprKind::OldField(self.access(a, scope)?)
            }
            ast::ExprKind::Call { callee, arguments } => {
                let function = self.functions.get(callee).copied().ok_or_else(|| error(expression.span, format!("unknown function `{callee}`"), "declare an ordinary value function"))?;
                if scope.state.is_some() || !self.ordinary[function.0] {
                    return Err(error(expression.span, "call expressions are supported only between ordinary value functions", "verified contracts/body expressions cannot call functions; mixed-assurance call rules remain open"));
                }
                let arguments = arguments.iter().map(|a| self.expression(a, scope, false)).collect::<ResolutionResult<Vec<_>>>()?;
                ResolvedExprKind::Call { function, arguments }
            }
            ast::ExprKind::Unary { op, operand } => ResolvedExprKind::Unary { op: *op, operand: Box::new(self.expression(operand, scope, allow_old)?) },
            ast::ExprKind::Binary { op, left, right } => ResolvedExprKind::Binary { op: *op, left: Box::new(self.expression(left, scope, allow_old)?), right: Box::new(self.expression(right, scope, allow_old)?) },
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
                        "declare a supported function",
                    )
                })?;
                if self.ordinary[function.0] {
                    return Err(error(call.span, "top-level prototype calls require a verified function", "ordinary functions are callable only as expressions inside ordinary functions; mixed-assurance rules remain open"));
                }
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
