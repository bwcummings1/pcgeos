use std::fs;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_adapter_mock::MockAdapter;
use swat_command::{
    Command, CommandHost, CommandSurface, command_completions, command_help, command_search,
    parse_command,
};
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
    assert!(
        session
            .lines
            .iter()
            .any(|line| line.contains("counts events=1"))
    );

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

    let status = host.execute("status").unwrap();
    assert!(status.lines.iter().any(|line| line.contains("snapshots=0")));

    let help = host.execute("help query").unwrap();
    assert!(help.summary.contains("help query"));
    assert!(help.lines.iter().any(|line| line.contains("event.id")));

    let scripted = host.execute("script ctx.event_count()").unwrap();
    assert_eq!(scripted.lines.len(), 1);
    assert!(scripted.lines[0].contains("result="));
}

#[test]
fn command_registry_exposes_family_help_and_alias_parsing() {
    let help = command_help(Some("breakpoint"), CommandSurface::Shell);
    assert!(help.summary.contains("help breakpoint"));
    assert!(
        help.lines
            .iter()
            .any(|line| line.contains("breakpoint add <name> <expr>"))
    );
    assert!(
        help.lines
            .iter()
            .any(|line| line.contains("semantic breakpoints"))
    );

    let tui_help = command_help(Some("source"), CommandSurface::Tui);
    assert!(
        tui_help
            .lines
            .iter()
            .any(|line| line.contains("source file <path>"))
    );
    assert!(
        command_help(None, CommandSurface::Shell)
            .lines
            .iter()
            .any(|line| line.contains("help search <needle>"))
    );

    let search = command_search("break", CommandSurface::Tui);
    assert!(search.summary.contains("help search break"));
    assert!(
        search
            .lines
            .iter()
            .any(|line| line.contains("topic=breakpoint"))
    );
    assert!(
        search
            .lines
            .iter()
            .any(|line| line.contains("breakpoint list [shell]"))
    );

    let completions = command_completions("help br", CommandSurface::Shell);
    assert!(completions.contains(&"help breakpoint".to_string()));
    let tui_completions = command_completions("sou", CommandSurface::Tui);
    assert!(tui_completions.contains(&"source show <event_id> [before] [after]".to_string()));

    assert_eq!(parse_command("stack").unwrap(), Command::Spans);
    assert_eq!(
        parse_command("help search break").unwrap(),
        Command::HelpSearch {
            needle: "break".to_string(),
        }
    );
    assert_eq!(
        parse_command("stack frame 0").unwrap(),
        Command::Frame { frame_index: 0 }
    );
    assert_eq!(
        parse_command("stack show 42").unwrap(),
        Command::Span {
            boundary_id: swat_core::BoundaryId::from_raw(42),
        }
    );
    assert_eq!(
        parse_command("source show 7 1 3").unwrap(),
        Command::Source {
            event_id: swat_core::EventId::from_raw(7),
            before: 1,
            after: 3,
        }
    );
    assert_eq!(
        parse_command("source file /tmp/agent.py").unwrap(),
        Command::SourceFile {
            file: "/tmp/agent.py".to_string(),
        }
    );
    assert_eq!(parse_command("source files").unwrap(), Command::SourceFiles);
    assert_eq!(
        parse_command("source view /tmp/agent.py 4 1 2").unwrap(),
        Command::SourceView {
            file: "/tmp/agent.py".to_string(),
            line: 4,
            before: 1,
            after: 2,
        }
    );
    assert_eq!(
        parse_command("breakpoint list").unwrap(),
        Command::Breakpoints
    );
    assert_eq!(
        parse_command("breakpoint show 9").unwrap(),
        Command::BreakpointShow {
            trigger_id: swat_core::TriggerId::from_raw(9),
        }
    );
    assert_eq!(
        parse_command("breakpoint groups").unwrap(),
        Command::BreakpointGroups
    );
    assert_eq!(
        parse_command("breakpoint disable 9").unwrap(),
        Command::TriggerDisable {
            trigger_id: swat_core::TriggerId::from_raw(9),
        }
    );
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
        .execute(r#"breakpoint once pause_search kind == ModelBoundary and artifact.json $.tool == "search""#)
        .unwrap();
    assert!(added.summary.contains("added trigger"));

    let listed = host.execute("breakpoint list").unwrap();
    assert!(
        listed
            .lines
            .iter()
            .any(|line| line == "group=enabled kind=state count=1")
    );
    let trigger_id = listed
        .lines
        .iter()
        .find(|line| line.starts_with("bp="))
        .unwrap()
        .split_whitespace()
        .find_map(|part| part.strip_prefix("bp="))
        .unwrap()
        .to_string();
    let breakpoint_line = listed
        .lines
        .iter()
        .find(|line| line.starts_with("bp="))
        .unwrap();
    assert!(breakpoint_line.contains("pause_search"));
    assert!(breakpoint_line.contains("actions=PauseTarget"));
    assert!(breakpoint_line.contains("hits=0"));
    assert!(breakpoint_line.contains("state=enabled"));

    host.execute("pump").unwrap();
    let triggered = host.execute("pump").unwrap();
    assert!(triggered.lines.iter().any(|line| line.contains("trigger=")));
    assert!(
        triggered
            .lines
            .iter()
            .any(|line| line.contains("control accepted=true"))
    );
    let listed = host.execute("breakpoint list").unwrap();
    let breakpoint_line = listed
        .lines
        .iter()
        .find(|line| line.starts_with("bp="))
        .unwrap();
    assert!(breakpoint_line.contains("hits=1"));
    assert!(breakpoint_line.contains("last_event="));

    let shown = host
        .execute(&format!("breakpoint show {trigger_id}"))
        .unwrap();
    assert!(shown.summary.contains("breakpoint"));
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line.contains("state=enabled"))
    );
    assert!(shown.lines.iter().any(|line| line.contains("activity=hit")));

    let grouped = host.execute("breakpoint groups").unwrap();
    assert!(
        grouped
            .lines
            .iter()
            .any(|line| line.contains("kind=state group=enabled"))
    );
    assert!(
        grouped
            .lines
            .iter()
            .any(|line| line.contains("kind=activity group=hit"))
    );

    let removed = host
        .execute(&format!("breakpoint remove {trigger_id}"))
        .unwrap();
    assert!(removed.summary.contains("removed trigger"));

    let listed_after = host.execute("breakpoint list").unwrap();
    assert_eq!(listed_after.lines.len(), 0);
}

