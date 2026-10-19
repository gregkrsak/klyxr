//! Flat, non-cloneable Result custody. Operand/slot IDs refer to these actual
//! machine-owned payload cells; metadata and histories never own a second copy.
use super::*;
use hir::{ResultTag, ResultTypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OwnerId(pub(super) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    PayloadPending,
    Active,
    MatchProtected { frame: u64, origin: Span },
    Consumed,
    Delivered,
}

// The bounded site/block namespace uses one tagged scalar, preserving the
// predecessor cell footprint for programs without propagation.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Authority(usize);
enum AuthorityKind {
    Constructor(crate::mir::temporaries::SiteId),
    Propagation(BasicBlockId),
}
impl Authority {
    const PROPAGATION: usize = 1usize << (usize::BITS - 1);
    fn constructor(site: crate::mir::temporaries::SiteId) -> Option<Self> {
        (site.0 < Self::PROPAGATION).then_some(Self(site.0))
    }
    fn propagation(block: BasicBlockId) -> Option<Self> {
        (block.0 < Self::PROPAGATION).then_some(Self(block.0 | Self::PROPAGATION))
    }
    fn kind(self) -> AuthorityKind {
        if self.0 & Self::PROPAGATION == 0 {
            AuthorityKind::Constructor(crate::mir::temporaries::SiteId(self.0))
        } else {
            AuthorityKind::Propagation(BasicBlockId(self.0 & !Self::PROPAGATION))
        }
    }
}
pub(super) struct Cell {
    function: FunctionId,
    authority: Authority,
    pub(super) ty: ResultTypeId,
    pub(super) tag: u8,
    pub(super) payload: Option<Value>,
    pub(super) parent: Option<OwnerId>,
    pub(super) phase: Phase,
    pub(super) origin: Span,
}

struct InertRecord {
    frame: u64,
    record: hir::RecordId,
    start: usize,
    index: usize,
    site: Option<(u64, crate::mir::temporaries::SiteId)>,
    child: Option<u64>,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) cells: Vec<Cell>,
    pub(super) matches: Vec<(u64, OwnerId)>,
    records: Vec<InertRecord>,
}

impl State {
    pub(super) fn storage(&self) -> Option<usize> {
        let base = size_of::<Self>()
            .checked_add(self.cells.capacity().checked_mul(size_of::<Cell>())?)?
            .checked_add(
                self.matches
                    .capacity()
                    .checked_mul(size_of::<(u64, OwnerId)>())?,
            )?;
        let base = base.checked_add(
            self.records
                .capacity()
                .checked_mul(size_of::<InertRecord>())?,
        )?;
        self.cells
            .iter()
            .filter_map(|c| c.payload.as_ref())
            .try_fold(base, |n, v| n.checked_add(v.extra_storage()))
    }
}

