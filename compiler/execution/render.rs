use super::*;
use std::{
    fmt::{self, Write as _},
    io::Write,
};

#[cfg(test)]
thread_local! {
    static FAIL_ALLOCATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Inject failure at the actual renderer growth boundary, isolated to this test
/// thread. The guard restores the previous setting even if a test unwinds.
#[cfg(test)]
pub(super) fn with_allocation_failure<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAIL_ALLOCATION.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(FAIL_ALLOCATION.with(|flag| flag.replace(true)));
    f()
}

struct Output {
    text: String,
    limit: usize,
    failure: Option<ErrorKind>,
}
impl Output {
    fn new(limit: usize) -> Result<Self, ExecutionError> {
        if limit == 0 || limit > ExecutionLimits::MAX.rendered_output {
            return Err(ResourceFailure::limit("rendered output override", START).into());
        }
        Ok(Self {
            text: String::new(),
            limit,
            failure: None,
        })
    }
    fn finish(self) -> Result<String, ExecutionError> {
        match self.failure {
            Some(k) => Err(ExecutionError::new(k, START)),
            None => Ok(self.text),
        }
    }
}
impl fmt::Write for Output {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.failure.is_some() {
            return Err(fmt::Error);
        }
        if self
            .text
            .len()
            .checked_add(s.len())
            .map_or(true, |n| n > self.limit)
        {
            self.failure = Some(ErrorKind::ConfiguredLimit("rendered output"));
            return Err(fmt::Error);
        }
        #[cfg(test)]
        if s.len() > self.text.capacity() - self.text.len()
            && FAIL_ALLOCATION.with(std::cell::Cell::get)
        {
            self.failure = Some(ErrorKind::Host("render allocation"));
            return Err(fmt::Error);
        }
        if self.text.try_reserve_exact(s.len()).is_err() {
            self.failure = Some(ErrorKind::Host("render allocation"));
            return Err(fmt::Error);
        }
        self.text.push_str(s);
        Ok(())
    }
}
fn range(out: &mut Output, e: &CheckedExecutable, v: &RangeValue) -> Result<(), ExecutionError> {
    let r = e
        .program
        .ranges()
        .get(v.ty.0)
        .filter(|r| r.id == v.ty && v.value >= r.min && v.value <= r.max)
        .ok_or_else(|| ExecutionError::integrity("render range identity or bounds"))?;
    let _ = write!(out, "{}({})", r.name, v.value);
    Ok(())
}
pub(super) fn result(
    e: &CheckedExecutable,
    r: &ExecutionResult,
    limit: usize,
) -> Result<String, ExecutionError> {
    let mut out = Output::new(limit)?;
    match r {
        ExecutionResult::NoValue => {
            let _ = write!(out, "NoValue");
        }
        ExecutionResult::Value(RuntimeValue::Bool(v)) => {
            let _ = write!(out, "Bool({v})");
        }
        ExecutionResult::Value(RuntimeValue::Range(v)) => range(&mut out, e, v)?,
        ExecutionResult::Value(RuntimeValue::Record { record, fields }) => {
            let r = e
                .program
                .records()
                .get(record.0)
                .filter(|r| r.id == *record && r.fields.len() == fields.len())
                .ok_or_else(|| ExecutionError::integrity("render record identity"))?;
            let _ = write!(out, "{} {{ ", r.name);
            for (i, id) in r.fields.iter().enumerate() {
                let f = e
                    .program
                    .fields()
                    .get(id.0)
                    .filter(|f| f.id == *id && f.record == *record)
                    .ok_or_else(|| ExecutionError::integrity("render field identity"))?;
                let mut found = fields.iter().filter(|(id, _)| *id == f.id);
                let (_, v) = found
                    .next()
                    .ok_or_else(|| ExecutionError::integrity("render field missing"))?;
                if found.next().is_some() || Some(v.ty) != f.range_type() {
                    return Err(ExecutionError::integrity("render field nominal mismatch"));
                }
                if i != 0 {
                    let _ = write!(out, ", ");
                }
                let _ = write!(out, "{}: ", f.name);
                range(&mut out, e, v)?;
                if out.failure.is_some() {
                    break;
                }
            }
            let _ = write!(out, " }}");
        }
    }
    out.finish()
}
pub(super) fn error(
    e: Option<&CheckedExecutable>,
    error: &ExecutionError,
    limit: usize,
) -> Result<String, ExecutionError> {
    let mut out = Output::new(limit)?;
    let _ = write!(out, "error: ");
    match &error.kind {
        ErrorKind::RangeTrap {
            ty,
            min,
            max,
            computed,
        } => {
            let name = e
                .and_then(|e| e.program.ranges().get(ty.0))
                .filter(|r| r.id == *ty)
                .map(|r| r.name.as_str())
                .unwrap_or("invalid range");
            let _ = write!(
                out,
                "named-range trap: {name}, inclusive {min}..{max}, computed {computed}"
            );
        }
        ErrorKind::ConfiguredLimit(n) => {
            let _ = write!(
                out,
                "configured tool limit exhausted: {n} (not a nontermination proof)"
            );
        }
        ErrorKind::Integrity(n) => {
            let _ = write!(out, "execution integrity failure: {n}");
        }
        ErrorKind::Host(n) => {
            let _ = write!(out, "detected host/tool failure: {n}");
        }
        ErrorKind::Frontend { phase, message } => {
            let _ = write!(out, "{phase}: {message}");
        }
    }
    let _ = write!(
        out,
        "\n  operation at {}:{}",
        error.origin.line, error.origin.column
    );
    if let Some(s) = error.expression {
        let _ = write!(out, "\n  expression at {}:{}", s.line, s.column);
    }
    for trace in &error.trace {
        let name = e
            .and_then(|e| e.program.functions().get(trace.function.0))
            .filter(|f| f.id() == trace.function)
            .map(|f| f.name())
            .unwrap_or("invalid function");
        let _ = write!(
            out,
            "\n  call {name} at {}:{}",
            trace.call_site.line, trace.call_site.column
        );
        if out.failure.is_some() {
            break;
        }
    }
    if error.trace_truncated {
        let _ = write!(out, "\n  [call trace truncated]");
    }
    if matches!(out.failure, Some(ErrorKind::ConfiguredLimit(_))) {
        // Fixed bounded fallback, rather than reconstructing an oversized error.
        let mut fallback = Output::new(limit)?;
        let _ = write!(
            fallback,
            "error: diagnostic truncated at {}:{}",
            error.origin.line, error.origin.column
        );
        return fallback.finish();
    }
    out.finish()
}
/// Output failure is a host/tool outcome, never configured fuel exhaustion.
pub fn write_output(mut destination: impl Write, text: &str) -> Result<(), ExecutionError> {
    destination
        .write_all(text.as_bytes())
        .and_then(|()| destination.write_all(b"\n"))
        .map_err(|_| ExecutionError::new(ErrorKind::Host("output write"), START))
}
