//! Explicit cleanup metadata emitted while structured lexical HIR still exists.
//! Runtime consumers use only this MIR artifact, never a HIR scope walk.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScopeId(pub(crate) usize);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scope {
    pub(crate) id: ScopeId,
    pub(crate) span: Span,
    pub(crate) parent: Option<ScopeId>,
    pub(crate) owners: Vec<LocalId>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Initialization {
    pub(crate) statement: usize,
    pub(crate) local: LocalId,
    pub(crate) scope: ScopeId,
    pub(crate) origin: Origin,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Block {
    pub(crate) enter: Option<(ScopeId, Origin)>,
    pub(crate) initialize: Vec<Initialization>,
    /// Innermost first. Return applies this only after protecting its result.
    pub(crate) exit: Vec<ScopeId>,
    pub(crate) exit_origin: Option<Origin>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Flow {
    pub(crate) scopes: Vec<Scope>,
    pub(crate) blocks: Vec<Block>,
    pub(crate) parameters: Vec<hir::ParameterId>,
    pub(crate) protected_return: bool,
}
impl Flow {
    pub(crate) fn validate(&self, function: &MirFunction) -> Result<(), &'static str> {
        if self.blocks.len() != function.blocks.len() {
            return Err("cleanup block correspondence");
        }
        let mut entered = Vec::new();
        entered
            .try_reserve_exact(self.scopes.len())
            .map_err(|_| "MIR validation allocation failed")?;
        entered.resize(self.scopes.len(), false);
        for (i, s) in self.scopes.iter().enumerate() {
            if s.id != ScopeId(i)
                || s.parent.is_some_and(|p| p.0 >= i)
                || s.span.line == 0
                || s.span.column == 0
                || s.span.start > s.span.end
            {
                return Err("invalid cleanup scope identity or parent");
            }
            for (j, owner) in s.owners.iter().enumerate() {
                if s.owners[..j].contains(owner) {
                    return Err("duplicate lexical owner declaration");
                }
            }
        }
        for (i, b) in self.blocks.iter().enumerate() {
            if let Some((scope, origin)) = b.enter {
                let was = entered.get_mut(scope.0).ok_or("foreign scope activation")?;
                if *was
                    || origin.kind != OriginKind::ScopeEntry
                    || self.scopes[scope.0].span != origin.span
                {
                    return Err("duplicate scope entry or invalid origin");
                }
                *was = true;
            }
            for (j, init) in b.initialize.iter().enumerate() {
                let scope = self
                    .scopes
                    .get(init.scope.0)
                    .ok_or("foreign initialization scope")?;
                if !scope.owners.contains(&init.local)
                    || init.origin.kind != OriginKind::OwnerInitialization
                    || b.initialize[..j]
                        .iter()
                        .any(|p| p.statement == init.statement)
                {
                    return Err("invalid successful owner initialization");
                }
                if !matches!(function.blocks[i].statements.get(init.statement), Some(Statement::Let {local, span,..}) if *local == init.local && *span == init.origin.span)
                {
                    return Err("initialization does not match MIR owner");
                }
            }
            for (j, scope) in b.exit.iter().enumerate() {
                let s = self.scopes.get(scope.0).ok_or("foreign exit scope")?;
                if b.exit[..j].contains(scope) {
                    return Err("duplicate scope cleanup");
                }
                if let Some(next) = b.exit.get(j + 1) {
                    if s.parent != Some(*next) {
                        return Err("cleanup exit order is not inner-to-outer");
                    }
                }
            }
            if !b.exit.is_empty()
                && b.exit_origin
                    .map_or(true, |o| o.kind != OriginKind::ScopeExit)
            {
                return Err("cleanup exit origin missing");
            }
        }
        if entered.contains(&false) {
            return Err("missing lexical scope activation");
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> MirProgram {
        let p = hir::resources::fixture();
        p.validate_record_expressions().unwrap();
        crate::ownership::check(&p).unwrap();
        lower(&p)
    }
    #[test]
    fn lexical_scope_activation_and_parameter_cleanup_survive_lowering() {
        let m = fixture();
        let f = &m.functions()[1];
        let flow = f.cleanup.as_ref().unwrap();
        assert_eq!(flow.scopes.len(), 1);
        assert_eq!(flow.blocks[0].exit, vec![ScopeId(0)]);
        assert_eq!(flow.parameters.len(), 1);
        f.validate().unwrap();
    }
    #[test]
    fn duplicate_cleanup_rejects_even_without_live_slots() {
        let mut m = fixture();
        let f = &mut m.functions[1];
        f.cleanup.as_mut().unwrap().blocks[0].exit.push(ScopeId(0));
        assert!(f.validate().is_err());
    }
    #[test]
    fn missing_activation_is_malformed() {
        let mut m = fixture();
        let f = &mut m.functions[1];
        f.cleanup.as_mut().unwrap().blocks[0].enter = None;
        assert!(f.validate().is_err());
    }
    #[test]
    fn synthesized_exit_requires_cleanup_origin() {
        let mut m = fixture();
        let f = &mut m.functions[1];
        f.cleanup.as_mut().unwrap().blocks[0].exit_origin = None;
        assert!(f.validate().is_err());
    }
}
