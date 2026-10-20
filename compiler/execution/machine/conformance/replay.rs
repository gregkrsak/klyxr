//! Private Stage I controlled-input context. These are conformance descriptors,
//! never source inputs, a user binding, incoming resource tokens or authority.
//! Logical addresses refer to A; process-local frame/Sensor IDs are lookup keys.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::execution::machine) enum Site {
    Capacity(FunctionId, Span),
    Call(FunctionId, Span),
    Invocation(usize),
    Release(FunctionId, Span),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::execution::machine) enum Phase {
    Reservation,
    BeforeHandoff,
    HandoffAccepted,
    BeforeAttempt,
    Attempt,
    Response,
    Validation,
    Protection,
    ReleaseAttempt,
    ReleaseComplete,
    ReleaseFailure,
    Delivery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::execution::machine) struct Address {
    pub site: Site,
    /// Ordinal in the logical activation ancestry, not a runtime frame number.
    pub activation: usize,
    pub occurrence: u64,
    pub phase: Phase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::execution::machine) struct Birth {
    pub plan: usize,
    pub activation: usize,
    pub occurrence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::execution::machine) enum Response {
    Open(hir::ResourceTypeId),
    Value(hir::RangeTypeId, i64, u8),
    Error(hir::RangeTypeId, i64, u8),
    Release(Birth, bool),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Item {
    pub address: Address,
    pub response: Response,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Limits {
    pub sites: usize,
    pub items: usize,
    pub work: usize,
    pub storage: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sites: 16_384,
            items: 4096,
            work: 100_000_000,
            storage: 134_217_728,
        }
    }
}
/// None uses the already selected deterministic fixture. Some is a strict
/// addressed response transcript, including fixed release outcomes.
pub(super) struct Input<'a> {
    pub responses: Option<&'a [Item]>,
    pub failures: &'a [Address],
    pub limits: Limits,
}
#[derive(Clone, Copy)]
struct Activation {
    function: FunctionId,
    parent: Option<usize>,
    call_site: Span,
    entry_span: Span,
    occurrence: u64,
}
#[derive(Clone, Copy)]
struct Invocation {
    raw: u64,
    birth: Birth,
}
#[derive(Clone, Copy)]
struct Entered {
    site: Site,
    activation: usize,
    occurrence: u64,
}
impl Entered {
    fn address(self, phase: Phase) -> Address {
        Address {
            site: self.site,
            activation: self.activation,
            occurrence: self.occurrence,
            phase,
        }
    }
}
/// All variable backing is admitted before controlled execution. Counters have
/// one fixed row per active logical depth, reclaimed only for the next entry.
/// Complete ancestry and invocation association histories remain separately.
/// No element owns an actual resource or requests cleanup.
pub(in crate::execution::machine) struct Controller {
    approval: Rc<Approval>,
    schema: contract::Schema,
    entry: FunctionId,
    execution: ExecutionLimits,
    sites: Vec<Site>,
    counters: Vec<u64>,
    active: Vec<usize>,
    activations: Vec<Activation>,
    invocations: Vec<Invocation>,
    responses: Vec<Item>,
    response_used: Vec<bool>,
    failures: Vec<Address>,
    failure_used: Vec<bool>,
    strict: bool,
    current: usize,
    protocol: Option<Entered>,
    release: Option<Entered>,
    stopped: Option<Address>,
    rejected: std::cell::Cell<bool>,
    work: usize,
    history: usize,
    root_bound: bool,
    occurrence_bound: u64,
}

