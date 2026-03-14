use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_control::{Trigger, TriggerAction, TriggerEngine, TriggerPredicate, pump_with_triggers};
use swat_core::{ControlAction, EventKind, EventPayload};
use swat_expr::parse_expression;
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::{QueriedValue, decode_event_artifacts};

fn python_sdk_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/python")
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
}

fn typescript_sdk_example_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/typescript/examples/emit_protocol.ts")
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
}

fn bun_available() -> bool {
    Command::new("bun")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn pump_until(
    manager: &mut SessionManager,
    session_id: swat_core::SessionId,
    adapter: &mut AgentRuntimeAdapter,
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
fn agent_adapter_emits_semantic_agent_events() {
    let code = r#"
import time

from swat_agent_protocol import LineEmitter, model, planner, policy, state, tool

emit = LineEmitter().emit

print("plain agent line")
emit(planner("draft-answer", phase="start", summary="planner started", file="agent.py", line=10, function="run"))
emit(model("gpt-4.1-mini", phase="request", span_id="model-1", correlation_id="req-42", summary="model requested", messages=2))
emit(tool("web_search", phase="start", span_id="tool-1", correlation_id="req-42", status="running", summary="tool started"))
emit(tool("web_search", phase="end", span_id="tool-1", correlation_id="req-42", status="ok", summary="tool completed"))
emit(state("memory.turn", phase="update", summary="memory updated", value={"answer": "42"}))
emit(policy("secret-redaction", phase="decision", summary="secret redacted", verdict="redact"))
emit(model("gpt-4.1-mini", phase="response", span_id="model-1", correlation_id="req-42", summary="model responded", tokens=128))
time.sleep(0.1)
"#;
    let spec = AgentRuntimeSpec::new("python3")
        .with_args(["-u", "-c", code])
        .with_env("PYTHONPATH", python_sdk_path());
    let mut adapter = AgentRuntimeAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    pump_until(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        Duration::from_secs(3),
        |store| {
            let events = store.events_for_session(session_id);
            events
                .iter()
                .any(|event| event.kind == EventKind::ModelBoundary)
                && events
                    .iter()
                    .any(|event| event.kind == EventKind::ToolBoundary)
                && events
                    .iter()
                    .any(|event| event.kind == EventKind::StateMutation)
                && events
                    .iter()
                    .any(|event| event.kind == EventKind::PolicyDecision)
                && events.iter().any(|event| {
                    matches!(
                        &event.payload,
                        EventPayload::Text { summary }
                            if summary.contains("agent runtime exited")
                    )
                })
        },
    );

    let events = store.events_for_session(session_id);
    assert!(
        events
            .iter()
            .any(|event| event.kind == EventKind::ValueObserved)
    );

    let model_request = events
        .iter()
        .find(|event| {
            event.kind == EventKind::ModelBoundary
                && matches!(
                    &event.payload,
                    EventPayload::Boundary { summary, .. } if summary.contains("requested")
                )
        })
        .unwrap()
        .clone();
    let model_response = events
        .iter()
        .find(|event| {
            event.kind == EventKind::ModelBoundary
                && matches!(
                    &event.payload,
                    EventPayload::Boundary { summary, .. } if summary.contains("responded")
                )
        })
        .unwrap()
        .clone();

    let request_boundary = match &model_request.payload {
        EventPayload::Boundary {
            boundary_id,
            summary,
            ..
        } => {
            assert!(summary.contains("requested"));
            *boundary_id
        }
        payload => panic!("unexpected payload: {payload:?}"),
    };
    let response_boundary = match &model_response.payload {
        EventPayload::Boundary {
            boundary_id,
            summary,
            ..
        } => {
            assert!(summary.contains("responded"));
            *boundary_id
        }
        payload => panic!("unexpected payload: {payload:?}"),
    };
    assert_eq!(request_boundary, response_boundary);
    assert_eq!(
        model_request.causality.correlation_id.as_deref(),
        Some("req-42")
    );

    let decoded = decode_event_artifacts(&store, &model_request).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(
        decoded[0].query_json_path("$.name").unwrap(),
        Some(QueriedValue::String("gpt-4.1-mini".to_string()))
    );
    assert_eq!(
        decoded[0].query_json_path("$.messages").unwrap(),
        Some(QueriedValue::Number("2".to_string()))
    );

    let policy_event = events
        .iter()
        .find(|event| event.kind == EventKind::PolicyDecision)
        .unwrap();
    match &policy_event.payload {
        EventPayload::Policy { verdict, summary } => {
            assert_eq!(*verdict, swat_core::PolicyVerdict::Redact);
            assert!(summary.contains("redacted"));
        }
        payload => panic!("unexpected payload: {payload:?}"),
    }
}

#[test]
fn agent_tool_error_can_drive_expression_trigger_pause() {
    let code = r#"
import time

from swat_agent_protocol import LineEmitter, state, tool

emit = LineEmitter().emit

emit(tool("web_search", phase="start", span_id="tool-1", status="running", summary="tool started"))
emit(tool("web_search", phase="error", span_id="tool-1", status="error", summary="tool failed", error="timeout"))
time.sleep(2)
emit(state("planner.outcome", phase="update", summary="late state update"))
"#;
    let spec = AgentRuntimeSpec::new("python3")
        .with_args(["-u", "-c", code])
        .with_env("PYTHONPATH", python_sdk_path());
    let mut adapter = AgentRuntimeAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let expr = parse_expression(
        r#"kind == ToolBoundary and artifact.json $.status == "error" and artifact.json $.name == "web_search""#,
    )
    .unwrap();
    let mut engine = TriggerEngine::new().with_trigger(
        Trigger::new(
            "pause-on-agent-tool-error",
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
            EventPayload::Text { summary } if summary.contains("agent runtime exited")
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
                    EventPayload::Text { summary }
                        if summary.contains("agent runtime exited")
                )
            })
        },
    );
}

