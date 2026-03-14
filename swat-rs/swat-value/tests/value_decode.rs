use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_adapter_mock::MockAdapter;
use swat_core::{ControlAction, EventKind};
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::{DecodedValueData, QueriedValue, ValueKind, decode_event_artifacts};

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
