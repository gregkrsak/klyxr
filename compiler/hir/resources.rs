//! Canonical private trusted-resource registration and shared structural cleanup.
//! A leaf uses the same nominal RecordId space as owned records. Only this
//! compiler-owned catalog can distinguish a trusted leaf from an inert record;
//! no spelling, source declaration or public constructor registers authority.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResourceId(pub(crate) usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OperationId(pub(crate) usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Constructed only by private synthetic registration, never by source.
pub(crate) enum OperationKind {
    Acquire(ResourceId),
    Release(ResourceId),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Operation {
    pub(crate) id: OperationId,
    pub(crate) function: FunctionId,
    pub(crate) kind: OperationKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Leaf {
    pub(crate) id: ResourceId,
    pub(crate) record: RecordId,
    pub(crate) acquire: OperationId,
    pub(crate) release: OperationId,
}
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Catalog {
    pub(crate) leaves: Vec<Leaf>,
    pub(crate) operations: Vec<Operation>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CleanupPlan {
    Release(ResourceId, OperationId),
    /// Reverse canonical declaration order. Children reference shared plans;
    /// nested records never expand into repeated leaf paths.
    Fields(Vec<(FieldId, RecordId)>),
}
impl Catalog {
    pub(crate) fn is_leaf(&self, record: RecordId) -> bool {
        self.leaves.iter().any(|l| l.record == record)
    }
    pub(crate) fn field_type_valid(&self, p: &Program, ty: ValueType) -> bool {
        match ty {
            ValueType::Range(id) => p.ranges.get(id.0).is_some_and(|r| r.id == id),
            ValueType::Record(id) if !self.leaves.is_empty() => {
                p.records.get(id.0).is_some_and(|r| r.id == id)
            }
            // The synthetic boundary admits owned range/record fields only.
            // Reference fields and wider source schemas remain deferred.
            _ => false,
        }
    }
    pub(crate) fn validate(&self, p: &Program) -> Result<(), &'static str> {
        if self.leaves.is_empty() && self.operations.is_empty() {
            return Ok(());
        }
        if self.leaves.len() > 4096 || self.operations.len() > 8192 {
            return Err("trusted resource catalog exceeds bounded capacity");
        }
        for (i, leaf) in self.leaves.iter().enumerate() {
            if leaf.id != ResourceId(i)
                || self.leaves[..i].iter().any(|l| l.record == leaf.record)
                || p.records
                    .get(leaf.record.0)
                    .map_or(true, |r| r.id != leaf.record || !r.fields.is_empty())
            {
                return Err("invalid or duplicate canonical resource leaf");
            }
            for (id, expected) in [
                (leaf.acquire, OperationKind::Acquire(leaf.id)),
                (leaf.release, OperationKind::Release(leaf.id)),
            ] {
                if self
                    .operations
                    .get(id.0)
                    .map_or(true, |o| o.id != id || o.kind != expected)
                {
                    return Err("trusted acquisition/release pairing mismatch");
                }
            }
        }
        for (i, op) in self.operations.iter().enumerate() {
            if op.id != OperationId(i)
                || self.operations[..i]
                    .iter()
                    .any(|o| o.function == op.function)
            {
                return Err("invalid or duplicate trusted operation identity");
            }
            let (resource, acquire) = match op.kind {
                OperationKind::Acquire(id) => (id, true),
                OperationKind::Release(id) => (id, false),
            };
            let leaf = self
                .leaves
                .get(resource.0)
                .filter(|l| l.id == resource)
                .ok_or("foreign trusted resource identity")?;
            if (acquire && leaf.acquire != op.id) || (!acquire && leaf.release != op.id) {
                return Err("orphan trusted operation");
            }
            let f = p
                .functions
                .get(op.function.0)
                .filter(|f| f.id() == op.function)
                .and_then(Function::as_ordinary)
                .ok_or("trusted operation is not canonical ordinary function")?;
            if acquire {
                if !matches!(f.body.as_slice(), [ValueStatement::Return { value: TypedExpr { kind: ExprKind::RecordConstruct { record, fields }, ty: ExprType::Record(ty), .. }, .. }] if *record == leaf.record && *ty == leaf.record && fields.is_empty())
                {
                    return Err("trusted acquisition must have the canonical private stub");
                }
                if !f.parameters.is_empty() || f.return_type != Some(ValueType::Record(leaf.record))
                {
                    return Err("trusted acquisition signature mismatch");
                }
            } else {
                if !f.body.is_empty() {
                    return Err("trusted release cannot dispatch user code");
                }
                if f.return_type.is_some() || f.parameters.len() != 1 {
                    return Err("trusted release signature mismatch");
                }
                let a = p
                    .parameters
                    .get(f.parameters[0].0)
                    .filter(|a| a.id == f.parameters[0] && a.function == f.id)
                    .ok_or("foreign trusted release parameter")?;
                if a.ty != ParameterType::Value(ValueType::Record(leaf.record)) {
                    return Err("trusted release must consume the exact owned leaf");
                }
            }
        }
        // Check the graph even for inert schemas; cycles are never permitted.
        let plans = self.plans(p)?;
        self.temporaries(p, &plans)
    }
    fn temporaries(&self, p: &Program, plans: &[Option<CleanupPlan>]) -> Result<(), &'static str> {
        fn bearing(e: &TypedExpr, plans: &[Option<CleanupPlan>]) -> bool {
            matches!(e.ty,ExprType::Record(id) if plans.get(id.0).is_some_and(Option::is_some))
        }
        fn expression(
            e: &TypedExpr,
            plans: &[Option<CleanupPlan>],
            boundary: bool,
        ) -> Result<(), &'static str> {
            if bearing(e, plans)
                && !boundary
                && !matches!(e.kind, ExprKind::Local(_) | ExprKind::Parameter(_))
            {
                return Err(
                    "resource-bearing expression temporary has no admitted lifetime boundary",
                );
            }
            match &e.kind {
                ExprKind::RecordConstruct { fields, .. } => {
                    for f in fields {
                        expression(&f.value, plans, true)?;
                    }
                }
                ExprKind::Call { arguments, .. } => {
                    for a in arguments {
                        expression(a, plans, false)?;
                    }
                }
                ExprKind::IfValue {
                    condition,
                    then_value,
                    else_value,
                } => {
                    expression(condition, plans, false)?;
                    expression(then_value, plans, boundary)?;
                    expression(else_value, plans, boundary)?;
                }
                ExprKind::Unary { operand, .. } => expression(operand, plans, false)?,
                ExprKind::Binary { left, right, .. } => {
                    expression(left, plans, false)?;
                    expression(right, plans, false)?;
                }
                _ => {}
            }
            Ok(())
        }
        fn statements(
            body: &[ValueStatement],
            plans: &[Option<CleanupPlan>],
        ) -> Result<(), &'static str> {
            for s in body {
                match s {
                    ValueStatement::Let { initializer, .. } => {
                        expression(initializer, plans, true)?
                    }
                    ValueStatement::Return { value, .. } => expression(value, plans, true)?,
                    ValueStatement::CallNoValue { arguments, .. } => {
                        for a in arguments {
                            expression(a, plans, false)?;
                        }
                    }
                    ValueStatement::Assign { value, .. }
                    | ValueStatement::DerefAssign { value, .. }
                    | ValueStatement::CopyFieldAssign { value, .. }
                    | ValueStatement::CopyReferenceFieldAssign { value, .. } => {
                        expression(value, plans, false)?
                    }
                    ValueStatement::If {
                        condition,
                        then_body,
                        else_body,
                        ..
                    } => {
                        expression(condition, plans, false)?;
                        statements(then_body, plans)?;
                        statements(else_body, plans)?;
                    }
                    ValueStatement::While {
                        condition, body, ..
                    } => {
                        expression(condition, plans, false)?;
                        statements(body, plans)?;
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        for f in &p.functions {
            if let Function::Ordinary(f) = f {
                statements(&f.body, plans)?;
            }
        }
        Ok(())
    }
    pub(crate) fn plans(&self, p: &Program) -> Result<Vec<Option<CleanupPlan>>, &'static str> {
        self.plans_bounded(p, 32)
    }
    pub(crate) fn plans_bounded(
        &self,
        p: &Program,
        max_depth: usize,
    ) -> Result<Vec<Option<CleanupPlan>>, &'static str> {
        if p.records.len() > 4096 || p.fields.len() > 8192 {
            return Err("cleanup type graph exceeds bounded capacity");
        }
        let mut plans = Vec::new();
        plans
            .try_reserve_exact(p.records.len())
            .map_err(|_| "cleanup plan allocation failed")?;
        plans.resize(p.records.len(), None);
        let mut marks = Vec::new();
        marks
            .try_reserve_exact(p.records.len())
            .map_err(|_| "cleanup graph allocation failed")?;
        marks.resize(p.records.len(), 0u8);
        let mut heights = Vec::new();
        heights
            .try_reserve_exact(p.records.len())
            .map_err(|_| "cleanup graph allocation failed")?;
        heights.resize(p.records.len(), 0usize);
        fn visit(
            c: &Catalog,
            p: &Program,
            id: RecordId,
            marks: &mut [u8],
            plans: &mut [Option<CleanupPlan>],
            heights: &mut [usize],
            depth: usize,
            max_depth: usize,
        ) -> Result<(), &'static str> {
            if depth > max_depth {
                return Err("cleanup type graph depth exceeded");
            }
            let mark = marks
                .get(id.0)
                .copied()
                .ok_or("foreign cleanup record identity")?;
            if mark == 1 {
                return Err("cyclic owned cleanup schema");
            }
            if mark == 2 {
                if depth
                    .checked_add(heights[id.0].saturating_sub(1))
                    .map_or(true, |d| d > max_depth)
                {
                    return Err("cleanup type graph depth exceeded");
                }
                return Ok(());
            }
            let r = p
                .records
                .get(id.0)
                .filter(|r| r.id == id)
                .ok_or("noncanonical cleanup record")?;
            if r.fields.len() > p.fields.len() {
                return Err("cleanup field membership exceeds canonical table");
            }
            marks[id.0] = 1;
            let mut height = 1;
            if let Some(l) = c.leaves.iter().find(|l| l.record == id) {
                plans[id.0] = Some(CleanupPlan::Release(l.id, l.release));
            } else {
                let mut children = Vec::new();
                children
                    .try_reserve_exact(r.fields.len())
                    .map_err(|_| "cleanup field plan allocation failed")?;
                for field in r.fields.iter().rev() {
                    let f = p
                        .fields
                        .get(field.0)
                        .filter(|f| f.id == *field && f.record == id)
                        .ok_or("foreign cleanup field identity")?;
                    if let ValueType::Record(child) = f.ty {
                        visit(c, p, child, marks, plans, heights, depth + 1, max_depth)?;
                        height = height.max(heights[child.0] + 1);
                        if plans[child.0].is_some() {
                            children.push((*field, child));
                        }
                    } else if !c.field_type_valid(p, f.ty) {
                        return Err("unsupported cleanup field type");
                    }
                }
                if !children.is_empty() {
                    plans[id.0] = Some(CleanupPlan::Fields(children));
                }
            }
            marks[id.0] = 2;
            heights[id.0] = height;
            Ok(())
        }
        for i in 0..p.records.len() {
            visit(
                self,
                p,
                RecordId(i),
                &mut marks,
                &mut plans,
                &mut heights,
                1,
                max_depth,
            )?;
        }
        Ok(plans)
    }
}

