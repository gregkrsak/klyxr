//! Table-aware invariants for ordinary record expressions at typed-HIR publication.
//! This checks canonical relationships and placement, not inference or ownership.
use super::*;
use crate::diagnostics::Diagnostic;
type Result = std::result::Result<(), Box<Diagnostic>>;

impl Program {
    pub(crate) fn validate_record_expressions(&self) -> Result {
        for function in &self.functions {
            match function {
                Function::Ordinary(f) => self.record_statements(f.id, &f.body)?,
                Function::Verified(f) => {
                    for expression in [&f.requires, &f.ensures]
                        .into_iter()
                        .chain(f.body.iter().map(|s| &s.operand))
                    {
                        self.record_expression(f.id, expression, false, false)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn record_statements(&self, function: FunctionId, body: &[ValueStatement]) -> Result {
        for statement in body {
            match statement {
                ValueStatement::Let { initializer, .. } => {
                    self.record_expression(function, initializer, true, true)?
                }
                ValueStatement::Return { value, .. }
                | ValueStatement::Assign { value, .. }
                | ValueStatement::DerefAssign { value, .. } => {
                    self.record_expression(function, value, true, false)?
                }
                ValueStatement::CallNoValue { arguments, .. } => {
                    for value in arguments {
                        self.record_expression(function, value, true, false)?;
                    }
                }
                ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.record_expression(function, condition, true, false)?;
                    self.record_statements(function, then_body)?;
                    self.record_statements(function, else_body)?;
                }
                ValueStatement::While {
                    condition, body, ..
                } => {
                    self.record_expression(function, condition, true, false)?;
                    self.record_statements(function, body)?;
                }
                ValueStatement::ReturnNoValue { .. }
                | ValueStatement::Break { .. }
                | ValueStatement::Continue { .. } => {}
            }
        }
        Ok(())
    }
    fn record_expression(
        &self,
        function: FunctionId,
        expression: &TypedExpr,
        ordinary: bool,
        conditional: bool,
    ) -> Result {
        let invalid = |message| {
            Box::new(Diagnostic::semantic(expression.span, message, "canonical ordinary record relationships and existing expression positions must be preserved at the typed HIR boundary"))
        };
        match &expression.kind {
            ExprKind::RecordConstruct {
                record,
                field,
                value,
            } => {
                if !ordinary {
                    return Err(invalid(
                        "ordinary record construction is ineligible in verified expressions",
                    ));
                }
                let range = self.record_field(*record, *field).ok_or_else(|| {
                    invalid("construction field does not belong to the canonical record")
                })?;
                if expression.ty != ExprType::Record(*record) || value.ty != ExprType::Range(range)
                {
                    return Err(invalid("construction result or initializer type is inconsistent with its canonical record field"));
                }
                self.record_expression(function, value, ordinary, false)?;
            }
            ExprKind::CopyFieldRead {
                owner,
                record,
                field,
            } => {
                if !ordinary {
                    return Err(invalid(
                        "ordinary Copy-field read is ineligible in verified expressions",
                    ));
                }
                let owner_type = match owner {
                    Place::Parameter(id) => self
                        .parameters
                        .get(id.0)
                        .filter(|p| p.id == *id && p.function == function)
                        .and_then(|p| match p.ty {
                            ParameterType::Value(ty) => Some(ty),
                            _ => None,
                        }),
                    Place::Local(id) => self
                        .locals
                        .get(id.0)
                        .filter(|l| l.id == *id && l.function == function)
                        .map(|l| l.ty),
                };
                if owner_type != Some(ValueType::Record(*record)) {
                    return Err(invalid("Copy-field read requires a canonical named owned root of the stated record type"));
                }
                let range = self.record_field(*record, *field).ok_or_else(|| {
                    invalid("Copy-field read field does not belong to the canonical owner record")
                })?;
                if expression.ty != ExprType::Range(range) {
                    return Err(invalid(
                        "Copy-field read result must be the exact declared named-range Copy type",
                    ));
                }
            }
            ExprKind::IfValue {
                condition,
                then_value,
                else_value,
            } => {
                if !conditional {
                    return Err(invalid("conditional values are permitted only as complete local initializers or their complete branch results"));
                }
                self.record_expression(function, condition, ordinary, false)?;
                self.record_expression(function, then_value, ordinary, true)?;
                self.record_expression(function, else_value, ordinary, true)?;
            }
            ExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    self.record_expression(function, argument, ordinary, false)?;
                }
            }
            ExprKind::Unary { operand, .. } => {
                self.record_expression(function, operand, ordinary, false)?
            }
            ExprKind::Binary { left, right, .. } => {
                self.record_expression(function, left, ordinary, false)?;
                self.record_expression(function, right, ordinary, false)?;
            }
            ExprKind::FieldAccess(_) | ExprKind::OldField(_) if ordinary => {
                return Err(invalid(
                    "verified-state field representation is ineligible in ordinary expressions",
                ))
            }
            ExprKind::FieldAccess(_)
            | ExprKind::OldField(_)
            | ExprKind::Parameter(_)
            | ExprKind::Local(_)
            | ExprKind::Deref { .. }
            | ExprKind::Borrow { .. }
            | ExprKind::BoolLiteral(_)
            | ExprKind::IntegerLiteral(_) => {}
        }
        Ok(())
    }
    fn record_field(&self, record: RecordId, field: FieldId) -> Option<RangeTypeId> {
        let r = self
            .records
            .get(record.0)
            .filter(|r| r.id == record && r.field == field)?;
        let f = self
            .fields
            .get(field.0)
            .filter(|f| f.id == field && f.record == r.id)?;
        self.ranges.get(f.ty.0).filter(|r| r.id == f.ty)?;
        Some(f.ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Program {
        crate::compile_source("type Percent = range 0..100; type OtherPercent = range 0..100; record Battery { charge: Percent } record Capacitor { charge: Percent } fn f(b: Battery,p: Percent) -> Battery { let local = b; let seen = local.charge; return Battery { charge: p }; } fn g(b: Capacitor) -> Percent { return b.charge; }").unwrap()
    }
    fn read(p: &Program) -> TypedExpr {
        let ValueStatement::Let { initializer, .. } =
            &p.functions[0].as_ordinary().unwrap().body[1]
        else {
            panic!()
        };
        initializer.clone()
    }
    fn check_read(p: &Program, expression: &TypedExpr) -> Result {
        p.record_expression(p.functions[0].id(), expression, true, false)
    }
    #[test]
    fn invariant_boundary_accepts_canonical_nodes_and_read_only_tables() {
        let p = fixture();
        p.validate_record_expressions().unwrap();
        assert!(p.bindings.is_empty());
        assert_eq!(p.locals.len(), 2);
        assert!(matches!(read(&p).kind, ExprKind::CopyFieldRead { .. }));
    }
    #[test]
    fn invariant_rejects_field_of_different_record_even_with_identical_spelling() {
        let p = fixture();
        let mut e = read(&p);
        let ExprKind::CopyFieldRead { field, .. } = &mut e.kind else {
            panic!()
        };
        *field = p.records[1].field;
        assert!(check_read(&p, &e)
            .unwrap_err()
            .message
            .contains("does not belong"));
    }
    #[test]
    fn invariant_rejects_stated_record_inconsistent_with_owner_type() {
        let p = fixture();
        let mut e = read(&p);
        let ExprKind::CopyFieldRead { record, field, .. } = &mut e.kind else {
            panic!()
        };
        *record = p.records[1].id;
        *field = p.records[1].field;
        assert!(check_read(&p, &e)
            .unwrap_err()
            .message
            .contains("owned root"));
    }
    #[test]
    fn invariant_rejects_invalid_parameter_and_local_id_without_panics() {
        let p = fixture();
        for owner in [
            Place::Parameter(ParameterId(999)),
            Place::Local(LocalId(999)),
        ] {
            let mut e = read(&p);
            let ExprKind::CopyFieldRead { owner: target, .. } = &mut e.kind else {
                panic!()
            };
            *target = owner;
            assert!(check_read(&p, &e).is_err());
        }
    }
    #[test]
    fn invariant_rejects_foreign_function_owner() {
        let p = fixture();
        let mut e = read(&p);
        let ExprKind::CopyFieldRead { owner, .. } = &mut e.kind else {
            panic!()
        };
        *owner = Place::Parameter(p.functions[1].as_ordinary().unwrap().parameters[0]);
        assert!(check_read(&p, &e).is_err());
    }
    #[test]
    fn invariant_rejects_reference_root_without_auto_deref() {
        let mut p = fixture();
        p.locals[0].ty = ValueType::SharedRef(ReferentType::Record(p.records[0].id));
        assert!(check_read(&p, &read(&p)).is_err());
        p.locals[0].ty = ValueType::MutableRef(ReferentType::Record(p.records[0].id));
        assert!(check_read(&p, &read(&p)).is_err());
    }
    #[test]
    fn invariant_rejects_wrong_nominal_range_and_move_valued_read_result() {
        let p = fixture();
        for ty in [
            ExprType::Range(p.ranges[1].id),
            ExprType::Record(p.records[0].id),
            ExprType::MutableRef(ReferentType::Record(p.records[0].id)),
            ExprType::Bool,
            ExprType::IntegerLiteral,
        ] {
            let mut e = read(&p);
            e.ty = ty;
            assert!(check_read(&p, &e)
                .unwrap_err()
                .message
                .contains("exact declared named-range Copy"));
        }
    }
    #[test]
    fn invariant_rejects_inconsistent_canonical_field_tables_and_invalid_record() {
        let mut p = fixture();
        p.fields[0].record = p.records[1].id;
        assert!(check_read(&p, &read(&p)).is_err());
        let p = fixture();
        let mut e = read(&p);
        let ExprKind::CopyFieldRead { record, .. } = &mut e.kind else {
            panic!()
        };
        *record = RecordId(999);
        assert!(check_read(&p, &e).is_err());
    }
    #[test]
    fn invariant_rejects_verified_parameter_and_state_projection_in_ordinary_code() {
        let mut p = fixture();
        let e = read(&p);
        p.locals[0].ty = ValueType::Bool;
        assert!(check_read(&p, &e).is_err());
        let p = fixture();
        let mut e = read(&p);
        let id = p.functions[0].as_ordinary().unwrap().parameters[0];
        for old in [false, true] {
            let access = FieldAccess {
                parameter: id,
                field: p.fields[0].id,
                ty: p.ranges[0].id,
                span: e.span,
            };
            e.kind = if old {
                ExprKind::OldField(access)
            } else {
                ExprKind::FieldAccess(access)
            };
            assert!(check_read(&p, &e)
                .unwrap_err()
                .message
                .contains("verified-state"));
        }
        let mut p = fixture();
        p.parameters[0].ty = ParameterType::MutableRecord(p.records[0].id);
        let mut e = read(&p);
        let ExprKind::CopyFieldRead { owner, .. } = &mut e.kind else {
            panic!()
        };
        *owner = Place::Parameter(p.parameters[0].id);
        assert!(check_read(&p, &e).is_err());
    }
    #[test]
    fn invariant_rejects_new_ordinary_nodes_at_verified_boundary() {
        let p = fixture();
        assert!(p
            .record_expression(p.functions[0].id(), &read(&p), false, false)
            .is_err());
        let ValueStatement::Return { value, .. } = &p.functions[0].as_ordinary().unwrap().body[2]
        else {
            panic!()
        };
        assert!(p
            .record_expression(p.functions[0].id(), value, false, false)
            .is_err());
    }
    #[test]
    fn invariant_rejects_construction_field_result_and_initializer_mismatches() {
        let p = fixture();
        let ValueStatement::Return { value, .. } = &p.functions[0].as_ordinary().unwrap().body[2]
        else {
            panic!()
        };
        for variant in 0..4 {
            let mut e = value.clone();
            let ExprKind::RecordConstruct {
                record,
                field,
                value,
            } = &mut e.kind
            else {
                panic!()
            };
            match variant {
                0 => *field = p.records[1].field,
                1 => *record = RecordId(999),
                2 => e.ty = ExprType::Bool,
                3 => value.ty = ExprType::Range(p.ranges[1].id),
                _ => unreachable!(),
            };
            assert!(check_read(&p, &e).is_err());
        }
    }
    fn construct(p: &Program, child: TypedExpr) -> TypedExpr {
        TypedExpr {
            ty: ExprType::Record(p.records[0].id),
            span: child.span,
            kind: ExprKind::RecordConstruct {
                record: p.records[0].id,
                field: p.records[0].field,
                value: Box::new(child),
            },
        }
    }
    #[test]
    fn invariant_follows_construction_child_and_rejects_hidden_conditionals_at_depth() {
        let p = fixture();
        let leaf = read(&p);
        let hidden = TypedExpr {
            kind: ExprKind::IfValue {
                condition: Box::new(TypedExpr {
                    kind: ExprKind::BoolLiteral(true),
                    ty: ExprType::Bool,
                    span: leaf.span,
                }),
                then_value: Box::new(leaf.clone()),
                else_value: Box::new(leaf.clone()),
            },
            ..leaf.clone()
        };
        for depth in 0..4 {
            let mut child = hidden.clone();
            for _ in 0..depth {
                child = TypedExpr {
                    kind: ExprKind::Call {
                        function: p.functions[0].id(),
                        arguments: vec![construct(&p, child)],
                    },
                    ..leaf.clone()
                };
            }
            assert!(check_read(&p, &construct(&p, child))
                .unwrap_err()
                .message
                .contains("complete local initializers"));
        }
    }
    #[test]
    fn invariant_visits_every_expression_and_statement_container() {
        let p = fixture();
        let function = p.functions[0].as_ordinary().unwrap();
        let mut bad = read(&p);
        bad.ty = ExprType::Record(p.records[0].id);
        let leaf = read(&p);
        let child = TypedExpr {
            kind: ExprKind::Call {
                function: function.id,
                arguments: vec![bad.clone()],
            },
            ..leaf.clone()
        };
        let constructed = construct(&p, child);
        let wrappers = [
            constructed.clone(),
            TypedExpr {
                kind: ExprKind::Call {
                    function: function.id,
                    arguments: vec![constructed.clone()],
                },
                ..leaf.clone()
            },
            TypedExpr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(constructed.clone()),
                },
                ..leaf.clone()
            },
            TypedExpr {
                kind: ExprKind::Binary {
                    op: BinaryOp::Equal,
                    left: Box::new(leaf.clone()),
                    right: Box::new(constructed.clone()),
                },
                ..leaf.clone()
            },
        ];
        for expression in wrappers {
            assert!(check_read(&p, &expression)
                .unwrap_err()
                .message
                .contains("exact declared named-range Copy"));
        }
        let local = p.locals[0].id;
        let span = leaf.span;
        let statements = [
            ValueStatement::Let {
                local,
                initializer: constructed.clone(),
                span,
            },
            ValueStatement::Assign {
                local,
                value: constructed.clone(),
                span,
            },
            ValueStatement::DerefAssign {
                reference: Place::Local(local),
                value: constructed.clone(),
                span,
            },
            ValueStatement::Return {
                value: constructed.clone(),
                span,
            },
            ValueStatement::CallNoValue {
                function: function.id,
                arguments: vec![constructed.clone()],
                span,
            },
            ValueStatement::While {
                condition: constructed.clone(),
                body: vec![],
                span,
            },
            ValueStatement::If {
                condition: constructed.clone(),
                then_body: vec![],
                else_body: vec![],
                span,
            },
        ];
        for statement in statements {
            assert!(p
                .record_statements(function.id, &[statement])
                .unwrap_err()
                .message
                .contains("exact declared named-range Copy"));
        }
        let mut p = fixture();
        let Function::Ordinary(f) = &mut p.functions[0] else {
            panic!()
        };
        f.body[1] = ValueStatement::Return { value: bad, span };
        assert!(p.validate_record_expressions().is_err());
    }
}
