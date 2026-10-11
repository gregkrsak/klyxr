//! Private CLI diagnostic boundary, also compiled into library unit tests so
//! fault injection exercises the same reporting paths used by `run`.
use crate::execution::{self, CheckedExecutable, ErrorKind, ExecutionError};
use std::io::Write;

pub(crate) fn preparation_error(
    error: &ExecutionError,
    allowance: usize,
    destination: impl Write,
) -> Result<(), i32> {
    report(
        error,
        execution::render_admission_error(error, allowance),
        destination,
    )
}

pub(crate) fn execution_error(
    artifact: &CheckedExecutable,
    error: &ExecutionError,
    allowance: usize,
    destination: impl Write,
) -> Result<(), i32> {
    report(error, artifact.render_error(error, allowance), destination)
}

fn report(
    original: &ExecutionError,
    rendered: Result<String, ExecutionError>,
    destination: impl Write,
) -> Result<(), i32> {
    let (text, kind) = match &rendered {
        Ok(text) => (text.as_str(), &original.kind),
        Err(error) => (
            match error.kind {
                ErrorKind::Host("render allocation") => {
                    "error: detected host/tool failure: render allocation [diagnostic unavailable]"
                }
                ErrorKind::Host(_) => {
                    "error: detected host/tool failure while rendering diagnostic [diagnostic unavailable]"
                }
                ErrorKind::ConfiguredLimit(_) => {
                    "error: diagnostic rendering allowance exhausted [truncated]"
                }
                _ => "error: diagnostic rendering integrity failure [diagnostic unavailable]",
            },
            &error.kind,
        ),
    };
    // Borrow the emergency message: failure reporting must not allocate another
    // String, retry the renderer, or replace a host failure with limit exhaustion.
    execution::write_output(destination, text).map_err(|_| 2)?;
    Err(if matches!(kind, ErrorKind::Host(_)) {
        2
    } else {
        1
    })
}
