use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Number(u64),
    Type,
    Range,
    Record,
    Verified,
    Fn,
    Requires,
    Ensures,
    Let,
    Mut,
    Old,
    True,
    False,
    Not,
    AndAnd,
    OrOr,
    Equal,
    EqualEqual,
    LessEqual,
    DotDot,
    MinusEqual,
    Minus,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Colon,
    Comma,
    Semicolon,
    Amp,
    Dot,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}:{}",
            self.message, self.span.line, self.span.column
        )
    }
}

impl std::error::Error for LexError {}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut column = 1usize;

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b' ' | b'\t' | b'\r' => {
                i += 1;
                column += 1;
            }
            b'\n' => {
                i += 1;
                line += 1;
                column = 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                    column += 1;
                }
            }
            b'0'..=b'9' => {
                let start = i;
                let start_column = column;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                    column += 1;
                }
                let text = &source[start..i];
                let value = text.parse::<u64>().map_err(|_| LexError {
                    message: format!("integer literal is out of range: {text}"),
                    span: Span {
                        line,
                        column: start_column,
                        start,
                        end: i,
                    },
                })?;
                tokens.push(Token {
                    kind: TokenKind::Number(value),
                    span: Span {
                        line,
                        column: start_column,
                        start,
                        end: i,
                    },
                });
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = i;
                let start_column = column;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                    column += 1;
                }

                let text = &source[start..i];
                let kind = match text {
                    "type" => TokenKind::Type,
                    "range" => TokenKind::Range,
                    "record" => TokenKind::Record,
                    "verified" => TokenKind::Verified,
                    "fn" | "function" => TokenKind::Fn,
                    "requires" => TokenKind::Requires,
                    "ensures" => TokenKind::Ensures,
                    "let" => TokenKind::Let,
                    "mut" | "mutable" => TokenKind::Mut,
                    "old" => TokenKind::Old,
                    "true" => TokenKind::True,
                    "false" => TokenKind::False,
                    _ => TokenKind::Ident(text.to_owned()),
                };

                tokens.push(Token {
                    kind,
                    span: Span {
                        line,
                        column: start_column,
                        start,
                        end: i,
                    },
                });
            }
            b'&' if i + 1 < bytes.len() && bytes[i + 1] == b'&' => {
                push_two(&mut tokens, TokenKind::AndAnd, line, column, i);
                i += 2;
                column += 2;
            }
            b'|' if i + 1 < bytes.len() && bytes[i + 1] == b'|' => {
                push_two(&mut tokens, TokenKind::OrOr, line, column, i);
                i += 2;
                column += 2;
            }
            b'!' => push_one(
                &mut tokens,
                TokenKind::Not,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b'<' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                push_two(&mut tokens, TokenKind::LessEqual, line, column, i);
                i += 2;
                column += 2;
            }
            b'.' if i + 1 < bytes.len() && bytes[i + 1] == b'.' => {
                push_two(&mut tokens, TokenKind::DotDot, line, column, i);
                i += 2;
                column += 2;
            }
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                push_two(&mut tokens, TokenKind::MinusEqual, line, column, i);
                i += 2;
                column += 2;
            }
            b'=' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                push_two(&mut tokens, TokenKind::EqualEqual, line, column, i);
                i += 2;
                column += 2;
            }
            b'=' => push_one(
                &mut tokens,
                TokenKind::Equal,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b'-' => push_one(
                &mut tokens,
                TokenKind::Minus,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b'{' => push_one(
                &mut tokens,
                TokenKind::LBrace,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b'}' => push_one(
                &mut tokens,
                TokenKind::RBrace,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b'(' => push_one(
                &mut tokens,
                TokenKind::LParen,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b')' => push_one(
                &mut tokens,
                TokenKind::RParen,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b':' => push_one(
                &mut tokens,
                TokenKind::Colon,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b',' => push_one(
                &mut tokens,
                TokenKind::Comma,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b';' => push_one(
                &mut tokens,
                TokenKind::Semicolon,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b'&' => push_one(
                &mut tokens,
                TokenKind::Amp,
                line,
                column,
                &mut i,
                &mut column,
            ),
            b'.' => push_one(
                &mut tokens,
                TokenKind::Dot,
                line,
                column,
                &mut i,
                &mut column,
            ),
            _ => {
                return Err(LexError {
                    message: format!("unexpected character `{}`", b as char),
                    span: Span {
                        line,
                        column,
                        start: i,
                        end: i + 1,
                    },
                });
            }
        }
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span {
            line,
            column,
            start: source.len(),
            end: source.len(),
        },
    });

    Ok(tokens)
}

fn push_one(
    tokens: &mut Vec<Token>,
    kind: TokenKind,
    line: usize,
    column: usize,
    i: &mut usize,
    current_column: &mut usize,
) {
    let start = *i;
    tokens.push(Token {
        kind,
        span: Span {
            line,
            column,
            start,
            end: start + 1,
        },
    });
    *i += 1;
    *current_column += 1;
}

fn push_two(tokens: &mut Vec<Token>, kind: TokenKind, line: usize, column: usize, start: usize) {
    tokens.push(Token {
        kind,
        span: Span {
            line,
            column,
            start,
            end: start + 2,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_compact_and_explicit_keyword_aliases() {
        let compact = lex("fn f(x: &mut T) {}").unwrap();
        let explicit = lex("function f(x: &mutable T) {}").unwrap();

        let compact_kinds: Vec<_> = compact.into_iter().map(|token| token.kind).collect();
        let explicit_kinds: Vec<_> = explicit.into_iter().map(|token| token.kind).collect();

        assert_eq!(compact_kinds, explicit_kinds);
    }

    #[test]
    fn lexes_range_and_precondition_operators() {
        let tokens = lex("type Percent = range 0..100; requires amount <= battery.charge").unwrap();

        assert!(tokens.iter().any(|token| token.kind == TokenKind::DotDot));
        assert!(tokens
            .iter()
            .any(|token| token.kind == TokenKind::LessEqual));
    }
}
