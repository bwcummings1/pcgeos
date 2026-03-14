use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use swat_adapter_python::{PythonAdapter, PythonAdapterSpec};
use swat_core::{
    AdapterEmission, ArtifactAccess, ArtifactAlias, ArtifactBinding, ArtifactEncoding,
    CausalityLink, EventKind, EventPayload, PendingArtifact, PendingEvent, SessionId, TargetId,
    Timestamp,
};
use swat_session::SessionManager;
use swat_source::{
    SourceFailureKind, extract_event_source_location, inspect_event_source, is_real_source_path,
    resolve_event_source,
};
use swat_store::InMemoryStore;

fn unique_script_path() -> std::path::PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::env::temp_dir().join(format!("swat-rs-source-{}-{millis}.py", std::process::id()))
}

#[test]
fn resolves_python_trace_event_back_to_source_file() {
    let script_path = unique_script_path();
    fs::write(
        &script_path,
        r#"def helper(value):
    print("source lookup")
    return value + 1

helper(2)
"#,
    )
    .unwrap();

    let mut adapter =
        PythonAdapter::new(PythonAdapterSpec::script(script_path.display().to_string()));
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        manager.pump(session_id, &mut adapter, &mut store).unwrap();
        if store.events_for_session(session_id).iter().any(|event| {
            matches!(
                &event.payload,
                swat_core::EventPayload::Text { summary }
                    if summary.contains("python runtime exited")
            )
        }) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let helper_event = store
        .events_for_session(session_id)
        .into_iter()
        .find(|event| {
            swat_value::decode_event_artifacts(&store, event)
                .unwrap()
                .into_iter()
                .any(|value| {
                    value.query_json_path("$.function").unwrap()
                        == Some(swat_value::QueriedValue::String("helper".to_string()))
                        && value.query_json_path("$.kind").unwrap()
                            == Some(swat_value::QueriedValue::String("call".to_string()))
                })
        })
        .unwrap();

    let location = extract_event_source_location(&store, &helper_event)
        .unwrap()
        .unwrap();
    assert!(is_real_source_path(&location.file));
    assert_eq!(location.line, 1);

    let snippet = resolve_event_source(&store, &helper_event, 0, 2)
        .unwrap()
        .unwrap();
    assert_eq!(snippet.focus_line, 1);
    assert!(
        snippet
            .lines
            .iter()
            .any(|line| line.text.contains("def helper"))
    );

    fs::remove_file(script_path).unwrap();
}

#[test]
fn source_inspection_classifies_synthetic_and_missing_paths() {
    let session_id = SessionId::from_raw(1);
    let target_id = TargetId::from_raw(1);
    let mut next_sequence = 1;
    let mut store = InMemoryStore::new();

    let synthetic_alias = ArtifactAlias::from_raw(1);
    let missing_alias = ArtifactAlias::from_raw(2);
    let stored = store
        .ingest_emission(
            session_id,
            target_id,
            &mut next_sequence,
            AdapterEmission {
                pending_events: vec![
                    PendingEvent {
                        observed_at: Timestamp::from_millis(1),
                        kind: EventKind::SourceResolution,
                        causality: CausalityLink::default(),
                        payload: EventPayload::Value {
                            value_key: "synthetic".to_string(),
                            summary: "synthetic source".to_string(),
                        },
                        artifacts: vec![ArtifactBinding::Pending(synthetic_alias)],
                    },
                    PendingEvent {
                        observed_at: Timestamp::from_millis(2),
                        kind: EventKind::SourceResolution,
                        causality: CausalityLink::default(),
                        payload: EventPayload::Value {
                            value_key: "missing".to_string(),
                            summary: "missing source".to_string(),
                        },
                        artifacts: vec![ArtifactBinding::Pending(missing_alias)],
                    },
                ],
                pending_artifacts: vec![
                    PendingArtifact {
                        alias: synthetic_alias,
                        media_type: "application/json".to_string(),
                        encoding: ArtifactEncoding::Json,
                        access: ArtifactAccess::Inline,
                        bytes: br#"{"file":"<swat-rs-inline>","line":1,"function":"inline"}"#.to_vec(),
                    },
                    PendingArtifact {
                        alias: missing_alias,
                        media_type: "application/json".to_string(),
                        encoding: ArtifactEncoding::Json,
                        access: ArtifactAccess::Inline,
                        bytes: br#"{"file":"/tmp/does-not-exist-swat-rs.py","line":7,"function":"missing"}"#.to_vec(),
                    },
                ],
            },
        )
        .unwrap();

    let synthetic = inspect_event_source(&store, &stored[0], 1, 1).unwrap();
    assert!(synthetic.snippet.is_none());
    assert_eq!(
        synthetic.failure.unwrap().kind,
        SourceFailureKind::SyntheticPath
    );

    let missing = inspect_event_source(&store, &stored[1], 1, 1).unwrap();
    assert!(missing.snippet.is_none());
    assert_eq!(
        missing.failure.unwrap().kind,
        SourceFailureKind::MissingFile
    );
}
