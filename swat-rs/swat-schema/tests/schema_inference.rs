use swat_adapter_mock::MockAdapter;
use swat_core::ControlAction;
use swat_schema::{
    SchemaNode, infer_schema_from_decoded_value, lookup_schema_path, validate_decoded_value,
};
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::decode_event_artifacts;

#[test]
fn infers_and_validates_mock_boundary_schema() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    manager.pump(session_id, &mut adapter, &mut store).unwrap();
    let second = manager.pump(session_id, &mut adapter, &mut store).unwrap();

    let boundary_event = second
        .stored_events
        .iter()
        .find(|event| event.kind == swat_core::EventKind::ModelBoundary)
        .unwrap();
    let decoded = decode_event_artifacts(&store, boundary_event).unwrap();
    let schema = infer_schema_from_decoded_value(&decoded[0]).unwrap();

    assert_eq!(
        lookup_schema_path(&schema, "$.tool").unwrap(),
        Some(SchemaNode::String)
    );

    let validation = validate_decoded_value(
        &decoded[0],
        &SchemaNode::Object(
            [
                ("decision".to_string(), SchemaNode::String),
                ("tool".to_string(), SchemaNode::String),
            ]
            .into_iter()
            .collect(),
        ),
    )
    .unwrap();
    assert!(validation.is_valid());
}

#[test]
fn reports_schema_mismatches_with_paths() {
    let json = serde_json::json!({
        "tool": "search",
        "count": "three",
    });
    let decoded = swat_value::DecodedValue {
        artifact_ref: swat_core::ArtifactRef {
            artifact_id: swat_core::ArtifactId::from_raw(1),
            media_type: "application/json".to_string(),
            encoding: swat_core::ArtifactEncoding::Json,
            size_hint: None,
            access: swat_core::ArtifactAccess::Lazy,
        },
        kind: swat_value::ValueKind::Json,
        data: swat_value::DecodedValueData::Json(json),
    };
    let schema = SchemaNode::Object(
        [
            ("tool".to_string(), SchemaNode::String),
            ("count".to_string(), SchemaNode::Number),
            (
                "args".to_string(),
                SchemaNode::Array(Box::new(SchemaNode::String)),
            ),
        ]
        .into_iter()
        .collect(),
    );

    let validation = validate_decoded_value(&decoded, &schema).unwrap();
    assert!(!validation.is_valid());
    assert!(
        validation
            .mismatches
            .iter()
            .any(|mismatch| mismatch.path == "$.count" && mismatch.expected == "number")
    );
    assert!(
        validation
            .mismatches
            .iter()
            .any(|mismatch| mismatch.path == "$.args" && mismatch.actual == "missing")
    );
}
