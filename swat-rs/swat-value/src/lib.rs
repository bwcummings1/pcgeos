#![forbid(unsafe_code)]

use std::collections::BTreeSet;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatientArtifactRecord {
    pub key: String,
    pub name: String,
    pub identifier: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
    pub runtime: Option<String>,
    pub path: Option<String>,
    pub is_default: Option<bool>,
    pub handle_ids: Vec<String>,
    pub resource_names: Vec<String>,
    pub object_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleArtifactRecord {
    pub key: String,
    pub name: Option<String>,
    pub patient: Option<String>,
    pub owner: Option<String>,
    pub resource: Option<String>,
    pub kind: Option<String>,
    pub address: Option<String>,
    pub segment: Option<String>,
    pub size: Option<u64>,
    pub attached: Option<bool>,
    pub state_flags: Vec<String>,
    pub object_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceArtifactRecord {
    pub key: String,
    pub name: String,
    pub identifier: Option<String>,
    pub patient: Option<String>,
    pub handle: Option<String>,
    pub kind: Option<String>,
    pub source_file: Option<String>,
    pub object_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectArtifactRecord {
    pub key: String,
    pub name: Option<String>,
    pub class_name: Option<String>,
    pub patient: Option<String>,
    pub handle: Option<String>,
    pub resource: Option<String>,
    pub address: Option<String>,
    pub state_flags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TypedEntityArtifacts {
    pub patients: Vec<PatientArtifactRecord>,
    pub handles: Vec<HandleArtifactRecord>,
    pub resources: Vec<ResourceArtifactRecord>,
    pub objects: Vec<ObjectArtifactRecord>,
}

impl TypedEntityArtifacts {
    pub fn extend(&mut self, other: Self) {
        self.patients.extend(other.patients);
        self.handles.extend(other.handles);
        self.resources.extend(other.resources);
        self.objects.extend(other.objects);
    }
}

impl DecodedValue {
    pub fn as_json(&self) -> Option<&JsonValue> {
        match &self.data {
            DecodedValueData::Json(value) => Some(value),
            _ => None,
        }
    }

    pub fn typed_entities(&self) -> TypedEntityArtifacts {
        self.as_json().map(extract_typed_entities).unwrap_or_default()
    }

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

#[derive(Clone, Debug, Default)]
struct EntityDefaults {
    patient: Option<String>,
    handle: Option<String>,
    resource: Option<String>,
}

fn extract_typed_entities(root: &JsonValue) -> TypedEntityArtifacts {
    let mut entities = TypedEntityArtifacts::default();

    for raw in entity_values(root, "patient", "patients", Some("patient")) {
        if let Some(patient) = parse_patient_record(raw) {
            let defaults = EntityDefaults {
                patient: Some(patient.key.clone()),
                ..EntityDefaults::default()
            };
            entities.patients.push(patient);
            collect_handles(raw, &defaults, &mut entities);
            collect_resources(raw, &defaults, &mut entities);
            collect_objects(raw, &defaults, &mut entities);
        }
    }

    collect_handles(root, &EntityDefaults::default(), &mut entities);
    collect_resources(root, &EntityDefaults::default(), &mut entities);
    collect_objects(root, &EntityDefaults::default(), &mut entities);

    entities
}

fn collect_handles(container: &JsonValue, defaults: &EntityDefaults, entities: &mut TypedEntityArtifacts) {
    for raw in entity_values(container, "handle", "handles", Some("handle")) {
        if let Some(handle) = parse_handle_record(raw, defaults) {
            let nested_defaults = EntityDefaults {
                patient: handle.patient.clone().or_else(|| defaults.patient.clone()),
                handle: Some(handle.key.clone()),
                resource: handle.resource.clone().or_else(|| defaults.resource.clone()),
            };
            entities.handles.push(handle);
            collect_resources(raw, &nested_defaults, entities);
            collect_objects(raw, &nested_defaults, entities);
        }
    }
}

fn collect_resources(
    container: &JsonValue,
    defaults: &EntityDefaults,
    entities: &mut TypedEntityArtifacts,
) {
    for raw in entity_values(container, "resource", "resources", Some("resource")) {
        if let Some(resource) = parse_resource_record(raw, defaults) {
            let nested_defaults = EntityDefaults {
                patient: resource.patient.clone().or_else(|| defaults.patient.clone()),
                handle: resource.handle.clone().or_else(|| defaults.handle.clone()),
                resource: Some(resource.key.clone()),
            };
            entities.resources.push(resource);
            collect_objects(raw, &nested_defaults, entities);
        }
    }
}

fn collect_objects(container: &JsonValue, defaults: &EntityDefaults, entities: &mut TypedEntityArtifacts) {
    for raw in entity_values(container, "object", "objects", Some("object")) {
        if let Some(object) = parse_object_record(raw, defaults) {
            entities.objects.push(object);
        }
    }
}

fn parse_patient_record(raw: &JsonValue) -> Option<PatientArtifactRecord> {
    let name = scalar_or_field(raw, &["name", "patient"]);
    let identifier = field_string(raw, &["id", "identifier"]);
    let key = name.clone().or_else(|| identifier.clone())?;

    Some(PatientArtifactRecord {
        key: key.clone(),
        name: name.unwrap_or(key),
        identifier,
        role: field_string(raw, &["role", "type"]),
        status: field_string(raw, &["status", "state"]),
        runtime: field_string(raw, &["runtime"]),
        path: field_string(raw, &["path", "module", "geode"]),
        is_default: field_bool(raw, &["default", "is_default"]),
        handle_ids: relation_keys(raw, &["handles"], handle_key),
        resource_names: relation_keys(raw, &["resources"], resource_key),
        object_ids: relation_keys(raw, &["objects"], object_key),
    })
}

fn parse_handle_record(raw: &JsonValue, defaults: &EntityDefaults) -> Option<HandleArtifactRecord> {
    let key = handle_key(raw)?;
    Some(HandleArtifactRecord {
        key,
        name: field_string(raw, &["name"]),
        patient: field_string(raw, &["patient"]).or_else(|| defaults.patient.clone()),
        owner: field_string(raw, &["owner"]),
        resource: field_string(raw, &["resource"]).or_else(|| defaults.resource.clone()),
        kind: field_string(raw, &["kind", "type"]),
        address: field_string(raw, &["address"]),
        segment: field_string(raw, &["segment"]),
        size: field_u64(raw, &["size"]),
        attached: field_bool(raw, &["attached"]),
        state_flags: flag_strings(raw, &["state", "states", "flags"]),
        object_ids: relation_keys(raw, &["objects"], object_key),
    })
}

fn parse_resource_record(
    raw: &JsonValue,
    defaults: &EntityDefaults,
) -> Option<ResourceArtifactRecord> {
    let name = scalar_or_field(raw, &["name", "resource"]);
    let identifier = field_string(raw, &["id", "identifier"]);
    let key = name.clone().or_else(|| identifier.clone())?;

    Some(ResourceArtifactRecord {
        key: key.clone(),
        name: name.unwrap_or(key),
        identifier,
        patient: field_string(raw, &["patient"]).or_else(|| defaults.patient.clone()),
        handle: field_string(raw, &["handle"]).or_else(|| defaults.handle.clone()),
        kind: field_string(raw, &["kind", "type"]),
        source_file: field_string(raw, &["source_file", "file", "source"]),
        object_ids: relation_keys(raw, &["objects"], object_key),
    })
}

fn parse_object_record(raw: &JsonValue, defaults: &EntityDefaults) -> Option<ObjectArtifactRecord> {
    let key = object_key(raw)?;
    Some(ObjectArtifactRecord {
        key,
        name: field_string(raw, &["name"]),
        class_name: field_string(raw, &["class", "class_name", "type"]),
        patient: field_string(raw, &["patient"]).or_else(|| defaults.patient.clone()),
        handle: field_string(raw, &["handle"]).or_else(|| defaults.handle.clone()),
        resource: field_string(raw, &["resource"]).or_else(|| defaults.resource.clone()),
        address: field_string(raw, &["address", "ptr"]),
        state_flags: flag_strings(raw, &["state", "states", "flags"]),
    })
}

fn entity_values<'a>(
    container: &'a JsonValue,
    singular: &str,
    plural: &str,
    kind_keyword: Option<&str>,
) -> Vec<&'a JsonValue> {
    let mut values = Vec::new();
    if kind_matches(container, kind_keyword) {
        values.push(container);
    }
    if let Some(object) = container.as_object() {
        if let Some(value) = object.get(singular) {
            extend_entity_values(value, &mut values);
        }
        if let Some(value) = object.get(plural) {
            extend_entity_values(value, &mut values);
        }
    }
    values
}

fn extend_entity_values<'a>(value: &'a JsonValue, values: &mut Vec<&'a JsonValue>) {
    match value {
        JsonValue::Array(entries) => values.extend(entries.iter()),
        JsonValue::Null => {}
        other => values.push(other),
    }
}

fn kind_matches(value: &JsonValue, kind_keyword: Option<&str>) -> bool {
    let Some(kind_keyword) = kind_keyword else {
        return false;
    };
    value
        .as_object()
        .and_then(|object| object.get("kind"))
        .and_then(JsonValue::as_str)
        .map(|kind| kind.eq_ignore_ascii_case(kind_keyword))
        .unwrap_or(false)
}

fn scalar_or_field(raw: &JsonValue, names: &[&str]) -> Option<String> {
    scalar_string(raw).or_else(|| field_string(raw, names))
}

fn field_string(raw: &JsonValue, names: &[&str]) -> Option<String> {
    let object = raw.as_object()?;
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(scalar_string))
}

fn field_bool(raw: &JsonValue, names: &[&str]) -> Option<bool> {
    let object = raw.as_object()?;
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(json_bool))
}