impl<'a> Machine<'a> {
    pub(super) fn result_export(&mut self, root: OwnerId) -> Result<RuntimeValue, ExecutionError> {
        // Validate the entire bounded chain before allocation or detachment.
        let mut ids = [root; hir::results::MAX_RESULT_DEPTH];
        let mut count = 0;
        let mut next = root;
        let mut parent = None;
        loop {
            if count == ids.len() || ids[..count].contains(&next) {
                return Err(self.integrity("cyclic or over-depth Result export"));
            }
            let cell = self.result_cell(next)?;
            let tag = Self::result_tag(cell.tag)?;
            if cell.phase != Phase::Active || cell.parent != parent {
                return Err(self.integrity("invalid Result export custody"));
            }
            let expected = self
                .executable
                .program
                .result_payload(cell.ty, tag)
                .ok_or_else(|| self.integrity("invalid Result export type"))?;
            let payload = cell
                .payload
                .as_ref()
                .ok_or_else(|| self.integrity("missing Result export payload"))?;
            if !self.result_value_matches(payload, expected) {
                return Err(self.integrity("Result export payload mismatch"));
            }
            ids[count] = next;
            count += 1;
            match payload {
                Value::Result(inner) => {
                    parent = Some(next);
                    next = *inner;
                }
                Value::Public(
                    RuntimeValue::Bool(_) | RuntimeValue::Range(_) | RuntimeValue::Record { .. },
                ) => break,
                _ => return Err(self.integrity("private resource payload cannot be exported")),
            }
        }
        self.reserve(
            count
                .checked_mul(size_of::<ResultLayer>())
                .ok_or_else(|| self.integrity("Result export size overflow"))?,
        )?;
        #[cfg(test)]
        self.temporary_allocation()?;
        let mut layers = Vec::new();
        layers
            .try_reserve_exact(count)
            .map_err(|_| self.failure(ErrorKind::Host("Result export allocation")))?;
        for id in &ids[..count] {
            let cell = self.result_cell(*id)?;
            layers.push(ResultLayer {
                ty: cell.ty,
                tag: Self::result_tag(cell.tag)?,
                payload: None,
            });
        }
        self.charge()?;
        // All receiving growth and all fallible checks precede this commit.
        let Value::Public(payload) = self.results.as_mut().unwrap().cells[next.0]
            .payload
            .take()
            .unwrap()
        else {
            unreachable!("validated public export leaf")
        };
        let payload = match payload {
            RuntimeValue::Bool(v) => ResultPayload::Bool(v),
            RuntimeValue::Range(v) => ResultPayload::Range(v),
            RuntimeValue::Record { record, fields } => ResultPayload::Record { record, fields },
            RuntimeValue::Result(_) => unreachable!("flat export leaf"),
        };
        layers.last_mut().unwrap().payload = Some(payload);
        for id in &ids[..count] {
            let cell = &mut self.results.as_mut().unwrap().cells[id.0];
            cell.payload = None;
            cell.parent = None;
            cell.phase = Phase::Delivered;
        }
        Ok(RuntimeValue::Result(layers))
    }
    /// Whole-wrapper traversal is a sealed primitive/protocol operation. Ordinary
    /// scope cleanup can never acknowledge a live Result by calling this path.
    pub(super) fn result_cleanup(&mut self, id: OwnerId) -> Result<(), ExecutionError> {
        if !self.executable.domain.primitive() {
            return Err(
                self.integrity("whole Result cleanup requires sealed primitive/protocol authority")
            );
        }
        let cell = self.result_cell(id)?;
        if cell.phase != Phase::Active || cell.parent.is_some() || cell.payload.is_none() {
            return Err(self.integrity("invalid whole Result cleanup custody"));
        }
        let tag = Self::result_tag(cell.tag)?;
        let plan = self
            .executable
            .result_plans
            .get(cell.ty.0)
            .filter(|p| p.result == cell.ty)
            .ok_or_else(|| self.integrity("missing canonical Result variant plan"))?;
        let child = plan.active(tag);
        let expected = self
            .executable
            .program
            .result_payload(cell.ty, tag)
            .ok_or_else(|| self.integrity("invalid Result cleanup payload type"))?;
        if !self.result_value_matches(cell.payload.as_ref().unwrap(), expected) {
            return Err(self.integrity("Result cleanup tag/payload mismatch"));
        }
        // The payload remains in its cell while cleanup continuation growth can
        // fail. Neither an inert alternative nor the other plan child is queued.
        if child.is_some() {
            let state = self
                .resources
                .as_ref()
                .ok_or_else(|| self.integrity("Result cleanup without protocol state"))?;
            if state.work_is_full() {
                self.reserve(size_of::<Value>())?;
                #[cfg(test)]
                self.temporary_allocation()?;
                self.resources
                    .as_mut()
                    .unwrap()
                    .reserve_work()
                    .map_err(|_| {
                        self.failure(ErrorKind::Host("Result cleanup continuation allocation"))
                    })?;
            }
        }
        self.charge()?;
        let payload = self.results.as_mut().unwrap().cells[id.0]
            .payload
            .take()
            .unwrap();
        if let Value::Result(inner) = &payload {
            self.results.as_mut().unwrap().cells[inner.0].parent = None;
        }
        self.results.as_mut().unwrap().cells[id.0].phase = Phase::Consumed;
        if child.is_some() {
            self.resources.as_mut().unwrap().push_reserved_work(payload);
        }
        Ok(())
    }
    /// Public range-field records still use their ordinary representation. Their
    /// child regions retire in order without activating the resource catalog.
    pub(super) fn result_record_start(
        &mut self,
        frame: u64,
        record: hir::RecordId,
        fields: &'a [hir::RecordFieldInit],
        depth: usize,
    ) -> Result<(), ExecutionError> {
        self.enable_results()?;
        if self.results.as_ref().unwrap().records.len()
            == self.results.as_ref().unwrap().records.capacity()
        {
            self.reserve(size_of::<InertRecord>())?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.results
                .as_mut()
                .unwrap()
                .records
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("inert record staging allocation")))?;
        }
        let id = self.results.as_ref().unwrap().records.len();
        self.results.as_mut().unwrap().records.push(InertRecord {
            frame,
            record,
            start: self.values.len(),
            index: 0,
            site: self.site,
            child: None,
        });
        self.task(
            frame,
            self.origin,
            self.expression,
            Action::ResultRecordNext(id, fields, depth),
        )
    }

    pub(super) fn result_record_next(
        &mut self,
        frame: u64,
        id: usize,
        fields: &'a [hir::RecordFieldInit],
        depth: usize,
    ) -> Result<(), ExecutionError> {
        self.charge()?;
        let row = self
            .results
            .as_ref()
            .and_then(|s| s.records.get(id))
            .filter(|r| r.frame == frame && r.child.is_none())
            .ok_or_else(|| self.integrity("invalid inert record staging"))?;
        if self.values.len() != row.start + row.index {
            return Err(self.integrity("inert record operand custody mismatch"));
        }
        let index = row.index;
        let record = row.record;
        if let Some(field) = fields.get(index) {
            let site = self.temporary_child(row.site, index)?;
            self.task(
                frame,
                self.origin,
                self.expression,
                Action::ResultRecordField(id, fields, depth),
            )?;
            let site = if let Some((_, site)) = site {
                let function = self.frames[self.frame_index(frame)?].function;
                let region = self
                    .executable
                    .function(function)?
                    .temporaries
                    .as_ref()
                    .unwrap()
                    .sites[site.0]
                    .region;
                let child = self.temporary_begin(frame, function, region)?;
                self.results.as_mut().unwrap().records[id].child = Some(child);
                Some((child, site))
            } else {
                None
            };
            self.eval_at(frame, &field.value, depth + 1, site)
        } else {
            self.finish_record(record, fields)?;
            if id + 1 != self.results.as_ref().unwrap().records.len() {
                return Err(self.integrity("inert record completion order"));
            }
            self.results.as_mut().unwrap().records.pop();
            Ok(())
        }
    }

    pub(super) fn result_record_field(
        &mut self,
        frame: u64,
        id: usize,
        fields: &'a [hir::RecordFieldInit],
        depth: usize,
    ) -> Result<(), ExecutionError> {
        let row = self
            .results
            .as_ref()
            .and_then(|s| s.records.get(id))
            .filter(|r| r.frame == frame)
            .ok_or_else(|| self.integrity("invalid inert record field staging"))?;
        let field = fields
            .get(row.index)
            .ok_or_else(|| self.integrity("invalid inert record field identity"))?;
        let expected = self.executable.program.field(field.field).ty;
        if !self
            .values
            .last()
            .is_some_and(|v| self.result_value_matches(v, expected))
        {
            return Err(self.integrity("inert record field payload mismatch"));
        }
        if let Some(child) = row.child {
            self.temporary_retire(child)?;
            self.temporary_end(child)?;
        }
        let row = &mut self.results.as_mut().unwrap().records[id];
        row.child = None;
        row.index += 1;
        self.task(
            frame,
            self.origin,
            self.expression,
            Action::ResultRecordNext(id, fields, depth),
        )
    }
    fn enable_results(&mut self) -> Result<(), ExecutionError> {
        if self.results.is_none() {
            self.reserve(size_of::<State>())?;
            self.results = Some(State::default());
        }
        Ok(())
    }

    pub(super) fn result_tag(raw: u8) -> Result<ResultTag, ExecutionError> {
        match raw {
            0 => Ok(ResultTag::Ok),
            1 => Ok(ResultTag::Error),
            _ => Err(ExecutionError::integrity(
                "unknown Result tag; no Err-specific semantic variant exists",
            )),
        }
    }

    pub(super) fn result_start(
        &mut self,
        frame: u64,
        ty: ResultTypeId,
        tag: ResultTag,
    ) -> Result<OwnerId, ExecutionError> {
        if self.executable.program.result_type(ty).is_none() {
            return Err(self.integrity("foreign Result constructor type"));
        }
        self.enable_results()?;
        let function = self.frames[self.frame_index(frame)?].function;
        let constructor = self
            .site
            .ok_or_else(|| self.integrity("Result construction without approved temporary site"))?
            .1;
        if self.results.as_ref().unwrap().cells.len()
            == self.results.as_ref().unwrap().cells.capacity()
        {
            self.reserve(size_of::<Cell>())?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.results
                .as_mut()
                .unwrap()
                .cells
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("Result custody allocation")))?;
        }
        let authority = Authority::constructor(constructor)
            .ok_or_else(|| self.integrity("Result constructor authority index exhausted"))?;
        let cells = &mut self.results.as_mut().unwrap().cells;
        let id = OwnerId(cells.len());
        cells.push(Cell {
            function,
            authority,
            ty,
            tag: match tag {
                ResultTag::Ok => 0,
                ResultTag::Error => 1,
            },
            payload: None,
            parent: None,
            phase: Phase::PayloadPending,
            origin: self.expression.unwrap_or(self.origin.span),
        });
        Ok(id)
    }

    fn result_cell(&self, id: OwnerId) -> Result<&Cell, ExecutionError> {
        let cell = self
            .results
            .as_ref()
            .and_then(|s| s.cells.get(id.0))
            .ok_or_else(|| self.integrity("foreign Result custody identity"))?;
        if let AuthorityKind::Propagation(block) = cell.authority.kind() {
            let function = self.executable.function(cell.function)?;
            let Some(crate::mir::Terminator::ForwardError { operation }) = function
                .blocks()
                .find(|(id, _)| *id == block)
                .map(|(_, b)| b.terminator())
            else {
                return Err(self.integrity("foreign propagation cell authority"));
            };
            if cell.ty != operation.outer
                || Self::result_tag(cell.tag)? != ResultTag::Error
                || cell.origin != operation.syntax.phrase
            {
                return Err(self.integrity("propagation cell tag/type/origin correspondence"));
            }
            return Ok(cell);
        }
        let AuthorityKind::Constructor(constructor) = cell.authority.kind() else {
            unreachable!()
        };
        let flow = self
            .executable
            .function(cell.function)?
            .temporaries
            .as_ref()
            .ok_or_else(|| self.integrity("Result custody without approved temporary flow"))?;
        let site = flow
            .sites
            .get(constructor.0)
            .ok_or_else(|| self.integrity("foreign Result constructor site"))?;
        let tag = Self::result_tag(cell.tag)?;
        if site.ty != ExprType::Result(cell.ty)
            || site.origin.span != cell.origin
            || site.children.len() != 1
            || flow.sites.get(site.children[0].0).map_or(true, |s| {
                s.destination != crate::mir::temporaries::Destination::ResultPayload(cell.ty, tag)
            })
        {
            return Err(self.integrity("Result constructor tag, type or origin correspondence"));
        }
        Ok(cell)
    }

    pub(super) fn result_value_matches(&self, value: &Value, expected: ValueType) -> bool {
        match (value, expected) {
            (Value::Resource { ty: actual, .. }, ValueType::Resource(expected)) => {
                *actual == expected
            }
            (Value::Result(id), ValueType::Result(ty)) => self.result_cell(*id).is_ok_and(|cell| {
                cell.ty == ty
                    && cell.payload.is_some()
                    && cell.phase == Phase::Active
                    && Self::result_tag(cell.tag).is_ok()
            }),
            (Value::Public(RuntimeValue::Bool(_)), ValueType::Bool) => true,
            (Value::Public(RuntimeValue::Range(v)), ValueType::Range(ty)) => v.ty == ty,
            (
                Value::Public(RuntimeValue::Record { record, .. })
                | Value::Leaf { record, .. }
                | Value::Aggregate { record, .. },
                ValueType::Record(ty),
            ) => *record == ty,
            _ => false,
        }
    }

    pub(super) fn result_complete(&mut self, id: OwnerId) -> Result<(), ExecutionError> {
        self.charge()?; // Complete constructor publication, before detachment.
        let cell = self.result_cell(id)?;
        if cell.phase != Phase::PayloadPending
            || cell.payload.is_some()
            || Some(cell.origin) != self.expression
        {
            return Err(self.integrity("duplicate Result constructor publication"));
        }
        let expected = self
            .executable
            .program
            .result_payload(cell.ty, Self::result_tag(cell.tag)?)
            .ok_or_else(|| self.integrity("invalid Result payload role"))?;
        let operand = self
            .values
            .last()
            .ok_or_else(|| self.integrity("missing Result payload"))?;
        let actual = match operand {
            Value::Temporary(owner) => self
                .temporaries
                .as_ref()
                .and_then(|s| s.owned_value(*owner))
                .ok_or_else(|| self.integrity("missing Result payload temporary"))?,
            value => value,
        };
        if !self.result_value_matches(actual, expected) {
            return Err(self.integrity("Result tag/payload type mismatch"));
        }
        if let Value::Result(inner) = actual {
            if *inner == id || self.result_cell(*inner)?.parent.is_some() {
                return Err(self.integrity("Result payload already has a parent owner"));
            }
        }
        // pop() can fail only before its temporary payload detaches. On success
        // the following commit allocates nothing and reuses the vacated operand.
        let cell_type = self.result_cell(id)?.ty;
        let cell_tag = Self::result_tag(self.result_cell(id)?.tag)?;
        let payload = self.pop_to(Some(crate::mir::temporaries::Destination::ResultPayload(
            cell_type, cell_tag,
        )))?;
        if let Value::Result(inner) = &payload {
            self.results.as_mut().unwrap().cells[inner.0].parent = Some(id);
        }
        let cell = &mut self.results.as_mut().unwrap().cells[id.0];
        cell.payload = Some(payload);
        cell.phase = Phase::Active;
        self.values.push(Value::Result(id));
        Ok(())
    }

    pub(super) fn result_select(
        &mut self,
        frame: u64,
        ty: ResultTypeId,
        ok: BasicBlockId,
        error: BasicBlockId,
    ) -> Result<(), ExecutionError> {
        self.charge()?; // Validated tag dispatch; neither body has executed.
        let Some(Value::Result(id)) = self.values.last() else {
            return Err(self.integrity("match scrutinee is not a protected Result"));
        };
        let id = *id;
        let cell = self.result_cell(id)?;
        if cell.ty != ty
            || cell.phase != Phase::Active
            || cell.payload.is_none()
            || cell.parent.is_some()
        {
            return Err(self.integrity("invalid complete Result scrutinee custody"));
        }
        let tag = Self::result_tag(cell.tag)?;
        if self.results.as_ref().unwrap().matches.len()
            == self.results.as_ref().unwrap().matches.capacity()
        {
            self.reserve(size_of::<(u64, OwnerId)>())?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.results
                .as_mut()
                .unwrap()
                .matches
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("Result match protection allocation")))?;
        }
        self.values.pop();
        let state = self.results.as_mut().unwrap();
        state.cells[id.0].phase = Phase::MatchProtected {
            frame,
            origin: self.origin.span,
        };
        state.matches.push((frame, id));
        self.cleanup_exit(frame)?;
        self.cleanup_next_block(frame)?;
        let index = self.frame_index(frame)?;
        self.frames[index].block = match tag {
            ResultTag::Ok => ok,
            ResultTag::Error => error,
        };
        self.frames[index].statement = 0;
        self.task(frame, self.origin, None, Action::Drive)
    }

    pub(super) fn result_forward_error(
        &mut self,
        frame: u64,
        operation: hir::Propagation,
    ) -> Result<(), ExecutionError> {
        let id = self
            .results
            .as_ref()
            .and_then(|s| s.matches.last())
            .filter(|(f, _)| *f == frame)
            .map(|(_, id)| *id)
            .ok_or_else(|| self.integrity("propagation without protected input"))?;
        let cell = self.result_cell(id)?;
        if cell.ty != operation.inner
            || Self::result_tag(cell.tag)? != ResultTag::Error
            || cell.phase
                != (Phase::MatchProtected {
                    frame,
                    origin: operation.span,
                })
            || cell
                .payload
                .as_ref()
                .map_or(true, |v| !self.result_value_matches(v, operation.failure))
        {
            return Err(self.integrity("propagation failure payload custody"));
        }
        if let Some(Value::Result(child)) = &cell.payload {
            if self.result_cell(*child)?.parent != Some(id) {
                return Err(self.integrity("propagation nested parent custody"));
            }
        }
        let cell_grow = self.results.as_ref().unwrap().cells.len()
            == self.results.as_ref().unwrap().cells.capacity();
        let value_grow = self.values.len() == self.values.capacity();
        // Combined receiving capacities are charged while F still belongs to
        // the protected input. No fallible operation follows active detachment.
        self.charge()?;
        self.reserve(
            (if cell_grow { size_of::<Cell>() } else { 0 })
                + (if value_grow { size_of::<Value>() } else { 0 }),
        )?;
        if cell_grow {
            #[cfg(test)]
            self.temporary_allocation()?;
            self.results
                .as_mut()
                .unwrap()
                .cells
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("propagated Result cell allocation")))?;
        }
        if value_grow {
            #[cfg(test)]
            self.temporary_allocation()?;
            self.values.try_reserve_exact(1).map_err(|_| {
                self.failure(ErrorKind::Host("propagated return receiving allocation"))
            })?;
        }
        self.charge()?;
        let f = self.frame_index(frame)?;
        let function = self.frames[f].function;
        let block = self.frames[f].block;
        let authority = Authority::propagation(block)
            .ok_or_else(|| self.integrity("propagation authority index exhausted"))?;
        let copied = self.result_cell(id)?.payload.as_ref().and_then(Value::copy);
        let original = self.results.as_mut().unwrap().cells[id.0]
            .payload
            .take()
            .unwrap();
        let payload = copied.unwrap_or(original);
        let out = OwnerId(self.results.as_ref().unwrap().cells.len());
        if let Value::Result(inner) = &payload {
            self.results.as_mut().unwrap().cells[inner.0].parent = Some(out);
        }
        let state = self.results.as_mut().unwrap();
        state.cells[id.0].phase = Phase::Consumed;
        state.matches.pop();
        state.cells.push(Cell {
            function,
            authority,
            ty: operation.outer,
            tag: 1,
            payload: Some(payload),
            parent: None,
            phase: Phase::Active,
            origin: operation.syntax.phrase,
        });
        self.values.push(Value::Result(out));
        Ok(())
    }

    pub(super) fn result_bind(
        &mut self,
        frame: u64,
        ty: ResultTypeId,
        tag: ResultTag,
        local: hir::LocalId,
        origin: Span,
    ) -> Result<(), ExecutionError> {
        let id = self
            .results
            .as_ref()
            .and_then(|s| s.matches.last())
            .filter(|(f, _)| *f == frame)
            .map(|(_, id)| *id)
            .ok_or_else(|| self.integrity("Result binding without selected scrutinee"))?;
        let cell = self.result_cell(id)?;
        if cell.ty != ty
            || Self::result_tag(cell.tag)? != tag
            || cell.phase != (Phase::MatchProtected { frame, origin })
            || cell.payload.is_none()
        {
            return Err(self.integrity("Result arm tag, type, frame or origin mismatch"));
        }
        let expected = self
            .executable
            .program
            .result_payload(ty, tag)
            .ok_or_else(|| self.integrity("foreign Result binding type"))?;
        if !self.result_value_matches(cell.payload.as_ref().unwrap(), expected) {
            return Err(self.integrity("Result binding payload mismatch"));
        }
        if let Some(Value::Result(inner)) = &cell.payload {
            if self.result_cell(*inner)?.parent != Some(id) {
                return Err(self.integrity("Result binding parent mismatch"));
            }
        }
        let function = self.frames[self.frame_index(frame)?].function;
        if self
            .executable
            .program
            .locals()
            .get(local.0)
            .map_or(true, |l| {
                l.id != local || l.function != function || l.ty != expected
            })
        {
            return Err(self.integrity("foreign or wrong-type Result binding"));
        }
        let at = Location {
            frame,
            place: Place::Local(local),
        };
        let (f, index) = self.slot_index(at)?; // Reserve before active detachment.
        if let Slot::Available(value) = &self.frames[f].slots[index].1 {
            // A recurring lexical arm may reuse inert storage from its previous
            // completed activation. Resource owners and unresolved wrappers must
            // already have transferred or cleaned before that edge.
            if self.resource_bearing(value) || matches!(value, Value::Result(_)) {
                return Err(self.integrity("Result binding would overwrite a live owner"));
            }
        }
        self.charge()?; // Active payload transfer.
        let copied = self.result_cell(id)?.payload.as_ref().and_then(Value::copy);
        let original = self.results.as_mut().unwrap().cells[id.0]
            .payload
            .take()
            .unwrap();
        let payload = copied.unwrap_or(original);
        if let Value::Result(inner) = &payload {
            self.results.as_mut().unwrap().cells[inner.0].parent = None;
        }
        self.frames[f].slots[index].1 = Slot::Available(payload);
        self.results.as_mut().unwrap().cells[id.0].phase = Phase::Consumed;
        self.results.as_mut().unwrap().matches.pop();
        self.cleanup_initialized(at)?;
        self.charge()?; // Arm entry; a stop retains the initialized binding.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::cleanup::{conservation_tests::conserved, Event};
    use super::*;
    use crate::execution::{prepare_execution, prepare_ordinary_synthetic};

    fn ordinary(source: &str) -> CheckedExecutable {
        prepare_ordinary_synthetic(
            hir::resources::fixture_with(source),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    fn completed(m: &Machine<'_>) -> Vec<u64> {
        m.resources
            .as_ref()
            .map(|s| {
                s.events
                    .iter()
                    .filter_map(|e| match e {
                        Event::Complete(id) => Some(*id),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn run(e: &CheckedExecutable) -> Machine<'_> {
        let mut m = Machine::new(e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        while let Some(task) = m.tasks.pop() {
            m.step(task).unwrap();
            if m.resources.is_some() {
                conserved(&m);
            }
        }
        m
    }
    fn roots(m: &Machine<'_>) -> usize {
        m.results.as_ref().map_or(0, |s| {
            s.cells
                .iter()
                .filter(|c| c.payload.is_some() && c.parent.is_none())
                .count()
        })
    }

    #[test]
    fn ordinary_resource_alternatives_and_nested_results_release_only_active_bindings() {
        for (ty, construction, ok_body, failure_body) in [
            ("Result<success:Leaf,failure:bool>", "Ok(acquire())", "", ""),
            (
                "Result<success:bool,failure:Leaf>",
                "Err(acquire())",
                "",
                "",
            ),
            (
                "Result<success:Leaf,failure:Leaf>",
                "Error(acquire())",
                "",
                "",
            ),
            (
                "Result<success:Result<success:Leaf,failure:Leaf>,failure:bool>",
                "Ok(Err(acquire()))",
                "match value{Ok(inner)=>{}Err(problem)=>{}}",
                "",
            ),
        ] {
            let e = ordinary(&format!("fn make()->{ty}{{return {construction};}}fn demo(){{match make(){{Ok(value)=>{{{ok_body}}}Error(problem)=>{{{failure_body}}}}}}}"));
            let m = run(&e);
            assert_eq!(completed(&m), vec![0]);
            assert_eq!(roots(&m), 0);
            assert!(e
                .result_plans
                .iter()
                .any(hir::results::VariantPlan::bearing));
        }
    }

    #[test]
    fn every_fuel_cut_retains_actual_resource_payload_and_terminal_prefix() {
        let e = ordinary("fn make()->Result<success:Leaf,failure:Leaf>{return Err(acquire());}fn forward()->Result<success:Leaf,failure:Leaf>{let guard=acquire();match make(){Ok(value)=>{return Ok(value);}Err(problem)=>{return Error(problem);}}}fn take(pending:Result<success:Leaf,failure:Leaf>){match pending{Ok(value)=>{}Error(problem)=>{}}}fn demo(){take(forward());}");
        let full = run(&e);
        assert_eq!(completed(&full), vec![0, 1]);
        let mut protected = false;
        for steps in 1..full.steps {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    steps,
                    ..ExecutionLimits::default()
                },
            );
            let outcome = m.start(e.entry("demo").unwrap()).and_then(|_| m.resume());
            assert!(outcome.is_err(), "fuel cut {steps}");
            if m.resources.is_some() {
                conserved(&m);
            }
            protected |= m
                .resources
                .as_ref()
                .is_some_and(|s| matches!(s.protected, Some(Value::Result(_))));
            let events = format!("{:?}", m.resources.as_ref().map(|s| &s.events));
            assert!(m.resume().is_err());
            assert_eq!(
                events,
                format!("{:?}", m.resources.as_ref().map(|s| &s.events))
            );
        }
        assert!(protected);
    }

    #[test]
    fn resource_free_results_retain_cells_without_enabling_resource_catalog() {
        let e = prepare_execution("fn make()->Result<success:bool,failure:bool>{return Err(false);}fn demo()->bool{match make(){Ok(value)=>{return value;}Err(problem)=>{return problem;}}}", AdmissionLimits::default()).unwrap();
        let full = run(&e);
        assert!(full.resources.is_none());
        let mut stopped_live = false;
        for steps in 1..full.steps {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    steps,
                    ..ExecutionLimits::default()
                },
            );
            assert!(m
                .start(e.entry("demo").unwrap())
                .and_then(|_| m.resume())
                .is_err());
            assert!(m.resources.is_none());
            stopped_live |= roots(&m) == 1;
            assert!(roots(&m) <= 1);
            assert!(m.resume().is_err());
        }
        assert!(stopped_live);
    }

    #[test]
    fn inert_nested_variant_plans_retain_child_identity_without_release_work() {
        let e = prepare_execution("fn demo()->Result<success:Result<success:bool,failure:bool>,failure:bool>{return Ok(Err(false));}", AdmissionLimits::default()).unwrap();
        assert_eq!(e.result_plans.len(), 2);
        assert_eq!(
            e.result_plans[1].success,
            Some(hir::results::VariantChild::Result(ResultTypeId(0)))
        );
        assert!(e
            .result_plans
            .iter()
            .all(|p| !p.bearing() && p.effects == crate::effects::EffectSet::EMPTY));
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let output = m.resume().unwrap();
        assert_eq!(
            e.render_result(&output, 100).unwrap(),
            "Ok(Error(Bool(false)))"
        );
        assert!(m
            .results
            .as_ref()
            .unwrap()
            .cells
            .iter()
            .all(|c| c.phase == Phase::Delivered && c.payload.is_none()));
        assert!(m.resources.is_none());
    }
    #[test]
    fn every_storage_and_host_allocation_cut_conserves_result_payloads() {
        for source in [
            "fn make()->Result<success:Leaf,failure:Leaf>{return Err(acquire());}fn demo(){match make(){Ok(value)=>{}Err(problem)=>{}}}",
            "fn make()->Result<success:bool,failure:bool>{return Ok(true);}fn demo(){match make(){Ok(value)=>{}Err(problem)=>{}}}",
        ] {
            let e = ordinary(source);
            let mut complete = Machine::new(&e, ExecutionLimits::default());
            complete.start(e.entry("demo").unwrap()).unwrap();
            let mut peak = complete.storage().unwrap();
            while let Some(task) = complete.tasks.pop() {
                complete.step(task).unwrap();
                peak = peak.max(complete.storage().unwrap());
            }
            peak = peak.max(complete.max_storage.get());
            for budget in 1..=peak {
                let mut m = Machine::new(&e, ExecutionLimits { working_storage: budget, ..ExecutionLimits::default() });
                let outcome = m.start(e.entry("demo").unwrap()).and_then(|_| m.resume());
                if m.resources.is_some() { conserved(&m); }
                if budget == peak { assert!(outcome.is_ok(), "peak {peak}: {outcome:?}"); }
                else if let Err(error) = outcome { assert!(matches!(error.kind, ErrorKind::ConfiguredLimit("working storage")), "{budget}: {error:?}"); }
            }
            let mut observed_success = false;
            for cut in 0..1000 {
                let mut m = Machine::new(&e, ExecutionLimits::default());
                super::super::temporaries::FAIL_ALLOCATION.with(|f| f.set(Some(cut)));
                let outcome = m.start(e.entry("demo").unwrap()).and_then(|_| m.resume());
                super::super::temporaries::FAIL_ALLOCATION.with(|f| f.set(None));
                if m.resources.is_some() { conserved(&m); }
                match outcome {
                    Ok(_) => { observed_success = true; break; },
                    Err(error) => { assert!(matches!(error.kind, ErrorKind::Host(_)), "{cut}: {error:?}"); assert!(m.resume().is_err()); },
                }
            }
            assert!(observed_success);
        }
    }

    #[test]
    fn failed_callee_cleanup_does_not_deliver_protected_failure_result() {
        let e = ordinary("fn forward()->Result<success:Leaf,failure:Leaf>{let guard=acquire();return Err(acquire());}fn demo(){match forward(){Ok(value)=>{}Error(problem)=>{}}}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.enable_resources().unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(0);
        m.start(e.entry("demo").unwrap()).unwrap();
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        conserved(&m);
        assert!(matches!(
            m.resources.as_ref().unwrap().protected,
            Some(Value::Result(_))
        ));
        assert_eq!(completed(&m), Vec::<u64>::new());
        assert!(m
            .results
            .as_ref()
            .unwrap()
            .cells
            .iter()
            .any(|c| c.phase == Phase::Active
                && matches!(c.payload, Some(Value::Leaf { identity: 1, .. }))));
        assert!(m.resume().is_err());
    }

    #[test]
    fn malformed_runtime_tags_payloads_origins_and_detach_reject() {
        for mutation in 0..8 {
            let e = prepare_execution("fn make()->Result<success:bool,failure:bool>{return Ok(true);}fn demo()->bool{match make(){Ok(value)=>{return value;}Error(problem)=>{return problem;}}}", AdmissionLimits::default()).unwrap();
            let mut m = Machine::new(&e, ExecutionLimits::default());
            m.start(e.entry("demo").unwrap()).unwrap();
            loop {
                let task = m.tasks.pop().unwrap();
                m.step(task).unwrap();
                if m.results
                    .as_ref()
                    .is_some_and(|s| s.cells.iter().any(|c| c.phase == Phase::Active))
                {
                    break;
                }
            }
            let cell = m
                .results
                .as_mut()
                .unwrap()
                .cells
                .iter_mut()
                .find(|c| c.phase == Phase::Active)
                .unwrap();
            match mutation {
                0 => cell.tag = 2, // Invented Err-specific tag.
                1 => cell.tag = 255,
                2 => cell.tag = 1, // Same bool payload types: source tag still binds.
                3 => cell.payload = None,
                4 => cell.payload = Some(Value::Raw(7)),
                5 => cell.origin.start += 1,
                6 => cell.phase = Phase::Consumed,
                7 => cell.parent = Some(OwnerId(999)),
                _ => unreachable!(),
            }
            assert!(
                matches!(m.resume().unwrap_err().kind, ErrorKind::Integrity(_)),
                "mutation {mutation}"
            );
            assert!(m.resume().is_err());
        }
    }

    #[test]
    fn primitive_whole_wrapper_cleanup_is_separate_from_ordinary_handling() {
        for (ty, construction, expected) in [
            (
                "Result<success:Leaf,failure:bool>",
                "Ok(acquire())",
                vec![0],
            ),
            ("Result<success:Leaf,failure:bool>", "Err(false)", vec![]),
            (
                "Result<success:bool,failure:Leaf>",
                "Error(acquire())",
                vec![0],
            ),
            (
                "Result<success:Result<success:Leaf,failure:bool>,failure:bool>",
                "Ok(Ok(acquire()))",
                vec![0],
            ),
            (
                "Result<success:Result<success:bool,failure:bool>,failure:bool>",
                "Ok(Err(true))",
                vec![],
            ),
        ] {
            let p =
                hir::resources::fixture_with(&format!("fn demo()->{ty}{{return {construction};}}"));
            let ordinary =
                prepare_ordinary_synthetic(p.clone(), AdmissionLimits::default()).unwrap();
            let mut m = run(&ordinary);
            m.enable_resources().unwrap();
            let value = m.pop().unwrap();
            assert!(m.release_value(value).is_err());
            assert!(completed(&m).is_empty());
            assert_eq!(roots(&m), 1);
            conserved(&m);
            let primitive =
                crate::execution::prepare_synthetic(p, AdmissionLimits::default()).unwrap();
            let mut m = run(&primitive);
            m.enable_resources().unwrap();
            let value = m.pop().unwrap();
            m.release_value(value).unwrap();
            assert_eq!(completed(&m), expected);
            assert_eq!(roots(&m), 0);
            assert!(m
                .results
                .as_ref()
                .unwrap()
                .cells
                .iter()
                .all(|c| c.phase == Phase::Consumed));
            conserved(&m);
        }
    }

    #[test]
    fn result_effect_union_remains_structural_for_inactive_resource_alternatives() {
        use crate::effects::{Effect, EffectSet};
        let mut acquire = EffectSet::EMPTY;
        acquire.insert(Effect::Io);
        let mut release = EffectSet::EMPTY;
        release.insert(Effect::Filesystem);
        for construction in ["Ok(acquire())", "Err(true)"] {
            let p = hir::resources::fixture_effects(&format!("fn make()->Result<success:Leaf,failure:bool> effects io,filesystem{{return {construction};}}fn demo() effects io,filesystem{{match make(){{Ok(value)=>{{}}Error(problem)=>{{}}}}}}"),acquire,release);
            let e = prepare_ordinary_synthetic(p, AdmissionLimits::default()).unwrap();
            assert_eq!(e.result_plans[0].effects, release);
            let m = run(&e);
            assert_eq!(
                completed(&m),
                if construction.starts_with("Ok") {
                    vec![0]
                } else {
                    vec![]
                }
            );
        }
        let p=hir::resources::fixture_effects("fn make()->Result<success:Leaf,failure:bool> effects io{return Err(true);}fn demo() effects io{match make(){Ok(value)=>{}Error(problem)=>{}}}",acquire,release);
        assert!(prepare_ordinary_synthetic(p, AdmissionLimits::default()).is_err());
    }
    #[test]
    fn failed_payload_evaluation_publishes_no_result_or_opposite_variant() {
        let e=ordinary("fn zero()->R{return 0;}fn trap()->Leaf{let held=acquire();let impossible=zero()-1;return held;}fn make()->Result<success:Leaf,failure:bool>{return Ok(trap());}fn demo(){match make(){Ok(value)=>{}Error(problem)=>{}}}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert!(matches!(
            m.resume().unwrap_err().kind,
            ErrorKind::RangeTrap { .. }
        ));
        conserved(&m);
        assert!(completed(&m).is_empty());
        assert!(m
            .results
            .as_ref()
            .unwrap()
            .cells
            .iter()
            .all(|c| c.phase == Phase::PayloadPending && c.payload.is_none()));
        assert!(m.resume().is_err());
    }
    #[test]
    fn result_custody_coexists_with_record_replacement_generations() {
        let e=ordinary("fn make()->Result<success:Leaf,failure:bool>{return Ok(acquire());}fn demo(){let mut held=acquire();let pending=make();held=acquire();match pending{Ok(value)=>{}Error(problem)=>{}}}");
        let m = run(&e);
        assert_eq!(completed(&m), vec![0, 1, 2]);
        conserved(&m);
    }

    #[test]
    fn resource_result_public_entry_rejects_before_execution_or_acquisition() {
        let e = ordinary("fn demo()->Result<success:Leaf,failure:bool>{return Ok(acquire());}");
        let error = crate::execution::execute(
            &e,
            e.entry("demo").unwrap(),
            ExecutionLimits {
                steps: 1,
                ..ExecutionLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            error.kind,
            ErrorKind::Integrity("live resource Result cannot cross public boundary")
        );
    }
    #[test]
    fn distinct_variant_children_union_contracts_and_reject_shrunken_or_foreign_plans() {
        use crate::effects::{Declaration, Effect, EffectSet};
        use hir::resources::{Leaf, Operation, OperationId, OperationKind, ResourceId};
        for constructor in ["Ok(acquire())", "Err(acquire_other())"] {
            let mut p=hir::resources::fixture_with(&format!("fn acquire_other()->Pair{{return Pair{{first:0,inert:0,last:0}};}}fn release_other(value:Pair){{}}fn make()->Result<success:Leaf,failure:Pair> effects filesystem,clock{{return {constructor};}}fn demo() effects filesystem,clock{{match make(){{Ok(value)=>{{}}Error(problem)=>{{}}}}}}"));
            p.records[1].fields.clear();
            p.fields.clear();
            let hir::Function::Ordinary(acquire) = &mut p.functions[2] else {
                panic!()
            };
            let hir::ValueStatement::Return { value, .. } = &mut acquire.body[0] else {
                panic!()
            };
            let hir::ExprKind::RecordConstruct { fields, .. } = &mut value.kind else {
                panic!()
            };
            fields.clear();
            let mut fs = EffectSet::EMPTY;
            fs.insert(Effect::Filesystem);
            let mut clock = EffectSet::EMPTY;
            clock.insert(Effect::Clock);
            for (index, effects) in [(1, fs), (3, clock)] {
                let hir::Function::Ordinary(f) = &mut p.functions[index] else {
                    panic!()
                };
                f.effects = Declaration::synthetic(effects, f.span);
            }
            p.resources.operations[1].effects = fs;
            p.resources.leaves[0].cleanup_effects = fs;
            p.resources.leaves.push(Leaf {
                id: ResourceId(1),
                record: hir::RecordId(1),
                acquire: OperationId(2),
                release: OperationId(3),
                cleanup_effects: clock,
            });
            p.resources.operations.push(Operation {
                id: OperationId(2),
                function: FunctionId(2),
                kind: OperationKind::Acquire(ResourceId(1)),
                effects: EffectSet::EMPTY,
            });
            p.resources.operations.push(Operation {
                id: OperationId(3),
                function: FunctionId(3),
                kind: OperationKind::Release(ResourceId(1)),
                effects: clock,
            });
            let e = prepare_ordinary_synthetic(p.clone(), AdmissionLimits::default()).unwrap();
            assert_eq!(e.result_plans[0].effects, fs.union(clock));
            assert_eq!(
                e.result_plans[0].success,
                Some(hir::results::VariantChild::Record(hir::RecordId(0)))
            );
            assert_eq!(
                e.result_plans[0].failure,
                Some(hir::results::VariantChild::Record(hir::RecordId(1)))
            );
            let m = run(&e);
            assert_eq!(completed(&m), vec![0]);
            conserved(&m);
            for mutation in 0..10 {
                let mut e =
                    prepare_ordinary_synthetic(p.clone(), AdmissionLimits::default()).unwrap();
                match mutation {
                    0 => e.result_plans[0].effects = fs,
                    1 => e.result_plans[0].failure = e.result_plans[0].success,
                    2 => {
                        let plan = &mut e.result_plans[0];
                        std::mem::swap(&mut plan.success, &mut plan.failure);
                    }
                    3 => {
                        e.result_plans[0].success =
                            Some(hir::results::VariantChild::Record(hir::RecordId(999)))
                    }
                    4 => e.domain = crate::execution::domain::Domain::Primitive,
                    5..=9 => crate::mir::mutate_result_mir(&mut e.mir, mutation + 14),
                    _ => unreachable!(),
                }
                assert!(e.validate().is_err(), "{constructor}/{mutation}");
            }
        }
    }
}
