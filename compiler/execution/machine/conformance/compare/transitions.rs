//! Bind newly queued operands to the approved transition that produced them.
//! Queue persistence and consumption are checked separately. No value evaluation,
//! loan liveness, ownership decision, or primary execution occurs here.
use super::*;

#[derive(Clone, Copy)]
enum Operands<'a> {
    None,
    One(&'a TypedExpr),
    Arguments(&'a [TypedExpr]),
    Fields(&'a [hir::RecordFieldInit]),
    Binary(&'a TypedExpr, &'a TypedExpr),
}
impl<'a> Operands<'a> {
    fn get(self, index: usize) -> Option<&'a TypedExpr> {
        match self {
            Self::None => None,
            Self::One(e) => (index == 0).then_some(e),
            Self::Arguments(args) => args.len().checked_sub(index + 1).and_then(|i| args.get(i)),
            Self::Fields(fields) => fields.len().checked_sub(index + 1).and_then(|i| fields.get(i)).map(|f| &f.value),
            Self::Binary(left, right) => match index { 0 => Some(right), 1 => Some(left), _ => None },
        }
    }
}
#[derive(Clone, Copy)]
enum Anchor {
    None,
    Kind(u64),
    Commit(u64, Place, Option<(hir::RecordId, hir::FieldId)>),
    Call(FunctionId, usize),
    Branch(BasicBlockId, BasicBlockId),
    Select(hir::ResultTypeId, BasicBlockId, BasicBlockId),
    Binary(BinaryOp, ExprType),
    Record(hir::RecordId),
    Field(hir::FieldId),
    Short(BinaryOp),
}
#[derive(Clone, Copy)]
pub(super) struct Producer<'a> {
    frame: u64,
    operands: Operands<'a>,
    fields: Option<&'a [hir::RecordFieldInit]>,
    depth: u64,
    next: usize,
    anchor: Anchor,
    ready: bool,
}
impl Producer<'_> {
    fn empty(frame: u64) -> Self {
        Self { frame, operands: Operands::None, fields: None, depth: 1, next: 0, anchor: Anchor::None, ready: true }
    }
}

