use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_python::{PythonAdapter, PythonAdapterSpec};
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::decode_event_artifacts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let code = r#"
def helper(value):
    print("python hello")
    return value + 1

helper(3)
"#;
    let mut adapter = PythonAdapter::new(PythonAdapterSpec::inline(code));
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store)?;
    let session_id = attach.session.session_id;

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        manager.pump(session_id, &mut adapter, &mut store)?;
        if store.events_for_session(session_id).iter().any(|event| {
            matches!(
                &event.payload,
                swat_core::EventPayload::Text { summary }
                    if summary.contains("python runtime exited")
            )
        }) {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    for event in store.events_for_session(session_id) {
        match &event.payload {
            swat_core::EventPayload::Text { summary }
            | swat_core::EventPayload::Value { summary, .. } => {
                println!("event #{}: {}", event.sequence_no, summary);
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
