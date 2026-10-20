//! Sealed consuming completion, protected Result and existing fixed cleanup.
use super::*;
use crate::trusted::approval::CompletionPhase;
#[cfg(test)]
thread_local! {pub(super) static ENVELOPE:std::cell::Cell<Option<(usize,usize,usize)>> = const {std::cell::Cell::new(None)};}

pub(super) struct Pending {
    pub(super) frame: u64,
    pub(super) invocation: u64,
    pub(super) plan: usize,
    pub(super) input: temporaries::OwnerId,
    pub(super) sensor: u64,
    pub(super) phase: CompletionPhase,
    pub(super) ticket: Option<results::TrustedTicket>,
    pub(super) value: Option<Value>,
    pub(super) protected: Option<results::OwnerId>,
}
impl Pending {
    pub(in crate::execution::machine) fn observe(&self, c: &mut conformance::Capture) {
        use conformance::{Part, Record, ABSENT};
        c.row(
            Part::Completion,
            &[
                self.frame,
                self.invocation,
                self.plan as u64,
                self.input.activation,
                self.input.ordinal as u64,
                self.sensor,
                match self.phase {
                    CompletionPhase::Reserved => 0,
                    CompletionPhase::Consumed => 1,
                    CompletionPhase::Attempted => 2,
                    CompletionPhase::OutcomeProtected => 3,
                    CompletionPhase::ReleasePending => 4,
                    CompletionPhase::Releasing => 5,
                    CompletionPhase::Released => 6,
                    CompletionPhase::Delivered => 7,
                },
            ],
        );
        if let Some(ticket) = &self.ticket {
            ticket.observe(c);
        } else {
            c.push(Record::Absent);
        }
        c.row(
            Part::CompletionOutcome,
            &[self.protected.map_or(ABSENT, |id| id.0 as u64)],
        );
        if let Some(value) = &self.value {
            c.push(Record::Payload);
            c.value(value);
        } else {
            c.push(Record::Absent);
        }
        c.push(Record::End);
    }
}

impl<'a> Machine<'a> {
    pub(in crate::execution::machine) fn completion_consume(
        &mut self,
        frame: u64,
    ) -> Result<(), ExecutionError> {
        self.completion_reserved()?;
        let state = self.trusted.as_ref().unwrap();
        let pending = state.completion.as_ref().unwrap();
        let invocation = &self.executable.trusted.as_ref().unwrap().invocations[pending.plan];
        #[cfg(test)]
        if state.fault == Some((Fault::BeforeInvocationHost, pending.invocation)) {
            return Err(self.failure(ErrorKind::Host("injected trusted invocation boundary")));
        }
        let value = self.pop_to(Some(crate::mir::temporaries::Destination::TrustedArgument(
            invocation.operation,
            0,
        )))?;
        // No fallible operation occurs between taking the current owner and
        // installing its one ticket in persistent completion custody.
        let pending = self.trusted.as_mut().unwrap().completion.as_mut().unwrap();
        pending.value = Some(value);
        pending.phase = CompletionPhase::Consumed;
        #[cfg(test)]
        {
            let state = self.trusted.as_mut().unwrap();
            let pending = state.completion.as_mut().unwrap();
            if let Some((fault, _)) = state.fault.filter(|(_, at)| *at == pending.invocation) {
                match fault {
                    Fault::CompletionTicket => {
                        if let Some(Value::Resource { identity, .. }) = pending.value.as_mut() {
                            *identity = u64::MAX;
                        }
                    }
                    Fault::CompletionRealm => {
                        state
                            .entries
                            .iter_mut()
                            .find_map(|entry| entry.token.as_mut())
                            .unwrap()
                            .realm += 1;
                    }
                    Fault::CompletionIdentity => pending.invocation += 1,
                    Fault::CompletionFrame => pending.frame = u64::MAX,
                    _ => {}
                }
            }
        }
        self.task(
            frame,
            self.origin,
            self.expression,
            Action::CompletionAttempt,
        )
    }

