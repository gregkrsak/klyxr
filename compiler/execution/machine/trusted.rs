//! The fixed deterministic binding and persistent production custody seam.
//! A binding writes into an already-owned receipt. No Drop implementation calls
//! release, and terminal reporting never consumes the machine or live payloads.
use super::*;
use crate::trusted::{approval::Invocation, Binding, OperationClass, Scenario};
use std::sync::atomic::{AtomicU64, Ordering};
mod completion;
#[cfg(test)]
thread_local! {
    static CONFORMANCE_RESPONSE: std::cell::Cell<Option<(u64,hir::RangeTypeId,i64)>>=const {std::cell::Cell::new(None)};
}
#[cfg(test)]
pub(in crate::execution::machine) fn conformance_physical_receipt_failure(machine:&mut Machine<'_>,invocation:u64) {
    machine.trusted.as_mut().unwrap().fault=Some((Fault::AfterReceiptHost,invocation));
}
#[cfg(test)]
pub(in crate::execution::machine) fn with_conformance_response<T>(invocation:u64,ty:hir::RangeTypeId,value:i64,run:impl FnOnce()->T)->T {
    struct Restore(Option<(u64,hir::RangeTypeId,i64)>);
    impl Drop for Restore {fn drop(&mut self){CONFORMANCE_RESPONSE.with(|slot|slot.set(self.0));}}
    let _restore=Restore(CONFORMANCE_RESPONSE.with(|slot|slot.replace(Some((invocation,ty,value)))));
    run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Attempt(u64, hir::TrustedOperationId),
    Received(u64, u8),
    Published(u64, hir::ResultTag),
    ReleaseAttempt(u64, hir::TrustedOperationId),
    ReleaseComplete(u64),
    ReleaseFailure(u64),
    FinishPrefix(u64, u8),
    FinishProtected(u64, hir::ResultTag),
}
// Neither fixture tokens nor raw binding payloads are Copy or Clone.
#[derive(Debug)]
struct Sensor {
    realm: u64,
    identity: u64,
}
#[derive(Debug)]
enum Payload {
    Sensor(Sensor),
    Range(RangeValue),
}
#[derive(Debug)]
struct Receipt {
    invocation: u64,
    plan: usize,
    expected_sensor: u64,
    tag: u8,
    payload: Option<Payload>,
}
struct Entry {
    token: Option<Sensor>,
}
pub(super) struct Production {
    pub(super) realm: u64,
    pub(super) cell: results::OwnerId,
    pub(super) plan: usize,
    pub(super) invocation: u64,
    pub(super) tag: hir::ResultTag,
    pub(super) range: Option<RangeValue>,
    pub(super) sensor: Option<u64>,
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    CompletionTicket,
    CompletionRealm,
    CompletionIdentity,
    CompletionFrame,
    CompletionInputFrame,
    CompletionErrorTwo,
    ProtectedTag,
    ProtectedPayload,
    ProtectedRealm,
    ProtectedIdentity,
    ProtectedReceiver,
    ProtectedContractOne,
    ProtectedContractTwo,
    DisabledCompletion,
    BeforeInvocationHost,
    AfterReceiptHost,
    BadTag,
    BadRange,
    PublicationLimit,
    ReleaseFailure,
    ForeignSensor,
    DuplicateSensor,
    ErrorWithSensor,
    WrongRangeIdentity,
    WrongErrorCode,
    WrongInvocation,
    ForgedObservation,
    StaleObservation,
    Allocation(Allocation),
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Allocation {
    Identity,
    Provenance,
    Audit,
    Operand,
    ResultCell,
    TemporaryOwner,
    TemporaryEvent,
    ReleaseAudit,
    CompletionTransfer,
    CompletionCleanup,
    CompletionDiagnostic,
}
#[cfg(test)]
thread_local! { static ACTUAL_GROWTH: std::cell::Cell<u64> = const {std::cell::Cell::new(0)}; }
enum Status {
    Pending,
    Complete(ExecutionResult),
    Stopped(ExecutionError),
}
pub(super) struct State {
    realm: u64,
    entry: FunctionId,
    next_invocation: u64,
    next_sensor: u64,
    receipt: Option<Receipt>,
    entries: Vec<Entry>,
    productions: Vec<Production>,
    events: Vec<Event>,
    status: Status,
    stop_storage: usize,
    completion: Option<completion::Pending>,
    diagnostic: std::cell::RefCell<Option<Vec<CallTrace>>>,
    #[cfg(test)]
    fault: Option<(Fault, u64)>,
}
pub(super) fn compatibility_shapes() -> [usize; 8] {
    [
        size_of::<State>(),
        size_of::<Entry>(),
        size_of::<Production>(),
        size_of::<Event>(),
        size_of::<Sensor>(),
        size_of::<Receipt>(),
        size_of::<completion::Pending>(),
        size_of::<Payload>(),
    ]
}
impl State {
    pub(in crate::execution::machine) fn controlled_response(
        &self,
        sensor: hir::ResourceTypeId,
    ) -> Option<conformance::replay::Response> {
        use conformance::replay::Response;
        let receipt = self.receipt.as_ref()?;
        let prefix = self.events.iter().try_fold(0u8, |sum, event| {
            if let Event::FinishPrefix(invocation, units) = event {
                if *invocation == receipt.invocation {
                    return sum.checked_add(*units);
                }
            }
            Some(sum)
        })?;
        match (receipt.tag, receipt.payload.as_ref()) {
            (0, Some(Payload::Sensor(_))) => Some(Response::Open(sensor)),
            (0, Some(Payload::Range(value))) => {
                Some(Response::Value(value.ty, value.value, prefix))
            }
            (1, Some(Payload::Range(value))) => {
                Some(Response::Error(value.ty, value.value, prefix))
            }
            _ => None,
        }
    }
    pub(in crate::execution::machine) fn observe(&self, c: &mut conformance::Capture) {
        use conformance::{Part, Record, ABSENT};
        c.row(
            Part::TrustedState,
            &[
                self.realm,
                self.entry.0 as u64,
                self.entries.len() as u64,
                self.entries.capacity() as u64,
                self.productions.capacity() as u64,
                self.events.capacity() as u64,
                self.stop_storage as u64,
            ],
        );
        let (status, kind, capacity) = match &self.status {
            Status::Pending => (0, 0, 0),
            Status::Stopped(error) => (2, 1, error.trace.capacity()),
            Status::Complete(ExecutionResult::Value(RuntimeValue::Result(layers))) => {
                (1, 2, layers.capacity())
            }
            Status::Complete(ExecutionResult::Value(RuntimeValue::Record { fields, .. })) => {
                (1, 3, fields.capacity())
            }
            Status::Complete(_) => (1, 0, 0),
        };
        c.row(
            Part::TrustedRetained,
            &[
                status,
                kind,
                capacity as u64,
                self.diagnostic.borrow().as_ref().map_or(0, Vec::capacity) as u64,
            ],
        );
        // Token-table positions are physical. Emit actual tokens in their
        // logical acquisition order; retain only the abstract table history
        // quantities needed by the compatibility schedule.
        let mut seen = 0;
        for (index, production) in self.productions.iter().enumerate() {
            if !c.spend() {
                return;
            }
            c.row(
                Part::Production,
                &[
                    index as u64,
                    production.realm,
                    production.cell.0 as u64,
                    production.plan as u64,
                    production.invocation,
                    match production.tag {
                        hir::ResultTag::Ok => 0,
                        hir::ResultTag::Error => 1,
                    },
                    production.sensor.unwrap_or(ABSENT),
                ],
            );
            if let Some(value) = &production.range {
                c.push(Record::Range(value.ty, value.value));
            } else {
                c.push(Record::Absent);
            }
            if let Some(sensor) = production.sensor {
                for entry in &self.entries {
                    if !c.spend() {
                        return;
                    }
                    if let Some(token) = &entry.token {
                        if token.identity == sensor {
                            if token.realm != self.realm {
                                c.fail(conformance::Incomplete::InvalidState);
                                return;
                            }
                            c.row(Part::Token, &[token.realm, token.identity]);
                            seen += 1;
                        }
                    }
                }
            }
            c.push(Record::End);
        }
        let mut live = 0;
        for (index, entry) in self.entries.iter().enumerate() {
            if !c.spend() {
                return;
            }
            if let Some(token) = &entry.token {
                live += 1;
                if token.realm != self.realm {
                    c.fail(conformance::Incomplete::InvalidState);
                    return;
                }
                for other in &self.entries[..index] {
                    if !c.spend() {
                        return;
                    }
                    if other
                        .token
                        .as_ref()
                        .is_some_and(|v| v.identity == token.identity)
                    {
                        c.fail(conformance::Incomplete::InvalidState);
                        return;
                    }
                }
            }
        }
        if seen != live {
            c.fail(conformance::Incomplete::InvalidState);
            return;
        }
        if let Some(receipt) = &self.receipt {
            c.row(
                if receipt.payload.is_none() {
                    Part::PendingReceipt
                } else {
                    Part::Receipt
                },
                &[receipt.invocation, receipt.plan as u64, receipt.tag as u64],
            );
            match &receipt.payload {
                Some(Payload::Sensor(token)) => {
                    if token.realm != self.realm {
                        c.fail(conformance::Incomplete::InvalidState);
                        return;
                    }
                    c.row(Part::Token, &[token.realm, token.identity]);
                }
                Some(Payload::Range(value)) => c.push(Record::Range(value.ty, value.value)),
                None => c.push(Record::Absent),
            }
            c.push(Record::End);
        }
        if let Some(pending) = &self.completion {
            pending.observe(c);
        }
        for event in &self.events {
            if !c.spend() {
                return;
            }
            c.push(Record::TrustedEvent(*event));
        }
    }
    pub(super) fn take_diagnostic(&self) -> Option<Vec<CallTrace>> {
        self.diagnostic.borrow_mut().take()
    }
    pub(super) fn storage(&self) -> Option<usize> {
        let mut n = contract::Schedule::reference().trusted[0]
            .checked_add(
                self.entries
                    .capacity()
                    .checked_mul(contract::Schedule::reference().trusted[1])?,
            )?
            .checked_add(
                self.productions
                    .capacity()
                    .checked_mul(contract::Schedule::reference().trusted[2])?,
            )?
            .checked_add(
                self.events
                    .capacity()
                    .checked_mul(contract::Schedule::reference().trusted[3])?,
            )?;
        let outcome = match &self.status {
            Status::Stopped(e) => e
                .trace
                .capacity()
                .checked_mul(contract::Schedule::reference().machine[6])?,
            Status::Complete(ExecutionResult::Value(RuntimeValue::Result(v))) => v
                .capacity()
                .checked_mul(contract::Schedule::reference().machine[7])?,
            Status::Complete(ExecutionResult::Value(RuntimeValue::Record { fields, .. })) => fields
                .capacity()
                .checked_mul(contract::Schedule::reference().machine[5])?,
            _ => 0,
        };
        let diagnostic = self.diagnostic.borrow().as_ref().map_or(Some(0), |trace| {
            trace
                .capacity()
                .checked_mul(contract::Schedule::reference().machine[6])
        })?;
        n = n.checked_add(self.stop_storage.max(outcome.checked_add(diagnostic)?))?;
        Some(n)
    }
    fn live(&self) -> usize {
        self.entries.iter().filter(|e| e.token.is_some()).count()
            + usize::from(
                self.receipt
                    .as_ref()
                    .is_some_and(|r| matches!(r.payload, Some(Payload::Sensor(_)))),
            )
    }
}

/// One opaque owner remains alive through completion or terminal reporting.
/// Repeated advancement returns the recorded outcome without executing again.
pub struct ExecutionSession<'a> {
    pub(in crate::execution::machine) machine: Machine<'a>,
}
impl<'a> ExecutionSession<'a> {
    pub(in crate::execution::machine) fn recorded(
        &self,
    ) -> Option<Result<&ExecutionResult, &ExecutionError>> {
        match &self.machine.trusted.as_ref().unwrap().status {
            Status::Pending => None,
            Status::Complete(value) => Some(Ok(value)),
            Status::Stopped(error) => Some(Err(error)),
        }
    }
    /// Private milestone control: preserve the tested pre-enable boundary.
    #[cfg(test)]
    pub(crate) fn disable_completion(&mut self) {
        self.machine.trusted.as_mut().unwrap().fault = Some((Fault::DisabledCompletion, 1));
    }
    pub fn new(
        executable: &'a CheckedExecutable,
        entry: FunctionId,
        limits: ExecutionLimits,
    ) -> Result<Self, ExecutionError> {
        crate::execution::checked_entry(executable, entry, limits)?;
        if executable.trusted.is_none() {
            return Err(ExecutionError::integrity(
                "persistent Sensor session requires an explicitly selected profile",
            ));
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let realm = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| {
                ExecutionError::new(
                    ErrorKind::ConfiguredLimit("execution session identities"),
                    START,
                )
            })?;
        let stop_storage = limits
            .trace_depth
            .checked_mul(contract::Schedule::reference().machine[6])
            .ok_or_else(|| ExecutionError::integrity("stopped diagnostic storage overflow"))?;
        let mut machine = Machine::new(executable, limits);
        machine.trusted = Some(State {
            realm,
            entry,
            next_invocation: 0,
            next_sensor: 0,
            receipt: None,
            entries: Vec::new(),
            productions: Vec::new(),
            events: Vec::new(),
            status: Status::Pending,
            stop_storage,
            completion: None,
            diagnostic: std::cell::RefCell::new(None),
            #[cfg(test)]
            fault: None,
        });
        machine.reserve(0)?;
        Ok(Self { machine })
    }
    pub fn advance(&mut self) -> Result<&ExecutionResult, &ExecutionError> {
        if matches!(
            self.machine.trusted.as_ref().unwrap().status,
            Status::Pending
        ) {
            let entry = self.machine.trusted.as_ref().unwrap().entry;
            let outcome = self
                .machine
                .start(entry)
                .and_then(|_| self.machine.resume());
            if outcome.is_err() {
                self.machine.terminal = true;
            }
            self.machine.trusted.as_mut().unwrap().status = match outcome {
                Ok(v) => Status::Complete(v),
                Err(e) => Status::Stopped(e),
            };
            self.machine.observe(conformance::Cut::Final);
            let state = self.machine.trusted.as_ref().unwrap();
            if let Some(capture) = self.machine.observation.borrow_mut().as_mut() {
                capture.outcome(match &state.status {
                    Status::Complete(value) => Ok(value),
                    Status::Stopped(error) => Err(error),
                    Status::Pending => unreachable!(),
                });
            }
        }
        match &self.machine.trusted.as_ref().unwrap().status {
            Status::Complete(v) => Ok(v),
            Status::Stopped(e) => Err(e),
            Status::Pending => unreachable!(),
        }
    }
    pub fn events(&self) -> &[Event] {
        &self.machine.trusted.as_ref().unwrap().events
    }
    pub fn live_sensors(&self) -> usize {
        self.machine.trusted.as_ref().unwrap().live()
    }
    pub fn is_stopped(&self) -> bool {
        matches!(
            self.machine.trusted.as_ref().unwrap().status,
            Status::Stopped(_)
        )
    }
    /// Rendering shares the configured storage allowance with retained custody.
    pub fn render_authority(&self, limits: ExecutionLimits) -> Result<String, ExecutionError> {
        let retained = self
            .machine
            .storage()
            .ok_or_else(|| ExecutionError::integrity("session storage overflow"))?;
        crate::execution::audit::render(self.machine.executable, limits, retained)
    }
}