type Checked<T> = Result<T, ExecutionError>;
fn invalid() -> ExecutionError {
    ExecutionError::integrity("controlled-input contract/admission")
}
fn spend(work: &mut usize, amount: usize) -> Checked<()> {
    *work = work.checked_sub(amount).ok_or_else(invalid)?;
    Ok(())
}
fn site_key(site: Site) -> (u8, usize, usize, usize, usize, usize) {
    let (tag, function, span) = match site {
        Site::Invocation(plan) => return (2, plan, 0, 0, 0, 0),
        Site::Capacity(function, span) => (0, function, span),
        Site::Call(function, span) => (1, function, span),
        Site::Release(function, span) => (3, function, span),
    };
    (
        tag,
        function.0,
        span.start,
        span.end,
        span.line,
        span.column,
    )
}
fn address_key(address: Address) -> (usize, (u8, usize, usize, usize, usize, usize), u64, u8) {
    (
        address.activation,
        site_key(address.site),
        address.occurrence,
        address.phase as u8,
    )
}
struct Inventory {
    sites: Vec<Site>,
    cap: usize,
    work: usize,
}
impl Inventory {
    fn insert(&mut self, site: Site) -> Checked<()> {
        let mut at = 0;
        while at < self.sites.len() {
            spend(&mut self.work, 1)?;
            let ordering = site_key(self.sites[at]).cmp(&site_key(site));
            if ordering.is_eq() {
                return Ok(());
            }
            if ordering.is_gt() {
                break;
            }
            at += 1;
        }
        if self.sites.len() == self.cap {
            return Err(invalid());
        }
        spend(&mut self.work, self.sites.len() - at + 1)?;
        self.sites.insert(at, site);
        Ok(())
    }
    fn origin(&mut self, function: FunctionId, span: Span) -> Checked<()> {
        self.insert(Site::Capacity(function, span))?;
        self.insert(Site::Release(function, span))
    }
    fn extract(contract: &contract::Contract<'_>, limits: Limits) -> Checked<Self> {
        let mut inventory = Self {
            sites: Vec::new(),
            cap: limits.sites,
            work: limits.work,
        };
        inventory
            .sites
            .try_reserve_exact(limits.sites)
            .map_err(|_| ExecutionError::new(ErrorKind::Host("control site admission"), START))?;
        for function in contract.functions() {
            inventory.origin(
                *function,
                contract.executable.program.function(*function).span(),
            )?;
            if *function == contract.entry() {
                inventory.origin(*function, START)?;
            }
            let definition = contract.executable.function(*function)?;
            for (_, block) in definition.blocks() {
                spend(&mut inventory.work, 1)?;
                for statement in block.statements() {
                    inventory.origin(*function, contract::statement_span(statement))?;
                    if let Statement::BindResult { pattern_origin, .. } = statement {
                        inventory.origin(*function, *pattern_origin)?;
                    }
                    if let Statement::CallNoValue { span, .. } = statement {
                        inventory.insert(Site::Call(*function, *span))?;
                    }
                }
                if let Terminator::Propagate { operation, .. }
                | Terminator::ForwardError { operation } = block.terminator()
                {
                    for span in [
                        operation.span,
                        operation.syntax.phrase,
                        operation.syntax.binding,
                    ] {
                        inventory.origin(*function, span)?;
                    }
                }
            }
            if let Some(flow) = &definition.temporaries {
                for region in &flow.regions {
                    for origin in [region.origin, region.publication, region.retirement] {
                        inventory.origin(*function, origin.span)?;
                    }
                }
                for site in &flow.sites {
                    for origin in [site.origin, site.transfer, site.cleanup] {
                        inventory.origin(*function, origin.span)?;
                    }
                }
            }
            if let Some(flow) = &definition.cleanup {
                for scope in &flow.scopes {
                    inventory.origin(*function, scope.span)?;
                }
                for block in &flow.blocks {
                    spend(&mut inventory.work, 1)?;
                    if let Some((_, origin)) = block.enter {
                        inventory.origin(*function, origin.span)?;
                    }
                    if let Some(origin) = block.exit_origin {
                        inventory.origin(*function, origin.span)?;
                    }
                    for initialized in &block.initialize {
                        inventory.origin(*function, initialized.origin.span)?;
                    }
                }
            }
        }
        for site in contract.sites() {
            inventory.origin(site.function, site.origin)?;
        }
        for (expression, ordinal) in contract.expressions() {
            if matches!(expression.kind, ExprKind::Call { .. }) {
                inventory.insert(Site::Call(
                    contract.sites()[*ordinal].function,
                    expression.span,
                ))?;
            }
        }
        if let Some(approved) = &contract.executable.trusted {
            for (plan, invocation) in approved.invocations.iter().enumerate() {
                spend(&mut inventory.work, contract.functions().len())?;
                if contract.functions().contains(&invocation.function) {
                    inventory.insert(Site::Invocation(plan))?;
                }
            }
        }
        Ok(inventory)
    }
}
impl Controller {
    pub(super) fn admit(
        contract: &contract::Contract<'_>,
        execution: ExecutionLimits,
        input: Input<'_>,
    ) -> Checked<Self> {
        let limits = input.limits;
        let responses = input.responses.unwrap_or(&[]);
        if limits.sites == 0
            || limits.sites > 65_536
            || limits.items > 65_536
            || limits.work == 0
            || limits.work > 1_000_000_000
            || limits.storage == 0
            || limits.storage > 268_435_456
            || responses.len() > limits.items
            || input.failures.len() > limits.items
        {
            return Err(invalid());
        }
        let initial = limits
            .sites
            .checked_mul(size_of::<Site>())
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .ok_or_else(invalid)?;
        if initial > limits.storage {
            return Err(invalid());
        }
        let inventory = Inventory::extract(contract, limits)?;
        let steps = usize::try_from(execution.steps).map_err(|_| invalid())?;
        let history = steps.checked_add(1).ok_or_else(invalid)?;
        let counters = inventory
            .sites
            .len()
            .checked_mul(execution.call_depth)
            .ok_or_else(invalid)?;
        let items = responses
            .len()
            .checked_add(input.failures.len())
            .ok_or_else(invalid)?;
        let runtime_work = history
            .checked_mul(inventory.sites.len().checked_add(128).ok_or_else(invalid)?)
            .and_then(|n| {
                n.checked_mul(
                    history
                        .checked_add(inventory.sites.len())?
                        .checked_add(items)?
                        .checked_add(execution.call_depth)?
                        .checked_add(16)?,
                )
            })
            .ok_or_else(invalid)?;
        if runtime_work > inventory.work {
            return Err(invalid());
        }
        let bytes = initial
            .checked_add(counters.checked_mul(size_of::<u64>()).ok_or_else(invalid)?)
            .and_then(|n| n.checked_add(execution.call_depth.checked_mul(size_of::<usize>())?))
            .and_then(|n| {
                n.checked_add(
                    history.checked_mul(size_of::<Activation>() + size_of::<Invocation>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    responses
                        .len()
                        .checked_mul(size_of::<Item>() + size_of::<bool>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    input
                        .failures
                        .len()
                        .checked_mul(size_of::<Address>() + size_of::<bool>())?,
                )
            })
            .ok_or_else(invalid)?;
        if bytes > limits.storage {
            return Err(invalid());
        }
        let mut c = Self {
            approval: Rc::clone(&contract.executable.approval),
            schema: contract.schema(),
            entry: contract.entry(),
            execution,
            sites: inventory.sites,
            counters: Vec::new(),
            active: Vec::new(),
            activations: Vec::new(),
            invocations: Vec::new(),
            responses: Vec::new(),
            response_used: Vec::new(),
            failures: Vec::new(),
            failure_used: Vec::new(),
            strict: input.responses.is_some(),
            current: 0,
            protocol: None,
            release: None,
            stopped: None,
            rejected: std::cell::Cell::new(false),
            work: runtime_work,
            history,
            root_bound: false,
            occurrence_bound: runtime_work as u64,
        };
        macro_rules! reserve {
            ($field:ident,$count:expr) => {
                c.$field.try_reserve_exact($count).map_err(|_| {
                    ExecutionError::new(ErrorKind::Host("control context admission"), START)
                })?;
            };
        }
        reserve!(counters, counters);
        reserve!(active, execution.call_depth);
        reserve!(activations, history);
        reserve!(invocations, history);
        reserve!(responses, responses.len());
        reserve!(response_used, responses.len());
        reserve!(failures, input.failures.len());
        reserve!(failure_used, input.failures.len());
        c.counters.resize(counters, 0);
        c.active.resize(execution.call_depth, 0);
        c.activations.push(Activation {
            function: contract.entry(),
            parent: None,
            call_site: contract
                .executable
                .program
                .function(contract.entry())
                .span(),
            entry_span: contract
                .executable
                .program
                .function(contract.entry())
                .span(),
            occurrence: 0,
        });
        c.responses.extend_from_slice(responses);
        c.response_used.resize(responses.len(), false);
        c.failures.extend_from_slice(input.failures);
        c.failure_used.resize(input.failures.len(), false);
        let mut admission = inventory.work - runtime_work;
        let mut validation_cost = c
            .sites
            .len()
            .checked_add(contract.sites().len())
            .and_then(|n| n.checked_add(16))
            .ok_or_else(invalid)?;
        for function in contract.functions() {
            spend(&mut admission, 1)?;
            if let Some(flow) = &contract.executable.function(*function)?.temporaries {
                validation_cost = validation_cost
                    .checked_add(flow.sites.len())
                    .ok_or_else(invalid)?;
            }
        }
        for item in &c.responses {
            spend(&mut admission, validation_cost)?;
            c.applicable(contract, item.address, steps)?;
            c.response_valid(contract, *item)?;
        }
        for address in &c.failures {
            spend(&mut admission, validation_cost)?;
            c.applicable(contract, *address, steps)?;
        }
        // Fixed admitted backing; insertion sorting charges every comparison and
        // movement, avoiding an unaccounted sorting scratch allocation.
        for index in 1..c.responses.len() {
            let mut at = index;
            while at > 0 {
                spend(&mut admission, 1)?;
                if address_key(c.responses[at - 1].address) <= address_key(c.responses[at].address)
                {
                    break;
                }
                c.responses.swap(at - 1, at);
                at -= 1;
            }
        }
        for index in 1..c.failures.len() {
            let mut at = index;
            while at > 0 {
                spend(&mut admission, 1)?;
                if address_key(c.failures[at - 1]) <= address_key(c.failures[at]) {
                    break;
                }
                c.failures.swap(at - 1, at);
                at -= 1;
            }
        }
        for pair in c.responses.windows(2) {
            spend(&mut admission, 1)?;
            if pair[0].address == pair[1].address {
                return Err(invalid());
            }
        }
        for pair in c.failures.windows(2) {
            spend(&mut admission, 1)?;
            if pair[0] == pair[1] {
                return Err(invalid());
            }
        }
        Ok(c)
    }
    fn applicable(
        &self,
        contract: &contract::Contract<'_>,
        address: Address,
        steps: usize,
    ) -> Checked<()> {
        if address.activation > steps
            || address.occurrence == 0
            || address.occurrence > self.occurrence_bound
            || !self.sites.contains(&address.site)
        {
            return Err(invalid());
        }
        let valid = match address.site {
            Site::Capacity(function, _) => {
                address.phase == Phase::Reservation
                    && (address.activation != 0 || function == contract.entry())
            }
            Site::Call(..) => false,
            Site::Release(function, _) => {
                let definition = contract.executable.function(function)?;
                let cleanup = definition.cleanup.is_some()
                    || definition.temporaries.as_ref().is_some_and(|flow| {
                        flow.sites
                            .iter()
                            .any(|site| site.plan.is_some() || site.result_plan.is_some_and(|id| contract.executable.result_plans.get(id.0).is_some_and(hir::results::VariantPlan::bearing)))
                    });
                cleanup
                    && matches!(
                        address.phase,
                        Phase::BeforeAttempt | Phase::ReleaseAttempt | Phase::ReleaseComplete
                    )
                    && (address.activation != 0 || function == contract.entry())
            }
            Site::Invocation(plan) => {
                let approved = contract.executable.trusted.as_ref().ok_or_else(invalid)?;
                let invocation = approved.invocations.get(plan).ok_or_else(invalid)?;
                if address.activation == 0 && invocation.function != contract.entry() {
                    return Err(invalid());
                }
                match address.phase {
                    Phase::BeforeHandoff
                    | Phase::HandoffAccepted
                    | Phase::BeforeAttempt
                    | Phase::Attempt
                    | Phase::Response
                    | Phase::Validation
                    | Phase::Delivery => true,
                    Phase::Protection => invocation.completion.is_some(),
                    _ => false,
                }
            }
        };
        if valid {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    pub(super) fn response_valid(
        &self,
        contract: &contract::Contract<'_>,
        item: Item,
    ) -> Checked<()> {
        let approved = contract.executable.trusted.as_ref().ok_or_else(invalid)?;
        match (item.address.site, item.address.phase, item.response) {
            (Site::Release(..), Phase::ReleaseAttempt, Response::Release(birth, _)) => {
                let invocation = approved.invocations.get(birth.plan).ok_or_else(invalid)?;
                let operation = approved
                    .catalog
                    .operation(invocation.operation)
                    .ok_or_else(invalid)?;
                if operation.binding != crate::trusted::Binding::SensorOpenV1
                    || birth.activation==0 && invocation.function!=contract.entry()
                    || birth.occurrence == 0
                    || birth.activation >= self.history
                    || birth.occurrence > self.occurrence_bound
                {
                    return Err(invalid());
                }
            }
            (Site::Invocation(plan), Phase::Response, response) => {
                let invocation = approved.invocations.get(plan).ok_or_else(invalid)?;
                let operation = approved
                    .catalog
                    .operation(invocation.operation)
                    .ok_or_else(invalid)?;
                let valid = match (operation.binding, response) {
                    (crate::trusted::Binding::SensorOpenV1, Response::Open(ty)) => {
                        ty == approved.catalog.sensor
                    }
                    (crate::trusted::Binding::SensorReadV1, Response::Value(ty, value, 0)) => {
                        let range = contract
                            .executable
                            .program
                            .ranges()
                            .get(ty.0)
                            .ok_or_else(invalid)?;
                        ty == approved.catalog.reading && range.min <= value && value <= range.max
                    }
                    (crate::trusted::Binding::SensorFinishV1, Response::Value(ty, 1, 0)) => {
                        ty == approved.catalog.finish_receipt
                    }
                    (binding, Response::Error(ty, code, prefix)) => {
                        let completion = binding == crate::trusted::Binding::SensorFinishV1;
                        ty == approved.catalog.error
                            && (0..8).contains(&code)
                            && operation.errors & (1u8 << code) != 0
                            && (prefix == 0 || completion && prefix == 1)
                    }
                    _ => false,
                };
                if !valid {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
    fn tick(&mut self, amount: usize) -> Checked<()> {
        if spend(&mut self.work, amount).is_err() {
            self.rejected.set(true);
            return Err(invalid());
        }
        Ok(())
    }
    fn index(&mut self, site: Site) -> Checked<usize> {
        self.tick(self.sites.len())?;
        self.sites
            .iter()
            .position(|candidate| *candidate == site)
            .ok_or_else(invalid)
    }
    fn enter_site(&mut self, site: Site) -> Checked<Entered> {
        let index = self.index(site)?;
        let at = self
            .current
            .checked_mul(self.sites.len())
            .and_then(|n| n.checked_add(index))
            .ok_or_else(invalid)?;
        let occurrence = self.counters.get_mut(at).ok_or_else(invalid)?;
        *occurrence = occurrence.checked_add(1).ok_or_else(invalid)?;
        Ok(Entered {
            site,
            activation: *self.active.get(self.current).ok_or_else(invalid)?,
            occurrence: *occurrence,
        })
    }
    pub(super) fn position(&mut self, depth: usize) -> Checked<()> {
        self.tick(1)?;
        if depth >= self.active.len() {
            self.rejected.set(true);
            return Err(invalid());
        }
        self.current = depth;
        Ok(())
    }
    pub(super) fn activate(
        &mut self,
        depth: usize,
        function: FunctionId,
        call_site: Span,
        entry_span: Span,
    ) -> Checked<(usize, Option<usize>, u64)> {
        if depth == 0 {
            if self.activations[0].function != function || self.root_bound {
                return Err(invalid());
            }
            self.root_bound = true;
            self.current = 0;
            return Ok((0, None, 0));
        }
        self.position(depth - 1)?;
        let caller = self.activations[self.active[self.current]].function;
        let entered = self.enter_site(Site::Call(caller, call_site))?;
        if self.activations.len() == self.history {
            self.rejected.set(true);
            return Err(invalid());
        }
        let id = self.activations.len();
        self.activations.push(Activation {
            function,
            parent: Some(entered.activation),
            call_site,
            entry_span,
            occurrence: entered.occurrence,
        });
        self.position(depth)?;
        self.active[depth] = id;
        self.tick(self.sites.len())?;
        let begin = depth * self.sites.len();
        self.counters[begin..begin + self.sites.len()].fill(0);
        Ok((id, Some(entered.activation), entered.occurrence))
    }
    pub(super) fn capacity(&mut self, origin: Span) -> Checked<Address> {
        let activation = self.activations[self.active[self.current]];
        let mut site = Site::Capacity(activation.function, origin);
        self.tick(self.sites.len())?;
        if !self.sites.contains(&site) && origin == activation.call_site {
            site = Site::Capacity(activation.function, activation.entry_span);
        }
        Ok(self.enter_site(site)?.address(Phase::Reservation))
    }
    pub(super) fn begin(&mut self, plan: usize, raw: u64) -> Checked<Address> {
        if self.protocol.is_some() {
            return Err(invalid());
        }
        self.tick(self.invocations.len())?;
        if self
            .invocations
            .iter()
            .any(|invocation| invocation.raw == raw)
        {
            self.rejected.set(true);
            return Err(invalid());
        }
        let entered = self.enter_site(Site::Invocation(plan))?;
        if self.invocations.len() == self.history {
            self.rejected.set(true);
            return Err(invalid());
        }
        self.invocations.push(Invocation {
            raw,
            birth: Birth {
                plan,
                activation: entered.activation,
                occurrence: entered.occurrence,
            },
        });
        self.protocol = Some(entered);
        Ok(entered.address(Phase::BeforeHandoff))
    }
    pub(super) fn protocol(&self, phase: Phase) -> Checked<Address> {
        Ok(self.protocol.ok_or_else(invalid)?.address(phase))
    }
    pub(super) fn end(&mut self) {
        self.protocol = None;
    }
    pub(super) fn begin_release(&mut self, origin: Span) -> Checked<Address> {
        if self.release.is_some() {
            return Err(invalid());
        }
        let function = self.activations[self.active[self.current]].function;
        let entered = self.enter_site(Site::Release(function, origin))?;
        self.release = Some(entered);
        Ok(entered.address(Phase::BeforeAttempt))
    }
    pub(super) fn release(&self, phase: Phase) -> Checked<Address> {
        Ok(self.release.ok_or_else(invalid)?.address(phase))
    }
    pub(super) fn end_release(&mut self) {
        self.release = None;
    }
    pub(super) fn birth(&mut self, raw: u64) -> Checked<Birth> {
        self.tick(self.invocations.len())?;
        self.invocations
            .iter()
            .find(|invocation| invocation.raw == raw)
            .map(|i| i.birth)
            .ok_or_else(invalid)
    }
    pub(super) fn fail(&mut self, address: Address) -> Checked<bool> {
        self.tick(self.failures.len() + 1)?;
        for (at, item) in self.failures.iter().enumerate() {
            if *item == address {
                if self.failure_used[at] {
                    self.rejected.set(true);
                    return Err(invalid());
                }
                self.failure_used[at] = true;
                self.stopped = Some(address);
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(super) fn response(
        &mut self,
        address: Address,
        consume: bool,
    ) -> Checked<Option<Response>> {
        if !self.strict {
            return Ok(None);
        }
        self.tick(self.responses.len() + 1)?;
        for (at, item) in self.responses.iter().enumerate() {
            if item.address == address {
                if self.response_used[at] {
                    self.rejected.set(true);
                    return Err(invalid());
                }
                if consume {
                    self.response_used[at] = true;
                }
                return Ok(Some(item.response));
            }
        }
        self.rejected.set(true);
        Err(invalid())
    }
    pub(super) fn stopped(&self) -> Option<Address> {
        self.stopped
    }
    pub(super) fn rejected(&self) -> bool {
        self.rejected.get()
    }
    pub(super) fn same_input(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.approval, &other.approval)
            && self.schema == other.schema
            && self.entry == other.entry
            && self.strict == other.strict
            && self.responses == other.responses
            && self.failures == other.failures
    }
}

impl Controller {
    pub(super) fn bound(
        &self,
        executable: &CheckedExecutable,
        entry: FunctionId,
        execution: ExecutionLimits,
    ) -> bool {
        Rc::ptr_eq(&self.approval, &executable.approval)
            && self.schema == executable.approval.schema
            && self.entry == entry
            && self.execution == execution
    }
    pub(super) fn has_site(&self, site: Site) -> bool {
        self.sites.contains(&site)
    }
    pub(super) fn site_work(&self) -> usize {
        self.sites.len()
    }
    pub(super) fn activation(&self, id: usize) -> Option<(FunctionId, Option<usize>, Span, u64)> {
        self.activations
            .get(id)
            .map(|a| (a.function, a.parent, a.call_site, a.occurrence))
    }
    pub(super) fn activation_count(&self) -> usize {
        if self.root_bound {
            self.activations.len()
        } else {
            0
        }
    }
    pub(super) fn used_responses(&self) -> usize {
        self.response_used.iter().filter(|used| **used).count()
    }
    pub(super) fn birth_for_raw(&self, raw: u64) -> Option<Birth> {
        self.invocations
            .iter()
            .find(|invocation| invocation.raw == raw)
            .map(|invocation| invocation.birth)
    }
    pub(super) fn raw_for_birth(&self, birth: Birth) -> Option<u64> {
        self.invocations
            .iter()
            .find(|invocation| invocation.birth == birth)
            .map(|invocation| invocation.raw)
    }
    pub(super) fn history_work(&self) -> usize {
        self.invocations.len()
    }
    pub(super) fn consumed(&self, address: Address) -> bool {
        self.responses
            .iter()
            .zip(&self.response_used)
            .any(|(item, used)| item.address == address && *used)
    }
    pub(super) fn modeled(&self, address: Address) -> bool {
        self.stopped == Some(address)
            && (self
                .failures
                .iter()
                .zip(&self.failure_used)
                .any(|(item, used)| *item == address && *used)
                || self
                    .responses
                    .iter()
                    .zip(&self.response_used)
                    .any(|(item, used)| {
                        item.address == address
                            && *used
                            && matches!(item.response, Response::Release(_, false))
                    }))
    }
    pub(super) fn input_work(&self) -> usize {
        self.responses.len() + self.failures.len() + 1
    }
    fn reject(&self) {
        self.rejected.set(true);
    }
    fn response_host(&mut self, address: Address) {
        self.stopped = Some(address);
    }
}

impl Machine<'_> {
    fn control_record(&self, record: Record) {
        if let Some(capture) = self.observation.borrow_mut().as_mut() {
            capture.push(record);
        }
    }
    fn control_error(&self) -> ExecutionError {
        if let Some(controller) = self.control.borrow().as_ref() {
            controller.reject();
        }
        self.failure(ErrorKind::Integrity(
            "controlled-input reached-event contract",
        ))
    }
    pub(in crate::execution::machine) fn control_check_entry(
        &self,
        entry: FunctionId,
    ) -> Checked<()> {
        if self
            .control
            .borrow()
            .as_ref()
            .is_some_and(|c| !c.bound(self.executable, entry, self.limits))
        {
            return Err(self.control_error());
        }
        Ok(())
    }
    pub(in crate::execution::machine) fn control_position(&self, frame: u64) -> Checked<()> {
        if self.control.borrow().is_none() {
            return Ok(());
        }
        let depth = self.frame_index(frame)?;
        let result = self.control.borrow_mut().as_mut().unwrap().position(depth);
        result.map_err(|_| self.control_error())
    }
    pub(in crate::execution::machine) fn control_activate(
        &self,
        raw: u64,
        function: FunctionId,
        call_site: Span,
    ) -> Checked<()> {
        if self.control.borrow().is_none() {
            return Ok(());
        }
        let depth = self
            .frames
            .len()
            .checked_sub(1)
            .ok_or_else(|| self.control_error())?;
        let entry_span = self.executable.program.function(function).span();
        let result = self
            .control
            .borrow_mut()
            .as_mut()
            .unwrap()
            .activate(depth, function, call_site, entry_span);
        let (id, parent, occurrence) = result.map_err(|_| self.control_error())?;
        self.control_record(Record::LogicalActivation(
            id, raw, function, parent, call_site, occurrence,
        ));
        Ok(())
    }
    fn control_test_host(&self, address: Address) -> Checked<bool> {
        self.control_record(Record::Control(address, 0));
        let result = self.control.borrow_mut().as_mut().unwrap().fail(address);
        let host = result.map_err(|_| self.control_error())?;
        if host {
            self.control_record(Record::Control(address, 2));
        }
        Ok(host)
    }
    fn control_cut(&self, address: Address) -> Checked<()> {
        if self.control_test_host(address)? {
            return Err(self.failure(ErrorKind::Host("modeled abstract Host")));
        }
        Ok(())
    }
    pub(in crate::execution::machine) fn control_capacity(&self) -> Checked<()> {
        if self.control.borrow().is_none() {
            return Ok(());
        }
        let result = self
            .control
            .borrow_mut()
            .as_mut()
            .unwrap()
            .capacity(self.origin.span);
        self.control_cut(result.map_err(|_| self.control_error())?)
    }
    pub(in crate::execution::machine) fn control_begin(
        &self,
        plan: usize,
        raw: u64,
    ) -> Checked<()> {
        if self.control.borrow().is_none() {
            return Ok(());
        }
        let result = self.control.borrow_mut().as_mut().unwrap().begin(plan, raw);
        self.control_cut(result.map_err(|_| self.control_error())?)
    }
    pub(in crate::execution::machine) fn control_phase(&self, phase: Phase) -> Checked<()> {
        if self.control.borrow().is_none() {
            return Ok(());
        }
        let address = self
            .control
            .borrow()
            .as_ref()
            .unwrap()
            .protocol(phase)
            .map_err(|_| self.control_error())?;
        self.control_cut(address)
    }
    pub(in crate::execution::machine) fn control_response(
        &self,
        consume: bool,
    ) -> Checked<Option<Response>> {
        if self.control.borrow().is_none() {
            return Ok(None);
        }
        let address = self
            .control
            .borrow()
            .as_ref()
            .unwrap()
            .protocol(Phase::Response)
            .map_err(|_| self.control_error())?;
        let result = self
            .control
            .borrow_mut()
            .as_mut()
            .unwrap()
            .response(address, consume);
        let response = result.map_err(|_| self.control_error())?;
        if consume && response.is_some() {
            self.control_record(Record::Control(address, 1));
        }
        Ok(response)
    }
    pub(in crate::execution::machine) fn control_committed_response(&self) -> Checked<()> {
        if self.control.borrow().is_none() {
            return Ok(());
        }
        let address = self
            .control
            .borrow()
            .as_ref()
            .unwrap()
            .protocol(Phase::Response)
            .map_err(|_| self.control_error())?;
        let state = self.trusted.as_ref().ok_or_else(|| self.control_error())?;
        let response = state
            .controlled_response(self.executable.trusted.as_ref().unwrap().catalog.sensor)
            .ok_or_else(|| self.control_error())?;
        self.control_record(Record::ControlledResponse(address, response));
        self.control_cut(address)
    }
    pub(in crate::execution::machine) fn control_end(&self) {
        if let Some(controller) = self.control.borrow_mut().as_mut() {
            controller.end();
        }
    }
    pub(in crate::execution::machine) fn control_begin_release(&self) -> Checked<()> {
        if self.control.borrow().is_none() {
            return Ok(());
        }
        let result = self
            .control
            .borrow_mut()
            .as_mut()
            .unwrap()
            .begin_release(self.origin.span);
        let address = result.map_err(|_| self.control_error())?;
        let (kind, raw) = match self
            .resources
            .as_ref()
            .and_then(|state| state.current.as_ref())
        {
            Some(Value::Leaf { identity, .. }) => (0, *identity),
            Some(Value::Resource { identity, .. }) => (1, *identity),
            _ => return Err(self.control_error()),
        };
        self.control_record(Record::Releasing(address, kind, raw));
        self.control_cut(address)
    }
    pub(in crate::execution::machine) fn control_release_response(
        &self,
        raw: Option<u64>,
        consume: bool,
    ) -> Checked<Option<bool>> {
        if self.control.borrow().is_none() {
            return Ok(None);
        }
        let address = self
            .control
            .borrow()
            .as_ref()
            .unwrap()
            .release(Phase::ReleaseAttempt)
            .map_err(|_| self.control_error())?;
        let result = (|| {
            if let Some(raw) = raw {
                let mut control = self.control.borrow_mut();
                let c = control.as_mut().unwrap();
                let birth = c.birth(raw)?;
                match c.response(address, consume)? {
                    None => Ok(None),
                    Some(Response::Release(expected, success)) if expected == birth => {
                        Ok(Some((birth, success)))
                    }
                    _ => Err(invalid()),
                }
            } else {
                Ok(None)
            }
        })();
        let response = result.map_err(|_| self.control_error())?;
        if let Some((birth, success)) = response {
            if consume {
                self.control_record(Record::Control(address, 1));
                self.control_record(Record::ControlledResponse(
                    address,
                    Response::Release(birth, success),
                ));
                if !success {
                    self.control
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .response_host(address);
                    self.control_record(Record::Control(address, 2));
                }
            }
            Ok(Some(success))
        } else {
            Ok(None)
        }
    }
    pub(in crate::execution::machine) fn control_release_attempt(&self) -> Checked<bool> {
        if self.control.borrow().is_none() {
            return Ok(false);
        }
        let address = self
            .control
            .borrow()
            .as_ref()
            .unwrap()
            .release(Phase::ReleaseAttempt)
            .map_err(|_| self.control_error())?;
        self.control_test_host(address)
    }
    pub(in crate::execution::machine) fn control_release_failed(&self) {
        if let Some(controller) = self.control.borrow_mut().as_mut() {
            if let Ok(address) = controller.release(Phase::ReleaseFailure) {
                self.control_record(Record::Control(address, 0));
            }
            controller.end_release();
        }
    }
    pub(in crate::execution::machine) fn control_release_complete(&self) -> Checked<()> {
        if self.control.borrow().is_none() {
            return Ok(());
        }
        let address = self
            .control
            .borrow()
            .as_ref()
            .unwrap()
            .release(Phase::ReleaseComplete)
            .map_err(|_| self.control_error())?;
        self.control_cut(address)?;
        self.control.borrow_mut().as_mut().unwrap().end_release();
        Ok(())
    }
}