    pub(in crate::execution::machine) fn completion_attempt(
        &mut self,
        frame: u64,
    ) -> Result<(), ExecutionError> {
        self.charge()?;
        let state = self.trusted.as_ref().unwrap();
        let pending = state
            .completion
            .as_ref()
            .ok_or_else(|| self.integrity("missing consumed completion"))?;
        let approved = self.executable.trusted.as_ref().unwrap();
        let plan = approved
            .invocations
            .get(pending.plan)
            .ok_or_else(|| self.integrity("foreign completion invocation"))?;
        let operation = approved
            .catalog
            .operation(plan.operation)
            .ok_or_else(|| self.integrity("foreign completion operation"))?;
        if pending.frame != frame
            || plan.completion.as_ref().map(|p| p.events)
                != Some(crate::trusted::approval::CompletionEvent::PROTOCOL)
            || pending.phase != CompletionPhase::Consumed
            || pending.invocation != state.next_invocation
            || !matches!(pending.value, Some(Value::Resource{ty, identity}) if ty == approved.catalog.sensor && identity == pending.sensor)
            || operation.binding != Binding::SensorFinishV1
            || !approved.catalog.granted(operation.id)
            || !approved.catalog.granted(approved.catalog.resource.release)
            || !state.entries.iter().any(|entry| {
                entry.token.as_ref().is_some_and(|token| {
                    token.realm == state.realm && token.identity == pending.sensor
                })
            })
            || state.receipt.is_some()
            || state.events.capacity() - state.events.len() < 7
            || state.productions.len() == state.productions.capacity()
            || !self.trusted_cell_capacity()
            || self.tasks.capacity() - self.tasks.len() < 2
        {
            return Err(self.integrity("completion attempt custody/authority/envelope mismatch"));
        }
        let invocation = pending.invocation;
        let next = invocation.checked_add(1).ok_or_else(|| {
            self.failure(ErrorKind::ConfiguredLimit("trusted invocation identities"))
        })?;
        let scenario = approved.catalog.selection.scenario();
        let failed = matches!(
            scenario,
            Scenario::FinishIoFailure | Scenario::FinishPartialIoFailure
        );
        let state = self.trusted.as_mut().unwrap();
        state.next_invocation = next;
        state.completion.as_mut().unwrap().phase = CompletionPhase::Attempted;
        state.receipt = Some(Receipt {
            invocation,
            plan: plan.ordinal,
            expected_sensor: state.completion.as_ref().unwrap().sensor,
            tag: u8::from(failed),
            payload: None,
        });
        state.events.push(Event::Attempt(invocation, operation.id));
        // Fixed deterministic binding: one irreversible prefix unit is realized
        // only in the partial-Error scenario. It is never success or rollback.
        if scenario == Scenario::FinishPartialIoFailure {
            state.events.push(Event::FinishPrefix(invocation, 1));
        }
        state.receipt.as_mut().unwrap().payload = Some(Payload::Range(RangeValue {
            ty: if failed {
                approved.catalog.error
            } else {
                approved.catalog.finish_receipt
            },
            value: if failed { 3 } else { 1 },
        }));
        state
            .events
            .push(Event::Received(invocation, u8::from(failed)));
        #[cfg(test)]
        if let Some((fault, _)) = state.fault.filter(|(_, selected)| *selected == invocation) {
            let receipt = state.receipt.as_mut().unwrap();
            match fault {
                Fault::BadTag => receipt.tag = 255,
                Fault::BadRange => {
                    if let Some(Payload::Range(value)) = receipt.payload.as_mut() {
                        value.value = 4096;
                    }
                }
                Fault::WrongRangeIdentity => {
                    if let Some(Payload::Range(value)) = receipt.payload.as_mut() {
                        value.ty = approved.catalog.reading;
                    }
                }
                Fault::WrongErrorCode => {
                    receipt.tag = 1;
                    receipt.payload = Some(Payload::Range(RangeValue {
                        ty: approved.catalog.error,
                        value: 1,
                    }));
                }
                Fault::WrongInvocation => receipt.plan = usize::MAX,
                Fault::CompletionErrorTwo => {
                    receipt.tag = 1;
                    receipt.payload = Some(Payload::Range(RangeValue {
                        ty: approved.catalog.error,
                        value: 2,
                    }));
                }
                _ => {}
            }
        }
        self.completion_receipt_validate(plan)?;
        let state = self.trusted.as_mut().unwrap();
        let receipt = state.receipt.take().unwrap();
        let tag = if receipt.tag == 0 {
            hir::ResultTag::Ok
        } else {
            hir::ResultTag::Error
        };
        let Some(Payload::Range(range)) = receipt.payload else {
            unreachable!("validated completion payload")
        };
        let ticket = state.completion.as_mut().unwrap().ticket.take().unwrap();
        let owner = ticket.owner;
        state.productions.push(Production {
            realm: state.realm,
            cell: owner,
            plan: plan.ordinal,
            invocation,
            tag,
            range: Some(range.clone()),
            sensor: None,
        });
        // The ordinary outcome is now a separate machine-owned Result cell.
        // It contains no Sensor and cannot be observed in CompletionProtected.
        self.completion_result_protect(
            ticket,
            plan,
            tag,
            Value::Public(RuntimeValue::Range(range)),
        );
        let state = self.trusted.as_mut().unwrap();
        let pending = state.completion.as_mut().unwrap();
        pending.protected = Some(owner);
        pending.phase = CompletionPhase::OutcomeProtected;
        state.events.push(Event::FinishProtected(invocation, tag));
        pending.phase = CompletionPhase::ReleasePending;
        #[cfg(test)]
        if let Some((fault, _)) = state.fault.filter(|(_, at)| *at == invocation) {
            match fault {
                Fault::ProtectedTag => self.results.as_mut().unwrap().cells[owner.0].tag = 255,
                Fault::ProtectedPayload => {
                    if let Some(Value::Public(RuntimeValue::Range(value))) =
                        self.results.as_mut().unwrap().cells[owner.0]
                            .payload
                            .as_mut()
                    {
                        value.value = 2;
                    }
                }
                Fault::ProtectedRealm => state.productions.last_mut().unwrap().realm += 1,
                Fault::ProtectedIdentity => state.productions.last_mut().unwrap().invocation = 0,
                Fault::ProtectedReceiver => pending.protected = Some(results::OwnerId(0)),
                Fault::ProtectedContractOne | Fault::ProtectedContractTwo => {
                    let code = if fault == Fault::ProtectedContractOne {
                        1
                    } else {
                        2
                    };
                    state
                        .productions
                        .last_mut()
                        .unwrap()
                        .range
                        .as_mut()
                        .unwrap()
                        .value = code;
                    if let Some(Value::Public(RuntimeValue::Range(value))) =
                        self.results.as_mut().unwrap().cells[owner.0]
                            .payload
                            .as_mut()
                    {
                        value.value = code;
                    }
                }
                _ => {}
            }
        }
        #[cfg(test)]
        if state.fault == Some((Fault::AfterReceiptHost, invocation)) {
            return Err(self.failure(ErrorKind::Host("injected trusted receipt boundary")));
        }
        self.task(
            frame,
            plan.completion.as_ref().unwrap().delivery,
            Some(plan.source),
            Action::CompletionDeliver,
        )?;
        self.task(
            frame,
            plan.completion.as_ref().unwrap().cleanup,
            Some(plan.source),
            Action::CompletionRelease,
        )
    }

