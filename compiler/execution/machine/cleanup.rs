//! Private synthetic resource execution. Fixed operations never call user code.
use super::*;
use crate::hir::resources::OperationKind;
use crate::mir::cleanup::ScopeId;
#[cfg(test)]
mod effects;
mod replacement;
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    Acquire(u64),
    Attempt(u64),
    Complete(u64),
    Failure(u64),
    Retire(u64),
}

#[cfg(test)]
mod expression_temporary_tests {
    use super::*;
    use crate::execution::machine::temporaries::Event as TemporaryEvent;

    fn artifact(source: &str) -> CheckedExecutable {
        crate::execution::prepare_synthetic(
            hir::resources::fixture_with(source),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    fn completed(m: &Machine<'_>) -> Vec<u64> {
        m.resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Complete(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn direct_argument_transfers_to_callee_without_caller_cleanup() {
        let e = artifact("fn take(x:Leaf){}fn demo(){take(acquire());}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        while let Some(task) = m.tasks.pop() {
            m.step(task).unwrap();
            super::conservation_tests::conserved(&m);
        }
        assert_eq!(completed(&m), vec![0]);
        assert!(!m
            .temporaries
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, TemporaryEvent::Cleanup(_))));
        assert!(m
            .resources
            .as_ref()
            .unwrap()
            .events
            .contains(&Event::Retire(1)));
        assert!(m.temporaries.as_ref().unwrap().is_empty());
        assert_eq!(m.resume().unwrap(), ExecutionResult::NoValue);
    }

    #[test]
    fn three_arguments_preserve_written_acquisition_and_parameter_cleanup() {
        let e = artifact(
            "fn take(a:Leaf,b:Leaf,c:Leaf){}fn demo(){take(acquire(),acquire(),acquire());}",
        );
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resume().unwrap();
        assert_eq!(completed(&m), vec![2, 1, 0]);
        let acquisitions: Vec<_> = m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Acquire(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(acquisitions, vec![0, 1, 2]);
        assert!(!m
            .temporaries
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, TemporaryEvent::Cleanup(_))));
        super::conservation_tests::conserved(&m);
    }

    #[test]
    fn nested_pass_through_keeps_one_leaf_through_return_and_outer_entry() {
        let e = artifact(
            "fn pass(x:Leaf)->Leaf{return x;}fn take(x:Leaf){}fn demo(){take(pass(acquire()));}",
        );
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        while let Some(task) = m.tasks.pop() {
            m.step(task).unwrap();
            super::conservation_tests::conserved(&m);
        }
        assert_eq!(completed(&m), vec![0]);
        assert_eq!(
            m.resources
                .as_ref()
                .unwrap()
                .events
                .iter()
                .filter(|e| matches!(e, Event::Acquire(_)))
                .count(),
            1
        );
        assert_eq!(
            m.temporaries
                .as_ref()
                .unwrap()
                .events
                .iter()
                .filter(|e| matches!(e, TemporaryEvent::Transfer(_, _)))
                .count(),
            3
        );
        assert_eq!(m.resume().unwrap(), ExecutionResult::NoValue);
    }
}
#[derive(Default)]
pub(super) struct State {
    pub(super) events: Vec<Event>,
    frames: Vec<ResourceFrame>,
    next_identity: u64,
    next_activation: u64,
    next_generation: u64,
    pub(super) replacements: Vec<replacement::Prepared>,
    next_replacement: u64,
    #[cfg(test)]
    fail_replacement: Option<replacement::Phase>,
    pub(super) protected: Option<Value>,
    pub(super) detached: Option<Value>,
    pub(super) current: Option<Value>,
    pub(super) failure_at: Option<u64>,
    pub(super) terminal: bool,
    pending: Vec<Pending>,
    work: Vec<Value>,
    next_pending: u64,
    pub(super) fail_field: Option<(hir::RecordId, hir::FieldId)>,
    pub(super) fail_field_skip: usize,
}
struct Pending {
    id: u64,
    frame: u64,
    record: hir::RecordId,
    initialized: Vec<(hir::FieldId, Value)>,
    complete: bool,
    temporary_site: Option<(u64, crate::mir::temporaries::SiteId)>,
    child_activation: Option<u64>,
}
struct ResourceFrame {
    id: u64,
    entered: bool,
    scopes: Vec<Activation>,
    // Slot association is distinct from ordering history and owns no payload.
    generations: Vec<(hir::LocalId, u64)>,
}
#[derive(Clone, Copy)]
struct Generation {
    local: hir::LocalId,
    identity: u64,
    discharged: bool,
}
struct Activation {
    scope: ScopeId,
    identity: u64,
    initialized: Vec<Generation>,
}
impl State {
    pub(super) fn work_is_full(&self) -> bool {
        self.work.len() == self.work.capacity()
    }
    pub(super) fn reserve_work(&mut self) -> Result<(), ()> {
        self.work.try_reserve_exact(1).map_err(|_| ())
    }
    pub(super) fn push_reserved_work(&mut self, value: Value) {
        self.work.push(value);
    }
    pub(super) fn storage(&self) -> Option<usize> {
        let mut n = size_of::<Self>()
            .checked_add(self.events.capacity().checked_mul(size_of::<Event>())?)?
            .checked_add(
                self.frames
                    .capacity()
                    .checked_mul(size_of::<ResourceFrame>())?,
            )?;
        n = n
            .checked_add(self.pending.capacity().checked_mul(size_of::<Pending>())?)?
            .checked_add(self.work.capacity().checked_mul(size_of::<Value>())?)?
            .checked_add(
                self.replacements
                    .capacity()
                    .checked_mul(size_of::<replacement::Prepared>())?,
            )?;
        for r in &self.replacements {
            n = n.checked_add(r.payload_storage()?)?;
        }
        for p in &self.pending {
            n = n.checked_add(
                p.initialized
                    .capacity()
                    .checked_mul(size_of::<(hir::FieldId, Value)>())?,
            )?;
            for (_, v) in &p.initialized {
                n = n.checked_add(v.extra_storage())?;
            }
        }
        for v in &self.work {
            n = n.checked_add(v.extra_storage())?;
        }
        for f in &self.frames {
            n = n.checked_add(f.scopes.capacity().checked_mul(size_of::<Activation>())?)?;
            n = n.checked_add(
                f.generations
                    .capacity()
                    .checked_mul(size_of::<(hir::LocalId, u64)>())?,
            )?;
            for a in &f.scopes {
                n = n.checked_add(
                    a.initialized
                        .capacity()
                        .checked_mul(size_of::<Generation>())?,
                )?;
            }
        }
        for v in [&self.protected, &self.current, &self.detached]
            .into_iter()
            .flatten()
        {
            n = n.checked_add(v.extra_storage())?;
        }
        Some(n)
    }
}
impl<'a> Machine<'a> {
    pub(super) fn completion_cleanup_idle(&self) -> bool {
        self.resources.as_ref().is_some_and(|s| s.detached.is_none() && s.current.is_none() && s.work.is_empty())
    }
    pub(super) fn completion_cleanup_growth(&self) -> Result<usize, ExecutionError> {
        let bytes = self
            .resources
            .as_ref()
            .map_or(size_of::<State>() + size_of::<Value>(), |s| {
                usize::from(s.work.len() == s.work.capacity()) * size_of::<Value>()
            });
        Ok(bytes)
    }

