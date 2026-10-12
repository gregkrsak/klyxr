//! Checked lookups precede lowering or representation comparison. This validates
//! compiler invariants only; it neither infers types nor checks ownership.
use super::*;
use crate::hir::{ExprKind, ExprType, ParameterType, Place, TypedExpr, ValueStatement};

#[cfg(test)]
thread_local! {
    pub(super) static RECORD_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn bad() -> ExecutionError {
    ExecutionError::integrity("invalid canonical execution metadata")
}
fn ty(p: &hir::Program, t: ExprType) -> Result<(), ExecutionError> {
    match t {
        ExprType::Range(id) => {
            if p.ranges()
                .get(id.0)
                .filter(|r| r.id == id && r.min <= r.max)
                .is_none()
            {
                return Err(bad());
            }
        }
        ExprType::Record(id) => {
            if p.records().get(id.0).filter(|r| r.id == id).is_none() {
                return Err(bad());
            }
        }
        ExprType::SharedRef(r) | ExprType::MutableRef(r) => ty(p, r.value_type().into())?,
        ExprType::Bool | ExprType::IntegerLiteral => {}
    }
    Ok(())
}
fn place(p: &hir::Program, f: FunctionId, place: Place) -> Result<ValueType, ExecutionError> {
    match place {
        Place::Local(id) => p
            .locals()
            .get(id.0)
            .filter(|l| l.id == id && l.function == f)
            .map(|l| l.ty)
            .ok_or_else(bad),
        Place::Parameter(id) => p
            .parameters()
            .get(id.0)
            .filter(|l| l.id == id && l.function == f)
            .and_then(|l| match l.ty {
                ParameterType::Value(t) => Some(t),
                _ => None,
            })
            .ok_or_else(bad),
    }
}
fn field(
    p: &hir::Program,
    record: hir::RecordId,
    field: hir::FieldId,
) -> Result<hir::RangeTypeId, ExecutionError> {
    let r = p
        .records()
        .get(record.0)
        .filter(|r| r.id == record)
        .ok_or_else(bad)?;
    let f = p
        .fields()
        .get(field.0)
        .filter(|f| f.id == field && f.record == record && r.fields.contains(&field))
        .ok_or_else(bad)?;
    let range = f.range_type().ok_or_else(bad)?;
    ty(p, ExprType::Range(range))?;
    Ok(range)
}
pub(super) fn expression(
    p: &hir::Program,
    f: FunctionId,
    e: &TypedExpr,
    c: &Context,
    depth: usize,
    ordinary: bool,
    conditional: bool,
) -> Result<(), ExecutionError> {
    if depth > c.limits.traversal_depth {
        return Err(ResourceFailure::limit("admission expression depth", e.span).into());
    }
    if !p.resources.leaves.is_empty() {
        c.resource_node(e.span)?;
    }
    ty(p, e.ty)?;
    match &e.kind {
        ExprKind::Parameter(id) if ordinary => {
            if ExprType::from(place(p, f, Place::Parameter(*id))?) != e.ty {
                return Err(bad());
            }
        }
        ExprKind::Local(id) => {
            if ExprType::from(place(p, f, Place::Local(*id))?) != e.ty {
                return Err(bad());
            }
        }
        ExprKind::BoolLiteral(_) => {
            if e.ty != ExprType::Bool {
                return Err(bad());
            }
        }
        ExprKind::IntegerLiteral(_) => {
            if e.ty != ExprType::IntegerLiteral {
                return Err(bad());
            }
        }
        ExprKind::FormedRangeLiteral(v) => {
            let ExprType::Range(id) = e.ty else {
                return Err(bad());
            };
            let r = &p.ranges()[id.0];
            if !ordinary || *v < r.min || *v > r.max {
                return Err(bad());
            }
        }
        ExprKind::CopyFieldRead {
            owner,
            record,
            field: id,
        } => {
            if place(p, f, *owner)? != ValueType::Record(*record)
                || e.ty != ExprType::Range(field(p, *record, *id)?)
            {
                return Err(bad());
            }
        }
        ExprKind::CopyReferenceFieldRead {
            reference,
            record,
            field: id,
        } => {
            if !matches!(place(p, f, *reference)?, ValueType::SharedRef(hir::ReferentType::Record(r)) | ValueType::MutableRef(hir::ReferentType::Record(r)) if r == *record)
                || e.ty != ExprType::Range(field(p, *record, *id)?)
            {
                return Err(bad());
            }
        }
        ExprKind::Deref { reference } => {
            let r = match place(p, f, *reference)? {
                ValueType::SharedRef(r) | ValueType::MutableRef(r) => r,
                _ => return Err(bad()),
            };
            if ExprType::from(r.value_type()) != e.ty {
                return Err(bad());
            }
        }
        ExprKind::Borrow { kind, place: at } => {
            let t = place(p, f, *at)?;
            let r = match t {
                ValueType::Bool => hir::ReferentType::Bool,
                ValueType::Range(r) => hir::ReferentType::Range(r),
                ValueType::Record(r) => hir::ReferentType::Record(r),
                _ => return Err(bad()),
            };
            let expected = match kind {
                hir::BorrowKind::Shared => ExprType::SharedRef(r),
                hir::BorrowKind::Mutable => ExprType::MutableRef(r),
            };
            if e.ty != expected {
                return Err(bad());
            }
        }
        ExprKind::RecordConstruct { record, fields } => {
            let r = p
                .records()
                .get(record.0)
                .filter(|r| r.id == *record)
                .ok_or_else(bad)?;
            if e.ty != ExprType::Record(*record) || fields.len() != r.fields.len() {
                return Err(bad());
            }
            for (i, item) in fields.iter().enumerate() {
                if fields[..i].iter().any(|a| a.field == item.field)
                    || item.value.ty
                        != ExprType::from(
                            p.fields
                                .get(item.field.0)
                                .filter(|f| {
                                    f.id == item.field
                                        && f.record == *record
                                        && r.fields.contains(&item.field)
                                })
                                .ok_or_else(bad)?
                                .ty,
                        )
                {
                    return Err(bad());
                }
                expression(p, f, &item.value, c, depth + 1, ordinary, false)?;
            }
        }
        ExprKind::Call {
            function,
            arguments,
        } => {
            let callee = p
                .functions()
                .get(function.0)
                .filter(|v| v.id() == *function)
                .and_then(|v| v.as_ordinary())
                .ok_or_else(bad)?;
            if callee.return_type.map(ExprType::from) != Some(e.ty)
                || arguments.len() != callee.parameters.len()
            {
                return Err(bad());
            }
            for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
                if ExprType::from(place(p, *function, Place::Parameter(*parameter))?) != argument.ty
                {
                    return Err(bad());
                }
                expression(p, f, argument, c, depth + 1, ordinary, false)?;
            }
        }
        ExprKind::Unary { operand, .. } => {
            if e.ty != ExprType::Bool || operand.ty != ExprType::Bool {
                return Err(bad());
            }
            expression(p, f, operand, c, depth + 1, ordinary, false)?;
        }
        ExprKind::Binary { op, left, right } => {
            let numeric = |a, b| {
                matches!((a,b), (ExprType::Range(x),ExprType::Range(y)) if x == y)
                    || matches!(
                        (a, b),
                        (ExprType::Range(_), ExprType::IntegerLiteral)
                            | (ExprType::IntegerLiteral, ExprType::Range(_))
                            | (ExprType::IntegerLiteral, ExprType::IntegerLiteral)
                    )
            };
            let valid = match op {
                hir::BinaryOp::And | hir::BinaryOp::Or => {
                    left.ty == ExprType::Bool
                        && right.ty == ExprType::Bool
                        && e.ty == ExprType::Bool
                }
                hir::BinaryOp::Equal => {
                    e.ty == ExprType::Bool
                        && ((left.ty == ExprType::Bool && right.ty == ExprType::Bool)
                            || numeric(left.ty, right.ty))
                }
                hir::BinaryOp::LessEqual => e.ty == ExprType::Bool && numeric(left.ty, right.ty),
                hir::BinaryOp::Subtract => {
                    matches!(left.ty, ExprType::Range(_))
                        && e.ty == left.ty
                        && (right.ty == left.ty || right.ty == ExprType::IntegerLiteral)
                }
            };
            if !valid {
                return Err(bad());
            }
            expression(p, f, left, c, depth + 1, ordinary, false)?;
            expression(p, f, right, c, depth + 1, ordinary, false)?;
        }
        ExprKind::IfValue {
            condition,
            then_value,
            else_value,
        } => {
            if !conditional
                || !ordinary
                || condition.ty != ExprType::Bool
                || then_value.ty != e.ty
                || else_value.ty != e.ty
            {
                return Err(bad());
            }
            expression(p, f, condition, c, depth + 1, true, false)?;
            expression(p, f, then_value, c, depth + 1, true, true)?;
            expression(p, f, else_value, c, depth + 1, true, true)?;
        }
        ExprKind::FieldAccess(_) | ExprKind::OldField(_) if ordinary => return Err(bad()),
        ExprKind::Parameter(id) => {
            if p.parameters()
                .get(id.0)
                .filter(|a| a.id == *id && a.function == f)
                .is_none()
            {
                return Err(bad());
            }
        }
        ExprKind::FieldAccess(a) | ExprKind::OldField(a) => {
            let f = p
                .fields()
                .get(a.field.0)
                .filter(|f| f.id == a.field && f.range_type() == Some(a.ty))
                .ok_or_else(bad)?;
            field(p, f.record, a.field)?;
        }
    }
    Ok(())
}
fn statements(
    p: &hir::Program,
    f: FunctionId,
    body: &[ValueStatement],
    c: &Context,
    depth: usize,
) -> Result<(), ExecutionError> {
    if depth > c.limits.traversal_depth {
        return Err(ResourceFailure::limit("admission statement depth", START).into());
    }
    for s in body {
        if !p.resources.leaves.is_empty() {
            c.resource_node(START)?;
        }
        match s {
            ValueStatement::Let {
                local, initializer, ..
            } => {
                let expected = ExprType::from(place(p, f, Place::Local(*local))?);
                if initializer.ty != expected {
                    return Err(bad());
                }
                expression(p, f, initializer, c, 1, true, true)?;
            }
            ValueStatement::Assign { local, value, .. } => {
                place(p, f, Place::Local(*local))?;
                expression(p, f, value, c, 1, true, false)?;
            }
            ValueStatement::CopyFieldAssign {
                owner,
                record,
                field: id,
                value,
                ..
            } => {
                place(p, f, Place::Local(*owner))?;
                field(p, *record, *id)?;
                expression(p, f, value, c, 1, true, false)?;
            }
            ValueStatement::CopyReferenceFieldAssign {
                reference,
                record,
                field: id,
                value,
                ..
            } => {
                place(p, f, *reference)?;
                field(p, *record, *id)?;
                expression(p, f, value, c, 1, true, false)?;
            }
            ValueStatement::DerefAssign {
                reference, value, ..
            } => {
                place(p, f, *reference)?;
                expression(p, f, value, c, 1, true, false)?;
            }
            ValueStatement::Return { value, .. } => expression(p, f, value, c, 1, true, false)?,
            ValueStatement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                expression(p, f, condition, c, 1, true, false)?;
                statements(p, f, then_body, c, depth + 1)?;
                statements(p, f, else_body, c, depth + 1)?;
            }
            ValueStatement::While {
                condition, body, ..
            } => {
                expression(p, f, condition, c, 1, true, false)?;
                statements(p, f, body, c, depth + 1)?;
            }
            ValueStatement::CallNoValue {
                function,
                arguments,
                ..
            } => {
                let callee = p
                    .functions()
                    .get(function.0)
                    .filter(|v| v.id() == *function)
                    .and_then(|v| v.as_ordinary())
                    .ok_or_else(bad)?;
                if callee.return_type.is_some() || arguments.len() != callee.parameters.len() {
                    return Err(bad());
                }
                for (a, parameter) in arguments.iter().zip(&callee.parameters) {
                    if a.ty != ExprType::from(place(p, *function, Place::Parameter(*parameter))?) {
                        return Err(bad());
                    }
                    expression(p, f, a, c, 1, true, false)?;
                }
            }
            ValueStatement::ReturnNoValue { .. }
            | ValueStatement::Continue { .. }
            | ValueStatement::Break { .. } => {}
        }
    }
    Ok(())
}
pub(super) fn program_shape(p: &hir::Program, c: &Context) -> Result<(), ExecutionError> {
    if !p.resources.leaves.is_empty()
        && (p
            .records
            .iter()
            .any(|r| r.name.len() > c.limits.identifier_bytes)
            || p.fields
                .iter()
                .any(|f| f.name.len() > c.limits.identifier_bytes)
            || p.functions
                .iter()
                .any(|f| f.name().len() > c.limits.identifier_bytes)
            || p.locals
                .iter()
                .any(|l| l.name.len() > c.limits.identifier_bytes)
            || p.parameters
                .iter()
                .any(|p| p.name.len() > c.limits.identifier_bytes))
    {
        return Err(ResourceFailure::limit("resource identifier bytes", START).into());
    }
    if p.resources.leaves.len() > c.limits.tokens || p.resources.operations.len() > c.limits.tokens
    {
        return Err(bad());
    }
    if p.functions().len() > c.limits.tokens
        || p.locals().len() > c.limits.tokens
        || p.parameters().len() > c.limits.tokens
        || p.fields().len() > c.limits.tokens
        || p.ranges().len() > c.limits.tokens
        || p.records().len() > c.limits.tokens
    {
        return Err(bad());
    }
    for (i, r) in p.ranges().iter().enumerate() {
        if r.id != hir::RangeTypeId(i) || r.min > r.max {
            return Err(bad());
        }
    }
    for (i, l) in p.locals().iter().enumerate() {
        if l.id != hir::LocalId(i)
            || p.functions()
                .get(l.function.0)
                .filter(|f| f.id() == l.function)
                .is_none()
        {
            return Err(bad());
        }
        ty(p, l.ty.into())?;
    }
    for (i, l) in p.parameters().iter().enumerate() {
        if l.id != hir::ParameterId(i)
            || p.functions()
                .get(l.function.0)
                .filter(|f| f.id() == l.function)
                .is_none()
        {
            return Err(bad());
        }
        match l.ty {
            hir::ParameterType::Value(t) => ty(p, t.into())?,
            hir::ParameterType::Range(r) => ty(p, ExprType::Range(r))?,
            hir::ParameterType::MutableRecord(r) => ty(p, ExprType::Record(r))?,
        }
    }
    for (i, f) in p.functions().iter().enumerate() {
        if f.id() != FunctionId(i) {
            return Err(bad());
        }
        if let Some(f) = f.as_ordinary() {
            for a in &f.parameters {
                place(p, f.id, Place::Parameter(*a))?;
            }
            if let Some(t) = f.return_type {
                ty(p, t.into())?;
            }
            statements(p, f.id, &f.body, c, 1)?;
        } else if let Some(f) = f.as_verified() {
            expression(p, f.id, &f.requires, c, 1, false, false)?;
            expression(p, f.id, &f.ensures, c, 1, false, false)?;
            // Retained proof operands are visited by record validation and the
            // authority comparison even though ordinary execution skips them.
            for statement in &f.body {
                expression(p, f.id, &statement.operand, c, 1, false, false)?;
            }
        }
    }
    if !p.resources.leaves.is_empty() {
        p.resources
            .plans_bounded(p, c.limits.traversal_depth)
            .map_err(|m| {
                if m == "cleanup type graph depth exceeded" {
                    ExecutionError::from(ResourceFailure::limit("cleanup type graph depth", START))
                } else {
                    ExecutionError::integrity(m)
                }
            })?;
        let n = c
            .resource_nodes
            .get()
            .checked_add(
                p.functions.len()
                    + p.fields.len()
                    + p.records.len()
                    + p.locals.len()
                    + p.parameters.len()
                    + p.ranges.len()
                    + p.resources.leaves.len()
                    + p.resources.operations.len(),
            )
            .ok_or_else(|| {
                ExecutionError::from(ResourceFailure::limit(
                    "resource admission accounting",
                    START,
                ))
            })?;
        c.reserve_frontend(n, 0)?;
    }
    #[cfg(test)]
    RECORD_WALKS.with(|n| n.set(n.get() + 1));
    p.validate_record_expressions().map_err(|_| bad())?;
    Ok(())
}