    fn completion_receipt_validate(&self, plan: &Invocation) -> Result<(), ExecutionError> {
        let state = self.trusted.as_ref().unwrap();
        let receipt = state
            .receipt
            .as_ref()
            .ok_or_else(|| self.integrity("missing completion response"))?;
        let pending = state.completion.as_ref().unwrap();
        let approved = self.executable.trusted.as_ref().unwrap();
        let valid = match (receipt.tag, receipt.payload.as_ref()) {
            (0, Some(Payload::Range(value))) => {
                value.ty == approved.catalog.finish_receipt && value.value == 1
            }
            (1, Some(Payload::Range(value))) => {
                value.ty == approved.catalog.error && value.value == 3
            }
            _ => false,
        };
        if !valid
            || receipt.plan != plan.ordinal
            || receipt.invocation != pending.invocation
            || receipt.expected_sensor != pending.sensor
            || receipt.invocation.checked_add(1) != Some(state.next_invocation)
        {
            return Err(self.integrity("completion response tag/payload/provenance contract"));
        }
        Ok(())
    }

    pub(in crate::execution::machine) fn completion_release(
        &mut self,
        frame: u64,
    ) -> Result<(), ExecutionError> {
        self.charge()?;
        let state = self.trusted.as_ref().unwrap();
        let pending = state
            .completion
            .as_ref()
            .ok_or_else(|| self.integrity("missing completion release continuation"))?;
        if pending.frame != frame
            || pending.phase != CompletionPhase::ReleasePending
            || pending.value.is_none()
        {
            return Err(self.integrity("completion release continuation phase"));
        }
        self.completion_result_validate(
            pending
                .protected
                .ok_or_else(|| self.integrity("completion release without protected outcome"))?,
        )?;
        if !self.completion_cleanup_idle() {
            return Err(self.integrity("completion release cursor already occupied"));
        }
        let pending = self.trusted.as_mut().unwrap().completion.as_mut().unwrap();
        pending.phase = CompletionPhase::Releasing;
        let value = pending.value.take().unwrap();
        // The one existing cleanup engine takes responsibility immediately.
        // Its detached/work/current cursor retains the ticket at every cut.
        self.release_value(value)?;
        self.trusted
            .as_mut()
            .unwrap()
            .completion
            .as_mut()
            .unwrap()
            .phase = CompletionPhase::Released;
        Ok(())
    }

