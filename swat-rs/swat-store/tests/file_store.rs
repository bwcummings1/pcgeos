use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use swat_adapter_mock::MockAdapter;
use swat_core::{ControlAction, EventKind, EventPayload};
use swat_session::SessionManager;
use swat_store::{FileStore, SwatStore};

fn unique_store_root(test_name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::env::temp_dir().join(format!(
        "swat-rs-{test_name}-{}-{millis}",
        std::process::id()
    ))
}

#[test]
fn file_store_persists_events_and_artifacts_across_reopen() {
    let root = unique_store_root("file-store");
    let mut store = FileStore::open(&root).unwrap();
    let mut manager = SessionManager::new();
    let mut adapter = MockAdapter::default();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    manager.pump(session_id, &mut adapter, &mut store).unwrap();
    let boundary_pump = manager.pump(session_id, &mut adapter, &mut store).unwrap();
    let snapshot = manager
        .control(
            session_id,
            &mut adapter,
            ControlAction::CreateSnapshot {
                reason: "persist snapshot".to_string(),
            },
            &mut store,
        )
        .unwrap();
    let snapshot_record = snapshot.snapshot.unwrap();

    let boundary_event = boundary_pump
        .stored_events
        .into_iter()
        .find(|event| event.kind == EventKind::ModelBoundary)
        .unwrap();
    let artifact_ref = boundary_event.artifact_refs[0].clone();
    let initial_artifact = store.artifact(artifact_ref.artifact_id).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&initial_artifact.bytes),
        r#"{"decision":"call-tool","tool":"search"}"#
    );
    let initial_event_count = store.events().len();
    let initial_artifact_count = store.artifact_count();
    let initial_snapshot_count = store.snapshots().len();

    drop(store);

    let reopened = FileStore::open(&root).unwrap();
    assert_eq!(reopened.events().len(), initial_event_count);
    assert_eq!(reopened.artifact_count(), initial_artifact_count);
    assert_eq!(reopened.snapshots().len(), initial_snapshot_count);

    let reopened_artifact = reopened.artifact(artifact_ref.artifact_id).unwrap();
    assert_eq!(reopened_artifact, initial_artifact);
    let reopened_snapshot = reopened.snapshot(snapshot_record.snapshot_id).unwrap();
    assert_eq!(reopened_snapshot, snapshot_record);

    let reopened_events = reopened.events_for_session(session_id);
    assert!(reopened_events.iter().any(|event| {
        matches!(
            &event.payload,
            EventPayload::Boundary { summary, .. } if summary.contains("observed")
        )
    }));
    assert!(
        reopened
            .artifact_path(artifact_ref.artifact_id)
            .unwrap()
            .exists()
    );
    assert!(
        reopened_events
            .iter()
            .any(|event| event.kind == EventKind::Snapshot)
    );

    fs::remove_dir_all(&root).unwrap();
}
