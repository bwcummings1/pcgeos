#![forbid(unsafe_code)]

use std::fs;
use std::path::Path;

use swat_core::{EventEnvelope, SwatError, SwatResult};
use swat_store::SwatStore;
use swat_value::{QueriedValue, decode_event_artifacts};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub function: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLine {
    pub line_number: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSnippet {
    pub location: SourceLocation,
    pub start_line: usize,
    pub end_line: usize,
    pub focus_line: usize,
    pub lines: Vec<SourceLine>,
}

pub fn extract_event_source_location<S: SwatStore + ?Sized>(
    store: &S,
    event: &EventEnvelope,
) -> SwatResult<Option<SourceLocation>> {
    for decoded in decode_event_artifacts(store, event)? {
        let file = decoded.query_json_path("$.file")?;
        let line = decoded.query_json_path("$.line")?;
        let function = decoded.query_json_path("$.function")?;

        let Some(QueriedValue::String(file)) = file else {
            continue;
        };
        let Some(line) = line else {
            continue;
        };
        let line = match line {
            QueriedValue::Number(value) => value
                .parse::<usize>()
                .map_err(|err| SwatError::new(format!("invalid source line '{value}': {err}")))?,
            _ => continue,
        };
        let function = match function {
            Some(QueriedValue::String(value)) => Some(value),
            _ => None,
        };

        return Ok(Some(SourceLocation {
            file,
            line,
            function,
        }));
    }

    Ok(None)
}

pub fn load_source_snippet(
    location: SourceLocation,
    before: usize,
    after: usize,
) -> SwatResult<SourceSnippet> {
    if location.file.starts_with('<') {
        return Err(SwatError::new(format!(
            "cannot resolve synthetic source file {}",
            location.file
        )));
    }

    let contents = fs::read_to_string(&location.file).map_err(|err| {
        SwatError::new(format!(
            "failed to read source file {}: {err}",
            location.file
        ))
    })?;
    let all_lines = contents
        .lines()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if location.line == 0 {
        return Err(SwatError::new("source location line numbers are 1-based"));
    }

    let start_line = location.line.saturating_sub(before).max(1);
    let end_line = (location.line + after).min(all_lines.len());
    let focus_line = location.line;
    let lines = (start_line..=end_line)
        .map(|line_number| SourceLine {
            line_number,
            text: all_lines.get(line_number - 1).cloned().unwrap_or_default(),
        })
        .collect();

    Ok(SourceSnippet {
        location,
        start_line,
        end_line,
        focus_line,
        lines,
    })
}

pub fn resolve_event_source<S: SwatStore + ?Sized>(
    store: &S,
    event: &EventEnvelope,
    before: usize,
    after: usize,
) -> SwatResult<Option<SourceSnippet>> {
    let Some(location) = extract_event_source_location(store, event)? else {
        return Ok(None);
    };
    Ok(Some(load_source_snippet(location, before, after)?))
}

pub fn is_real_source_path(path: &str) -> bool {
    !path.starts_with('<') && Path::new(path).exists()
}