    pub(in crate::execution::machine) fn completion_deliver(
        &mut self,
        frame: u64,
    ) -> Result<(), ExecutionError> {
        self.charge()?;
        let state = self.trusted.as_ref().unwrap();
        let pending = state
            .completion
            .as_ref()
            .ok_or_else(|| self.integrity("missing completion delivery continuation"))?;
        if pending.frame != frame
            || pending.phase != CompletionPhase::Released
            || pending.value.is_some()
            || state.entries.iter().any(|entry| {
                entry
                    .token
                    .as_ref()
                    .is_some_and(|token| token.identity == pending.sensor)
            })
        {
            return Err(self.integrity("completion delivery before exact fixed release"));
        }
        let owner = pending
            .protected
            .ok_or_else(|| self.integrity("completion delivery without protected outcome"))?;
        let tag = self.completion_result_validate(owner)?;
        if self.values.len() == self.values.capacity()
            || state.events.len() == state.events.capacity()
        {
            return Err(self.failure(ErrorKind::ConfiguredLimit("completion delivery envelope")));
        }
        let invocation = pending.invocation;
        self.completion_result_deliver(owner);
        let state = self.trusted.as_mut().unwrap();
        state.completion.as_mut().unwrap().phase = CompletionPhase::Delivered;
        state.events.push(Event::Published(invocation, tag));
        state.completion = None;
        Ok(())
    }

