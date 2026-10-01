pub mod ast;
pub mod diagnostics;
pub mod lexer;
pub mod parser;
pub mod verify;

use std::fmt;

use ast::Program;

#[derive(Debug)]
pub enum FrontendError {
    Lex(lexer::LexError),
    Parse(parser::ParseError),
}

impl fmt::Display for FrontendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => write!(f, "{error}"),
            Self::Parse(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for FrontendError {}

pub fn compile_source(source: &str) -> Result<Program, FrontendError> {
    let tokens = lexer::lex(source).map_err(FrontendError::Lex)?;
    parser::parse(&tokens).map_err(FrontendError::Parse)
}