#[test]
fn command_host_can_toggle_trigger_enabled_state() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();
    host.execute(
        r#"trigger-expr pause_search kind == ModelBoundary and artifact.json $.tool == "search""#,
    )
    .unwrap();

    let listed = host.execute("triggers").unwrap();
    let trigger_id = listed.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("trigger="))
        .unwrap()
        .to_string();
    assert!(listed.lines[0].contains("enabled=true"));

    let disabled = host
        .execute(&format!("trigger-disable {trigger_id}"))
        .unwrap();
    assert!(disabled.summary.contains("disabled trigger"));

    let listed = host.execute("triggers").unwrap();
    assert!(listed.lines[0].contains("enabled=false"));

    let enabled = host
        .execute(&format!("trigger-enable {trigger_id}"))
        .unwrap();
    assert!(enabled.summary.contains("enabled trigger"));

    let listed = host.execute("triggers").unwrap();
    assert!(listed.lines[0].contains("enabled=true"));
}

#[test]
fn raw_trigger_listing_remains_available_for_compatibility() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();
    host.execute(
        r#"trigger-expr pause_search kind == ModelBoundary and artifact.json $.tool == "search""#,
    )
    .unwrap();

    let listed = host.execute("triggers").unwrap();
    assert_eq!(listed.lines.len(), 1);
    assert!(listed.lines[0].contains("trigger="));
    assert!(listed.lines[0].contains("actions=PauseTarget"));
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
    host.execute(r#"trigger-snapshot snapshot_search kind == ModelBoundary and artifact.json $.tool == "search" capture search boundary"#)
        .unwrap();
    let listed = host.execute("triggers").unwrap();
    let trigger_id = listed.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("trigger="))
        .unwrap()
        .to_string();
    host.execute(&format!("trigger-disable {trigger_id}"))
        .unwrap();

    let saved = host.execute(&format!("trigger-save {path_str}")).unwrap();
    assert!(saved.summary.contains("saved 1 trigger"));
    let persisted = fs::read_to_string(&path).unwrap();
    assert!(persisted.contains("snapshot_search"));
    assert!(persisted.contains(r#""format_version": 2"#));
    assert!(persisted.contains(r#""kind": "create_snapshot""#));
    assert!(persisted.contains(r#""enabled": false"#));

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
    assert!(listed.lines[0].contains("snapshot_search"));
    assert!(listed.lines[0].contains("enabled=false"));
    assert!(listed.lines[0].contains("hits=0"));
    assert!(listed.lines[0].contains("CreateSnapshot(\"capture search boundary\")"));
    assert!(listed.lines[0].contains("artifact.json $.tool"));

    let trigger_id = listed.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("trigger="))
        .unwrap()
        .to_string();
    restored
        .execute(&format!("trigger-enable {trigger_id}"))
        .unwrap();
    restored.execute("pump").unwrap();
    let triggered = restored.execute("pump").unwrap();
    assert!(triggered.lines.iter().any(|line| line.contains("trigger=")));
    assert!(
        triggered
            .lines
            .iter()
            .any(|line| line.contains("mock snapshot requested: capture search boundary"))
    );
    let listed = restored.execute("triggers").unwrap();
    assert!(listed.lines[0].contains("hits=1"));

    let _ = fs::remove_file(path);
}

#[test]
fn command_host_can_run_until_expression() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();

    let until = host
        .execute(r#"until kind == ModelBoundary and artifact.json $.tool == "search""#)
        .unwrap();
    assert!(until.summary.contains("until matched"));
    assert!(
        until
            .lines
            .iter()
            .any(|line| line.contains("mock target resumed"))
    );
    assert!(until.lines.iter().any(|line| line.contains("TriggerHit")));
    assert!(
        until
            .lines
            .iter()
            .any(|line| line.contains("control accepted=true"))
    );

    let listed = host.execute("triggers").unwrap();
    assert!(listed.lines.is_empty());
}

#[test]
fn command_host_can_list_show_and_replay_snapshots() {
    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );

    host.execute("attach").unwrap();
    host.execute("resume").unwrap();
    host.execute("pump").unwrap();
    host.execute("pump").unwrap();

    let snapshot = host.execute("snapshot shell checkpoint").unwrap();
    assert!(snapshot.summary.contains("created snapshot"));
    let snapshot_id = snapshot
        .lines
        .iter()
        .find_map(|line| {
            line.split_whitespace()
                .find_map(|part| part.strip_prefix("snapshot="))
                .map(ToString::to_string)
        })
        .unwrap();

    let listed = host.execute("snapshots").unwrap();
    assert_eq!(listed.lines.len(), 1);
    assert!(listed.lines[0].contains(&format!("snapshot={snapshot_id}")));
    assert!(listed.lines[0].contains("replay=1"));

    let shown = host
        .execute(&format!("snapshot-show {snapshot_id}"))
        .unwrap();
    assert!(shown.summary.contains("snapshot"));
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line.contains("reason=\"shell checkpoint\""))
    );
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line.contains("replay_directives=1"))
    );

    let replay = host.execute(&format!("replay {snapshot_id}")).unwrap();
    assert!(replay.summary.contains("applied 1 replay directive"));
    assert!(replay.lines.iter().any(|line| line.contains("boundary=42")));
    assert!(
        replay
            .lines
            .iter()
            .any(|line| line.contains("prepared replay for boundary"))
    );
}