fn field_u64(raw: &JsonValue, names: &[&str]) -> Option<u64> {
    let object = raw.as_object()?;
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(json_u64))
}

fn relation_keys(raw: &JsonValue, names: &[&str], key_fn: fn(&JsonValue) -> Option<String>) -> Vec<String> {
    let Some(object) = raw.as_object() else {
        return Vec::new();
    };
    let mut values = BTreeSet::new();
    for name in names {
        if let Some(value) = object.get(*name) {
            collect_relation_keys(value, key_fn, &mut values);
        }
    }
    values.into_iter().collect()
}

fn collect_relation_keys(
    value: &JsonValue,
    key_fn: fn(&JsonValue) -> Option<String>,
    values: &mut BTreeSet<String>,
) {
    match value {
        JsonValue::Array(entries) => {
            for entry in entries {
                collect_relation_keys(entry, key_fn, values);
            }
        }
        JsonValue::Null => {}
        other => {
            if let Some(key) = key_fn(other) {
                values.insert(key);
            }
        }
    }
}

fn flag_strings(raw: &JsonValue, names: &[&str]) -> Vec<String> {
    let Some(object) = raw.as_object() else {
        return Vec::new();
    };
    let mut flags = BTreeSet::new();
    for name in names {
        if let Some(value) = object.get(*name) {
            collect_flag_strings(value, &mut flags);
        }
    }
    flags.into_iter().collect()
}

