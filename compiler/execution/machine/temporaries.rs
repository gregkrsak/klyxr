//! Explicit expression custody; neither history nor operand handles own payloads.
use super::*;
use crate::mir::temporaries::{Destination as Transfer, Point, RegionId, SiteId};

#[cfg(test)]
thread_local! {
    pub(super) static FAIL_ALLOCATION: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OwnerId {
    pub(super) activation: u64,
    pub(super) ordinal: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Custody {
    Owned,
    Transferred,
    Cleaning,
    Cleaned,
}
struct Owner {
    site: SiteId,
    custody: Custody,
    value: Option<Value>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Active,
    Retiring,
    Retired,
}
struct Activation {
    id: u64,
    frame: u64,
    function: FunctionId,
    region: RegionId,
    parent: Option<u64>,
    owners: Vec<Owner>,
    protected: Option<Value>,
    phase: Phase,
    cursor: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    Begin(u64, u64, RegionId, Option<u64>),
    Register(OwnerId, SiteId),
    Transfer(OwnerId, Transfer),
    Cleanup(OwnerId),
    Retire(u64),
}
#[derive(Default)]
pub(super) struct State {
    next: u64,
    frames: Vec<Activation>,
    pub(super) events: Vec<Event>,
}
#[derive(Clone, Copy)]
pub(super) struct Context {
    pub(super) activation: u64,
    pub(super) site: Option<SiteId>,
    pub(super) complete: bool,
}

impl State {
    pub(super) fn owned_value(&self, id: OwnerId) -> Option<&Value> {
        self.frames
            .iter()
            .find(|f| f.id == id.activation)?
            .owners
            .get(id.ordinal)
            .filter(|o| o.custody == Custody::Owned)?
            .value
            .as_ref()
    }
    pub(super) fn storage(&self) -> Option<usize> {
        let mut n = size_of::<Self>()
            .checked_add(
                self.frames
                    .capacity()
                    .checked_mul(size_of::<Activation>())?,
            )?
            .checked_add(self.events.capacity().checked_mul(size_of::<Event>())?)?;
        for f in &self.frames {
            n = n.checked_add(f.owners.capacity().checked_mul(size_of::<Owner>())?)?;
            for value in f
                .owners
                .iter()
                .filter_map(|o| o.value.as_ref())
                .chain(f.protected.iter())
            {
                n = n.checked_add(value.extra_storage())?;
            }
        }
        Some(n)
    }
    pub(super) fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    #[cfg(test)]
    pub(super) fn visit_values(&self, mut visit: impl FnMut(&Value)) {
        for f in &self.frames {
            for v in f
                .owners
                .iter()
                .filter_map(|o| o.value.as_ref())
                .chain(f.protected.iter())
            {
                visit(v);
            }
        }
    }
    #[cfg(test)]
    pub(super) fn visit_holders(&self, mut visit: impl FnMut(&Value, String)) {
        for f in &self.frames {
            for (ordinal, owner) in f.owners.iter().enumerate() {
                if let Some(v) = &owner.value {
                    visit(v, format!("temporary:{}:owner:{ordinal}", f.id));
                }
            }
            if let Some(v) = &f.protected {
                visit(v, format!("protected-completion:{}", f.id));
            }
        }
    }
}

impl<'a> Machine<'a> {
    pub(super) fn completion_temporary_growth(
        &self,
        activation: u64,
        site: SiteId,
    ) -> Result<usize, ExecutionError> {
        let index = self.temporary_index(activation)?;
        let frame = &self.temporaries.as_ref().unwrap().frames[index];
        let metadata = &self
            .executable
            .function(frame.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .sites[site.0];
        if frame.phase != Phase::Active
            || frame.region != metadata.region
            || self.frames.last().map(|current| current.id) != Some(frame.frame)
        {
            return Err(self.integrity("completion receiving region mismatch"));
        }
        let owners =
            usize::from(frame.owners.len() == frame.owners.capacity()) * size_of::<Owner>();
        let events = &self.temporaries.as_ref().unwrap().events;
        owners
            .checked_add(
                2usize
                    .saturating_sub(events.capacity() - events.len())
                    .checked_mul(size_of::<Event>())
                    .ok_or_else(|| self.integrity("completion temporary event overflow"))?,
            )
            .ok_or_else(|| self.integrity("completion temporary envelope overflow"))
    }

    pub(super) fn completion_input(
        &self,
        input: SiteId,
        operation: hir::TrustedOperationId,
    ) -> Result<(OwnerId, hir::ResourceTypeId, u64), ExecutionError> {
        let Some(Value::Temporary(id)) = self.values.last() else {
            return Err(self.integrity("completion requires current owned temporary"));
        };
        let index = self.temporary_index(id.activation)?;
        let frame = &self.temporaries.as_ref().unwrap().frames[index];
        let owner = frame
            .owners
            .get(id.ordinal)
            .filter(|o| o.custody == Custody::Owned && o.site == input)
            .ok_or_else(|| self.integrity("completion input holder/site mismatch"))?;
        let flow = self
            .executable
            .function(frame.function)?
            .temporaries
            .as_ref()
            .unwrap();
        if flow.sites[input.0].destination != Transfer::TrustedArgument(operation, 0) {
            return Err(self.integrity("completion input destination mismatch"));
        }
        let Some(Value::Resource { ty, identity }) = owner.value.as_ref() else {
            return Err(self.integrity("completion input lacks exact Sensor ticket"));
        };
        Ok((*id, *ty, *identity))
    }

    #[cfg(test)]
    pub(super) fn corrupt_completion_input_frame(&mut self) {
        let Some(Value::Temporary(id)) = self.values.last() else { unreachable!("owned completion argument") };
        let index = self.temporary_index(id.activation).unwrap();
        self.temporaries.as_mut().unwrap().frames[index].frame = u64::MAX;
    }

    pub(super) fn completion_temporary_reserve(
        &mut self,
        activation: u64,
        site: SiteId,
    ) -> Result<(), ExecutionError> {
        self.temporary_reserve_register(activation, site)?;
        let events = &self.temporaries.as_ref().unwrap().events;
        let growth = 2usize.saturating_sub(events.capacity() - events.len());
        self.reserve(
            growth
                .checked_mul(size_of::<Event>())
                .ok_or_else(|| self.integrity("completion transfer events overflow"))?,
        )?;
        #[cfg(test)]
        self.trusted_allocation(super::trusted::Allocation::CompletionTransfer, growth > 0)?;
        self.temporaries
            .as_mut()
            .unwrap()
            .events
            .try_reserve_exact(2)
            .map_err(|_| self.failure(ErrorKind::Host("completion temporary events")))?;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn temporary_allocation(&self) -> Result<(), ExecutionError> {
        FAIL_ALLOCATION.with(|remaining| {
            if let Some(n) = remaining.get() {
                if n == 0 {
                    return Err(self.failure(ErrorKind::Host("synthetic runtime allocation")));
                }
                remaining.set(Some(n - 1));
            }
            Ok(())
        })
    }
    #[cfg(test)]
    pub(super) fn inject_temporary_leaf(&mut self, activation: u64) -> Result<u64, ExecutionError> {
        if !self.executable.domain.primitive() {
            return Err(
                self.integrity("temporary injection requires sealed primitive evidence domain")
            );
        }
        let i = self.temporary_index(activation).unwrap();
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        let flow = self
            .executable
            .function(f.function)
            .unwrap()
            .temporaries
            .as_ref()
            .unwrap();
        let site = flow
            .sites
            .iter()
            .find(|s| {
                s.region == f.region
                    && s.plan
                        .is_some_and(|r| matches!(r, hir::resources::CleanupType::Record(record) if self.executable.program.resources.is_leaf(record)))
            })
            .unwrap()
            .id;
        let function = self
            .executable
            .program
            .resources
            .operations
            .iter()
            .find(|o| matches!(o.kind, hir::resources::OperationKind::Acquire(_)))
            .unwrap()
            .function;
        self.trusted_invoke(function, 0).unwrap();
        let identity = match self.values.last().unwrap() {
            Value::Leaf { identity, .. } => *identity,
            _ => panic!(),
        };
        self.temporary_register(activation, site).unwrap();
        assert!(matches!(self.values.pop(), Some(Value::Temporary(_))));
        Ok(identity)
    }
    fn temporary_event_reserve(&mut self) -> Result<(), ExecutionError> {
        let s = self.temporaries.as_ref().unwrap();
        if s.events.len() == s.events.capacity() {
            self.reserve(size_of::<Event>())?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.temporaries
                .as_mut()
                .unwrap()
                .events
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("temporary event allocation")))?;
        }
        Ok(())
    }
    fn temporary_index(&self, id: u64) -> Result<usize, ExecutionError> {
        self.temporaries
            .as_ref()
            .and_then(|s| s.frames.iter().position(|f| f.id == id))
            .ok_or_else(|| self.integrity("unknown temporary activation"))
    }
    pub(super) fn temporary_begin(
        &mut self,
        frame: u64,
        function: FunctionId,
        region: RegionId,
    ) -> Result<u64, ExecutionError> {
        self.charge()?;
        let description = &self
            .executable
            .function(function)?
            .temporaries
            .as_ref()
            .ok_or_else(|| self.integrity("missing temporary authority"))?
            .regions[region.0];
        let parent = self.temporaries.as_ref().and_then(|s| s.frames.last());
        if parent.is_some_and(|p| {
            p.phase != Phase::Active || (description.parent.is_none() && p.frame == frame)
        }) {
            return Err(self.integrity("overlapping source temporary regions"));
        }
        if let Some(expected) = description.parent {
            if parent.map_or(true, |p| {
                p.frame != frame
                    || p.function != function
                    || p.region != expected
                    || p.phase != Phase::Active
            }) {
                return Err(self.integrity("temporary child activation parent mismatch"));
            }
        }
        let parent = parent.map(|p| p.id);
        self.origin = description.origin;
        if self.executable.cleanup_plans.iter().any(Option::is_some) || self.trusted.is_some() {
            self.enable_resources()?;
        }
        if self.temporaries.is_none() {
            self.reserve(
                size_of::<State>()
                    .checked_add(size_of::<Activation>())
                    .ok_or_else(|| self.integrity("temporary state size"))?,
            )?;
            self.temporaries = Some(State::default());
        }
        let s = self.temporaries.as_ref().unwrap();
        let id = s.next;
        let next = id
            .checked_add(1)
            .ok_or_else(|| self.integrity("temporary activation exhausted"))?;
        if s.frames.len() == s.frames.capacity() {
            self.reserve(size_of::<Activation>())?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.temporaries
                .as_mut()
                .unwrap()
                .frames
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("temporary activation allocation")))?;
        }
        self.temporary_event_reserve()?;
        let s = self.temporaries.as_mut().unwrap();
        s.next = next;
        s.frames.push(Activation {
            id,
            frame,
            function,
            region,
            parent,
            owners: Vec::new(),
            protected: None,
            phase: Phase::Active,
            cursor: 0,
        });
        s.events.push(Event::Begin(id, frame, region, parent));
        Ok(id)
    }
    pub(super) fn temporary_point(
        &mut self,
        frame: u64,
        function: FunctionId,
        block: BasicBlockId,
        statement: usize,
    ) -> Result<Option<Context>, ExecutionError> {
        let point: Option<Point> = self
            .executable
            .function(function)?
            .temporaries
            .as_ref()
            .and_then(|f| {
                f.points
                    .iter()
                    .find(|p| p.block == block && p.statement == statement)
            })
            .cloned();
        let Some(p) = point else {
            return Ok(None);
        };
        let activation = if p.begin {
            self.temporary_begin(frame, function, p.region)?
        } else {
            self.temporaries
                .as_ref()
                .and_then(|s| s.frames.last())
                .filter(|a| {
                    a.frame == frame
                        && a.function == function
                        && a.region == p.region
                        && a.phase == Phase::Active
                })
                .map(|a| a.id)
                .ok_or_else(|| self.integrity("temporary continuation activation mismatch"))?
        };
        Ok(Some(Context {
            activation,
            site: p.site,
            complete: p.complete,
        }))
    }
    pub(super) fn temporary_site_origin(
        &self,
        activation: u64,
        site: SiteId,
        frame: u64,
    ) -> Result<Origin, ExecutionError> {
        let i = self.temporary_index(activation)?;
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        let site = self
            .executable
            .function(f.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .sites
            .get(site.0)
            .ok_or_else(|| self.integrity("foreign temporary expression site"))?;
        if f.frame != frame || f.region != site.region || f.phase != Phase::Active {
            return Err(self.integrity("temporary expression activation mismatch"));
        }
        Ok(site.origin)
    }
    pub(super) fn temporary_register(
        &mut self,
        activation: u64,
        site: SiteId,
    ) -> Result<(), ExecutionError> {
        self.charge()?;
        let i = self.temporary_index(activation)?;
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        let metadata = &self
            .executable
            .function(f.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .sites[site.0];
        let record_valid = metadata.result_plan.is_none()
            && metadata.plan.is_some()
            && matches!(self.values.last(), Some(Value::Leaf {record,..} | Value::Aggregate {record,..}) if Some(hir::resources::CleanupType::Record(*record)) == metadata.plan);
        let resource_valid = metadata.result_plan.is_none()
            && matches!((metadata.plan, self.values.last()), (Some(hir::resources::CleanupType::Resource(expected)), Some(Value::Resource {ty, ..})) if expected == *ty);
        let result_valid = metadata.plan.is_none()
            && metadata.result_plan.is_some_and(|ty| {
                self.values
                    .last()
                    .is_some_and(|value| self.result_value_matches(value, ValueType::Result(ty)))
            });
        if i + 1 != self.temporaries.as_ref().unwrap().frames.len()
            || f.phase != Phase::Active
            || f.region != metadata.region
            || !(record_valid || resource_valid || result_valid)
        {
            return Err(self.integrity("invalid temporary creation type, region or plan"));
        }
        let ordinal = f.owners.len();
        self.temporary_reserve_register(activation, site)?;
        // Payload remains in values through every fallible reservation.
        let value = self.values.pop().unwrap();
        let id = OwnerId {
            activation,
            ordinal,
        };
        let s = self.temporaries.as_mut().unwrap();
        s.frames[i].owners.push(Owner {
            site,
            custody: Custody::Owned,
            value: Some(value),
        });
        s.events.push(Event::Register(id, site));
        self.values.push(Value::Temporary(id));
        Ok(())
    }
    pub(super) fn temporary_reserve_register(
        &mut self,
        activation: u64,
        site: SiteId,
    ) -> Result<(), ExecutionError> {
        let i = self.temporary_index(activation)?;
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        let metadata = &self
            .executable
            .function(f.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .sites[site.0];
        if i + 1 != self.temporaries.as_ref().unwrap().frames.len()
            || f.phase != Phase::Active
            || f.region != metadata.region
        {
            return Err(self.integrity("invalid trusted receiving temporary activation"));
        }
        let owner_growth = if f.owners.len() == f.owners.capacity() {
            size_of::<Owner>()
        } else {
            0
        };
        let events = &self.temporaries.as_ref().unwrap().events;
        let event_growth = if events.len() == events.capacity() {
            size_of::<Event>()
        } else {
            0
        };
        self.reserve(
            owner_growth
                .checked_add(event_growth)
                .ok_or_else(|| self.integrity("temporary prospective storage overflow"))?,
        )?;
        if owner_growth > 0 {
            self.reserve(size_of::<Owner>())?;
            #[cfg(test)]
            self.trusted_allocation(super::trusted::Allocation::TemporaryOwner, true)?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.temporaries.as_mut().unwrap().frames[i]
                .owners
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("temporary owner allocation")))?;
        }
        #[cfg(test)]
        self.trusted_allocation(super::trusted::Allocation::TemporaryEvent, event_growth > 0)?;
        self.temporary_event_reserve()?;
        Ok(())
    }
    pub(super) fn temporary_take(
        &mut self,
        value: Value,
        expected: Option<Transfer>,
    ) -> Result<Value, ExecutionError> {
        let Value::Temporary(id) = value else {
            return Ok(value);
        };
        self.charge()?;
        let i = self.temporary_index(id.activation)?;
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        let owner = f
            .owners
            .get(id.ordinal)
            .filter(|o| o.custody == Custody::Owned && o.value.is_some())
            .ok_or_else(|| self.integrity("temporary transfer without current owner"))?;
        let destination = self
            .executable
            .function(f.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .sites[owner.site.0]
            .destination;
        if expected.is_some_and(|e| e != destination) {
            return Err(self.integrity("temporary transfer destination mismatch"));
        }
        self.temporary_event_reserve()?;
        let s = self.temporaries.as_mut().unwrap();
        let owner = &mut s.frames[i].owners[id.ordinal];
        owner.custody = Custody::Transferred;
        let value = owner.value.take().unwrap();
        s.events.push(Event::Transfer(id, destination));
        Ok(value)
    }
    pub(super) fn pop_to(
        &mut self,
        destination: Option<Transfer>,
    ) -> Result<Value, ExecutionError> {
        let value = self
            .values
            .pop()
            .ok_or_else(|| self.integrity("missing evaluated value"))?;
        self.temporary_take(value, destination)
    }
    pub(super) fn temporary_root(
        &self,
        activation: u64,
        position: usize,
    ) -> Result<SiteId, ExecutionError> {
        let i = self.temporary_index(activation)?;
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        self.executable
            .function(f.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .regions[f.region.0]
            .roots
            .get(position)
            .copied()
            .ok_or_else(|| self.integrity("missing canonical temporary root"))
    }
    pub(super) fn temporary_child(
        &self,
        site: Option<(u64, SiteId)>,
        index: usize,
    ) -> Result<Option<(u64, SiteId)>, ExecutionError> {
        let Some((activation, site)) = site else {
            return Ok(None);
        };
        let i = self.temporary_index(activation)?;
        let function = self.temporaries.as_ref().unwrap().frames[i].function;
        let flow = self
            .executable
            .function(function)?
            .temporaries
            .as_ref()
            .ok_or_else(|| self.integrity("missing temporary child authority"))?;
        let child = flow
            .sites
            .get(site.0)
            .and_then(|s| s.children.get(index))
            .copied()
            .ok_or_else(|| self.integrity("missing temporary child site"))?;
        Ok(Some((activation, child)))
    }
    pub(super) fn eval_at(
        &mut self,
        frame: u64,
        e: &'a TypedExpr,
        depth: usize,
        site: Option<(u64, SiteId)>,
    ) -> Result<(), ExecutionError> {
        if let Some((activation, site)) = site {
            let i = self.temporary_index(activation)?;
            let function = self.temporaries.as_ref().unwrap().frames[i].function;
            let metadata = &self
                .executable
                .function(function)?
                .temporaries
                .as_ref()
                .unwrap()
                .sites[site.0];
            if metadata.plan.is_some() || metadata.result_plan.is_some() {
                self.task(
                    frame,
                    self.origin,
                    Some(e.span),
                    Action::RegisterTemporary(activation, site),
                )?;
            }
            self.task(
                frame,
                self.origin,
                Some(e.span),
                Action::SiteEval(e, depth, activation, site),
            )
        } else {
            self.eval(frame, e, depth)
        }
    }
    pub(super) fn eval_region(
        &mut self,
        frame: u64,
        e: &'a TypedExpr,
        depth: usize,
        context: Option<Context>,
    ) -> Result<(), ExecutionError> {
        if let Some(c) = context {
            if c.complete {
                self.task(
                    frame,
                    self.origin,
                    None,
                    Action::FinishTemporary(c.activation),
                )?;
            }
        }
        self.eval_at(
            frame,
            e,
            depth,
            context.and_then(|c| c.site.map(|s| (c.activation, s))),
        )
    }
    pub(super) fn temporary_retire(&mut self, activation: u64) -> Result<(), ExecutionError> {
        self.charge()?;
        let i = self.temporary_index(activation)?;
        if i + 1 != self.temporaries.as_ref().unwrap().frames.len() {
            return Err(self.integrity("temporary retirement crosses active child"));
        }
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        let retirement = self
            .executable
            .function(f.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .regions[f.region.0]
            .retirement;
        self.origin = retirement;
        self.expression = None;
        let f = &mut self.temporaries.as_mut().unwrap().frames[i];
        if f.phase == Phase::Active {
            f.phase = Phase::Retiring;
            f.cursor = f.owners.len();
        }
        if f.phase != Phase::Retiring {
            return Err(self.integrity("duplicate temporary retirement"));
        }
        while self.temporaries.as_ref().unwrap().frames[i].cursor > 0 {
            self.charge()?;
            let ordinal = self.temporaries.as_ref().unwrap().frames[i].cursor - 1;
            let owner = &self.temporaries.as_ref().unwrap().frames[i].owners[ordinal];
            if (owner.custody == Custody::Owned) != owner.value.is_some() {
                return Err(self.integrity("temporary custody and payload disagree"));
            }
            match owner.custody {
                Custody::Transferred => {
                    if owner.value.is_some() {
                        return Err(self.integrity("transferred temporary retained payload"));
                    }
                }
                Custody::Owned => {
                    // Successful ordinary expression roots have transferred to
                    // their admitted destination or protected result. An injected
                    // leftover cannot manufacture a source cleanup edge.
                    if !self.executable.domain.primitive() {
                        return Err(self.integrity("unapproved residual expression temporary"));
                    }
                    let f = &self.temporaries.as_ref().unwrap().frames[i];
                    self.origin = self
                        .executable
                        .function(f.function)?
                        .temporaries
                        .as_ref()
                        .unwrap()
                        .sites[owner.site.0]
                        .cleanup;
                    self.expression = Some(self.origin.span);
                    self.temporary_event_reserve()?;
                    let s = self.temporaries.as_mut().unwrap();
                    let owner = &mut s.frames[i].owners[ordinal];
                    owner.custody = Custody::Cleaning;
                    let value = owner.value.take().unwrap();
                    s.events.push(Event::Cleanup(OwnerId {
                        activation,
                        ordinal,
                    }));
                    self.release_value(value)?;
                    self.temporaries.as_mut().unwrap().frames[i].owners[ordinal].custody =
                        Custody::Cleaned;
                }
                _ => return Err(self.integrity("duplicate or resumed temporary cleanup")),
            }
            self.temporaries.as_mut().unwrap().frames[i].cursor = ordinal;
        }
        self.origin = retirement;
        self.expression = None;
        self.charge()?;
        self.temporary_event_reserve()?;
        let s = self.temporaries.as_mut().unwrap();
        s.frames[i].phase = Phase::Retired;
        s.events.push(Event::Retire(activation));
        Ok(())
    }
    pub(super) fn temporary_finish(&mut self, activation: u64) -> Result<(), ExecutionError> {
        self.charge()?;
        let i = self.temporary_index(activation)?;
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        let frame = f.frame;
        let destination = self
            .executable
            .function(f.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .regions[f.region.0]
            .destination;
        if matches!(destination, Transfer::NoValue(_)) {
            // Completion is a control marker, never a dummy owned result.
            if self.values.last() != Some(&Value::Completion) {
                return Err(self.integrity("NoValue temporary completion mismatch"));
            }
            self.temporary_retire(activation)?;
            return self.temporary_end(activation);
        }
        if let Transfer::Replacement(local) = destination {
            self.replacement_accepts_result(frame, local)?;
        }
        let resource_return = destination == Transfer::Return && self.resources.is_some();
        if resource_return
            && (self.staged_return.is_some()
                || self
                    .resources
                    .as_ref()
                    .is_some_and(|s| s.protected.is_some()))
        {
            return Err(self.integrity("protected return already occupied"));
        }
        let value = self.pop_to(Some(destination))?;
        match destination {
            Transfer::Return if resource_return => {
                self.resources.as_mut().unwrap().protected = Some(value);
                self.staged_return = Some(frame);
            }
            Transfer::Replacement(_) => self.replacement_stage_result(value),
            _ => self.temporaries.as_mut().unwrap().frames[i].protected = Some(value),
        }
        self.reserve(0)?;
        self.temporary_retire(activation)?;
        self.reserve(0)?;
        if !resource_return && !matches!(destination, Transfer::Replacement(_)) {
            // Reserved destination staging receives R only after retirement.
            let value = self.temporaries.as_mut().unwrap().frames[i]
                .protected
                .take()
                .unwrap();
            self.values.push(value);
        }
        self.temporary_end(activation)
    }
    pub(super) fn temporary_end(&mut self, activation: u64) -> Result<(), ExecutionError> {
        let s = self.temporaries.as_mut().unwrap();
        let f = s
            .frames
            .last()
            .filter(|f| f.id == activation && f.phase == Phase::Retired && f.protected.is_none())
            .ok_or_else(|| ExecutionError::integrity("unfinished temporary completion"))?;
        let parent = if s.frames.len() > 1 {
            Some(s.frames[s.frames.len() - 2].id)
        } else {
            None
        };
        if f.parent != parent {
            return Err(ExecutionError::integrity(
                "temporary dynamic parent changed",
            ));
        }
        s.frames.pop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::machine::cleanup::{conservation_tests::conserved, Event as Release};

    fn artifact(source: &str) -> CheckedExecutable {
        crate::execution::prepare_synthetic(
            hir::resources::fixture_with(source),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    fn pause_region(m: &mut Machine<'_>, predicate: impl Fn(Transfer) -> bool) -> u64 {
        loop {
            let task = m.tasks.pop().unwrap();
            m.step(task).unwrap();
            if let Some(s) = &m.temporaries {
                if let Some(f) = s.frames.last() {
                    let destination = m
                        .executable
                        .function(f.function)
                        .unwrap()
                        .temporaries
                        .as_ref()
                        .unwrap()
                        .regions[f.region.0]
                        .destination;
                    if f.phase == Phase::Active && predicate(destination) {
                        return f.id;
                    }
                }
            }
        }
    }
    fn completed(m: &Machine<'_>) -> Vec<u64> {
        m.resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Release::Complete(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn private_remaining_abc_frame_cleans_exact_identities_cba() {
        let e = artifact(
            "fn take(a:Leaf,b:Leaf,c:Leaf){}fn demo(){take(acquire(),acquire(),acquire());}",
        );
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        loop {
            let t = m.tasks.pop().unwrap();
            if matches!(t.action, Action::Invoke(_, 3)) {
                break;
            }
            m.step(t).unwrap();
            conserved(&m);
        }
        // Private primitive oracle: no source value-discard or altered call executes.
        assert!(m.values.iter().all(|v| matches!(v, Value::Temporary(_))));
        m.values.clear();
        let activation = m.temporaries.as_ref().unwrap().frames.last().unwrap().id;
        m.temporary_retire(activation).unwrap();
        m.temporary_end(activation).unwrap();
        assert_eq!(completed(&m), vec![2, 1, 0]);
        let owners: Vec<_> = m
            .temporaries
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Cleanup(id) = e {
                    Some(id.ordinal)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(owners, vec![2, 1, 0]);
        assert!(!m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Release::Retire(_))));
        conserved(&m);
    }

    #[test]
    fn local_ur_failure_keeps_result_protected_and_local_unpublished() {
        let e = artifact("fn demo(){let r=acquire();}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let activation = pause_region(&mut m, |d| matches!(d, Transfer::Local(_)));
        let u = m.inject_temporary_leaf(activation).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(u);
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        assert!(matches!(m.frames[0].slots[0].1, Slot::Uninitialized));
        let f = m.temporaries.as_ref().unwrap().frames.last().unwrap();
        assert!(matches!(f.protected, Some(Value::Leaf { identity: 1, .. })));
        assert_eq!(completed(&m), Vec::<u64>::new());
        conserved(&m);
        assert!(matches!(
            m.resume().unwrap_err().kind,
            ErrorKind::Integrity(_)
        ));
    }

    #[test]
    fn local_ur_success_retires_u_before_publishing_r() {
        let e = artifact("fn demo(){let r=acquire();}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let activation = pause_region(&mut m, |d| matches!(d, Transfer::Local(_)));
        m.inject_temporary_leaf(activation).unwrap();
        m.resume().unwrap();
        assert_eq!(completed(&m), vec![0, 1]);
        conserved(&m);
    }

    #[test]
    fn return_result_uses_return_protection_before_operand_cleanup_stop() {
        let e = artifact("fn make()->Leaf{return acquire();}fn demo(){let r=make();}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let activation = pause_region(&mut m, |d| d == Transfer::Return);
        let u = m.inject_temporary_leaf(activation).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(u);
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        assert!(matches!(
            m.resources.as_ref().unwrap().protected,
            Some(Value::Leaf { identity: 1, .. })
        ));
        assert_eq!(m.staged_return, Some(1));
        assert_eq!(m.frames.len(), 2);
        assert!(matches!(m.frames[0].slots[0].1, Slot::Uninitialized));
        assert!(m
            .temporaries
            .as_ref()
            .unwrap()
            .frames
            .last()
            .unwrap()
            .protected
            .is_none());
        conserved(&m);
    }

    #[test]
    fn replacement_result_is_protected_before_u_stop_and_old_displacement() {
        let e = artifact("fn demo(){let mut a=acquire();a=acquire();}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let activation = pause_region(&mut m, |d| matches!(d, Transfer::Replacement(_)));
        let u = m.inject_temporary_leaf(activation).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(u);
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        assert!(matches!(
            m.frames[0].slots[0].1,
            Slot::Available(Value::Leaf { identity: 0, .. })
        ));
        assert!(m
            .temporaries
            .as_ref()
            .unwrap()
            .frames
            .last()
            .unwrap()
            .protected
            .is_none());
        assert!(matches!(
            m.resources
                .as_ref()
                .unwrap()
                .replacements
                .last()
                .unwrap()
                .protected,
            Some(Value::Leaf { identity: 2, .. })
        ));
        assert_eq!(completed(&m), Vec::<u64>::new());
        conserved(&m);
    }

    #[test]
    fn assignment_cleanup_stop_preserves_rhs_prefix_and_prevents_pending_write() {
        let e = artifact("type N=range 0..3;record Cell{x:N}fn next(c:&mut Cell,r:Leaf)->N{c.x=1;return 2;}fn demo(){let mut c=Cell{x:0};c.x=next(&mut c,acquire());}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let activation = pause_region(&mut m, |d| matches!(d, Transfer::Field(_, _, _)));
        let u = m.inject_temporary_leaf(activation).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(u);
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        let Slot::Available(Value::Public(RuntimeValue::Record { fields, .. })) =
            &m.frames[0].slots[0].1
        else {
            panic!()
        };
        assert_eq!(fields[0].1.value, 1);
        assert!(matches!(
            m.temporaries
                .as_ref()
                .unwrap()
                .frames
                .last()
                .unwrap()
                .protected,
            Some(Value::Public(RuntimeValue::Range(RangeValue {
                value: 2,
                ..
            })))
        ));
        assert_eq!(completed(&m), vec![1]);
        conserved(&m);
    }

    #[test]
    fn later_argument_trap_retains_earlier_owner_without_call_entry() {
        let e = artifact("type N=range 0..1;fn zero()->N{return 0;}fn trap()->N{return zero()-1;}fn take(a:Leaf,n:N,b:Leaf){}fn demo(){take(acquire(),trap(),acquire());}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert!(matches!(
            m.resume().unwrap_err().kind,
            ErrorKind::RangeTrap { .. }
        ));
        assert_eq!(completed(&m), Vec::<u64>::new());
        let s = m.resources.as_ref().unwrap();
        assert_eq!(
            s.events
                .iter()
                .filter(|e| matches!(e, Release::Acquire(_)))
                .count(),
            1
        );
        assert_eq!(
            m.temporaries.as_ref().unwrap().frames[0]
                .owners
                .iter()
                .filter(|o| o.value.is_some())
                .count(),
            1
        );
        conserved(&m);
    }

    #[test]
    fn every_call_fuel_cut_retains_partial_parameters_and_refuses_resume() {
        let e = artifact("fn take(a:Leaf,b:Leaf,c:Leaf){let body=acquire();}fn demo(){take(acquire(),acquire(),acquire());}");
        let mut full = Machine::new(&e, ExecutionLimits::default());
        full.start(e.entry("demo").unwrap()).unwrap();
        full.resume().unwrap();
        let mut partial = 0;
        for steps in 1..full.steps {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    steps,
                    ..ExecutionLimits::default()
                },
            );
            let result = m.start(e.entry("demo").unwrap()).and_then(|_| m.resume());
            assert_eq!(
                result.unwrap_err().kind,
                ErrorKind::ConfiguredLimit("steps")
            );
            if m.resources.is_some() {
                conserved(&m);
                if m.frames.len() == 2 && matches!(m.frames[1].slots.len(), 1 | 2) {
                    partial += 1;
                    assert_eq!(
                        m.resources
                            .as_ref()
                            .unwrap()
                            .events
                            .iter()
                            .filter(|e| matches!(e, Release::Acquire(_)))
                            .count(),
                        3
                    );
                }
                assert!(matches!(
                    m.resume().unwrap_err().kind,
                    ErrorKind::Integrity(_)
                ));
            }
        }
        assert!(partial >= 2);
        conserved(&full);
    }

    #[test]
    fn every_argument_storage_cut_conserves_and_exact_peak_succeeds() {
        let e = artifact("fn pass(x:Leaf)->Leaf{return x;}fn take(a:Leaf,b:Leaf){}fn demo(){take(pass(acquire()),acquire());}");
        let mut full = Machine::new(&e, ExecutionLimits::default());
        full.start(e.entry("demo").unwrap()).unwrap();
        full.resume().unwrap();
        let peak = full.max_storage.get();
        for working_storage in 1..=peak {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    working_storage,
                    ..ExecutionLimits::default()
                },
            );
            let result = m.start(e.entry("demo").unwrap()).and_then(|_| m.resume());
            if working_storage == peak {
                assert_eq!(result.unwrap(), ExecutionResult::NoValue);
            } else {
                assert_eq!(
                    result.unwrap_err().kind,
                    ErrorKind::ConfiguredLimit("working storage")
                );
            }
            if m.resources.is_some() {
                conserved(&m);
            }
        }
    }

    #[test]
    fn every_existing_assignment_has_success_and_retirement_stop_neighbors() {
        let bodies = [
            "let mut x=zero();x=next(acquire());",
            "let mut c=Cell{x:0};c.x=next(acquire());",
            "let mut c=Cell{x:0};let h=&mut c;h.x=next(acquire());",
            "let mut x=zero();let h=&mut x;*h=next(acquire());",
        ];
        for (case, body) in bodies.iter().enumerate() {
            for stop in [false, true] {
                let e = artifact(&format!("type N=range 0..3;record Cell{{x:N}}fn zero()->N{{return 0;}}fn next(r:Leaf)->N{{return 2;}}fn demo(){{{body}}}"));
                let mut m = Machine::new(&e, ExecutionLimits::default());
                m.start(e.entry("demo").unwrap()).unwrap();
                let activation = pause_region(&mut m, |d| match case {
                    0 => matches!(d, Transfer::Assign(_)),
                    1 => matches!(d, Transfer::Field(_, _, _)),
                    2 => matches!(d, Transfer::ReferenceField(_, _, _)),
                    _ => matches!(d, Transfer::Dereference(_)),
                });
                let u = m.inject_temporary_leaf(activation).unwrap();
                if stop {
                    m.resources.as_mut().unwrap().failure_at = Some(u);
                    assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
                    assert!(matches!(
                        m.temporaries
                            .as_ref()
                            .unwrap()
                            .frames
                            .last()
                            .unwrap()
                            .protected,
                        Some(Value::Public(RuntimeValue::Range(RangeValue {
                            value: 2,
                            ..
                        })))
                    ));
                } else {
                    loop {
                        let task = m.tasks.pop().unwrap();
                        let commit = matches!(task.action, Action::Commit(_));
                        m.step(task).unwrap();
                        if commit {
                            break;
                        }
                    }
                }
                let value = match &m.frames[0].slots[0].1 {
                    Slot::Available(Value::Public(RuntimeValue::Range(v))) => v.value,
                    Slot::Available(Value::Public(RuntimeValue::Record { fields, .. })) => {
                        fields[0].1.value
                    }
                    _ => panic!("assignment target {case}"),
                };
                assert_eq!(value, if stop { 0 } else { 2 }, "case {case}");
                conserved(&m);
                if !stop {
                    m.resume().unwrap();
                    conserved(&m);
                }
            }
        }
    }

    #[test]
    fn condition_retirement_stop_retains_bool_and_prevents_arm_entry() {
        let e = artifact("fn ready(r:Leaf)->bool{return true;}fn demo(){if ready(acquire()){let arm=acquire();}}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let activation = pause_region(&mut m, |d| d == Transfer::Condition);
        let u = m.inject_temporary_leaf(activation).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(u);
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        assert!(matches!(
            m.temporaries
                .as_ref()
                .unwrap()
                .frames
                .last()
                .unwrap()
                .protected,
            Some(Value::Public(RuntimeValue::Bool(true)))
        ));
        assert_eq!(
            m.resources
                .as_ref()
                .unwrap()
                .events
                .iter()
                .filter(|e| matches!(e, Release::Acquire(_)))
                .count(),
            2
        );
        assert_eq!(completed(&m), vec![1]);
        assert!(m.frames[0].slots.is_empty());
        conserved(&m);
    }

    #[test]
    fn if_selection_occurs_after_retirement_and_only_selected_arm_acquires() {
        for flag in [false, true] {
            let e = artifact(&format!("fn ready(r:Leaf)->bool{{return {flag};}}fn demo(){{if ready(acquire()){{let arm=acquire();}}}}"));
            let mut m = Machine::new(&e, ExecutionLimits::default());
            m.start(e.entry("demo").unwrap()).unwrap();
            loop {
                let task = m.tasks.pop().unwrap();
                if matches!(task.action, Action::Select(_, _)) {
                    assert!(m.temporaries.as_ref().unwrap().is_empty());
                    assert_eq!(
                        m.values.last(),
                        Some(&Value::Public(RuntimeValue::Bool(flag)))
                    );
                    assert_eq!(completed(&m), vec![0]);
                    m.step(task).unwrap();
                    break;
                }
                m.step(task).unwrap();
            }
            m.resume().unwrap();
            assert_eq!(completed(&m), if flag { vec![0, 1] } else { vec![0] });
            conserved(&m);
        }
    }

    #[test]
    fn conditional_initializer_keeps_one_activation_and_skips_other_leaf() {
        for flag in [false, true] {
            let e = artifact(&format!("fn ready(r:Leaf)->bool{{return {flag};}}fn demo(){{let r=if ready(acquire()){{acquire()}}else{{acquire()}};}}"));
            let mut m = Machine::new(&e, ExecutionLimits::default());
            m.start(e.entry("demo").unwrap()).unwrap();
            m.resume().unwrap();
            assert_eq!(completed(&m), vec![0, 1]);
            let flow = e
                .mir
                .functions()
                .last()
                .unwrap()
                .temporaries
                .as_ref()
                .unwrap();
            let root = &flow.sites[flow.regions[0].roots[0].0];
            let omitted = root.children[if flag { 2 } else { 1 }];
            let events = &m.temporaries.as_ref().unwrap().events;
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, Event::Begin(_, 0, _, _)))
                    .count(),
                1
            );
            assert!(!events
                .iter()
                .any(|e| matches!(e, Event::Register(_,site) if *site == omitted)));
            conserved(&m);
        }
    }

    #[test]
    fn loop_statement_activations_are_fresh_and_bool_wrapper_remains_legal() {
        let e = artifact("type N=range 0..2;fn start()->N{return 2;}fn ready(r:Leaf)->bool{return true;}fn demo(){let mut n=start();while !(n==0){let r=acquire();let b=ready(acquire());n=n-1;}}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resume().unwrap();
        assert_eq!(completed(&m), vec![1, 0, 3, 2]);
        let activations: Vec<_> = m
            .temporaries
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Begin(id, 0, RegionId(0), _) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(activations.len(), 2);
        assert_ne!(activations[0], activations[1]);
        conserved(&m);
        let e = artifact("fn ready_wrapper()->bool{let r=acquire();return false;}fn demo(){while ready_wrapper(){}}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resume().unwrap();
        assert_eq!(completed(&m), vec![0]);
        conserved(&m);
    }

    #[test]
    fn source_borrow_discard_and_recurring_condition_exclusions_remain() {
        let prefix = "type R=range 0..1;record Leaf{data:R}fn acquire()->Leaf{return Leaf{data:0};}fn ready(r:Leaf)->bool{return true;}fn inspect(r:&Leaf)->bool{return true;}";
        for body in [
            "inspect(&acquire());",
            "let view=&acquire();",
            "return &acquire();",
        ] {
            assert!(matches!(
                crate::compile_source(&format!("{prefix}fn demo(){{{body}}}")),
                Err(crate::FrontendError::Parse(_))
            ));
        }
        for body in ["acquire();", "ready(acquire());"] {
            assert!(matches!(
                crate::compile_source(&format!("{prefix}fn demo(){{{body}}}")),
                Err(crate::FrontendError::Type(_))
            ));
        }
        for condition in ["ready(acquire())", "true && ready(acquire())"] {
            assert!(matches!(
                crate::compile_source(&format!("{prefix}fn demo(){{while {condition}{{}}}}")),
                Err(crate::FrontendError::Ownership(_))
            ));
        }
        assert!(matches!(
            crate::compile_source(&format!(
                "{prefix}fn demo(){{let r=acquire();while inspect(&r){{}}}}"
            )),
            Err(crate::FrontendError::Ownership(_))
        ));
        let mut p = hir::resources::fixture_with("fn demo(){release(acquire());}");
        let hir::Function::Ordinary(f) = p.functions.last_mut().unwrap() else {
            panic!()
        };
        let hir::ValueStatement::CallNoValue {
            function,
            arguments,
            ..
        } = &mut f.body[0]
        else {
            panic!()
        };
        *function = FunctionId(0);
        arguments.clear();
        assert!(matches!(
            crate::execution::prepare_synthetic(p, AdmissionLimits::default())
                .err()
                .unwrap()
                .kind,
            ErrorKind::Integrity(_)
        ));
    }

    #[test]
    fn holder_census_tracks_same_leaf_across_inner_return_and_outer_parameter() {
        use crate::execution::machine::cleanup::conservation_tests::holders;
        let e = artifact(
            "fn pass(x:Leaf)->Leaf{return x;}fn take(x:Leaf){}fn demo(){take(pass(acquire()));}",
        );
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let mut history = Vec::new();
        let mut release_holder = None;
        while let Some(task) = m.tasks.pop() {
            let before = holders(&m);
            let count = m.resources.as_ref().map_or(0, |s| {
                s.events
                    .iter()
                    .filter(|e| matches!(e, Release::Complete(0)))
                    .count()
            });
            m.step(task).unwrap();
            if m.resources.is_some() {
                conserved(&m);
            }
            for (id, label) in holders(&m) {
                assert_eq!(id, 0);
                history.push(label);
            }
            if m.resources.as_ref().is_some_and(|s| {
                s.events
                    .iter()
                    .filter(|e| matches!(e, Release::Complete(0)))
                    .count()
                    > count
            }) {
                release_holder = before
                    .into_iter()
                    .find(|(id, _)| *id == 0)
                    .map(|(_, label)| label);
            }
        }
        let mut offset = 0;
        for prefix in [
            "temporary:0:owner:0",
            "frame:1:Parameter",
            "protected-return",
            "temporary:0:owner:1",
            "frame:2:Parameter",
        ] {
            offset += history[offset..]
                .iter()
                .position(|label| label.starts_with(prefix))
                .unwrap()
                + 1;
        }
        assert!(release_holder.unwrap().starts_with("frame:2:Parameter"));
        assert_eq!(completed(&m), vec![0]);
        assert!(!m
            .temporaries
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::Cleanup(_))));
    }

    #[test]
    fn each_detected_runtime_allocation_failure_is_host_and_conserves_payloads() {
        let e = artifact("fn pass(x:Leaf)->Leaf{return x;}fn take(a:Leaf,b:Leaf,c:Leaf){let local=acquire();}fn demo(){take(acquire(),pass(acquire()),acquire());}");
        let take = e
            .program
            .functions()
            .iter()
            .find(|f| f.name() == "take")
            .unwrap()
            .id();
        let mut success = None;
        let mut after_acquisition = 0;
        let mut partial_entry = 0;
        for cut in 0..512 {
            let mut m = Machine::new(&e, ExecutionLimits::default());
            FAIL_ALLOCATION.with(|f| f.set(Some(cut)));
            let result = m.start(e.entry("demo").unwrap()).and_then(|_| m.resume());
            FAIL_ALLOCATION.with(|f| f.set(None));
            if result.is_ok() {
                success = Some(cut);
                conserved(&m);
                break;
            }
            let error = result.unwrap_err();
            assert!(
                matches!(error.kind, ErrorKind::Host(_)),
                "allocation {cut}: {error:?}"
            );
            if m.resources.is_some() {
                conserved(&m);
                if m.resources
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .any(|e| matches!(e, Release::Acquire(_)))
                {
                    after_acquisition += 1;
                }
                if m.frames
                    .last()
                    .is_some_and(|f| f.function == take && matches!(f.slots.len(), 1 | 2))
                {
                    partial_entry += 1;
                    assert_eq!(
                        m.resources
                            .as_ref()
                            .unwrap()
                            .events
                            .iter()
                            .filter(|e| matches!(e, Release::Acquire(_)))
                            .count(),
                        3
                    );
                }
            }
        }
        assert!(success.unwrap() > 20);
        assert!(after_acquisition > 10);
        assert!(partial_entry > 0);
    }

    #[test]
    fn failed_acquisition_allocates_no_resource_identity_or_responsibility() {
        use crate::execution::machine::cleanup::conservation_tests::holders;
        let e = artifact("fn take(x:Leaf){}fn demo(){take(acquire());}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        loop {
            let task = m.tasks.pop().unwrap();
            if matches!(task.action, Action::Invoke(FunctionId(0), 0)) {
                m.tasks.push(task);
                break;
            }
            m.step(task).unwrap();
        }
        FAIL_ALLOCATION.with(|f| f.set(Some(0)));
        let result = m.resume();
        FAIL_ALLOCATION.with(|f| f.set(None));
        assert!(matches!(result.unwrap_err().kind, ErrorKind::Host(_)));
        assert!(holders(&m).is_empty());
        assert!(!m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Release::Acquire(_))));
        conserved(&m);
    }

    #[test]
    fn malformed_runtime_custody_never_uses_vacancy_or_borrowable_temporary_places() {
        let e = artifact("fn demo(){let r=acquire();}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let activation = pause_region(&mut m, |d| matches!(d, Transfer::Local(_)));
        m.inject_temporary_leaf(activation).unwrap();
        assert!(matches!(
            m.reference(
                Location {
                    frame: 0,
                    place: Place::Local(hir::LocalId(0))
                },
                false
            )
            .unwrap_err()
            .kind,
            ErrorKind::Integrity(_)
        ));
        let id = OwnerId {
            activation,
            ordinal: 0,
        };
        assert!(matches!(
            m.temporary_take(
                Value::Temporary(id),
                Some(Transfer::Parameter(hir::ParameterId(999)))
            )
            .unwrap_err()
            .kind,
            ErrorKind::Integrity(_)
        ));
        assert!(matches!(
            m.temporary_take(
                Value::Temporary(OwnerId {
                    activation,
                    ordinal: 999
                }),
                None
            )
            .unwrap_err()
            .kind,
            ErrorKind::Integrity(_)
        ));
        conserved(&m);
        let value = m.temporary_take(Value::Temporary(id), None).unwrap();
        m.value(value).unwrap();
        assert!(matches!(
            m.temporary_take(Value::Temporary(id), None)
                .unwrap_err()
                .kind,
            ErrorKind::Integrity(_)
        ));
        m.temporaries.as_mut().unwrap().frames[0].owners[0].custody = Custody::Owned;
        assert!(matches!(
            m.temporary_retire(activation).unwrap_err().kind,
            ErrorKind::Integrity(_)
        ));
        conserved(&m);
    }

    #[test]
    fn forged_resource_leaf_construction_cannot_manufacture_an_acquisition() {
        let mut p = hir::resources::fixture_with("fn demo(){release(acquire());}");
        let hir::Function::Ordinary(f) = p.functions.last_mut().unwrap() else {
            panic!()
        };
        let hir::ValueStatement::CallNoValue { arguments, .. } = &mut f.body[0] else {
            panic!()
        };
        arguments[0].kind = ExprKind::RecordConstruct {
            record: hir::RecordId(0),
            fields: Vec::new(),
        };
        assert!(matches!(
            crate::execution::prepare_synthetic(p, AdmissionLimits::default())
                .err()
                .unwrap()
                .kind,
            ErrorKind::Integrity(_)
        ));
    }

    #[test]
    fn every_ur_fuel_cut_retains_unpublished_result_and_completed_prefix() {
        let e = artifact("fn demo(){let r=acquire();}");
        let seeded = || {
            let mut m = Machine::new(&e, ExecutionLimits::default());
            m.start(e.entry("demo").unwrap()).unwrap();
            let activation = pause_region(&mut m, |d| matches!(d, Transfer::Local(_)));
            m.inject_temporary_leaf(activation).unwrap();
            m
        };
        let mut full = seeded();
        let start = full.steps;
        full.resume().unwrap();
        let mut protected_stops = 0;
        for steps in start..=full.steps {
            let mut m = seeded();
            m.limits.steps = steps;
            let result = m.resume();
            if steps == full.steps {
                assert_eq!(result.unwrap(), ExecutionResult::NoValue);
            } else {
                assert_eq!(
                    result.unwrap_err().kind,
                    ErrorKind::ConfiguredLimit("steps")
                );
                if let Some(f) = m.temporaries.as_ref().unwrap().frames.last() {
                    if f.phase == Phase::Retiring
                        && matches!(f.protected, Some(Value::Leaf { identity: 1, .. }))
                        && !completed(&m).contains(&0)
                    {
                        protected_stops += 1;
                        assert!(matches!(m.frames[0].slots[0].1, Slot::Uninitialized));
                    }
                }
                assert!(matches!(
                    m.resume().unwrap_err().kind,
                    ErrorKind::Integrity(_)
                ));
            }
            conserved(&m);
        }
        assert!(protected_stops > 0);
    }
    #[test]
    fn propagation_private_sibling_retirement_precedes_tag_selection() {
        let source="fn wrap(x:Leaf)->Result<success:Leaf,failure:bool>{return Ok(x);}fn sample()->Result<success:bool,failure:bool>{let value=wrap(acquire()) or return;return Ok(true);}fn demo(){match sample(){Ok(v)=>{}Error(e)=>{}}}";
        let e = artifact(source);
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let activation = loop {
            let task = m.tasks.pop().unwrap();
            m.step(task).unwrap();
            if let Some(f) = m.temporaries.as_ref().and_then(|s| s.frames.last()) {
                let definition = e.function(f.function).unwrap();
                if e.program.function(f.function).name() == "sample"
                    && f.phase == Phase::Active
                    && definition.temporaries.as_ref().unwrap().regions[f.region.0].destination
                        == Transfer::Match
                {
                    break f.id;
                }
            }
        };
        let sibling = m.inject_temporary_leaf(activation).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(sibling);
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        conserved(&m);
        assert!(m.results.as_ref().unwrap().matches.is_empty());
        let frame = m.temporaries.as_ref().unwrap().frames.last().unwrap();
        assert!(matches!(frame.protected, Some(Value::Result(_))));
        assert!(completed(&m).is_empty());
        assert!(m.resume().is_err());
    }
}
