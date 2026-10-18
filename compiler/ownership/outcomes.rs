//! Unresolved outcome facts inside the existing ownership checker. Neither Move
//! availability nor cleanup custody is evidence that an outcome was handled.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Outcome {
    pub(super) result: hir::ResultTypeId,
    pub(super) origin: Span,
    pub(super) conditional: Option<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Witness {
    function: FunctionId,
    origin: Span,
    action: Action,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Action {
    Initialize(Place, hir::ResultTypeId),
    Transfer {
        place: Place,
        result: hir::ResultTypeId,
        destination: Transfer,
    },
    Construct {
        result: hir::ResultTypeId,
        tag: hir::ResultTag,
        payload: Span,
    },
    Match {
        result: hir::ResultTypeId,
        bindings: [(hir::ResultTag, LocalId, Span); 2],
    },
    Exit,
}
#[cfg(test)]
thread_local! {
    pub(crate) static FAIL_ALLOCATION: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(crate) fn allocation_fails() -> bool {
    FAIL_ALLOCATION.with(|c| match c.get() {
        Some(0) => true,
        Some(n) => {
            c.set(Some(n - 1));
            false
        }
        None => false,
    })
}
impl Checker<'_> {
    pub(super) fn witness(&self, origin: Span, action: Action) -> OwnershipResult {
        if let Some(trail) = &self.outcome_trail {
            let mut trail = trail.borrow_mut();
            let limit = self
                .context
                .map_or(4096usize * 4096, |c| c.outcome_limit.get());
            if trail.len() >= limit {
                if let Some(c) = self.context {
                    c.fail(crate::bounded::ResourceFailure::limit(
                        "Result outcome witness count",
                        origin,
                    ));
                }
                return Err(self.diagnostic(
                    origin,
                    "Result outcome witness count exhausted".into(),
                    "bounded immutable outcome evidence is required",
                    None,
                ));
            }
            #[cfg(test)]
            if trail.len() == trail.capacity() && allocation_fails() {
                if let Some(c) = self.context {
                    c.fail(crate::bounded::ResourceFailure::host(
                        "Result outcome witness allocation",
                        origin,
                    ));
                }
                return Err(self.diagnostic(
                    origin,
                    "Result outcome witness allocation failed".into(),
                    "no partial approval is published",
                    None,
                ));
            }
            trail.try_reserve_exact(1).map_err(|_| {
                if let Some(c) = self.context {
                    c.fail(crate::bounded::ResourceFailure::host(
                        "Result outcome witness allocation",
                        origin,
                    ));
                }
                self.diagnostic(
                    origin,
                    "Result outcome witness allocation failed".into(),
                    "no partial approval is published",
                    None,
                )
            })?;
            trail.push(Witness {
                function: self.function,
                origin,
                action,
            });
        }
        Ok(())
    }

    pub(super) fn initialize_outcome(&mut self, place: Place, ty: ValueType, origin: Span) {
        if let ValueType::Result(result) = ty {
            self.outcomes.insert(
                place,
                Outcome {
                    result,
                    origin,
                    conditional: None,
                },
            );
        }
    }

    pub(super) fn check_outcome_exit(
        &self,
        exits: impl Fn(Place) -> bool,
        span: Span,
    ) -> OwnershipResult {
        if let Some((place, outcome)) = self.outcomes.iter().find(|(p, _)| exits(**p)) {
            return Err(self.diagnostic(
                span,
                format!("{}unresolved Result outcome `{}` at structured exit", if outcome.conditional.is_some() { "conditionally " } else { "" }, self.metadata(*place).0),
                "on every successful path, exhaustively match the Result, return it as the complete exact Result, or transfer it to an exact Result receiver; cleanup and conditional Move unavailability do not acknowledge an outcome",
                Some(outcome.origin),
            ));
        }
        self.witness(span, Action::Exit)
    }

    pub(super) fn match_result(
        &mut self,
        scrutinee: &TypedExpr,
        arms: &[hir::MatchArm],
        span: Span,
        function: FunctionId,
        after: &FutureUses,
    ) -> OwnershipResult<Flow> {
        if arms.len() != 2 || arms[0].tag == arms[1].tag {
            return Err(self.diagnostic(
                span,
                "invalid Result arm coverage".into(),
                "one canonical Ok and one Error arm are required",
                None,
            ));
        }
        let ExprType::Result(result) = scrutinee.ty else {
            return Err(self.diagnostic(
                span,
                "match requires a Result".into(),
                "only the fixed two-variant Result match is admitted",
                None,
            ));
        };
        self.witness(
            span,
            Action::Match {
                result,
                bindings: [
                    (arms[0].tag, arms[0].binding, arms[0].pattern_span),
                    (arms[1].tag, arms[1].binding, arms[1].pattern_span),
                ],
            },
        )?;
        self.expression(scrutinee, Transfer::Match)?;
        self.expire();
        self.assert_no_holds();
        let mut yes = self.clone();
        let mut no = self.clone();
        for (state, arm) in [(&mut yes, &arms[0]), (&mut no, &arms[1])] {
            let declaration = self.program.locals().get(arm.binding.0).ok_or_else(|| {
                self.diagnostic(
                    span,
                    "invalid Result binding".into(),
                    "canonical arm-local binding required",
                    None,
                )
            })?;
            if declaration.function != function
                || Some(declaration.ty) != self.program.result_payload(result, arm.tag)
            {
                return Err(self.diagnostic(
                    span,
                    "invalid Result binding type".into(),
                    "the selected binding has the exact active payload type",
                    None,
                ));
            }
            let place = Place::Local(arm.binding);
            if ownership_kind(declaration.ty) == OwnershipKind::Move {
                state.states.insert(place, State::Available);
            }
            // An inner Result is a new live obligation, independently of the
            // outer wrapper's exhaustive consumption and resource properties.
            state.initialize_outcome(place, declaration.ty, arm.pattern_span);
            if let ValueType::Result(ty) = declaration.ty {
                state.witness(arm.pattern_span, Action::Initialize(place, ty))?;
            }
            if let Some(custody) = &mut state.cleanup_custody {
                custody.initialize(place, declaration.ty);
            }
        }
        let yes_flow = yes.statements_with(&arms[0].body, function, after)?;
        let no_flow = no.statements_with(&arms[1].body, function, after)?;
        self.finish_branches(
            yes,
            no,
            yes_flow,
            no_flow,
            &arms[0].body,
            &arms[1].body,
            after,
            span,
        )
    }
}
