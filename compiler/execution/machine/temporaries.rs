//! Explicit expression custody; neither history nor operand handles own payloads.
use super::*;
use crate::mir::temporaries::{Destination as Transfer, Point, RegionId, SiteId};

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
}

impl<'a> Machine<'a> {
    fn temporary_event_reserve(&mut self) -> Result<(), ExecutionError> {
        let s = self.temporaries.as_ref().unwrap();
        if s.events.len() == s.events.capacity() {
            self.reserve(size_of::<Event>())?;
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
        self.enable_resources()?;
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
        if f.phase != Phase::Active
            || f.region != metadata.region
            || metadata.plan.is_none()
            || !matches!(self.values.last(), Some(Value::Leaf {record,..} | Value::Aggregate {record,..}) if Some(*record) == metadata.plan)
        {
            return Err(self.integrity("invalid temporary creation type, region or plan"));
        }
        let ordinal = f.owners.len();
        if ordinal == f.owners.capacity() {
            self.reserve(size_of::<Owner>())?;
            self.temporaries.as_mut().unwrap().frames[i]
                .owners
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("temporary owner allocation")))?;
        }
        self.temporary_event_reserve()?;
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
            if self
                .executable
                .function(function)?
                .temporaries
                .as_ref()
                .unwrap()
                .sites[site.0]
                .plan
                .is_some()
            {
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
        let i = self.temporary_index(activation)?;
        if i + 1 != self.temporaries.as_ref().unwrap().frames.len() {
            return Err(self.integrity("temporary retirement crosses active child"));
        }
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
            match owner.custody {
                Custody::Transferred => {
                    if owner.value.is_some() {
                        return Err(self.integrity("transferred temporary retained payload"));
                    }
                }
                Custody::Owned => {
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
        self.charge()?;
        self.temporary_event_reserve()?;
        let s = self.temporaries.as_mut().unwrap();
        s.frames[i].phase = Phase::Retired;
        s.events.push(Event::Retire(activation));
        Ok(())
    }
    pub(super) fn temporary_finish(&mut self, activation: u64) -> Result<(), ExecutionError> {
        let i = self.temporary_index(activation)?;
        let f = &self.temporaries.as_ref().unwrap().frames[i];
        let destination = self
            .executable
            .function(f.function)?
            .temporaries
            .as_ref()
            .unwrap()
            .regions[f.region.0]
            .destination;
        let value = self.pop_to(Some(destination))?;
        self.temporaries.as_mut().unwrap().frames[i].protected = Some(value);
        self.reserve(0)?;
        self.temporary_retire(activation)?;
        // The popped result leaves a reserved operand entry for destination staging.
        self.reserve(0)?;
        let value = self.temporaries.as_mut().unwrap().frames[i]
            .protected
            .take()
            .unwrap();
        self.values.push(value);
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