#[test]
fn agent_adapter_surfaces_protocol_version_mismatch_as_lifecycle_event() {
    let code = r#"
import sys
import time

sys.stdout.write('__SWATAGENT__{"protocol_version":"9.9.9-test","kind":"model","phase":"request","name":"gpt-bad"}\n')
sys.stdout.flush()
time.sleep(0.1)
"#;
    let spec = AgentRuntimeSpec::new("python3").with_args(["-u", "-c", code]);
    let mut adapter = AgentRuntimeAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

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
                    EventPayload::Text { summary }
                        if summary.contains("unsupported agent protocol version")
                )
            })
        },
    );

    let protocol_error_event = store
        .events_for_session(session_id)
        .into_iter()
        .find(|event| {
            matches!(
                &event.payload,
                EventPayload::Text { summary }
                    if summary.contains("unsupported agent protocol version")
            )
        })
        .unwrap();

    let decoded = decode_event_artifacts(&store, &protocol_error_event).unwrap();
    assert_eq!(decoded.len(), 1);
    assert!(
        decoded[0]
            .preview(200)
            .contains("\"protocol_version\":\"9.9.9-test\"")
    );
}

#[test]
fn agent_adapter_accepts_typescript_sdk_records() {
    if !bun_available() {
        eprintln!("skipping bun-backed typescript sdk test because bun is unavailable");
        return;
    }

    let spec = AgentRuntimeSpec::new("bun")
        .with_args(vec!["run".to_string(), typescript_sdk_example_path()]);
    let mut adapter = AgentRuntimeAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    pump_until(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        Duration::from_secs(3),
        |store| {
            let events = store.events_for_session(session_id);
            events
                .iter()
                .any(|event| event.kind == EventKind::ModelBoundary)
                && events
                    .iter()
                    .any(|event| event.kind == EventKind::ToolBoundary)
                && events
                    .iter()
                    .any(|event| event.kind == EventKind::StateMutation)
                && events.iter().any(|event| {
                    matches!(
                        &event.payload,
                        EventPayload::Text { summary }
                            if summary.contains("agent runtime exited")
                    )
                })
        },
    );

    let model_event = store
        .events_for_session(session_id)
        .into_iter()
        .find(|event| event.kind == EventKind::ModelBoundary)
        .unwrap();

    let decoded = decode_event_artifacts(&store, &model_event).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(
        decoded[0].query_json_path("$.protocol_version").unwrap(),
        Some(QueriedValue::String("0.1.0-alpha".to_string()))
    );
    assert_eq!(
        decoded[0].query_json_path("$.correlation_id").unwrap(),
        Some(QueriedValue::String("req-ts-7".to_string()))
    );
}