    pub(super) fn completion_cleanup_reserve(&mut self) -> Result<(), ExecutionError> {
        self.enable_resources()?;
        let growth = self.resources.as_ref().unwrap().work.len()
            == self.resources.as_ref().unwrap().work.capacity();
        if growth {
            self.reserve(size_of::<Value>())?;
            #[cfg(test)]
            self.trusted_allocation(super::trusted::Allocation::CompletionCleanup, true)?;
            self.resources
                .as_mut()
                .unwrap()
                .work
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("completion fixed cleanup cursor")))?;
        }
        Ok(())
    }

    pub(super) fn enable_resources(&mut self) -> Result<(), ExecutionError> {
        if self.resources.is_none() {
            self.reserve(size_of::<State>())?;
            self.resources = Some(State::default());
        }
        Ok(())
    }
    fn has_cleanup(&self, id: u64) -> Result<bool, ExecutionError> {
        let f = &self.frames[self.frame_index(id)?];
        Ok(self.executable.function(f.function)?.cleanup.is_some())
    }
    fn resource_frame_index(&self, id: u64) -> Result<usize, ExecutionError> {
        self.resources
            .as_ref()
            .and_then(|s| s.frames.iter().position(|f| f.id == id))
            .ok_or_else(|| self.integrity("missing cleanup frame"))
    }
    fn event(&mut self, event: Event) -> Result<(), ExecutionError> {
        if self.resources.as_ref().unwrap().events.len()
            == self.resources.as_ref().unwrap().events.capacity()
        {
            self.reserve(size_of::<Event>())?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.resources
                .as_mut()
                .unwrap()
                .events
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("resource event allocation")))?;
        }
        self.resources.as_mut().unwrap().events.push(event);
        Ok(())
    }
    pub(super) fn cleanup_frame(&mut self, id: u64) -> Result<(), ExecutionError> {
        if !self.has_cleanup(id)? {
            return Ok(());
        }

        if self.resources.is_none() {
            return Ok(());
        }
        let s = self.resources.as_ref().unwrap();
        if s.frames.len() == s.frames.capacity() {
            self.reserve(size_of::<ResourceFrame>())?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.resources
                .as_mut()
                .unwrap()
                .frames
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("cleanup frame allocation")))?;
        }
        self.resources.as_mut().unwrap().frames.push(ResourceFrame {
            id,
            entered: false,
            scopes: Vec::new(),
            generations: Vec::new(),
        });
        Ok(())
    }
    pub(super) fn cleanup_enter_block(&mut self, id: u64) -> Result<(), ExecutionError> {
        if !self.has_cleanup(id)? {
            return Ok(());
        }

        if self.resources.is_none() {
            return Ok(());
        }
        let r = self.resource_frame_index(id)?;
        if self.resources.as_ref().unwrap().frames[r].entered {
            return Ok(());
        }
        let f = &self.frames[self.frame_index(id)?];
        let flow = self
            .executable
            .function(f.function)?
            .cleanup
            .as_ref()
            .ok_or_else(|| self.integrity("cleanup flow missing"))?;
        if let Some((scope, origin)) = flow.blocks[f.block.0].enter {
            self.origin = origin;
            self.charge()?;
            let parent = flow.scopes[scope.0].parent;
            if self.resources.as_ref().unwrap().frames[r]
                .scopes
                .last()
                .map(|a| a.scope)
                != parent
            {
                return Err(self.integrity("runtime lexical activation parent"));
            }
            let grow = {
                let v = &self.resources.as_ref().unwrap().frames[r].scopes;
                v.len() == v.capacity()
            };
            if grow {
                self.reserve(size_of::<Activation>())?;
                #[cfg(test)]
                self.temporary_allocation()?;
                self.resources.as_mut().unwrap().frames[r]
                    .scopes
                    .try_reserve_exact(1)
                    .map_err(|_| self.failure(ErrorKind::Host("scope activation allocation")))?;
            }
            let s = self.resources.as_mut().unwrap();
            let identity = s.next_activation;
            s.next_activation = s
                .next_activation
                .checked_add(1)
                .ok_or_else(|| ExecutionError::integrity("scope activation identity exhausted"))?;
            s.frames[r].scopes.push(Activation {
                scope,
                identity,
                initialized: Vec::new(),
            });
        }
        self.resources.as_mut().unwrap().frames[r].entered = true;
        Ok(())
    }
    pub(super) fn cleanup_next_block(&mut self, id: u64) -> Result<(), ExecutionError> {
        if !self.has_cleanup(id)? {
            return Ok(());
        }

        if self.resources.is_some() {
            let r = self.resource_frame_index(id)?;
            self.resources.as_mut().unwrap().frames[r].entered = false;
        }
        Ok(())
    }
    pub(super) fn cleanup_reinitialization(&self, at: Location) -> Result<(), ExecutionError> {
        if !self.has_cleanup(at.frame)? {
            return Ok(());
        }

        if self.resources.is_none() {
            return Ok(());
        }
        let r = self.resource_frame_index(at.frame)?;
        if self.resources.as_ref().unwrap().frames[r]
            .scopes
            .iter()
            .any(|s| {
                s.initialized
                    .iter()
                    .any(|g| at.place == Place::Local(g.local))
            })
        {
            return Err(self.integrity("owner slot reused before activation cleanup"));
        }
        if self
            .slot(at)
            .is_ok_and(|value| self.resource_bearing(value))
        {
            return Err(self.integrity("live resource overwritten at initialization"));
        }
        Ok(())
    }
    pub(super) fn cleanup_initialized(&mut self, at: Location) -> Result<(), ExecutionError> {
        if !self.has_cleanup(at.frame)? {
            return Ok(());
        }

        if self.resources.is_none() {
            return Ok(());
        }
        let r = self.resource_frame_index(at.frame)?;
        let f = &self.frames[self.frame_index(at.frame)?];
        let flow = self
            .executable
            .function(f.function)?
            .cleanup
            .as_ref()
            .unwrap();
        let Some(init) = flow.blocks[f.block.0]
            .initialize
            .iter()
            .find(|i| i.statement + 1 == f.statement)
        else {
            return Ok(());
        };
        if at.place != Place::Local(init.local) {
            return Err(self.integrity("successful initialization destination mismatch"));
        }
        let a = self.resources.as_ref().unwrap().frames[r]
            .scopes
            .last()
            .ok_or_else(|| self.integrity("owner initialized outside activation"))?;
        if a.scope != init.scope || a.initialized.iter().any(|g| g.local == init.local) {
            return Err(self.integrity("duplicate or wrong-scope initialization"));
        }
        let grow = a.initialized.len() == a.initialized.capacity();
        let association = &self.resources.as_ref().unwrap().frames[r].generations;
        let map_grow = !association.iter().any(|(local, _)| *local == init.local)
            && association.len() == association.capacity();
        let identity = self.resources.as_ref().unwrap().next_generation;
        let next = identity
            .checked_add(1)
            .ok_or_else(|| self.integrity("generation identity exhausted"))?;
        self.origin = init.origin;
        self.charge()?;
        self.reserve(
            (if grow { size_of::<Generation>() } else { 0 })
                + if map_grow {
                    size_of::<(hir::LocalId, u64)>()
                } else {
                    0
                },
        )?;
        if grow {
            #[cfg(test)]
            self.temporary_allocation()?;
            self.resources.as_mut().unwrap().frames[r]
                .scopes
                .last_mut()
                .unwrap()
                .initialized
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("initialization history allocation")))?;
        }
        if map_grow {
            #[cfg(test)]
            self.temporary_allocation()?;
            self.resources.as_mut().unwrap().frames[r]
                .generations
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("generation association allocation")))?;
        }
        let s = self.resources.as_mut().unwrap();
        s.next_generation = next;
        let associations = &mut s.frames[r].generations;
        if let Some((_, current)) = associations
            .iter_mut()
            .find(|(local, _)| *local == init.local)
        {
            *current = identity;
        } else {
            associations.push((init.local, identity));
        }
        self.resources.as_mut().unwrap().frames[r]
            .scopes
            .last_mut()
            .unwrap()
            .initialized
            .push(Generation {
                local: init.local,
                identity,
                discharged: false,
            });
        Ok(())
    }
    fn release_leaf(&mut self, value: Value) -> Result<(), ExecutionError> {
        // Detachment does not evade accounting: the current cleanup payload stays
        // retained even when a terminal host outcome prevents completion.
        self.reserve(value.extra_storage())?;
        if self.resources.as_ref().unwrap().current.is_some() {
            return Err(self.integrity("cleanup cursor already occupied"));
        }
        self.resources.as_mut().unwrap().current = Some(value);
        self.charge()?;
        let (resource, record, identity) =
            match self.resources.as_ref().unwrap().current.as_ref().unwrap() {
                Value::Leaf {
                    resource,
                    record,
                    identity,
                } => (*resource, *record, *identity),
                _ => return Err(self.integrity("cleanup targets inert value or reference")),
            };
        let leaf = self
            .executable
            .program
            .resources
            .leaves
            .get(resource.0)
            .filter(|l| l.id == resource && l.record == record)
            .ok_or_else(|| self.integrity("cleanup leaf identity mismatch"))?;
        if self
            .executable
            .program
            .resources
            .operations
            .get(leaf.release.0)
            .map(|o| o.kind)
            != Some(OperationKind::Release(resource))
        {
            return Err(self.integrity("cleanup trusted release pairing"));
        }
        let needed = 2usize.saturating_sub(
            self.resources.as_ref().unwrap().events.capacity()
                - self.resources.as_ref().unwrap().events.len(),
        );
        if needed > 0 {
            self.reserve(
                needed
                    .checked_mul(size_of::<Event>())
                    .ok_or_else(|| self.integrity("release event reservation overflow"))?,
            )?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.resources
                .as_mut()
                .unwrap()
                .events
                .try_reserve_exact(2)
                .map_err(|_| self.failure(ErrorKind::Host("release event allocation")))?;
        }
        self.event(Event::Attempt(identity))?;
        if self.resources.as_ref().unwrap().failure_at == Some(identity) {
            self.event(Event::Failure(identity))?;
            self.resources.as_mut().unwrap().terminal = true;
            return Err(self.failure(ErrorKind::Host("synthetic trusted release")));
        }
        self.event(Event::Complete(identity))?;
        self.resources.as_mut().unwrap().current = None;
        Ok(())
    }
    fn queue_cleanup(&mut self, value: Value) -> Result<(), ExecutionError> {
        if self.resources.as_ref().unwrap().detached.is_some() {
            return Err(self.integrity("detached cleanup payload already occupied"));
        }
        self.resources.as_mut().unwrap().detached = Some(value);
        let grow = self.resources.as_ref().unwrap().work.len()
            == self.resources.as_ref().unwrap().work.capacity();
        self.reserve(if grow { size_of::<Value>() } else { 0 })?;
        if grow {
            #[cfg(test)]
            self.temporary_allocation()?;
            self.resources
                .as_mut()
                .unwrap()
                .work
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("cleanup continuation allocation")))?;
        }
        let value = self.resources.as_mut().unwrap().detached.take().unwrap();
        self.resources.as_mut().unwrap().work.push(value);
        Ok(())
    }
    pub(super) fn release_value(&mut self, value: Value) -> Result<(), ExecutionError> {
        self.queue_cleanup(value)?;
        while let Some(value) = self.resources.as_mut().unwrap().work.pop() {
            self.resources.as_mut().unwrap().current = Some(value);
            self.charge()?;
            let value = self.resources.as_mut().unwrap().current.take().unwrap();
            match value {
                Value::Result(id) => self.result_cleanup(id)?,
                Value::Leaf { .. } => self.release_leaf(value)?,
                Value::Resource { .. } => self.release_sensor(value)?,
                Value::Aggregate { record, fields } => {
                    let Some(hir::resources::CleanupPlan::Fields(plan)) = self
                        .executable
                        .cleanup_plans
                        .get(record.0)
                        .and_then(Option::as_ref)
                    else {
                        return Err(self.integrity("aggregate has no canonical cleanup plan"));
                    };
                    self.resources.as_mut().unwrap().current =
                        Some(Value::Aggregate { record, fields });
                    // Plans are reverse declaration order; push in opposite order
                    // so the explicit LIFO continuation executes canonical order.
                    for (field, child) in plan.iter().rev() {
                        self.charge()?;
                        let position =
                            match self.resources.as_ref().unwrap().current.as_ref().unwrap() {
                                Value::Aggregate { fields, .. } => {
                                    fields.iter().position(|(id, _)| id == field)
                                }
                                _ => None,
                            }
                            .ok_or_else(|| self.integrity("cleanup field membership mismatch"))?;
                        let value = match self.resources.as_mut().unwrap().current.as_mut().unwrap()
                        {
                            Value::Aggregate { fields, .. } => fields.remove(position).1,
                            _ => unreachable!(),
                        };
                        if !matches!(&value,Value::Leaf {record,..}|Value::Aggregate {record,..} if record==child)
                        {
                            return Err(self.integrity("cleanup child canonical type mismatch"));
                        }
                        self.queue_cleanup(value)?;
                    }
                    self.resources.as_mut().unwrap().current = None;
                }
                _ => return Err(self.integrity("cleanup targets inert value or reference")),
            }
        }
        Ok(())
    }
    fn cleanup_owner(&mut self, at: Location) -> Result<(), ExecutionError> {
        self.charge()?;
        let (f, i) = self.slot_index(at)?;
        if matches!(self.frames[f].slots[i].1, Slot::Transferred) {
            return Ok(());
        }
        if !matches!(
            self.frames[f].slots[i].1,
            Slot::Available(Value::Leaf { .. } | Value::Aggregate { .. } | Value::Resource { .. })
        ) {
            return Err(
                self.integrity("cleanup owner has no validated live or transferred responsibility")
            );
        }
        let Slot::Available(v) = std::mem::replace(&mut self.frames[f].slots[i].1, Slot::Cleaned)
        else {
            unreachable!()
        };
        self.release_value(v)
    }
    pub(super) fn cleanup_exit(&mut self, id: u64) -> Result<(), ExecutionError> {
        if !self.has_cleanup(id)? {
            return Ok(());
        }

        if self.resources.is_none() {
            return Ok(());
        }
        if self.resources.as_ref().unwrap().terminal {
            return Err(self.integrity("cleanup cannot resume after terminal release failure"));
        }
        let r = self.resource_frame_index(id)?;
        let frame = &self.frames[self.frame_index(id)?];
        let block = &self
            .executable
            .function(frame.function)?
            .cleanup
            .as_ref()
            .unwrap()
            .blocks[frame.block.0];
        if let Some(origin) = block.exit_origin {
            self.origin = origin;
            self.expression = None;
        }
        for scope in &block.exit {
            self.charge()?;
            let a = self.resources.as_ref().unwrap().frames[r]
                .scopes
                .last()
                .ok_or_else(|| self.integrity("cleanup activation already consumed"))?;
            if a.scope != *scope || a.identity >= self.resources.as_ref().unwrap().next_activation {
                return Err(self.integrity("cleanup exits wrong active scope"));
            }
            loop {
                let generation = self.resources.as_ref().unwrap().frames[r]
                    .scopes
                    .last()
                    .unwrap()
                    .initialized
                    .last()
                    .copied();
                let Some(generation) = generation else {
                    break;
                };
                if !generation.discharged {
                    let current = self.resources.as_ref().unwrap().frames[r]
                        .generations
                        .iter()
                        .find(|(local, _)| *local == generation.local)
                        .map(|(_, identity)| *identity);
                    if current != Some(generation.identity) {
                        return Err(self.integrity("stale generation cannot clean current slot"));
                    }
                    self.cleanup_owner(Location {
                        frame: id,
                        place: Place::Local(generation.local),
                    })?;
                } else {
                    self.charge()?;
                }
                self.resources.as_mut().unwrap().frames[r]
                    .scopes
                    .last_mut()
                    .unwrap()
                    .initialized
                    .pop();
            }
            self.resources.as_mut().unwrap().frames[r].scopes.pop();
        }
        Ok(())
    }
    pub(super) fn cleanup_return(&mut self, id: u64, value: Value) -> Result<(), ExecutionError> {
        if self.resources.as_ref().unwrap().protected.is_some() {
            return Err(self.integrity("protected return already occupied"));
        }
        self.resources.as_mut().unwrap().protected = Some(value);
        self.cleanup_return_protected(id)
    }
    pub(super) fn cleanup_return_protected(&mut self, id: u64) -> Result<(), ExecutionError> {
        self.reserve(0)?;
        self.cleanup_exit(id)?;
        let r = self.resource_frame_index(id)?;
        if !self.resources.as_ref().unwrap().frames[r].scopes.is_empty() {
            return Err(self.integrity("return omitted lexical cleanup"));
        }
        let f = &self.frames[self.frame_index(id)?];
        let parameters = &self
            .executable
            .function(f.function)?
            .cleanup
            .as_ref()
            .unwrap()
            .parameters;
        for p in parameters {
            self.cleanup_owner(Location {
                frame: id,
                place: Place::Parameter(*p),
            })?;
        }
        self.event(Event::Retire(id))?;
        self.frames.pop();
        self.resources.as_mut().unwrap().frames.remove(r);
        self.deliver_protected_value()
    }
    pub(super) fn deliver_protected_return(&mut self, id: u64) -> Result<(), ExecutionError> {
        if self.frames.last().map(|f| f.id) != Some(id) {
            return Err(self.integrity("protected return frame mismatch"));
        }
        self.deliver_protected_value()?;
        self.frames.pop();
        Ok(())
    }
    fn deliver_protected_value(&mut self) -> Result<(), ExecutionError> {
        let grow = self.values.len() == self.values.capacity();
        self.reserve(if grow { size_of::<Value>() } else { 0 })?;
        if grow {
            #[cfg(test)]
            self.temporary_allocation()?;
            self.values
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("protected result allocation")))?;
        }
        self.reserve(0)?;
        self.values
            .push(self.resources.as_mut().unwrap().protected.take().unwrap());
        Ok(())
    }
    fn pending_index(&self, id: u64) -> Result<usize, ExecutionError> {
        self.resources
            .as_ref()
            .unwrap()
            .pending
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| self.integrity("unknown pending construction"))
    }
    pub(super) fn pending_start(
        &mut self,
        frame: u64,
        record: hir::RecordId,
        fields: &'a [hir::RecordFieldInit],
        depth: usize,
    ) -> Result<(), ExecutionError> {
        self.enable_resources()?;
        self.charge()?;
        let grow = self.resources.as_ref().unwrap().pending.len()
            == self.resources.as_ref().unwrap().pending.capacity();
        if grow {
            self.reserve(size_of::<Pending>())?;
            #[cfg(test)]
            self.temporary_allocation()?;
            self.resources
                .as_mut()
                .unwrap()
                .pending
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("pending construction allocation")))?;
        }
        let s = self.resources.as_mut().unwrap();
        let id = s.next_pending;
        s.next_pending = s
            .next_pending
            .checked_add(1)
            .ok_or_else(|| ExecutionError::integrity("pending construction identity exhausted"))?;
        s.pending.push(Pending {
            id,
            frame,
            record,
            initialized: Vec::new(),
            complete: false,
            temporary_site: self.site,
            child_activation: None,
        });
        self.task(
            frame,
            self.origin,
            self.expression,
            Action::PendingNext(id, fields, 0, depth),
        )
    }
    pub(super) fn pending_next(
        &mut self,
        frame: u64,
        id: u64,
        fields: &'a [hir::RecordFieldInit],
        index: usize,
        depth: usize,
    ) -> Result<(), ExecutionError> {
        self.charge()?;
        let p = self.pending_index(id)?;
        if self.resources.as_ref().unwrap().pending[p].frame != frame
            || self.resources.as_ref().unwrap().pending[p].complete
        {
            return Err(self.integrity("pending construction completion state"));
        }
        if let Some(field) = fields.get(index) {
            if self.resources.as_ref().unwrap().fail_field
                == Some((
                    self.resources.as_ref().unwrap().pending[p].record,
                    field.field,
                ))
            {
                if self.resources.as_ref().unwrap().fail_field_skip > 0 {
                    self.resources.as_mut().unwrap().fail_field_skip -= 1;
                } else {
                    self.expression = Some(field.span);
                    self.pending_abort(frame, id, field.field)?;
                    self.resources.as_mut().unwrap().terminal = true;
                    return Err(self.failure(ErrorKind::SyntheticConstructionFailure));
                }
            }
            self.task(
                frame,
                self.origin,
                self.expression,
                Action::PendingNext(id, fields, index + 1, depth),
            )?;
            self.task(
                frame,
                self.origin,
                Some(field.span),
                Action::PendingField(id, field.field),
            )?;
            let parent_site = self.resources.as_ref().unwrap().pending[p].temporary_site;
            let child_site = self.temporary_child(parent_site, index)?;
            let child_site = if let Some((parent, site)) = child_site {
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
                self.resources.as_mut().unwrap().pending[p].child_activation = Some(child);
                let _parent = parent;
                Some((child, site))
            } else {
                None
            };
            self.eval_at(frame, &field.value, depth + 1, child_site)?;
        } else {
            let pending = &self.resources.as_ref().unwrap().pending[p];
            let declaration = self.executable.program.record(pending.record);
            if pending.initialized.len() != declaration.fields.len()
                || declaration
                    .fields
                    .iter()
                    .any(|f| !pending.initialized.iter().any(|(id, _)| id == f))
            {
                return Err(self.integrity("incomplete construction cannot publish"));
            }
            self.resources.as_mut().unwrap().pending[p].complete = true;
            let pending = self.resources.as_mut().unwrap().pending.remove(p);
            self.resources.as_mut().unwrap().detached = Some(Value::Aggregate {
                record: pending.record,
                fields: pending.initialized,
            });
            let grow = self.values.len() == self.values.capacity();
            self.reserve(if grow { size_of::<Value>() } else { 0 })?;
            if grow {
                #[cfg(test)]
                self.temporary_allocation()?;
                self.values
                    .try_reserve_exact(1)
                    .map_err(|_| self.failure(ErrorKind::Host("aggregate result allocation")))?;
            }
            self.reserve(0)?;
            self.values
                .push(self.resources.as_mut().unwrap().detached.take().unwrap());
        }
        Ok(())
    }
    pub(super) fn pending_field(
        &mut self,
        id: u64,
        field: hir::FieldId,
    ) -> Result<(), ExecutionError> {
        self.charge()?;
        let p = self.pending_index(id)?;
        let pending = &self.resources.as_ref().unwrap().pending[p];
        if pending.complete
            || pending.initialized.iter().any(|(id, _)| *id == field)
            || !self
                .executable
                .program
                .record(pending.record)
                .fields
                .contains(&field)
        {
            return Err(self.integrity("pending field membership or duplicate initialization"));
        }
        let record = self.resources.as_ref().unwrap().pending[p].record;
        let value = self.pop_to(Some(crate::mir::temporaries::Destination::Pending(
            record, field,
        )))?;
        if self.resources.as_ref().unwrap().detached.is_some() {
            return Err(self.integrity("detached construction payload already occupied"));
        }
        self.resources.as_mut().unwrap().detached = Some(value);
        let grow = {
            let v = &self.resources.as_ref().unwrap().pending[p].initialized;
            v.len() == v.capacity()
        };
        self.reserve(if grow {
            size_of::<(hir::FieldId, Value)>()
        } else {
            0
        })?;
        if grow {
            #[cfg(test)]
            self.temporary_allocation()?;
            self.resources.as_mut().unwrap().pending[p]
                .initialized
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("pending field allocation")))?;
        }
        let value = self.resources.as_mut().unwrap().detached.take().unwrap();
        self.resources.as_mut().unwrap().pending[p]
            .initialized
            .push((field, value));
        if let Some(child) = self.resources.as_ref().unwrap().pending[p].child_activation {
            self.temporary_retire(child)?;
            self.temporary_end(child)?;
            self.resources.as_mut().unwrap().pending[p].child_activation = None;
        }
        Ok(())
    }
    fn pending_abort(
        &mut self,
        _frame: u64,
        _id: u64,
        _field: hir::FieldId,
    ) -> Result<(), ExecutionError> {
        if !self.executable.domain.permits_abort() {
            return Err(self.integrity("ordinary execution has no synthetic abort authority"));
        }
        #[cfg(test)]
        if let crate::execution::domain::Domain::Abort(templates) = &self.executable.domain {
            self.validate_pending_abort(templates, _frame, _id, _field)?;
        }
        while let Some(pending) = self.resources.as_ref().unwrap().pending.last() {
            if pending.complete {
                return Err(self.integrity("complete aggregate presented as pending"));
            }
            let record = pending.record;
            #[cfg(test)]
            if let crate::execution::domain::Domain::Abort(templates) = &self.executable.domain {
                let template = &templates[self.resources.as_ref().unwrap().pending.len() - 1];
                self.origin = template.origin;
                self.expression = Some(template.failure);
            }
            if let Some(hir::resources::CleanupPlan::Fields(plan)) = self
                .executable
                .cleanup_plans
                .get(record.0)
                .and_then(Option::as_ref)
            {
                for (field, _) in plan {
                    self.charge()?;
                    let position = self
                        .resources
                        .as_ref()
                        .unwrap()
                        .pending
                        .last()
                        .unwrap()
                        .initialized
                        .iter()
                        .position(|(id, _)| id == field);
                    if let Some(position) = position {
                        let value = self
                            .resources
                            .as_mut()
                            .unwrap()
                            .pending
                            .last_mut()
                            .unwrap()
                            .initialized
                            .remove(position)
                            .1;
                        self.release_value(value)?;
                    }
                }
            }
            self.resources.as_mut().unwrap().pending.pop();
        }
        Ok(())
    }
    #[cfg(test)]
    fn validate_pending_abort(
        &self,
        templates: &[crate::execution::domain::Abort],
        frame: u64,
        id: u64,
        field: hir::FieldId,
    ) -> Result<(), ExecutionError> {
        let pending = &self.resources.as_ref().unwrap().pending;
        if templates.len() != pending.len()
            || templates.last().map_or(true, |t| {
                t.request.frame != frame || t.request.pending != id || t.request.before != field
            })
        {
            return Err(self.integrity("private abort complete pending stack or trigger mismatch"));
        }
        // Validate every container before the first attempted release. This loop
        // is bounded by the sealed scenario; no ownership or effect inference.
        for (ordinal, (template, pending)) in templates.iter().zip(pending).enumerate() {
            let request = template.request;
            let actual = self
                .frames
                .get(request.depth)
                .filter(|f| f.id == request.frame && f.function == request.function)
                .ok_or_else(|| self.integrity("private abort exact frame binding"))?;
            if pending.frame != actual.id
                || pending.id != request.pending
                || pending.record != template.record
                || pending.complete
                || pending.initialized.len() != template.initialized.len()
                || !pending
                    .initialized
                    .iter()
                    .map(|(id, _)| id)
                    .eq(template.initialized.iter())
            {
                return Err(
                    self.integrity("private abort construction or initialized membership binding")
                );
            }
            let (activation, site) = pending
                .temporary_site
                .ok_or_else(|| self.integrity("private abort missing construction site"))?;
            if site != request.site
                || self.temporary_site_origin(activation, site, pending.frame)? != template.origin
            {
                return Err(self.integrity("private abort exact origin binding"));
            }
            if ordinal + 1 == templates.len() {
                if pending.child_activation.is_some() {
                    return Err(self.integrity("private abort trigger has an active field"));
                }
            } else {
                let child = pending.child_activation.ok_or_else(|| {
                    self.integrity("private abort ancestor has no in-progress field")
                })?;
                if self.temporary_site_origin(child, template.before_site, pending.frame)?
                    != template.before_origin
                {
                    return Err(self.integrity("private abort in-progress field binding"));
                }
            }
        }
        Ok(())
    }
    pub(super) fn trusted_invoke(
        &mut self,
        function: FunctionId,
        count: usize,
    ) -> Result<bool, ExecutionError> {
        let Some(op) = self
            .executable
            .program
            .resources
            .operations
            .iter()
            .find(|o| o.function == function)
        else {
            return Ok(false);
        };
        self.enable_resources()?;
        self.charge()?;
        if self.frames.len() >= self.limits.call_depth {
            return Err(self.failure(ErrorKind::ConfiguredLimit("call depth")));
        }
        if self.resources.as_ref().map_or(true, |s| s.terminal) {
            return Err(self.integrity("resource machine unavailable or terminal"));
        }
        match op.kind {
            OperationKind::Acquire(resource) => {
                if count != 0 {
                    return Err(self.integrity("acquisition argument count"));
                }
                let value_grow = self.values.len() == self.values.capacity();
                let event_grow = self.resources.as_ref().unwrap().events.len()
                    == self.resources.as_ref().unwrap().events.capacity();
                self.reserve(
                    (if value_grow { size_of::<Value>() } else { 0 })
                        + (if event_grow { size_of::<Event>() } else { 0 }),
                )?;
                if value_grow {
                    #[cfg(test)]
                    self.temporary_allocation()?;
                    self.values.try_reserve_exact(1).map_err(|_| {
                        self.failure(ErrorKind::Host("acquisition value allocation"))
                    })?;
                }
                if event_grow {
                    #[cfg(test)]
                    self.temporary_allocation()?;
                    self.resources
                        .as_mut()
                        .unwrap()
                        .events
                        .try_reserve_exact(1)
                        .map_err(|_| {
                            self.failure(ErrorKind::Host("acquisition event allocation"))
                        })?;
                }
                let s = self.resources.as_mut().unwrap();
                let identity = s.next_identity;
                s.next_identity = s
                    .next_identity
                    .checked_add(1)
                    .ok_or_else(|| ExecutionError::integrity("acquisition identity exhausted"))?;
                let leaf = &self.executable.program.resources.leaves[resource.0];
                self.value(Value::Leaf {
                    resource,
                    record: leaf.record,
                    identity,
                })?;
                self.event(Event::Acquire(identity))?;
            }
            OperationKind::Release(resource) => {
                if count != 1 {
                    return Err(self.integrity("release argument count"));
                }
                let value = self.pop()?;
                if !matches!(value,Value::Leaf {resource:r,..} if r==resource) {
                    return Err(self.integrity("explicit release resource identity"));
                }
                self.release_value(value)?;
                self.value(Value::Completion)?;
            }
        }
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn artifact(extra: &str) -> CheckedExecutable {
        super::super::super::prepare_synthetic(
            hir::resources::fixture_with(extra),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    fn run(e: &CheckedExecutable) -> Machine<'_> {
        let mut m = Machine::new(e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert_eq!(m.resume().unwrap(), ExecutionResult::NoValue);
        m
    }
    #[test]
    fn vertical_leaf_is_released_once_before_frame_retirement() {
        let e = artifact("fn demo() { let owner = acquire(); }");
        let m = run(&e);
        assert_eq!(
            m.resources.unwrap().events,
            vec![
                Event::Acquire(0),
                Event::Attempt(0),
                Event::Complete(0),
                Event::Retire(0)
            ]
        );
        assert!(m.frames.is_empty());
    }
    #[test]
    fn borrowed_handle_never_owns_or_releases_a_resource() {
        let e = artifact("fn demo() { let owner = acquire(); let view = &owner; }");
        let m = run(&e);
        assert_eq!(
            m.resources.unwrap().events,
            vec![
                Event::Acquire(0),
                Event::Attempt(0),
                Event::Complete(0),
                Event::Retire(0)
            ]
        );
    }
    #[test]
    fn implicit_no_value_completion_and_explicit_return_share_cleanup() {
        let e = artifact("fn demo() { let owner = acquire(); return; }");
        let m = run(&e);
        assert_eq!(
            m.resources.unwrap().events,
            vec![
                Event::Acquire(0),
                Event::Attempt(0),
                Event::Complete(0),
                Event::Retire(0)
            ]
        );
    }
    #[test]
    fn name_changes_cannot_change_trusted_dispatch() {
        let mut p = hir::resources::fixture_with("fn demo() { let owner = acquire(); }");
        if let hir::Function::Ordinary(f) = &mut p.functions[0] {
            f.name = "not_acquire".into();
        }
        p.records[0].name = "not_resource".into();
        let e = super::super::super::prepare_synthetic(p, AdmissionLimits::default()).unwrap();
        let m = run(&e);
        assert_eq!(m.resources.unwrap().events[0], Event::Acquire(0));
    }
    #[test]
    fn public_entry_rejects_live_resource_results_before_acquisition() {
        let e = artifact("fn demo() -> Leaf { return acquire(); }");
        assert!(matches!(
            execute(&e, e.entry("demo").unwrap(), ExecutionLimits::default()),
            Err(ExecutionError {
                kind: ErrorKind::Integrity("live resource result cannot cross public boundary"),
                ..
            })
        ));
    }
    #[test]
    fn missing_cleanup_cannot_be_approved() {
        let mut e = artifact("fn demo() { let owner = acquire(); }");
        e.mir.test_functions_mut()[2]
            .cleanup
            .as_mut()
            .unwrap()
            .blocks[0]
            .exit
            .clear();
        assert!(e.validate().is_err());
    }
    #[test]
    fn forged_cleanup_plan_cannot_be_approved() {
        let mut e = artifact("fn demo() { let owner = acquire(); }");
        e.cleanup_plans[0] = None;
        assert!(e.validate().is_err());
    }
}
#[cfg(test)]
mod transfer_and_flow_tests {
    use super::*;
    fn events(extra: &str) -> Vec<Event> {
        let e = super::super::super::prepare_synthetic(
            hir::resources::fixture_with(extra),
            AdmissionLimits::default(),
        )
        .unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert_eq!(m.resume().unwrap(), ExecutionResult::NoValue);
        m.resources.unwrap().events
    }
    fn completes(events: &[Event]) -> Vec<u64> {
        events
            .iter()
            .filter_map(|e| {
                if let Event::Complete(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }
    #[test]
    fn same_scope_transfer_uses_reverse_successful_initialization() {
        let es = events("fn demo() { let a=acquire(); let b=acquire(); let c=a; }");
        assert_eq!(completes(&es), vec![0, 1]);
        assert_eq!(
            es.iter().filter(|e| matches!(e, Event::Attempt(_))).count(),
            2
        );
    }
    #[test]
    fn explicit_consuming_release_prevents_later_cleanup() {
        assert_eq!(
            completes(&events("fn demo() { let a=acquire(); release(a); }")),
            vec![0]
        );
    }
    #[test]
    fn callee_parameters_clean_in_reverse_declaration_order() {
        assert_eq!(
            completes(&events(
                "fn take(a:Leaf,b:Leaf) {} fn demo() {let a=acquire();let b=acquire();take(a,b);}"
            )),
            vec![1, 0]
        );
    }
    #[test]
    fn locals_clean_before_parameters() {
        assert_eq!(
            completes(&events(
                "fn take(a:Leaf) {let b=acquire();} fn demo() {let a=acquire();take(a);}"
            )),
            vec![1, 0]
        );
    }
    #[test]
    fn parameter_transfer_into_local_changes_cleanup_position_without_duplicate() {
        assert_eq!(completes(&events("fn take(a:Leaf,b:Leaf) {let moved=a;} fn demo(){let a=acquire();let b=acquire();take(a,b);}")),vec![0,1]);
    }
    #[test]
    fn protected_return_reaches_caller_after_callee_cleanup() {
        let es=events("fn make()->Leaf {let unused=acquire();let returned=acquire();return returned;} fn demo(){let received=make();}");
        assert_eq!(
            es,
            vec![
                Event::Acquire(0),
                Event::Acquire(1),
                Event::Attempt(0),
                Event::Complete(0),
                Event::Retire(1),
                Event::Attempt(1),
                Event::Complete(1),
                Event::Retire(0)
            ]
        );
    }
    #[test]
    fn returning_owned_parameter_preserves_responsibility() {
        assert_eq!(
            completes(&events(
                "fn pass(x:Leaf)->Leaf {return x;} fn demo(){let a=acquire();let b=pass(a);}"
            )),
            vec![0]
        );
    }
    #[test]
    fn conditional_move_cleans_actual_owner_on_both_runtime_paths() {
        for flag in ["true", "false"] {
            let es = events(&format!(
                "fn demo(){{let a=acquire();if {flag} {{release(a);}}}}"
            ));
            assert_eq!(completes(&es), vec![0]);
        }
    }
    #[test]
    fn skipped_short_circuit_consumption_does_not_suppress_cleanup() {
        for condition in ["false && take(a)", "true || take(a)"] {
            let es=events(&format!("fn take(x:Leaf)->bool{{return true;}} fn demo(){{let a=acquire();let ignored={condition};}}"));
            assert_eq!(completes(&es), vec![0]);
            assert_eq!(
                es.iter().filter(|e| matches!(e, Event::Retire(_))).count(),
                1
            );
        }
    }
    #[test]
    fn executed_short_circuit_consumption_cleans_callee_parameter() {
        assert_eq!(completes(&events("fn take(x:Leaf)->bool{return true;} fn demo(){let a=acquire();let ignored=true && take(a);}")),vec![0]);
    }
    #[test]
    fn nested_fallthrough_exits_inner_before_outer() {
        assert_eq!(
            completes(&events(
                "fn demo(){let a=acquire();if true {let b=acquire();if true {let c=acquire();}}}"
            )),
            vec![2, 1, 0]
        );
    }
    #[test]
    fn loop_body_fallthrough_cleans_before_reentry() {
        assert_eq!(completes(&events("type Count=range 0..3; fn start()->Count{return 3;} fn demo(){let mut n=start();while !(n==0) {let a=acquire();n=n-1;}}")),vec![0,1,2]);
    }
    #[test]
    fn continue_cleans_each_iteration() {
        assert_eq!(completes(&events("type Count=range 0..3; fn start()->Count{return 3;} fn demo(){let mut n=start();while !(n==0){let a=acquire();n=n-1;continue;}}")),vec![0,1,2]);
    }
    #[test]
    fn break_cleans_only_exited_scopes() {
        assert_eq!(completes(&events("fn demo(){let a=acquire();while true {let b=acquire();if true {let c=acquire();break;}}}")),vec![2,1,0]);
    }
    #[test]
    fn nested_loop_break_retains_outer_owner() {
        assert_eq!(completes(&events("fn demo(){while true {let a=acquire();while true {let b=acquire();break;}let c=acquire();break;}}")),vec![1,2,0]);
    }
    #[test]
    fn nested_return_cleans_all_exited_scopes() {
        assert_eq!(completes(&events("fn demo(){let a=acquire();while true {let b=acquire();if true {let c=acquire();return;}}}")),vec![2,1,0]);
    }
    #[test]
    fn conditional_initialization_transfers_only_selected_owner() {
        for flag in ["true", "false"] {
            assert_eq!(completes(&events(&format!("fn demo(){{let a=acquire();let b=acquire();let selected=if {flag}{{a}}else{{b}};}}"))),if flag=="true"{vec![0,1]}else{vec![1,0]});
        }
    }
    #[test]
    fn moved_owner_remains_protected_by_existing_borrow_rules() {
        let p = hir::resources::fixture_with("fn demo(){let a=acquire();let view=&a;let b=a;}");
        assert!(super::super::super::prepare_synthetic(p, AdmissionLimits::default()).is_ok());
        // A live loan still prohibits transfer; the source compiler must reject it.
        let s="type R=range 0..1;record Leaf{data:R}fn acquire()->Leaf{return Leaf{data:0};}fn use_view(v:&Leaf){}fn demo(){let a=acquire();let view=&a;let b=a;use_view(view);}";
        assert!(matches!(
            crate::compile_source(s),
            Err(crate::FrontendError::Ownership(_))
        ));
    }
}
#[cfg(test)]
mod aggregate_tests {
    use super::*;
    fn acquire(span: Span) -> hir::TypedExpr {
        hir::TypedExpr {
            kind: hir::ExprKind::Call {
                function: FunctionId(0),
                arguments: Vec::new(),
            },
            ty: hir::ExprType::Record(hir::RecordId(0)),
            span,
        }
    }
    fn pair(span: Span) -> hir::TypedExpr {
        hir::TypedExpr {
            kind: hir::ExprKind::RecordConstruct {
                record: hir::RecordId(1),
                fields: [2, 0, 1]
                    .into_iter()
                    .map(|i| hir::RecordFieldInit {
                        field: hir::FieldId(i),
                        value: acquire(span),
                        span,
                    })
                    .collect(),
            },
            ty: hir::ExprType::Record(hir::RecordId(1)),
            span,
        }
    }
    pub(super) fn program(extra: &str, bearing: [bool; 3]) -> hir::Program {
        let mut p = hir::resources::fixture_with(extra);
        for (i, resource) in bearing.into_iter().enumerate() {
            p.fields[i].ty = if resource {
                ValueType::Record(hir::RecordId(0))
            } else {
                ValueType::Range(hir::RangeTypeId(0))
            };
        }
        fn expression(e: &mut hir::TypedExpr, bearing: [bool; 3]) {
            match &mut e.kind {
                hir::ExprKind::RecordConstruct { record, fields }
                    if *record == hir::RecordId(1) =>
                {
                    for f in fields {
                        if bearing[f.field.0] {
                            f.value = acquire(f.value.span);
                        }
                    }
                }
                hir::ExprKind::Call { arguments, .. } => {
                    for a in arguments {
                        expression(a, bearing);
                    }
                }
                _ => {}
            }
        }
        for f in &mut p.functions {
            if let hir::Function::Ordinary(f) = f {
                for s in &mut f.body {
                    match s {
                        hir::ValueStatement::Let { initializer, .. } => {
                            expression(initializer, bearing)
                        }
                        hir::ValueStatement::Return { value, .. }
                        | hir::ValueStatement::Assign { value, .. } => expression(value, bearing),
                        _ => {}
                    }
                }
            }
        }
        p
    }
    fn artifact(p: hir::Program) -> CheckedExecutable {
        super::super::super::prepare_synthetic(p, AdmissionLimits::default()).unwrap()
    }
    fn completes(s: &State) -> Vec<u64> {
        s.events
            .iter()
            .filter_map(|e| {
                if let Event::Complete(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }
    fn run(e: &CheckedExecutable) -> Machine<'_> {
        let mut m = Machine::new(e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        assert_eq!(m.resume().unwrap(), ExecutionResult::NoValue);
        m
    }
    #[test]
    fn complete_aggregate_cleanup_is_reverse_declaration_not_evaluation() {
        let e = artifact(program(
            "fn demo(){let owner=Pair{last:0,first:0,inert:0};}",
            [true; 3],
        ));
        let m = run(&e);
        assert_eq!(completes(m.resources.as_ref().unwrap()), vec![0, 2, 1]);
    }
    #[test]
    fn resource_field_not_last_declared_still_releases() {
        let e = artifact(program(
            "fn demo(){let owner=Pair{last:0,first:0,inert:0};}",
            [true, false, false],
        ));
        let m = run(&e);
        assert_eq!(completes(m.resources.as_ref().unwrap()), vec![0]);
    }
    #[test]
    fn pending_third_first_second_fails_uses_filtered_reverse_declaration_order() {
        let e = artifact(program(
            "fn demo(){let owner=Pair{last:0,first:0,inert:0};}",
            [true; 3],
        ));
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
        assert_eq!(
            m.resume().unwrap_err().kind,
            ErrorKind::SyntheticConstructionFailure
        );
        assert_eq!(completes(m.resources.as_ref().unwrap()), vec![0, 1]);
        assert!(m.resources.as_ref().unwrap().pending.is_empty());
        assert!(!m.frames.is_empty());
        assert!(!m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::Retire(_))));
        assert!(matches!(m.frames[0].slots[0].1, Slot::Uninitialized));
    }
    #[test]
    fn whole_aggregate_moves_without_cloning_responsibility() {
        let e = artifact(program(
            "fn demo(){let a=Pair{first:0,inert:0,last:0};let b=a;}",
            [true; 3],
        ));
        let m = run(&e);
        assert_eq!(completes(m.resources.as_ref().unwrap()), vec![2, 1, 0]);
    }
    #[test]
    fn aggregate_return_is_protected_until_caller_cleanup() {
        let e = artifact(program(
            "fn make()->Pair{return Pair{last:0,first:0,inert:0};}fn demo(){let owner=make();}",
            [true; 3],
        ));
        let m = run(&e);
        assert_eq!(completes(m.resources.as_ref().unwrap()), vec![0, 2, 1]);
        let events = &m.resources.as_ref().unwrap().events;
        assert!(
            events.iter().position(|e| *e == Event::Retire(1)).unwrap()
                < events
                    .iter()
                    .position(|e| matches!(e, Event::Complete(_)))
                    .unwrap()
        );
    }
    pub(super) fn nested(extra: &str) -> hir::Program {
        let mut p = program(extra, [true; 3]);
        p.fields[3].ty = ValueType::Record(hir::RecordId(1));
        p.fields[4].ty = ValueType::Record(hir::RecordId(0));
        for f in &mut p.functions {
            if let hir::Function::Ordinary(f) = f {
                for s in &mut f.body {
                    if let hir::ValueStatement::Let {
                        initializer:
                            hir::TypedExpr {
                                kind: hir::ExprKind::RecordConstruct { record, fields },
                                ..
                            },
                        ..
                    } = s
                    {
                        if *record == hir::RecordId(2) {
                            for field in fields {
                                field.value = if field.field == hir::FieldId(3) {
                                    pair(field.value.span)
                                } else {
                                    acquire(field.value.span)
                                };
                            }
                        }
                    }
                }
            }
        }
        p
    }
    #[test]
    fn nested_resource_records_clean_shared_plans_recursively() {
        let e = artifact(nested(
            "record Wrap{x:R,last:R}fn demo(){let owner=Wrap{last:0,x:0};}",
        ));
        let m = run(&e);
        assert_eq!(completes(m.resources.as_ref().unwrap()), vec![0, 1, 3, 2]);
    }
    #[test]
    fn nested_pending_states_abort_inner_before_outer() {
        let e = artifact(nested(
            "record Wrap{x:R,last:R}fn demo(){let owner=Wrap{last:0,x:0};}",
        ));
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
        assert_eq!(
            m.resume().unwrap_err().kind,
            ErrorKind::SyntheticConstructionFailure
        );
        assert_eq!(completes(m.resources.as_ref().unwrap()), vec![1, 2, 0]);
    }
    #[test]
    fn pending_move_does_not_restore_source_on_later_failure() {
        let mut p = program(
            "fn demo(){let source=acquire();let owner=Pair{first:0,inert:0,last:0};}",
            [true; 3],
        );
        let hir::Function::Ordinary(f) = &mut p.functions[2] else {
            panic!()
        };
        let hir::ValueStatement::Let {
            initializer:
                hir::TypedExpr {
                    kind: hir::ExprKind::RecordConstruct { fields, .. },
                    ..
                },
            ..
        } = &mut f.body[1]
        else {
            panic!()
        };
        fields[0].value = hir::TypedExpr {
            kind: hir::ExprKind::Local(hir::LocalId(0)),
            ty: hir::ExprType::Record(hir::RecordId(0)),
            span: fields[0].span,
        };
        let e = artifact(p);
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resources.as_mut().unwrap().fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
        assert_eq!(
            m.resume().unwrap_err().kind,
            ErrorKind::SyntheticConstructionFailure
        );
        assert_eq!(completes(m.resources.as_ref().unwrap()), vec![0]);
        assert!(matches!(m.frames[0].slots[0].1, Slot::Transferred));
    }
    #[test]
    fn resource_field_projection_is_not_admitted() {
        let p = program(
            "fn demo(){let owner=Pair{first:0,inert:0,last:0};let forbidden=owner.first;}",
            [true; 3],
        );
        assert!(super::super::super::prepare_synthetic(p, AdmissionLimits::default()).is_err());
    }
    #[test]
    fn incomplete_aggregate_mir_is_rejected_by_correspondence() {
        let mut e = artifact(program(
            "fn demo(){let owner=Pair{first:0,inert:0,last:0};}",
            [true; 3],
        ));
        let initializer = match &mut e.mir.test_functions_mut()[2].test_blocks_mut()[0]
            .test_statements_mut()[0]
        {
            Statement::Let { initializer, .. } => initializer,
            _ => panic!(),
        };
        let hir::ExprKind::RecordConstruct { fields, .. } = &mut initializer.kind else {
            panic!()
        };
        fields.pop();
        assert!(e.validate().is_err());
    }
}
#[cfg(test)]
mod bounds_and_artifact_tests {
    use super::*;
    fn artifact(s: &str) -> CheckedExecutable {
        super::super::super::prepare_synthetic(
            hir::resources::fixture_with(s),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    fn start(e: &CheckedExecutable, limits: ExecutionLimits) -> Machine<'_> {
        let mut m = Machine::new(e, limits);
        m.start(e.entry("demo").unwrap()).unwrap();
        m
    }
    fn completes(m: &Machine<'_>) -> Vec<u64> {
        m.resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Complete(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }
    #[test]
    fn exact_sufficient_and_insufficient_cleanup_fuel() {
        let e = artifact("fn demo(){let owner=acquire();}");
        let mut m = start(&e, ExecutionLimits::default());
        m.resume().unwrap();
        let steps = m.steps;
        assert_eq!(steps, 18);
        assert_eq!(
            execute(
                &e,
                e.entry("demo").unwrap(),
                ExecutionLimits {
                    steps,
                    ..ExecutionLimits::default()
                }
            )
            .unwrap(),
            ExecutionResult::NoValue
        );
        let mut m = start(
            &e,
            ExecutionLimits {
                steps: steps - 1,
                ..ExecutionLimits::default()
            },
        );
        assert!(matches!(
            m.resume().unwrap_err().kind,
            ErrorKind::ConfiguredLimit("steps")
        ));
        assert!(completes(&m).is_empty());
        assert!(m.resources.as_ref().unwrap().current.is_some());
    }
    #[test]
    fn exact_sufficient_and_insufficient_resource_storage() {
        let e = artifact("fn demo(){let owner=acquire();}");
        let id = e.entry("demo").unwrap();
        let succeeds = |bytes| {
            execute(
                &e,
                id,
                ExecutionLimits {
                    working_storage: bytes,
                    ..ExecutionLimits::default()
                },
            )
            .is_ok()
        };
        let mut low = 1;
        let mut high = 16_384;
        assert!(succeeds(high));
        while low < high {
            let mid = (low + high) / 2;
            if succeeds(mid) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert!(succeeds(low));
        assert!(!succeeds(low - 1));
        assert!(matches!(
            execute(
                &e,
                id,
                ExecutionLimits {
                    working_storage: low - 1,
                    ..ExecutionLimits::default()
                }
            )
            .unwrap_err()
            .kind,
            ErrorKind::ConfiguredLimit("working storage")
        ));
    }
    #[test]
    fn controlled_release_failure_preserves_prefix_and_never_retries() {
        let e = artifact("fn demo(){let a=acquire();let b=acquire();}");
        let mut m = start(&e, ExecutionLimits::default());
        m.resources.as_mut().unwrap().failure_at = Some(0);
        assert_eq!(
            m.resume().unwrap_err().kind,
            ErrorKind::Host("synthetic trusted release")
        );
        assert_eq!(completes(&m), vec![1]);
        let before = m.resources.as_ref().unwrap().events.clone_for_test();
        assert!(matches!(
            m.resume().unwrap_err().kind,
            ErrorKind::Integrity("resource execution is terminal")
        ));
        assert_eq!(
            m.resources.as_ref().unwrap().events.clone_for_test(),
            before
        );
        assert!(matches!(
            m.resources.as_ref().unwrap().current,
            Some(Value::Leaf { identity: 0, .. })
        ));
    }
    #[test]
    fn failure_while_cleaning_callee_keeps_protected_result_outside_cleanup() {
        let e=artifact("fn make()->Leaf{let failed=acquire();let returned=acquire();return returned;}fn demo(){let owner=make();}");
        let mut m = start(&e, ExecutionLimits::default());
        m.resources.as_mut().unwrap().failure_at = Some(0);
        assert_eq!(
            m.resume().unwrap_err().kind,
            ErrorKind::Host("synthetic trusted release")
        );
        assert!(matches!(
            m.resources.as_ref().unwrap().protected,
            Some(Value::Leaf { identity: 1, .. })
        ));
        assert!(!m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::Retire(_))));
        assert!(!m.frames.is_empty());
    }
    #[test]
    fn undefined_resource_argument_temporary_is_rejected() {
        let p = hir::resources::fixture_with("fn demo(){release(acquire());}");
        let e = super::super::super::prepare_synthetic(p, AdmissionLimits::default()).unwrap();
        let mut m = start(&e, ExecutionLimits::default());
        m.resume().unwrap();
        assert_eq!(completes(&m), vec![0]);
        super::conservation_tests::conserved(&m);
    }
    #[test]
    fn unused_resource_catalog_preserves_resource_free_fuel_and_storage() {
        let e = artifact("fn demo(){}");
        let mut m = start(&e, ExecutionLimits::default());
        m.resume().unwrap();
        assert_eq!(m.steps, 2);
        assert!(m.resources.is_none());
        let plain = prepare_execution("fn demo(){}", AdmissionLimits::default()).unwrap();
        let mut other = Machine::new(&plain, ExecutionLimits::default());
        other.start(plain.entry("demo").unwrap()).unwrap();
        other.resume().unwrap();
        assert_eq!(m.storage(), other.storage());
    }
    #[test]
    fn resource_free_caller_can_invoke_resource_owning_helper() {
        let e = artifact("fn helper(){let owner=acquire();}fn demo(){helper();}");
        let mut m = start(&e, ExecutionLimits::default());
        assert!(m.resources.is_none());
        m.resume().unwrap();
        assert_eq!(completes(&m), vec![0]);
    }
    #[test]
    fn reference_only_function_does_not_acquire_cleanup_responsibility() {
        let e = artifact("fn inspect(view:&Leaf){}fn demo(){let owner=acquire();inspect(&owner);}");
        let mut m = start(&e, ExecutionLimits::default());
        m.resume().unwrap();
        assert_eq!(completes(&m), vec![0]);
    }
    #[test]
    fn configured_type_graph_depth_is_enforced() {
        let p = hir::resources::fixture_with("fn demo(){let owner=acquire();}");
        assert!(matches!(
            super::super::super::prepare_synthetic(
                p,
                AdmissionLimits {
                    traversal_depth: 1,
                    ..AdmissionLimits::default()
                }
            )
            .err()
            .unwrap()
            .kind,
            ErrorKind::ConfiguredLimit("cleanup type graph depth")
        ));
    }
    #[test]
    fn oversized_resource_metadata_is_rejected_before_approval() {
        let mut p = hir::resources::fixture_with("fn demo(){let owner=acquire();}");
        p.records[0].name = "x".repeat(257);
        assert!(matches!(
            super::super::super::prepare_synthetic(p, AdmissionLimits::default())
                .err()
                .unwrap()
                .kind,
            ErrorKind::ConfiguredLimit("resource identifier bytes")
        ));
    }
    #[test]
    fn malformed_cleanup_owner_scope_exit_and_origin_controls() {
        for fault in 0..9 {
            let mut e = artifact("fn demo(){let owner=acquire();if true {let inner=acquire();}}");
            let flow = e.mir.test_functions_mut()[2].cleanup.as_mut().unwrap();
            match fault {
                0 => flow.scopes[0].owners.push(hir::LocalId(999)),
                1 => flow.blocks[0].initialize[0].local = hir::LocalId(999),
                2 => flow.blocks[0].initialize[0].scope = crate::mir::cleanup::ScopeId(1),
                3 => flow.blocks[0].initialize[0].origin.kind = OriginKind::Statement,
                4 => flow
                    .blocks
                    .iter_mut()
                    .find(|b| !b.exit.is_empty())
                    .unwrap()
                    .exit
                    .clear(),
                5 => {
                    let b = flow.blocks.iter_mut().find(|b| !b.exit.is_empty()).unwrap();
                    b.exit.push(b.exit[0]);
                }
                6 => flow.scopes[1].parent = None,
                7 => flow.protected_return = true,
                8 => flow.parameters.push(hir::ParameterId(999)),
                _ => unreachable!(),
            }
            assert!(e.validate().is_err(), "fault {fault}");
        }
    }
    #[test]
    fn cleanup_targeting_inert_value_or_reference_rejects() {
        for body in ["let inert=true;", "let view=&owner;"] {
            let mut e = artifact(&format!("fn demo(){{let owner=acquire();{body}}}"));
            let flow = e.mir.test_functions_mut()[2].cleanup.as_mut().unwrap();
            flow.scopes[0].owners.push(hir::LocalId(1));
            let mut forged = flow.blocks[0].initialize[0].clone();
            forged.local = hir::LocalId(1);
            forged.statement = 1;
            flow.blocks[0].initialize.push(forged);
            assert!(e.validate().is_err());
        }
    }
    #[test]
    fn loop_cleanup_omission_or_reordering_cannot_gain_authority() {
        for fault in 0..2 {
            let mut e =
                artifact("fn demo(){let outer=acquire();while true{let inner=acquire();break;}}");
            let flow = e.mir.test_functions_mut()[2].cleanup.as_mut().unwrap();
            let b = flow
                .blocks
                .iter_mut()
                .find(|b| b.exit == vec![crate::mir::cleanup::ScopeId(1)])
                .unwrap();
            if fault == 0 {
                b.exit.clear();
            } else {
                b.exit = vec![crate::mir::cleanup::ScopeId(0)];
            }
            assert!(e.validate().is_err());
        }
    }
    #[test]
    fn reordered_or_foreign_cleanup_plan_is_rejected() {
        for fault in 0..3 {
            let mut e = artifact("fn demo(){let owner=acquire();}");
            let Some(hir::resources::CleanupPlan::Fields(fields)) = &mut e.cleanup_plans[1] else {
                panic!()
            };
            match fault {
                0 => fields.reverse(),
                1 => fields[0].0 = hir::FieldId(999),
                2 => fields[0].1 = hir::RecordId(999),
                _ => unreachable!(),
            }
            assert!(e.validate().is_err());
        }
    }
    #[test]
    fn foreign_trusted_function_and_resource_pairing_rejects() {
        for fault in 0..2 {
            let mut p = hir::resources::fixture_with("fn demo(){let owner=acquire();}");
            if fault == 0 {
                p.resources.operations[0].function = FunctionId(2);
            } else {
                p.resources.leaves[0].release = hir::resources::OperationId(0);
            }
            assert!(super::super::super::prepare_synthetic(p, AdmissionLimits::default()).is_err());
        }
    }
    #[test]
    fn ordinary_inert_replacement_is_accepted() {
        let source="type R=range 0..1;record Leaf{data:R}fn acquire()->Leaf{return Leaf{data:0};}fn demo(){let mut a=acquire();let b=acquire();a=b;}";
        crate::compile_source(source).unwrap();
    }
}
#[cfg(test)]
trait TestEvents {
    fn clone_for_test(&self) -> Vec<(u8, u64)>;
}
#[cfg(test)]
impl TestEvents for Vec<Event> {
    fn clone_for_test(&self) -> Vec<(u8, u64)> {
        self.iter()
            .map(|e| match e {
                Event::Acquire(id) => (0, *id),
                Event::Attempt(id) => (1, *id),
                Event::Complete(id) => (2, *id),
                Event::Failure(id) => (3, *id),
                Event::Retire(id) => (4, *id),
            })
            .collect()
    }
}
#[cfg(test)]
mod copy_and_reporting_tests {
    use super::*;
    fn artifact() -> CheckedExecutable {
        super::super::super::prepare_synthetic(
            hir::resources::fixture_with("fn demo(){let owner=acquire();}"),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    #[test]
    fn forged_copy_local_cannot_seal_a_resource_payload() {
        let mut p = hir::resources::fixture_with("fn demo(){let owner=acquire();}");
        p.locals[0].ty = ValueType::Bool;
        assert!(super::super::super::prepare_synthetic(p, AdmissionLimits::default()).is_err());
    }
    #[test]
    fn mismatched_statement_call_cannot_hide_resource_in_copy_parameter() {
        let mut p = hir::resources::fixture_with(
            "fn take(x:Leaf){}fn demo(){let owner=acquire();take(owner);}",
        );
        p.parameters[1].ty = hir::ParameterType::Value(ValueType::Bool);
        assert!(super::super::super::prepare_synthetic(p, AdmissionLimits::default()).is_err());
    }
    #[test]
    fn trusted_acquisition_cannot_be_reclassified_as_copy() {
        let mut p = hir::resources::fixture_with("fn demo(){let owner=acquire();}");
        let hir::Function::Ordinary(f) = &mut p.functions[0] else {
            panic!()
        };
        f.return_type = Some(ValueType::Bool);
        assert!(super::super::super::prepare_synthetic(p, AdmissionLimits::default()).is_err());
    }
    #[test]
    fn resource_payloads_and_aggregates_have_no_copy_operation() {
        let leaf = Value::Leaf {
            resource: hir::resources::ResourceId(0),
            record: hir::RecordId(0),
            identity: 0,
        };
        assert!(leaf.copy().is_none());
        let aggregate = Value::Aggregate {
            record: hir::RecordId(1),
            fields: vec![(hir::FieldId(0), leaf)],
        };
        assert!(aggregate.copy().is_none());
    }
    #[test]
    fn host_failure_reporting_stays_bounded_and_writer_failure_is_host() {
        let e = artifact();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(0);
        let error = m.resume().unwrap_err();
        let text = e.render_error(&error, 4096).unwrap();
        assert!(text.contains("detected host/tool failure"));
        assert!(matches!(
            e.render_error(&error, 1).unwrap_err().kind,
            ErrorKind::ConfiguredLimit("rendered output")
        ));
        struct Fails;
        impl std::io::Write for Fails {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::Other.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            write_output(Fails, &text).unwrap_err().kind,
            ErrorKind::Host("output write")
        );
    }
    #[test]
    fn diagnostic_allocation_failure_preserves_resource_host_classification() {
        let e = artifact();
        let error = ExecutionError::new(ErrorKind::Host("synthetic trusted release"), START);
        super::super::super::render::FAIL_ALLOCATION.with(|v| v.set(true));
        let result = e.render_error(&error, 4096);
        super::super::super::render::FAIL_ALLOCATION.with(|v| v.set(false));
        assert!(matches!(
            result.unwrap_err().kind,
            ErrorKind::Host("render allocation")
        ));
    }
}
#[cfg(test)]
pub(super) mod conservation_tests {
    use super::*;
    pub(in crate::execution::machine) fn holders(m: &Machine<'_>) -> Vec<(u64, String)> {
        fn append(v: &Value, label: String, out: &mut Vec<(u64, String)>) {
            match v {
                Value::Leaf { identity, .. } => out.push((*identity, label)),
                Value::Aggregate { fields, .. } => {
                    for (field, value) in fields {
                        append(value, format!("{label}.field:{}", field.0), out);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for frame in &m.frames {
            for (place, slot) in &frame.slots {
                if let Slot::Available(v) = slot {
                    append(v, format!("frame:{}:{place:?}", frame.id), &mut out);
                }
            }
        }
        for (index, v) in m.values.iter().enumerate() {
            append(v, format!("operand:{index}"), &mut out);
        }
        if let Some(s) = &m.resources {
            for (label, value) in [
                ("cleanup-current", &s.current),
                ("protected-return", &s.protected),
                ("detached", &s.detached),
            ] {
                if let Some(v) = value {
                    append(v, label.into(), &mut out);
                }
            }
            for (index, v) in s.work.iter().enumerate() {
                append(v, format!("cleanup-work:{index}"), &mut out);
            }
            for p in &s.pending {
                for (field, v) in &p.initialized {
                    append(v, format!("pending:{}:field:{}", p.id, field.0), &mut out);
                }
            }
            for (index, r) in s.replacements.iter().enumerate() {
                if let Some(v) = &r.protected {
                    append(v, format!("protected-replacement:{index}"), &mut out);
                }
                if let Some(v) = &r.displaced {
                    append(v, format!("displaced-replacement:{index}"), &mut out);
                }
            }
        }
        if let Some(s) = &m.temporaries {
            s.visit_holders(|v, label| append(v, label, &mut out));
        }
        if let Some(s) = &m.results {
            for (index, cell) in s.cells.iter().enumerate() {
                if let Some(v) = &cell.payload {
                    append(v, format!("Result-cell:{index}"), &mut out);
                }
            }
        }
        out
    }
    fn leaves(value: &Value, ids: &mut Vec<u64>) {
        match value {
            Value::Leaf { identity, .. } => ids.push(*identity),
            Value::Aggregate { fields, .. } => {
                for (_, v) in fields {
                    leaves(v, ids);
                }
            }
            _ => {}
        }
    }
    pub(in crate::execution::machine) fn conserved(m: &Machine<'_>) {
        let s = m.resources.as_ref().unwrap();
        let mut acquired = Vec::new();
        let mut completed = Vec::new();
        for e in &s.events {
            match e {
                Event::Acquire(id) => acquired.push(*id),
                Event::Complete(id) => completed.push(*id),
                _ => {}
            }
        }
        let mut owned = Vec::new();
        for f in &m.frames {
            for (_, slot) in &f.slots {
                if let Slot::Available(v) = slot {
                    leaves(v, &mut owned);
                }
            }
        }
        for v in &m.values {
            leaves(v, &mut owned);
        }
        for v in [&s.current, &s.protected, &s.detached]
            .into_iter()
            .flatten()
        {
            leaves(v, &mut owned);
        }
        for v in &s.work {
            leaves(v, &mut owned);
        }
        for p in &s.pending {
            for (_, v) in &p.initialized {
                leaves(v, &mut owned);
            }
        }
        for r in &s.replacements {
            for v in [&r.protected, &r.displaced].into_iter().flatten() {
                leaves(v, &mut owned);
            }
        }
        if let Some(temporaries) = &m.temporaries {
            temporaries.visit_values(|v| leaves(v, &mut owned));
        }
        if let Some(results) = &m.results {
            for cell in &results.cells {
                if let Some(v) = &cell.payload {
                    leaves(v, &mut owned);
                }
            }
        }
        let mut labelled: Vec<_> = holders(m).iter().map(|(id, _)| *id).collect();
        let mut physical = owned.clone();
        labelled.sort_unstable();
        physical.sort_unstable();
        assert_eq!(
            labelled, physical,
            "holder census must cover all physical payloads"
        );
        owned.extend(completed);
        owned.sort_unstable();
        acquired.sort_unstable();
        assert_eq!(owned, acquired);
        assert!(owned.windows(2).all(|ids| ids[0] != ids[1]));
    }
    #[test]
    fn later_argument_publication_stop_retains_detached_leaf_exactly_once() {
        let e = super::super::super::prepare_synthetic(
            hir::resources::fixture_with(
                "fn take(flag:bool,item:Leaf){}fn demo(){let item=acquire();take(true,item);}",
            ),
            AdmissionLimits::default(),
        )
        .unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        loop {
            let task = m.tasks.pop().unwrap();
            if matches!(
                &task.action,
                Action::Eval(
                    hir::TypedExpr {
                        kind: ExprKind::Local(_),
                        ..
                    },
                    _
                ) | Action::SiteEval(
                    hir::TypedExpr {
                        kind: ExprKind::Local(_),
                        ..
                    },
                    _,
                    _,
                    _
                )
            ) {
                assert_eq!(m.values.len(), 1);
                assert_eq!(m.values.capacity(), 1);
                assert_eq!(m.values[0], Value::Public(RuntimeValue::Bool(true)));
                let storage = m.storage().unwrap();
                m.limits.working_storage = storage;
                m.tasks.push(task);
                assert_eq!(
                    m.resume().unwrap_err().kind,
                    ErrorKind::ConfiguredLimit("working storage")
                );
                assert!(matches!(m.frames[0].slots[0].1, Slot::Transferred));
                assert!(matches!(
                    m.resources.as_ref().unwrap().detached,
                    Some(Value::Leaf { identity: 0, .. })
                ));
                assert!(!m
                    .resources
                    .as_ref()
                    .unwrap()
                    .events
                    .contains(&Event::Complete(0)));
                conserved(&m);
                assert!(m.storage().unwrap() <= storage);
                let events = format!("{:?}", m.resources.as_ref().unwrap().events);
                assert!(m.resume().is_err());
                assert_eq!(
                    format!("{:?}", m.resources.as_ref().unwrap().events),
                    events
                );
                conserved(&m);
                break;
            }
            m.step(task).unwrap();
        }
    }
    #[test]
    fn destination_slot_growth_failure_retains_transferred_owner() {
        let e = super::super::super::prepare_synthetic(
            hir::resources::fixture_with("fn demo(){let item=acquire();let receiver=item;}"),
            AdmissionLimits::default(),
        )
        .unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        loop {
            let task = m.tasks.pop().unwrap();
            if matches!(task.action, Action::Commit(_)) {
                m.step(task).unwrap();
                break;
            }
            m.step(task).unwrap();
        }
        let source = Location {
            frame: 0,
            place: Place::Local(hir::LocalId(0)),
        };
        let destination = Location {
            frame: 0,
            place: Place::Local(hir::LocalId(1)),
        };
        assert_eq!(m.frames[0].slots.len(), m.frames[0].slots.capacity());
        let storage = m.storage().unwrap();
        m.limits.working_storage = storage;
        let value = m.rvalue(source).unwrap();
        assert_eq!(
            m.publish(Destination::Slot(destination), value)
                .unwrap_err()
                .kind,
            ErrorKind::ConfiguredLimit("working storage")
        );
        assert!(matches!(m.frames[0].slots[0].1, Slot::Transferred));
        assert!(matches!(
            m.resources.as_ref().unwrap().detached,
            Some(Value::Leaf { identity: 0, .. })
        ));
        conserved(&m);
    }
    #[test]
    fn every_cleanup_fuel_stop_conserves_acquired_leaf_responsibilities() {
        let e = super::super::super::prepare_synthetic(
            hir::resources::fixture_with("fn demo(){let a=acquire();let b=acquire();let c=a;}"),
            AdmissionLimits::default(),
        )
        .unwrap();
        let mut full = Machine::new(&e, ExecutionLimits::default());
        full.start(e.entry("demo").unwrap()).unwrap();
        full.resume().unwrap();
        let complete = full.steps;
        conserved(&full);
        for steps in 1..complete {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    steps,
                    ..ExecutionLimits::default()
                },
            );
            if m.start(e.entry("demo").unwrap()).is_ok() {
                assert!(m.resume().is_err());
                conserved(&m);
            }
        }
    }
    #[test]
    fn controlled_host_failure_retains_unique_uncompleted_payload() {
        let e = super::super::super::prepare_synthetic(
            hir::resources::fixture_with("fn demo(){let a=acquire();let b=acquire();}"),
            AdmissionLimits::default(),
        )
        .unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resources.as_mut().unwrap().failure_at = Some(0);
        assert!(m.resume().is_err());
        conserved(&m);
    }

    fn aggregate() -> CheckedExecutable {
        super::super::super::prepare_synthetic(
            super::aggregate_tests::program(
                "fn make()->Pair{return Pair{last:0,first:0,inert:0};}fn demo(){let a=make();let b=a;}",
                [true; 3],
            ),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    #[test]
    fn aggregate_pending_return_and_cleanup_conserve_at_every_fuel_stop() {
        let e = aggregate();
        let mut full = Machine::new(&e, ExecutionLimits::default());
        full.start(e.entry("demo").unwrap()).unwrap();
        full.resume().unwrap();
        conserved(&full);
        for steps in 1..full.steps {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    steps,
                    ..ExecutionLimits::default()
                },
            );
            if m.start(e.entry("demo").unwrap()).is_ok() {
                assert!(m.resume().is_err());
                if m.resources.is_some() {
                    conserved(&m);
                }
            }
        }
    }
    #[test]
    fn aggregate_storage_boundary_and_detachment_stops_conserve() {
        let e = aggregate();
        let run = |bytes| {
            let mut m = Machine::new(
                &e,
                ExecutionLimits {
                    working_storage: bytes,
                    ..ExecutionLimits::default()
                },
            );
            let result = m.start(e.entry("demo").unwrap()).and_then(|()| m.resume());
            assert!(
                m.storage().unwrap() <= bytes,
                "storage {:?} exceeds {bytes}",
                m.storage()
            );
            if m.resources.is_some() {
                conserved(&m);
            }
            result
        };
        let mut low = 1;
        let mut high = 16_384;
        assert!(run(high).is_ok());
        while low < high {
            let mid = (low + high) / 2;
            if run(mid).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert!(run(low).is_ok());
        assert_eq!(
            run(low - 1).unwrap_err().kind,
            ErrorKind::ConfiguredLimit("working storage")
        );
        for bytes in (1..low).step_by(16) {
            assert!(run(bytes).is_err());
        }
    }
    #[test]
    fn partial_abort_release_failure_retains_remaining_unique_leaves() {
        let e = super::super::super::prepare_synthetic(
            super::aggregate_tests::program(
                "fn demo(){let owner=Pair{last:0,first:0,inert:0};}",
                [true; 3],
            ),
            AdmissionLimits::default(),
        )
        .unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let s = m.resources.as_mut().unwrap();
        s.fail_field = Some((hir::RecordId(1), hir::FieldId(1)));
        s.failure_at = Some(1);
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        conserved(&m);
        assert_eq!(
            m.resources
                .as_ref()
                .unwrap()
                .events
                .iter()
                .filter(|e| matches!(e, Event::Complete(_)))
                .count(),
            1
        );
        let events = format!("{:?}", m.resources.as_ref().unwrap().events);
        assert!(m.resume().is_err());
        assert_eq!(
            format!("{:?}", m.resources.as_ref().unwrap().events),
            events
        );
    }
}

#[cfg(test)]
mod repair_classification_tests {
    use super::*;
    fn inert_nested(guard: bool) -> hir::Program {
        let extra = format!("record Scalar{{item:R}} record Nest{{inner:R}} fn demo(){{{}let mut once=true;let mut again=true;while again{{let value=Nest{{inner:0}};if once{{once=false;}}else{{again=false;}}}}}}", if guard { "let guard=acquire();" } else { "" });
        let mut p = super::aggregate_tests::program(&extra, [false; 3]);
        let scalar = p.records.iter().find(|r| r.name == "Scalar").unwrap().id;
        let nest = p.records.iter().find(|r| r.name == "Nest").unwrap().id;
        let inner = p.record(nest).fields[0];
        let item = p.record(scalar).fields[0];
        p.fields[inner.0].ty = ValueType::Record(scalar);
        for f in &mut p.functions {
            let hir::Function::Ordinary(f) = f else {
                continue;
            };
            for statement in &mut f.body {
                let hir::ValueStatement::While { body, .. } = statement else {
                    continue;
                };
                let hir::ValueStatement::Let { initializer, .. } = &mut body[0] else {
                    panic!()
                };
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut initializer.kind else {
                    panic!()
                };
                let value = &mut fields[0].value;
                let leaf = value.clone();
                value.kind = hir::ExprKind::RecordConstruct {
                    record: scalar,
                    fields: vec![hir::RecordFieldInit {
                        field: item,
                        value: leaf,
                        span: value.span,
                    }],
                };
                value.ty = ExprType::Record(scalar);
            }
        }
        p
    }
    fn repeated_inert(guard: bool) {
        let e =
            super::super::super::prepare_synthetic(inert_nested(guard), AdmissionLimits::default())
                .unwrap();
        assert!(e.cleanup_plans[2].is_none() && e.cleanup_plans[3].is_none());
        let local = e
            .program
            .locals
            .iter()
            .find(|l| l.name == "value")
            .unwrap()
            .id;
        for f in e.mir.functions() {
            if let Some(flow) = &f.cleanup {
                assert!(flow
                    .blocks
                    .iter()
                    .flat_map(|b| &b.initialize)
                    .all(|i| i.local != local));
            }
        }
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let mut initializations = 0;
        while let Some(task) = m.tasks.pop() {
            if let Action::Commit(Destination::Slot(at)) = &task.action {
                if at.place == Place::Local(local) {
                    initializations += 1;
                    if guard {
                        assert!(m
                            .frames
                            .iter()
                            .flat_map(|f| &f.slots)
                            .any(|(_, s)| matches!(
                                s,
                                Slot::Available(Value::Leaf { identity: 0, .. })
                            )));
                        assert!(!m
                            .resources
                            .as_ref()
                            .unwrap()
                            .events
                            .contains(&Event::Complete(0)));
                    }
                }
            }
            m.step(task).unwrap();
        }
        assert_eq!(initializations, 2);
        assert_eq!(m.resume().unwrap(), ExecutionResult::NoValue);
        let completes = m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter(|e| matches!(e, Event::Complete(_)))
            .count();
        assert_eq!(completes, usize::from(guard));
    }
    #[test]
    fn inert_nested_record_reinitializes_without_unrelated_resource() {
        repeated_inert(false);
    }
    #[test]
    fn inert_nested_record_reinitializes_with_live_outer_resource() {
        repeated_inert(true);
    }
    #[test]
    fn live_canonical_resource_still_rejects_reinitialization() {
        let e = super::super::super::prepare_synthetic(
            hir::resources::fixture_with("fn demo(){let owner=acquire();}"),
            AdmissionLimits::default(),
        )
        .unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        loop {
            let task = m.tasks.pop().unwrap();
            if matches!(task.action, Action::Commit(_)) {
                m.step(task).unwrap();
                break;
            }
            m.step(task).unwrap();
        }
        let at = Location {
            frame: 0,
            place: Place::Local(hir::LocalId(0)),
        };
        assert!(m.cleanup_reinitialization(at).is_err());
        // Independently exercise the live-payload check without initialization history.
        m.resources.as_mut().unwrap().frames[0].scopes[0]
            .initialized
            .clear();
        assert_eq!(
            m.cleanup_reinitialization(at).unwrap_err().kind,
            ErrorKind::Integrity("live resource overwritten at initialization")
        );
        assert!(matches!(
            m.slot(at).unwrap(),
            Value::Leaf { identity: 0, .. }
        ));
    }
}
#[cfg(test)]
mod repair_plan_failure_tests {
    use super::*;
    use hir::resources::{PlanError, FAIL_PLAN_ALLOCATION};
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAIL_PLAN_ALLOCATION.with(|n| n.set(None));
        }
    }
    fn failing<T>(after: usize, operation: impl FnOnce() -> T) -> T {
        FAIL_PLAN_ALLOCATION.with(|n| n.set(Some(after)));
        let _reset = Reset;
        operation()
    }
    fn program() -> hir::Program {
        hir::resources::fixture_with("fn demo(){let owner=acquire();}")
    }
    #[test]
    fn initial_plan_reservation_failures_are_host_without_artifact() {
        for after in 0..4 {
            let p = program();
            let result = failing(after, || {
                super::super::super::prepare_synthetic(p, AdmissionLimits::default())
            });
            assert!(matches!(result.err().unwrap().kind, ErrorKind::Host(_)));
        }
    }
    #[test]
    fn later_lowering_plan_failure_after_valid_schema_is_host() {
        let p = program();
        p.resources.validate(&p).unwrap();
        let result = failing(0, || mir::lower_with_origins(&p));
        let error = ExecutionError::from(result.unwrap_err());
        assert_eq!(
            error.kind,
            ErrorKind::Host("cleanup plan allocation failed")
        );
    }
    #[test]
    fn later_sealing_plan_failure_publishes_no_executable() {
        let p = program();
        p.resources.validate(&p).unwrap();
        let cleanup_sites = crate::ownership::analyze(&p).unwrap();
        crate::effects::calls::check(&p).unwrap();
        crate::effects::custody::check(&p, &cleanup_sites).unwrap();
        for after in 0..8 {
            let result = failing(after, || {
                super::super::super::seal(
                    p.clone(),
                    AdmissionLimits::default(),
                    cleanup_sites.clone(),
                    Vec::new(),
                )
            });
            assert!(matches!(result.err().unwrap().kind, ErrorKind::Host(_)));
        }
    }
    #[test]
    fn validated_executable_rechecks_every_fallible_plan_attempt_as_host() {
        let e =
            super::super::super::prepare_synthetic(program(), AdmissionLimits::default()).unwrap();
        e.validate().unwrap();
        // Shape plans, catalog plans, correspondence plans, and lowering plans.
        for after in 0..16 {
            assert!(matches!(
                failing(after, || e.validate()).unwrap_err().kind,
                ErrorKind::Host(_)
            ));
        }
        e.validate().unwrap();
    }
    #[test]
    fn malformed_plan_and_configured_graph_limits_remain_distinct() {
        let mut p = program();
        p.fields[0].ty = ValueType::Record(hir::RecordId(1));
        assert_eq!(
            ExecutionError::from(p.resources.plans(&p).unwrap_err()).kind,
            ErrorKind::Integrity("cyclic owned cleanup schema")
        );
        let p = program();
        assert_eq!(
            ExecutionError::from(p.resources.plans_bounded(&p, 1).unwrap_err()).kind,
            ErrorKind::ConfiguredLimit("cleanup type graph depth")
        );
        let mut large = p.clone();
        large.records.resize(4097, p.records[0].clone());
        assert_eq!(
            large.resources.plans(&large).unwrap_err(),
            PlanError::Limit("cleanup type graph exceeds bounded capacity")
        );
        assert!(matches!(
            ExecutionError::from(large.resources.plans(&large).unwrap_err()).kind,
            ErrorKind::ConfiguredLimit(_)
        ));
    }
}

#[cfg(test)]
mod replacement_authority_tests {
    use super::*;
    fn artifact() -> CheckedExecutable {
        super::super::super::prepare_synthetic(
            hir::resources::fixture_with(
                "fn demo(){let mut a=acquire();if true{let inner=acquire();a=acquire();}}",
            ),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    #[test]
    fn canonical_replacement_uses_declaring_scope_and_fixed_plan() {
        let e = artifact();
        e.validate().unwrap();
        let f = e.mir.functions().last().unwrap();
        let operations: Vec<_> = f
            .blocks()
            .flat_map(|(_, b)| b.statements())
            .filter_map(|s| {
                if let Statement::ReplaceOwned {
                    owning_scope,
                    cleanup_plan,
                    origin,
                    ..
                } = s
                {
                    Some((*owning_scope, *cleanup_plan, origin.kind))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            operations,
            vec![(0, Some(hir::RecordId(0)), OriginKind::Replacement)]
        );
    }
    #[test]
    fn independent_replacement_authority_mutations_reject() {
        for fault in 0..11 {
            let mut e = artifact();
            e.validate().unwrap();
            let f = e.mir.test_functions_mut().last_mut().unwrap();
            let b = f
                .test_blocks_mut()
                .iter_mut()
                .find(|b| {
                    b.statements()
                        .iter()
                        .any(|s| matches!(s, Statement::ReplaceOwned { .. }))
                })
                .unwrap();
            let operations = b.test_statements_mut();
            let index = operations
                .iter()
                .position(|s| matches!(s, Statement::ReplaceOwned { .. }))
                .unwrap();
            let copy = operations[index].clone();
            match fault {
                0 => {
                    operations.remove(index);
                }
                1 => operations.insert(index, copy),
                2 => operations.swap(0, index),
                10 => {
                    let Statement::ReplaceOwned {
                        local, value, span, ..
                    } = copy
                    else {
                        panic!()
                    };
                    operations[index] = Statement::Assign { local, value, span };
                }
                _ => {
                    let Statement::ReplaceOwned {
                        local,
                        record,
                        owning_scope,
                        cleanup_plan,
                        origin,
                        ..
                    } = &mut operations[index]
                    else {
                        panic!()
                    };
                    match fault {
                        3 => *local = hir::LocalId(999),
                        4 => *record = hir::RecordId(1),
                        5 => *owning_scope = 1,
                        6 => *cleanup_plan = None,
                        7 => origin.kind = OriginKind::Statement,
                        8 => origin.span.line += 1,
                        9 => *cleanup_plan = Some(hir::RecordId(1)),
                        _ => unreachable!(),
                    }
                }
            }
            assert!(e.validate().is_err(), "fault {fault}");
        }
    }
    #[test]
    fn replacement_result_boundary_does_not_admit_argument_temporary() {
        let p = hir::resources::fixture_with(
            "fn pass(x:Leaf)->Leaf{return x;}fn demo(){let mut a=acquire();a=pass(acquire());}",
        );
        let e = super::super::super::prepare_synthetic(p, AdmissionLimits::default()).unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        m.resume().unwrap();
        let completed: Vec<_> = m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Complete(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(completed, vec![0, 1]);
        super::conservation_tests::conserved(&m);
        artifact().validate().unwrap();
    }
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    fn artifact(body: &str) -> CheckedExecutable {
        super::super::super::prepare_synthetic(
            hir::resources::fixture_with(body),
            AdmissionLimits::default(),
        )
        .unwrap()
    }
    #[test]
    fn repeated_local_id_has_distinct_activation_and_generation() {
        let e = artifact("type N=range 0..3;fn demo(){let mut n=N_start();while !(n==0){let a=acquire();n=n-1;}}fn N_start()->N{return 3;}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        let mut identities = std::collections::BTreeSet::new();
        while let Some(task) = m.tasks.pop() {
            m.step(task).unwrap();
            if let Some(s) = &m.resources {
                for f in &s.frames {
                    for a in &f.scopes {
                        for g in &a.initialized {
                            assert_eq!(
                                f.generations.iter().find(|(l, _)| *l == g.local).unwrap().1,
                                g.identity
                            );
                            identities.insert((a.identity, g.identity));
                        }
                    }
                }
            }
        }
        assert_eq!(identities.len(), 3);
        conservation_tests::conserved(&m);
    }
    #[test]
    fn stale_history_cannot_release_a_live_slot() {
        let e = artifact("fn demo(){let a=acquire();}");
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        loop {
            let task = m.tasks.pop().unwrap();
            let commit = matches!(task.action, Action::Commit(_));
            m.step(task).unwrap();
            if commit {
                break;
            }
        }
        m.resources.as_mut().unwrap().frames[0].scopes[0].initialized[0].identity = u64::MAX;
        assert!(matches!(
            m.resume().unwrap_err().kind,
            ErrorKind::Integrity("stale generation cannot clean current slot")
        ));
        assert!(!m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::Complete(_))));
        conservation_tests::conserved(&m);
    }
}

#[cfg(test)]
mod temporary_aggregate_tests {
    use super::*;
    use crate::mir::temporaries::Destination as Transfer;

    fn middle_field_stop(m: &mut Machine<'_>) {
        loop {
            let task = m.tasks.pop().unwrap();
            if let Action::SiteEval(_, _, activation, site) = &task.action {
                let function = m.frames[m.frame_index(task.frame).unwrap()].function;
                let destination = m
                    .executable
                    .function(function)
                    .unwrap()
                    .temporaries
                    .as_ref()
                    .unwrap()
                    .sites[site.0]
                    .destination;
                if destination == Transfer::Pending(hir::RecordId(1), hir::FieldId(1)) {
                    let u = m.inject_temporary_leaf(*activation).unwrap();
                    m.resources.as_mut().unwrap().failure_at = Some(u);
                    m.tasks.push(task);
                    break;
                }
            }
            m.step(task).unwrap();
            conservation_tests::conserved(m);
        }
        assert!(matches!(m.resume().unwrap_err().kind, ErrorKind::Host(_)));
        conservation_tests::conserved(m);
    }

    #[test]
    fn private_a_aggregate_b_retirement_keeps_both_orders_independent() {
        let p = aggregate_tests::program("fn pass(a:Leaf,g:Pair,b:Leaf)->Leaf{return a;}fn demo(){let r=pass(acquire(),Pair{last:0,first:0,inert:0},acquire());}", [true,false,true]);
        let e = crate::execution::prepare_synthetic(p, AdmissionLimits::default()).unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        loop {
            let task = m.tasks.pop().unwrap();
            if matches!(task.action, Action::Invoke(_, 3)) {
                break;
            }
            m.step(task).unwrap();
            conservation_tests::conserved(&m);
        }
        let Value::Temporary(id) = &m.values[0] else {
            panic!()
        };
        let activation = id.activation;
        assert!(m.values.iter().all(|v| matches!(v, Value::Temporary(_))));
        m.values.clear();
        m.temporary_retire(activation).unwrap();
        m.temporary_end(activation).unwrap();
        let ids: Vec<_> = m
            .resources
            .as_ref()
            .unwrap()
            .events
            .iter()
            .filter_map(|e| {
                if let Event::Complete(id) = e {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        // B, then G's last-declared/first-declared leaves, then A.
        assert_eq!(ids, vec![3, 1, 2, 0]);
        conservation_tests::conserved(&m);
    }

    #[test]
    fn middle_pending_field_keeps_moved_source_membership_after_child_stop() {
        let mut p = aggregate_tests::program(
            "fn demo(){let source=acquire();let owner=Pair{last:0,inert:0,first:0};}",
            [true; 3],
        );
        let hir::Function::Ordinary(f) = p.functions.last_mut().unwrap() else {
            panic!()
        };
        let hir::ValueStatement::Let { initializer, .. } = &mut f.body[1] else {
            panic!()
        };
        let ExprKind::RecordConstruct { fields, .. } = &mut initializer.kind else {
            panic!()
        };
        fields
            .iter_mut()
            .find(|f| f.field == hir::FieldId(1))
            .unwrap()
            .value
            .kind = ExprKind::Local(hir::LocalId(0));
        let e = crate::execution::prepare_synthetic(p, AdmissionLimits::default()).unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        middle_field_stop(&mut m);
        assert!(matches!(m.frames[0].slots[0].1, Slot::Transferred));
        assert!(matches!(m.frames[0].slots[1].1, Slot::Uninitialized));
        let pending = &m.resources.as_ref().unwrap().pending[0];
        assert!(!pending.complete);
        assert_eq!(pending.initialized.len(), 2);
        assert!(matches!(
            pending
                .initialized
                .iter()
                .find(|(f, _)| *f == hir::FieldId(1))
                .unwrap()
                .1,
            Value::Leaf { identity: 0, .. }
        ));
        conservation_tests::conserved(&m);
    }

    #[test]
    fn nested_pending_last_written_middle_field_cannot_publish_before_retirement() {
        let p = aggregate_tests::nested(
            "record Wrap{x:R,last:R}fn demo(){let owner=Wrap{last:0,x:0};}",
        );
        let e = crate::execution::prepare_synthetic(p, AdmissionLimits::default()).unwrap();
        let mut m = Machine::new(&e, ExecutionLimits::default());
        m.start(e.entry("demo").unwrap()).unwrap();
        middle_field_stop(&mut m);
        let pending = &m.resources.as_ref().unwrap().pending;
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|p| !p.complete));
        assert_eq!(pending[0].initialized.len(), 1);
        assert_eq!(pending[1].initialized.len(), 3);
        assert!(matches!(m.frames[0].slots[0].1, Slot::Uninitialized));
        conservation_tests::conserved(&m);
    }

    #[test]
    fn each_remaining_aggregate_leaf_failure_retains_other_temporary_owners() {
        let p = aggregate_tests::program("fn pass(a:Leaf,g:Pair,b:Leaf)->Leaf{return a;}fn demo(){let r=pass(acquire(),Pair{last:0,first:0,inert:0},acquire());}", [true,false,true]);
        let e = crate::execution::prepare_synthetic(p, AdmissionLimits::default()).unwrap();
        let order = [3, 1, 2, 0];
        for (position, failure) in order.iter().enumerate() {
            let mut m = Machine::new(&e, ExecutionLimits::default());
            m.start(e.entry("demo").unwrap()).unwrap();
            loop {
                let task = m.tasks.pop().unwrap();
                if matches!(task.action, Action::Invoke(_, 3)) {
                    break;
                }
                m.step(task).unwrap();
            }
            let Value::Temporary(id) = &m.values[0] else {
                panic!()
            };
            let activation = id.activation;
            m.values.clear();
            m.resources.as_mut().unwrap().failure_at = Some(*failure);
            assert!(matches!(
                m.temporary_retire(activation).unwrap_err().kind,
                ErrorKind::Host(_)
            ));
            let completed: Vec<_> = m
                .resources
                .as_ref()
                .unwrap()
                .events
                .iter()
                .filter_map(|e| {
                    if let Event::Complete(id) = e {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(completed, order[..position]);
            assert!(m.resources.as_ref().unwrap().terminal);
            assert!(matches!(
                m.resume().unwrap_err().kind,
                ErrorKind::Integrity(_)
            ));
            let holders = conservation_tests::holders(&m);
            assert!(holders
                .iter()
                .any(|(id, label)| id == failure && label == "cleanup-current"));
            if *failure != 0 {
                assert!(holders
                    .iter()
                    .any(|(id, label)| *id == 0 && label.starts_with("temporary:")));
            }
            conservation_tests::conserved(&m);
        }
    }
}
