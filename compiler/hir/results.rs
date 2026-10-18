//! Canonical finite Result DAG. Ordered payload roles are part of identity.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VariantChild {
    Record(RecordId),
    Result(ResultTypeId),
}

/// Ordered alternatives reference shared canonical plans, never two live fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VariantPlan {
    pub(crate) result: ResultTypeId,
    pub(crate) success: Option<VariantChild>,
    pub(crate) failure: Option<VariantChild>,
    pub(crate) effects: crate::effects::EffectSet,
    pub(crate) resource_bearing: bool,
}
impl VariantPlan {
    pub(crate) fn bearing(&self) -> bool {
        self.resource_bearing
    }
    pub(crate) fn active(&self, tag: ResultTag) -> Option<VariantChild> {
        match tag {
            ResultTag::Ok => self.success,
            ResultTag::Error => self.failure,
        }
    }
}

pub(crate) const MAX_RESULT_DEPTH: usize = 32;
pub(crate) const MAX_RESULT_TYPES: usize = 4096;

/// Source-shape errors remain diagnostics; tool limits and detected allocation
/// failures must survive resolution as typed resource failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionError {
    Source(&'static str),
    Resource(crate::bounded::ResourceFailure),
}
impl From<&'static str> for AdmissionError {
    fn from(message: &'static str) -> Self {
        Self::Source(message)
    }
}
impl From<crate::bounded::ResourceFailure> for AdmissionError {
    fn from(failure: crate::bounded::ResourceFailure) -> Self {
        Self::Resource(failure)
    }
}

