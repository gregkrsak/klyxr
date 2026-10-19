//! A sealed receiving envelope; M3 prepares custody but never dispatches.
use super::*;
use crate::trusted::approval::CompletionPhase;

pub(super) struct Pending {
    pub(super) invocation: u64,
    pub(super) plan: usize,
    pub(super) input: temporaries::OwnerId,
    pub(super) sensor: u64,
    pub(super) phase: CompletionPhase,
    pub(super) ticket: Option<results::TrustedTicket>,
    pub(super) value: Option<Value>,
    pub(super) protected: Option<results::OwnerId>,
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
            let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
            assert!(matches!(s.advance().unwrap_err().kind, ErrorKind::Integrity(_)));
            let state = s.machine.trusted.as_ref().unwrap();
            let pending = state.completion.as_ref().unwrap();
            assert_eq!(pending.phase, CompletionPhase::Reserved);
            assert!(pending.value.is_none() && pending.protected.is_none());
            assert!(s.machine.temporaries.as_ref().unwrap().owned_value(pending.input).is_some());
            assert_eq!(s.live_sensors(), 1);
            assert_eq!(s.events().iter().filter(|e| matches!(e, Event::Attempt(..))).count(), 1);
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
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
        s.machine.trusted.as_mut().unwrap().fault = Some((Fault::Allocation(Allocation::CompletionDiagnostic), 1));
        assert_eq!(s.advance().unwrap_err().kind, ErrorKind::Host("injected trusted allocation boundary"));
        assert_eq!(s.live_sensors(), 1);
        assert!(s.machine.trusted.as_ref().unwrap().completion.is_none());
        assert_eq!(s.events().iter().filter(|e| matches!(e, Event::Attempt(..))).count(), 1);
        let mut current = 0;
        s.machine.temporaries.as_ref().unwrap().visit_values(|v| current += usize::from(matches!(v, Value::Resource {..})));
        assert_eq!(current, 1);
        assert!(s.machine.frames.iter().flat_map(|f| &f.slots).all(|(_, slot)| !matches!(slot, Slot::Available(Value::Resource{..}))));
    }

    #[test]
    fn m3_simultaneous_storage_neighbor_retains_the_input() {
        let e = artifact(true);
        let mut measured = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
        assert!(matches!(measured.advance().unwrap_err().kind, ErrorKind::Integrity(_)));
        let peak = measured.machine.max_storage.get();
        for (bytes, ready) in [(peak, true), (peak - 1, false)] {
            let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits { working_storage: bytes, ..ExecutionLimits::default() }).unwrap();
            let kind = s.advance().unwrap_err().kind.clone();
            if ready {
                assert!(matches!(kind, ErrorKind::Integrity(_)));
                assert!(s.machine.trusted.as_ref().unwrap().completion.is_some());
            } else {
                assert_eq!(kind, ErrorKind::ConfiguredLimit("working storage"));
                assert!(s.machine.trusted.as_ref().unwrap().completion.is_none());
            }
            assert_eq!(s.live_sensors(), 1);
            assert_eq!(s.events().iter().filter(|e| matches!(e, Event::Attempt(..))).count(), 1);
        }
        println!("K34 M3 relay receiving envelope: sufficient={peak}; insufficient={}", peak-1);
    }

    #[test]
    fn m3_receiving_capacities_coexist_before_handoff() {
        let e = artifact(false);
        let mut s = ExecutionSession::new(&e, e.entry("demo").unwrap(), ExecutionLimits::default()).unwrap();
        assert!(s.advance().is_err());
        let state = s.machine.trusted.as_ref().unwrap();
        let pending = state.completion.as_ref().unwrap();
        assert!(state.events.capacity() - state.events.len() >= 7);
        assert!(state.productions.len() < state.productions.capacity());
        assert!(s.machine.trusted_cell_capacity());
        assert!(s.machine.values.len() < s.machine.values.capacity());
        assert!(s.machine.tasks.capacity() - s.machine.tasks.len() >= 2);
        assert_eq!(s.machine.completion_cleanup_growth().unwrap(), 0);
        assert_eq!(s.machine.completion_temporary_growth(pending.input.activation, e.trusted.as_ref().unwrap().invocations[pending.plan].site).unwrap(), 0);
        assert!(state.diagnostic.borrow().is_none()); // moved into the retained terminal error
        if let Status::Stopped(error) = &state.status {
            assert!(error.trace.capacity() >= s.machine.limits.trace_depth);
        } else { panic!("disabled dispatch must be terminal"); }
    }
}

