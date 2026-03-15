use std::fs;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_adapter_mock::MockAdapter;
use swat_adapter_python::{PythonAdapter, PythonAdapterSpec};
use swat_control::TriggerEngine;
use swat_core::ControlAction;
use swat_script::{LiveScriptSession, ScriptHost};
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
    assert_eq!(host.eval_i64("ctx.stack_frame_count()").unwrap(), 1);
    assert!(
        !host
            .eval_string("ctx.stack_frame_label(0)")
            .unwrap()
            .is_empty()
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
    let script_path_str = script_path.display().to_string();
    assert_eq!(host.eval_i64("ctx.source_file_count()").unwrap(), 1);
    assert!(
        host.eval_i64(&format!(
            r#"ctx.source_file_event_count("{}")"#,
            script_path_str
        ))
        .unwrap()
            >= 1
    );
    assert!(
        host.eval_bool(
            r#"ctx.source_contains("artifact.json $.function == \"helper\"", "def helper", 0, 1)"#,
        )
        .unwrap()
    );
    assert!(
        host.eval_bool(&format!(
            r#"ctx.source_view_contains("{}", 1, 0, 2, "def helper")"#,
            script_path_str
        ))
        .unwrap()
    );

    fs::remove_file(script_path).unwrap();
}

#[test]
fn script_host_can_query_frame_locals_and_registers() {
    let code = r#"
import json
import sys
import time

PREFIX = "__SWATAGENT__"

def emit(record):
    sys.stdout.write(PREFIX + json.dumps(record) + "\n")
    sys.stdout.flush()

emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-9", "name": "web_search", "summary": "tool started", "file": "/tmp/agent.py", "line": 14, "function": "run", "locals": {"query": {"type": "str", "value": "weather"}}, "registers": {"pc": {"group": "trace", "type": "str", "value": "run:14"}}, "patient": {"name": "ui", "handles": [{"id": "h:1001", "resource": "AppResource", "objects": [{"id": "^lui:0002", "class": "GenApplication"}]}], "resources": [{"name": "AppResource", "handle": "h:1001", "objects": ["^lui:0002"]}], "objects": [{"id": "^lui:0002", "class": "GenApplication", "handle": "h:1001", "resource": "AppResource"}]}})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-9", "name": "web_search", "summary": "tool completed", "file": "/tmp/agent.py", "line": 18, "function": "run"})
emit({"kind": "state", "phase": "update", "name": "memory.turn", "summary": "memory updated"})
time.sleep(0.1)
"#;

    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter =
        AgentRuntimeAdapter::new(AgentRuntimeSpec::new("python3").with_args(["-u", "-c", code]));

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    let deadline = Instant::now() + Duration::from_secs(2);
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
        std::thread::sleep(Duration::from_millis(25));
    }

    let mut host = ScriptHost::new(&store, session_id);
    assert_eq!(host.eval_i64("ctx.stack_frame_count()").unwrap(), 1);
    assert_eq!(host.eval_i64("ctx.stack_frame_local_count(0)").unwrap(), 1);
    assert_eq!(
        host.eval_string("ctx.stack_frame_local_name(0, 0)")
            .unwrap(),
        "query"
    );
    assert_eq!(
        host.eval_i64("ctx.stack_frame_register_count(0)").unwrap(),
        1
    );
    assert_eq!(
        host.eval_string("ctx.stack_frame_register_name(0, 0)")
            .unwrap(),
        "pc"
    );
    assert_eq!(host.eval_i64("ctx.patient_count()").unwrap(), 1);
    assert_eq!(host.eval_string("ctx.patient_name(0)").unwrap(), "ui");
    assert_eq!(host.eval_i64(r#"ctx.patient_handle_count("ui")"#).unwrap(), 1);
    assert_eq!(host.eval_i64("ctx.handle_count()").unwrap(), 1);
    assert_eq!(host.eval_string("ctx.handle_name(0)").unwrap(), "h:1001");
    assert_eq!(
        host.eval_i64(r#"ctx.handle_object_count("h:1001")"#).unwrap(),
        1
    );
    assert_eq!(host.eval_i64("ctx.resource_count()").unwrap(), 1);
    assert_eq!(
        host.eval_string("ctx.resource_name(0)").unwrap(),
        "AppResource"
    );
    assert_eq!(
        host.eval_i64(r#"ctx.resource_object_count("AppResource")"#)
            .unwrap(),
        1
    );
    assert_eq!(host.eval_i64("ctx.object_count()").unwrap(), 1);
    assert_eq!(
        host.eval_string("ctx.object_identity(0)").unwrap(),
        "^lui:0002"
    );
    assert_eq!(
        host.eval_string(r#"ctx.object_class("^lui:0002")"#).unwrap(),
        "GenApplication"
    );
    assert_eq!(host.eval_i64("ctx.value_count()").unwrap(), 1);
    assert_eq!(host.eval_string("ctx.value_key(0)").unwrap(), "agent.state");
    assert_eq!(
        host.eval_i64(r#"ctx.value_event_count("agent.state")"#).unwrap(),
        1
    );
    assert_eq!(host.eval_i64("ctx.source_function_count()").unwrap(), 1);
    assert_eq!(
        host.eval_string("ctx.source_function_name(0)").unwrap(),
        "run"
    );
    assert_eq!(
        host.eval_i64(r#"ctx.source_function_event_count("run")"#)
            .unwrap(),
        2
    );
}

#[test]
fn live_script_session_uses_public_mutation_api() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut triggers = TriggerEngine::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let mut live = LiveScriptSession::new(
        session_id,
        &mut manager,
        &mut adapter,
        &mut store,
        &mut triggers,
    );
    assert_eq!(live.event_count(), 1);
    assert_eq!(live.trigger_count(), 0);
    assert_eq!(live.breakpoint_count(), 0);
    assert!(live.resume().unwrap().contains("resumed"));

    let trigger_id = live
        .add_trigger_expr(
            "pause_search",
            r#"kind == ModelBoundary and artifact.json $.tool == "search""#,
            true,
        )
        .unwrap();
    assert_eq!(live.trigger_count(), 1);
    assert_eq!(live.breakpoint_count(), 1);
    assert_eq!(live.breakpoint_enabled_count(), 1);
    assert_eq!(live.breakpoint_group_count("state", "enabled").unwrap(), 1);
    assert_eq!(live.disable_trigger(trigger_id).unwrap(), true);
    assert_eq!(live.breakpoint_enabled_count(), 0);
    assert_eq!(live.breakpoint_group_count("state", "disabled").unwrap(), 1);
    assert_eq!(live.enable_trigger(trigger_id).unwrap(), false);
    assert_eq!(live.breakpoint_enabled_count(), 1);

    assert!(
        !live
            .define_breakpoint_predicate(
                "search_tool",
                r#"kind == ModelBoundary and artifact.json $.tool == "search""#,
            )
            .unwrap()
    );
    assert_eq!(live.breakpoint_predicate_count(), 1);
    assert_eq!(
        live.breakpoint_predicate_breakpoint_count("search_tool")
            .unwrap(),
        0
    );

    let grouped_id = live
        .add_trigger_expr(
            "grouped_search",
            r#"kind == ModelBoundary and artifact.json $.tool == "search""#,
            false,
        )
        .unwrap();
    assert_eq!(live.breakpoint_definition_group_count(), 0);
    assert!(
        live.remove_trigger(grouped_id)
            .unwrap()
            .contains("grouped_search")
    );

    let watchpoint_id = live
        .add_watchpoint_with_scope(
            "memory_turn",
            "agent.state",
            "$.status",
            250,
            "Lifecycle",
            "loaded",
            true,
            "load",
        )
        .unwrap();
    assert_eq!(live.watchpoint_count(), 1);
    assert_eq!(live.watchpoint_hit_count(watchpoint_id).unwrap(), 0);
    assert_eq!(live.breakpoint_definition_group_count(), 1);
    assert_eq!(live.breakpoint_definition_group_size("load"), 1);
    assert!(live.breakpoint_definition_group_enabled("load").unwrap());
    assert_eq!(
        live.set_breakpoint_group_enabled("load", false).unwrap(),
        true
    );
    assert!(!live.breakpoint_definition_group_enabled("load").unwrap());
    assert_eq!(
        live.set_breakpoint_group_enabled("load", true).unwrap(),
        false
    );
    assert!(live.breakpoint_definition_group_enabled("load").unwrap());

    live.pump_once().unwrap();
    live.pump_once().unwrap();
    assert_eq!(live.stack_frame_count().unwrap(), 1);
    assert!(!live.stack_frame_label(0).unwrap().is_empty());
    let snapshot_id = live.snapshot("script checkpoint").unwrap();
    assert!(snapshot_id.raw() > 0);
    assert_eq!(
        live.query_count(r#"kind == ModelBoundary and artifact.json $.tool == "search""#)
            .unwrap(),
        1
    );
    assert_eq!(
        live.remove_breakpoint_predicate("search_tool").unwrap(),
        "search_tool"
    );
    assert_eq!(live.breakpoint_predicate_count(), 0);
    assert_eq!(live.remove_trigger(watchpoint_id).unwrap(), "memory_turn");
    assert_eq!(live.remove_trigger(trigger_id).unwrap(), "pause_search");
    assert_eq!(live.trigger_count(), 0);
}
