pub mod ast;
pub mod diagnostics;
pub mod hir;
pub mod lexer;
pub mod parser;
pub mod resolve;
pub mod types;
pub mod verify;

use std::fmt;

use ast::Program;

#[derive(Debug)]
pub enum FrontendError {
    Lex(lexer::LexError),
    Parse(parser::ParseError),
    Resolve(Vec<diagnostics::Diagnostic>),
    Type(Vec<diagnostics::Diagnostic>),
}

impl fmt::Display for FrontendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => write!(f, "{error}"),
            Self::Parse(error) => write!(f, "{error}"),
            Self::Resolve(errors) | Self::Type(errors) => {
                for (index, error) in errors.iter().enumerate() {
                    if index > 0 {
                        writeln!(f)?;
                    }
                    write!(
                        f,
                        "{} at {}:{}",
                        error.message, error.span.line, error.span.column
                    )?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for FrontendError {}

/// Parse source syntax without resolving declaration identity.
pub fn parse_source(source: &str) -> Result<Program, FrontendError> {
    let tokens = lexer::lex(source).map_err(FrontendError::Lex)?;
    parser::parse(&tokens).map_err(FrontendError::Parse)
}

/// Parse, resolve, and type-check the supported subset to canonical typed HIR.
/// Numerical proof and ordered call-state checking are separate verifier work.
pub fn compile_source(source: &str) -> Result<hir::Program, FrontendError> {
    let ast = parse_source(source)?;
    let resolved = resolve::resolve(&ast).map_err(FrontendError::Resolve)?;
    types::check(resolved).map_err(FrontendError::Type)
}
