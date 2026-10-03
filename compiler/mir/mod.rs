//! Deterministic control-flow lowering of checked ordinary HIR. No execution or proof.
use crate::hir::{self, FunctionId, LocalId, Place, TypedExpr, ValueStatement};
use crate::lexer::Span;

/// Compilation-local CFG identity, unrelated to source declaration identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BasicBlockId(usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
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
            if *mark == 1 {
                return Err("cyclic control flow");
            }
            if *mark == 2 {
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
                Terminator::Return { .. } => {}
            }
            marks[id.0] = 2;
            Ok(())
        }
        fn contains_conditional(expression: &TypedExpr) -> bool {
            match &expression.kind {
                hir::ExprKind::IfValue { .. } => true,
                hir::ExprKind::Call { arguments, .. } => arguments.iter().any(contains_conditional),
                hir::ExprKind::Unary { operand, .. } => contains_conditional(operand),
                hir::ExprKind::Binary { left, right, .. } => {
                    contains_conditional(left) || contains_conditional(right)
                }
                _ => false,
            }
        }
        for block in &self.blocks {
            for statement in &block.statements {
                let expression = match statement {
                    Statement::Let { initializer, .. } => initializer,
                    Statement::DerefAssign { value, .. } | Statement::Assign { value, .. } => value,
                };
                if contains_conditional(expression) {
                    return Err("conditional value must lower to control flow");
                }
            }
            let expression = match &block.terminator {
                Terminator::Branch { condition, .. } => Some(condition),
                Terminator::Return { value } => Some(value),
                Terminator::Goto { .. } => None,
            };
            if expression.is_some_and(contains_conditional) {
                return Err("conditional value must lower to control flow");
            }
        }
        let mut marks = vec![0; self.blocks.len()];
        visit(self, self.entry, &mut marks)?;
        if marks.contains(&0) {
            return Err("unreachable block");
        }
        // An acyclic finite graph whose only leaves are Return reaches Return on
        // every path. Table indices give unique IDs; each block owns one terminator.
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
            let mut builder = Builder { blocks: Vec::new() };
            let entry = builder.reserve();
            let (last, statements) = builder.body(entry, &f.body[..f.body.len() - 1]);
            let ValueStatement::Return { value, .. } = f.body.last().expect("checked final return")
            else {
                unreachable!("checked final return")
            };
            builder.finish(
                last,
                statements,
                Terminator::Return {
                    value: value.clone(),
                },
            );
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
    ) -> (BasicBlockId, Vec<Statement>) {
        let mut statements = Vec::new();
        for statement in body {
            match statement {
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
                    let join = self.reserve();
                    self.finish(
                        current,
                        std::mem::take(&mut statements),
                        Terminator::Branch {
                            condition: condition.clone(),
                            then_target,
                            else_target: else_target.unwrap_or(join),
                        },
                    );
                    let (end, body) = self.body(then_target, then_body);
                    self.finish(end, body, Terminator::Goto { target: join });
                    if let Some(target) = else_target {
                        let (end, body) = self.body(target, else_body);
                        self.finish(end, body, Terminator::Goto { target: join });
                    }
                    current = join;
                }
                ValueStatement::Return { .. } => {
                    unreachable!("no branch-local returns in checked HIR")
                }
            }
        }
        (current, statements)
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
    fn validator_rejects_invalid_entry_target_cycle_and_unreachable_block() {
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
        f.blocks[0].terminator = Terminator::Goto { target: f.entry };
        assert_eq!(f.validate(), Err("cyclic control flow"));
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
}
