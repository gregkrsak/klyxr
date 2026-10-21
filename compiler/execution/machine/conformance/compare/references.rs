//! Stored handles retain canonical identity after last use. These checks resolve
//! immutable approved initializer/call lineage, not loan activity or future uses.
//! The existing ownership checker remains the sole lifetime authority.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReferenceValue {
    pub kind: BorrowKind,
    pub referent: ReferentType,
    pub frame: u64,
    pub place: Place,
}

// History of actual reference-value initialization/Move, not a live-loan map.
// The birth snapshot distinguishes repeated execution of the same MIR borrow.
#[derive(Clone, Copy)]
pub(super) struct History {
    frame: u64,
    place: Place,
    state: u8,
    value: Option<ReferenceValue>,
    root: Option<(u64, usize, usize)>,
}

impl<'a> Validator<'a, '_> {
    fn history_index(&mut self, frame: u64, place: Place) -> Valid<Option<usize>> {
        for index in 0..self.scratch.references.len() {
            self.work.spend()?;
            let history = self.scratch.references[index];
            if history.frame == frame && history.place == place {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
    pub(super) fn reference_binding(
        &mut self,
        frame: u64,
        place: Place,
        value: ReferenceValue,
        canonical: (u64, usize),
    ) -> Valid<(u64, usize, usize)> {
        if let Some(index) = self.history_index(frame, place)? {
            let prior = self.scratch.references[index];
            if prior.state == 1 {
                require(prior.value == Some(value))?;
                let root = prior.root.ok_or(Incomplete::InvalidState)?;
                require((root.0, root.1) == canonical)?;
                return Ok(root);
            }
            // A transferred handle cannot resurrect. Reinitialization must have
            // reached its observed Uninitialized state before a new binding.
            protection(Protection::Resurrection, prior.state == 0)?;
        }
        let (at, expression) = self.reference_initializer(frame, place)?;
        let source = match expression.kind {
            ExprKind::Local(local) => Some(Place::Local(local)),
            ExprKind::Parameter(parameter) => Some(Place::Parameter(parameter)),
            ExprKind::Borrow { .. } => None,
            _ => return Err(Incomplete::InvalidState),
        };
        if let Some(source) = source {
            let index = self
                .history_index(at, source)?
                .ok_or(Incomplete::InvalidState)?;
            let history = self.scratch.references[index];
            require(history.value == Some(value))?;
            let root = history.root.ok_or(Incomplete::InvalidState)?;
            require((root.0, root.1) == canonical)?;
            Ok(root)
        } else {
            Ok((canonical.0, canonical.1, self.stamp))
        }
    }
    pub(super) fn reference_history(&mut self, frame: u64, place: Place, state: u8) -> Valid {
        let slot = *self.scratch.slots.last().ok_or(Incomplete::InvalidState)?;
        if let Some(index) = self.history_index(frame, place)? {
            let history = &mut self.scratch.references[index];
            history.state = state;
            if state == 0 || state == 1 {
                history.value = slot.reference;
                history.root = slot.reference_root;
            }
        } else {
            if self.scratch.references.len() == self.scratch.cap {
                return Err(Incomplete::Records);
            }
            self.scratch.references.push(History {
                frame,
                place,
                state,
                value: slot.reference,
                root: slot.reference_root,
            });
        }
        Ok(())
    }

    fn reference_initializer(&mut self, frame: u64, place: Place) -> Valid<(u64, &'a TypedExpr)> {
        let function = self.function(frame)?;
        self.place(frame, place)?;
        let contract = self.contract;
        let definition = contract
            .executable
            .function(function)
            .map_err(|_| Incomplete::InvalidState)?;
        match place {
            Place::Local(local) => {
                let mut found = None;
                for (_, block) in definition.blocks() {
                    for statement in block.statements() {
                        self.work.spend()?;
                        if let Statement::Let {
                            local: id,
                            initializer,
                            ..
                        } = statement
                        {
                            if *id == local {
                                require(found.is_none())?;
                                found = Some(initializer);
                            }
                        }
                    }
                }
                Ok((frame, found.ok_or(Incomplete::InvalidState)?))
            }
            Place::Parameter(parameter) => {
                let node = self.node(key(Kind::Frame, frame))?;
                let parent = node.parent.ok_or(Incomplete::InvalidState)?.1;
                let caller = self.function(parent)?;
                let ordinary = contract
                    .executable
                    .program
                    .function(function)
                    .as_ordinary()
                    .ok_or(Incomplete::InvalidState)?;
                let mut position = None;
                for (index, id) in ordinary.parameters.iter().enumerate() {
                    self.work.spend()?;
                    if *id == parameter {
                        position = Some(index);
                    }
                }
                let position = position.ok_or(Incomplete::InvalidState)?;
                let mut argument = None;
                for (expression, site) in contract.expressions() {
                    self.work.spend()?;
                    if contract.sites()[*site].function == caller
                        && Some(expression.span) == node.origin
                    {
                        if let ExprKind::Call {
                            function: target,
                            arguments,
                        } = &expression.kind
                        {
                            if *target == function {
                                require(argument.is_none())?;
                                argument = arguments.get(position);
                            }
                        }
                    }
                }
                let caller_definition = contract
                    .executable
                    .function(caller)
                    .map_err(|_| Incomplete::InvalidState)?;
                for (_, block) in caller_definition.blocks() {
                    for statement in block.statements() {
                        self.work.spend()?;
                        if let Statement::CallNoValue {
                            function: target,
                            arguments,
                            span,
                        } = statement
                        {
                            if *target == function && Some(*span) == node.origin {
                                require(argument.is_none())?;
                                argument = arguments.get(position);
                            }
                        }
                    }
                }
                Ok((parent, argument.ok_or(Incomplete::InvalidState)?))
            }
        }
    }
    pub(super) fn reference_origin(
        &mut self,
        mut frame: u64,
        mut place: Place,
    ) -> Valid<(ReferenceValue, (u64, usize))> {
        // A checked finite lineage walk, not recursive lifetime reconstruction.
        for _ in 0..self.lineage {
            self.work.spend()?;
            let (at, expression) = self.reference_initializer(frame, place)?;
            frame = at;
            match expression.kind {
                ExprKind::Local(local) => place = Place::Local(local),
                ExprKind::Parameter(parameter) => place = Place::Parameter(parameter),
                ExprKind::Borrow { kind, place: owner } => {
                    let referent = match expression.ty {
                        ExprType::SharedRef(ty) | ExprType::MutableRef(ty) => ty,
                        _ => return Err(Incomplete::InvalidState),
                    };
                    require(self.place(frame, owner)? == referent.value_type())?;
                    for (canonical, index) in self.contract.expressions() {
                        self.work.spend()?;
                        if std::ptr::eq(*canonical, expression) {
                            return Ok((
                                ReferenceValue {
                                    kind,
                                    referent,
                                    frame,
                                    place: owner,
                                },
                                (frame, *index),
                            ));
                        }
                    }
                    return Err(Incomplete::InvalidState);
                }
                _ => return Err(Incomplete::InvalidState),
            }
        }
        Err(Incomplete::Lineage)
    }
    pub(super) fn stored_references(&mut self) -> Valid {
        for index in 0..self.scratch.slots.len() {
            self.work.spend()?;
            let slot = self.scratch.slots[index];
            let Some(reference) = slot.reference else {
                continue;
            };
            let target = self.node(key(Kind::Frame, reference.frame))?;
            require(target.declared == self.stamp && target.status == 0)?;
            let mut exists = false;
            for owner in &self.scratch.slots {
                self.work.spend()?;
                if owner.frame == reference.frame && owner.place == reference.place {
                    exists = true;
                }
            }
            // A moved owner can leave dead handle bits behind. Their canonical
            // relationship survives; their mere storage grants no live loan.
            require(exists && slot.reference_root.is_some())?;
            if reference.kind == BorrowKind::Mutable {
                for prior in &self.scratch.slots[..index] {
                    self.work.spend()?;
                    // Mutable transfers cannot duplicate the SAME provenance.
                    // Distinct borrows of one owner may have disjoint last uses.
                    if prior
                        .reference
                        .is_some_and(|r| r.kind == BorrowKind::Mutable)
                    {
                        protection(Protection::Duplicate, prior.reference_root != slot.reference_root)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn live_reference(&mut self, frame: u64, place: Place, write: bool) -> Valid {
        // The operation has an independently bound approved producer/operand;
        // retained bits alone never establish a new access transition.
        let mut value = None;
        for slot in &self.scratch.slots {
            self.work.spend()?;
            if slot.frame == frame && slot.place == place {
                require(slot.state == 1)?;
                value = slot.reference;
            }
        }
        let reference = value.ok_or(Incomplete::InvalidState)?;
        require(!write || reference.kind == BorrowKind::Mutable)?;
        self.scratch.link(Link::Borrow(
            key(Kind::Frame, reference.frame),
            reference.place,
            reference.kind,
        ))
    }
    pub(super) fn reference_expression_use(&mut self, frame: u64, ordinal: usize) -> Valid {
        for (expression, index) in self.contract.expressions() {
            self.work.spend()?;
            if *index != ordinal {
                continue;
            }
            return match expression.kind {
                ExprKind::Deref { reference }
                | ExprKind::CopyReferenceFieldRead { reference, .. } => {
                    self.live_reference(frame, reference, false)
                }
                ExprKind::Local(local)
                    if matches!(
                        expression.ty,
                        ExprType::SharedRef(_) | ExprType::MutableRef(_)
                    ) =>
                {
                    self.live_reference(frame, Place::Local(local), false)
                }
                ExprKind::Parameter(parameter)
                    if matches!(
                        expression.ty,
                        ExprType::SharedRef(_) | ExprType::MutableRef(_)
                    ) =>
                {
                    self.live_reference(frame, Place::Parameter(parameter), false)
                }
                _ => Ok(()),
            };
        }
        Err(Incomplete::InvalidState)
    }
    pub(super) fn reference_write_use(&mut self, frame: u64) -> Valid {
        let node = self.node(key(Kind::Frame, frame))?;
        let definition = self
            .contract
            .executable
            .function(self.function(frame)?)
            .map_err(|_| Incomplete::InvalidState)?;
        for (id, block) in definition.blocks() {
            self.work.spend()?;
            if id.0 == node.block {
                if let Some(
                    Statement::DerefAssign { reference, .. }
                    | Statement::CopyReferenceFieldAssign { reference, .. },
                ) = block.statements().get(node.statement)
                {
                    return self.live_reference(frame, *reference, true);
                }
                return Ok(());
            }
        }
        Err(Incomplete::InvalidState)
    }
}
