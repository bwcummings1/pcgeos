#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::{self, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_TRACE_PREFIX: &str = "__SWATAGENT__";
pub const CURRENT_AGENT_PROTOCOL_VERSION: &str = "0.1.0-alpha";

fn default_protocol_version() -> String {
    CURRENT_AGENT_PROTOCOL_VERSION.to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentProtocolError {
    UnsupportedVersion { found: String, expected: String },
    Json(String),
}

impl fmt::Display for AgentProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { found, expected } => write!(
                f,
                "unsupported agent protocol version {found}; expected {expected}"
            ),
            Self::Json(message) => f.write_str(message),
        }
    }
}

impl Error for AgentProtocolError {}

impl From<serde_json::Error> for AgentProtocolError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentEventKind {
    Planner,
    Model,
    Tool,
    State,
    Policy,
    Source,
    Schema,
    Lifecycle,
    Log,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentEventRecord {
    #[serde(default = "default_protocol_version")]
    pub protocol_version: String,
    pub kind: AgentEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub determinism: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(flatten)]
    pub attributes: BTreeMap<String, Value>,
}

impl Default for AgentEventRecord {
    fn default() -> Self {
        Self {
            protocol_version: default_protocol_version(),
            kind: AgentEventKind::Log,
            phase: None,
            name: None,
            summary: None,
            status: None,
            verdict: None,
            span_id: None,
            correlation_id: None,
            determinism: None,
            file: None,
            line: None,
            function: None,
            attributes: BTreeMap::new(),
        }
    }
}

impl AgentEventRecord {
    pub fn new(kind: AgentEventKind) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    pub fn planner(name: impl Into<String>) -> Self {
        Self::new(AgentEventKind::Planner).with_name(name)
    }

    pub fn model(name: impl Into<String>) -> Self {
        Self::new(AgentEventKind::Model).with_name(name)
    }

    pub fn tool(name: impl Into<String>) -> Self {
        Self::new(AgentEventKind::Tool).with_name(name)
    }

    pub fn state(name: impl Into<String>) -> Self {
        Self::new(AgentEventKind::State).with_name(name)
    }

    pub fn policy(name: impl Into<String>) -> Self {
        Self::new(AgentEventKind::Policy).with_name(name)
    }

    pub fn with_phase(mut self, phase: impl Into<String>) -> Self {
        self.phase = Some(phase.into());
        self
    }

    pub fn with_protocol_version(mut self, protocol_version: impl Into<String>) -> Self {
        self.protocol_version = protocol_version.into();
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    pub fn with_verdict(mut self, verdict: impl Into<String>) -> Self {
        self.verdict = Some(verdict.into());
        self
    }

    pub fn with_span_id(mut self, span_id: impl Into<String>) -> Self {
        self.span_id = Some(span_id.into());
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    pub fn with_determinism(mut self, determinism: impl Into<String>) -> Self {
        self.determinism = Some(determinism.into());
        self
    }

    pub fn with_source(
        mut self,
        file: impl Into<String>,
        line: u64,
        function: impl Into<String>,
    ) -> Self {
        self.file = Some(file.into());
        self.line = Some(line);
        self.function = Some(function.into());
        self
    }

    pub fn with_attribute_value(mut self, key: impl Into<String>, value: Value) -> Self {
        self.attributes.insert(key.into(), value);
        self
    }

    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            AgentEventKind::Planner => "planner",
            AgentEventKind::Model => "model",
            AgentEventKind::Tool => "tool",
            AgentEventKind::State => "state",
            AgentEventKind::Policy => "policy",
            AgentEventKind::Source => "source",
            AgentEventKind::Schema => "schema",
            AgentEventKind::Lifecycle => "lifecycle",
            AgentEventKind::Log => "log",
        }
    }

    pub fn phase(&self) -> Option<&str> {
        self.phase.as_deref()
    }

    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn verdict(&self) -> Option<&str> {
        self.verdict.as_deref()
    }

    pub fn span_id(&self) -> Option<&str> {
        self.span_id.as_deref()
    }

    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    pub fn determinism(&self) -> Option<&str> {
        self.determinism.as_deref()
    }

    pub fn is_terminal_phase(&self) -> bool {
        matches!(
            self.phase(),
            Some("end" | "finish" | "done" | "response" | "error")
        )
    }

    pub fn validate(&self) -> Result<(), AgentProtocolError> {
        validate_protocol_version(self.protocol_version())
    }
}

pub struct LineEmitter<W> {
    writer: W,
    prefix: String,
}

impl<W: Write> LineEmitter<W> {
    pub fn new(writer: W) -> Self {
        Self::with_prefix(writer, DEFAULT_TRACE_PREFIX)
    }

    pub fn with_prefix(writer: W, prefix: impl Into<String>) -> Self {
        Self {
            writer,
            prefix: prefix.into(),
        }
    }

    pub fn emit(&mut self, record: &AgentEventRecord) -> io::Result<()> {
        let line =
            encode_prefixed_line_with_prefix(record, &self.prefix).map_err(io::Error::other)?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

pub fn encode_prefixed_line(record: &AgentEventRecord) -> serde_json::Result<String> {
    encode_prefixed_line_with_prefix(record, DEFAULT_TRACE_PREFIX)
}

pub fn encode_prefixed_line_with_prefix(
    record: &AgentEventRecord,
    prefix: &str,
) -> serde_json::Result<String> {
    Ok(format!("{prefix}{}", serde_json::to_string(record)?))
}

pub fn parse_prefixed_line(line: &str) -> Option<serde_json::Result<AgentEventRecord>> {
    parse_prefixed_line_with_prefix(line, DEFAULT_TRACE_PREFIX)
}

pub fn parse_prefixed_line_with_prefix(
    line: &str,
    prefix: &str,
) -> Option<serde_json::Result<AgentEventRecord>> {
    let payload = line.strip_prefix(prefix)?;
    Some(serde_json::from_str(payload))
}

pub fn parse_validated_prefixed_line(
    line: &str,
) -> Option<Result<AgentEventRecord, AgentProtocolError>> {
    parse_validated_prefixed_line_with_prefix(line, DEFAULT_TRACE_PREFIX)
}

pub fn parse_validated_prefixed_line_with_prefix(
    line: &str,
    prefix: &str,
) -> Option<Result<AgentEventRecord, AgentProtocolError>> {
    let payload = line.strip_prefix(prefix)?;
    Some(
        serde_json::from_str::<AgentEventRecord>(payload)
            .map_err(AgentProtocolError::from)
            .and_then(|record| {
                record.validate()?;
                Ok(record)
            }),
    )
}

pub fn validate_protocol_version(version: &str) -> Result<(), AgentProtocolError> {
    if version == CURRENT_AGENT_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(AgentProtocolError::UnsupportedVersion {
            found: version.to_string(),
            expected: CURRENT_AGENT_PROTOCOL_VERSION.to_string(),
        })
    }
}
