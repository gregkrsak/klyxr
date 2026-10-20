//! Separately admitted, non-owning conformance evidence. This module does not
//! execute source or grant an operation. The ordinary driver remains unchanged.
use super::*;
use std::cell::Ref;

/// All limits belong to observation, never to ordinary fuel or storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub records: usize,
    pub work: usize,
    pub storage: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self { records: 32_768, work: 1_000_000, storage: 16_777_216 }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Incomplete {
    Admission,
    Allocation,
    Records,
    Work,
    UncoveredState,
    InvalidState,
    Pending,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cut {
    Start,
    Fuel,
    Storage(usize),
    Transition,
    Final,
}

// A flat vocabulary bounds traversal, comparison and destruction. No record
// contains a live payload, heap-owning recursive value or authority handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Record {
    Cut(Cut, u64, usize, Span, Option<Span>, bool),
    Frame(u64, FunctionId, BasicBlockId, usize, Option<u64>),
    Slot(Place, u8),
    Operand,
    Bool(bool),
    Range(hir::RangeTypeId, i64),
    Raw(i64),
    Reference(BorrowKind, ReferentType, u64, Place),
    NoValue,
    End,
    Delivered,
    Stop(u8, Span, Option<Span>),
    Trap(hir::RangeTypeId, i64, i64, i128),
    Limit(&'static str),
}
pub(super) struct Capture {
    pub(super) records: Vec<Record>,
    limits: Limits,
    work: usize,
    pub(super) incomplete: Option<Incomplete>,
    finished: bool,
}
impl Capture {
    fn new(limits: Limits) -> Self {
        let mut capture = Self {
            records: Vec::new(), limits, work: 0,
            incomplete: None, finished: false,
        };
        let bytes = limits.records.checked_mul(size_of::<Record>())
            .and_then(|n| n.checked_add(size_of::<Self>()));
        if limits.records == 0 || limits.records > 1_048_576
            || limits.work == 0 || limits.work > 100_000_000
            || limits.storage > 268_435_456
            || bytes.map_or(true, |n| n > limits.storage) {
            capture.incomplete = Some(Incomplete::Admission);
        } else if capture.records.try_reserve_exact(limits.records).is_err() {
            capture.incomplete = Some(Incomplete::Allocation);
        }
        capture
    }
    pub(super) fn fail(&mut self, reason: Incomplete) {
        if self.incomplete.is_none() { self.incomplete = Some(reason); }
    }
    pub(super) fn push(&mut self, record: Record) {
        if self.incomplete.is_some() { return; }
        if self.work == self.limits.work { self.fail(Incomplete::Work); return; }
        self.work += 1;
        if self.records.len() == self.limits.records {
            self.fail(Incomplete::Records);
        } else {
            // Capacity was reserved before execution. This cannot allocate.
            self.records.push(record);
        }
    }
    fn public(&mut self, value: &RuntimeValue) {
        match value {
            RuntimeValue::Bool(v) => self.push(Record::Bool(*v)),
            RuntimeValue::Range(v) => self.push(Record::Range(v.ty, v.value)),
            // Coverage is explicit. An unimplemented census can never be Equal.
            _ => self.fail(Incomplete::UncoveredState),
        }
    }
    pub(super) fn value(&mut self, value: &Value) {
        match value {
            Value::Public(v) => self.public(v),
            Value::Raw(v) => self.push(Record::Raw(*v)),
            Value::Reference { kind, referent, location } =>
                self.push(Record::Reference(*kind, *referent, location.frame, location.place)),
            Value::Completion => self.push(Record::NoValue),
            _ => self.fail(Incomplete::UncoveredState),
        }
    }
    fn outcome(&mut self, outcome: Result<&ExecutionResult, &ExecutionError>) {
        match outcome {
            Ok(v) => {
                self.push(Record::Delivered);
                match v {
                    ExecutionResult::NoValue => self.push(Record::NoValue),
                    ExecutionResult::Value(v) => self.public(v),
                }
            }
            Err(e) => {
                let class = match e.kind {
                    ErrorKind::RangeTrap { .. } => 1,
                    ErrorKind::ConfiguredLimit(_) => 2,
                    ErrorKind::Integrity(_) => 3,
                    ErrorKind::Host(_) => 4,
                    _ => { self.fail(Incomplete::UncoveredState); return; }
                };
                self.push(Record::Stop(class, e.origin, e.expression));
                match e.kind {
                    ErrorKind::RangeTrap { ty, min, max, computed } =>
                        self.push(Record::Trap(ty, min, max, computed)),
                    ErrorKind::ConfiguredLimit(name) => self.push(Record::Limit(name)),
                    _ => (),
                }
                // Diagnostic trace wording/capacity are presentation. Logical
                // activations have already been captured, without taking the
                // session's reserved diagnostic backing.
            }
        }
        self.push(Record::End);
        self.finished = true;
    }
}

impl Machine<'_> {
    pub(super) fn observe(&self, cut: Cut) {
        let mut observer = self.observation.borrow_mut();
        let Some(c) = observer.as_mut() else { return; };
        if c.incomplete.is_some() { return; }
        c.push(Record::Cut(cut, self.steps, self.frames.len(), self.origin.span,
            self.expression, self.terminal));
        let mut parent = None;
        for f in &self.frames {
            let Ok(definition) = self.executable.function(f.function) else {
                c.fail(Incomplete::InvalidState); return;
            };
            if definition.blocks().find(|(id, _)| *id == f.block).is_none() {
                c.fail(Incomplete::InvalidState); return;
            }
            c.push(Record::Frame(f.id, f.function, f.block, f.statement, parent));
            parent = Some(f.id);
            for (place, slot) in &f.slots {
                let state = match slot {
                    Slot::Uninitialized => 0, Slot::Available(_) => 1,
                    Slot::Vacant => 2, Slot::Transferred => 3,
                    Slot::Cleaned => 4, Slot::Replacing => 5,
                };
                c.push(Record::Slot(*place, state));
                if let Slot::Available(value) = slot { c.value(value); }
            }
            c.push(Record::End);
        }
        for value in &self.values {
            c.push(Record::Operand); c.value(value);
        }
        if self.resources.is_some() || self.results.is_some()
            || self.temporaries.is_some() || self.trusted.is_some() {
            c.fail(Incomplete::UncoveredState);
        }
        c.push(Record::End);
    }
}

