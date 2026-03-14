#![forbid(unsafe_code)]

use swat_core::{EventEnvelope, EventKind, EventPayload, SwatError, SwatResult};
use swat_store::SwatStore;
use swat_value::{QueriedValue, decode_artifact};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryExpr {
    KindIs(EventKind),
    SummaryContains(String),
    ArtifactTextContains(String),
    ArtifactJsonPathExists(String),
    ArtifactJsonPathEquals {
        path: String,
        expected: QueriedValue,
    },
    And(Vec<QueryExpr>),
    Or(Vec<QueryExpr>),
}

pub fn parse_expression(input: &str) -> SwatResult<QueryExpr> {
    let tokens = tokenize(input)?;
    let mut parser = Parser { tokens, index: 0 };
    let expr = parser.parse_or_expr()?;
    if parser.peek().is_some() {
        return Err(SwatError::new("unexpected trailing tokens in expression"));
    }
    Ok(expr)
}

pub fn evaluate_expression<S: SwatStore + ?Sized>(
    store: &S,
    event: &EventEnvelope,
    expr: &QueryExpr,
) -> bool {
    match expr {
        QueryExpr::KindIs(kind) => event.kind == *kind,
        QueryExpr::SummaryContains(needle) => payload_summary(event)
            .map(|summary| summary.contains(needle))
            .unwrap_or(false),
        QueryExpr::ArtifactTextContains(needle) => event.artifact_refs.iter().any(|artifact_ref| {
            store
                .artifact(artifact_ref.artifact_id)
                .and_then(|artifact| String::from_utf8(artifact.bytes).ok())
                .map(|text| text.contains(needle))
                .unwrap_or(false)
        }),
        QueryExpr::ArtifactJsonPathExists(path) => event.artifact_refs.iter().any(|artifact_ref| {
            store
                .artifact(artifact_ref.artifact_id)
                .and_then(|artifact| decode_artifact(artifact).ok())
                .and_then(|value| value.query_json_path(path).ok())
                .flatten()
                .is_some()
        }),
        QueryExpr::ArtifactJsonPathEquals { path, expected } => {
            event.artifact_refs.iter().any(|artifact_ref| {
                store
                    .artifact(artifact_ref.artifact_id)
                    .and_then(|artifact| decode_artifact(artifact).ok())
                    .and_then(|value| value.query_json_path(path).ok())
                    .flatten()
                    .map(|actual| actual == *expected)
                    .unwrap_or(false)
            })
        }
        QueryExpr::And(exprs) => exprs
            .iter()
            .all(|expr| evaluate_expression(store, event, expr)),
        QueryExpr::Or(exprs) => exprs
            .iter()
            .any(|expr| evaluate_expression(store, event, expr)),
    }
}