impl<'a> Validator<'a, '_> {
    fn canonical_expression(&mut self, ordinal: usize) -> Valid<&'a TypedExpr> {
        for (expression, index) in self.contract.expressions() {
            self.work.spend()?;
            if *index == ordinal { return Ok(expression); }
        }
        Err(Incomplete::InvalidState)
    }
    fn task_expression(&mut self, start: usize, end: usize) -> Valid<&'a TypedExpr> {
        for index in start..end {
            self.work.spend()?;
            if let Record::Expression(ordinal) = self.records[index] {
                return self.canonical_expression(ordinal);
            }
        }
        Err(Incomplete::InvalidState)
    }
    fn canonical_fields(&mut self, start: usize, end: usize) -> Valid<&'a [hir::RecordFieldInit]> {
        // The task was independently authorized when queued and has just been
        // consumed unchanged. Resolve its already-bound canonical field table.
        let first = self.task_expression(start, end)?;
        for (expression, _) in self.contract.expressions() {
            self.work.spend()?;
            if let ExprKind::RecordConstruct { fields, .. } = &expression.kind {
                if fields.first().is_some_and(|f| std::ptr::eq(&f.value, first)) {
                    return Ok(fields);
                }
            }
        }
        Err(Incomplete::InvalidState)
    }
    pub(super) fn transition_producer(&mut self, start: usize, end: usize) -> Valid {
        let Record::Continuation(frame) = self.records[start] else { return Err(Incomplete::InvalidState); };
        let Record::Meta(Part::Task, d) = self.records[start + 1] else { return Err(Incomplete::InvalidState); };
        let mut p = Producer::empty(frame);
        match d[0] {
            0 => {
                let control = self.node(key(Kind::Frame, frame))?;
                let definition = self.contract.executable.function(self.function(frame)?).map_err(|_| Incomplete::InvalidState)?;
                let body = definition.blocks().nth(control.block).map(|(_, b)| b).ok_or(Incomplete::InvalidState)?;
                if let Some(statement) = body.statements().get(control.statement) {
                    match statement {
                        Statement::Let { local, initializer, .. } => {
                            p.operands = Operands::One(initializer);
                            p.anchor = Anchor::Commit(frame, Place::Local(*local), None);
                        }
                        Statement::Assign { local, value, .. } => {
                            p.operands = Operands::One(value);
                            p.anchor = Anchor::Commit(frame, Place::Local(*local), None);
                        }
                        Statement::ReplaceOwned { value, .. } => { p.operands = Operands::One(value); p.anchor = Anchor::Kind(12); }
                        Statement::CopyFieldAssign { owner, record, field, value, .. } => {
                            p.operands = Operands::One(value);
                            p.anchor = Anchor::Commit(frame, Place::Local(*owner), Some((*record, *field)));
                        }
                        Statement::DerefAssign { reference, value, .. }
                        | Statement::CopyReferenceFieldAssign { reference, value, .. } => {
                            let mut found = None;
                            for slot in &self.scratch.slots {
                                self.work.spend()?;
                                if slot.frame == frame && slot.place == *reference { found = slot.reference; }
                            }
                            let reference = found.ok_or(Incomplete::InvalidState)?;
                            let field = if let Statement::CopyReferenceFieldAssign { record, field, .. } = statement { Some((*record, *field)) } else { None };
                            p.operands = Operands::One(value);
                            p.anchor = Anchor::Commit(reference.frame, reference.place, field);
                        }
                        Statement::CallNoValue { function, arguments, .. } => {
                            p.operands = Operands::Arguments(arguments);
                            p.anchor = Anchor::Call(*function, arguments.len());
                        }
                        Statement::BindResult { .. } => (),
                    }
                } else {
                    require(control.statement == body.statements().len())?;
                    match body.terminator() {
                        Terminator::Return { value } => { p.operands = Operands::One(value); p.anchor = Anchor::Kind(14); }
                        Terminator::Branch { condition, then_target, else_target } => { p.operands = Operands::One(condition); p.anchor = Anchor::Branch(*then_target, *else_target); }
                        Terminator::Match { scrutinee, result, ok_target, error_target } => { p.operands = Operands::One(scrutinee); p.anchor = Anchor::Select(*result, *ok_target, *error_target); }
                        Terminator::Propagate { initializer, operation, ok_target, error_target, .. } => { p.operands = Operands::One(initializer); p.anchor = Anchor::Select(operation.inner, *ok_target, *error_target); }
                        _ => (),
                    }
                }
            }
            1 | 2 => {
                let expression = self.task_expression(start, end)?;
                p.depth = d[1].checked_add(1).ok_or(Incomplete::InvalidState)?;
                match &expression.kind {
                    ExprKind::Unary { operand, .. } => { p.operands = Operands::One(operand); p.anchor = Anchor::Kind(6); }
                    ExprKind::Binary { op, left, right } => {
                        p.operands = Operands::Binary(left, right);
                        p.anchor = if matches!(op, BinaryOp::And | BinaryOp::Or) { Anchor::Short(*op) } else { Anchor::Binary(*op, expression.ty) };
                    }
                    ExprKind::Call { function, arguments } => { p.operands = Operands::Arguments(arguments); p.anchor = Anchor::Call(*function, arguments.len()); }
                    ExprKind::TrustedCall { arguments, .. } => { p.operands = Operands::Arguments(arguments); p.anchor = Anchor::Kind(22); }
                    ExprKind::RecordConstruct { record, fields } => { p.operands = Operands::Fields(fields); p.fields = Some(fields); p.anchor = Anchor::Record(*record); }
                    ExprKind::ResultConstruct { payload, .. } => { p.operands = Operands::One(payload); p.anchor = Anchor::Kind(20); }
                    _ => (),
                }
            }
            5 | 8 => { p.operands = Operands::One(self.task_expression(start, end)?); p.depth = d[1]; }
            16 | 18 | 19 => {
                let fields = self.canonical_fields(start, end)?;
                p.fields = Some(fields);
                if d[0] != 19 {
                    let index = if d[0] == 16 { d[2] as usize } else { self.node(key(Kind::Staging, d[1]))?.statement };
                    if let Some(field) = fields.get(index) {
                        p.operands = Operands::One(&field.value);
                        p.depth = if d[0] == 16 { d[3] } else { d[2] }.checked_add(1).ok_or(Incomplete::InvalidState)?;
                        p.anchor = if d[0] == 16 { Anchor::Field(field.field) } else { Anchor::Kind(19) };
                    } else { require(index == fields.len())?; }
                }
            }
            _ => (),
        }
        p.ready = matches!(p.anchor, Anchor::None);
        self.producer = Some(p);
        Ok(())
    }
    pub(super) fn bind_queued_task(&mut self, start: usize, end: usize) -> Valid {
        let Record::Continuation(frame) = self.records[start] else { return Err(Incomplete::InvalidState); };
        let Record::Meta(Part::Task, d) = self.records[start + 1] else { return Err(Incomplete::InvalidState); };
        let Some(mut p) = self.producer else {
            return protection(Protection::Operand, d[0] == 0);
        };
        if matches!(d[0], 9 | 16 | 18 | 19) {
            let fields = p.fields.ok_or(Incomplete::InvalidState)?;
            let mut at = start + 2;
            for field in fields {
                self.work.spend()?;
                protection(Protection::Operand, self.records.get(at) == Some(&Record::Meta(Part::Initializer, { let mut r = [ABSENT; 8]; r[0] = field.field.0 as u64; r })))?;
                let Some(Record::Expression(ordinal)) = self.records.get(at + 1) else { return Err(Incomplete::InvalidState); };
                let expression = self.canonical_expression(*ordinal)?;
                protection(Protection::Operand, std::ptr::eq(expression, &field.value))?;
                at += 2;
            }
            protection(Protection::Operand, at + 2 == end && self.records[at] == Record::End)?;
        }
        let anchor = match p.anchor {
            Anchor::None => false,
            Anchor::Kind(kind) => d[0] == kind,
            Anchor::Commit(at, place, field) => d[0] == 11 && self.records.get(start + 2) == Some(&Record::Destination(at, place, field)),
            Anchor::Call(function, count) => d[0] == 10 && d[1] == function.0 as u64 && d[2] == count as u64,
            Anchor::Branch(yes, no) => d[0] == 13 && d[1] == yes.0 as u64 && d[2] == no.0 as u64,
            Anchor::Select(ty, yes, no) => d[0] == 21 && d[1] == ty.0 as u64 && d[2] == yes.0 as u64 && d[3] == no.0 as u64,
            Anchor::Binary(op, ty) => d[0] == 7 && self.records.get(start + 2) == Some(&Record::Binary(op)) && self.records.get(start + 3) == Some(&Record::ExprType(ty)),
            Anchor::Record(record) => d[0] == 9 && d[1] == record.0 as u64 || matches!(d[0], 16 | 18),
            Anchor::Field(field) => d[0] == 17 && d[2] == field.0 as u64,
            Anchor::Short(op) => matches!(d[0], 5 | 8) && self.records.get(start + 2) == Some(&Record::Binary(op)),
        };
        if anchor {
            protection(Protection::Operand, frame == p.frame && !p.ready)?;
            p.ready = true;
            if matches!(p.anchor, Anchor::Record(_)) && matches!(d[0], 16 | 18) { p.operands = Operands::None; }
        }
        if matches!(d[0], 1 | 2 | 5 | 8) {
            let expected = p.operands.get(p.next).ok_or(Incomplete::InvalidState)?;
            let expression = self.task_expression(start, end)?;
            let short = matches!(p.anchor, Anchor::Short(_)) && p.next == 0;
            protection(Protection::Operand, p.ready && frame == p.frame && p.depth == d[1]
                && std::ptr::eq(expression, expected) && short == matches!(d[0], 5 | 8))?;
            p.next += 1;
        }
        self.producer = Some(p);
        Ok(())
    }
    pub(super) fn same_task_records(&mut self, expected: (usize, usize), actual: (usize, usize)) -> Valid {
        let left = self.records.get(expected.0..expected.1).ok_or(Incomplete::InvalidState)?;
        let right = self.records.get(actual.0..actual.1).ok_or(Incomplete::InvalidState)?;
        protection(Protection::Queue, left.len() == right.len())?;
        for (left, right) in left.iter().zip(right) {
            self.work.spend()?;
            protection(Protection::Queue, left == right)?;
        }
        Ok(())
    }
}