fn collect_flag_strings(value: &JsonValue, flags: &mut BTreeSet<String>) {
    match value {
        JsonValue::Array(entries) => {
            for entry in entries {
                collect_flag_strings(entry, flags);
            }
        }
        JsonValue::Object(entries) => {
            for (key, value) in entries {
                match value {
                    JsonValue::Bool(true) => {
                        flags.insert(key.clone());
                    }
                    JsonValue::String(text) if !text.is_empty() => {
                        flags.insert(format!("{key}={text}"));
                    }
                    JsonValue::Number(number) => {
                        flags.insert(format!("{key}={number}"));
                    }
                    _ => {}
                }
            }
        }
        other => {
            if let Some(flag) = scalar_string(other) {
                flags.insert(flag);
            }
        }
    }
}

fn handle_key(value: &JsonValue) -> Option<String> {
    scalar_or_field(value, &["id", "handle", "name", "address", "segment"])
}

fn resource_key(value: &JsonValue) -> Option<String> {
    scalar_or_field(value, &["name", "resource", "id", "identifier"])
}

fn object_key(value: &JsonValue) -> Option<String> {
    scalar_or_field(value, &["id", "object", "name", "address", "ptr", "class"])
}

fn scalar_string(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(value) => Some(value.clone()),
        JsonValue::Number(value) => Some(value.to_string()),
        JsonValue::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn json_bool(value: &JsonValue) -> Option<bool> {
    match value {
        JsonValue::Bool(value) => Some(*value),
        JsonValue::String(value) => match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn json_u64(value: &JsonValue) -> Option<u64> {
    match value {
        JsonValue::Number(value) => value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok())),
        JsonValue::String(value) => value.parse::<u64>().ok(),
        _ => None,
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
