use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_core::{EventKind, EventPayload};
use swat_resolver::{ResolvedEntityKind, TraceResolver};
use swat_session::SessionManager;
use swat_store::InMemoryStore;

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
fn resolver_indexes_agent_entities_and_boundary_spans() {
    let code = r#"
import json
import sys
import time

PREFIX = "__SWATAGENT__"

def emit(record):
    sys.stdout.write(PREFIX + json.dumps(record) + "\n")
    sys.stdout.flush()

emit({"kind": "planner", "phase": "start", "name": "draft-answer", "summary": "planner started", "file": "/tmp/agent.py", "line": 10, "function": "run"})
emit({"kind": "model", "phase": "request", "span_id": "model-1", "correlation_id": "req-42", "name": "gpt-4.1-mini", "summary": "model requested"})
emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-42", "name": "web_search", "summary": "tool started"})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-42", "name": "web_search", "summary": "tool completed"})
emit({"kind": "model", "phase": "response", "span_id": "model-1", "correlation_id": "req-42", "name": "gpt-4.1-mini", "summary": "model responded"})
emit({"kind": "state", "phase": "update", "name": "memory.turn", "summary": "memory updated"})
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
                    EventPayload::Text { summary } if summary.contains("agent runtime exited")
                )
            })
        },
    );

    let resolver = TraceResolver::new(&store);
    let index = resolver.index_session(session_id).unwrap();

    assert!(index.entities.iter().any(|entity| {
        entity.entity.kind == ResolvedEntityKind::ToolName
            && entity.entity.name == "web_search"
            && entity.event_ids.len() == 2
    }));
    assert!(index.entities.iter().any(|entity| {
        entity.entity.kind == ResolvedEntityKind::CorrelationId
            && entity.entity.name == "req-42"
            && entity.event_ids.len() == 4
    }));
    assert!(index.entities.iter().any(|entity| {
        entity.entity.kind == ResolvedEntityKind::StateKey && entity.entity.name == "memory.turn"
    }));
    assert!(index.relations.iter().any(|relation| {
        let names = [
            (&relation.left.kind, relation.left.name.as_str()),
            (&relation.right.kind, relation.right.name.as_str()),
        ];
        names.contains(&(&ResolvedEntityKind::CorrelationId, "req-42"))
            && names.contains(&(&ResolvedEntityKind::ToolName, "web_search"))
    }));
    assert!(index.correlation_groups.iter().any(|group| {
        group.correlation_id == "req-42"
            && group.span_ids.iter().any(|span| span == "model-1")
            && group.span_ids.iter().any(|span| span == "tool-1")
            && group.entities.iter().any(|entity| {
                entity.kind == ResolvedEntityKind::ModelName && entity.name == "gpt-4.1-mini"
            })
    }));

    let web_entities = resolver.find_entities(session_id, "web").unwrap();
    assert_eq!(web_entities.len(), 1);
    assert_eq!(web_entities[0].entity.kind, ResolvedEntityKind::ToolName);

    let correlation_events = resolver.events_for_correlation(session_id, "req-42");
    assert_eq!(correlation_events.len(), 4);
    assert!(correlation_events.iter().all(|event| matches!(
        event.kind,
        EventKind::ModelBoundary | EventKind::ToolBoundary
    )));

    let model_boundary_id = store
        .events_for_session(session_id)
        .into_iter()
        .find_map(|event| match event.payload {
            EventPayload::Boundary { boundary_id, .. }
                if event.kind == EventKind::ModelBoundary =>
            {
                Some(boundary_id)
            }
            _ => None,
        })
        .unwrap();
    let model_span = resolver.boundary_span(session_id, model_boundary_id);
    assert_eq!(model_span.len(), 2);
    assert_eq!(
        resolver
            .events_for_span(session_id, "model-1")
            .unwrap()
            .len(),
        2
    );

    let model_request = resolver
        .events_by_kind(session_id, EventKind::ModelBoundary)
        .into_iter()
        .find(|event| {
            matches!(
                &event.payload,
                EventPayload::Boundary { summary, .. } if summary.contains("requested")
            )
        })
        .unwrap();
    let event_entities = resolver.event_entities(&model_request).unwrap();
    assert!(event_entities.iter().any(|entity| {
        entity.kind == ResolvedEntityKind::ModelName && entity.name == "gpt-4.1-mini"
    }));
    assert!(
        event_entities.iter().any(|entity| {
            entity.kind == ResolvedEntityKind::SpanId && entity.name == "model-1"
        })
    );
}