    pub(super) fn completion_prepare(
        &mut self,
        index: usize,
        activation: u64,
        frame: u64,
    ) -> Result<(), ExecutionError> {
        let approved = self.executable.trusted.as_ref().unwrap();
        let invocation = &approved.invocations[index];
        let plan = invocation
            .completion
            .as_ref()
            .ok_or_else(|| self.integrity("completion lacks sealed protocol"))?;
        let (input, ty, sensor) = self.completion_input(plan.input, invocation.operation)?;
        let state = self.trusted.as_ref().unwrap();
        if state.completion.is_some()
            || state.receipt.is_some()
            || ty != plan.resource
            || input.activation != activation
            || self.frames.last().map(|current| current.id) != Some(frame)
            || self.frames[self.frame_index(frame)?].function != invocation.function
            || !state.entries.iter().any(|entry| {
                entry
                    .token
                    .as_ref()
                    .is_some_and(|token| token.realm == state.realm && token.identity == sensor)
            })
        {
            return Err(self.integrity("completion holder/payload correspondence"));
        }
        state.next_invocation.checked_add(1).ok_or_else(|| {
            self.failure(ErrorKind::ConfiguredLimit("trusted invocation identities"))
        })?;
        let production = state.productions.len();
        let events = 7usize.saturating_sub(state.events.capacity() - state.events.len());
        let provenance = usize::from(production == state.productions.capacity())
            * self.executable.approval.schedule.trusted[2];
        let operands = usize::from(self.values.len() == self.values.capacity())
            * self.executable.approval.schedule.machine[2];
        let tasks = 2usize
            .saturating_sub(self.tasks.capacity() - self.tasks.len())
            .checked_mul(self.executable.approval.schedule.machine[1])
            .ok_or_else(|| self.integrity("completion continuation overflow"))?;
        // Evaluating the owned argument already required its evaluation and
        // registration continuations above this caller's retained stack. Their
        // completed slots cover both release/delivery tasks. An inconsistent
        // private machine must reject before handoff, rather than invent a
        // supposedly reachable allocation boundary here.
        if tasks != 0 {
            return Err(self.integrity("completion evaluation continuation capacity"));
        }
        let total = [
            provenance,
            events
                .checked_mul(self.executable.approval.schedule.trusted[3])
                .ok_or_else(|| self.integrity("completion events overflow"))?,
            operands,
            tasks,
            self.completion_result_growth(),
            self.completion_temporary_growth(activation, invocation.site)?,
            self.completion_cleanup_growth()?,
        ]
        .into_iter()
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| self.integrity("completion combined envelope overflow"))?;
        #[cfg(test)]
        ENVELOPE.with(|record| {
            record.set(Some((
                self.storage().unwrap(),
                total,
                [
                    provenance,
                    events * self.executable.approval.schedule.trusted[3],
                    operands,
                    self.completion_result_growth(),
                    self.completion_temporary_growth(activation, invocation.site)
                        .unwrap(),
                    self.completion_cleanup_growth().unwrap(),
                ]
                .into_iter()
                .max()
                .unwrap(),
            )))
        });
        // This is one simultaneous bound, before any receiving allocation or
        // current-holder transfer. Diagnostics are already charged by State.
        self.reserve(total)?;
        #[cfg(test)]
        self.trusted_allocation(Allocation::Provenance, provenance > 0)?;
        self.trusted
            .as_mut()
            .unwrap()
            .productions
            .try_reserve_exact(1)
            .map_err(|_| self.failure(ErrorKind::Host("completion provenance")))?;
        #[cfg(test)]
        self.trusted_allocation(Allocation::Audit, events > 0)?;
        self.trusted
            .as_mut()
            .unwrap()
            .events
            .try_reserve_exact(7)
            .map_err(|_| self.failure(ErrorKind::Host("completion protocol events")))?;
        let ticket = self.trusted_result_prepare(production)?;
        self.reserve_trusted_operand()?;
        self.completion_temporary_reserve(activation, invocation.site)?;
        self.completion_cleanup_reserve()?;
        if self.trusted.as_ref().unwrap().diagnostic.borrow().is_none() {
            #[cfg(test)]
            self.trusted_allocation(
                Allocation::CompletionDiagnostic,
                self.limits.trace_depth > 0,
            )?;
            let mut trace = Vec::new();
            trace
                .try_reserve_exact(self.limits.trace_depth)
                .map_err(|_| self.failure(ErrorKind::Host("completion diagnostic envelope")))?;
            *self.trusted.as_ref().unwrap().diagnostic.borrow_mut() = Some(trace);
        }
        self.reserve(0)?;
        let state = self.trusted.as_mut().unwrap();
        state.completion = Some(Pending {
            frame,
            invocation: state.next_invocation,
            plan: index,
            input,
            sensor,
            phase: CompletionPhase::Reserved,
            ticket: Some(ticket),
            value: None,
            protected: None,
        });
        self.completion_reserved()
    }

    fn completion_reserved(&self) -> Result<(), ExecutionError> {
        let state = self.trusted.as_ref().unwrap();
        let pending = state
            .completion
            .as_ref()
            .ok_or_else(|| self.integrity("missing completion envelope"))?;
        let plan = self
            .executable
            .trusted
            .as_ref()
            .unwrap()
            .invocations
            .get(pending.plan)
            .ok_or_else(|| self.integrity("foreign pending completion plan"))?;
        let (holder, _, sensor) =
            self.completion_input(plan.completion.as_ref().unwrap().input, plan.operation)?;
        if pending.phase != CompletionPhase::Reserved
            || pending.invocation != state.next_invocation
            || pending.input != holder
            || pending.sensor != sensor
            || pending.value.is_some()
            || pending.protected.is_some()
            || pending.ticket.as_ref().map(|t| t.owner.0)
                != self.results.as_ref().map(|r| r.cells.len())
        {
            return Err(self.integrity("inconsistent reserved completion custody"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trusted::{Grants, Selection};

    fn artifact(relay: bool) -> CheckedExecutable {
        let argument = if relay { "relay(sensor)" } else { "sensor" };
        let source = format!("fn relay(sensor:Sensor)->Sensor{{return sensor;}}fn demo()->Result<success:FinishReceipt,failure:SensorError>effects io{{let sensor=sensor_open()or return;return sensor_finish({argument});}}");
        let selected = Selection::sensor_demo(Scenario::Sample, Grants::ALL, &["demo"]).unwrap();
        prepare_execution_with_profile(&source, AdmissionLimits::default(), &selected).unwrap()
    }

    #[test]
    fn m3_disabled_completion_reserves_but_never_consumes_current_owner() {
        for relay in [false, true] {
            let e = artifact(relay);
            let mut s =
                ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
                    .unwrap();
            s.disable_completion();
            assert!(matches!(
                s.advance().unwrap_err().kind,
                ErrorKind::Integrity(_)
            ));
            let state = s.machine.trusted.as_ref().unwrap();
            let pending = state.completion.as_ref().unwrap();
            assert_eq!(pending.phase, CompletionPhase::Reserved);
            assert!(pending.value.is_none() && pending.protected.is_none());
            assert!(s
                .machine
                .temporaries
                .as_ref()
                .unwrap()
                .owned_value(pending.input)
                .is_some());
            assert_eq!(s.live_sensors(), 1);
            assert_eq!(
                s.events()
                    .iter()
                    .filter(|e| matches!(e, Event::Attempt(..)))
                    .count(),
                1
            );
            let prefix = s.events().to_vec();
            let steps = s.machine.steps;
            assert!(s.advance().is_err());
            assert_eq!(s.events(), prefix);
            assert_eq!(s.machine.steps, steps);
        }
    }

    #[test]
    fn m3_diagnostic_host_cut_keeps_relay_returned_owner_without_attempt() {
        let e = artifact(true);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
            .unwrap();
        s.machine.trusted.as_mut().unwrap().fault =
            Some((Fault::Allocation(Allocation::CompletionDiagnostic), 1));
        assert_eq!(
            s.advance().unwrap_err().kind,
            ErrorKind::Host("injected trusted allocation boundary")
        );
        assert_eq!(s.live_sensors(), 1);
        assert!(s.machine.trusted.as_ref().unwrap().completion.is_none());
        assert_eq!(
            s.events()
                .iter()
                .filter(|e| matches!(e, Event::Attempt(..)))
                .count(),
            1
        );
        let mut current = 0;
        s.machine
            .temporaries
            .as_ref()
            .unwrap()
            .visit_values(|v| current += usize::from(matches!(v, Value::Resource { .. })));
        assert_eq!(current, 1);
        assert!(s
            .machine
            .frames
            .iter()
            .flat_map(|f| &f.slots)
            .all(|(_, slot)| !matches!(slot, Slot::Available(Value::Resource { .. }))));
    }

    #[test]
    fn m3_simultaneous_storage_neighbor_retains_the_input() {
        let e = artifact(true);
        let mut measured =
            ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
                .unwrap();
        measured.disable_completion();
        assert!(matches!(
            measured.advance().unwrap_err().kind,
            ErrorKind::Integrity(_)
        ));
        let peak = measured.machine.max_storage.get();
        for (bytes, ready) in [(peak, true), (peak - 1, false)] {
            let mut s = ExecutionSession::new(
                &e,
                e.entry("demo").unwrap(),
                ExecutionLimits {
                    working_storage: bytes,
                    ..ExecutionLimits::default()
                },
            )
            .unwrap();
            s.disable_completion();
            let kind = s.advance().unwrap_err().kind.clone();
            if ready {
                assert!(matches!(kind, ErrorKind::Integrity(_)));
                assert!(s.machine.trusted.as_ref().unwrap().completion.is_some());
            } else {
                assert_eq!(kind, ErrorKind::ConfiguredLimit("working storage"));
                assert!(s.machine.trusted.as_ref().unwrap().completion.is_none());
            }
            assert_eq!(s.live_sensors(), 1);
            assert_eq!(
                s.events()
                    .iter()
                    .filter(|e| matches!(e, Event::Attempt(..)))
                    .count(),
                1
            );
        }
        println!(
            "K34 M3 relay receiving envelope: sufficient={peak}; insufficient={}",
            peak - 1
        );
    }

    #[test]
    fn m3_receiving_capacities_coexist_before_handoff() {
        let e = artifact(false);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default())
            .unwrap();
        s.disable_completion();
        assert!(s.advance().is_err());
        let state = s.machine.trusted.as_ref().unwrap();
        let pending = state.completion.as_ref().unwrap();
        assert!(state.events.capacity() - state.events.len() >= 7);
        assert!(state.productions.len() < state.productions.capacity());
        assert!(s.machine.trusted_cell_capacity());
        assert!(s.machine.values.len() < s.machine.values.capacity());
        assert!(s.machine.tasks.capacity() - s.machine.tasks.len() >= 2);
        assert_eq!(s.machine.completion_cleanup_growth().unwrap(), 0);
        assert_eq!(
            s.machine
                .completion_temporary_growth(
                    pending.input.activation,
                    e.trusted.as_ref().unwrap().invocations[pending.plan].site
                )
                .unwrap(),
            0
        );
        assert!(state.diagnostic.borrow().is_none()); // moved into the retained terminal error
        if let Status::Stopped(error) = &state.status {
            assert!(error.trace.capacity() >= s.machine.limits.trace_depth);
        } else {
            panic!("disabled dispatch must be terminal");
        }
    }
}