impl Program {
    pub(crate) fn variant_plans(
        &self,
        records: &[Option<resources::CleanupPlan>],
        effects: &[crate::effects::EffectSet],
    ) -> std::result::Result<Vec<VariantPlan>, resources::PlanError> {
        self.validate_result_tables()
            .map_err(resources::PlanError::Integrity)?;
        let mut result: Vec<VariantPlan> = Vec::new();
        if !self.results.is_empty() {
            resources::reserve_plan(
                &mut result,
                self.results.len(),
                "Result variant-plan allocation",
            )?;
        }
        for ty in &self.results {
            let child = |value| -> std::result::Result<_, resources::PlanError> {
                Ok(match value {
                    ValueType::Record(id) if records.get(id.0).is_some_and(Option::is_some) => (
                        Some(VariantChild::Record(id)),
                        if effects.is_empty() {
                            crate::effects::EffectSet::EMPTY
                        } else {
                            *effects
                                .get(id.0)
                                .ok_or("missing record cleanup effect summary")?
                        },
                        true,
                    ),
                    ValueType::Result(id) => {
                        let plan = result
                            .get(id.0)
                            .ok_or("invalid Result child-plan identity")?;
                        (Some(VariantChild::Result(id)), plan.effects, plan.bearing())
                    }
                    ValueType::Bool | ValueType::Range(_) | ValueType::Record(_) => {
                        (None, crate::effects::EffectSet::EMPTY, false)
                    }
                    _ => return Err("ineligible Result variant payload".into()),
                })
            };
            let (success, a, ar) = child(ty.success)?;
            let (failure, b, br) = child(ty.failure)?;
            result.push(VariantPlan {
                result: ty.id,
                success,
                failure,
                effects: a.union(b),
                resource_bearing: ar || br,
            });
        }
        Ok(result)
    }
    pub fn result_types(&self) -> &[ResultType] {
        &self.results
    }
    pub fn result_type(&self, id: ResultTypeId) -> Option<&ResultType> {
        self.results.get(id.0).filter(|r| r.id == id)
    }
    pub(crate) fn result_payload(&self, id: ResultTypeId, tag: ResultTag) -> Option<ValueType> {
        let result = self.result_type(id)?;
        Some(match tag {
            ResultTag::Ok => result.success,
            ResultTag::Error => result.failure,
        })
    }
    pub(crate) fn intern_result(
        &mut self,
        success: ValueType,
        failure: ValueType,
    ) -> std::result::Result<ResultTypeId, AdmissionError> {
        let mut depth = 1;
        for ty in [success, failure] {
            let child = self.result_value_depth(ty, self.results.len())?;
            depth = depth.max(child + 1);
        }
        if depth > MAX_RESULT_DEPTH {
            return Err(crate::bounded::ResourceFailure::limit(
                "Result type nesting",
                crate::bounded::START,
            )
            .into());
        }
        if let Some(r) = self
            .results
            .iter()
            .find(|r| r.success == success && r.failure == failure)
        {
            return Ok(r.id);
        }
        if self.results.len() >= MAX_RESULT_TYPES {
            return Err(crate::bounded::ResourceFailure::limit(
                "Result instantiation count",
                crate::bounded::START,
            )
            .into());
        }
        crate::bounded::result_admission::reserve(
            &mut self.results,
            1,
            crate::bounded::result_admission::Allocation::Table,
            crate::bounded::START,
        )?;
        let id = ResultTypeId(self.results.len());
        self.results.push(ResultType {
            id,
            success,
            failure,
            depth,
        });
        Ok(id)
    }
    fn result_value_depth(
        &self,
        ty: ValueType,
        before: usize,
    ) -> std::result::Result<usize, &'static str> {
        match ty {
            ValueType::Bool => Ok(0),
            ValueType::Range(id) if self.ranges.get(id.0).is_some_and(|r| r.id == id) => Ok(0),
            ValueType::Record(id) if self.records.get(id.0).is_some_and(|r| r.id == id) => Ok(0),
            ValueType::Result(id) if id.0 < before => self
                .result_type(id)
                .map(|r| r.depth)
                .ok_or("invalid Result payload identity"),
            _ => {
                Err("Result payload requires an exact ordinary owned type; references are excluded")
            }
        }
    }
    pub(crate) fn validate_result_tables(&self) -> std::result::Result<(), &'static str> {
        if self.results.len() > MAX_RESULT_TYPES {
            return Err("Result instantiation limit exceeded");
        }
        for (index, result) in self.results.iter().enumerate() {
            if result.id.0 != index {
                return Err("invalid canonical Result identity");
            }
            let depth = 1 + self
                .result_value_depth(result.success, index)?
                .max(self.result_value_depth(result.failure, index)?);
            if depth > MAX_RESULT_DEPTH || result.depth != depth {
                return Err("invalid or over-depth canonical Result type");
            }
            if self.results[..index]
                .iter()
                .any(|r| r.success == result.success && r.failure == result.failure)
            {
                return Err("duplicate canonical Result payload pair");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn typed() -> Program {
        crate::compile_source("type A=range 0..1;type B=range 0..1;fn first(a:A)->Result<success:A,failure:B>{return Ok(a);}fn second(a:A)->Result<success:Result<success:A,failure:B>,failure:bool>{return Ok(first(a));}").unwrap()
    }
    #[test]
    fn invalid_unused_result_identities_roles_cycles_and_depth_reject_before_lowering() {
        for mutation in 0..9 {
            let mut p = typed();
            match mutation {
                0 => p.results[0].id = ResultTypeId(99),
                1 => {
                    p.results[1] = p.results[0].clone();
                    p.results[1].id = ResultTypeId(1);
                }
                2 => p.results[0].success = ValueType::Result(ResultTypeId(0)),
                3 => p.results[0].failure = ValueType::Result(ResultTypeId(1)),
                4 => p.results[1].depth = MAX_RESULT_DEPTH + 1,
                5 => p.results[0].success = ValueType::Range(RangeTypeId(99)),
                6 => p.results[0].failure = ValueType::SharedRef(ReferentType::Bool),
                7 => {
                    let mut unused = p.results[0].clone();
                    unused.id = ResultTypeId(2);
                    p.results.push(unused);
                }
                8 => p.results[0].depth = usize::MAX,
                _ => unreachable!(),
            }
            assert!(p.validate_result_tables().is_err(), "mutation {mutation}");
            assert!(crate::execution::prepare_ordinary_synthetic(
                p,
                crate::execution::AdmissionLimits::default()
            )
            .is_err());
        }
    }
    #[test]
    fn ordered_payload_roles_remain_distinct_and_nesting_is_finite() {
        let mut p = typed();
        let first = p.results[0].clone();
        let reversed = p.intern_result(first.failure, first.success).unwrap();
        assert_ne!(reversed, first.id);
        assert_eq!(
            p.intern_result(first.success, first.failure).unwrap(),
            first.id
        );
        let mut child = ValueType::Bool;
        for _ in 0..MAX_RESULT_DEPTH {
            child = ValueType::Result(p.intern_result(child, ValueType::Bool).unwrap());
        }
        assert!(p.intern_result(child, ValueType::Bool).is_err());
        p.validate_result_tables().unwrap();
    }
}
