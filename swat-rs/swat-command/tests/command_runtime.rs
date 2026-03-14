use std::fs;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_adapter_mock::MockAdapter;
use swat_command::CommandHost;
use swat_store::InMemoryStore;

#[test]
fn mock_command_host_can_attach_pump_query_and_script() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    let attach = host.execute("attach").unwrap();
    assert!(attach.summary.contains("attached"));

    let session = host.execute("session").unwrap();
    assert!(session.summary.contains("session="));

    let resume = host.execute("resume").unwrap();
    assert!(resume.summary.contains("resumed"));

    host.execute("pump").unwrap();
    let second = host.execute("pump").unwrap();
    assert!(
        second
            .lines
            .iter()
            .any(|line| line.contains("ModelBoundary"))
    );

    let queried = host
        .execute(r#"query kind == ModelBoundary and artifact.json $.tool == "search""#)
        .unwrap();
    assert_eq!(queried.lines.len(), 1);

    let scripted = host.execute("script ctx.event_count()").unwrap();
    assert_eq!(scripted.lines.len(), 1);
    assert!(scripted.lines[0].contains("result="));
}

#[test]
fn command_host_can_manage_and_fire_semantic_triggers() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();

    let added = host
        .execute(r#"trigger-expr-once pause_search kind == ModelBoundary and artifact.json $.tool == "search""#)
        .unwrap();
    assert!(added.summary.contains("added trigger"));

    let listed = host.execute("triggers").unwrap();
    assert_eq!(listed.lines.len(), 1);
    let trigger_id = listed.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("trigger="))
        .unwrap()
        .to_string();
    assert!(listed.lines[0].contains("pause_search"));

    host.execute("pump").unwrap();
    let triggered = host.execute("pump").unwrap();
    assert!(triggered.lines.iter().any(|line| line.contains("trigger=")));
    assert!(
        triggered
            .lines
            .iter()
            .any(|line| line.contains("control accepted=true"))
    );

    let removed = host
        .execute(&format!("trigger-remove {trigger_id}"))
        .unwrap();
    assert!(removed.summary.contains("removed trigger"));

    let listed_after = host.execute("triggers").unwrap();
    assert_eq!(listed_after.lines.len(), 0);
}

#[test]
fn command_host_can_save_and_restore_trigger_sets() {
    let path = std::env::temp_dir().join(format!(
        "swat-trigger-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path_str = path.display().to_string();

    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    host.execute("attach").unwrap();
    host.execute("resume").unwrap();
    host.execute(
        r#"trigger-expr-once pause_search kind == ModelBoundary and artifact.json $.tool == "search""#,
    )
    .unwrap();

    let saved = host.execute(&format!("trigger-save {path_str}")).unwrap();
    assert!(saved.summary.contains("saved 1 trigger"));
    let persisted = fs::read_to_string(&path).unwrap();
    assert!(persisted.contains("pause_search"));
    assert!(persisted.contains(r#""format_version": 1"#));

    let mut restored = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    restored.execute("attach").unwrap();
    restored.execute("resume").unwrap();

    let loaded = restored
        .execute(&format!("trigger-load {path_str}"))
        .unwrap();
    assert!(loaded.summary.contains("loaded 1 trigger"));

    let listed = restored.execute("triggers").unwrap();
    assert_eq!(listed.lines.len(), 1);
    assert!(listed.lines[0].contains("pause_search"));
    assert!(listed.lines[0].contains("artifact.json $.tool"));

    restored.execute("pump").unwrap();
    let triggered = restored.execute("pump").unwrap();
    assert!(triggered.lines.iter().any(|line| line.contains("trigger=")));

    let _ = fs::remove_file(path);
}

#[test]
fn agent_command_host_can_resolve_entities_spans_and_artifacts() {
    let code = r#"
import json
import sys
import time

PREFIX = "__SWATAGENT__"

def emit(record):
    sys.stdout.write(PREFIX + json.dumps(record) + "\n")
    sys.stdout.flush()

emit({"kind": "planner", "phase": "start", "name": "draft-answer", "summary": "planner started", "file": "/tmp/agent.py", "line": 10, "function": "run"})
emit({"kind": "model", "phase": "request", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model requested"})
emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool started"})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool completed"})
emit({"kind": "model", "phase": "response", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model responded"})
time.sleep(0.1)
"#;
    let adapter =
        AgentRuntimeAdapter::new(AgentRuntimeSpec::new("python3").with_args(["-u", "-c", code]));
    let mut host = CommandHost::new(Box::new(adapter), Box::new(InMemoryStore::new()));

    host.execute("attach").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let pumped = host.execute("pump").unwrap();
        if pumped
            .lines
            .iter()
            .any(|line| line.contains("agent runtime exited"))
        {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let entities = host.execute("entities web").unwrap();
    assert_eq!(entities.lines.len(), 1);
    assert!(entities.lines[0].contains("ToolName"));
    assert!(entities.lines[0].contains("web_search"));

    let correlated = host.execute("correlation req-7").unwrap();
    assert_eq!(correlated.lines.len(), 4);

    let spans = host.execute("spans").unwrap();
    assert_eq!(spans.lines.len(), 2);
    let model_boundary = spans
        .lines
        .iter()
        .find_map(|line| {
            let raw = line.strip_prefix("boundary=")?.split_whitespace().next()?;
            Some(raw.to_string())
        })
        .unwrap();
    let span = host.execute(&format!("span {model_boundary}")).unwrap();
    assert_eq!(span.lines.len(), 2);

    let model_events = host.execute("events ModelBoundary").unwrap();
    assert_eq!(model_events.lines.len(), 2);
    let event_id = model_events.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("event="))
        .unwrap()
        .to_string();
    let artifacts = host.execute(&format!("artifacts {event_id}")).unwrap();
    assert_eq!(artifacts.lines.len(), 1);
    assert!(artifacts.lines[0].contains("gpt-4.1-mini"));
}
