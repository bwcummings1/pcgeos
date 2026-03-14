use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_python::{PythonAdapter, PythonAdapterSpec};
use swat_control::{Trigger, TriggerAction, TriggerEngine, TriggerPredicate, pump_with_triggers};
use swat_core::{ControlAction, EventKind};
use swat_expr::parse_expression;
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::decode_event_artifacts;

fn pump_until(
    manager: &mut SessionManager,
    session_id: swat_core::SessionId,
    adapter: &mut PythonAdapter,
    store: &mut InMemoryStore,
    timeout: Duration,
    predicate: impl Fn(&InMemoryStore) -> bool,
) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        manager.pump(session_id, adapter, store).unwrap();
        if predicate(store) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }

    panic!("condition was not met before timeout");
}

#[test]
fn python_adapter_emits_structured_trace_events() {
    let code = r#"
def helper(value):
    print("python hello")
    return value + 1

helper(3)
"#;
    let mut adapter = PythonAdapter::new(PythonAdapterSpec::inline(code));
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    pump_until(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        Duration::from_secs(2),
        |store| {
            let events = store.events_for_session(session_id);
            events
                .iter()
                .any(|event| event.kind == EventKind::Execution)
                && events
                    .iter()
                    .any(|event| event.kind == EventKind::ValueObserved)
                && events.iter().any(|event| {
                    matches!(
                        &event.payload,
                        swat_core::EventPayload::Text { summary }
                            if summary.contains("python runtime exited")
                    )
                })
        },
    );

    let execution_event = store
        .events_for_session(session_id)
        .into_iter()
        .find(|event| event.kind == EventKind::Execution)
        .unwrap();
    let decoded = decode_event_artifacts(&store, &execution_event).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(
        decoded[0].query_json_path("$.function").unwrap(),
        Some(swat_value::QueriedValue::String("<module>".to_string()))
    );

    let helper_event = store
        .events_for_session(session_id)
        .into_iter()
        .filter(|event| event.kind == EventKind::Execution)
        .find(|event| {
            decode_event_artifacts(&store, event)
                .unwrap()
                .into_iter()
                .any(|value| {
                    value.query_json_path("$.function").unwrap()
                        == Some(swat_value::QueriedValue::String("helper".to_string()))
                })
        })
        .unwrap();
    let helper_decoded = decode_event_artifacts(&store, &helper_event).unwrap();
    assert_eq!(
        helper_decoded[0].query_json_path("$.kind").unwrap(),
        Some(swat_value::QueriedValue::String("call".to_string()))
    );
}

#[test]
fn python_trace_events_can_drive_expression_trigger_pause() {
    let code = r#"
import time

def helper():
    print("before sleep")
    time.sleep(2)
    return "done"

helper()
"#;
    let mut adapter = PythonAdapter::new(PythonAdapterSpec::inline(code));
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let expr = parse_expression(
        r#"artifact.json $.function == "helper" and artifact.json $.kind == "call""#,
    )
    .unwrap();
    let mut engine = TriggerEngine::new().with_trigger(
        Trigger::new(
            "pause-on-python-helper-call",
            TriggerPredicate::Expr(expr),
            vec![TriggerAction::PauseTarget],
        )
        .fire_once(),
    );

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut paused = false;
    while Instant::now() < deadline {
        let report = pump_with_triggers(
            &mut manager,
            session_id,
            &mut adapter,
            &mut store,
            &mut engine,
        )
        .unwrap();
        if !report.trigger_matches.is_empty() {
            paused = true;
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(paused);

    let pause_window = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < pause_window {
        manager.pump(session_id, &mut adapter, &mut store).unwrap();
        thread::sleep(Duration::from_millis(25));
    }
    assert!(!store.events_for_session(session_id).iter().any(|event| {
        matches!(
            &event.payload,
            swat_core::EventPayload::Text { summary } if summary.contains("python runtime exited")
        )
    }));

    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    pump_until(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        Duration::from_secs(3),
        |store| {
            store.events_for_session(session_id).iter().any(|event| {
                matches!(
                    &event.payload,
                    swat_core::EventPayload::Text { summary }
                        if summary.contains("python runtime exited")
                )
            })
        },
    );
}