impl<'a> Machine<'a> {
    pub(super) fn completion_prepare(
        &mut self,
        index: usize,
        activation: u64,
        frame: u64,
    ) -> Result<(), ExecutionError> {
        let approved = self.executable.trusted.as_ref().unwrap();
        let invocation = &approved.invocations[index];
        let plan = invocation.completion.as_ref().ok_or_else(|| self.integrity("completion lacks sealed protocol"))?;
        let (input, ty, sensor) = self.completion_input(plan.input, invocation.operation)?;
        let state = self.trusted.as_ref().unwrap();
        if state.completion.is_some() || state.receipt.is_some() || ty != plan.resource
            || self.frames[self.frame_index(frame)?].function != invocation.function
            || !state.entries.iter().any(|entry| entry.token.as_ref().is_some_and(|token| token.realm == state.realm && token.identity == sensor))
        {
            return Err(self.integrity("completion holder/payload correspondence"));
        }
        state.next_invocation.checked_add(1).ok_or_else(|| self.failure(ErrorKind::ConfiguredLimit("trusted invocation identities")))?;
        let production = state.productions.len();
        let events = 7usize.saturating_sub(state.events.capacity() - state.events.len());
        let provenance = usize::from(production == state.productions.capacity()) * size_of::<Production>();
        let operands = usize::from(self.values.len() == self.values.capacity()) * size_of::<Value>();
        let tasks = 2usize.saturating_sub(self.tasks.capacity() - self.tasks.len()).checked_mul(size_of::<Task<'_>>()).ok_or_else(|| self.integrity("completion continuation overflow"))?;
        let total = [provenance, events.checked_mul(size_of::<Event>()).ok_or_else(|| self.integrity("completion events overflow"))?, operands, tasks, self.completion_result_growth(), self.completion_temporary_growth(activation, invocation.site)?, self.completion_cleanup_growth()?]
            .into_iter().try_fold(0usize, usize::checked_add).ok_or_else(|| self.integrity("completion combined envelope overflow"))?;
        // This is one simultaneous bound, before any receiving allocation or
        // current-holder transfer. Diagnostics are already charged by State.
        self.reserve(total)?;
        #[cfg(test)]
        self.trusted_allocation(Allocation::Provenance, provenance > 0)?;
        self.trusted.as_mut().unwrap().productions.try_reserve_exact(1)
            .map_err(|_| self.failure(ErrorKind::Host("completion provenance")))?;
        #[cfg(test)]
        self.trusted_allocation(Allocation::Audit, events > 0)?;
        self.trusted.as_mut().unwrap().events.try_reserve_exact(7)
            .map_err(|_| self.failure(ErrorKind::Host("completion protocol events")))?;
        let ticket = self.trusted_result_prepare(production)?;
        self.reserve_trusted_operand()?;
        self.completion_temporary_reserve(activation, invocation.site)?;
        self.completion_cleanup_reserve()?;
        #[cfg(test)]
        self.trusted_allocation(Allocation::CompletionContinuation, tasks > 0)?;
        self.tasks.try_reserve_exact(2).map_err(|_| self.failure(ErrorKind::Host("completion continuations")))?;
        if self.trusted.as_ref().unwrap().diagnostic.borrow().is_none() {
            #[cfg(test)]
            self.trusted_allocation(Allocation::CompletionDiagnostic, self.limits.trace_depth > 0)?;
            let mut trace = Vec::new();
            trace.try_reserve_exact(self.limits.trace_depth).map_err(|_| self.failure(ErrorKind::Host("completion diagnostic envelope")))?;
            *self.trusted.as_ref().unwrap().diagnostic.borrow_mut() = Some(trace);
        }
        self.reserve(0)?;
        let state = self.trusted.as_mut().unwrap();
        state.completion = Some(Pending {
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
        let pending = state.completion.as_ref().ok_or_else(|| self.integrity("missing completion envelope"))?;
        let plan = self.executable.trusted.as_ref().unwrap().invocations.get(pending.plan).ok_or_else(|| self.integrity("foreign pending completion plan"))?;
        let (holder, _, sensor) = self.completion_input(plan.completion.as_ref().unwrap().input, plan.operation)?;
        if pending.phase != CompletionPhase::Reserved || pending.invocation != state.next_invocation
            || pending.input != holder || pending.sensor != sensor || pending.value.is_some() || pending.protected.is_some()
            || pending.ticket.as_ref().map(|t| t.owner.0) != self.results.as_ref().map(|r| r.cells.len())
        {
            return Err(self.integrity("inconsistent reserved completion custody"));
        }
        Ok(())
    }
}