pub(super) fn mir_shape(
    p: &hir::Program,
    m: &mir::MirProgram,
    c: &Context,
) -> Result<(), ExecutionError> {
    c.resource_nodes.set(0);
    if m.functions().len() > c.limits.tokens {
        return Err(bad());
    }
    for f in m.functions() {
        let definition = p
            .functions()
            .get(f.function().0)
            .filter(|v| v.id() == f.function())
            .and_then(|v| v.as_ordinary())
            .ok_or_else(bad)?;
        if f.blocks().len() > c.limits.tokens.checked_mul(4).ok_or_else(bad)? {
            return Err(bad());
        }
        if let Some(flow) = &f.cleanup {
            if flow.scopes.len() > c.limits.tokens
                || flow.blocks.len() > c.limits.tokens.saturating_mul(4)
                || flow.parameters.len() > c.limits.tokens
            {
                return Err(bad());
            }
            for scope in &flow.scopes {
                if scope.owners.len() > c.limits.tokens {
                    return Err(bad());
                }
            }
            for block in &flow.blocks {
                if block.initialize.len() > c.limits.tokens
                    || block.exit.len() > c.limits.traversal_depth
                {
                    return Err(bad());
                }
            }
        }
        for (_, b) in f.blocks() {
            if b.statements().len() > c.limits.tokens {
                return Err(bad());
            }
            for s in b.statements() {
                let value = match s {
                    mir::Statement::CallNoValue {
                        function,
                        arguments,
                        ..
                    } => {
                        let callee = p
                            .functions()
                            .get(function.0)
                            .filter(|v| v.id() == *function)
                            .and_then(|v| v.as_ordinary())
                            .ok_or_else(bad)?;
                        if arguments.len() != callee.parameters.len()
                            || callee.return_type.is_some()
                        {
                            return Err(bad());
                        }
                        for a in arguments {
                            expression(p, f.function(), a, c, 1, true, false)?;
                        }
                        continue;
                    }
                    mir::Statement::Let { initializer, .. } => initializer,
                    mir::Statement::Assign { value, .. }
                    | mir::Statement::CopyFieldAssign { value, .. }
                    | mir::Statement::CopyReferenceFieldAssign { value, .. }
                    | mir::Statement::DerefAssign { value, .. } => value,
                };
                expression(p, f.function(), value, c, 1, true, false)?;
            }
            match b.terminator() {
                mir::Terminator::Branch { condition, .. } => {
                    expression(p, f.function(), condition, c, 1, true, false)?;
                }
                mir::Terminator::Return { value } => {
                    if definition.return_type.map(ExprType::from) != Some(value.ty) {
                        return Err(bad());
                    }
                    expression(p, f.function(), value, c, 1, true, false)?;
                }
                mir::Terminator::ReturnNoValue if definition.return_type.is_some() => {
                    return Err(bad())
                }
                _ => {}
            }
        }
    }
    Ok(())
}
