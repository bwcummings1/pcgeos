use swat_core::{
    AdapterEmission, ArtifactAccess, ArtifactAlias, ArtifactBinding, ArtifactEncoding, ArtifactId,
    ArtifactRef, EventKind, EventPayload, PendingEvent, SessionId, TargetId,
};
use swat_store::InMemoryStore;

#[test]
fn rejects_missing_materialized_aliases() {
    let mut store = InMemoryStore::new();
    let mut next_sequence = 1;
    let emission = AdapterEmission {
        pending_events: vec![
            PendingEvent::new(
                EventKind::ModelBoundary,
                EventPayload::Text {
                    summary: "missing alias".to_string(),
                },
            )
            .with_artifact(ArtifactBinding::Pending(ArtifactAlias::from_raw(99))),
        ],
        pending_artifacts: Vec::new(),
    };

    let err = store
        .ingest_emission(
            SessionId::from_raw(1),
            TargetId::from_raw(2),
            &mut next_sequence,
            emission,
        )
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("missing materialized artifact for alias 99")
    );
    assert_eq!(store.events().len(), 0);
    assert_eq!(store.artifact_count(), 0);
    assert_eq!(next_sequence, 1);
}

#[test]
fn rejects_unknown_existing_artifact_references() {
    let mut store = InMemoryStore::new();
    let mut next_sequence = 1;
    let emission = AdapterEmission {
        pending_events: vec![
            PendingEvent::new(
                EventKind::ToolBoundary,
                EventPayload::Text {
                    summary: "unknown artifact".to_string(),
                },
            )
            .with_artifact(ArtifactBinding::Existing(ArtifactRef {
                artifact_id: ArtifactId::from_raw(77),
                media_type: "application/json".to_string(),
                encoding: ArtifactEncoding::Json,
                size_hint: Some(8),
                access: ArtifactAccess::Lazy,
            })),
        ],
        pending_artifacts: Vec::new(),
    };

    let err = store
        .ingest_emission(
            SessionId::from_raw(3),
            TargetId::from_raw(4),
            &mut next_sequence,
            emission,
        )
        .unwrap_err();

    assert!(err.to_string().contains("missing referenced artifact 77"));
    assert_eq!(store.events().len(), 0);
    assert_eq!(store.artifact_count(), 0);
    assert_eq!(next_sequence, 1);
}
