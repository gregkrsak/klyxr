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

impl<'a> Validator<'a, '_> {
    fn reference_initializer(&mut self, frame: u64, place: Place) -> Valid<(u64, &'a TypedExpr)> {
        let function = self.function(frame)?;
        self.place(frame, place)?;
        let contract = self.contract;
        let definition = contract.executable.function(function).map_err(|_| Incomplete::InvalidState)?;
        match place {
            Place::Local(local) => {
                let mut found = None;
                for (_, block) in definition.blocks() {
                    for statement in block.statements() {
                        self.work.spend()?;
                        if let Statement::Let { local: id, initializer, .. } = statement {
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
                let ordinary = contract.executable.program.function(function).as_ordinary().ok_or(Incomplete::InvalidState)?;
                let mut position = None;
                for (index, id) in ordinary.parameters.iter().enumerate() {
                    self.work.spend()?;
                    if *id == parameter { position = Some(index); }
                }
                let position = position.ok_or(Incomplete::InvalidState)?;
                let mut argument = None;
                for (expression, site) in contract.expressions() {
                    self.work.spend()?;
                    if contract.sites()[*site].function == caller && Some(expression.span) == node.origin {
                        if let ExprKind::Call { function: target, arguments } = &expression.kind {
                            if *target == function {
                                require(argument.is_none())?;
                                argument = arguments.get(position);
                            }
                        }
                    }
                }
                let caller_definition = contract.executable.function(caller).map_err(|_| Incomplete::InvalidState)?;
                for (_, block) in caller_definition.blocks() {
                    for statement in block.statements() {
                        self.work.spend()?;
                        if let Statement::CallNoValue { function: target, arguments, span } = statement {
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
    pub(super) fn reference_origin(&mut self, mut frame: u64, mut place: Place) -> Valid<(ReferenceValue, (u64, usize))> {
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
                            return Ok((ReferenceValue { kind, referent, frame, place: owner }, (frame, *index)));
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
            let Some(reference) = slot.reference else { continue; };
            let target = self.node(key(Kind::Frame, reference.frame))?;
            require(target.declared == self.stamp && target.status == 0)?;
            let mut exists = false;
            for owner in &self.scratch.slots {
                self.work.spend()?;
                if owner.frame == reference.frame && owner.place == reference.place { exists = true; }
            }
            // A moved owner can leave dead handle bits behind. Their canonical
            // relationship survives; their mere storage grants no live loan.
            require(exists && slot.reference_root.is_some())?;
            if reference.kind == BorrowKind::Mutable {
                for prior in &self.scratch.slots[..index] {
                    self.work.spend()?;
                    // Mutable transfers cannot duplicate the SAME provenance.
                    // Distinct borrows of one owner may have disjoint last uses.
                    if prior.reference.is_some_and(|r| r.kind == BorrowKind::Mutable) {
                        require(prior.reference_root != slot.reference_root)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn live_reference(&mut self, frame: u64, place: Place, write: bool) -> Valid {
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
        self.scratch.link(Link::Borrow(key(Kind::Frame, reference.frame), reference.place, reference.kind))
    }
    pub(super) fn reference_expression_use(&mut self, frame: u64, ordinal: usize) -> Valid {
        for (expression, index) in self.contract.expressions() {
            self.work.spend()?;
            if *index != ordinal { continue; }
            return match expression.kind {
                ExprKind::Deref { reference } | ExprKind::CopyReferenceFieldRead { reference, .. } => self.live_reference(frame, reference, false),
                ExprKind::Local(local) if matches!(expression.ty, ExprType::SharedRef(_) | ExprType::MutableRef(_)) => self.live_reference(frame, Place::Local(local), false),
                ExprKind::Parameter(parameter) if matches!(expression.ty, ExprType::SharedRef(_) | ExprType::MutableRef(_)) => self.live_reference(frame, Place::Parameter(parameter), false),
                _ => Ok(()),
            };
        }
        Err(Incomplete::InvalidState)
    }
    pub(super) fn reference_write_use(&mut self, frame: u64) -> Valid {
        let node = self.node(key(Kind::Frame, frame))?;
        let definition = self.contract.executable.function(self.function(frame)?).map_err(|_| Incomplete::InvalidState)?;
        for (id, block) in definition.blocks() {
            self.work.spend()?;
            if id.0 == node.block {
                if let Some(Statement::DerefAssign { reference, .. } | Statement::CopyReferenceFieldAssign { reference, .. }) = block.statements().get(node.statement) {
                    return self.live_reference(frame, *reference, true);
                }
                return Ok(());
            }
        }
        Err(Incomplete::InvalidState)
    }
}
