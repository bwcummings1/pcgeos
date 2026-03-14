use std::fs;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use swat_adapter_mock::MockAdapter;
use swat_adapter_python::{PythonAdapter, PythonAdapterSpec};
use swat_core::ControlAction;
use swat_script::ScriptHost;
use swat_session::SessionManager;
use swat_store::InMemoryStore;

fn unique_script_path() -> std::path::PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::env::temp_dir().join(format!("swat-rs-script-{}-{millis}.py", std::process::id()))
}

#[test]
fn script_host_can_query_mock_trace() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    manager.pump(session_id, &mut adapter, &mut store).unwrap();
    manager.pump(session_id, &mut adapter, &mut store).unwrap();

    let mut host = ScriptHost::new(&store, session_id);
    assert!(host.eval_i64("ctx.event_count()").unwrap() >= 4);
    assert_eq!(
        host.eval_i64(r#"ctx.query_count("kind == ModelBoundary")"#)
            .unwrap(),
        1
    );
    assert_eq!(
        host.eval_i64(r#"ctx.artifact_search_count("call-tool")"#)
            .unwrap(),
        1
    );
    assert!(
        host.eval_string(r#"ctx.first_summary("summary contains \"observed\"")"#)
            .unwrap()
            .contains("observed")
    );
}

#[test]
fn script_host_can_resolve_source_for_python_trace() {
    let script_path = unique_script_path();
    fs::write(
        &script_path,
        r#"def helper(value):
    print("source script")
    return value + 1

helper(2)
"#,
    )
    .unwrap();

    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter =
        PythonAdapter::new(PythonAdapterSpec::script(script_path.display().to_string()));

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
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
        std::thread::sleep(Duration::from_millis(25));
    }

    let mut host = ScriptHost::new(&store, session_id);
    assert!(
        host.eval_bool(
            r#"ctx.source_contains("artifact.json $.function == \"helper\"", "def helper", 0, 1)"#,
        )
        .unwrap()
    );

    fs::remove_file(script_path).unwrap();
}
