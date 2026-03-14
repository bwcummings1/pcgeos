use swat_core::{
    AdapterEmission, ArtifactAccess, ArtifactAlias, ArtifactBinding, ArtifactEncoding, BoundaryId,
    CausalityLink, EventKind, EventPayload, PendingArtifact, PendingEvent, SessionId, TargetId,
};
use swat_expr::{QueryExpr, QueryField, QueryValue, evaluate_expression, parse_expression};
use swat_store::InMemoryStore;
use swat_value::QueriedValue;

fn synthetic_session_events() -> (
    InMemoryStore,
    swat_core::EventEnvelope,
    swat_core::EventEnvelope,
) {
    let session_id = SessionId::from_raw(1);
    let target_id = TargetId::from_raw(9);
    let mut next_sequence = 1;
    let mut store = InMemoryStore::new();

    let boundary_alias = ArtifactAlias::from_raw(1);
    let value_alias = ArtifactAlias::from_raw(2);

    let boundary_event = PendingEvent {
        observed_at: swat_core::Timestamp::from_millis(1),
        kind: EventKind::ModelBoundary,
        causality: CausalityLink {
            parent_event_id: None,
            correlation_id: Some("req-7".to_string()),
        },
        payload: EventPayload::Boundary {
            boundary_id: BoundaryId::from_raw(42),
            determinism: swat_core::DeterminismClass::ExternalBoundary,
            summary: "model requested".to_string(),
        },
        artifacts: vec![ArtifactBinding::Pending(boundary_alias)],
    };

    let value_event = PendingEvent {
        observed_at: swat_core::Timestamp::from_millis(2),
        kind: EventKind::ValueObserved,
        causality: CausalityLink::default(),
        payload: EventPayload::Value {
            value_key: "memory.turn".to_string(),
            summary: "memory updated".to_string(),
        },
        artifacts: vec![ArtifactBinding::Pending(value_alias)],
    };

    let stored = store
        .ingest_emission(
            session_id,
            target_id,
            &mut next_sequence,
            AdapterEmission {
                pending_events: vec![boundary_event, value_event],
                pending_artifacts: vec![
                    PendingArtifact {
                        alias: boundary_alias,
                        media_type: "application/json".to_string(),
                        encoding: ArtifactEncoding::Json,
                        access: ArtifactAccess::Lazy,
                        bytes: br#"{"tool":"search","decision":"call-tool","correlation_id":"req-7","span_id":"model-1","file":"/tmp/demo.py","function":"helper"}"#.to_vec(),
                    },
                    PendingArtifact {
                        alias: value_alias,
                        media_type: "application/json".to_string(),
                        encoding: ArtifactEncoding::Json,
                        access: ArtifactAccess::Lazy,
                        bytes: br#"{"kind":"state","name":"memory.turn","summary":"memory updated","file":"/tmp/demo.py","function":"update_memory"}"#.to_vec(),
                    },
                ],
            },
        )
        .unwrap();

    (store, stored[0].clone(), stored[1].clone())
}

#[test]
fn parses_basic_query_language() {
    let expr = parse_expression(
        r#"kind == ModelBoundary and artifact.json $.tool == "search" and not artifact.text contains "error""#,
    )
    .unwrap();

    match expr {
        QueryExpr::And(parts) => assert_eq!(parts.len(), 3),
        other => panic!("unexpected parsed expr: {other:?}"),
    }
}

#[test]
fn parses_literal_and_field_variants() {
    let expr = parse_expression(
        r#"event.id == 1 or sequence == 2 or correlation exists or source.file contains "demo.py""#,
    )
    .unwrap();
    match expr {
        QueryExpr::Or(parts) => {
            assert_eq!(parts.len(), 4);
            assert_eq!(
                parts[0],
                QueryExpr::FieldEquals {
                    field: QueryField::EventId,
                    expected: QueryValue::Value(QueriedValue::Number("1".to_string())),
                }
            );
            assert_eq!(parts[2], QueryExpr::FieldExists(QueryField::CorrelationId));
        }
        other => panic!("unexpected expr: {other:?}"),
    }
}

#[test]
fn evaluates_query_against_richer_event_fields() {
    let (store, boundary_event, value_event) = synthetic_session_events();

    let expr = parse_expression(&format!(
        concat!(
            "kind == ModelBoundary and ",
            "event.id == {} and ",
            "sequence == {} and ",
            "correlation == \"req-7\" and ",
            "boundary == 42 and ",
            "span == \"model-1\" and ",
            "source.file contains \"demo.py\" and ",
            "source.function == \"helper\" and ",
            "not value.key exists"
        ),
        boundary_event.event_id.raw(),
        boundary_event.sequence_no
    ))
    .unwrap();
    assert!(evaluate_expression(&store, &boundary_event, &expr));

    let negative = parse_expression(r#"not source.function == "helper""#).unwrap();
    assert!(!evaluate_expression(&store, &boundary_event, &negative));

    let value_expr =
        parse_expression(r#"value.key == "memory.turn" and source.function contains "update""#)
            .unwrap();
    assert!(evaluate_expression(&store, &value_event, &value_expr));
}

#[test]
fn preserves_existing_artifact_json_equality_behavior() {
    let (store, boundary_event, _) = synthetic_session_events();

    let expr =
        parse_expression(r#"artifact.json $.tool == "search" and summary contains "requested""#)
            .unwrap();
    assert!(evaluate_expression(&store, &boundary_event, &expr));

    let negative = parse_expression(r#"artifact.json $.tool == "other""#).unwrap();
    assert!(!evaluate_expression(&store, &boundary_event, &negative));
}
