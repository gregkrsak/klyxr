//! Deterministic control-flow lowering of checked ordinary HIR. No execution or proof.
use crate::hir::{self, FunctionId, LocalId, Place, TypedExpr, ValueStatement};
use crate::lexer::Span;

/// Compilation-local CFG identity, unrelated to source declaration identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BasicBlockId(usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    CallNoValue {
        function: FunctionId,
        arguments: Vec<TypedExpr>,
        span: Span,
    },
    Assign {
        local: LocalId,
        value: TypedExpr,
        span: Span,
    },
    Let {
        local: LocalId,
        initializer: TypedExpr,
        span: Span,
    },
    DerefAssign {
        reference: Place,
        value: TypedExpr,
        span: Span,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    ReturnNoValue,
    Goto {
        target: BasicBlockId,
    },
    Branch {
        condition: TypedExpr,
        then_target: BasicBlockId,
        else_target: BasicBlockId,
    },
    Return {
        value: TypedExpr,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlock {
    statements: Vec<Statement>,
    terminator: Terminator,
}
impl BasicBlock {
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }
    pub fn terminator(&self) -> &Terminator {
        &self.terminator
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFunction {
    function: FunctionId,
    entry: BasicBlockId,
    blocks: Vec<BasicBlock>,
}
impl MirFunction {
    pub fn function(&self) -> FunctionId {
        self.function
    }
    pub fn entry(&self) -> BasicBlockId {
        self.entry
    }
    pub fn blocks(&self) -> impl ExactSizeIterator<Item = (BasicBlockId, &BasicBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .map(|(i, block)| (BasicBlockId(i), block))
    }
    pub fn block(&self, id: BasicBlockId) -> &BasicBlock {
        &self.blocks[id.0]
    }
    /// Validate compiler structure, not source semantics or numerical obligations.
    pub fn validate(&self) -> Result<(), &'static str> {
        fn visit(f: &MirFunction, id: BasicBlockId, marks: &mut [u8]) -> Result<(), &'static str> {
            let Some(mark) = marks.get(id.0) else {
                return Err("invalid block target");
            };
            if *mark != 0 {
                return Ok(());
            }
            marks[id.0] = 1;
            match &f.block(id).terminator {
                Terminator::Goto { target } => visit(f, *target, marks)?,
                Terminator::Branch {
                    condition,
                    then_target,
                    else_target,
                } => {
                    if condition.ty != hir::ExprType::Bool {
                        return Err("non-Bool branch condition");
                    }
                    visit(f, *then_target, marks)?;
                    visit(f, *else_target, marks)?;
                }
                Terminator::Return { .. } | Terminator::ReturnNoValue => {}
            }
            marks[id.0] = 2;
            Ok(())
        }
        fn expression_valid(expression: &TypedExpr) -> Result<(), &'static str> {
            match &expression.kind {
                hir::ExprKind::IntegerLiteral(_) => {
                    if expression.ty != hir::ExprType::IntegerLiteral { return Err("raw integer literal has invalid type"); }
                }
                hir::ExprKind::FormedRangeLiteral(_) => {
                    if !matches!(expression.ty, hir::ExprType::Range(_)) { return Err("formed range literal has non-range type"); }
                }
                hir::ExprKind::IfValue { .. } => return Err("conditional value must lower to control flow"),
                hir::ExprKind::RecordConstruct { fields, .. } => { for entry in fields { expression_valid(&entry.value)?; } },
                hir::ExprKind::Call { arguments, .. } => for argument in arguments { expression_valid(argument)?; },
                hir::ExprKind::Unary { operand, .. } => expression_valid(operand)?,
                hir::ExprKind::Binary { left, right, .. } => { expression_valid(left)?; expression_valid(right)?; },
                hir::ExprKind::FieldAccess(_) | hir::ExprKind::OldField(_) => return Err("verified-state or residual projection representation is ineligible in ordinary MIR"),
                hir::ExprKind::CopyFieldRead { .. } | hir::ExprKind::Parameter(_) | hir::ExprKind::Local(_) | hir::ExprKind::Deref { .. } | hir::ExprKind::Borrow { .. } | hir::ExprKind::BoolLiteral(_) => {}
            }
            Ok(())
        }
        for block in &self.blocks {
            for statement in &block.statements {
                match statement {
                    Statement::CallNoValue { arguments, .. } => {
                        for argument in arguments {
                            expression_valid(argument)?;
                        }
                    }
                    Statement::Let { initializer, .. } => expression_valid(initializer)?,
                    Statement::DerefAssign { value, .. } | Statement::Assign { value, .. } => {
                        expression_valid(value)?
                    }
                }
            }
            match &block.terminator {
                Terminator::Branch { condition, .. } => expression_valid(condition)?,
                Terminator::Return { value } => expression_valid(value)?,
                Terminator::Goto { .. } | Terminator::ReturnNoValue => {}
            }
        }
        let mut marks = vec![0; self.blocks.len()];
        visit(self, self.entry, &mut marks)?;
        if marks.contains(&0) {
            return Err("unreachable block");
        }
        // Indexed storage gives unique IDs and one terminator per block. Cycles
        // are legal; structural validity makes no claim that Return is reached.
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirProgram {
    functions: Vec<MirFunction>,
}
impl MirProgram {
    pub fn functions(&self) -> &[MirFunction] {
        &self.functions
    }
}

/// Lower canonical ordinary HIR after frontend/ownership checks. Verified
/// functions retain their specialized proof path. Unsupported valid HIR is a
/// compiler defect, never a new user diagnostic phase.
pub fn lower(program: &hir::Program) -> MirProgram {
    let functions = program
        .functions()
        .iter()
        .filter_map(|f| f.as_ordinary())
        .map(|f| {
            let mut builder = Builder {
                blocks: Vec::new(),
                loop_targets: Vec::new(),
            };
            let entry = builder.reserve();
            if let Some((tail, statements)) = builder.body(entry, &f.body) {
                assert!(
                    f.return_type.is_none(),
                    "value functions have no closing-brace fallthrough"
                );
                builder.finish(tail, statements, Terminator::ReturnNoValue);
            }
            let function = MirFunction {
                function: f.id,
                entry,
                blocks: builder
                    .blocks
                    .into_iter()
                    .map(|b| b.expect("all reserved blocks finished"))
                    .collect(),
            };
            debug_assert_eq!(function.validate(), Ok(()));
            function
        })
        .collect();
    MirProgram { functions }
}
struct Builder {
    blocks: Vec<Option<BasicBlock>>,
    loop_targets: Vec<(BasicBlockId, BasicBlockId)>,
}
impl Builder {
    fn reserve(&mut self) -> BasicBlockId {
        let id = BasicBlockId(self.blocks.len());
        self.blocks.push(None);
        id
    }
    fn finish(&mut self, id: BasicBlockId, statements: Vec<Statement>, terminator: Terminator) {
        assert!(self.blocks[id.0].is_none(), "block finished once");
        self.blocks[id.0] = Some(BasicBlock {
            statements,
            terminator,
        });
    }
    // Each value leaf initializes the canonical destination once on its path.
    // Nested value branches share the destination's control-flow-only join.
    fn initialize(
        &mut self,
        current: BasicBlockId,
        mut statements: Vec<Statement>,
        local: LocalId,
        value: &TypedExpr,
        span: Span,
        join: BasicBlockId,
    ) {
        if let hir::ExprKind::IfValue {
            condition,
            then_value,
            else_value,
        } = &value.kind
        {
            let then_target = self.reserve();
            let else_target = self.reserve();
            self.finish(
                current,
                statements,
                Terminator::Branch {
                    condition: (**condition).clone(),
                    then_target,
                    else_target,
                },
            );
            self.initialize(then_target, Vec::new(), local, then_value, span, join);
            self.initialize(else_target, Vec::new(), local, else_value, span, join);
        } else {
            statements.push(Statement::Let {
                local,
                initializer: value.clone(),
                span,
            });
            self.finish(current, statements, Terminator::Goto { target: join });
        }
    }
    fn body(
        &mut self,
        mut current: BasicBlockId,
        body: &[ValueStatement],
    ) -> Option<(BasicBlockId, Vec<Statement>)> {
        let mut statements = Vec::new();
        for statement in body {
            match statement {
                ValueStatement::ReturnNoValue { .. } => {
                    self.finish(current, statements, Terminator::ReturnNoValue);
                    return None;
                }
                ValueStatement::CallNoValue {
                    function,
                    arguments,
                    span,
                } => {
                    statements.push(Statement::CallNoValue {
                        function: *function,
                        arguments: arguments.clone(),
                        span: *span,
                    });
                }
                ValueStatement::Break { .. } => {
                    let target = self.loop_targets.last().expect("checked innermost loop").1;
                    self.finish(current, statements, Terminator::Goto { target });
                    return None;
                }
                ValueStatement::Continue { .. } => {
                    let target = self.loop_targets.last().expect("checked innermost loop").0;
                    self.finish(current, statements, Terminator::Goto { target });
                    return None;
                }
                ValueStatement::Let {
                    local,
                    initializer,
                    span,
                } => {
                    if matches!(initializer.kind, hir::ExprKind::IfValue { .. }) {
                        let join = self.reserve();
                        self.initialize(
                            current,
                            std::mem::take(&mut statements),
                            *local,
                            initializer,
                            *span,
                            join,
                        );
                        current = join;
                    } else {
                        statements.push(Statement::Let {
                            local: *local,
                            initializer: initializer.clone(),
                            span: *span,
                        });
                    }
                }
                ValueStatement::Assign { local, value, span } => {
                    statements.push(Statement::Assign {
                        local: *local,
                        value: value.clone(),
                        span: *span,
                    })
                }
                ValueStatement::DerefAssign {
                    reference,
                    value,
                    span,
                } => statements.push(Statement::DerefAssign {
                    reference: *reference,
                    value: value.clone(),
                    span: *span,
                }),
                ValueStatement::While {
                    condition, body, ..
                } => {
                    let header = self.reserve();
                    let body_target = self.reserve();
                    let exit = self.reserve();
                    self.finish(
                        current,
                        std::mem::take(&mut statements),
                        Terminator::Goto { target: header },
                    );
                    self.finish(
                        header,
                        Vec::new(),
                        Terminator::Branch {
                            condition: condition.clone(),
                            then_target: body_target,
                            else_target: exit,
                        },
                    );
                    self.loop_targets.push((header, exit));
                    if let Some((end, body)) = self.body(body_target, body) {
                        self.finish(end, body, Terminator::Goto { target: header });
                    }
                    assert_eq!(self.loop_targets.pop(), Some((header, exit)));
                    current = exit;
                }
                ValueStatement::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    let then_target = self.reserve();
                    let else_target = if else_body.is_empty() {
                        None
                    } else {
                        Some(self.reserve())
                    };
                    let join = if falls_through(then_body) || falls_through(else_body) {
                        Some(self.reserve())
                    } else {
                        None
                    };
                    self.finish(
                        current,
                        std::mem::take(&mut statements),
                        Terminator::Branch {
                            condition: condition.clone(),
                            then_target,
                            else_target: else_target.or(join).expect("false edge"),
                        },
                    );
                    if let Some((end, body)) = self.body(then_target, then_body) {
                        self.finish(
                            end,
                            body,
                            Terminator::Goto {
                                target: join.expect("then fallthrough"),
                            },
                        );
                    }
                    if let Some(target) = else_target {
                        if let Some((end, body)) = self.body(target, else_body) {
                            self.finish(
                                end,
                                body,
                                Terminator::Goto {
                                    target: join.expect("else fallthrough"),
                                },
                            );
                        }
                    }
                    let join = join?;
                    current = join;
                }
                ValueStatement::Return { value, .. } => {
                    self.finish(
                        current,
                        statements,
                        Terminator::Return {
                            value: value.clone(),
                        },
                    );
                    return None;
                }
            }
        }
        Some((current, statements))
    }
}

// Structural source fallthrough only; while retains its real false exit.
fn falls_through(body: &[ValueStatement]) -> bool {
    match body.last() {
        Some(
            ValueStatement::Continue { .. }
            | ValueStatement::Break { .. }
            | ValueStatement::Return { .. }
            | ValueStatement::ReturnNoValue { .. },
        ) => false,
        Some(ValueStatement::If {
            then_body,
            else_body,
            ..
        }) => falls_through(then_body) || falls_through(else_body),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile_source, ownership, parse_source, resolve, types};
    #[test]
    fn assignment_identity_shape_and_loan_outcomes_ignore_metadata() {
        let source = "type Percent = range 0..100; fn next(value: Percent) -> Percent { return value; } fn f(flag: bool, value: Percent) -> Percent { let mut current = if flag { value } else { value }; let view = &current; if flag { let observed = *view; } else { current = next(current); } return current; }";
        let mut program = compile_source(source).unwrap();
        let original = lower(&program);
        for range in &mut program.ranges {
            range.name = "display".into();
        }
        for parameter in &mut program.parameters {
            parameter.name = "display".into();
        }
        for local in &mut program.locals {
            local.name = "display".into();
        }
        for function in &mut program.functions {
            if let hir::Function::Ordinary(f) = function {
                f.name = "display".into();
            }
        }
        ownership::check(&program).unwrap();
        assert_eq!(original, lower(&program));
        let source = source.replace("return current;", "return *view;");
        let mut program =
            types::check(resolve::resolve(&parse_source(&source).unwrap()).unwrap()).unwrap();
        let original = ownership::check(&program).unwrap_err();
        for local in &mut program.locals {
            local.name = "display".into();
        }
        let renamed = ownership::check(&program).unwrap_err();
        assert_eq!(original[0].span, renamed[0].span);
        assert!(renamed[0].message.contains("cannot assign"));
    }
    #[test]
    fn validator_rejects_hidden_if_value_in_assignment_rhs() {
        let program = compile_source("fn f(flag: bool) -> bool { let mut value = if flag { true } else { false }; value = true; return value; }").unwrap();
        let hir::ValueStatement::Let { initializer, .. } =
            &program.functions()[0].as_ordinary().unwrap().body[0]
        else {
            panic!()
        };
        let mut mir = lower(&program);
        for block in &mut mir.functions[0].blocks {
            for statement in &mut block.statements {
                if let Statement::Assign { value, .. } = statement {
                    *value = initializer.clone();
                }
            }
        }
        assert_eq!(
            mir.functions[0].validate(),
            Err("conditional value must lower to control flow")
        );
    }

    #[test]
    fn conditional_value_identity_and_ownership_ignore_display_names() {
        let source = "type Percent = range 0..100; record Ticket { value: Percent } fn id(a: Ticket) -> Ticket { return a; } fn f(flag: bool, a: Ticket, b: Ticket) -> Ticket { let chosen = if flag { id(a) } else { if flag { b } else { a } }; return chosen; }";
        let mut p = compile_source(source).unwrap();
        let original = lower(&p);
        for range in &mut p.ranges {
            range.name = "display".into();
        }
        for record in &mut p.records {
            record.name = "display".into();
        }
        for field in &mut p.fields {
            field.name = "display".into();
        }
        for parameter in &mut p.parameters {
            parameter.name = "display".into();
        }
        for local in &mut p.locals {
            local.name = "display".into();
        }
        for function in &mut p.functions {
            if let hir::Function::Ordinary(f) = function {
                f.name = "display".into();
            }
        }
        ownership::check(&p).unwrap();
        assert_eq!(original, lower(&p));
        let failed = source.replace("return chosen;", "return a;");
        let mut p =
            types::check(resolve::resolve(&parse_source(&failed).unwrap()).unwrap()).unwrap();
        let original = ownership::check(&p).unwrap_err();
        for parameter in &mut p.parameters {
            parameter.name = "display".into();
        }
        for local in &mut p.locals {
            local.name = "display".into();
        }
        let renamed = ownership::check(&p).unwrap_err();
        assert_eq!(original[0].span, renamed[0].span);
        assert!(original[0].message.contains("may have been moved"));
        assert!(renamed[0].message.contains("may have been moved"));
    }
    #[test]
    fn validator_rejects_hidden_conditional_value() {
        let p = compile_source("fn f(flag: bool) -> bool { let chosen = if flag { true } else { false }; return chosen; }").unwrap();
        let hir::ValueStatement::Let { initializer, .. } =
            &p.functions()[0].as_ordinary().unwrap().body[0]
        else {
            panic!()
        };
        let mut m = lower(&p);
        let f = &mut m.functions[0];
        let statement = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.statements)
            .next()
            .unwrap();
        let Statement::Let {
            initializer: value, ..
        } = statement
        else {
            panic!()
        };
        *value = initializer.clone();
        assert_eq!(
            f.validate(),
            Err("conditional value must lower to control flow")
        );
    }
    fn program() -> hir::Program {
        compile_source("type Percent = range 0..100; record Ticket { value: Percent } fn identity(value: Percent) -> Percent { return value; } fn example(flag: bool, value: Percent) -> Percent { if flag { let observed = identity(value); if flag { let copied = observed; } } else { let observed = value; } return value; }").unwrap()
    }
    #[test]
    fn canonical_ids_and_cfg_ignore_colliding_display_names() {
        let mut p = program();
        let original = lower(&p);
        for r in &mut p.ranges {
            r.name = "display".into();
        }
        for r in &mut p.records {
            r.name = "display".into();
        }
        for f in &mut p.fields {
            f.name = "display".into();
        }
        for parameter in &mut p.parameters {
            parameter.name = "display".into();
        }
        for local in &mut p.locals {
            local.name = "display".into();
        }
        for f in &mut p.functions {
            if let hir::Function::Ordinary(f) = f {
                f.name = "display".into();
            }
        }
        ownership::check(&p).unwrap();
        assert_eq!(original, lower(&p));
        assert_ne!(p.locals[0].id, p.locals[2].id);
        let f = &original.functions()[1];
        assert_eq!(f.function(), p.functions[1].id());
        let call = f
            .blocks()
            .flat_map(|(_, b)| b.statements())
            .find_map(|s| match s {
                Statement::Let {
                    initializer:
                        TypedExpr {
                            kind: hir::ExprKind::Call { function, .. },
                            ..
                        },
                    ..
                } => Some(*function),
                _ => None,
            })
            .unwrap();
        assert_eq!(call, p.functions[0].id());
    }
    #[test]
    fn conditional_move_diagnostic_outcome_ignores_names() {
        let ast = parse_source("type Percent = range 0..100; record Ticket { value: Percent } fn bad(flag: bool, ticket: Ticket) -> Ticket { if flag { let moved = ticket; } return ticket; }").unwrap();
        let mut p = types::check(resolve::resolve(&ast).unwrap()).unwrap();
        let before = ownership::check(&p).unwrap_err();
        for parameter in &mut p.parameters {
            parameter.name = "display".into();
        }
        for local in &mut p.locals {
            local.name = "display".into();
        }
        let after = ownership::check(&p).unwrap_err();
        assert_eq!(before[0].span, after[0].span);
        assert_eq!(before[0].known, after[0].known);
        assert!(after[0].message.contains("`display` may have been moved"));
    }
    #[test]
    fn validator_rejects_invalid_entry_target_and_unreachable_block() {
        let original = lower(&program()).functions()[1].clone();
        let mut f = original.clone();
        f.entry = BasicBlockId(f.blocks.len());
        assert_eq!(f.validate(), Err("invalid block target"));
        let mut f = original.clone();
        f.blocks[0].terminator = Terminator::Goto {
            target: BasicBlockId(f.blocks.len()),
        };
        assert_eq!(f.validate(), Err("invalid block target"));
        let mut f = original.clone();
        f.blocks.push(original.blocks.last().unwrap().clone());
        assert_eq!(f.validate(), Err("unreachable block"));
    }
    #[test]
    fn validator_rejects_non_boolean_branch_conditions() {
        let mut f = lower(&program()).functions()[1].clone();
        let Terminator::Branch { condition, .. } = &mut f.blocks[0].terminator else {
            panic!()
        };
        condition.ty = hir::ExprType::IntegerLiteral;
        assert_eq!(f.validate(), Err("non-Bool branch condition"));
    }
    #[test]
    fn validator_accepts_cycles_without_claiming_termination() {
        let p = compile_source("fn f() -> bool { return true; }").unwrap();
        let mut f = lower(&p).functions()[0].clone();
        f.blocks[0].terminator = Terminator::Goto { target: f.entry };
        assert_eq!(f.validate(), Ok(())); // No Return is required for structural validity.
        f.blocks[0].terminator = Terminator::Goto {
            target: BasicBlockId(1),
        };
        assert_eq!(f.validate(), Err("invalid block target"));
    }
    #[test]
    fn validator_checks_structure_of_nested_cyclic_cfgs() {
        let p = compile_source("fn f(flag: bool) -> bool { let mut current = flag; while current { while flag { current = flag; } current = false; } return current; }").unwrap();
        let original = lower(&p).functions()[0].clone();
        assert_eq!(original.validate(), Ok(()));
        let mut f = original.clone();
        f.blocks.push(original.blocks.last().unwrap().clone());
        assert_eq!(f.validate(), Err("unreachable block"));
        let mut f = original;
        let branch = f
            .blocks
            .iter_mut()
            .find_map(|b| match &mut b.terminator {
                Terminator::Branch { condition, .. } => Some(condition),
                _ => None,
            })
            .unwrap();
        branch.ty = hir::ExprType::IntegerLiteral;
        assert_eq!(f.validate(), Err("non-Bool branch condition"));
    }

    #[test]
    fn validator_inspects_each_no_value_argument_tree_for_hidden_conditional_values() {
        let p=compile_source("fn consume(first: bool, second: bool) {} fn f(flag: bool) { let chosen = if flag { true } else { false }; consume(flag, chosen); }").unwrap();
        let hir::ValueStatement::Let { initializer, .. } =
            &p.functions()[1].as_ordinary().unwrap().body[0]
        else {
            panic!()
        };
        for position in 0..2 {
            let mut m = lower(&p);
            let f = &mut m.functions[1];
            let statement = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.statements)
                .find(|s| matches!(s, Statement::CallNoValue { .. }))
                .unwrap();
            let Statement::CallNoValue { arguments, .. } = statement else {
                panic!()
            };
            arguments[position] = TypedExpr {
                kind: hir::ExprKind::Call {
                    function: p.functions()[0].id(),
                    arguments: vec![initializer.clone()],
                },
                ty: hir::ExprType::Bool,
                span: initializer.span,
            };
            assert_eq!(
                f.validate(),
                Err("conditional value must lower to control flow")
            );
        }
    }
    #[test]
    fn validator_no_value_completions_keep_target_reachability_and_bool_checks() {
        let p = compile_source("fn f(flag: bool) { if flag { return; } }").unwrap();
        let original = lower(&p);
        original.functions[0].validate().unwrap();
        let mut m = original.clone();
        m.functions[0].blocks[0].terminator = Terminator::Goto {
            target: BasicBlockId(999),
        };
        assert_eq!(m.functions[0].validate(), Err("invalid block target"));
        let mut m = original.clone();
        m.functions[0].blocks[0].terminator = Terminator::ReturnNoValue;
        assert_eq!(m.functions[0].validate(), Err("unreachable block"));
        let mut m = original;
        let Terminator::Branch { condition, .. } = &mut m.functions[0].blocks[0].terminator else {
            panic!()
        };
        condition.ty = hir::ExprType::IntegerLiteral;
        assert_eq!(m.functions[0].validate(), Err("non-Bool branch condition"));
    }

    #[test]
    fn validator_descends_construction_at_multiple_depths_in_every_mir_container() {
        let p=compile_source("type Percent = range 0..100; record Battery { charge: Percent } fn f(flag: bool,p: Percent) -> Battery { let chosen = if flag { p } else { p }; return Battery { charge: chosen }; }").unwrap();
        let f = p.functions()[0].as_ordinary().unwrap();
        let hir::ValueStatement::Let {
            initializer: hidden,
            local,
            span,
        } = &f.body[0]
        else {
            panic!()
        };
        let hir::ValueStatement::Return { value: record, .. } = &f.body[1] else {
            panic!()
        };
        for depth in 0..4 {
            let mut nested = hidden.clone();
            for _ in 0..depth {
                let mut construct = record.clone();
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut construct.kind else {
                    panic!()
                };
                fields[0].value = nested;
                nested = TypedExpr {
                    kind: hir::ExprKind::Call {
                        function: f.id,
                        arguments: vec![construct],
                    },
                    ..hidden.clone()
                };
            }
            let mut construct = record.clone();
            let hir::ExprKind::RecordConstruct { fields, .. } = &mut construct.kind else {
                panic!()
            };
            fields[0].value = nested;
            for statement in [
                Statement::Let {
                    local: *local,
                    initializer: construct.clone(),
                    span: *span,
                },
                Statement::Assign {
                    local: *local,
                    value: construct.clone(),
                    span: *span,
                },
                Statement::DerefAssign {
                    reference: Place::Local(*local),
                    value: construct.clone(),
                    span: *span,
                },
                Statement::CallNoValue {
                    function: f.id,
                    arguments: vec![construct.clone()],
                    span: *span,
                },
            ] {
                let m = MirFunction {
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![statement],
                        terminator: Terminator::ReturnNoValue,
                    }],
                };
                assert_eq!(
                    m.validate(),
                    Err("conditional value must lower to control flow")
                );
            }
            for terminator in [
                Terminator::Return {
                    value: construct.clone(),
                },
                Terminator::Branch {
                    condition: TypedExpr {
                        ty: hir::ExprType::Bool,
                        ..construct.clone()
                    },
                    then_target: BasicBlockId(1),
                    else_target: BasicBlockId(1),
                },
            ] {
                let m = MirFunction {
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![
                        BasicBlock {
                            statements: vec![],
                            terminator,
                        },
                        BasicBlock {
                            statements: vec![],
                            terminator: Terminator::ReturnNoValue,
                        },
                    ],
                };
                assert_eq!(
                    m.validate(),
                    Err("conditional value must lower to control flow")
                );
            }
        }
    }
    #[test]
    fn validator_rejects_residual_verified_projection_under_nested_construction() {
        let p=compile_source("type Percent = range 0..100; record Battery { charge: Percent } fn f(b: Battery) -> Battery { return Battery { charge: b.charge }; }").unwrap();
        let f = p.functions()[0].as_ordinary().unwrap();
        let hir::ValueStatement::Return { value: record, .. } = &f.body[0] else {
            panic!()
        };
        for old in [false, true] {
            let mut construct = record.clone();
            let hir::ExprKind::RecordConstruct { fields, .. } = &mut construct.kind else {
                panic!()
            };
            let access = hir::FieldAccess {
                parameter: f.parameters[0],
                field: p.fields()[0].id,
                ty: p.ranges()[0].id,
                span: fields[0].value.span,
            };
            fields[0].value.kind = if old {
                hir::ExprKind::OldField(access)
            } else {
                hir::ExprKind::FieldAccess(access)
            };
            let m = MirFunction {
                function: f.id,
                entry: BasicBlockId(0),
                blocks: vec![BasicBlock {
                    statements: vec![],
                    terminator: Terminator::Return { value: construct },
                }],
            };
            assert!(m.validate().unwrap_err().contains("residual projection"));
        }
    }
}

