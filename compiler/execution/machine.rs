use super::*;
use crate::hir::{BinaryOp, BorrowKind, ExprKind, ExprType, Place, ReferentType, TypedExpr};
use crate::mir::{BasicBlockId, OriginKind, Statement, Terminator};
use std::mem::size_of;
mod cleanup;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Location {
    frame: u64,
    place: Place,
}
#[derive(Debug, PartialEq, Eq)]
enum Value {
    Leaf {
        resource: hir::resources::ResourceId,
        record: hir::RecordId,
        identity: u64,
    },
    Aggregate {
        record: hir::RecordId,
        fields: Vec<(hir::FieldId, Value)>,
    },
    Public(RuntimeValue),
    Raw(i64),
    Reference {
        kind: BorrowKind,
        referent: ReferentType,
        location: Location,
    },
    Completion,
}
impl Value {
    fn copy(&self) -> Option<Self> {
        match self {
            Self::Public(RuntimeValue::Bool(v)) => Some(Self::Public(RuntimeValue::Bool(*v))),
            Self::Public(RuntimeValue::Range(v)) => {
                Some(Self::Public(RuntimeValue::Range(v.clone())))
            }
            Self::Reference {
                kind: BorrowKind::Shared,
                referent,
                location,
            } => Some(Self::Reference {
                kind: BorrowKind::Shared,
                referent: *referent,
                location: *location,
            }),
            _ => None,
        }
    }
    fn extra_storage(&self) -> usize {
        match self {
            Self::Aggregate { fields, .. } => fields
                .iter()
                .try_fold(
                    fields
                        .capacity()
                        .checked_mul(size_of::<(hir::FieldId, Value)>())
                        .unwrap_or(usize::MAX),
                    |sum, (_, v)| sum.checked_add(v.extra_storage()),
                )
                .unwrap_or(usize::MAX),
            Self::Public(RuntimeValue::Record { fields, .. }) => fields
                .capacity()
                .checked_mul(size_of::<(hir::FieldId, RangeValue)>())
                .unwrap_or(usize::MAX),
            _ => 0,
        }
    }
}
#[derive(Debug)]
enum Slot {
    Uninitialized,
    Available(Value),
    Vacant,
    Transferred,
    Cleaned,
    Replacing,
}
#[derive(Debug)]
struct Frame {
    id: u64,
    function: FunctionId,
    block: BasicBlockId,
    statement: usize,
    slots: Vec<(Place, Slot)>,
    call_site: Span,
}
#[derive(Clone, Copy)]
enum Destination {
    Slot(Location),
    Field(Location, hir::RecordId, hir::FieldId),
}
enum Action<'a> {
    Drive,
    Eval(&'a TypedExpr, usize),
    Negate,
    Binary(BinaryOp, ExprType),
    ShortCircuit(BinaryOp, &'a TypedExpr, usize),
    Record(hir::RecordId, &'a [hir::RecordFieldInit]),
    Invoke(FunctionId, usize),
    Commit(Destination),
    ReplaceCommit(u64),
    Select(BasicBlockId, BasicBlockId),
    Return,
    DropNoValue,
    PendingNext(u64, &'a [hir::RecordFieldInit], usize, usize),
    PendingField(u64, hir::FieldId),
}
struct Task<'a> {
    frame: u64,
    origin: Origin,
    expression: Option<Span>,
    action: Action<'a>,
}

pub(super) struct Machine<'a> {
    executable: &'a CheckedExecutable,
    limits: ExecutionLimits,
    frames: Vec<Frame>,
    tasks: Vec<Task<'a>>,
    values: Vec<Value>,
    next_frame: u64,
    steps: u64,
    origin: Origin,
    expression: Option<Span>,
    resources: Option<cleanup::State>,
    #[cfg(test)]
    max_storage: std::cell::Cell<usize>,
}
impl<'a> Machine<'a> {
    pub fn new(executable: &'a CheckedExecutable, limits: ExecutionLimits) -> Self {
        Self {
            executable,
            limits,
            frames: Vec::new(),
            tasks: Vec::new(),
            values: Vec::new(),
            resources: None,
            #[cfg(test)]
            max_storage: std::cell::Cell::new(0),
            next_frame: 0,
            steps: 0,
            origin: Origin {
                span: START,
                kind: OriginKind::FunctionBoundary,
            },
            expression: None,
        }
    }
    fn failure(&self, kind: ErrorKind) -> ExecutionError {
        let mut e = ExecutionError::new(kind, self.origin.span);
        e.expression = self.expression;
        let count = self.frames.len().min(self.limits.trace_depth);
        // Error construction is independently bounded. Failure to allocate a
        // trace reports host failure with the scalar immediate origin intact.
        if e.trace.try_reserve_exact(count).is_err() {
            e.kind = ErrorKind::Host("call trace allocation");
            return e;
        }
        for f in self.frames.iter().rev().take(count) {
            e.trace.push(CallTrace {
                function: f.function,
                call_site: f.call_site,
            });
        }
        e.trace_truncated = self.frames.len() > count;
        e
    }
    fn integrity(&self, message: &'static str) -> ExecutionError {
        self.failure(ErrorKind::Integrity(message))
    }
    fn charge(&mut self) -> Result<(), ExecutionError> {
        let next = self
            .steps
            .checked_add(1)
            .ok_or_else(|| self.failure(ErrorKind::ConfiguredLimit("steps")))?;
        if next > self.limits.steps {
            return Err(self.failure(ErrorKind::ConfiguredLimit("steps")));
        }
        self.steps = next;
        Ok(())
    }
    fn storage(&self) -> Option<usize> {
        let mut n = self
            .frames
            .capacity()
            .checked_mul(size_of::<Frame>())?
            .checked_add(self.tasks.capacity().checked_mul(size_of::<Task<'_>>())?)?
            .checked_add(self.values.capacity().checked_mul(size_of::<Value>())?)?;
        for f in &self.frames {
            n = n.checked_add(f.slots.capacity().checked_mul(size_of::<(Place, Slot)>())?)?;
            for (_, s) in &f.slots {
                if let Slot::Available(v) = s {
                    n = n.checked_add(v.extra_storage())?;
                }
            }
        }
        for v in &self.values {
            n = n.checked_add(v.extra_storage())?;
        }
        if let Some(s) = &self.resources {
            n = n.checked_add(s.storage()?)?;
        }
        Some(n)
    }
    fn reserve(&self, bytes: usize) -> Result<(), ExecutionError> {
        let wanted = self.storage().and_then(|n| n.checked_add(bytes));
        #[cfg(test)]
        if let Some(n) = wanted {
            self.max_storage.set(self.max_storage.get().max(n));
        }
        if wanted.map_or(true, |n| n > self.limits.working_storage)
        {
            return Err(self.failure(ErrorKind::ConfiguredLimit("working storage")));
        }
        Ok(())
    }
    fn task(
        &mut self,
        frame: u64,
        origin: Origin,
        expression: Option<Span>,
        action: Action<'a>,
    ) -> Result<(), ExecutionError> {
        if self.tasks.len() == self.tasks.capacity() {
            self.reserve(size_of::<Task<'_>>())?;
            self.tasks
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("continuation allocation")))?;
        }
        self.tasks.push(Task {
            frame,
            origin,
            expression,
            action,
        });
        Ok(())
    }
    fn resource_bearing(&self, value: &Value) -> bool {
        matches!(value, Value::Leaf { record, .. } | Value::Aggregate { record, .. }
            if self.executable.cleanup_plans.get(record.0).is_some_and(Option::is_some))
    }
    // Hold ownership in an already reserved machine slot before any fallible
    // receiving-container operation. The caller takes it only after reservation.
    fn retain_publication(&mut self, value: Value) -> Result<Option<Value>, ExecutionError> {
        if self.resource_bearing(&value) {
            let state = self.resources.as_mut().ok_or_else(|| {
                ExecutionError::integrity("resource publication without runtime state")
            })?;
            if state.detached.is_some() {
                return Err(ExecutionError::integrity(
                    "publication payload already occupied",
                ));
            }
            state.detached = Some(value);
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }
    fn take_publication(&mut self, ordinary: Option<Value>) -> Value {
        ordinary.unwrap_or_else(|| self.resources.as_mut().unwrap().detached.take().unwrap())
    }
    fn value(&mut self, value: Value) -> Result<(), ExecutionError> {
        let ordinary = self.retain_publication(value)?;
        let grow = self.values.len() == self.values.capacity();
        // Retained resources already participate in storage(); ordinary payloads
        // remain prospective until publication, preserving predecessor accounting.
        self.reserve(
            ordinary
                .as_ref()
                .map_or(0, Value::extra_storage)
                .checked_add(if grow { size_of::<Value>() } else { 0 })
                .ok_or_else(|| self.failure(ErrorKind::ConfiguredLimit("working storage")))?,
        )?;
        if grow {
            self.values
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("value allocation")))?;
        }
        let value = self.take_publication(ordinary);
        self.values.push(value);
        Ok(())
    }
    fn pop(&mut self) -> Result<Value, ExecutionError> {
        self.values
            .pop()
            .ok_or_else(|| self.integrity("missing evaluated value"))
    }
    fn frame_index(&self, id: u64) -> Result<usize, ExecutionError> {
        self.frames
            .iter()
            .position(|f| f.id == id)
            .ok_or_else(|| self.integrity("retired or unknown runtime frame"))
    }
    fn slot(&self, at: Location) -> Result<&Value, ExecutionError> {
        let f = &self.frames[self.frame_index(at.frame)?];
        match f.slots.iter().find(|(p, _)| *p == at.place).map(|(_, s)| s) {
            Some(Slot::Available(v)) => Ok(v),
            _ => Err(self.integrity("unavailable runtime slot")),
        }
    }
    fn slot_index(&mut self, at: Location) -> Result<(usize, usize), ExecutionError> {
        self.slot_index_with_payload(at, 0)
    }
    fn slot_index_with_payload(
        &mut self,
        at: Location,
        payload: usize,
    ) -> Result<(usize, usize), ExecutionError> {
        let f = self.frame_index(at.frame)?;
        if let Some(i) = self.frames[f]
            .slots
            .iter()
            .position(|(p, _)| *p == at.place)
        {
            self.reserve(payload)?;
            return Ok((f, i));
        }
        let grow = self.frames[f].slots.len() == self.frames[f].slots.capacity();
        self.reserve(
            payload
                .checked_add(if grow { size_of::<(Place, Slot)>() } else { 0 })
                .ok_or_else(|| self.failure(ErrorKind::ConfiguredLimit("working storage")))?,
        )?;
        if grow {
            self.frames[f]
                .slots
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("slot allocation")))?;
        }
        self.frames[f].slots.push((at.place, Slot::Uninitialized));
        Ok((f, self.frames[f].slots.len() - 1))
    }
    fn rvalue(&mut self, at: Location) -> Result<Value, ExecutionError> {
        if let Some(v) = self.slot(at)?.copy() {
            return Ok(v);
        }
        let (f, i) = self.slot_index(at)?;
        let vacant = if matches!(
            self.frames[f].slots[i].1,
            Slot::Available(Value::Leaf { .. } | Value::Aggregate { .. })
        ) {
            Slot::Transferred
        } else {
            Slot::Vacant
        };
        match std::mem::replace(&mut self.frames[f].slots[i].1, vacant) {
            Slot::Available(v) => Ok(v),
            _ => Err(self.integrity("unavailable Move source")),
        }
    }
    fn reference(
        &self,
        at: Location,
        write: bool,
    ) -> Result<(Location, ReferentType), ExecutionError> {
        match self.slot(at)? {
            Value::Reference {
                kind,
                referent,
                location,
            } if !write || *kind == BorrowKind::Mutable => {
                self.slot(*location)?;
                Ok((*location, *referent))
            }
            _ => Err(self.integrity("invalid runtime reference access")),
        }
    }
    fn read_field(
        &self,
        at: Location,
        record: hir::RecordId,
        field: hir::FieldId,
    ) -> Result<Value, ExecutionError> {
        match self.slot(at)? {
            Value::Public(RuntimeValue::Record { record: r, fields }) if *r == record => fields
                .iter()
                .find(|(f, _)| *f == field)
                .map(|(_, v)| Value::Public(RuntimeValue::Range(v.clone())))
                .ok_or_else(|| self.integrity("runtime record field missing")),
            Value::Aggregate { record: r, fields } if *r == record => {
                match fields.iter().find(|(id, _)| *id == field).map(|(_, v)| v) {
                    Some(Value::Public(RuntimeValue::Range(v))) => {
                        Ok(Value::Public(RuntimeValue::Range(v.clone())))
                    }
                    _ => Err(self.integrity("Copy-field is not an owned range")),
                }
            }
            _ => Err(self.integrity("runtime record identity mismatch")),
        }
    }
    fn publish(&mut self, destination: Destination, value: Value) -> Result<(), ExecutionError> {
        match destination {
            Destination::Slot(at) => {
                let ordinary = self.retain_publication(value)?;
                let (f, i) = self.slot_index_with_payload(
                    at,
                    ordinary.as_ref().map_or(0, Value::extra_storage),
                )?;
                let value = self.take_publication(ordinary);
                self.frames[f].slots[i].1 = Slot::Available(value);
            }
            Destination::Field(at, record, field) => {
                let Value::Public(RuntimeValue::Range(value)) = value else {
                    return Err(self.integrity("field RHS is not a range"));
                };
                let (f, i) = self.slot_index(at)?;
                if let Slot::Available(Value::Aggregate { record: r, fields }) =
                    &mut self.frames[f].slots[i].1
                {
                    if *r != record {
                        return Err(self.integrity("field owner nominal mismatch"));
                    }
                    let Some((_, Value::Public(RuntimeValue::Range(old)))) =
                        fields.iter_mut().find(|(id, _)| *id == field)
                    else {
                        return Err(self.integrity("Copy-field destination is not a range"));
                    };
                    if old.ty != value.ty {
                        return Err(self.integrity("field range nominal mismatch"));
                    }
                    *old = value;
                    return Ok(());
                }
                let Slot::Available(Value::Public(RuntimeValue::Record { record: r, fields })) =
                    &mut self.frames[f].slots[i].1
                else {
                    return Err(self.integrity("field owner unavailable"));
                };
                if *r != record {
                    return Err(self.integrity("field owner nominal mismatch"));
                }
                let Some((_, old)) = fields.iter_mut().find(|(f, _)| *f == field) else {
                    return Err(self.integrity("field destination missing"));
                };
                if old.ty != value.ty {
                    return Err(self.integrity("field range nominal mismatch"));
                }
                *old = value;
            }
        }
        Ok(())
    }
    fn enter(&mut self, function: FunctionId, count: usize) -> Result<u64, ExecutionError> {
        // Stable precedence: entry fuel, call depth, storage, host allocation.
        self.charge()?;
        if self.frames.len() >= self.limits.call_depth {
            return Err(self.failure(ErrorKind::ConfiguredLimit("call depth")));
        }
        let definition = self
            .executable
            .program
            .functions()
            .get(function.0)
            .and_then(|f| f.as_ordinary())
            .ok_or_else(|| self.integrity("invalid ordinary callee"))?;
        if definition.id != function
            || definition.parameters.len() != count
            || self.values.len() < count
        {
            return Err(self.integrity("runtime call signature mismatch"));
        }
        let mir = self.executable.function(function)?;
        if mir.cleanup.is_some() {
            self.enable_resources()?;
        }
        let bytes = count
            .checked_mul(size_of::<(Place, Slot)>())
            .and_then(|n| {
                n.checked_add(if self.frames.len() == self.frames.capacity() {
                    size_of::<Frame>()
                } else {
                    0
                })
            })
            .ok_or_else(|| self.failure(ErrorKind::ConfiguredLimit("working storage")))?;
        self.reserve(bytes)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(count)
            .map_err(|_| self.failure(ErrorKind::Host("parameter allocation")))?;
        if self.frames.len() == self.frames.capacity() {
            self.frames
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("frame allocation")))?;
        }
        let id = self.next_frame;
        self.next_frame = self
            .next_frame
            .checked_add(1)
            .ok_or_else(|| self.integrity("frame identity exhausted"))?;
        let start = self.values.len() - count;
        for (parameter, value) in definition.parameters.iter().zip(self.values.drain(start..)) {
            slots.push((Place::Parameter(*parameter), Slot::Available(value)));
        }
        self.frames.push(Frame {
            id,
            function,
            block: mir.entry(),
            statement: 0,
            slots,
            call_site: self.expression.unwrap_or(self.origin.span),
        });
        self.cleanup_frame(id)?;
        Ok(id)
    }
    fn eval(&mut self, frame: u64, e: &'a TypedExpr, depth: usize) -> Result<(), ExecutionError> {
        self.task(frame, self.origin, Some(e.span), Action::Eval(e, depth))
    }
    fn operands(
        &mut self,
        frame: u64,
        args: impl DoubleEndedIterator<Item = &'a TypedExpr>,
        depth: usize,
    ) -> Result<(), ExecutionError> {
        for e in args.rev() {
            self.eval(frame, e, depth)?;
        }
        Ok(())
    }
    fn drive(&mut self, id: u64) -> Result<(), ExecutionError> {
        let index = self.frame_index(id)?;
        self.cleanup_enter_block(id)?;
        let frame = &self.frames[index];
        let f = self.executable.function(frame.function)?;
        let block = f.block(frame.block);
        let origin = self.executable.origin(frame.function, frame.block)?;
        if let Some(statement) = block.statements().get(frame.statement) {
            let span = match statement {
                Statement::CallNoValue { span, .. }
                | Statement::CopyReferenceFieldAssign { span, .. }
                | Statement::CopyFieldAssign { span, .. }
                | Statement::Assign { span, .. }
                | Statement::ReplaceOwned { span, .. }
                | Statement::Let { span, .. }
                | Statement::DerefAssign { span, .. } => *span,
            };
            self.origin = Origin {
                span,
                kind: OriginKind::Statement,
            };
            self.expression = None;
            self.charge()?;
            self.frames[index].statement += 1;
            self.task(id, self.origin, None, Action::Drive)?;
            match statement {
                Statement::CallNoValue {
                    function,
                    arguments,
                    ..
                } => {
                    self.task(id, self.origin, None, Action::DropNoValue)?;
                    self.task(
                        id,
                        self.origin,
                        None,
                        Action::Invoke(*function, arguments.len()),
                    )?;
                    self.operands(id, arguments.iter(), 1)?;
                }
                Statement::Let {
                    local, initializer, ..
                } => {
                    let at = Location {
                        frame: id,
                        place: Place::Local(*local),
                    };
                    self.cleanup_reinitialization(at)?;
                    let (f, i) = self.slot_index(at)?;
                    self.frames[f].slots[i].1 = Slot::Uninitialized;
                    self.task(id, self.origin, None, Action::Commit(Destination::Slot(at)))?;
                    self.eval(id, initializer, 1)?;
                }
                Statement::Assign { local, value, .. } => {
                    self.task(
                        id,
                        self.origin,
                        None,
                        Action::Commit(Destination::Slot(Location {
                            frame: id,
                            place: Place::Local(*local),
                        })),
                    )?;
                    self.eval(id, value, 1)?;
                }
                Statement::ReplaceOwned {
                    local,
                    record,
                    owning_scope,
                    cleanup_plan,
                    value,
                    origin,
                    ..
                } => {
                    self.origin = *origin;
                    let replacement = self.replacement_start(
                        Location {
                            frame: id,
                            place: Place::Local(*local),
                        },
                        *record,
                        *owning_scope,
                        *cleanup_plan,
                    )?;
                    self.task(id, *origin, None, Action::ReplaceCommit(replacement))?;
                    self.eval(id, value, 1)?;
                }
                Statement::CopyFieldAssign {
                    owner,
                    record,
                    field,
                    value,
                    ..
                } => {
                    self.task(
                        id,
                        self.origin,
                        None,
                        Action::Commit(Destination::Field(
                            Location {
                                frame: id,
                                place: Place::Local(*owner),
                            },
                            *record,
                            *field,
                        )),
                    )?;
                    self.eval(id, value, 1)?;
                }
                Statement::DerefAssign {
                    reference, value, ..
                } => {
                    let (at, _) = self.reference(
                        Location {
                            frame: id,
                            place: *reference,
                        },
                        true,
                    )?;
                    self.task(id, self.origin, None, Action::Commit(Destination::Slot(at)))?;
                    self.eval(id, value, 1)?;
                }
                Statement::CopyReferenceFieldAssign {
                    reference,
                    record,
                    field,
                    value,
                    ..
                } => {
                    let (at, r) = self.reference(
                        Location {
                            frame: id,
                            place: *reference,
                        },
                        true,
                    )?;
                    if r != ReferentType::Record(*record) {
                        return Err(self.integrity("reference record mismatch"));
                    }
                    self.task(
                        id,
                        self.origin,
                        None,
                        Action::Commit(Destination::Field(at, *record, *field)),
                    )?;
                    self.eval(id, value, 1)?;
                }
            }
        } else {
            self.origin = origin;
            self.expression = None;
            self.charge()?;
            match block.terminator() {
                Terminator::Goto { target } => {
                    self.cleanup_exit(id)?;
                    self.cleanup_next_block(id)?;
                    self.frames[index].block = *target;
                    self.frames[index].statement = 0;
                    self.task(id, origin, None, Action::Drive)?;
                }
                Terminator::Branch {
                    condition,
                    then_target,
                    else_target,
                } => {
                    self.task(id, origin, None, Action::Select(*then_target, *else_target))?;
                    self.eval(id, condition, 1)?;
                }
                Terminator::Return { value } => {
                    self.task(id, origin, None, Action::Return)?;
                    self.eval(id, value, 1)?;
                }
                Terminator::ReturnNoValue => {
                    self.value(Value::Completion)?;
                    self.task(id, origin, None, Action::Return)?;
                }
            }
        }
        Ok(())
    }
    fn expression(
        &mut self,
        frame: u64,
        e: &'a TypedExpr,
        depth: usize,
    ) -> Result<(), ExecutionError> {
        // Expression fuel precedes expression depth; no child starts on failure.
        self.charge()?;
        if depth > self.limits.expression_depth {
            return Err(self.failure(ErrorKind::ConfiguredLimit("expression depth")));
        }
        match &e.kind {
            ExprKind::BoolLiteral(v) => self.value(Value::Public(RuntimeValue::Bool(*v)))?,
            ExprKind::IntegerLiteral(v) => self.value(Value::Raw(*v))?,
            ExprKind::FormedRangeLiteral(v) => {
                let ExprType::Range(ty) = e.ty else {
                    return Err(self.integrity("formed literal type"));
                };
                self.range(ty, *v as i128)?;
            }
            ExprKind::Parameter(p) => {
                let v = self.rvalue(Location {
                    frame,
                    place: Place::Parameter(*p),
                })?;
                self.value(v)?;
            }
            ExprKind::Local(p) => {
                let v = self.rvalue(Location {
                    frame,
                    place: Place::Local(*p),
                })?;
                self.value(v)?;
            }
            ExprKind::Borrow { kind, place } => {
                let at = Location {
                    frame,
                    place: *place,
                };
                self.slot(at)?;
                let referent = match e.ty {
                    ExprType::SharedRef(r) | ExprType::MutableRef(r) => r,
                    _ => return Err(self.integrity("borrow type")),
                };
                self.value(Value::Reference {
                    kind: *kind,
                    referent,
                    location: at,
                })?;
            }
            ExprKind::Deref { reference } => {
                let (at, _) = self.reference(
                    Location {
                        frame,
                        place: *reference,
                    },
                    false,
                )?;
                let v = self
                    .slot(at)?
                    .copy()
                    .ok_or_else(|| self.integrity("non-Copy dereference"))?;
                self.value(v)?;
            }
            ExprKind::CopyFieldRead {
                owner,
                record,
                field,
            } => {
                let v = self.read_field(
                    Location {
                        frame,
                        place: *owner,
                    },
                    *record,
                    *field,
                )?;
                self.value(v)?;
            }
            ExprKind::CopyReferenceFieldRead {
                reference,
                record,
                field,
            } => {
                let (at, r) = self.reference(
                    Location {
                        frame,
                        place: *reference,
                    },
                    false,
                )?;
                if r != ReferentType::Record(*record) {
                    return Err(self.integrity("reference field type"));
                }
                let v = self.read_field(at, *record, *field)?;
                self.value(v)?;
            }
            ExprKind::RecordConstruct { record, fields } => {
                if !self.executable.program.resources.is_leaf(*record)
                    && (self
                        .executable
                        .cleanup_plans
                        .get(record.0)
                        .is_some_and(Option::is_some)
                        || self
                            .executable
                            .program
                            .record(*record)
                            .fields
                            .iter()
                            .any(|id| {
                                matches!(
                                    self.executable.program.field(*id).ty,
                                    ValueType::Record(_)
                                )
                            }))
                {
                    self.pending_start(frame, *record, fields, depth)?;
                    return Ok(());
                }
                self.task(
                    frame,
                    self.origin,
                    self.expression,
                    Action::Record(*record, fields),
                )?;
                self.operands(frame, fields.iter().map(|f| &f.value), depth + 1)?;
            }
            ExprKind::Call {
                function,
                arguments,
            } => {
                self.task(
                    frame,
                    self.origin,
                    self.expression,
                    Action::Invoke(*function, arguments.len()),
                )?;
                self.operands(frame, arguments.iter(), depth + 1)?;
            }
            ExprKind::Unary { operand, .. } => {
                self.task(frame, self.origin, self.expression, Action::Negate)?;
                self.eval(frame, operand, depth + 1)?;
            }
            ExprKind::Binary { op, left, right } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    self.task(
                        frame,
                        self.origin,
                        self.expression,
                        Action::ShortCircuit(*op, right, depth + 1),
                    )?;
                    self.eval(frame, left, depth + 1)?;
                } else {
                    self.task(
                        frame,
                        self.origin,
                        self.expression,
                        Action::Binary(*op, e.ty),
                    )?;
                    self.eval(frame, right, depth + 1)?;
                    self.eval(frame, left, depth + 1)?;
                }
            }
            ExprKind::IfValue { .. } | ExprKind::FieldAccess(_) | ExprKind::OldField(_) => {
                return Err(self.integrity("ineligible ordinary expression"))
            }
        }
        Ok(())
    }
    fn range(&mut self, ty: hir::RangeTypeId, computed: i128) -> Result<(), ExecutionError> {
        let range = self
            .executable
            .program
            .ranges()
            .get(ty.0)
            .filter(|r| r.id == ty)
            .ok_or_else(|| self.integrity("range identity"))?;
        if computed < i128::from(range.min) || computed > i128::from(range.max) {
            return Err(self.failure(ErrorKind::RangeTrap {
                ty,
                min: range.min,
                max: range.max,
                computed,
            }));
        }
        self.value(Value::Public(RuntimeValue::Range(RangeValue {
            ty,
            value: computed as i64,
        })))
    }
    fn boolean(&self, value: Value) -> Result<bool, ExecutionError> {
        match value {
            Value::Public(RuntimeValue::Bool(b)) => Ok(b),
            _ => Err(self.integrity("expected Bool")),
        }
    }
    fn number(&self, value: Value) -> Result<i128, ExecutionError> {
        match value {
            Value::Raw(n) => Ok(i128::from(n)),
            Value::Public(RuntimeValue::Range(n)) => Ok(i128::from(n.value)),
            _ => Err(self.integrity("expected integer operand")),
        }
    }
    fn binary(&mut self, op: BinaryOp, ty: ExprType) -> Result<(), ExecutionError> {
        let right = self.pop()?;
        let left = self.pop()?;
        if op == BinaryOp::Equal && matches!(left, Value::Public(RuntimeValue::Bool(_))) {
            let a = self.boolean(left)?;
            let b = self.boolean(right)?;
            return self.value(Value::Public(RuntimeValue::Bool(a == b)));
        }
        let a = self.number(left)?;
        let b = self.number(right)?;
        match op {
            BinaryOp::Subtract => {
                let ExprType::Range(r) = ty else {
                    return Err(self.integrity("subtraction result type"));
                };
                self.range(r, a - b)
            }
            BinaryOp::Equal => self.value(Value::Public(RuntimeValue::Bool(a == b))),
            BinaryOp::LessEqual => self.value(Value::Public(RuntimeValue::Bool(a <= b))),
            _ => Err(self.integrity("invalid eager binary operator")),
        }
    }
    fn finish_record(
        &mut self,
        record: hir::RecordId,
        fields: &[hir::RecordFieldInit],
    ) -> Result<(), ExecutionError> {
        if self.values.len() < fields.len() {
            return Err(self.integrity("record operands missing"));
        }
        // Admitted records are nonempty and their fields are scalar ranges.
        // Draining them frees a value entry for the completed record, so this
        // construction cannot also grow the value buffer. enter() separately
        // reserves parameter and frame headers together while argument payloads
        // remain counted; return publication reuses the entry just popped.
        self.reserve(
            fields
                .len()
                .checked_mul(size_of::<(hir::FieldId, RangeValue)>())
                .ok_or_else(|| self.integrity("record storage overflow"))?,
        )?;
        let mut result = Vec::new();
        result
            .try_reserve_exact(fields.len())
            .map_err(|_| self.failure(ErrorKind::Host("record allocation")))?;
        let start = self.values.len() - fields.len();
        for (field, value) in fields.iter().zip(self.values.drain(start..)) {
            let Value::Public(RuntimeValue::Range(value)) = value else {
                return Err(ExecutionError::new(
                    ErrorKind::Integrity("record field not a range"),
                    self.origin.span,
                ));
            };
            result.push((field.field, value));
        }
        self.value(Value::Public(RuntimeValue::Record {
            record,
            fields: result,
        }))
    }
    fn finish_return(&mut self, id: u64) -> Result<(), ExecutionError> {
        let value = self.pop()?;
        let f = self
            .frames
            .last()
            .filter(|f| f.id == id)
            .ok_or_else(|| self.integrity("return frame mismatch"))?;
        let declared = self
            .executable
            .program
            .function(f.function)
            .as_ordinary()
            .ok_or_else(|| self.integrity("return function"))?
            .return_type;
        let matches = match (&value, declared) {
            (
                Value::Leaf { record, .. } | Value::Aggregate { record, .. },
                Some(ValueType::Record(t)),
            ) => *record == t,
            (Value::Completion, None) => true,
            (Value::Public(RuntimeValue::Bool(_)), Some(ValueType::Bool)) => true,
            (Value::Public(RuntimeValue::Range(v)), Some(ValueType::Range(t))) => v.ty == t,
            (Value::Public(RuntimeValue::Record { record, .. }), Some(ValueType::Record(t))) => {
                *record == t
            }
            _ => false,
        };
        if !matches {
            return Err(self.integrity("return result or escaping reference mismatch"));
        }
        if self.executable.function(f.function)?.cleanup.is_some() {
            return self.cleanup_return(id, value);
        }
        self.value(value)?;
        self.frames.pop();
        Ok(())
    }
    fn step(&mut self, task: Task<'a>) -> Result<(), ExecutionError> {
        if self.resources.as_ref().is_some_and(|s| s.terminal) {
            return Err(self.integrity("resource execution is terminal"));
        }
        self.origin = task.origin;
        self.expression = task.expression;
        match task.action {
            Action::Drive => self.drive(task.frame)?,
            Action::Eval(e, d) => self.expression(task.frame, e, d)?,
            Action::Negate => {
                let v = self.pop()?;
                let b = self.boolean(v)?;
                self.value(Value::Public(RuntimeValue::Bool(!b)))?;
            }
            Action::Binary(op, ty) => self.binary(op, ty)?,
            Action::ShortCircuit(op, right, depth) => {
                let v = self.pop()?;
                let b = self.boolean(v)?;
                if (op == BinaryOp::And && !b) || (op == BinaryOp::Or && b) {
                    self.value(Value::Public(RuntimeValue::Bool(b)))?;
                } else {
                    self.eval(task.frame, right, depth)?;
                }
            }
            Action::Record(r, f) => self.finish_record(r, f)?,
            Action::Invoke(f, n) => {
                if !self.trusted_invoke(f, n)? {
                    let frame = self.enter(f, n)?;
                    self.task(frame, self.origin, None, Action::Drive)?;
                }
            }
            Action::Commit(d) => {
                let v = self.pop()?;
                self.publish(d, v)?;
                if let Destination::Slot(at) = d {
                    self.cleanup_initialized(at)?;
                }
            }
            Action::ReplaceCommit(id) => self.replacement_commit(id)?,
            Action::Select(yes, no) => {
                let v = self.pop()?;
                let b = self.boolean(v)?;
                let index = self.frame_index(task.frame)?;
                self.cleanup_exit(task.frame)?;
                self.cleanup_next_block(task.frame)?;
                self.frames[index].block = if b { yes } else { no };
                self.frames[index].statement = 0;
                self.task(task.frame, task.origin, None, Action::Drive)?;
            }
            Action::PendingNext(id, fields, index, depth) => {
                self.pending_next(task.frame, id, fields, index, depth)?
            }
            Action::PendingField(id, field) => self.pending_field(id, field)?,
            Action::Return => self.finish_return(task.frame)?,
            Action::DropNoValue => {
                if self.pop()? != Value::Completion {
                    return Err(self.integrity("NoValue call result mismatch"));
                }
            }
        }
        Ok(())
    }
    fn start(&mut self, entry: FunctionId) -> Result<(), ExecutionError> {
        if self
            .executable
            .program
            .resources
            .operations
            .iter()
            .any(|op| op.function == entry)
        {
            return Err(self.integrity("trusted operation requires an admitted caller"));
        }
        self.origin = Origin {
            span: self.executable.program.function(entry).span(),
            kind: OriginKind::FunctionBoundary,
        };
        let frame = self.enter(entry, 0)?;
        self.task(frame, self.origin, None, Action::Drive)
    }
    fn resume(&mut self) -> Result<ExecutionResult, ExecutionError> {
        if self.resources.as_ref().is_some_and(|s| s.terminal) {
            return Err(self.integrity("resource execution is terminal"));
        }
        while let Some(task) = self.tasks.pop() {
            if let Err(error) = self.step(task) {
                if let Some(state) = &mut self.resources {
                    state.terminal = true;
                }
                return Err(error);
            }
        }
        if !self.frames.is_empty()
            || self.values.len() != 1
            || self
                .resources
                .as_ref()
                .is_some_and(|s| !s.replacements.is_empty())
        {
            return Err(self.integrity("incomplete execution state"));
        }
        match self.pop()? {
            Value::Completion => Ok(ExecutionResult::NoValue),
            Value::Public(v) => Ok(ExecutionResult::Value(v)),
            _ => Err(self.integrity("ineligible public result")),
        }
    }
    pub fn run(mut self, entry: FunctionId) -> Result<ExecutionResult, ExecutionError> {
        self.start(entry)?;
        self.resume()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detached_record(e: &CheckedExecutable) -> Value {
        let record = &e.program.records()[0];
        let mut fields = Vec::new();
        fields.try_reserve_exact(record.fields.len()).unwrap();
        for id in &record.fields {
            fields.push((
                *id,
                RangeValue {
                    ty: e.program.fields()[id.0].range_type().unwrap(),
                    value: 80,
                },
            ));
        }
        Value::Public(RuntimeValue::Record {
            record: record.id,
            fields,
        })
    }

    const STORAGE_SOURCE: &str = "type P=range 0..100;record R{x:P,y:P}fn demo()->R{let pending=R{x:80,y:80};return pending;}";

    #[test]
    fn p02_detached_record_and_value_buffer_require_combined_budget() {
        let e = artifact(STORAGE_SOURCE);
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.value(Value::Public(RuntimeValue::Bool(true))).unwrap();
        while m.values.len() < m.values.capacity() {
            m.value(Value::Public(RuntimeValue::Bool(true))).unwrap();
        }
        let value = detached_record(&e);
        let payload = value.extra_storage();
        let header = size_of::<Value>();
        let initial = m.storage().unwrap();
        let old_budget = initial.checked_add(payload.max(header)).unwrap();
        let combined = initial
            .checked_add(payload)
            .unwrap()
            .checked_add(header)
            .unwrap();
        assert!(old_budget < combined);
        assert!(initial.checked_add(payload).unwrap() <= old_budget);
        assert!(initial.checked_add(header).unwrap() <= old_budget);
        let capacity = m.values.capacity();
        let count = m.values.len();
        m.limits.working_storage = old_budget;
        assert!(matches!(
            m.value(value),
            Err(ExecutionError {
                kind: ErrorKind::ConfiguredLimit("working storage"),
                ..
            })
        ));
        assert_eq!((m.values.len(), m.values.capacity()), (count, capacity));
        assert_eq!(m.storage(), Some(initial));
        assert!(m.storage().unwrap() <= m.limits.working_storage);
        // The adjacent byte below the complete prospective peak also rejects.
        m.limits.working_storage = combined - 1;
        assert!(m.value(detached_record(&e)).is_err());
        assert_eq!(m.storage(), Some(initial));
        m.limits.working_storage = combined;
        m.value(detached_record(&e)).unwrap();
        assert_eq!(m.values.capacity(), capacity + 1);
        assert_eq!(m.storage(), Some(combined));
        assert!(m.storage().unwrap() <= m.limits.working_storage);
    }

    #[test]
    fn p02_detached_record_and_new_destination_slot_require_combined_budget() {
        let e = artifact(STORAGE_SOURCE);
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert_eq!(m.frames[0].slots.capacity(), 0);
        let pending = e
            .program
            .locals()
            .iter()
            .find(|l| l.name == "pending")
            .unwrap()
            .id;
        let destination = Destination::Slot(Location {
            frame: m.frames[0].id,
            place: Place::Local(pending),
        });
        let value = detached_record(&e);
        let payload = value.extra_storage();
        let header = size_of::<(Place, Slot)>();
        let initial = m.storage().unwrap();
        let old_budget = initial.checked_add(payload.max(header)).unwrap();
        let combined = initial
            .checked_add(payload)
            .unwrap()
            .checked_add(header)
            .unwrap();
        assert!(old_budget < combined);
        assert!(initial.checked_add(payload).unwrap() <= old_budget);
        assert!(initial.checked_add(header).unwrap() <= old_budget);
        for budget in [old_budget, combined - 1] {
            m.limits.working_storage = budget;
            assert!(matches!(
                m.publish(destination, detached_record(&e)),
                Err(ExecutionError {
                    kind: ErrorKind::ConfiguredLimit("working storage"),
                    ..
                })
            ));
            assert!(
                m.frames[0].slots.is_empty(),
                "destination must remain unpublished"
            );
            assert_eq!(m.frames[0].slots.capacity(), 0);
            assert_eq!(m.storage(), Some(initial));
            assert!(m.storage().unwrap() <= m.limits.working_storage);
        }
        m.limits.working_storage = combined;
        m.publish(destination, value).unwrap();
        assert!(matches!(
            slot(&m, "pending"),
            Slot::Available(Value::Public(RuntimeValue::Record { .. }))
        ));
        assert_eq!(m.storage(), Some(combined));
        assert!(m.storage().unwrap() <= m.limits.working_storage);
    }

    #[test]
    fn p02_record_moves_calls_returns_and_construction_stay_within_storage_after_each_step() {
        let e = artifact(concat!(
            "type P=range 0..100;record R{x:P,y:P}",
            "fn take(a:R,b:R)->R{let moved=b;return moved;}",
            "fn wrap(a:R)->R{let moved=a;return moved;}",
            "fn demo()->R{let a=R{x:80,y:70};let b=R{x:60,y:50};",
            "let result=take(wrap(a),b);return result;}"
        ));
        let mut peak = 0;
        for budget in [ExecutionLimits::default().working_storage, 0] {
            let budget = if budget == 0 { peak } else { budget };
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    working_storage: budget,
                    ..ExecutionLimits::default()
                },
            );
            m.start(e.entry("demo").unwrap()).unwrap();
            assert!(m.storage().unwrap() <= budget);
            peak = peak.max(m.storage().unwrap());
            while let Some(task) = m.tasks.pop() {
                m.step(task).unwrap();
                let actual = m.storage().unwrap();
                assert!(
                    actual <= budget,
                    "successful transition exceeded its storage allowance"
                );
                peak = peak.max(actual);
            }
            let result = m.resume().unwrap();
            assert_eq!(
                e.render_result(&result, 65536).unwrap(),
                "R { x: P(60), y: P(50) }"
            );
            assert!(m.storage().unwrap() <= budget);
        }
    }

    fn artifact(s: &str) -> CheckedExecutable {
        prepare_execution(
            &if (s.contains("type P=") || s.contains("type P =")) && !s.contains("fn seed(") {
                format!("{s} fn seed(v:P)->P{{return v;}}")
            } else {
                s.into()
            },
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    fn slot<'a>(m: &'a Machine<'_>, name: &str) -> &'a Slot {
        let id = m
            .executable
            .program
            .locals
            .iter()
            .find(|l| l.name == name)
            .unwrap()
            .id;
        &m.frames[0]
            .slots
            .iter()
            .find(|(p, _)| *p == Place::Local(id))
            .unwrap()
            .1
    }
    const SOURCE:&str="type P=range 0..100;record Pair{a:P,b:P}fn tick(p:&mut P)->P{let previous=*p;*p=previous-1;return previous;}fn demo()->Pair{let mut p=seed(80);let pending=Pair{a:tick(&mut p),b:p-100};return pending;}";
    #[test]
    fn stopped_range_trap_keeps_prefix_and_never_publishes_partial_record() {
        let e = artifact(SOURCE);
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let error = m.resume().unwrap_err();
        assert!(matches!(
            error.kind,
            ErrorKind::RangeTrap { computed: -21, .. }
        ));
        assert!(matches!(
            slot(&m, "p"),
            Slot::Available(Value::Public(RuntimeValue::Range(RangeValue {
                value: 79,
                ..
            })))
        ));
        assert!(matches!(slot(&m, "pending"), Slot::Uninitialized));
        assert!(!error.trace.is_empty());
    }
    #[test]
    fn stopped_fuel_preserves_mutation_and_pending_destination() {
        let e = artifact(SOURCE);
        let mut found = false;
        for fuel in 1..60 {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    steps: fuel,
                    ..ExecutionLimits::default()
                },
            );
            if m.start(e.entry("demo").unwrap()).is_err() {
                continue;
            }
            let error = m.resume().unwrap_err();
            if matches!(error.kind, ErrorKind::ConfiguredLimit("steps"))
                && m.frames.len() == 1
                && m.frames[0].slots.len() >= 2
                && matches!(
                    slot(&m, "p"),
                    Slot::Available(Value::Public(RuntimeValue::Range(RangeValue {
                        value: 79,
                        ..
                    })))
                )
            {
                assert!(matches!(slot(&m, "pending"), Slot::Uninitialized));
                found = true;
                break;
            }
        }
        assert!(found);
    }
    #[test]
    fn incomplete_assignment_keeps_old_destination_and_rhs_prefix() {
        let e=artifact("type P=range 0..100;fn tick(p:&mut P)->P{let previous=*p;*p=previous-1;return previous;}fn demo()->P{let mut p=seed(80);let mut target=seed(70);target=tick(&mut p)-100;return target;}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert!(matches!(
            m.resume(),
            Err(ExecutionError {
                kind: ErrorKind::RangeTrap { .. },
                ..
            })
        ));
        assert!(matches!(
            slot(&m, "p"),
            Slot::Available(Value::Public(RuntimeValue::Range(RangeValue {
                value: 79,
                ..
            })))
        ));
        assert!(matches!(
            slot(&m, "target"),
            Slot::Available(Value::Public(RuntimeValue::Range(RangeValue {
                value: 70,
                ..
            })))
        ));
    }
    #[test]
    fn recursive_frames_have_distinct_ids_and_bounded_traces() {
        let e =
            artifact("fn recurse()->bool{return recurse();} fn demo()->bool{return recurse();}");
        let mut m = Machine::new(
            &e,
            ExecutionLimits {
                call_depth: 8,
                trace_depth: 3,
                ..ExecutionLimits::default()
            },
        );
        m.start(e.entry("demo").unwrap()).unwrap();
        let error = m.resume().unwrap_err();
        assert!(matches!(
            error.kind,
            ErrorKind::ConfiguredLimit("call depth")
        ));
        assert_eq!(m.frames.len(), 8);
        assert_eq!(error.trace.len(), 3);
        assert!(error.trace_truncated);
        for (i, f) in m.frames.iter().enumerate() {
            assert_eq!(f.id, i as u64);
        }
    }
    #[test]
    fn retired_locations_do_not_resolve_new_frames() {
        let e = artifact(
            "fn value()->bool{return true;}fn demo()->bool{let a=value();let b=value();return b;}",
        );
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert_eq!(
            m.resume().unwrap(),
            ExecutionResult::Value(RuntimeValue::Bool(true))
        );
        assert_eq!(m.next_frame, 3);
        assert!(m.frame_index(1).is_err());
        assert!(m.frame_index(2).is_err());
    }
    #[test]
    fn synthetic_edges_and_short_circuit_have_exact_fuel() {
        for (s, expected) in [
            ("fn demo()->bool{return true;}", 3),
            ("fn demo()->bool{return true || false;}", 4),
            ("fn demo()->bool{return false || true;}", 5),
            (
                "fn demo()->bool{let x=if true{true}else{false};return x;}",
                8,
            ),
        ] {
            let e = artifact(s);
            let mut m = Machine::new(&e, ExecutionLimits::default());
            m.start(e.entry("demo").unwrap()).unwrap();
            m.resume().unwrap();
            assert_eq!(m.steps, expected, "{s}");
        }
    }

    #[test]
    fn stopped_expression_depth_preserves_prefix_and_pending_record() {
        let e=artifact("type P=range 0..100;record Pair{a:P,b:P}fn tick(p:&mut P)->P{let previous=*p;*p=previous-1;return previous;}fn demo()->Pair{let mut p=seed(80);let pending=Pair{a:tick(&mut p),b:(p-0)-0};return pending;}");
        let mut m = Machine::new(
            &e,
            ExecutionLimits {
                expression_depth: 3,
                ..ExecutionLimits::default()
            },
        );
        m.start(e.entry("demo").unwrap()).unwrap();
        let error = m.resume().unwrap_err();
        assert!(matches!(
            error.kind,
            ErrorKind::ConfiguredLimit("expression depth")
        ));
        assert!(matches!(
            slot(&m, "p"),
            Slot::Available(Value::Public(RuntimeValue::Range(RangeValue {
                value: 79,
                ..
            })))
        ));
        assert!(matches!(slot(&m, "pending"), Slot::Uninitialized));
    }
    #[test]
    fn handle_parameter_slot_is_distinct_from_active_caller_referent() {
        let e=artifact("type P=range 0..100;fn forever(p:&mut P)->P{while true{}return *p;}fn demo()->P{let mut p=seed(80);return forever(&mut p);}");
        let mut m = Machine::new(
            &e,
            ExecutionLimits {
                steps: 40,
                ..ExecutionLimits::default()
            },
        );
        m.start(e.entry("demo").unwrap()).unwrap();
        let error = m.resume().unwrap_err();
        assert!(matches!(error.kind, ErrorKind::ConfiguredLimit("steps")));
        let frame = m.frames.last().unwrap();
        let Value::Reference { location, kind, .. } = m
            .slot(Location {
                frame: frame.id,
                place: Place::Parameter(
                    e.program
                        .function(frame.function)
                        .as_ordinary()
                        .unwrap()
                        .parameters[0],
                ),
            })
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(*kind, BorrowKind::Mutable);
        assert_ne!(location.frame, frame.id);
        assert_eq!(location.frame, m.frames[0].id);
        assert!(matches!(
            m.slot(*location).unwrap(),
            Value::Public(RuntimeValue::Range(RangeValue { value: 80, .. }))
        ));
    }
}
