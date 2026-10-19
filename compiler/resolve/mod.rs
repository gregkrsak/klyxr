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
fn unreserved(name: &str, span: Span) -> ResolutionResult<()> {
    if crate::lexer::reserved_result_family(name).is_some() {
        return Err(error(span, format!("reserved Result-family identifier `{name}`"), "complete Result, Ok, Error, Err, and match spelling families are unavailable as user identifiers"));
    }
    Ok(())
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
    Result(hir::ResultTypeId),
    Bool,
    Range(RangeTypeId),
    Record(RecordId),
    Resource(hir::ResourceTypeId),
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
    /// None is explicitly NoValue; Some(T) is Value(T), never result inference.
    pub return_type: Option<ResolvedValueType>,
    pub effects: crate::effects::Declaration,
    pub body: Vec<ResolvedValueStatement>,
    pub span: Span,
}
#[derive(Debug)]
pub(crate) enum ResolvedValueStatement {
    Match {
        scrutinee: ResolvedExpr,
        arms: Vec<ResolvedMatchArm>,
        span: Span,
    },
    ReturnNoValue {
        span: Span,
    },
    CallNoValue {
        function: FunctionId,
        arguments: Vec<ResolvedExpr>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    While {
        condition: ResolvedExpr,
        body: Vec<ResolvedValueStatement>,
        span: Span,
    },
    FieldAssign {
        owner: hir::Place,
        field_name: String,
        owner_span: Span,
        field_span: Span,
        value: ResolvedExpr,
        span: Span,
    },
    Assign {
        target: hir::Place,
        value: ResolvedExpr,
        span: Span,
    },
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
        propagation: Option<ast::PropagationSyntax>,
        span: Span,
    },
    Return {
        value: ResolvedExpr,
        span: Span,
    },
}
#[derive(Debug)]
pub(crate) struct ResolvedMatchArm {
    pub tag: hir::ResultTag,
    pub binding: LocalId,
    pub function: FunctionId,
    pub name: String,
    pub pattern_span: Span,
    pub body: Vec<ResolvedValueStatement>,
    pub span: Span,
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
    ResultConstruct {
        tag: hir::ResultTag,
        payload: Box<ResolvedExpr>,
        variant_span: Span,
    },
    RecordConstruct {
        record: RecordId,
        fields: Vec<ResolvedRecordFieldInit>,
    },
    // The owner is resolved now; inferred-local field selection finishes in typing.
    CopyFieldRead {
        owner: hir::Place,
        field_name: String,
        field_span: Span,
    },
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
#[derive(Debug)]
pub(crate) struct ResolvedRecordFieldInit {
    pub field: FieldId,
    pub value: ResolvedExpr,
    pub span: Span,
}
/// Transient resolved signatures share the same FunctionId indices as the final table.
enum Signature {
    Ordinary {
        parameters: Vec<ParameterId>,
        return_type: Option<ResolvedValueType>,
        effects: crate::effects::Declaration,
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
    resolve_with(ast, None, None)
}

pub(crate) fn resolve_bounded(
    ast: &ast::Program,
    context: &crate::bounded::Context,
) -> Result<ResolvedProgram, Vec<Diagnostic>> {
    resolve_with(ast, Some(context), None)
}

pub(crate) fn resolve_profile(
    ast: &ast::Program,
    context: &crate::bounded::Context,
    selection: &crate::trusted::Selection,
) -> Result<ResolvedProgram, Vec<Diagnostic>> {
    resolve_with(ast, Some(context), Some(selection))
}

fn resolve_with(
    ast: &ast::Program,
    context: Option<&crate::bounded::Context>,
    selection: Option<&crate::trusted::Selection>,
) -> Result<ResolvedProgram, Vec<Diagnostic>> {
    let mut resolver = Resolver {
        context,
        ..Resolver::default()
    };
    let mut diagnostics = Vec::new();
    let mut types: HashSet<&str> = HashSet::new();
    if let Some(selection) = selection {
        let context = context.expect("profile preparation always carries bounded admission");
        let catalog = crate::trusted::Catalog::collect(selection, context).map_err(|failure| {
            context.fail(failure);
            vec![Diagnostic::semantic(
                failure.span,
                failure.resource,
                "catalog admission stopped before signature collection",
            )]
        })?;
        resolver.program.ranges.try_reserve_exact(2).map_err(|_| {
            let failure = crate::bounded::ResourceFailure::host(
                "imported range table",
                crate::bounded::START,
            );
            context.fail(failure);
            vec![Diagnostic::semantic(
                failure.span,
                failure.resource,
                "no catalog is published",
            )]
        })?;
        for (index, (name, min, max)) in [("Reading", 0, 4095), ("SensorError", 1, 3)]
            .into_iter()
            .enumerate()
        {
            let id = RangeTypeId(index);
            types.insert(name);
            resolver.ranges.insert(name.into(), id);
            resolver.program.ranges.push(hir::RangeType {
                id,
                name: name.into(),
                min,
                max,
                origin: hir::DeclarationOrigin::Catalog(catalog.origin(index)),
            });
        }
        types.insert("Sensor");
        resolver.program.trusted = Some(catalog);
    }
    for range in &ast.ranges {
        unreserved(&range.name, range.span).map_err(|e| vec![*e])?;
        if !types.insert(&range.name) {
            diagnostics.push(*duplicate(range.span, &range.name));
            continue;
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
            origin: hir::DeclarationOrigin::Source(range.span),
        });
    }
    for (index, record) in ast.records.iter().enumerate() {
        unreserved(&record.name, record.span).map_err(|e| vec![*e])?;
        if !types.insert(&record.name) {
            diagnostics.push(*duplicate(record.span, &record.name));
            continue;
        }
        resolver
            .records
            .insert(record.name.clone(), RecordId(index));
    }
    for (index, function) in ast.functions.iter().enumerate() {
        unreserved(function.name(), function.span()).map_err(|e| vec![*e])?;
        if resolver
            .program
            .trusted
            .as_ref()
            .is_some_and(|c| c.callable(function.name()).is_some())
        {
            diagnostics.push(*duplicate(function.span(), function.name()));
            continue;
        }
        match function {
            ast::FunctionDecl::Ordinary(f) => {
                for p in &f.parameters {
                    unreserved(&p.name, p.span).map_err(|e| vec![*e])?;
                }
            }
            ast::FunctionDecl::Verified(f) => {
                unreserved(&f.state_param, f.span).map_err(|e| vec![*e])?;
                unreserved(&f.amount_param, f.span).map_err(|e| vec![*e])?;
            }
        }
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
        let id = RecordId(index);
        let mut fields = Vec::new();
        let mut names = HashSet::new();
        if record.fields.is_empty() {
            return Err(vec![*error(
                record.span,
                "record declaration requires at least one named-range field",
                "empty records are unsupported",
            )]);
        }
        for declaration in &record.fields {
            unreserved(&declaration.name, declaration.span).map_err(|e| vec![*e])?;
            if !names.insert(&declaration.name) {
                return Err(vec![*duplicate(declaration.span, &declaration.name)]);
            }
            let ty = resolver
                .range(&declaration.ty, declaration.span)
                .map_err(|e| vec![*e])?;
            let field = FieldId(resolver.program.fields.len());
            resolver.program.fields.push(hir::Field {
                id: field,
                record: id,
                name: declaration.name.clone(),
                ty: hir::ValueType::Range(ty),
                span: declaration.span,
            });
            fields.push(field);
        }
        resolver.program.records.push(hir::Record {
            id,
            name: record.name.clone(),
            fields,
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
struct Resolver<'a> {
    context: Option<&'a crate::bounded::Context>,
    program: hir::Program,
    parameters: Vec<ResolvedParameter>,
    local_count: usize,
    ranges: HashMap<String, RangeTypeId>,
    records: HashMap<String, RecordId>,
    functions: HashMap<String, FunctionId>,
    ordinary: Vec<bool>,
    bindings: HashMap<String, BindingId>,
}
impl Resolver<'_> {
    // M1 admission resolves the exact catalog row. Invocation remains disabled
    // until canonical calls, receipts and persistent sessions are implemented.
    fn selected_call(&self, name: &str, span: Span) -> ResolutionResult<()> {
        if let Some(catalog) = &self.program.trusted {
            if let Some(op) = catalog.callable(name) {
                if !catalog.granted(op.id)
                    || (op.class == crate::trusted::OperationClass::Acquisition
                        && !catalog.granted(catalog.operations[2].id))
                {
                    return Err(error(span, format!("trusted operation `{name}` is not granted"), "the exact selected operation and its fixed cleanup-only release require authority"));
                }
                return Err(error(
                    span,
                    "trusted invocation is not enabled at this implementation milestone",
                    "the selected canonical row has no ordinary-function fallback",
                ));
            }
        }
        Ok(())
    }
    fn result_resource_error(&self, failure: crate::bounded::ResourceFailure) -> Box<Diagnostic> {
        if let Some(context) = self.context {
            context.fail(failure);
        }
        error(
            failure.span,
            failure.resource,
            "Result admission stopped; no partial program is published",
        )
    }
    fn prototype_field(&self, record: RecordId, span: Span) -> ResolutionResult<FieldId> {
        let fields = &self.program.record(record).fields;
        if fields.len() != 1 {
            return Err(error(
                span,
                "unsupported multi-field state in verified prototype or harness",
                "the specialized verified-state path requires exactly one named-range field",
            ));
        }
        Ok(fields[0])
    }
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
    fn value_type(
        &mut self,
        ty: &ast::ValueType,
        span: Span,
    ) -> ResolutionResult<ResolvedValueType> {
        let owned =
            if ty.result_nodes.is_empty() {
                self.owned_type(&ty.name, span)?
            } else {
                if ty.result_nodes.len() > 4096 {
                    return Err(self.result_resource_error(
                        crate::bounded::ResourceFailure::limit("Result syntax nodes", span),
                    ));
                }
                if ty.name != "Result" || !ty.references.is_empty() {
                    return Err(error(
                        span,
                        "invalid Result type syntax",
                        "Result interfaces require bounded exact owned payloads",
                    ));
                }
                let mut values: Vec<hir::ValueType> = Vec::new();
                crate::bounded::result_admission::reserve(
                    &mut values,
                    ty.result_nodes.len(),
                    crate::bounded::result_admission::Allocation::Scratch,
                    span,
                )
                .map_err(|failure| self.result_resource_error(failure))?;
                for node in &ty.result_nodes {
                    let value = match node {
                        ast::ResultTypeSyntax::Owned(name) => match self.owned_type(name, span)? {
                            ResolvedValueType::Bool => hir::ValueType::Bool,
                            ResolvedValueType::Range(id) => hir::ValueType::Range(id),
                            ResolvedValueType::Record(id) => hir::ValueType::Record(id),
                            ResolvedValueType::Resource(id) => hir::ValueType::Resource(id),
                            _ => {
                                return Err(error(
                                    span,
                                    "invalid Result payload",
                                    "payloads must be ordinary owned values",
                                ))
                            }
                        },
                        ast::ResultTypeSyntax::Result { success, failure } => {
                            let success = *values.get(*success).ok_or_else(|| {
                                error(
                                    span,
                                    "invalid Result success syntax identity",
                                    "type syntax must be finite postorder",
                                )
                            })?;
                            let failure = *values.get(*failure).ok_or_else(|| {
                                error(
                                    span,
                                    "invalid Result failure syntax identity",
                                    "type syntax must be finite postorder",
                                )
                            })?;
                            let id = self.program.intern_result(success, failure).map_err(
                                |failure| {
                                    match failure {
                                hir::results::AdmissionError::Source(message) => error(
                                    span,
                                    message,
                                    "canonical Result identities preserve ordered payload roles",
                                ),
                                hir::results::AdmissionError::Resource(mut failure) => {
                                    failure.span = span;
                                    self.result_resource_error(failure)
                                }
                                }
                                },
                            )?;
                            hir::ValueType::Result(id)
                        }
                    };
                    values.push(value);
                }
                match values.last() {
                    Some(hir::ValueType::Result(id)) => ResolvedValueType::Result(*id),
                    _ => {
                        return Err(error(
                            span,
                            "incomplete Result type syntax",
                            "a complete Result root is required",
                        ))
                    }
                }
            };
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
            ResolvedValueType::Resource(id) => {
                if *kind == ast::BorrowKind::Mutable {
                    return Err(error(
                        span,
                        "mutable Sensor references are excluded",
                        "opaque resources permit shared observation only",
                    ));
                }
                hir::ReferentType::Resource(id)
            }
            ResolvedValueType::Result(_) => {
                return Err(error(
                    span,
                    "Result reference interfaces are excluded",
                    "Result parameters and returns must be owned",
                ))
            }
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
        if name == "Sensor" {
            if let Some(catalog) = &self.program.trusted {
                return Ok(ResolvedValueType::Resource(catalog.sensor));
            }
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
                self.prototype_field(state_type, f.span)?;
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
                let return_type = f
                    .return_type
                    .as_ref()
                    .map(|ty| self.value_type(ty, f.span))
                    .transpose()?;
                let effects = crate::effects::Declaration::resolve(f.effects.as_ref())?;
                Ok(Signature::Ordinary {
                    parameters,
                    return_type,
                    effects,
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
            ty: declaration
                .range_type()
                .expect("resolved source fields are ranges"),
            span: access.span,
        })
    }
    fn value_statements(
        &mut self,
        function: FunctionId,
        statements: &[ast::ValueStatement],
        scope: &mut Scope,
        loop_depth: usize,
    ) -> ResolutionResult<Vec<ResolvedValueStatement>> {
        let mut body = Vec::new();
        for (index, statement) in statements.iter().enumerate() {
            body.push(match statement {
                ast::ValueStatement::Match { scrutinee, arms, span } => {
                    if arms.len() != 2 || arms[0].tag == arms[1].tag {
                        return Err(error(*span, "Result match requires one Ok and one failure arm (Error or Err)", "duplicate or missing canonical variants are forbidden"));
                    }
                    let scrutinee = self.expression(scrutinee, scope, false)?;
                    let mut resolved_arms = Vec::new();
                    for arm in arms {
                        if crate::lexer::reserved_result_family(&arm.binding).is_some() || scope.values.contains_key(&arm.binding) {
                            return Err(error(arm.pattern_span, "invalid Result arm binding", "bindings must be unreserved and cannot shadow a visible name"));
                        }
                        let binding = LocalId(self.local_count);
                        self.local_count += 1;
                        let mut child = scope.clone();
                        child.values.insert(arm.binding.clone(), Reference::Local(binding));
                        let body = self.value_statements(function, &arm.body, &mut child, loop_depth)?;
                        resolved_arms.push(ResolvedMatchArm { tag: arm.tag, binding, function, name: arm.binding.clone(), pattern_span: arm.pattern_span, body, span: arm.span });
                    }
                    ResolvedValueStatement::Match { scrutinee, arms: resolved_arms, span: *span }
                }
                ast::ValueStatement::Break { span } => {
                    if loop_depth == 0 {
                        return Err(error(
                            *span,
                            "break is permitted only inside an ordinary while loop",
                            "break targets the innermost enclosing while",
                        ));
                    }
                    ResolvedValueStatement::Break { span: *span }
                }
                ast::ValueStatement::Continue { span } => {
                    if loop_depth == 0 {
                        return Err(error(
                            *span,
                            "continue is permitted only inside an ordinary while loop",
                            "continue targets the innermost enclosing while",
                        ));
                    }
                    ResolvedValueStatement::Continue { span: *span }
                }
                ast::ValueStatement::While {
                    condition,
                    body,
                    span,
                } => ResolvedValueStatement::While {
                    condition: self.expression(condition, scope, false)?,
                    body: self.value_statements(
                        function,
                        body,
                        &mut scope.clone(),
                        loop_depth + 1,
                    )?,
                    span: *span,
                },
                ast::ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    span,
                } => {
                    let condition = self.expression(condition, scope, false)?;
                    let then_body =
                        self.value_statements(function, then_body, &mut scope.clone(), loop_depth)?;
                    let else_body = self.value_statements(
                        function,
                        else_body.as_deref().unwrap_or(&[]),
                        &mut scope.clone(),
                        loop_depth,
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
                    propagation,
                    span,
                } => {
                    unreserved(name, *span)?;
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
                        propagation: *propagation,
                        span: *span,
                    }
                }
                ast::ValueStatement::FieldAssign { owner, field, owner_span, field_span, value, span } => {
                    let owner = self.reference_place(owner, scope, *owner_span)?;
                    ResolvedValueStatement::FieldAssign {
                        owner, field_name: field.clone(), owner_span: *owner_span,
                        field_span: *field_span, value: self.expression(value, scope, false)?, span: *span,
                    }
                }
                ast::ValueStatement::Assign {
                    target,
                    value,
                    span,
                } => {
                    let target = match scope.values.get(target) {
                        Some(Reference::Local(id)) => hir::Place::Local(*id),
                        Some(Reference::Parameter(id)) => hir::Place::Parameter(*id),
                        None => {
                            return Err(error(
                                *span,
                                format!("unknown assignment target `{target}`"),
                                "assign a local declared earlier in the current lexical scope",
                            ))
                        }
                    };
                    ResolvedValueStatement::Assign {
                        target,
                        value: self.expression(value, scope, false)?,
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
                ast::ValueStatement::ReturnNoValue { span } => ResolvedValueStatement::ReturnNoValue { span: *span },
                ast::ValueStatement::CallNoValue { callee, arguments, span } => {
                    self.selected_call(callee, *span)?;
                    let function = self.functions.get(callee).copied().ok_or_else(|| error(*span, format!("unknown function `{callee}`"), "declare an ordinary function"))?;
                    if !self.ordinary[function.0] {
                        return Err(error(*span, "ordinary call statements require an ordinary no-value function", "verified functions retain their specialized call harness; mixed-assurance calls are unsupported"));
                    }
                    let arguments = arguments.iter().map(|a| self.expression(a, scope, false)).collect::<ResolutionResult<Vec<_>>>()?;
                    ResolvedValueStatement::CallNoValue { function, arguments, span: *span }
                },
                ast::ValueStatement::Return { value, span } => ResolvedValueStatement::Return {
                    value: self.expression(value, scope, false)?,
                    span: *span,
                },
            });
            // Public AST callers receive the same structural boundary as source
            // parsing; malformed manually edited blocks must not reach MIR.
            if index + 1 < statements.len() && !statement.falls_through() {
                let span = match statement {
                    ast::ValueStatement::Continue { span }
                    | ast::ValueStatement::Break { span }
                    | ast::ValueStatement::Return { span, .. }
                    | ast::ValueStatement::ReturnNoValue { span }
                    | ast::ValueStatement::If { span, .. }
                    | ast::ValueStatement::Match { span, .. } => *span,
                    _ => unreachable!("only return/break/continue/conditional can terminate here"),
                };
                return Err(error(span, "statements after a terminal path with no fallthrough are unsupported", "return/break/continue must terminate its lexical block; only real fallthrough paths can have a suffix"));
            }
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
                        self.prototype_field(state_type, f.span)?,
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
                    effects,
                },
            ) => {
                let mut scope = Scope::default();
                for (p, id) in f.parameters.iter().zip(&parameters) {
                    scope
                        .values
                        .insert(p.name.clone(), Reference::Parameter(*id));
                }
                let body = self.value_statements(id, &f.body, &mut scope, 0)?;
                Ok(ResolvedFunction::Ordinary(ResolvedValueFunction {
                    id,
                    name: f.name.clone(),
                    parameters,
                    return_type,
                    effects,
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
            ast::ExprKind::ResultConstruct { tag, payload, variant_span } => {
                if scope.state.is_some() {
                    return Err(error(expression.span, "Result construction is ordinary-only", "verified and harness Result forms are excluded"));
                }
                ResolvedExprKind::ResultConstruct { tag: *tag, payload: Box::new(self.expression(payload, scope, false)?), variant_span: *variant_span }
            }
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
            ast::ExprKind::RecordConstruct { record, fields, record_span } => {
                if scope.state.is_some() { return Err(error(expression.span, "record construction is supported only in ordinary functions", "verified expressions retain their restricted proof grammar")); }
                let record = self.records.get(record).copied().ok_or_else(|| error(*record_span, format!("unknown record construction `{record}`"), "construct a declared record"))?;
                let declaration = self.program.record(record);
                let mut selected = Vec::new();
                let mut seen = HashSet::new();
                // Establish membership and uniqueness before resolving any initializer.
                for entry in fields {
                    let field = declaration.fields.iter().copied().find(|id| self.program.field(*id).name == entry.name).ok_or_else(|| error(entry.span, format!("unknown field `{}` for record `{}`", entry.name, declaration.name), "initialize a declared field"))?;
                    if !seen.insert(field) { return Err(error(entry.span, format!("duplicate initializer field `{}`", entry.name), "initialize every declared field exactly once")); }
                    selected.push(field);
                }
                let missing = declaration.fields.iter().filter(|id| !seen.contains(id)).map(|id| self.program.field(*id).name.as_str()).collect::<Vec<_>>();
                if !missing.is_empty() { return Err(error(expression.span, format!("missing initializer fields: {}", missing.join(", ")), "missing fields are listed in declaration order")); }
                let fields = fields.iter().zip(selected).map(|(entry, field)| Ok(ResolvedRecordFieldInit { field, value: self.expression(&entry.value, scope, false)?, span: entry.span })).collect::<ResolutionResult<Vec<_>>>()?;
                ResolvedExprKind::RecordConstruct { record, fields }
            }
            ast::ExprKind::FieldAccess(a) => {
                if scope.state.is_some() { ResolvedExprKind::FieldAccess(self.access(a, scope)?) }
                else { ResolvedExprKind::CopyFieldRead { owner: self.reference_place(&a.binding, scope, a.span)?, field_name: a.field.clone(), field_span: a.field_span } }
            },
            ast::ExprKind::OldField(a) => {
                if !allow_old { return Err(error(expression.span, "old(...) is permitted only in ensures", "this prototype snapshots only the mutable state field at function entry")); }
                ResolvedExprKind::OldField(self.access(a, scope)?)
            }
            ast::ExprKind::Call { callee, arguments } => {
                self.selected_call(callee, expression.span)?;
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
                unreserved(&binding.name, binding.span)?;
                if self.bindings.contains_key(&binding.name) {
                    return Err(duplicate(binding.span, &binding.name));
                }
                let record_type = self.record(&binding.record_type, binding.span)?;
                let field = self.prototype_field(record_type, binding.span)?;
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