#[cfg(test)]
pub(crate) fn fixture() -> Program {
    fixture_with("")
}
#[cfg(test)]
pub(crate) fn fixture_with(extra: &str) -> Program {
    let mut p = crate::compile_source(&format!("type R = range 0..1; record Leaf {{ data: R }} record Pair {{ first: R, inert: R, last: R }} fn acquire() -> Leaf {{ return Leaf {{ data: 0 }}; }} fn release(x: Leaf) {{}} {extra}")).unwrap();
    p.records[0].fields.clear();
    p.fields.remove(0);
    for (i, f) in p.fields.iter_mut().enumerate() {
        f.id = FieldId(i);
    }
    for record in &mut p.records[1..] {
        for field in &mut record.fields {
            field.0 -= 1;
        }
    }
    fn field(id: &mut FieldId) {
        if id.0 > 0 {
            id.0 -= 1;
        }
    }
    fn expression(e: &mut TypedExpr) {
        match &mut e.kind {
            ExprKind::RecordConstruct { fields, .. } => {
                for f in fields {
                    field(&mut f.field);
                    expression(&mut f.value);
                }
            }
            ExprKind::CopyFieldRead { field: id, .. }
            | ExprKind::CopyReferenceFieldRead { field: id, .. } => field(id),
            ExprKind::Call { arguments, .. } => {
                for a in arguments {
                    expression(a);
                }
            }
            ExprKind::IfValue {
                condition,
                then_value,
                else_value,
            } => {
                expression(condition);
                expression(then_value);
                expression(else_value);
            }
            ExprKind::Unary { operand, .. } => expression(operand),
            ExprKind::Binary { left, right, .. } => {
                expression(left);
                expression(right);
            }
            _ => {}
        }
    }
    fn statements(body: &mut [ValueStatement]) {
        for s in body {
            match s {
                ValueStatement::Let { initializer, .. } => expression(initializer),
                ValueStatement::Return { value, .. }
                | ValueStatement::Assign { value, .. }
                | ValueStatement::DerefAssign { value, .. } => expression(value),
                ValueStatement::CopyFieldAssign {
                    field: id, value, ..
                }
                | ValueStatement::CopyReferenceFieldAssign {
                    field: id, value, ..
                } => {
                    field(id);
                    expression(value);
                }
                ValueStatement::CallNoValue { arguments, .. } => {
                    for a in arguments {
                        expression(a);
                    }
                }
                ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    expression(condition);
                    statements(then_body);
                    statements(else_body);
                }
                ValueStatement::While {
                    condition, body, ..
                } => {
                    expression(condition);
                    statements(body);
                }
                _ => {}
            }
        }
    }
    for f in &mut p.functions {
        if let Function::Ordinary(f) = f {
            statements(&mut f.body);
        }
    }
    p.fields[0].ty = ValueType::Record(RecordId(0));
    p.fields[2].ty = ValueType::Record(RecordId(0));
    if let Function::Ordinary(f) = &mut p.functions[0] {
        if let ValueStatement::Return { value, .. } = &mut f.body[0] {
            value.kind = ExprKind::RecordConstruct {
                record: RecordId(0),
                fields: Vec::new(),
            };
        }
    }
    p.resources = Catalog {
        leaves: vec![Leaf {
            id: ResourceId(0),
            record: RecordId(0),
            acquire: OperationId(0),
            release: OperationId(1),
        }],
        operations: vec![
            Operation {
                id: OperationId(0),
                function: FunctionId(0),
                kind: OperationKind::Acquire(ResourceId(0)),
            },
            Operation {
                id: OperationId(1),
                function: FunctionId(1),
                kind: OperationKind::Release(ResourceId(0)),
            },
        ],
    };
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_child_plans_do_not_hide_full_type_graph_depth() {
        let mut p = fixture_with("record Outer{payload:R}");
        p.fields[3].ty = ValueType::Record(RecordId(1));
        assert_eq!(
            p.resources.plans_bounded(&p, 2),
            Err("cleanup type graph depth exceeded")
        );
        assert!(p.resources.plans_bounded(&p, 3).is_ok());
    }
    #[test]
    fn plans_share_leaf_identity_in_reverse_canonical_order() {
        let p = fixture();
        p.resources.validate(&p).unwrap();
        assert_eq!(
            p.resources.plans(&p).unwrap(),
            vec![
                Some(CleanupPlan::Release(ResourceId(0), OperationId(1))),
                Some(CleanupPlan::Fields(vec![
                    (FieldId(2), RecordId(0)),
                    (FieldId(0), RecordId(0))
                ]))
            ]
        );
    }
    #[test]
    fn names_never_classify_or_dispatch_resources() {
        let mut p = fixture();
        let before = p.resources.plans(&p).unwrap();
        for r in &mut p.records {
            r.name = "same".into();
        }
        for f in &mut p.fields {
            f.name = "same".into();
        }
        assert_eq!(p.resources.plans(&p).unwrap(), before);
    }
    #[test]
    fn inert_move_records_have_no_cleanup() {
        let p =
            crate::compile_source("type R = range 0..1; record Inert { x: R } fn f() {}").unwrap();
        assert_eq!(p.resources.plans(&p).unwrap(), vec![None]);
    }
    #[test]
    fn foreign_resource_identity_rejects() {
        let mut p = fixture();
        p.resources.operations[0].kind = OperationKind::Acquire(ResourceId(9));
        assert!(p.resources.validate(&p).is_err());
    }
    #[test]
    fn wrong_trusted_pairing_rejects() {
        let mut p = fixture();
        p.resources.leaves[0].release = OperationId(0);
        assert!(p.resources.validate(&p).is_err());
    }
    #[test]
    fn duplicate_leaf_identity_rejects() {
        let mut p = fixture();
        p.resources.leaves.push(p.resources.leaves[0].clone());
        assert!(p.resources.validate(&p).is_err());
    }
    #[test]
    fn borrowed_release_signature_rejects() {
        let mut p = fixture();
        p.parameters[0].ty =
            ParameterType::Value(ValueType::SharedRef(ReferentType::Record(RecordId(0))));
        assert!(p.resources.validate(&p).is_err());
    }
    #[test]
    fn cycles_reject_without_expanding_plans() {
        let mut p = fixture();
        p.fields[0].ty = ValueType::Record(RecordId(1));
        assert!(p.resources.validate(&p).is_err());
    }
    #[test]
    fn foreign_field_identity_rejects() {
        let mut p = fixture();
        p.records[1].fields[0] = FieldId(99);
        assert!(p.resources.validate(&p).is_err());
    }
}
