#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value as JsonValue;
use swat_core::{SwatError, SwatResult};
use swat_value::{DecodedValue, DecodedValueData};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaNode {
    Any,
    Null,
    Bool,
    Number,
    String,
    Array(Box<SchemaNode>),
    Object(BTreeMap<String, SchemaNode>),
    Union(Vec<SchemaNode>),
}

impl fmt::Display for SchemaNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaNode::Any => f.write_str("any"),
            SchemaNode::Null => f.write_str("null"),
            SchemaNode::Bool => f.write_str("bool"),
            SchemaNode::Number => f.write_str("number"),
            SchemaNode::String => f.write_str("string"),
            SchemaNode::Array(item) => write!(f, "array<{item}>"),
            SchemaNode::Object(fields) => {
                write!(f, "object{{")?;
                let mut first = true;
                for (name, schema) in fields {
                    if !first {
                        write!(f, ", ")?;
                    }
                    first = false;
                    write!(f, "{name}: {schema}")?;
                }
                write!(f, "}}")
            }
            SchemaNode::Union(options) => {
                let rendered = options
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ");
                f.write_str(&rendered)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaMismatch {
    pub path: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SchemaValidation {
    pub mismatches: Vec<SchemaMismatch>,
}

impl SchemaValidation {
    pub fn is_valid(&self) -> bool {
        self.mismatches.is_empty()
    }
}

pub fn infer_json_schema(value: &JsonValue) -> SchemaNode {
    match value {
        JsonValue::Null => SchemaNode::Null,
        JsonValue::Bool(_) => SchemaNode::Bool,
        JsonValue::Number(_) => SchemaNode::Number,
        JsonValue::String(_) => SchemaNode::String,
        JsonValue::Array(values) => {
            if values.is_empty() {
                SchemaNode::Array(Box::new(SchemaNode::Any))
            } else {
                let merged = merge_schemas(values.iter().map(infer_json_schema).collect());
                SchemaNode::Array(Box::new(merged))
            }
        }
        JsonValue::Object(entries) => SchemaNode::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), infer_json_schema(value)))
                .collect(),
        ),
    }
}

pub fn infer_schema_from_decoded_value(value: &DecodedValue) -> Option<SchemaNode> {
    match &value.data {
        DecodedValueData::Json(value) => Some(infer_json_schema(value)),
        _ => None,
    }
}

pub fn lookup_schema_path(schema: &SchemaNode, path: &str) -> SwatResult<Option<SchemaNode>> {
    let tokens = parse_json_path(path)?;
    let mut current = schema;
    for token in tokens {
        current = match (current, token) {
            (SchemaNode::Object(fields), JsonPathToken::Field(field)) => {
                let Some(next) = fields.get(&field) else {
                    return Ok(None);
                };
                next
            }
            (SchemaNode::Array(item), JsonPathToken::Index(_)) => item,
            (SchemaNode::Union(options), token) => {
                let mut resolved = Vec::new();
                for option in options {
                    if let Some(next) = lookup_schema_path(option, &format_token_path(&token))? {
                        resolved.push(next);
                    }
                }
                if resolved.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(merge_schemas(resolved)));
            }
            _ => return Ok(None),
        };
    }

    Ok(Some(current.clone()))
}

pub fn validate_json(value: &JsonValue, schema: &SchemaNode) -> SchemaValidation {
    let mut mismatches = Vec::new();
    validate_json_at_path(value, schema, "$", &mut mismatches);
    SchemaValidation { mismatches }
}

pub fn validate_decoded_value(
    value: &DecodedValue,
    schema: &SchemaNode,
) -> SwatResult<SchemaValidation> {
    let DecodedValueData::Json(root) = &value.data else {
        return Err(SwatError::new("decoded value is not json"));
    };
    Ok(validate_json(root, schema))
}

fn validate_json_at_path(
    value: &JsonValue,
    schema: &SchemaNode,
    path: &str,
    mismatches: &mut Vec<SchemaMismatch>,
) {
    match schema {
        SchemaNode::Any => {}
        SchemaNode::Null => validate_primitive(value, "null", value.is_null(), path, mismatches),
        SchemaNode::Bool => validate_primitive(value, "bool", value.is_boolean(), path, mismatches),
        SchemaNode::Number => {
            validate_primitive(value, "number", value.is_number(), path, mismatches)
        }
        SchemaNode::String => {
            validate_primitive(value, "string", value.is_string(), path, mismatches)
        }
        SchemaNode::Array(item_schema) => match value {
            JsonValue::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    let item_path = format!("{path}[{index}]");
                    validate_json_at_path(item, item_schema, &item_path, mismatches);
                }
            }
            other => mismatches.push(SchemaMismatch {
                path: path.to_string(),
                expected: schema.to_string(),
                actual: actual_type(other),
            }),
        },
        SchemaNode::Object(fields) => match value {
            JsonValue::Object(entries) => {
                for (field, field_schema) in fields {
                    let field_path = format!("{path}.{field}");
                    match entries.get(field) {
                        Some(field_value) => {
                            validate_json_at_path(
                                field_value,
                                field_schema,
                                &field_path,
                                mismatches,
                            );
                        }
                        None => mismatches.push(SchemaMismatch {
                            path: field_path,
                            expected: field_schema.to_string(),
                            actual: "missing".to_string(),
                        }),
                    }
                }
            }
            other => mismatches.push(SchemaMismatch {
                path: path.to_string(),
                expected: schema.to_string(),
                actual: actual_type(other),
            }),
        },
        SchemaNode::Union(options) => {
            let mut branch_valid = false;
            for option in options {
                let mut branch_mismatches = Vec::new();
                validate_json_at_path(value, option, path, &mut branch_mismatches);
                if branch_mismatches.is_empty() {
                    branch_valid = true;
                    break;
                }
            }
            if !branch_valid {
                mismatches.push(SchemaMismatch {
                    path: path.to_string(),
                    expected: schema.to_string(),
                    actual: actual_type(value),
                });
            }
        }
    }
}

fn validate_primitive(
    value: &JsonValue,
    expected: &str,
    valid: bool,
    path: &str,
    mismatches: &mut Vec<SchemaMismatch>,
) {
    if !valid {
        mismatches.push(SchemaMismatch {
            path: path.to_string(),
            expected: expected.to_string(),
            actual: actual_type(value),
        });
    }
}

fn actual_type(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(_) => "bool".to_string(),
        JsonValue::Number(_) => "number".to_string(),
        JsonValue::String(_) => "string".to_string(),
        JsonValue::Array(_) => "array".to_string(),
        JsonValue::Object(_) => "object".to_string(),
    }
}

fn merge_schemas(schemas: Vec<SchemaNode>) -> SchemaNode {
    let mut unique: BTreeMap<String, SchemaNode> = BTreeMap::new();
    for schema in schemas {
        unique.entry(schema.to_string()).or_insert(schema);
    }
    let mut deduped = unique.into_values().collect::<Vec<_>>();
    if deduped.len() == 1 {
        deduped.remove(0)
    } else {
        SchemaNode::Union(deduped)
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
                    SwatError::new(format!("invalid schema path index '{index}': {err}"))
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

fn format_token_path(token: &JsonPathToken) -> String {
    match token {
        JsonPathToken::Field(field) => format!("$.{field}"),
        JsonPathToken::Index(index) => format!("$[{index}]"),
    }
}
