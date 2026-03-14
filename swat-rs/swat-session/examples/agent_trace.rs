use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::decode_event_artifacts;

fn python_sdk_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/python")
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let code = r#"
import time

from swat_agent_protocol import LineEmitter, model, planner, state, tool

emit = LineEmitter().emit

print("agent console line")
emit(planner("draft-answer", phase="start", summary="planner started"))
emit(model("gpt-4.1-mini", phase="request", span_id="model-1", correlation_id="req-99", summary="model requested", messages=2))
emit(tool("web_search", phase="start", span_id="tool-1", correlation_id="req-99", summary="tool started", status="running"))
emit(tool("web_search", phase="end", span_id="tool-1", correlation_id="req-99", summary="tool completed", status="ok"))
emit(model("gpt-4.1-mini", phase="response", span_id="model-1", correlation_id="req-99", summary="model responded", tokens=64))
emit(state("memory.turn", phase="update", summary="memory updated", value={"answer": "42"}))
time.sleep(0.1)
"#;

    let spec = AgentRuntimeSpec::new("python3")
        .with_args(["-u", "-c", code])
        .with_env("PYTHONPATH", python_sdk_path());
    let mut adapter = AgentRuntimeAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store)?;
    let session_id = attach.session.session_id;

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        manager.pump(session_id, &mut adapter, &mut store)?;
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

    for event in store.events_for_session(session_id) {
        match &event.payload {
            swat_core::EventPayload::Text { summary }
            | swat_core::EventPayload::Value { summary, .. }
            | swat_core::EventPayload::Boundary { summary, .. }
            | swat_core::EventPayload::Policy { summary, .. } => {
                println!("event #{} {:?}: {}", event.sequence_no, event.kind, summary);
            }
            _ => {}
        }

        let decoded = decode_event_artifacts(&store, &event)?;
        for value in decoded {
            println!("  artifact: {}", value.preview(200));
        }
    }

    Ok(())
}
