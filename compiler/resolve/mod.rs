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

/// Resolve complete declaration sets, then executable statements in source order.
/// No partially resolved program is returned on failure.
pub fn resolve(ast: &ast::Program) -> Result<hir::Program, Vec<Diagnostic>> {
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
    for (index, function) in ast.functions.iter().enumerate() {
        match resolver.function(FunctionId(index), function) {
            Ok(function) => resolver.program.functions.push(function),
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
    Ok(resolver.program)
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
    ) -> ResolutionResult<hir::VerifiedFunction> {
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
        let requires = &function.precondition;
        if requires.amount != function.amount_param {
            return Err(error(
                requires.span,
                format!(
                    "vertical slice expects precondition lhs `{}`, found `{}`",
                    function.amount_param, requires.amount
                ),
                "use the declared amount parameter",
            ));
        }
        if requires.state.binding != function.state_param {
            return Err(error(
                requires.state.span,
                format!(
                    "vertical slice expects precondition state `{}`, found `{}`",
                    function.state_param, requires.state.binding
                ),
                "use the declared mutable record parameter",
            ));
        }
        if requires.state.field != self.program.field(field).name {
            return Err(error(
                requires.span,
                format!("unknown precondition field `{}`", requires.state.field),
                format!(
                    "record `{}` has field `{}`",
                    function.state_type,
                    self.program.field(field).name
                ),
            ));
        }
        let precondition = hir::Precondition {
            amount: amount_param,
            state: self.access(&requires.state, function, state_param, field)?,
            span: requires.span,
        };
        let post = &function.postcondition;
        let target = self.access(&post.target, function, state_param, field)?;
        let old = self.access(&post.old, function, state_param, field)?;
        if post.amount != function.amount_param {
            return Err(error(
                post.span,
                format!("unknown postcondition parameter `{}`", post.amount),
                format!(
                    "the supported amount parameter is `{}`",
                    function.amount_param
                ),
            ));
        }
        let postcondition = hir::Postcondition {
            target,
            old,
            amount: amount_param,
            span: post.span,
        };
        let mut body = Vec::new();
        for statement in &function.body {
            let target = self.access(&statement.target, function, state_param, field)?;
            let operand = match &statement.operand {
                ast::Operand::Literal(value) => hir::Operand::Literal(*value),
                ast::Operand::Parameter(name) => {
                    if name != &function.amount_param {
                        return Err(error(
                            statement.span,
                            format!("unknown subtraction parameter `{name}`"),
                            format!(
                                "use `{}` or a signed integer literal",
                                function.amount_param
                            ),
                        ));
                    }
                    hir::Operand::Parameter(amount_param)
                }
            };
            body.push(hir::Subtract {
                target,
                operand,
                span: statement.span,
            });
        }
        // The mandatory postcondition subtracts amount even for empty/literal bodies.
        // Nominal identity, never display-name or numeric-bound equality.
        let state_type_id = postcondition.target.ty;
        if state_type_id != amount_param.ty {
            let state_range = self.program.range(state_type_id);
            let amount_range = self.program.range(amount_param.ty);
            return Err(error(
                post.span,
                format!(
                    "incompatible named range types for subtraction: `{}` and `{}`",
                    state_range.name, amount_range.name
                ),
                format!(
                    "state field `{}.{}` has type `{}`; parameter `{}` has type `{}`. \
                     This prototype does not permit implicit arithmetic between distinct named range types; \
                     both must use the same declared range type",
                    function.state_param, self.program.field(field).name, state_range.name,
                    function.amount_param, amount_range.name
                ),
            ));
        }
        Ok(hir::VerifiedFunction {
            id,
            name: function.name.clone(),
            span: function.span,
            state_param,
            state_type,
            amount_param,
            precondition,
            postcondition,
            body,
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
