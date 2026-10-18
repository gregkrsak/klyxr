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
    pub(crate) effects: crate::effects::EffectSet,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Leaf {
    pub(crate) id: ResourceId,
    pub(crate) record: RecordId,
    pub(crate) acquire: OperationId,
    pub(crate) release: OperationId,
    pub(crate) cleanup_effects: crate::effects::EffectSet,
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
/// Detected preparation failures remain distinct from malformed compiler state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanError {
    Integrity(&'static str),
    Limit(&'static str),
    Host(&'static str),
}
impl From<&'static str> for PlanError {
    fn from(message: &'static str) -> Self {
        Self::Integrity(message)
    }
}
impl PlanError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Integrity(m) | Self::Limit(m) | Self::Host(m) => m,
        }
    }
}
#[cfg(test)]
thread_local! {
    // Number of actual reservation boundaries to permit before detected failure.
    pub(crate) static FAIL_PLAN_ALLOCATION: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
pub(super) fn reserve_plan<T>(
    values: &mut Vec<T>,
    count: usize,
    message: &'static str,
) -> Result<(), PlanError> {
    #[cfg(test)]
    FAIL_PLAN_ALLOCATION.with(|remaining| {
        if let Some(n) = remaining.get() {
            if n == 0 {
                return Err(PlanError::Host(message));
            }
            remaining.set(Some(n - 1));
        }
        Ok(())
    })?;
    values
        .try_reserve_exact(count)
        .map_err(|_| PlanError::Host(message))
}
impl Catalog {
    /// Called only with the canonical plans produced/checked by admission.
    /// An empty vector canonically denotes all-empty summaries, not absent plans.
    pub(crate) fn summaries(
        &self,
        p: &Program,
        plans: &[Option<CleanupPlan>],
    ) -> Result<Vec<crate::effects::EffectSet>, PlanError> {
        use crate::effects::EffectSet;
        for leaf in &self.leaves {
            let operation = self
                .operations
                .get(leaf.release.0)
                .filter(|o| o.id == leaf.release && o.kind == OperationKind::Release(leaf.id))
                .ok_or("foreign cleanup release operation")?;
            let function = p
                .functions
                .get(operation.function.0)
                .filter(|f| f.id() == operation.function)
                .and_then(Function::as_ordinary)
                .ok_or("foreign cleanup release function")?;
            if leaf.cleanup_effects != operation.effects
                || operation.effects != function.effects.set()
            {
                return Err("cleanup contract derivation mismatch".into());
            }
        }
        if self
            .leaves
            .iter()
            .all(|l| l.cleanup_effects == EffectSet::EMPTY)
        {
            return Ok(Vec::new());
        }
        if plans.len() != p.records.len() || plans.len() > 4096 {
            return Err("cleanup summary table shape".into());
        }
        let mut summaries = Vec::new();
        reserve_plan(
            &mut summaries,
            plans.len(),
            "cleanup effect summary allocation failed",
        )?;
        summaries.resize(plans.len(), EffectSet::EMPTY);
        let mut marks = Vec::new();
        reserve_plan(
            &mut marks,
            plans.len(),
            "cleanup effect graph allocation failed",
        )?;
        marks.resize(plans.len(), 0u8);
        fn visit(
            catalog: &Catalog,
            plans: &[Option<CleanupPlan>],
            summaries: &mut [EffectSet],
            marks: &mut [u8],
            record: RecordId,
            depth: usize,
        ) -> Result<EffectSet, PlanError> {
            if depth > 32 {
                return Err(PlanError::Limit("cleanup effect graph depth exceeded"));
            }
            match marks
                .get(record.0)
                .copied()
                .ok_or("foreign cleanup summary record")?
            {
                1 => return Err("cyclic cleanup effect graph".into()),
                2 => return Ok(summaries[record.0]),
                _ => {}
            }
            marks[record.0] = 1;
            let mut result = EffectSet::EMPTY;
            match &plans[record.0] {
                Some(CleanupPlan::Release(resource, operation)) => {
                    let leaf = catalog
                        .leaves
                        .get(resource.0)
                        .filter(|l| {
                            l.id == *resource && l.record == record && l.release == *operation
                        })
                        .ok_or("cleanup summary release binding mismatch")?;
                    result = leaf.cleanup_effects;
                }
                Some(CleanupPlan::Fields(children)) => {
                    for (_, child) in children {
                        let child_effects =
                            visit(catalog, plans, summaries, marks, *child, depth + 1)?;
                        result = result.union(child_effects);
                    }
                }
                None => {}
            }
            marks[record.0] = 2;
            summaries[record.0] = result;
            Ok(result)
        }
        for index in 0..plans.len() {
            visit(self, plans, &mut summaries, &mut marks, RecordId(index), 1)?;
        }
        Ok(summaries)
    }

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
    pub(crate) fn validate(&self, p: &Program) -> Result<(), PlanError> {
        if self.leaves.is_empty() && self.operations.is_empty() {
            return Ok(());
        }
        if self.leaves.len() > 4096 || self.operations.len() > 8192 {
            return Err("trusted resource catalog exceeds bounded capacity".into());
        }
        for (i, leaf) in self.leaves.iter().enumerate() {
            if leaf.id != ResourceId(i)
                || self.leaves[..i].iter().any(|l| l.record == leaf.record)
                || p.records
                    .get(leaf.record.0)
                    .map_or(true, |r| r.id != leaf.record || !r.fields.is_empty())
            {
                return Err("invalid or duplicate canonical resource leaf".into());
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
                    return Err("trusted acquisition/release pairing mismatch".into());
                }
            }
        }
        for (i, op) in self.operations.iter().enumerate() {
            if op.id != OperationId(i)
                || self.operations[..i]
                    .iter()
                    .any(|o| o.function == op.function)
            {
                return Err("invalid or duplicate trusted operation identity".into());
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
                return Err("orphan trusted operation".into());
            }
            let f = p
                .functions
                .get(op.function.0)
                .filter(|f| f.id() == op.function)
                .and_then(Function::as_ordinary)
                .ok_or("trusted operation is not canonical ordinary function")?;
            if op.effects != f.effects.set() {
                return Err("trusted operation/function effect contract mismatch".into());
            }
            if !acquire && leaf.cleanup_effects != op.effects {
                return Err("resource leaf cleanup effect derivation mismatch".into());
            }
            if acquire {
                if !matches!(f.body.as_slice(), [ValueStatement::Return { value: TypedExpr { kind: ExprKind::RecordConstruct { record, fields }, ty: ExprType::Record(ty), .. }, .. }] if *record == leaf.record && *ty == leaf.record && fields.is_empty())
                {
                    return Err("trusted acquisition must have the canonical private stub".into());
                }
                if !f.parameters.is_empty() || f.return_type != Some(ValueType::Record(leaf.record))
                {
                    return Err("trusted acquisition signature mismatch".into());
                }
            } else {
                if !f.body.is_empty() {
                    return Err("trusted release cannot dispatch user code".into());
                }
                if f.return_type.is_some() || f.parameters.len() != 1 {
                    return Err("trusted release signature mismatch".into());
                }
                let a = p
                    .parameters
                    .get(f.parameters[0].0)
                    .filter(|a| a.id == f.parameters[0] && a.function == f.id)
                    .ok_or("foreign trusted release parameter")?;
                if a.ty != ParameterType::Value(ValueType::Record(leaf.record)) {
                    return Err("trusted release must consume the exact owned leaf".into());
                }
            }
        }
        // Check the graph even for inert schemas; cycles are never permitted.
        let plans = self.plans(p)?;
        self.temporaries(p, &plans).map_err(PlanError::from)
    }
    fn temporaries(&self, p: &Program, plans: &[Option<CleanupPlan>]) -> Result<(), &'static str> {
        fn bearing(e: &TypedExpr, plans: &[Option<CleanupPlan>]) -> bool {
            matches!(e.ty,ExprType::Record(id) if plans.get(id.0).is_some_and(Option::is_some))
        }
        fn expression(
            e: &TypedExpr,
            plans: &[Option<CleanupPlan>],
            leaves: &[Leaf],
            boundary: bool,
        ) -> Result<(), &'static str> {
            if matches!(e.kind, ExprKind::RecordConstruct {record,..} if leaves.iter().any(|l| l.record == record))
            {
                return Err(
                    "trusted resource leaf construction outside canonical acquisition stub",
                );
            }
            if bearing(e, plans)
                && !boundary
                && !matches!(e.kind, ExprKind::Local(_) | ExprKind::Parameter(_))
            {
                return Err(
                    "resource-bearing expression temporary has no admitted lifetime boundary",
                );
            }
            match &e.kind {
                ExprKind::ResultConstruct { payload, .. } => {
                    expression(payload, plans, leaves, true)?
                }
                ExprKind::RecordConstruct { fields, .. } => {
                    for f in fields {
                        expression(&f.value, plans, leaves, true)?;
                    }
                }
                ExprKind::Call { arguments, .. } => {
                    for a in arguments {
                        expression(a, plans, leaves, true)?;
                    }
                }
                ExprKind::IfValue {
                    condition,
                    then_value,
                    else_value,
                } => {
                    expression(condition, plans, leaves, false)?;
                    expression(then_value, plans, leaves, boundary)?;
                    expression(else_value, plans, leaves, boundary)?;
                }
                ExprKind::Unary { operand, .. } => expression(operand, plans, leaves, false)?,
                ExprKind::Binary { left, right, .. } => {
                    expression(left, plans, leaves, false)?;
                    expression(right, plans, leaves, false)?;
                }
                _ => {}
            }
            Ok(())
        }
        fn statements(
            body: &[ValueStatement],
            plans: &[Option<CleanupPlan>],
            leaves: &[Leaf],
        ) -> Result<(), &'static str> {
            for s in body {
                match s {
                    ValueStatement::Match {
                        scrutinee, arms, ..
                    } => {
                        expression(scrutinee, plans, leaves, true)?;
                        for arm in arms {
                            statements(&arm.body, plans, leaves)?;
                        }
                    }
                    ValueStatement::Let { initializer, .. } => {
                        expression(initializer, plans, leaves, true)?
                    }
                    ValueStatement::Return { value, .. } => expression(value, plans, leaves, true)?,
                    ValueStatement::CallNoValue { arguments, .. } => {
                        for a in arguments {
                            expression(a, plans, leaves, true)?;
                        }
                    }
                    ValueStatement::Assign { value, .. } => {
                        // Complete replacement results and exact owned call
                        // arguments have their respective protected boundaries.
                        expression(
                            value,
                            plans,
                            leaves,
                            matches!(value.ty, ExprType::Record(_)),
                        )?
                    }
                    ValueStatement::DerefAssign { value, .. }
                    | ValueStatement::CopyFieldAssign { value, .. }
                    | ValueStatement::CopyReferenceFieldAssign { value, .. } => {
                        expression(value, plans, leaves, false)?
                    }
                    ValueStatement::If {
                        condition,
                        then_body,
                        else_body,
                        ..
                    } => {
                        expression(condition, plans, leaves, false)?;
                        statements(then_body, plans, leaves)?;
                        statements(else_body, plans, leaves)?;
                    }
                    ValueStatement::While {
                        condition, body, ..
                    } => {
                        expression(condition, plans, leaves, false)?;
                        statements(body, plans, leaves)?;
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        for f in &p.functions {
            if let Function::Ordinary(f) = f {
                if self
                    .operations
                    .iter()
                    .any(|o| o.function == f.id && matches!(o.kind, OperationKind::Acquire(_)))
                {
                    continue;
                }
                statements(&f.body, plans, &self.leaves)?;
            }
        }
        Ok(())
    }
    pub(crate) fn plans(&self, p: &Program) -> Result<Vec<Option<CleanupPlan>>, PlanError> {
        self.plans_bounded(p, 32)
    }
    pub(crate) fn plans_bounded(
        &self,
        p: &Program,
        max_depth: usize,
    ) -> Result<Vec<Option<CleanupPlan>>, PlanError> {
        if p.records.len() > 4096 || p.fields.len() > 8192 {
            return Err(PlanError::Limit(
                "cleanup type graph exceeds bounded capacity",
            ));
        }
        let mut plans = Vec::new();
        reserve_plan(
            &mut plans,
            p.records.len(),
            "cleanup plan allocation failed",
        )?;
        plans.resize(p.records.len(), None);
        let mut marks = Vec::new();
        reserve_plan(
            &mut marks,
            p.records.len(),
            "cleanup graph allocation failed",
        )?;
        marks.resize(p.records.len(), 0u8);
        let mut heights = Vec::new();
        reserve_plan(
            &mut heights,
            p.records.len(),
            "cleanup graph allocation failed",
        )?;
        heights.resize(p.records.len(), 0usize);
        fn visit(
            c: &Catalog,
            p: &Program,
            id: RecordId,
            marks: &mut [u8],
            plans: &mut [Option<CleanupPlan>],
            heights: &mut [usize],
            bounds: (usize, usize),
        ) -> Result<(), PlanError> {
            let (depth, max_depth) = bounds;
            if depth > max_depth {
                return Err(PlanError::Limit("cleanup type graph depth exceeded"));
            }
            let mark = marks
                .get(id.0)
                .copied()
                .ok_or("foreign cleanup record identity")?;
            if mark == 1 {
                return Err("cyclic owned cleanup schema".into());
            }
            if mark == 2 {
                if depth
                    .checked_add(heights[id.0].saturating_sub(1))
                    .map_or(true, |d| d > max_depth)
                {
                    return Err(PlanError::Limit("cleanup type graph depth exceeded"));
                }
                return Ok(());
            }
            let r = p
                .records
                .get(id.0)
                .filter(|r| r.id == id)
                .ok_or("noncanonical cleanup record")?;
            if r.fields.len() > p.fields.len() {
                return Err("cleanup field membership exceeds canonical table".into());
            }
            marks[id.0] = 1;
            let mut height = 1;
            if let Some(l) = c.leaves.iter().find(|l| l.record == id) {
                plans[id.0] = Some(CleanupPlan::Release(l.id, l.release));
            } else {
                let mut children = Vec::new();
                reserve_plan(
                    &mut children,
                    r.fields.len(),
                    "cleanup field plan allocation failed",
                )?;
                for field in r.fields.iter().rev() {
                    let f = p
                        .fields
                        .get(field.0)
                        .filter(|f| f.id == *field && f.record == id)
                        .ok_or("foreign cleanup field identity")?;
                    if let ValueType::Record(child) = f.ty {
                        visit(c, p, child, marks, plans, heights, (depth + 1, max_depth))?;
                        height = height.max(heights[child.0] + 1);
                        if plans[child.0].is_some() {
                            children.push((*field, child));
                        }
                    } else if !c.field_type_valid(p, f.ty) {
                        return Err("unsupported cleanup field type".into());
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
                (1, max_depth),
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
            ExprKind::ResultConstruct { payload, .. } => expression(payload),
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
                ValueStatement::Match {
                    scrutinee, arms, ..
                } => {
                    expression(scrutinee);
                    for arm in arms {
                        statements(&mut arm.body);
                    }
                }
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
            cleanup_effects: crate::effects::EffectSet::EMPTY,
        }],
        operations: vec![
            Operation {
                id: OperationId(0),
                function: FunctionId(0),
                kind: OperationKind::Acquire(ResourceId(0)),
                effects: crate::effects::EffectSet::EMPTY,
            },
            Operation {
                id: OperationId(1),
                function: FunctionId(1),
                kind: OperationKind::Release(ResourceId(0)),
                effects: crate::effects::EffectSet::EMPTY,
            },
        ],
    };
    p
}

#[cfg(test)]
pub(crate) fn fixture_effects(
    extra: &str,
    acquire: crate::effects::EffectSet,
    release: crate::effects::EffectSet,
) -> Program {
    let mut p = fixture_with(extra);
    for (index, set) in [(0, acquire), (1, release)] {
        let Function::Ordinary(f) = &mut p.functions[index] else {
            unreachable!()
        };
        f.effects = crate::effects::Declaration::synthetic(set, f.span);
        p.resources.operations[index].effects = set;
    }
    p.resources.leaves[0].cleanup_effects = release;
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_release_contracts_derive_leaf_and_shared_aggregate_unions() {
        use crate::effects::{Effect, EffectSet};
        for effect in Effect::ALL {
            let mut release = EffectSet::EMPTY;
            release.insert(effect);
            let mut acquire = EffectSet::EMPTY;
            acquire.insert(Effect::Io);
            let p = fixture_effects("", acquire, release);
            p.resources.validate(&p).unwrap();
            let plans = p.resources.plans(&p).unwrap();
            let summaries = p.resources.summaries(&p, &plans).unwrap();
            assert_eq!(summaries, vec![release, release]);
            assert_eq!(p.resources.operations[0].effects, acquire);
            assert_eq!(p.resources.leaves[0].cleanup_effects, release);
        }
    }

    #[test]
    fn aggregate_summary_unions_distinct_release_contracts_without_expansion() {
        use crate::effects::{Declaration, Effect, EffectSet};
        let mut first = EffectSet::EMPTY;
        first.insert(Effect::Filesystem);
        first.insert(Effect::Blocking);
        let mut second = EffectSet::EMPTY;
        second.insert(Effect::Clock);
        let mut p = fixture_effects(
            "record Other{data:R} fn acquire_other()->Other{return Other{data:0};} fn release_other(x:Other){}",
            EffectSet::EMPTY,
            first,
        );
        p.records[2].fields.clear();
        p.fields.pop();
        p.fields[2].ty = ValueType::Record(RecordId(2));
        let Function::Ordinary(acquire) = &mut p.functions[2] else {
            unreachable!()
        };
        let ValueStatement::Return { value, .. } = &mut acquire.body[0] else {
            unreachable!()
        };
        value.kind = ExprKind::RecordConstruct {
            record: RecordId(2),
            fields: Vec::new(),
        };
        let Function::Ordinary(release) = &mut p.functions[3] else {
            unreachable!()
        };
        release.effects = Declaration::synthetic(second, release.span);
        p.resources.leaves.push(Leaf {
            id: ResourceId(1),
            record: RecordId(2),
            acquire: OperationId(2),
            release: OperationId(3),
            cleanup_effects: second,
        });
        p.resources.operations.extend([
            Operation {
                id: OperationId(2),
                function: FunctionId(2),
                kind: OperationKind::Acquire(ResourceId(1)),
                effects: EffectSet::EMPTY,
            },
            Operation {
                id: OperationId(3),
                function: FunctionId(3),
                kind: OperationKind::Release(ResourceId(1)),
                effects: second,
            },
        ]);
        p.validate_record_expressions().unwrap();
        let plans = p.resources.plans(&p).unwrap();
        assert_eq!(
            p.resources.summaries(&p, &plans).unwrap(),
            vec![first, first.union(second), second]
        );
        assert_eq!(
            plans[1],
            Some(CleanupPlan::Fields(vec![
                (FieldId(2), RecordId(2)),
                (FieldId(0), RecordId(0))
            ]))
        );
    }

    #[test]
    fn operation_function_leaf_and_plan_mutations_reject_independently() {
        use crate::effects::{Effect, EffectSet};
        let mut release = EffectSet::EMPTY;
        release.insert(Effect::Filesystem);
        release.insert(Effect::Blocking);
        for mutation in 0..5 {
            let mut p = fixture_effects("", EffectSet::EMPTY, release);
            match mutation {
                0 => p.resources.operations[1].effects = EffectSet::EMPTY,
                1 => {
                    let Function::Ordinary(f) = &mut p.functions[1] else {
                        unreachable!()
                    };
                    f.effects = crate::effects::Declaration::default();
                }
                2 => p.resources.leaves[0].cleanup_effects = EffectSet::EMPTY,
                3 => p.resources.leaves[0].release = OperationId(0),
                _ => p.resources.operations[1].function = FunctionId(0),
            }
            assert!(p.resources.validate(&p).is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn empty_cleanup_effects_preserve_real_plans_and_responsibility() {
        let p = fixture();
        let plans = p.resources.plans(&p).unwrap();
        assert!(plans[0].is_some());
        assert!(plans[1].is_some());
        assert!(p.resources.summaries(&p, &plans).unwrap().is_empty());
        assert_eq!(
            p.resources.leaves[0].cleanup_effects,
            crate::effects::EffectSet::EMPTY
        );
        let inert = crate::compile_source("type N=range 0..1;record R{x:N}fn f(){}").unwrap();
        assert_eq!(inert.resources.plans(&inert).unwrap(), vec![None]);
    }
    #[test]
    fn cached_child_plans_do_not_hide_full_type_graph_depth() {
        let mut p = fixture_with("record Outer{payload:R}");
        p.fields[3].ty = ValueType::Record(RecordId(1));
        assert_eq!(
            p.resources.plans_bounded(&p, 2),
            Err(PlanError::Limit("cleanup type graph depth exceeded"))
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
