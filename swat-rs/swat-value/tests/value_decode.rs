use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_adapter_mock::MockAdapter;
use swat_core::{ArtifactAccess, ArtifactEncoding, ArtifactId, ArtifactRef};
use swat_core::{ControlAction, EventKind};
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_store::StoredArtifact;
use swat_value::{
    DecodedValueData, QueriedValue, ValueKind, decode_artifact, decode_event_artifacts,
};

#[test]
fn decodes_mock_boundary_artifact_as_json() {
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
        .find(|event| event.kind == EventKind::ModelBoundary)
        .unwrap();
    let decoded = decode_event_artifacts(&store, boundary_event).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].kind, ValueKind::Json);
    assert_eq!(
        decoded[0].query_json_path("$.tool").unwrap(),
        Some(QueriedValue::String("search".to_string()))
    );
    match &decoded[0].data {
        DecodedValueData::Json(value) => {
            assert_eq!(value["decision"], "call-tool");
        }
        other => panic!("unexpected decoded data: {other:?}"),
    }
}

#[test]
fn decodes_local_process_output_as_text() {
    let spec = LocalProcessSpec::new("/bin/sh").with_args(["-c", "printf 'hello value layer'"]);
    let mut adapter = LocalProcessAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let report = manager.pump(session_id, &mut adapter, &mut store).unwrap();
        if let Some(value_event) = report
            .stored_events
            .iter()
            .find(|event| event.kind == EventKind::ValueObserved)
        {
            let decoded = decode_event_artifacts(&store, value_event).unwrap();
            assert_eq!(decoded.len(), 1);
            assert_eq!(decoded[0].kind, ValueKind::Text);
            match &decoded[0].data {
                DecodedValueData::Text(text) => {
                    assert!(text.contains("hello value layer"));
                    assert!(decoded[0].preview(5).starts_with("hello"));
                }
                other => panic!("unexpected decoded data: {other:?}"),
            }
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }

    panic!("did not observe local-process output artifact before timeout");
}

#[test]
fn value_presentations_format_json_text_and_binary_for_operators() {
    let json_value = decode_artifact(StoredArtifact {
        artifact_ref: ArtifactRef {
            artifact_id: ArtifactId::from_raw(1),
            media_type: "application/json".to_string(),
            encoding: ArtifactEncoding::Json,
            size_hint: None,
            access: ArtifactAccess::Lazy,
        },
        created_at: swat_core::Timestamp::from_millis(1),
        bytes: br#"{"tool":"search","decision":"call-tool"}"#.to_vec(),
    })
    .unwrap();
    let json_presentation = json_value.presentation(12);
    assert!(json_presentation.preview.ends_with("..."));
    assert!(json_presentation.detail.contains('\n'));

    let text_value = decode_artifact(StoredArtifact {
        artifact_ref: ArtifactRef {
            artifact_id: ArtifactId::from_raw(2),
            media_type: "text/plain".to_string(),
            encoding: ArtifactEncoding::Utf8,
            size_hint: None,
            access: ArtifactAccess::Inline,
        },
        created_at: swat_core::Timestamp::from_millis(2),
        bytes: b"line one\nline two".to_vec(),
    })
    .unwrap();
    assert_eq!(text_value.preview(32), "line one\\nline two");

    let binary_value = decode_artifact(StoredArtifact {
        artifact_ref: ArtifactRef {
            artifact_id: ArtifactId::from_raw(3),
            media_type: "application/octet-stream".to_string(),
            encoding: ArtifactEncoding::Binary,
            size_hint: None,
            access: ArtifactAccess::Lazy,
        },
        created_at: swat_core::Timestamp::from_millis(3),
        bytes: vec![0xde, 0xad, 0xbe, 0xef, 0x01],
    })
    .unwrap();
    let binary_presentation = binary_value.presentation(32);
    assert!(binary_presentation.preview.contains("de ad be ef"));
    assert!(binary_presentation.detail.contains("de ad be ef"));
}
