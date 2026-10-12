//! Private synthetic resource execution. Fixed operations never call user code.
use super::*;
use crate::hir::resources::OperationKind;
use crate::mir::cleanup::ScopeId;
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    Acquire(u64),
    Attempt(u64),
    Complete(u64),
    Failure(u64),
    Retire(u64),
}
#[derive(Default)]
pub(super) struct State {
    pub(super) events: Vec<Event>,
    frames: Vec<ResourceFrame>,
    next_identity: u64,
    next_activation: u64,
    protected: Option<Value>,
    current: Option<Value>,
    pub(super) failure_at: Option<u64>,
    terminal: bool,
}
struct ResourceFrame {
    id: u64,
    entered: bool,
    scopes: Vec<Activation>,
}
struct Activation {
    scope: ScopeId,
    _identity: u64,
    initialized: Vec<hir::LocalId>,
}
impl State {
    pub(super) fn storage(&self) -> Option<usize> {
        let mut n = size_of::<Self>()
            .checked_add(self.events.capacity().checked_mul(size_of::<Event>())?)?
            .checked_add(
                self.frames
                    .capacity()
                    .checked_mul(size_of::<ResourceFrame>())?,
            )?;
        for f in &self.frames {
            n = n.checked_add(f.scopes.capacity().checked_mul(size_of::<Activation>())?)?;
            for a in &f.scopes {
                n = n.checked_add(
                    a.initialized
                        .capacity()
                        .checked_mul(size_of::<hir::LocalId>())?,
                )?;
            }
        }
        for v in [&self.protected, &self.current].into_iter().flatten() {
            n = n.checked_add(v.extra_storage())?;
        }
        Some(n)
    }
}
impl Machine<'_> {
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
        if self.resources.is_none() {
            return Ok(());
        }
        let s = self.resources.as_ref().unwrap();
        if s.frames.len() == s.frames.capacity() {
            self.reserve(size_of::<ResourceFrame>())?;
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
        });
        Ok(())
    }
    pub(super) fn cleanup_enter_block(&mut self, id: u64) -> Result<(), ExecutionError> {
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
                _identity: identity,
                initialized: Vec::new(),
            });
        }
        self.resources.as_mut().unwrap().frames[r].entered = true;
        Ok(())
    }
    pub(super) fn cleanup_next_block(&mut self, id: u64) -> Result<(), ExecutionError> {
        if self.resources.is_some() {
            let r = self.resource_frame_index(id)?;
            self.resources.as_mut().unwrap().frames[r].entered = false;
        }
        Ok(())
    }
    pub(super) fn cleanup_initialized(&mut self, at: Location) -> Result<(), ExecutionError> {
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
        if a.scope != init.scope || a.initialized.contains(&init.local) {
            return Err(self.integrity("duplicate or wrong-scope initialization"));
        }
        let grow = a.initialized.len() == a.initialized.capacity();
        self.origin = init.origin;
        self.charge()?;
        if grow {
            self.reserve(size_of::<hir::LocalId>())?;
            self.resources.as_mut().unwrap().frames[r]
                .scopes
                .last_mut()
                .unwrap()
                .initialized
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("initialization history allocation")))?;
        }
        self.resources.as_mut().unwrap().frames[r]
            .scopes
            .last_mut()
            .unwrap()
            .initialized
            .push(init.local);
        Ok(())
    }
    fn release_value(&mut self, value: Value) -> Result<(), ExecutionError> {
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
    fn cleanup_owner(&mut self, at: Location) -> Result<(), ExecutionError> {
        self.charge()?;
        let (f, i) = self.slot_index(at)?;
        if matches!(self.frames[f].slots[i].1, Slot::Transferred) {
            return Ok(());
        }
        if !matches!(
            self.frames[f].slots[i].1,
            Slot::Available(Value::Leaf { .. })
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
        if self.resources.is_none() {
            return Ok(());
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
            if a.scope != *scope {
                return Err(self.integrity("cleanup exits wrong active scope"));
            }
            loop {
                let local = self.resources.as_ref().unwrap().frames[r]
                    .scopes
                    .last()
                    .unwrap()
                    .initialized
                    .last()
                    .copied();
                let Some(local) = local else {
                    break;
                };
                self.cleanup_owner(Location {
                    frame: id,
                    place: Place::Local(local),
                })?;
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
        self.reserve(value.extra_storage())?;
        if self.resources.as_ref().unwrap().protected.is_some() {
            return Err(self.integrity("protected return already occupied"));
        }
        self.resources.as_mut().unwrap().protected = Some(value);
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
        let value = self.resources.as_mut().unwrap().protected.take().unwrap();
        self.value(value)
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