enum Engine<'a> {
    Ordinary { machine: Machine<'a>, outcome: Option<Result<ExecutionResult, ExecutionError>> },
    Selected(trusted::ExecutionSession<'a>),
}
/// Retains the actual interpreter session; evidence owns no resource token.
/// Public conformance inputs are the already approved zero-parameter root and
/// existing limits. No source/public resource input authority is added.
pub struct Session<'a> {
    engine: Engine<'a>,
    entry: FunctionId,
}
impl<'a> Session<'a> {
    pub fn new(contract: &contract::Contract<'a>, execution: ExecutionLimits,
        observation: Limits) -> Result<Self, ExecutionError> {
        contract.validate()?;
        checked_entry(contract.executable, contract.entry(), execution)?;
        let engine = if contract.executable.trusted.is_some() {
            Engine::Selected(trusted::ExecutionSession::new(contract.executable, contract.entry(), execution)?)
        } else {
            Engine::Ordinary { machine: Machine::new(contract.executable, execution), outcome: None }
        };
        let mut session = Self { engine, entry: contract.entry() };
        *session.machine_mut().observation.get_mut() = Some(Capture::new(observation));
        session.machine().observe(Cut::Start);
        Ok(session)
    }
    fn machine(&self) -> &Machine<'a> {
        match &self.engine { Engine::Ordinary { machine, .. } => machine,
            Engine::Selected(s) => &s.machine }
    }
    fn machine_mut(&mut self) -> &mut Machine<'a> {
        match &mut self.engine { Engine::Ordinary { machine, .. } => machine,
            Engine::Selected(s) => &mut s.machine }
    }
    pub fn advance(&mut self) -> Result<&ExecutionResult, &ExecutionError> {
        match &mut self.engine {
            Engine::Ordinary { machine, outcome } => {
                if outcome.is_none() {
                    *outcome = Some(machine.start(self.entry).and_then(|_| machine.resume()));
                    if outcome.as_ref().unwrap().is_err() { machine.terminal = true; }
                    machine.observe(Cut::Final);
                    machine.observation.borrow_mut().as_mut().unwrap()
                        .outcome(outcome.as_ref().unwrap().as_ref());
                }
                outcome.as_ref().unwrap().as_ref()
            }
            Engine::Selected(s) => s.advance(),
        }
    }
    fn capture(&self) -> Ref<'_, Capture> {
        Ref::map(self.machine().observation.borrow(), |o| o.as_ref().unwrap())
    }
    pub fn completeness(&self) -> Result<(), Incomplete> {
        let c = self.capture();
        if let Some(reason) = c.incomplete { Err(reason) }
        else if !c.finished { Err(Incomplete::Pending) } else { Ok(()) }
    }
    pub fn record_count(&self) -> usize { self.capture().records.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_capture_is_admitted_separately_and_repeat_is_read_only() {
        let e = prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        let c = contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default()).unwrap();
        let mut s = Session::new(&c, ExecutionLimits::default(), Limits::default()).unwrap();
        assert_eq!(s.completeness(), Err(Incomplete::Pending));
        assert_eq!(s.advance().unwrap(), &ExecutionResult::Value(RuntimeValue::Bool(true)));
        assert_eq!(s.completeness(), Ok(()));
        let count = s.record_count();
        s.advance().unwrap();
        assert_eq!(s.record_count(), count);
    }
    #[test]
    fn exhausted_observation_preserves_primary_execution() {
        let e = prepare_execution("fn demo()->bool{return true;}", AdmissionLimits::default()).unwrap();
        let c = contract::Contract::extract(&e, e.entry("demo").unwrap(), contract::Limits::default()).unwrap();
        for limits in [Limits { records: 1, ..Limits::default() },
            Limits { work: 1, ..Limits::default() },
            Limits { storage: 1, ..Limits::default() }] {
            let mut s = Session::new(&c, ExecutionLimits::default(), limits).unwrap();
            assert_eq!(s.advance().unwrap(), &execute(&e, c.entry(), ExecutionLimits::default()).unwrap());
            assert!(s.completeness().is_err());
        }
    }
}