#[test]
fn command_host_can_load_legacy_v1_trigger_files() {
    let path = std::env::temp_dir().join(format!(
        "swat-trigger-v1-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(
        &path,
        r#"{
  "format_version": 1,
  "triggers": [
    {
      "name": "pause_search",
      "expr": "kind == ModelBoundary and artifact.json $.tool == \"search\"",
      "fire_once": true
    }
  ]
}"#,
    )
    .unwrap();

    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    host.execute("attach").unwrap();
    host.execute("resume").unwrap();

    let loaded = host
        .execute(&format!("trigger-load {}", path.display()))
        .unwrap();
    assert!(loaded.summary.contains("loaded 1 trigger"));

    let listed = host.execute("triggers").unwrap();
    assert!(listed.lines[0].contains("pause_search"));
    assert!(listed.lines[0].contains("enabled=true"));
    assert!(listed.lines[0].contains("actions=PauseTarget"));

    let _ = fs::remove_file(path);
}

#[test]
fn command_host_can_view_source_file_directly() {
    let path = std::env::temp_dir().join(format!(
        "swat-command-source-view-{}-{}.py",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(
        &path,
        "def alpha():\n    return 1\n\ndef beta():\n    return alpha()\n",
    )
    .unwrap();

    let mut host = CommandHost::new(
        Box::new(MockAdapter::default()),
        Box::new(InMemoryStore::new()),
    );
    let viewed = host
        .execute(&format!("source view {} 4 1 1", path.display()))
        .unwrap();
    assert!(
        viewed
            .summary
            .contains(&format!("source {}:4", path.display()))
    );
    assert!(viewed.lines.iter().any(|line| line.contains("def beta")));

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
emit({"kind": "model", "phase": "request", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model requested", "file": "/tmp/agent.py", "line": 12, "function": "run"})
emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool started", "file": "/tmp/agent.py", "line": 14, "function": "run"})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool completed", "file": "/tmp/agent.py", "line": 18, "function": "run"})
emit({"kind": "model", "phase": "response", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model responded", "file": "/tmp/agent.py", "line": 21, "function": "run"})
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
    assert_eq!(correlated.lines.len(), 7);
    assert!(
        correlated
            .lines
            .iter()
            .any(|line| line.contains("span_ids=model-1,tool-1"))
    );
    assert!(
        correlated
            .lines
            .iter()
            .any(|line| line.contains("entity_count="))
    );

    let spans = host.execute("stack").unwrap();
    assert_eq!(spans.lines.len(), 2);
    assert!(spans.lines[0].contains("frame=0"));
    assert!(spans.lines[0].contains("label=web_search"));
    assert!(spans.lines[1].contains("frame=1"));
    assert!(spans.lines[1].contains("label=gpt-4.1-mini"));

    let frame = host.execute("stack frame 0").unwrap();
    assert!(frame.summary.contains("stack frame 0"));
    assert!(frame.lines.iter().any(|line| line.contains("depth=1")));
    assert!(
        frame
            .lines
            .iter()
            .any(|line| line.contains("label=web_search"))
    );
    assert!(frame.lines.iter().any(|line| line.contains("events:")));

    let model_boundary = spans
        .lines
        .iter()
        .find_map(|line| {
            let raw = line
                .split_whitespace()
                .find_map(|part| part.strip_prefix("boundary="))?;
            if line.contains("label=gpt-4.1-mini") {
                Some(raw.to_string())
            } else {
                None
            }
        })
        .unwrap();
    let span = host
        .execute(&format!("stack show {model_boundary}"))
        .unwrap();
    assert!(span.summary.contains("stack boundary"));
    assert!(
        span.lines
            .iter()
            .any(|line| line.contains("label=gpt-4.1-mini"))
    );
    assert!(span.lines.iter().any(|line| line.contains("events:")));
    assert!(span.lines.iter().any(|line| line.contains("event=")));

    let model_events = host.execute("events ModelBoundary").unwrap();
    assert_eq!(model_events.lines.len(), 2);
    let event_id = model_events.lines[0]
        .split_whitespace()
        .find_map(|part| part.strip_prefix("event="))
        .unwrap()
        .to_string();
    let artifacts = host.execute(&format!("artifacts {event_id}")).unwrap();
    assert_eq!(artifacts.lines.len(), 1);
    assert!(artifacts.lines[0].contains("bytes="));
    assert!(artifacts.lines[0].contains("lines="));
    assert!(artifacts.lines[0].contains("gpt-4.1-mini"));

    let artifact_detail = host.execute(&format!("artifact-show {event_id}")).unwrap();
    assert!(artifact_detail.summary.contains("artifact 0"));
    assert!(
        artifact_detail
            .lines
            .iter()
            .any(|line| line.contains("detail") && line.contains("gpt-4.1-mini"))
    );

    let source_event_id = host
        .execute("events Execution")
        .unwrap()
        .lines
        .into_iter()
        .find(|line| line.contains("planner started"))
        .and_then(|line| {
            line.split_whitespace()
                .find_map(|part| part.strip_prefix("event="))
                .map(ToString::to_string)
        })
        .unwrap();
    let source = host
        .execute(&format!("source show {source_event_id}"))
        .unwrap();
    assert!(source.summary.contains("source unresolved"));
    assert!(
        source
            .lines
            .iter()
            .any(|line| line.contains("failure_kind=MissingFile"))
    );

    let source_file = host.execute("source file /tmp/agent.py").unwrap();
    assert!(source_file.summary.contains("/tmp/agent.py"));
    assert!(
        source_file
            .lines
            .iter()
            .any(|line| line.contains("planner started"))
    );

    let source_files = host.execute("source files").unwrap();
    assert_eq!(source_files.lines.len(), 1);
    assert!(source_files.lines[0].contains("file=/tmp/agent.py"));
    assert!(source_files.lines[0].contains("functions=run"));
}
