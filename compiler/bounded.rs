//! Resource envelopes for the existing semantic pipeline. No source semantics.
use std::cell::Cell;

use crate::{ast, lexer::Span};

pub const START: Span = Span {
    line: 1,
    column: 1,
    start: 0,
    end: 0,
};

/// Version-one admission limits. Reservations are conservative peak envelopes,
/// not Klyxr execution fuel. Existing check/verify do not use this contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionLimits {
    pub source_bytes: usize,
    pub tokens: usize,
    pub identifier_bytes: usize,
    pub frontend_work: u64,
    pub traversal_depth: usize,
    pub alias_depth: usize,
    pub alias_work: u64,
    pub working_storage: usize,
    pub diagnostic_storage: usize,
    pub rendered_output: usize,
}
impl AdmissionLimits {
    pub const MAX: Self = Self {
        source_bytes: 1_048_576,
        tokens: 4096,
        identifier_bytes: 256,
        frontend_work: 1_000_000_000_000_000_000,
        traversal_depth: 32,
        alias_depth: 64,
        alias_work: 10_000_000,
        working_storage: 1_073_741_824,
        diagnostic_storage: 4_194_304,
        rendered_output: 1_048_576,
    };
    pub fn validate(self) -> Result<(), ResourceFailure> {
        let m = Self::MAX;
        for (value, max, name) in [
            (self.source_bytes, m.source_bytes, "source bytes"),
            (self.tokens, m.tokens, "tokens"),
            (
                self.identifier_bytes,
                m.identifier_bytes,
                "identifier bytes",
            ),
            (self.traversal_depth, m.traversal_depth, "traversal depth"),
            (self.alias_depth, m.alias_depth, "alias depth"),
            (self.working_storage, m.working_storage, "admission storage"),
            (
                self.diagnostic_storage,
                m.diagnostic_storage,
                "diagnostic storage",
            ),
            (self.rendered_output, m.rendered_output, "rendered output"),
        ] {
            if value == 0 || value > max {
                return Err(ResourceFailure::limit(name, START));
            }
        }
        if self.frontend_work == 0 || self.frontend_work > m.frontend_work {
            return Err(ResourceFailure::limit("frontend work", START));
        }
        if self.alias_work == 0 || self.alias_work > m.alias_work {
            return Err(ResourceFailure::limit("alias work", START));
        }
        Ok(())
    }
}
impl Default for AdmissionLimits {
    fn default() -> Self {
        Self {
            source_bytes: 65_536,
            tokens: 1024,
            identifier_bytes: 128,
            frontend_work: 100_000_000_000_000,
            traversal_depth: 24,
            alias_depth: 48,
            alias_work: 1_000_000,
            working_storage: 134_217_728,
            diagnostic_storage: 1_048_576,
            rendered_output: 65_536,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Limit,
    Host,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceFailure {
    pub kind: ResourceKind,
    pub resource: &'static str,
    pub span: Span,
}
impl ResourceFailure {
    pub fn limit(resource: &'static str, span: Span) -> Self {
        Self {
            kind: ResourceKind::Limit,
            resource,
            span,
        }
    }
    pub fn host(resource: &'static str, span: Span) -> Self {
        Self {
            kind: ResourceKind::Host,
            resource,
            span,
        }
    }
}

/// Shared only within one preparation. Checker snapshots share counters, so
/// analysis rollback cannot refund tool work or hide a resource failure.
pub(crate) struct Context {
    pub limits: AdmissionLimits,
    pub failure: Cell<Option<ResourceFailure>>,
    depth: Cell<usize>,
    alias_work: Cell<u64>,
}
impl Context {
    pub fn new(limits: AdmissionLimits) -> Self {
        Self {
            limits,
            failure: Cell::new(None),
            depth: Cell::new(0),
            alias_work: Cell::new(0),
        }
    }
    pub fn fail(&self, failure: ResourceFailure) {
        if self.failure.get().is_none() {
            self.failure.set(Some(failure));
        }
    }
    pub fn enter(&self, span: Span) -> Result<Depth<'_>, ()> {
        let Some(next) = self.depth.get().checked_add(1) else {
            self.fail(ResourceFailure::limit("parser traversal depth", span));
            return Err(());
        };
        if next > self.limits.traversal_depth {
            self.fail(ResourceFailure::limit("parser traversal depth", span));
            return Err(());
        }
        self.depth.set(next);
        Ok(Depth(self))
    }
    pub fn lineage(&self, depth: usize, span: Span) -> bool {
        if depth > self.limits.alias_depth {
            self.fail(ResourceFailure::limit("alias lineage depth", span));
            return false;
        }
        let Some(work) = self.alias_work.get().checked_add(1) else {
            self.fail(ResourceFailure::limit("alias lineage work", span));
            return false;
        };
        if work > self.limits.alias_work {
            self.fail(ResourceFailure::limit("alias lineage work", span));
            return false;
        }
        self.alias_work.set(work);
        true
    }
    /// Reserve before entering unchanged resolver/typer/checker/lowerer code.
    /// n bounds all source nodes, tables and CFG blocks up to a constant factor.
    /// Recursive visitors are protected by parser construction depth; lineage
    /// has separate actual counters. The quartic work reservation covers repeated
    /// suffix walks, table scans, tree comparisons and nested checker snapshots.
    /// Peak snapshots/scopes are bounded by depth, each with O(n) entries/names.
    pub fn reserve_frontend(&self, n: usize, source: usize) -> Result<(), ResourceFailure> {
        let work = (n as u64)
            .checked_pow(4)
            .and_then(|v| v.checked_mul(128))
            .and_then(|v| v.checked_add(source as u64));
        if work.map_or(true, |v| v > self.limits.frontend_work) {
            return Err(ResourceFailure::limit("frontend work reservation", START));
        }
        let storage = self
            .limits
            .traversal_depth
            .checked_mul(16)
            .and_then(|v| v.checked_add(32))
            .and_then(|v| v.checked_mul(n))
            .and_then(|v| v.checked_mul(self.limits.identifier_bytes + 512))
            .and_then(|v| source.checked_mul(8).and_then(|s| v.checked_add(s)));
        if storage.map_or(true, |v| v > self.limits.working_storage) {
            return Err(ResourceFailure::limit(
                "frontend storage reservation",
                START,
            ));
        }
        let diagnostics = n
            .checked_mul(self.limits.identifier_bytes + 128)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| v.checked_add(512));
        if diagnostics.map_or(true, |v| v > self.limits.diagnostic_storage) {
            return Err(ResourceFailure::limit(
                "diagnostic storage reservation",
                START,
            ));
        }
        Ok(())
    }
    pub fn expression(&self, expression: &ast::Expr) -> Result<(), ()> {
        fn visit(e: &ast::Expr, d: usize, c: &Context) -> Result<(), ()> {
            if d > c.limits.traversal_depth {
                c.fail(ResourceFailure::limit(
                    "expression construction depth",
                    e.span,
                ));
                return Err(());
            }
            match &e.kind {
                ast::ExprKind::Binary { left, right, .. } => {
                    visit(left, d + 1, c)?;
                    visit(right, d + 1, c)?;
                }
                ast::ExprKind::Unary { operand, .. } => visit(operand, d + 1, c)?,
                ast::ExprKind::Call { arguments, .. } => {
                    for e in arguments {
                        visit(e, d + 1, c)?;
                    }
                }
                ast::ExprKind::RecordConstruct { fields, .. } => {
                    for f in fields {
                        visit(&f.value, d + 1, c)?;
                    }
                }
                ast::ExprKind::IfValue {
                    condition,
                    then_value,
                    else_value,
                } => {
                    visit(condition, d + 1, c)?;
                    visit(then_value, d + 1, c)?;
                    visit(else_value, d + 1, c)?;
                }
                _ => {}
            }
            Ok(())
        }
        visit(expression, 1, self)
    }
}
pub(crate) struct Depth<'a>(&'a Context);
impl Drop for Depth<'_> {
    fn drop(&mut self) {
        self.0.depth.set(self.0.depth.get() - 1);
    }
}
