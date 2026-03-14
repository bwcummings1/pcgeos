use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_core::EventPayload;
use swat_session::SessionManager;
use swat_store::InMemoryStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = LocalProcessSpec::new("/bin/sh")
        .with_args([
            "-c",
            "printf 'hello from stdout\\n'; printf 'hello from stderr\\n' 1>&2",
        ])
        .with_cwd("/tmp")
        .with_env("SWAT_RS_EXAMPLE", "1");
    let mut adapter = LocalProcessAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store)?;
    let session_id = attach.session.session_id;

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let report = manager.pump(session_id, &mut adapter, &mut store)?;
        let exited = report.stored_events.iter().any(|event| {
            matches!(
                &event.payload,
                EventPayload::Text { summary } if summary.contains("exited")
            )
        });
        if exited {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let session_events = store.events_for_session(session_id);
    println!(
        "captured {} event(s) and {} artifact(s)",
        session_events.len(),
        store.artifact_count()
    );

    for event in &session_events {
        match &event.payload {
            EventPayload::Text { summary }
            | EventPayload::Value { summary, .. }
            | EventPayload::Control { summary, .. } => {
                println!("event #{}: {}", event.sequence_no, summary);
            }
            _ => {}
        }
    }

    for event in session_events {
        for artifact_ref in event.artifact_refs {
            if let Some(artifact) = store.artifact(artifact_ref.artifact_id) {
                let rendered = String::from_utf8_lossy(&artifact.bytes);
                println!(
                    "artifact {}: {}",
                    artifact_ref.artifact_id.raw(),
                    rendered.trim_end()
                );
            }
        }
    }

    Ok(())
}