fn payload_summary(event: &EventEnvelope) -> Option<&str> {
    match &event.payload {
        EventPayload::Empty => None,
        EventPayload::Text { summary }
        | EventPayload::Control { summary, .. }
        | EventPayload::Boundary { summary, .. }
        | EventPayload::Snapshot { summary, .. }
        | EventPayload::Trigger { summary, .. }
        | EventPayload::Value { summary, .. }
        | EventPayload::Policy { summary, .. } => Some(summary.as_str()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Ident(String),
    String(String),
    JsonPath(String),
    EqEq,
    Contains,
    Exists,
    And,
    Or,
    LParen,
    RParen,
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn parse_or_expr(&mut self) -> SwatResult<QueryExpr> {
        let mut exprs = vec![self.parse_and_expr()?];
        while matches!(self.peek(), Some(Token::Or)) {
            self.index += 1;
            exprs.push(self.parse_and_expr()?);
        }
        Ok(if exprs.len() == 1 {
            exprs.remove(0)
        } else {
            QueryExpr::Or(exprs)
        })
    }

    fn parse_and_expr(&mut self) -> SwatResult<QueryExpr> {
        let mut exprs = vec![self.parse_primary()?];
        while matches!(self.peek(), Some(Token::And)) {
            self.index += 1;
            exprs.push(self.parse_primary()?);
        }
        Ok(if exprs.len() == 1 {
            exprs.remove(0)
        } else {
            QueryExpr::And(exprs)
        })
    }

    fn parse_primary(&mut self) -> SwatResult<QueryExpr> {
        if matches!(self.peek(), Some(Token::LParen)) {
            self.index += 1;
            let expr = self.parse_or_expr()?;
            self.expect_token(Token::RParen)?;
            return Ok(expr);
        }

        self.parse_atom()
    }

    fn parse_atom(&mut self) -> SwatResult<QueryExpr> {
        let Some(token) = self.next() else {
            return Err(SwatError::new("unexpected end of expression"));
        };

        match token {
            Token::Ident(name) if name == "kind" => {
                self.expect_token(Token::EqEq)?;
                let kind = self.expect_ident()?;
                Ok(QueryExpr::KindIs(parse_event_kind(&kind)?))
            }
            Token::Ident(name) if name == "summary" => {
                self.expect_token(Token::Contains)?;
                let needle = self.expect_string()?;
                Ok(QueryExpr::SummaryContains(needle))
            }
            Token::Ident(name) if name == "artifact.text" => {
                self.expect_token(Token::Contains)?;
                let needle = self.expect_string()?;
                Ok(QueryExpr::ArtifactTextContains(needle))
            }
            Token::Ident(name) if name == "artifact.json" => match self.next() {
                Some(Token::Exists) => {
                    let path = self.expect_json_path()?;
                    Ok(QueryExpr::ArtifactJsonPathExists(path))
                }
                Some(Token::JsonPath(path)) => {
                    self.expect_token(Token::EqEq)?;
                    let expected = self.expect_literal()?;
                    Ok(QueryExpr::ArtifactJsonPathEquals { path, expected })
                }
                other => Err(SwatError::new(format!(
                    "unexpected token after artifact.json: {:?}",
                    other
                ))),
            },
            other => Err(SwatError::new(format!(
                "could not parse expression atom from token {:?}",
                other
            ))),
        }
    }

    fn expect_token(&mut self, expected: Token) -> SwatResult<()> {
        match self.next() {
            Some(token) if token == expected => Ok(()),
            other => Err(SwatError::new(format!(
                "expected token {:?}, found {:?}",
                expected, other
            ))),
        }
    }

    fn expect_ident(&mut self) -> SwatResult<String> {
        match self.next() {
            Some(Token::Ident(value)) => Ok(value),
            other => Err(SwatError::new(format!(
                "expected identifier, found {:?}",
                other
            ))),
        }
    }

    fn expect_string(&mut self) -> SwatResult<String> {
        match self.next() {
            Some(Token::String(value)) => Ok(value),
            other => Err(SwatError::new(format!(
                "expected string literal, found {:?}",
                other
            ))),
        }
    }

    fn expect_json_path(&mut self) -> SwatResult<String> {
        match self.next() {
            Some(Token::JsonPath(value)) => Ok(value),
            other => Err(SwatError::new(format!(
                "expected json path, found {:?}",
                other
            ))),
        }
    }

    fn expect_literal(&mut self) -> SwatResult<QueriedValue> {
        match self.next() {
            Some(Token::String(value)) => Ok(QueriedValue::String(value)),
            Some(Token::Ident(value)) if value == "true" => Ok(QueriedValue::Bool(true)),
            Some(Token::Ident(value)) if value == "false" => Ok(QueriedValue::Bool(false)),
            Some(Token::Ident(value)) if value == "null" => Ok(QueriedValue::Null),
            Some(Token::Ident(value)) if value.chars().all(|ch| ch.is_ascii_digit()) => {
                Ok(QueriedValue::Number(value))
            }
            other => Err(SwatError::new(format!(
                "expected literal value, found {:?}",
                other
            ))),
        }
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned();
        if token.is_some() {
            self.index += 1;
        }
        token
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }
}

fn parse_event_kind(kind: &str) -> SwatResult<EventKind> {
    match kind {
        "Lifecycle" => Ok(EventKind::Lifecycle),
        "Control" => Ok(EventKind::Control),
        "Execution" => Ok(EventKind::Execution),
        "StateMutation" => Ok(EventKind::StateMutation),
        "ValueObserved" => Ok(EventKind::ValueObserved),
        "TriggerHit" => Ok(EventKind::TriggerHit),
        "Snapshot" => Ok(EventKind::Snapshot),
        "Replay" => Ok(EventKind::Replay),
        "ModelBoundary" => Ok(EventKind::ModelBoundary),
        "ToolBoundary" => Ok(EventKind::ToolBoundary),
        "SourceResolution" => Ok(EventKind::SourceResolution),
        "SchemaResolution" => Ok(EventKind::SchemaResolution),
        "PolicyDecision" => Ok(EventKind::PolicyDecision),
        _ => Err(SwatError::new(format!("unknown event kind '{kind}'"))),
    }
}

fn tokenize(input: &str) -> SwatResult<Vec<Token>> {
    let chars = input.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut tokens = Vec::new();

    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }

        if ch == '(' {
            tokens.push(Token::LParen);
            index += 1;
            continue;
        }
        if ch == ')' {
            tokens.push(Token::RParen);
            index += 1;
            continue;
        }
        if ch == '=' && chars.get(index + 1) == Some(&'=') {
            tokens.push(Token::EqEq);
            index += 2;
            continue;
        }
        if ch == '"' {
            index += 1;
            let start = index;
            while index < chars.len() && chars[index] != '"' {
                index += 1;
            }
            if index >= chars.len() {
                return Err(SwatError::new("unterminated string literal"));
            }
            let value = chars[start..index].iter().collect::<String>();
            tokens.push(Token::String(value));
            index += 1;
            continue;
        }
        if ch == '$' {
            let start = index;
            index += 1;
            while index < chars.len()
                && !chars[index].is_whitespace()
                && chars[index] != ')'
                && !(chars[index] == '=' && chars.get(index + 1) == Some(&'='))
            {
                index += 1;
            }
            tokens.push(Token::JsonPath(chars[start..index].iter().collect()));
            continue;
        }

        let start = index;
        while index < chars.len()
            && !chars[index].is_whitespace()
            && chars[index] != '('
            && chars[index] != ')'
            && !(chars[index] == '=' && chars.get(index + 1) == Some(&'='))
        {
            index += 1;
        }
        let word = chars[start..index].iter().collect::<String>();
        let token = match word.as_str() {
            "contains" => Token::Contains,
            "exists" => Token::Exists,
            "and" => Token::And,
            "or" => Token::Or,
            _ => Token::Ident(word),
        };
        tokens.push(token);
    }

    Ok(tokens)
}
