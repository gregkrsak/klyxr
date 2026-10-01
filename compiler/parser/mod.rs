use std::fmt;

use crate::ast::{
    BinaryOp, Call, Expr, ExprKind, FieldAccess, Program, RangeType, RecordBinding, RecordDef,
    Statement, Subtract, UnaryOp, VerifiedFunction,
};
use crate::lexer::{Span, Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}:{}",
            self.message, self.span.line, self.span.column
        )
    }
}

impl std::error::Error for ParseError {}

pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    Parser { tokens, current: 0 }.parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    current: usize,
}

impl Parser<'_> {
    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut program = Program::default();

        while !self.at(&TokenKind::Eof) {
            if self.at(&TokenKind::Type) {
                program.ranges.push(self.parse_range_type()?);
            } else if self.at(&TokenKind::Record) {
                program.records.push(self.parse_record()?);
            } else if self.at(&TokenKind::Verified) {
                program.functions.push(self.parse_verified_function()?);
            } else if self.at(&TokenKind::Let) {
                program
                    .statements
                    .push(Statement::Binding(self.parse_binding()?));
            } else if matches!(self.peek_kind(), TokenKind::Ident(_))
                && self.lookahead_is(1, &TokenKind::LParen)
            {
                program.statements.push(Statement::Call(self.parse_call()?));
            } else {
                return Err(self.error(format!(
                    "unsupported or unexpected top-level token in prototype: {:?}",
                    self.peek_kind()
                )));
            }
        }

        Ok(program)
    }

    fn parse_range_type(&mut self) -> Result<RangeType, ParseError> {
        let span = self.expect(&TokenKind::Type)?.span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Equal)?;
        self.expect(&TokenKind::Range)?;
        let min = self.expect_number()?;
        self.expect(&TokenKind::DotDot)?;
        let max = self.expect_number()?;
        self.expect(&TokenKind::Semicolon)?;

        Ok(RangeType {
            name,
            min,
            max,
            span,
        })
    }

    fn parse_record(&mut self) -> Result<RecordDef, ParseError> {
        let span = self.expect(&TokenKind::Record)?.span;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let field_name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let field_type = self.expect_ident()?;
        self.expect(&TokenKind::RBrace)?;

        Ok(RecordDef {
            name,
            field_name,
            field_type,
            span,
        })
    }

    fn parse_verified_function(&mut self) -> Result<VerifiedFunction, ParseError> {
        let span = self.expect(&TokenKind::Verified)?.span;
        self.expect(&TokenKind::Fn)?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;

        let state_param = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Amp)?;
        self.expect(&TokenKind::Mut)?;
        let state_type = self.expect_ident()?;
        self.expect(&TokenKind::Comma)?;

        let amount_param = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let amount_type = self.expect_ident()?;
        self.expect(&TokenKind::RParen)?;

        self.expect(&TokenKind::Requires)?;
        let requires = self.parse_expression(0)?;
        self.expect(&TokenKind::Ensures)?;
        let ensures = self.parse_expression(0)?;

        self.expect(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            let target = self.parse_field_access()?;
            let statement_span = target.span;
            self.expect(&TokenKind::MinusEqual)?;
            let operand = self.parse_expression(0)?;
            self.expect(&TokenKind::Semicolon)?;
            body.push(Subtract {
                target,
                operand,
                span: statement_span,
            });
        }
        self.expect(&TokenKind::RBrace)?;

        Ok(VerifiedFunction {
            name,
            span,
            state_param,
            state_type,
            amount_param,
            amount_type,
            requires,
            ensures,
            body,
        })
    }

    // Precedence climbing: only the explicitly authorized operators participate.
    fn parse_expression(&mut self, minimum: u8) -> Result<Expr, ParseError> {
        let mut left = self.parse_primary()?;
        let mut comparison_seen = None;
        while let Some((op, precedence)) = self.binary_operator() {
            if precedence < minimum {
                break;
            }
            let comparison = matches!(op, BinaryOp::LessEqual | BinaryOp::Equal);
            if comparison && comparison_seen == Some(op) {
                return Err(
                    self.error("comparison operators are non-associative; use parentheses".into())
                );
            }
            self.advance();
            let right = self.parse_expression(precedence + 1)?;
            let span = Span {
                end: right.span.end,
                ..left.span
            };
            left = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
            if comparison {
                comparison_seen = Some(op);
            }
        }
        Ok(left)
    }
    fn binary_operator(&self) -> Option<(BinaryOp, u8)> {
        match self.peek_kind() {
            TokenKind::OrOr => Some((BinaryOp::Or, 1)),
            TokenKind::AndAnd => Some((BinaryOp::And, 2)),
            TokenKind::EqualEqual => Some((BinaryOp::Equal, 3)),
            TokenKind::LessEqual => Some((BinaryOp::LessEqual, 4)),
            TokenKind::Minus => Some((BinaryOp::Subtract, 5)),
            _ => None,
        }
    }
    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek().span;
        let kind = match self.peek_kind() {
            TokenKind::True | TokenKind::False => {
                let value = self.at(&TokenKind::True);
                self.advance();
                ExprKind::BoolLiteral(value)
            }
            TokenKind::Number(_) | TokenKind::Minus => {
                ExprKind::IntegerLiteral(self.expect_number()?)
            }
            TokenKind::Not => {
                self.advance();
                ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(self.parse_primary()?),
                }
            }
            TokenKind::Ident(_) => {
                if self.lookahead_is(1, &TokenKind::Dot) {
                    ExprKind::FieldAccess(self.parse_field_access()?)
                } else {
                    ExprKind::Name(self.expect_ident()?)
                }
            }
            TokenKind::Old => {
                self.advance();
                self.expect(&TokenKind::LParen)?;
                let inner = self.parse_expression(0)?;
                self.expect(&TokenKind::RParen)?;
                let ExprKind::FieldAccess(field) = inner.kind else {
                    return Err(ParseError {
                        span: inner.span,
                        message: "old(...) supports only the mutable state field in this prototype"
                            .into(),
                    });
                };
                ExprKind::OldField(field)
            }
            TokenKind::LParen => {
                self.advance();
                let mut expression = self.parse_expression(0)?;
                let end = self.expect(&TokenKind::RParen)?.span.end;
                expression.span = Span { end, ..start };
                return Ok(expression);
            }
            other => {
                return Err(self.error(format!("unsupported or malformed expression: {other:?}")))
            }
        };
        Ok(Expr {
            kind,
            span: Span {
                end: self.tokens[self.current - 1].span.end,
                ..start
            },
        })
    }

    fn parse_binding(&mut self) -> Result<RecordBinding, ParseError> {
        let span = self.expect(&TokenKind::Let)?.span;

        let mutable = self.at(&TokenKind::Mut);
        if mutable {
            self.advance();
        }

        let name = self.expect_ident()?;
        self.expect(&TokenKind::Equal)?;
        let record_type = self.expect_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let field_name = self.expect_ident()?;
        self.expect(&TokenKind::Colon)?;
        let value = self.expect_number()?;
        self.expect(&TokenKind::RBrace)?;
        self.expect(&TokenKind::Semicolon)?;

        Ok(RecordBinding {
            name,
            mutable,
            record_type,
            field_name,
            value,
            span,
        })
    }

    fn parse_call(&mut self) -> Result<Call, ParseError> {
        let span = self.peek().span;
        let function = self.expect_ident()?;

        self.expect(&TokenKind::LParen)?;
        self.expect(&TokenKind::Amp)?;
        self.expect(&TokenKind::Mut)?;
        let binding = self.expect_ident()?;
        self.expect(&TokenKind::Comma)?;
        let amount = self.expect_number()?;
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::Semicolon)?;

        Ok(Call {
            function,
            binding,
            amount,
            span,
        })
    }

    fn parse_field_access(&mut self) -> Result<FieldAccess, ParseError> {
        let span = self.peek().span;
        let binding = self.expect_ident()?;
        self.expect(&TokenKind::Dot)?;
        let field = self.expect_ident()?;
        let span = Span {
            end: self.tokens[self.current - 1].span.end,
            ..span
        };
        Ok(FieldAccess {
            binding,
            field,
            span,
        })
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(self.error(format!("expected identifier, found {other:?}"))),
        }
    }

    fn expect_number(&mut self) -> Result<i64, ParseError> {
        let span = self.peek().span;
        let negative = self.at(&TokenKind::Minus);
        if negative {
            self.advance();
        }
        match self.peek_kind().clone() {
            TokenKind::Number(value) => {
                self.advance();
                let value = if negative {
                    -(value as i128)
                } else {
                    value as i128
                };
                i64::try_from(value).map_err(|_| ParseError {
                    message: "integer literal is outside the prototype's signed i64 domain".into(),
                    span,
                })
            }
            other => Err(self.error(format!("expected integer, found {other:?}"))),
        }
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<Token, ParseError> {
        if self.at(expected) {
            Ok(self.advance().clone())
        } else {
            Err(self.error(format!(
                "unsupported or malformed prototype syntax: expected {expected:?}, found {:?}",
                self.peek_kind()
            )))
        }
    }

    fn at(&self, expected: &TokenKind) -> bool {
        same_variant(self.peek_kind(), expected)
    }

    fn lookahead_is(&self, offset: usize, expected: &TokenKind) -> bool {
        self.tokens
            .get(self.current + offset)
            .map(|token| same_variant(&token.kind, expected))
            .unwrap_or(false)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn advance(&mut self) -> &Token {
        let index = self.current;
        if !matches!(&self.tokens[index].kind, TokenKind::Eof) {
            self.current += 1;
        }
        &self.tokens[index]
    }

    fn error(&self, message: String) -> ParseError {
        ParseError {
            message,
            span: self.peek().span,
        }
    }
}

fn same_variant(left: &TokenKind, right: &TokenKind) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    const PROGRAM: &str = r#"
type Percent = range 0..100;

record Battery {
    charge: Percent
}

verified fn consume(
    battery: &mut Battery,
    amount: Percent
)
    requires amount <= battery.charge
    ensures battery.charge == old(battery.charge) - amount
{
    battery.charge -= amount;
}

let mut battery = Battery { charge: 80 };
consume(&mut battery, 90);
"#;

    #[test]
    fn parses_battery_vertical_slice() {
        let tokens = lex(PROGRAM).unwrap();
        let program = parse(&tokens).unwrap();

        assert_eq!(program.ranges[0].name, "Percent");
        assert!(matches!(
            &program.functions[0].requires.kind,
            ExprKind::Binary {
                op: BinaryOp::LessEqual,
                ..
            }
        ));
        assert!(
            matches!(&program.statements[0], Statement::Binding(binding) if binding.value == 80)
        );
        assert!(matches!(&program.statements[1], Statement::Call(call) if call.amount == 90));
        assert!(matches!(
            &program.functions[0].ensures.kind,
            ExprKind::Binary {
                op: BinaryOp::Equal,
                ..
            }
        ));
        assert_eq!(program.functions[0].body.len(), 1);
    }
}