impl<'a> Machine<'a> {
    #[cfg(test)]
    pub(super) fn trusted_allocation(
        &self,
        site: Allocation,
        growth: bool,
    ) -> Result<(), ExecutionError> {
        if growth {
            ACTUAL_GROWTH.with(|mask| mask.set(mask.get() | (1u64 << site as u32)));
        }
        if growth
            && self
                .trusted
                .as_ref()
                .is_some_and(|s| s.fault == Some((Fault::Allocation(site), s.next_invocation)))
        {
            return Err(self.failure(ErrorKind::Host("injected trusted allocation boundary")));
        }
        Ok(())
    }
    fn reserve_trusted_operand(&mut self) -> Result<(), ExecutionError> {
        if self.values.len() == self.values.capacity() {
            self.reserve(self.executable.approval.schedule.machine[2])?;
            #[cfg(test)]
            self.trusted_allocation(Allocation::Operand, true)?;
            self.values
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("trusted Result operand reservation")))?;
        }
        Ok(())
    }
    fn reserve_trusted(&mut self, open: bool) -> Result<(), ExecutionError> {
        let state = self
            .trusted
            .as_ref()
            .ok_or_else(|| self.integrity("trusted invocation without persistent session"))?;
        if state.receipt.is_some() {
            return Err(self.integrity("trusted receipt already occupied"));
        }
        let entry_growth = usize::from(
            open && state.entries.iter().all(|e| e.token.is_some())
                && state.entries.len() == state.entries.capacity(),
        ) * self.executable.approval.schedule.trusted[1];
        let production_growth =
            usize::from(state.productions.len() == state.productions.capacity())
                * self.executable.approval.schedule.trusted[2];
        let events = 3usize.saturating_sub(state.events.capacity() - state.events.len());
        let bytes = entry_growth
            .checked_add(production_growth)
            .and_then(|n| {
                n.checked_add(events.checked_mul(self.executable.approval.schedule.trusted[3])?)
            })
            .ok_or_else(|| self.integrity("trusted reservation overflow"))?;
        self.reserve(bytes)?;
        if entry_growth > 0 {
            #[cfg(test)]
            self.trusted_allocation(Allocation::Identity, true)?;
            self.trusted
                .as_mut()
                .unwrap()
                .entries
                .try_reserve_exact(1)
                .map_err(|_| self.failure(ErrorKind::Host("Sensor identity reservation")))?;
        }
        if production_growth > 0 {
            #[cfg(test)]
            self.trusted_allocation(Allocation::Provenance, true)?;
            self.trusted
                .as_mut()
                .unwrap()
                .productions
                .try_reserve_exact(1)
                .map_err(|_| {
                    self.failure(ErrorKind::Host("trusted Result provenance reservation"))
                })?;
        }
        #[cfg(test)]
        self.trusted_allocation(Allocation::Audit, events > 0)?;
        self.trusted
            .as_mut()
            .unwrap()
            .events
            .try_reserve_exact(3)
            .map_err(|_| self.failure(ErrorKind::Host("trusted invocation audit reservation")))?;
        Ok(())
    }

    fn shared_sensor(&self) -> Result<u64, ExecutionError> {
        let selected = self
            .executable
            .trusted
            .as_ref()
            .ok_or_else(|| self.integrity("missing trusted runtime authority"))?;
        let reference = self
            .values
            .last()
            .ok_or_else(|| self.integrity("missing Sensor observation argument"))?;
        let Value::Reference {
            kind: BorrowKind::Shared,
            referent: ReferentType::Resource(ty),
            location,
        } = reference
        else {
            return Err(self.integrity("Sensor observation requires an exact shared reference"));
        };
        if *ty != selected.catalog.sensor {
            return Err(self.integrity("foreign Sensor referent type"));
        }
        let Value::Resource {
            ty: actual,
            identity,
        } = self.slot(*location)?
        else {
            return Err(self.integrity("Sensor reference lacks a live owner"));
        };
        if actual != ty {
            return Err(self.integrity("Sensor referent/owner type mismatch"));
        }
        let state = self
            .trusted
            .as_ref()
            .ok_or_else(|| self.integrity("Sensor observation outside persistent session"))?;
        if !state.entries.iter().any(|e| {
            e.token
                .as_ref()
                .is_some_and(|t| t.realm == state.realm && t.identity == *identity)
        }) {
            return Err(self.integrity("stale or foreign Sensor identity"));
        }
        Ok(*identity)
    }

    pub(super) fn invoke_selected(
        &mut self,
        index: usize,
        count: usize,
        activation: u64,
        frame: u64,
    ) -> Result<(), ExecutionError> {
        self.charge()?;
        // Complete bounded scans of invocation/resource metadata are charged,
        // including failed lookups and already-released table slots.
        let metadata = self.trusted.as_ref().map_or(0, |s| s.entries.len());
        let scans = metadata
            .checked_mul(8)
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| self.integrity("trusted scan work overflow"))?;
        for _ in 0..scans {
            self.charge()?;
        }
        let approved = self
            .executable
            .trusted
            .as_ref()
            .ok_or_else(|| self.integrity("trusted invocation lacks runtime approval"))?;
        let plan = approved
            .invocations
            .get(index)
            .filter(|p| p.ordinal == index)
            .ok_or_else(|| self.integrity("foreign trusted invocation plan"))?;
        let op = approved
            .catalog
            .operation(plan.operation)
            .ok_or_else(|| self.integrity("foreign trusted operation identity"))?;
        let current_function = self.frames[self.frame_index(frame)?].function;
        if current_function != plan.function
            || count != usize::from(op.parameter.is_some())
            || self.expression != Some(plan.source)
            || !approved.catalog.granted(op.id)
            || op.source_name.is_none()
        {
            return Err(self.integrity("trusted invocation signature/origin/authority mismatch"));
        }
        if op.class == OperationClass::Completion {
            #[cfg(test)]
            ACTUAL_GROWTH.with(|mask| mask.set(0));
            #[cfg(test)]
            if self.trusted.as_ref().unwrap().fault
                == Some((
                    Fault::CompletionInputFrame,
                    self.trusted.as_ref().unwrap().next_invocation,
                ))
            {
                self.corrupt_completion_input_frame();
            }
            self.completion_prepare(index, activation, frame)?;
            #[cfg(test)]
            if self.trusted.as_ref().unwrap().fault
                == Some((
                    Fault::DisabledCompletion,
                    self.trusted.as_ref().unwrap().next_invocation,
                ))
            {
                return Err(
                    self.integrity("completion invocation disabled before custody protocol")
                );
            }
            return self.completion_consume(frame);
        }
        let open = op.class == OperationClass::Acquisition;
        if open && !approved.catalog.granted(approved.catalog.resource.release) {
            return Err(self.integrity("Sensor acquisition lacks exact release authority"));
        }
        if self.executable.approval.schedule.trusted[7] > approved.catalog.resource.payload_bytes {
            return Err(self.integrity("binding payload exceeds sealed reservation bound"));
        }
        #[cfg(test)]
        if !open {
            let state = self.trusted.as_ref().unwrap();
            let fault = state
                .fault
                .filter(|(_, at)| *at == state.next_invocation)
                .map(|(f, _)| f);
            if matches!(
                fault,
                Some(Fault::ForgedObservation | Fault::StaleObservation)
            ) {
                let Some(Value::Reference { location, .. }) = self.values.last() else {
                    unreachable!()
                };
                let location = *location;
                if fault == Some(Fault::StaleObservation) {
                    let Some(Value::Reference { location, .. }) = self.values.last_mut() else {
                        unreachable!()
                    };
                    location.frame = u64::MAX;
                } else {
                    let (f, i) = self.slot_index(location)?;
                    let Slot::Available(Value::Resource { identity, .. }) =
                        &mut self.frames[f].slots[i].1
                    else {
                        unreachable!()
                    };
                    *identity = u64::MAX;
                }
            }
        }
        let observed = if open {
            None
        } else {
            Some(self.shared_sensor()?)
        };
        let state = self
            .trusted
            .as_ref()
            .ok_or_else(|| self.integrity("trusted dispatch outside persistent session"))?;
        if open && state.live() >= approved.catalog.selection.sensor_limit() {
            return Err(self.failure(ErrorKind::ConfiguredLimit("live Sensor count")));
        }
        let invocation = state.next_invocation;
        let next_invocation = invocation.checked_add(1).ok_or_else(|| {
            self.failure(ErrorKind::ConfiguredLimit("trusted invocation identities"))
        })?;
        let expected_sensor = state.next_sensor;
        expected_sensor
            .checked_add(1)
            .ok_or_else(|| self.failure(ErrorKind::ConfiguredLimit("Sensor identities")))?;
        let production = state.productions.len();
        self.reserve_trusted(open)?;
        let ticket = self.trusted_result_prepare(production)?;
        self.reserve_trusted_operand()?;
        self.temporary_reserve_register(activation, plan.site)?;
        self.control_begin(index, invocation)?;
        #[cfg(test)]
        if self.trusted.as_ref().unwrap().fault == Some((Fault::BeforeInvocationHost, invocation)) {
            return Err(self.failure(ErrorKind::Host("injected trusted invocation boundary")));
        }
        // All immediate receiving containers now have capacity together. Receipt
        // ownership is installed before the attempt or binding can run.
        let state = self.trusted.as_mut().unwrap();
        state.next_invocation = next_invocation;
        state.receipt = Some(Receipt {
            invocation,
            plan: index,
            expected_sensor,
            tag: 255,
            payload: None,
        });
        self.control_phase(conformance::replay::Phase::HandoffAccepted)?;
        self.control_phase(conformance::replay::Phase::BeforeAttempt)?;
        self.control_response(false)?;
        self.trusted
            .as_mut()
            .unwrap()
            .events
            .push(Event::Attempt(invocation, op.id));
        self.control_phase(conformance::replay::Phase::Attempt)?;
        let controlled = self.control_response(true)?;
        let state = self.trusted.as_mut().unwrap();
        match controlled {
            Some(conformance::replay::Response::Open(ty))
                if open && ty == approved.catalog.sensor =>
            {
                let receipt = state.receipt.as_mut().unwrap();
                receipt.tag = 0;
                receipt.payload = Some(Payload::Sensor(Sensor {
                    realm: state.realm,
                    identity: expected_sensor,
                }));
                state.next_sensor = expected_sensor + 1;
            }
            Some(conformance::replay::Response::Value(ty, value, 0)) if !open => {
                let receipt = state.receipt.as_mut().unwrap();
                receipt.tag = 0;
                receipt.payload = Some(Payload::Range(RangeValue { ty, value }));
            }
            Some(conformance::replay::Response::Error(ty, value, 0)) => {
                let receipt = state.receipt.as_mut().unwrap();
                receipt.tag = 1;
                receipt.payload = Some(Payload::Range(RangeValue { ty, value }));
            }
            Some(_) => return Err(self.integrity("controlled binding response mismatch")),
            None => match op.binding {
                Binding::SensorOpenV1 if open && observed.is_none() => {
                    let receipt = state.receipt.as_mut().unwrap();
                    match approved.catalog.selection.scenario() {
                        Scenario::NoSensor | Scenario::OpenIoFailure => {
                            receipt.tag = 1;
                            receipt.payload = Some(Payload::Range(RangeValue {
                                ty: approved.ranges[1].id,
                                value: if approved.catalog.selection.scenario()
                                    == Scenario::NoSensor
                                {
                                    1
                                } else {
                                    3
                                },
                            }));
                        }
                        _ => {
                            receipt.payload = Some(Payload::Sensor(Sensor {
                                realm: state.realm,
                                identity: expected_sensor,
                            }));
                            receipt.tag = 0;
                            state.next_sensor = expected_sensor + 1;
                        }
                    }
                }
                Binding::SensorReadV1 if !open && observed.is_some() => {
                    let receipt = state.receipt.as_mut().unwrap();
                    let scenario = approved.catalog.selection.scenario();
                    receipt.tag = u8::from(matches!(
                        scenario,
                        Scenario::NoSample | Scenario::ReadIoFailure
                    ));
                    receipt.payload = Some(Payload::Range(RangeValue {
                        ty: if receipt.tag == 0 {
                            approved.ranges[0].id
                        } else {
                            approved.ranges[1].id
                        },
                        value: match scenario {
                            Scenario::NoSample => 2,
                            Scenario::ReadIoFailure => 3,
                            _ => 42,
                        },
                    }));
                }
                _ => {
                    return Err(
                        self.integrity("exact trusted runtime binding mismatch; no fallback")
                    )
                }
            },
        }
        let tag = state.receipt.as_ref().unwrap().tag;
        state.events.push(Event::Received(invocation, tag));
        #[cfg(test)]
        if let Some((_,ty,value))=CONFORMANCE_RESPONSE.with(|slot|slot.get()).filter(|(at,_,_)|*at==invocation) {
            CONFORMANCE_RESPONSE.with(|slot|slot.set(None));
            if let Some(Payload::Range(actual))=self.trusted.as_mut().unwrap().receipt.as_mut().unwrap().payload.as_mut() {
                *actual=RangeValue{ty,value};
            }
        }
        self.control_committed_response()?;
        #[cfg(test)]
        let state = self.trusted.as_mut().unwrap();
        #[cfg(test)]
        if let Some((fault, _)) = state.fault.filter(|(_, selected)| *selected == invocation) {
            match fault {
                Fault::AfterReceiptHost => {
                    return Err(self.failure(ErrorKind::Host("injected trusted receipt boundary")))
                }
                Fault::BadTag => state.receipt.as_mut().unwrap().tag = 255,
                Fault::BadRange => {
                    if let Some(Payload::Range(value)) =
                        state.receipt.as_mut().unwrap().payload.as_mut()
                    {
                        value.value = 4096;
                    }
                }
                Fault::ForeignSensor | Fault::DuplicateSensor => {
                    if let Some(Payload::Sensor(token)) =
                        state.receipt.as_mut().unwrap().payload.as_mut()
                    {
                        if fault == Fault::ForeignSensor {
                            token.realm += 1;
                        } else {
                            token.identity = 0;
                        }
                    }
                }
                Fault::ErrorWithSensor => state.receipt.as_mut().unwrap().tag = 1,
                Fault::WrongRangeIdentity => {
                    if let Some(Payload::Range(value)) =
                        state.receipt.as_mut().unwrap().payload.as_mut()
                    {
                        value.ty = if value.ty == approved.ranges[0].id {
                            approved.ranges[1].id
                        } else {
                            approved.ranges[0].id
                        };
                    }
                }
                Fault::WrongErrorCode => {
                    if let Some(Payload::Range(value)) =
                        state.receipt.as_mut().unwrap().payload.as_mut()
                    {
                        value.value = if open { 2 } else { 1 };
                    }
                }
                Fault::WrongInvocation => state.receipt.as_mut().unwrap().plan = usize::MAX,
                _ => {}
            }
        }
        self.validate_receipt(plan, open)?;
        self.control_phase(conformance::replay::Phase::Validation)?;
        #[cfg(test)]
        if self.trusted.as_ref().unwrap().fault == Some((Fault::PublicationLimit, invocation)) {
            return Err(self.failure(ErrorKind::ConfiguredLimit("trusted publication storage")));
        }
        let state = self.trusted.as_ref().unwrap();
        if !self.trusted_cell_capacity()
            || self.values.len() == self.values.capacity()
            || state.productions.len() == state.productions.capacity()
            || state.events.len() == state.events.capacity()
            || open
                && state.entries.iter().all(|e| e.token.is_some())
                && state.entries.len() == state.entries.capacity()
        {
            return Err(self.failure(ErrorKind::ConfiguredLimit("trusted publication storage")));
        }
        self.control_phase(conformance::replay::Phase::Delivery)?;
        // No fallible operation follows removal from the receipt. The physical
        // Sensor enters a reserved machine-owned table; the non-Copy value is
        // its sole language-level ticket. A terminal session retains both.
        let state = self.trusted.as_mut().unwrap();
        let receipt = state.receipt.take().unwrap();
        let tag = if receipt.tag == 0 {
            hir::ResultTag::Ok
        } else {
            hir::ResultTag::Error
        };
        let (payload, range, sensor) = match receipt.payload.unwrap() {
            Payload::Sensor(token) => {
                let identity = token.identity;
                if let Some(entry) = state.entries.iter_mut().find(|e| e.token.is_none()) {
                    entry.token = Some(token);
                } else {
                    state.entries.push(Entry { token: Some(token) });
                }
                (
                    Value::Resource {
                        ty: approved.catalog.sensor,
                        identity,
                    },
                    None,
                    Some(identity),
                )
            }
            Payload::Range(value) => (
                Value::Public(RuntimeValue::Range(value.clone())),
                Some(value),
                None,
            ),
        };
        state.productions.push(Production {
            realm: state.realm,
            cell: ticket.owner,
            plan: index,
            invocation,
            tag,
            range,
            sensor,
        });
        state.events.push(Event::Published(invocation, tag));
        if count != 0 {
            self.values.pop();
        }
        self.trusted_result_commit(ticket, plan, tag, payload);
        self.control_end();
        Ok(())
    }

    fn validate_receipt(&self, plan: &Invocation, open: bool) -> Result<(), ExecutionError> {
        let approved = self.executable.trusted.as_ref().unwrap();
        let state = self.trusted.as_ref().unwrap();
        let receipt = state
            .receipt
            .as_ref()
            .ok_or_else(|| self.integrity("missing owned trusted receipt"))?;
        if receipt.plan != plan.ordinal
            || receipt.invocation.checked_add(1) != Some(state.next_invocation)
        {
            return Err(self.integrity("trusted receipt operation provenance mismatch"));
        }
        match (receipt.tag, receipt.payload.as_ref()) {
            (0, Some(Payload::Sensor(token))) if open => {
                if token.realm != state.realm
                    || token.identity != receipt.expected_sensor
                    || state.entries.iter().any(|e| {
                        e.token
                            .as_ref()
                            .is_some_and(|old| old.identity == token.identity)
                    })
                {
                    return Err(self.integrity("duplicate or foreign fresh Sensor identity"));
                }
            }
            (tag, Some(Payload::Range(value))) if tag == 1 || tag == 0 && !open => {
                let range = &approved.ranges[usize::from(tag == 1)];
                if value.ty != range.id
                    || value.value < range.min
                    || value.value > range.max
                    || tag == 1 && !matches!((open, value.value), (true, 1 | 3) | (false, 2 | 3))
                {
                    return Err(self
                        .integrity("imported trusted range or operation Error contract mismatch"));
                }
            }
            _ => return Err(self.integrity("trusted receipt tag or active payload mismatch")),
        }
        Ok(())
    }

    pub(super) fn validate_trusted_cell(
        &self,
        id: results::OwnerId,
        production: usize,
        cell: &results::Cell,
    ) -> Result<(), ExecutionError> {
        let state = self
            .trusted
            .as_ref()
            .ok_or_else(|| self.integrity("trusted Result outside persistent session"))?;
        let row = state
            .productions
            .get(production)
            .filter(|r| r.cell == id)
            .ok_or_else(|| self.integrity("foreign trusted Result provenance"))?;
        let approved = self.executable.trusted.as_ref().unwrap();
        let plan = approved
            .invocations
            .get(row.plan)
            .ok_or_else(|| self.integrity("foreign trusted Result invocation"))?;
        if cell.function != plan.function
            || cell.ty != plan.result
            || cell.origin != plan.source
            || Self::result_tag(cell.tag)? != row.tag
            || row.invocation >= state.next_invocation
            || row.realm != state.realm
            || u64::try_from(production).ok() != Some(row.invocation)
        {
            return Err(
                self.integrity("trusted-produced Result operation/tag/type/origin correspondence")
            );
        }
        let operation = approved
            .catalog
            .operation(plan.operation)
            .ok_or_else(|| self.integrity("missing trusted production operation"))?;
        let contract = match (row.tag, row.range.as_ref(), row.sensor) {
            (hir::ResultTag::Ok, None, Some(_)) => operation.class == OperationClass::Acquisition,
            (tag, Some(value), None) => {
                let expected = if tag == hir::ResultTag::Error {
                    operation.failure
                } else {
                    match operation.success {
                        Some(crate::trusted::Payload::Range(id)) => Some(id),
                        _ => None,
                    }
                };
                approved
                    .ranges
                    .iter()
                    .find(|range| Some(range.id) == expected)
                    .is_some_and(|range| {
                        value.ty == range.id
                            && value.value >= range.min
                            && value.value <= range.max
                            && (tag != hir::ResultTag::Error
                                || u32::try_from(value.value).ok().is_some_and(|code| {
                                    code < 8 && operation.errors & (1u8 << code) != 0
                                }))
                    })
            }
            _ => false,
        };
        if !contract {
            return Err(
                self.integrity("trusted production violates exact operation outcome contract")
            );
        }
        if cell.phase == results::Phase::CompletionProtected
            && !state.completion.as_ref().is_some_and(|pending| {
                pending.protected == Some(id)
                    && pending.plan == row.plan
                    && pending.invocation == row.invocation
            })
        {
            return Err(self.integrity("protected completion invocation association"));
        }
        if let Some(payload) = cell.payload.as_ref() {
            let valid = match payload {
                Value::Resource { ty, identity } => {
                    *ty == approved.catalog.sensor
                        && row.sensor == Some(*identity)
                        && row.range.is_none()
                }
                Value::Public(RuntimeValue::Range(value)) => {
                    row.range.as_ref() == Some(value) && row.sensor.is_none()
                }
                _ => false,
            };
            if !valid {
                return Err(self.integrity("trusted-produced Result active payload correspondence"));
            }
        }
        Ok(())
    }

    pub(super) fn release_sensor(&mut self, value: Value) -> Result<(), ExecutionError> {
        self.resources.as_mut().unwrap().current = Some(value);
        self.charge()?;
        let scans = self
            .trusted
            .as_ref()
            .map_or(0, |s| s.entries.len())
            .checked_add(4)
            .ok_or_else(|| self.integrity("release scan work overflow"))?;
        for _ in 0..scans {
            self.charge()?;
        }
        let (ty, identity) = match self.resources.as_ref().unwrap().current.as_ref().unwrap() {
            Value::Resource { ty, identity } => (*ty, *identity),
            _ => return Err(self.integrity("fixed release requires Resource custody")),
        };
        let approved = self
            .executable
            .trusted
            .as_ref()
            .ok_or_else(|| self.integrity("Sensor release without runtime approval"))?;
        let release = approved
            .catalog
            .operation(approved.catalog.resource.release)
            .filter(|op| {
                op.binding == Binding::SensorReleaseV1
                    && op.source_name.is_none()
                    && op.class == OperationClass::Release
                    && approved.catalog.granted(op.id)
            })
            .ok_or_else(|| self.integrity("missing or denied exact cleanup-only Sensor release"))?;
        let state = self
            .trusted
            .as_ref()
            .ok_or_else(|| self.integrity("Sensor release outside persistent session"))?;
        let index = state
            .entries
            .iter()
            .position(|e| {
                e.token
                    .as_ref()
                    .is_some_and(|t| t.identity == identity && t.realm == state.realm)
            })
            .ok_or_else(|| self.integrity("stale or duplicate Sensor cleanup identity"))?;
        if ty != approved.catalog.sensor {
            return Err(self.integrity("foreign Sensor cleanup type"));
        }
        let needed = 2usize.saturating_sub(state.events.capacity() - state.events.len());
        self.reserve(
            needed
                .checked_mul(self.executable.approval.schedule.trusted[3])
                .ok_or_else(|| self.integrity("Sensor release audit overflow"))?,
        )?;
        #[cfg(test)]
        self.trusted_allocation(Allocation::ReleaseAudit, needed > 0)?;
        self.trusted
            .as_mut()
            .unwrap()
            .events
            .try_reserve_exact(2)
            .map_err(|_| self.failure(ErrorKind::Host("Sensor release audit reservation")))?;
        let controlled_invocation = if self.control.borrow().is_some() {
            let state = self.trusted.as_ref().unwrap();
            Some(
                state
                    .productions
                    .iter()
                    .find(|row| row.sensor == Some(identity) && row.realm == state.realm)
                    .ok_or_else(|| self.integrity("controlled release provenance"))?
                    .invocation,
            )
        } else {
            None
        };
        self.control_begin_release()?;
        self.control_release_response(controlled_invocation, false)?;
        let state = self.trusted.as_mut().unwrap();
        state
            .events
            .push(Event::ReleaseAttempt(identity, release.id));
        #[cfg(test)]
        if state
            .fault
            .is_some_and(|(fault, _)| fault == Fault::ReleaseFailure)
        {
            state.events.push(Event::ReleaseFailure(identity));
            self.control_release_failed();
            return Err(self.failure(ErrorKind::Host("injected fixed Sensor release")));
        }
        let modeled = self.control_release_attempt()?;
        let response = if modeled {
            None
        } else {
            self.control_release_response(controlled_invocation, true)?
        };
        if modeled || response == Some(false) {
            self.trusted
                .as_mut()
                .unwrap()
                .events
                .push(Event::ReleaseFailure(identity));
            self.control_release_failed();
            return Err(self.failure(ErrorKind::Host("modeled abstract Host")));
        }
        let state = self.trusted.as_mut().unwrap();
        // Ordinary source gains no explicit completion Result from fixed release.
        state.entries[index].token = None;
        state.events.push(Event::ReleaseComplete(identity));
        self.resources.as_mut().unwrap().current = None;
        if !self
            .trusted
            .as_ref()
            .unwrap()
            .completion
            .as_ref()
            .is_some_and(|pending| {
                pending.phase == crate::trusted::approval::CompletionPhase::Releasing
            })
        {
            self.control_release_complete()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod completion_bound_tests;
#[cfg(test)]
mod completion_composition_tests;
#[cfg(test)]
mod completion_runtime_tests;
#[cfg(test)]
mod tests;