#[cfg(test)]
mod multiple_field_tests {
    use super::*;
    use crate::{compile_source, hir};
    #[test]
    fn mir_traverses_all_initializers_at_depth_in_every_expression_container() {
        let p=compile_source("type P = range 0..100; record Triple { a: P, b: P, c: P } fn f(flag: bool,p: P) -> Triple { let chosen = if flag { p } else { p }; return Triple { c: chosen, a: p, b: p }; }").unwrap();
        let f = p.functions()[0].as_ordinary().unwrap();
        let hir::ValueStatement::Let {
            local,
            initializer: hidden,
            span,
        } = &f.body[0]
        else {
            panic!()
        };
        let hir::ValueStatement::Return { value: record, .. } = &f.body[1] else {
            panic!()
        };
        for position in 0..3 {
            for depth in 0..3 {
                let mut nested = hidden.clone();
                for _ in 0..depth {
                    nested = TypedExpr {
                        kind: hir::ExprKind::Call {
                            function: f.id,
                            arguments: vec![nested],
                        },
                        ..hidden.clone()
                    };
                }
                let mut e = record.clone();
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut e.kind else {
                    panic!()
                };
                fields[position].value = nested;
                let statements = [
                    Statement::Let {
                        local: *local,
                        initializer: e.clone(),
                        span: *span,
                    },
                    Statement::Assign {
                        local: *local,
                        value: e.clone(),
                        span: *span,
                    },
                    Statement::DerefAssign {
                        reference: hir::Place::Local(*local),
                        value: e.clone(),
                        span: *span,
                    },
                    Statement::CallNoValue {
                        function: f.id,
                        arguments: vec![e.clone()],
                        span: *span,
                    },
                ];
                for statement in statements {
                    let m = MirFunction {
                        function: f.id,
                        entry: BasicBlockId(0),
                        blocks: vec![BasicBlock {
                            statements: vec![statement],
                            terminator: Terminator::ReturnNoValue,
                        }],
                    };
                    assert_eq!(
                        m.validate(),
                        Err("conditional value must lower to control flow")
                    );
                }
                let m = MirFunction {
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![],
                        terminator: Terminator::Return { value: e.clone() },
                    }],
                };
                assert_eq!(
                    m.validate(),
                    Err("conditional value must lower to control flow")
                );
                e.ty = hir::ExprType::Bool;
                let m = MirFunction {
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![
                        BasicBlock {
                            statements: vec![],
                            terminator: Terminator::Branch {
                                condition: e,
                                then_target: BasicBlockId(1),
                                else_target: BasicBlockId(1),
                            },
                        },
                        BasicBlock {
                            statements: vec![],
                            terminator: Terminator::ReturnNoValue,
                        },
                    ],
                };
                assert_eq!(
                    m.validate(),
                    Err("conditional value must lower to control flow")
                );
            }
        }
    }
    #[test]
    fn mir_rejects_residual_projections_in_first_middle_and_final_children() {
        let p=compile_source("type P = range 0..100; record Triple { a: P, b: P, c: P } fn f(p: P) -> Triple { return Triple { c: p, a: p, b: p }; }").unwrap();
        let f = p.functions()[0].as_ordinary().unwrap();
        let hir::ValueStatement::Return { value: record, .. } = &f.body[0] else {
            panic!()
        };
        for position in 0..3 {
            for old in [false, true] {
                let mut e = record.clone();
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut e.kind else {
                    panic!()
                };
                let access = hir::FieldAccess {
                    parameter: f.parameters[0],
                    field: p.records()[0].fields[position],
                    ty: p.ranges()[0].id,
                    span: e.span,
                };
                fields[position].value.kind = if old {
                    hir::ExprKind::OldField(access)
                } else {
                    hir::ExprKind::FieldAccess(access)
                };
                let m = MirFunction {
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![],
                        terminator: Terminator::Return { value: e },
                    }],
                };
                assert_eq!(m.validate(),Err("verified-state or residual projection representation is ineligible in ordinary MIR"));
            }
        }
    }
    #[test]
    fn literal_discriminator_is_preserved_and_validator_traverses_each_child() {
        let p=compile_source("type P = range 0..100; record R {a: P,b: P,c: P} fn id(p: P) -> P {return p;} fn f() -> R {return R {c: id(80),a: id(80),b: id(80)};}").unwrap();
        let f = p.functions().last().unwrap().as_ordinary().unwrap();
        let hir::ValueStatement::Return { value, .. } = &f.body[0] else {
            panic!()
        };
        let m = lower(&p);
        let lowered = m.functions().last().unwrap();
        assert_eq!(
            &lowered.blocks[0].terminator,
            &Terminator::Return {
                value: value.clone()
            }
        );
        lowered.validate().unwrap();
        for position in 0..3 {
            for raw in [false, true] {
                let mut e = value.clone();
                let hir::ExprKind::RecordConstruct { fields, .. } = &mut e.kind else {
                    panic!()
                };
                let hir::ExprKind::Call { arguments, .. } = &mut fields[position].value.kind else {
                    panic!()
                };
                if raw {
                    arguments[0].kind = hir::ExprKind::IntegerLiteral(80);
                } else {
                    arguments[0].ty = hir::ExprType::Bool;
                }
                let m = MirFunction {
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![],
                        terminator: Terminator::Return { value: e },
                    }],
                };
                assert_eq!(
                    m.validate(),
                    Err(if raw {
                        "raw integer literal has invalid type"
                    } else {
                        "formed range literal has non-range type"
                    })
                );
            }
        }
        // MIR has no canonical tables: shape-valid range tags are not bounds proofs.
        let e = hir::TypedExpr {
            kind: hir::ExprKind::FormedRangeLiteral(101),
            ty: hir::ExprType::Range(hir::RangeTypeId(usize::MAX)),
            span: value.span,
        };
        let m = MirFunction {
            function: f.id,
            entry: BasicBlockId(0),
            blocks: vec![BasicBlock {
                statements: vec![],
                terminator: Terminator::Return { value: e },
            }],
        };
        m.validate().unwrap();
    }

    #[test]
    fn raw_and_formed_validation_covers_expression_containers_and_all_mir_positions() {
        let p=compile_source("type P = range 0..100; fn id(p: P) -> P {return p;} fn f(p: P,r: &mut P) -> P {let mut x=p;return x;}").unwrap();
        let f = p.functions()[1].as_ordinary().unwrap();
        let span = f.span;
        for raw in [false, true] {
            let leaf = hir::TypedExpr {
                kind: if raw {
                    hir::ExprKind::IntegerLiteral(80)
                } else {
                    hir::ExprKind::FormedRangeLiteral(80)
                },
                ty: if raw {
                    hir::ExprType::Range(p.ranges()[0].id)
                } else {
                    hir::ExprType::IntegerLiteral
                },
                span,
            };
            let zero = hir::TypedExpr {
                kind: hir::ExprKind::IntegerLiteral(0),
                ty: hir::ExprType::IntegerLiteral,
                span,
            };
            let compare = hir::TypedExpr {
                kind: hir::ExprKind::Binary {
                    op: hir::BinaryOp::LessEqual,
                    left: Box::new(leaf.clone()),
                    right: Box::new(zero),
                },
                ty: hir::ExprType::Bool,
                span,
            };
            let unary = hir::TypedExpr {
                kind: hir::ExprKind::Unary {
                    op: hir::UnaryOp::Not,
                    operand: Box::new(compare.clone()),
                },
                ty: hir::ExprType::Bool,
                span,
            };
            let call = hir::TypedExpr {
                kind: hir::ExprKind::Call {
                    function: p.functions()[0].id(),
                    arguments: vec![leaf.clone()],
                },
                ty: hir::ExprType::Range(p.ranges()[0].id),
                span,
            };
            let error = if raw {
                "raw integer literal has invalid type"
            } else {
                "formed range literal has non-range type"
            };
            for e in [leaf, compare, unary, call] {
                for statement in [
                    Statement::Let {
                        local: p.locals()[0].id,
                        initializer: e.clone(),
                        span,
                    },
                    Statement::Assign {
                        local: p.locals()[0].id,
                        value: e.clone(),
                        span,
                    },
                    Statement::DerefAssign {
                        reference: hir::Place::Parameter(f.parameters[1]),
                        value: e.clone(),
                        span,
                    },
                    Statement::CallNoValue {
                        function: f.id,
                        arguments: vec![e.clone()],
                        span,
                    },
                ] {
                    let m = MirFunction {
                        function: f.id,
                        entry: BasicBlockId(0),
                        blocks: vec![BasicBlock {
                            statements: vec![statement],
                            terminator: Terminator::ReturnNoValue,
                        }],
                    };
                    assert_eq!(m.validate(), Err(error));
                }
                let m = MirFunction {
                    function: f.id,
                    entry: BasicBlockId(0),
                    blocks: vec![BasicBlock {
                        statements: vec![],
                        terminator: Terminator::Return { value: e.clone() },
                    }],
                };
                assert_eq!(m.validate(), Err(error));
                if e.ty == hir::ExprType::Bool {
                    let m = MirFunction {
                        function: f.id,
                        entry: BasicBlockId(0),
                        blocks: vec![
                            BasicBlock {
                                statements: vec![],
                                terminator: Terminator::Branch {
                                    condition: e,
                                    then_target: BasicBlockId(1),
                                    else_target: BasicBlockId(1),
                                },
                            },
                            BasicBlock {
                                statements: vec![],
                                terminator: Terminator::ReturnNoValue,
                            },
                        ],
                    };
                    assert_eq!(m.validate(), Err(error));
                }
            }
        }
    }
    #[test]
    fn formed_type_validation_reaches_first_middle_final_call_arguments() {
        let p=compile_source("type P = range 0..100; fn tri(a: P,b: P,c: P) -> P {return a;} fn f() -> P {return tri(80,80,80);}").unwrap();
        let mut m = lower(&p);
        let f = m.functions.last_mut().unwrap();
        for position in 0..3 {
            let mut candidate = f.clone();
            let Terminator::Return { value } = &mut candidate.blocks[0].terminator else {
                panic!()
            };
            let hir::ExprKind::Call { arguments, .. } = &mut value.kind else {
                panic!()
            };
            arguments[position].ty = hir::ExprType::Bool;
            assert_eq!(
                candidate.validate(),
                Err("formed range literal has non-range type")
            );
        }
    }
}
