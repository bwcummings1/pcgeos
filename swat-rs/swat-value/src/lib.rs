#![forbid(unsafe_code)]

use serde_json::Value as JsonValue;
use swat_core::{ArtifactEncoding, ArtifactRef, EventEnvelope, SwatError, SwatResult};
use swat_store::{StoredArtifact, SwatStore};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Text,
    Json,
    Binary,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedValueData {
    Text(String),
    Json(JsonValue),
    Binary(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedValue {
    pub artifact_ref: ArtifactRef,
    pub kind: ValueKind,
    pub data: DecodedValueData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValuePresentation {
    pub preview: String,
    pub detail: String,
    pub line_count: usize,
    pub byte_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueriedValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Json(String),
}

impl DecodedValue {
    pub fn preview(&self, limit: usize) -> String {
        truncate(self.render_compact(), limit)
    }

    pub fn detail(&self) -> String {
        match &self.data {
            DecodedValueData::Text(text) => text.clone(),
            DecodedValueData::Json(value) => {
                serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
            }
            DecodedValueData::Binary(bytes) => render_binary(bytes),
        }
    }

    pub fn presentation(&self, preview_limit: usize) -> ValuePresentation {
        let detail = self.detail();
        ValuePresentation {
            preview: self.preview(preview_limit),
            line_count: detail.lines().count().max(1),
            byte_len: self.byte_len(),
            detail,
        }
    }

    pub fn query_json_path(&self, path: &str) -> SwatResult<Option<QueriedValue>> {
        let DecodedValueData::Json(root) = &self.data else {
            return Ok(None);
        };

        let tokens = parse_json_path(path)?;
        let mut current = root;
        for token in tokens {
            match token {
                JsonPathToken::Field(field) => {
                    let Some(next) = current.get(&field) else {
                        return Ok(None);
                    };
                    current = next;
                }
                JsonPathToken::Index(index) => {
                    let Some(next) = current.get(index) else {
                        return Ok(None);
                    };
                    current = next;
                }
            }
        }

        Ok(Some(convert_queried_value(current)))
    }

    fn render_compact(&self) -> String {
        match &self.data {
            DecodedValueData::Text(text) => normalize_preview_text(text),
            DecodedValueData::Json(value) => normalize_preview_text(&value.to_string()),
            DecodedValueData::Binary(bytes) => compact_binary_preview(bytes),
        }
    }

    fn byte_len(&self) -> usize {
        match &self.data {
            DecodedValueData::Text(text) => text.len(),
            DecodedValueData::Json(value) => value.to_string().len(),
            DecodedValueData::Binary(bytes) => bytes.len(),
        }
    }
}

pub fn decode_artifact(artifact: StoredArtifact) -> SwatResult<DecodedValue> {
    let artifact_ref = artifact.artifact_ref.clone();
    let kind = detect_kind(&artifact);
    let data = match kind {
        ValueKind::Text => {
            DecodedValueData::Text(String::from_utf8(artifact.bytes).map_err(|err| {
                SwatError::new(format!(
                    "artifact {} is not valid utf-8: {err}",
                    artifact_ref.artifact_id.raw()
                ))
            })?)
        }
        ValueKind::Json => {
            let value = serde_json::from_slice::<JsonValue>(&artifact.bytes).map_err(|err| {
                SwatError::new(format!(
                    "artifact {} is not valid json: {err}",
                    artifact_ref.artifact_id.raw()
                ))
            })?;
            DecodedValueData::Json(value)
        }
        ValueKind::Binary => DecodedValueData::Binary(artifact.bytes),
    };

    Ok(DecodedValue {
        artifact_ref,
        kind,
        data,
    })
}

pub fn decode_event_artifacts<S: SwatStore + ?Sized>(
    store: &S,
    event: &EventEnvelope,
) -> SwatResult<Vec<DecodedValue>> {
    event
        .artifact_refs
        .iter()
        .map(|artifact_ref| {
            let artifact = store.artifact(artifact_ref.artifact_id).ok_or_else(|| {
                SwatError::new(format!(
                    "missing artifact {} for event {}",
                    artifact_ref.artifact_id.raw(),
                    event.event_id.raw()
                ))
            })?;
            decode_artifact(artifact)
        })
        .collect()
}

fn detect_kind(artifact: &StoredArtifact) -> ValueKind {
    if matches!(artifact.artifact_ref.encoding, ArtifactEncoding::Json)
        || artifact.artifact_ref.media_type.contains("json")
    {
        ValueKind::Json
    } else if matches!(artifact.artifact_ref.encoding, ArtifactEncoding::Utf8)
        || artifact.artifact_ref.media_type.starts_with("text/")
    {
        ValueKind::Text
    } else {
        ValueKind::Binary
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum JsonPathToken {
    Field(String),
    Index(usize),
}

fn parse_json_path(path: &str) -> SwatResult<Vec<JsonPathToken>> {
    let mut chars = path.chars().peekable();
    if matches!(chars.peek(), Some('$')) {
        chars.next();
    }

    let mut tokens = Vec::new();
    let mut current_field = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !current_field.is_empty() {
                    tokens.push(JsonPathToken::Field(std::mem::take(&mut current_field)));
                }
            }
            '[' => {
                if !current_field.is_empty() {
                    tokens.push(JsonPathToken::Field(std::mem::take(&mut current_field)));
                }
                let mut index = String::new();
                while let Some(next) = chars.next() {
                    if next == ']' {
                        break;
                    }
                    index.push(next);
                }
                let parsed = index.parse::<usize>().map_err(|err| {
                    SwatError::new(format!("invalid json path index '{index}': {err}"))
                })?;
                tokens.push(JsonPathToken::Index(parsed));
            }
            _ => current_field.push(ch),
        }
    }

    if !current_field.is_empty() {
        tokens.push(JsonPathToken::Field(current_field));
    }
    Ok(tokens)
}

fn convert_queried_value(value: &JsonValue) -> QueriedValue {
    match value {
        JsonValue::Null => QueriedValue::Null,
        JsonValue::Bool(value) => QueriedValue::Bool(*value),
        JsonValue::Number(value) => QueriedValue::Number(value.to_string()),
        JsonValue::String(value) => QueriedValue::String(value.clone()),
        other => QueriedValue::Json(other.to_string()),
    }
}

fn truncate(mut value: String, limit: usize) -> String {
    if value.len() > limit {
        value.truncate(limit);
        value.push_str("...");
    }
    value
}

fn normalize_preview_text(value: &str) -> String {
    value.replace('\n', "\\n")
}

fn compact_binary_preview(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "<0 bytes>".to_string();
    }

    let preview = bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > 8 {
        format!("<{} bytes: {} ...>", bytes.len(), preview)
    } else {
        format!("<{} bytes: {}>", bytes.len(), preview)
    }
}

fn render_binary(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "<0 bytes>".to_string();
    }

    bytes
        .chunks(16)
        .map(|chunk| {
            chunk
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
