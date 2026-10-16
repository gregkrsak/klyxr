//! Source-derived temporary authority. These IDs are never borrowable HIR places.
//! Ordering here describes evaluation sites; successful creation order is dynamic.
use super::*;
use hir::resources::PlanError;
use hir::{ExprKind, ExprType, FieldId, ParameterId, RecordId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RegionId(pub(crate) usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SiteId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Destination {
    Local(LocalId),
    Assign(LocalId),
    Field(LocalId, RecordId, FieldId),
    ReferenceField(Place, RecordId, FieldId),
    Dereference(Place),
    Replacement(LocalId),
    Return,
    Condition,
    NoValue(FunctionId),
    Parameter(ParameterId),
    Pending(RecordId, FieldId),
    Operand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Region {
    pub(crate) id: RegionId,
    pub(crate) parent: Option<RegionId>,
    pub(crate) evaluation: Option<usize>,
    pub(crate) retirement: Origin,
    pub(crate) publication: Origin,
    pub(crate) protect_result: bool,
    pub(crate) retire_before_publication: bool,
    pub(crate) destination: Destination,
    pub(crate) origin: Origin,
    pub(crate) roots: Vec<SiteId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site {
    pub(crate) id: SiteId,
    pub(crate) region: RegionId,
    pub(crate) ty: ExprType,
    pub(crate) plan: Option<RecordId>,
    pub(crate) destination: Destination,
    pub(crate) origin: Origin,
    pub(crate) transfer: Origin,
    pub(crate) cleanup: Origin,
    pub(crate) children: Vec<SiteId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Point {
    pub(crate) block: BasicBlockId,
    pub(crate) statement: usize,
    pub(crate) region: RegionId,
    pub(crate) site: Option<SiteId>,
    pub(crate) begin: bool,
    pub(crate) complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Flow {
    pub(crate) evaluations: usize,
    pub(crate) points: Vec<Point>,
    pub(crate) regions: Vec<Region>,
    pub(crate) sites: Vec<Site>,
}

impl Flow {
    pub(crate) fn lower(
        p: &hir::Program,
        body: &[ValueStatement],
        plans: &[Option<hir::resources::CleanupPlan>],
    ) -> Result<Option<Self>, PlanError> {
        if plans.iter().all(Option::is_none) {
            return Ok(None);
        }
        let mut flow = Self::default();
        flow.body(p, body, plans)?;
        // At most one MIR point per retained expression site: initializer
        // conditions and leaves replace, rather than duplicate, the root.
        flow.points
            .try_reserve_exact(flow.sites.len())
            .map_err(|_| PlanError::Host("temporary point allocation"))?;
        Ok((!flow.regions.is_empty()).then_some(flow))
    }

    fn bearing(e: &TypedExpr, plans: &[Option<hir::resources::CleanupPlan>]) -> bool {
        if matches!(e.ty, ExprType::Record(id) if plans.get(id.0).is_some_and(Option::is_some)) {
            return true;
        }
        match &e.kind {
            ExprKind::Call { arguments, .. } => arguments.iter().any(|e| Self::bearing(e, plans)),
            ExprKind::RecordConstruct { fields, .. } => {
                fields.iter().any(|f| Self::bearing(&f.value, plans))
            }
            ExprKind::IfValue {
                condition,
                then_value,
                else_value,
            } => [condition, then_value, else_value]
                .iter()
                .any(|e| Self::bearing(e, plans)),
            ExprKind::Unary { operand, .. } => Self::bearing(operand, plans),
            ExprKind::Binary { left, right, .. } => {
                Self::bearing(left, plans) || Self::bearing(right, plans)
            }
            _ => false,
        }
    }

    fn region(
        &mut self,
        parent: Option<RegionId>,
        destination: Destination,
        span: Span,
    ) -> Result<RegionId, PlanError> {
        let id = RegionId(self.regions.len());
        self.regions
            .try_reserve_exact(1)
            .map_err(|_| PlanError::Host("temporary region allocation"))?;
        self.regions.push(Region {
            id,
            parent,
            evaluation: None,
            retirement: Origin {
                span,
                kind: OriginKind::TemporaryRetirement,
            },
            publication: Origin {
                span,
                kind: OriginKind::DestinationPublication,
            },
            protect_result: !matches!(
                destination,
                Destination::NoValue(_) | Destination::Pending(_, _)
            ),
            retire_before_publication: !matches!(destination, Destination::Pending(_, _)),
            destination,
            origin: Origin {
                span,
                kind: OriginKind::TemporaryRegion,
            },
            roots: Vec::new(),
        });
        Ok(id)
    }

    fn expression(
        &mut self,
        p: &hir::Program,
        e: &TypedExpr,
        region: RegionId,
        destination: Destination,
        plans: &[Option<hir::resources::CleanupPlan>],
    ) -> Result<SiteId, PlanError> {
        let id = SiteId(self.sites.len());
        let plan = match e.ty {
            ExprType::Record(r) if plans.get(r.0).is_some_and(Option::is_some) => Some(r),
            _ => None,
        };
        self.sites
            .try_reserve_exact(1)
            .map_err(|_| PlanError::Host("temporary site allocation"))?;
        self.sites.push(Site {
            id,
            region,
            ty: e.ty,
            plan,
            destination,
            origin: Origin {
                span: e.span,
                kind: OriginKind::TemporaryCreation,
            },
            transfer: Origin {
                span: e.span,
                kind: OriginKind::TemporaryTransfer,
            },
            cleanup: Origin {
                span: e.span,
                kind: OriginKind::TemporaryCleanup,
            },
            children: Vec::new(),
        });
        let mut children = Vec::new();
        let count = match &e.kind {
            ExprKind::Call { arguments, .. } => arguments.len(),
            ExprKind::RecordConstruct { fields, .. } => fields.len(),
            ExprKind::IfValue { .. } => 3,
            ExprKind::Unary { .. } => 1,
            ExprKind::Binary { .. } => 2,
            _ => 0,
        };
        children
            .try_reserve_exact(count)
            .map_err(|_| PlanError::Host("temporary child allocation"))?;
        match &e.kind {
            ExprKind::Call {
                function,
                arguments,
            } => {
                let callee = p
                    .function(*function)
                    .as_ordinary()
                    .expect("checked ordinary call");
                for (e, parameter) in arguments.iter().zip(&callee.parameters) {
                    children.push(self.expression(
                        p,
                        e,
                        region,
                        Destination::Parameter(*parameter),
                        plans,
                    )?);
                }
            }
            ExprKind::RecordConstruct { record, fields } => {
                for field in fields {
                    let destination = Destination::Pending(*record, field.field);
                    let child = self.region(Some(region), destination, field.span)?;
                    let root = self.expression(p, &field.value, child, destination, plans)?;
                    self.root(child, root)?;
                    children.push(root);
                }
            }
            ExprKind::IfValue {
                condition,
                then_value,
                else_value,
            } => {
                children.push(self.expression(
                    p,
                    condition,
                    region,
                    Destination::Condition,
                    plans,
                )?);
                children.push(self.expression(p, then_value, region, destination, plans)?);
                children.push(self.expression(p, else_value, region, destination, plans)?);
            }
            ExprKind::Unary { operand, .. } => {
                children.push(self.expression(p, operand, region, Destination::Operand, plans)?)
            }
            ExprKind::Binary { left, right, .. } => {
                children.push(self.expression(p, left, region, Destination::Operand, plans)?);
                children.push(self.expression(p, right, region, Destination::Operand, plans)?);
            }
            _ => {}
        }
        self.sites[id.0].children = children;
        Ok(id)
    }

    fn full(
        &mut self,
        p: &hir::Program,
        values: &[&TypedExpr],
        destination: Destination,
        span: Span,
        plans: &[Option<hir::resources::CleanupPlan>],
    ) -> Result<(), PlanError> {
        let evaluation = self.evaluations;
        self.evaluations += 1;
        if !values.iter().any(|e| Self::bearing(e, plans)) {
            return Ok(());
        }
        let region = self.region(None, destination, span)?;
        self.regions[region.0].evaluation = Some(evaluation);
        for e in values {
            let root = self.expression(p, e, region, destination, plans)?;
            self.root(region, root)?;
        }
        Ok(())
    }

    fn root(&mut self, region: RegionId, site: SiteId) -> Result<(), PlanError> {
        let roots = &mut self.regions[region.0].roots;
        roots
            .try_reserve_exact(1)
            .map_err(|_| PlanError::Host("temporary root allocation"))?;
        roots.push(site);
        Ok(())
    }

    fn body(
        &mut self,
        p: &hir::Program,
        body: &[ValueStatement],
        plans: &[Option<hir::resources::CleanupPlan>],
    ) -> Result<(), PlanError> {
        for s in body {
            match s {
                ValueStatement::Let {
                    local,
                    initializer,
                    span,
                } => self.full(p, &[initializer], Destination::Local(*local), *span, plans)?,
                ValueStatement::Assign { local, value, span } => self.full(
                    p,
                    &[value],
                    if matches!(value.ty, ExprType::Record(_)) {
                        Destination::Replacement(*local)
                    } else {
                        Destination::Assign(*local)
                    },
                    *span,
                    plans,
                )?,
                ValueStatement::CopyFieldAssign {
                    owner,
                    record,
                    field,
                    value,
                    span,
                    ..
                } => self.full(
                    p,
                    &[value],
                    Destination::Field(*owner, *record, *field),
                    *span,
                    plans,
                )?,
                ValueStatement::CopyReferenceFieldAssign {
                    reference,
                    record,
                    field,
                    value,
                    span,
                    ..
                } => self.full(
                    p,
                    &[value],
                    Destination::ReferenceField(*reference, *record, *field),
                    *span,
                    plans,
                )?,
                ValueStatement::DerefAssign {
                    reference,
                    value,
                    span,
                } => self.full(
                    p,
                    &[value],
                    Destination::Dereference(*reference),
                    *span,
                    plans,
                )?,
                ValueStatement::Return { value, span } => {
                    self.full(p, &[value], Destination::Return, *span, plans)?
                }
                ValueStatement::CallNoValue {
                    function,
                    arguments,
                    span,
                } => {
                    let evaluation = self.evaluations;
                    self.evaluations += 1;
                    if arguments.iter().any(|e| Self::bearing(e, plans)) {
                        let r = self.region(None, Destination::NoValue(*function), *span)?;
                        self.regions[r.0].evaluation = Some(evaluation);
                        let f = p
                            .function(*function)
                            .as_ordinary()
                            .expect("ordinary statement call");
                        for (e, parameter) in arguments.iter().zip(&f.parameters) {
                            let root = self.expression(
                                p,
                                e,
                                r,
                                Destination::Parameter(*parameter),
                                plans,
                            )?;
                            self.root(r, root)?;
                        }
                    }
                }
                ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    span,
                } => {
                    self.full(p, &[condition], Destination::Condition, *span, plans)?;
                    self.body(p, then_body, plans)?;
                    self.body(p, else_body, plans)?;
                }
                ValueStatement::While {
                    condition,
                    body,
                    span,
                } => {
                    self.full(p, &[condition], Destination::Condition, *span, plans)?;
                    self.body(p, body, plans)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(body: &str) -> Flow {
        let p = hir::resources::fixture_with(body);
        p.validate_record_expressions().unwrap();
        crate::ownership::check(&p).unwrap();
        let (m, _) = lower_with_origins(&p).unwrap();
        m.functions.last().unwrap().temporaries.clone().unwrap()
    }

    #[test]
    fn conditional_initializer_keeps_one_source_region_across_cfg() {
        let f = flow("fn demo(){let a=acquire();let chosen=if true{a}else{acquire()};}");
        assert_eq!(f.regions.len(), 2);
        let r = &f.regions[1];
        let root = &f.sites[r.roots[0].0];
        assert_eq!(root.children.len(), 3);
        assert!(root.children.iter().all(|id| f.sites[id.0].region == r.id));
        assert!(r.parent.is_none());
    }

    #[test]
    fn call_sites_bind_exact_parameter_destinations_in_written_order() {
        let f =
            flow("fn take(a:Leaf,b:Leaf){}fn demo(){let a=acquire();let b=acquire();take(a,b);}");
        let r = f.regions.last().unwrap();
        assert_eq!(r.roots.len(), 2);
        let destinations: Vec<_> = r.roots.iter().map(|s| f.sites[s.0].destination).collect();
        assert!(
            matches!(destinations.as_slice(), [Destination::Parameter(a), Destination::Parameter(b)] if a != b)
        );
    }

    #[test]
    fn nested_call_is_not_an_extra_caller_region() {
        let p = hir::resources::fixture_with("fn pass(a:Leaf)->Leaf{return a;}fn take(a:Leaf){}fn demo(){let a=acquire();take(pass(a));}");
        // The frozen owned-argument boundary now admits this original witness.
        p.validate_record_expressions().unwrap();
        crate::ownership::check(&p).unwrap();
        p.validate_record_expressions_without_resources().unwrap();
        let body = &p.functions.last().unwrap().as_ordinary().unwrap().body;
        let f = Flow::lower(&p, body, &p.resources.plans(&p).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(f.regions.len(), 2);
        let r = f.regions.last().unwrap();
        let root = &f.sites[r.roots[0].0];
        assert_eq!(root.children.len(), 1);
        assert_eq!(f.sites[root.children[0].0].region, r.id);
    }

    #[test]
    fn unused_catalog_does_not_create_resource_free_regions() {
        let p = hir::resources::fixture_with("fn demo(){let flag=true;}");
        let (m, _) = lower_with_origins(&p).unwrap();
        assert!(m.functions.last().unwrap().temporaries.is_none());
    }
}
