use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_adapter_mock::MockAdapter;
use swat_api::TraceInspector;
use swat_core::{ControlAction, EventKind};
use swat_expr::parse_expression;
use swat_session::SessionManager;
use swat_store::InMemoryStore;

#[test]
fn trace_inspector_can_find_boundary_events_and_artifact_text() {
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

    let inspector = TraceInspector::new(&store);
    let boundary_events = inspector.events_by_kind(session_id, EventKind::ModelBoundary);
    assert_eq!(boundary_events.len(), 1);

    let decoded = inspector.decoded_artifacts(&boundary_events[0]).unwrap();
    assert_eq!(decoded.len(), 1);

    let summary_matches = inspector.search_summaries(session_id, "observed");
    assert!(!summary_matches.is_empty());

    let artifact_matches = inspector
        .search_artifact_text(session_id, "call-tool")
        .unwrap();
    assert_eq!(artifact_matches.len(), 1);
    assert!(artifact_matches[0].matched_text.contains("call-tool"));

    let parsed =
        parse_expression(r#"kind == ModelBoundary and artifact.json $.tool == "search""#).unwrap();
    let queried = inspector.query_events(session_id, &parsed);
    assert_eq!(queried.len(), 1);

    let queried_str = inspector
        .query_events_str(session_id, r#"summary contains "attached""#)
        .unwrap();
    assert_eq!(queried_str.len(), 1);
}

#[test]
fn trace_inspector_can_resolve_agent_entities() {
    let code = r#"
import json
import sys
import time

PREFIX = "__SWATAGENT__"

def emit(record):
    sys.stdout.write(PREFIX + json.dumps(record) + "\n")
    sys.stdout.flush()

emit({"kind": "model", "phase": "request", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model requested"})
emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool started"})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool completed"})
emit({"kind": "model", "phase": "response", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model responded"})
time.sleep(0.1)
"#;
    let mut adapter =
        AgentRuntimeAdapter::new(AgentRuntimeSpec::new("python3").with_args(["-u", "-c", code]));
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        manager.pump(session_id, &mut adapter, &mut store).unwrap();
        if store.events_for_session(session_id).iter().any(|event| {
            matches!(
                &event.payload,
                swat_core::EventPayload::Text { summary }
                    if summary.contains("agent runtime exited")
            )
        }) {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let inspector = TraceInspector::new(&store);
    let entities = inspector.find_entities(session_id, "web").unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].entity.name, "web_search");

    let correlated = inspector.events_for_correlation(session_id, "req-7");
    assert_eq!(correlated.len(), 4);

    let boundary_id = correlated
        .iter()
        .find_map(|event| match event.payload {
            swat_core::EventPayload::Boundary { boundary_id, .. }
                if event.kind == EventKind::ModelBoundary =>
            {
                Some(boundary_id)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(inspector.boundary_span(session_id, boundary_id).len(), 2);
}
