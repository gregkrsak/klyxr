use std::fmt::Write;

use crate::lexer::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub required: String,
    pub known: Vec<String>,
    pub conclusion: String,
}

impl Diagnostic {
    pub fn render(&self, path: &str, source: &str) -> String {
        let line_text = source
            .lines()
            .nth(self.span.line.saturating_sub(1))
            .unwrap_or("");

        let gutter_width = self.span.line.to_string().len();
        let mut out = String::new();

        let _ = writeln!(out, "error: {}", self.message);
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "  --> {path}:{}:{}",
            self.span.line, self.span.column
        );
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<width$} | {}",
            self.span.line,
            line_text,
            width = gutter_width
        );
        let _ = writeln!(
            out,
            "{:>width$} | {}^",
            "",
            " ".repeat(self.span.column.saturating_sub(1)),
            width = gutter_width
        );
        let _ = writeln!(out);
        let _ = writeln!(out, "required:");
        let _ = writeln!(out, "    {}", self.required);
        let _ = writeln!(out);
        let _ = writeln!(out, "known:");
        for fact in &self.known {
            let _ = writeln!(out, "    {fact}");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", self.conclusion);

        out
    }
}
