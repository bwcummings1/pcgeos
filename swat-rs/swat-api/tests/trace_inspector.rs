use std::fs;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_adapter_mock::MockAdapter;
use swat_api::{
    BreakpointActivity, BreakpointDisposition, BreakpointGroupKind, BreakpointLifetime,
    BreakpointState, LiveSessionApi, TraceInspector, WatchpointSpec,
};
use swat_control::{Trigger, TriggerAction, TriggerEngine, TriggerPredicate, pump_with_triggers};
use swat_core::{ControlAction, EventKind, EventPayload, PolicyVerdict, TriggerId};
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
    let snapshot = manager
        .control(
            session_id,
            &mut adapter,
            ControlAction::CreateSnapshot {
                reason: "api snapshot".to_string(),
            },
            &mut store,
        )
        .unwrap();
    let snapshot_id = snapshot.snapshot.unwrap().snapshot_id;

    let inspector = TraceInspector::new(&store);
    let boundary_events = inspector.events_by_kind(session_id, EventKind::ModelBoundary);
    assert_eq!(boundary_events.len(), 1);

    let decoded = inspector.decoded_artifacts(&boundary_events[0]).unwrap();
    assert_eq!(decoded.len(), 1);
    let presentations = inspector
        .artifact_presentations(&boundary_events[0], 24)
        .unwrap();
    assert_eq!(presentations.len(), 1);
    assert!(presentations[0].detail.contains('\n'));
    assert!(presentations[0].preview.len() <= 27);

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
    let source = inspector
        .source_inspection(&boundary_events[0], 1, 1)
        .unwrap();
    assert!(source.location.is_none());
    assert!(source.failure.is_none());

    let snapshots = inspector.session_snapshots(session_id);
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].snapshot_id, snapshot_id);
    let inspection = inspector.snapshot_inspection(snapshot_id).unwrap();
    assert_eq!(inspection.snapshot.reason, "api snapshot");
    assert!(inspection.captured_event_count >= 5);
    assert_eq!(inspection.replay_directive_count, 1);
    assert!(
        inspector
            .replay_plan_for_snapshot(snapshot_id)
            .unwrap()
            .has_boundary(swat_adapter_mock::MOCK_BOUNDARY_ID)
    );
    assert!(
        inspector
            .replay_plan_for_boundary(session_id, swat_adapter_mock::MOCK_BOUNDARY_ID)
            .has_boundary(swat_adapter_mock::MOCK_BOUNDARY_ID)
    );
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
emit({"kind": "planner", "phase": "start", "name": "draft-answer", "summary": "planner started", "file": "/tmp/agent.py", "line": 10, "function": "run"})
emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool started", "file": "/tmp/agent.py", "line": 14, "function": "run", "locals": {"query": {"type": "str", "value": "weather"}, "limit": {"type": "int", "value": 3}}, "registers": {"pc": {"group": "trace", "type": "str", "value": "run:14"}, "phase": {"group": "trace", "type": "str", "value": "start"}}, "patient": {"name": "ui", "id": "patient-ui", "role": "application", "status": "running", "runtime": "pcgeos", "path": "/repo/UI.geo", "default": True, "handles": [{"id": "h:1001", "kind": "resource", "state": ["in", "fixed"], "resource": "AppResource", "objects": [{"id": "^lui:0002", "class": "GenApplication", "state": ["usable"]}]}], "resources": [{"name": "AppResource", "handle": "h:1001", "kind": "ui", "source_file": "/repo/ui.goc", "objects": ["^lui:0002"]}], "objects": [{"id": "^lui:0002", "class": "GenApplication", "handle": "h:1001", "resource": "AppResource", "address": "^lui:0002"}]}})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-7", "name": "web_search", "summary": "tool completed", "file": "/tmp/agent.py", "line": 18, "function": "run", "handle": {"id": "h:1001", "patient": "ui", "resource": "AppResource", "attached": True, "size": 8192}})
emit({"kind": "model", "phase": "response", "span_id": "model-1", "correlation_id": "req-7", "name": "gpt-4.1-mini", "summary": "model responded", "file": "/tmp/agent.py", "line": 21, "function": "run"})
emit({"kind": "state", "phase": "update", "name": "memory.turn", "summary": "memory updated"})
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
    let relations = inspector.entity_relations(session_id).unwrap();
    assert!(relations.iter().any(|relation| {
        let names = [
            (&relation.left.kind, relation.left.name.as_str()),
            (&relation.right.kind, relation.right.name.as_str()),
        ];
        names.contains(&(&swat_resolver::ResolvedEntityKind::CorrelationId, "req-7"))
            && names.contains(&(&swat_resolver::ResolvedEntityKind::ToolName, "web_search"))
    }));
    let groups = inspector.correlation_groups(session_id).unwrap();
    assert!(groups.iter().any(|group| {
        group.correlation_id == "req-7"
            && group.span_ids.iter().any(|span| span == "model-1")
            && group
                .entities
                .iter()
                .any(|entity| entity.name == "gpt-4.1-mini")
    }));

    let correlated = inspector.events_for_correlation(session_id, "req-7");
    assert_eq!(correlated.len(), 4);
    assert_eq!(
        inspector
            .events_for_span(session_id, "model-1")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        inspector
            .events_for_source_file(session_id, "/tmp/agent.py")
            .unwrap()
            .len(),
        4
    );
    let source_files = inspector.source_files(session_id).unwrap();
    assert_eq!(source_files.len(), 1);
    assert_eq!(source_files[0].file, "/tmp/agent.py");
    assert_eq!(source_files[0].event_count, 4);
    assert_eq!(source_files[0].first_line, Some(10));
    assert_eq!(source_files[0].last_line, Some(21));
    assert_eq!(source_files[0].functions, vec!["run".to_string()]);
    assert!(!source_files[0].is_real_path);
    let source_functions = inspector.source_functions(session_id).unwrap();
    assert_eq!(source_functions.len(), 1);
    assert_eq!(source_functions[0].function, "run");
    assert_eq!(source_functions[0].file, "/tmp/agent.py");
    assert_eq!(source_functions[0].event_count, 4);
    assert_eq!(source_functions[0].first_line, Some(10));
    assert_eq!(source_functions[0].last_line, Some(21));
    assert_eq!(
        inspector
            .events_for_source_function(session_id, "run")
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        inspector
            .events_for_value_key(session_id, "agent.state")
            .unwrap()
            .len(),
        1
    );
    let values = inspector.observed_values(session_id).unwrap();
    assert!(values.iter().any(|value| {
        value.value_key == "agent.state"
            && value.event_count == 1
            && value.last_summary.as_deref() == Some("memory updated")
    }));
    let value_detail = inspector
        .observed_value_detail(session_id, "agent.state")
        .unwrap()
        .unwrap();
    assert_eq!(value_detail.history.len(), 1);
    assert!(value_detail.history[0]
        .preview
        .as_deref()
        .unwrap_or_default()
        .contains("memory.turn"));
    assert_eq!(inspector.events_for_patient(session_id, "ui").unwrap().len(), 2);
    assert_eq!(
        inspector.events_for_handle(session_id, "h:1001").unwrap().len(),
        2
    );
    assert_eq!(
        inspector
            .events_for_resource(session_id, "AppResource")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        inspector
            .events_for_object(session_id, "^lui:0002")
            .unwrap()
            .len(),
        1
    );

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

    let frames = inspector.stack_frames(session_id).unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].frame_index, 0);
    assert_eq!(frames[0].label, "web_search");
    assert_eq!(frames[0].depth, 1);
    assert_eq!(frames[0].span_id.as_deref(), Some("tool-1"));
    assert_eq!(frames[0].source_file.as_deref(), Some("/tmp/agent.py"));
    assert_eq!(frames[0].source_line, Some(14));
    assert_eq!(frames[1].label, "gpt-4.1-mini");
    assert_eq!(frames[1].depth, 0);
    assert_eq!(frames[1].span_id.as_deref(), Some("model-1"));
    assert_eq!(frames[1].correlation_id.as_deref(), Some("req-7"));
    assert_eq!(
        inspector
            .stack_frame(session_id, 0)
            .unwrap()
            .unwrap()
            .boundary_id,
        frames[0].boundary_id
    );
    assert_eq!(
        inspector
            .stack_frame_by_boundary(session_id, boundary_id)
            .unwrap()
            .unwrap()
            .label,
        "gpt-4.1-mini"
    );
    let frame_inspection = inspector
        .stack_frame_inspection(session_id, 0)
        .unwrap()
        .unwrap();
    assert_eq!(frame_inspection.locals.len(), 2);
    assert_eq!(frame_inspection.locals[0].name, "limit");
    assert_eq!(frame_inspection.locals[0].type_name.as_deref(), Some("int"));
    assert_eq!(frame_inspection.locals[1].name, "query");
    assert_eq!(
        frame_inspection.locals[1].value_kind,
        swat_api::InspectedValueKind::String
    );
    assert_eq!(frame_inspection.registers.len(), 2);
    assert_eq!(
        frame_inspection.registers[0].group.as_deref(),
        Some("trace")
    );
    assert_eq!(frame_inspection.registers[0].name, "pc");
    assert_eq!(
        inspector
            .stack_frame_locals(session_id, 0)
            .unwrap()
            .iter()
            .map(|local| local.name.as_str())
            .collect::<Vec<_>>(),
        vec!["limit", "query"]
    );
    assert_eq!(
        inspector
            .stack_frame_registers(session_id, 0)
            .unwrap()
            .iter()
            .map(|register| register.name.as_str())
            .collect::<Vec<_>>(),
        vec!["pc", "phase"]
    );

    let event_entities = inspector.event_typed_entities(&correlated[1]).unwrap();
    assert_eq!(event_entities.patients.len(), 1);
    assert_eq!(event_entities.handles.len(), 1);
    assert_eq!(event_entities.resources.len(), 1);

    let patients = inspector.patients(session_id).unwrap();
    assert_eq!(patients.len(), 1);
    assert_eq!(patients[0].name, "ui");
    assert_eq!(patients[0].handle_count, 1);
    assert_eq!(patients[0].resource_count, 1);
    assert_eq!(patients[0].object_count, 1);
    assert_eq!(patients[0].runtime.as_deref(), Some("pcgeos"));
    assert_eq!(patients[0].path.as_deref(), Some("/repo/UI.geo"));
    assert_eq!(patients[0].is_default, Some(true));

    let patient_detail = inspector.patient_detail(session_id, "ui").unwrap().unwrap();
    assert_eq!(patient_detail.handles, vec!["h:1001".to_string()]);
    assert_eq!(patient_detail.resources, vec!["AppResource".to_string()]);
    assert_eq!(patient_detail.objects, vec!["^lui:0002".to_string()]);
    assert_eq!(patient_detail.source_files, vec!["/tmp/agent.py".to_string()]);

    let handles = inspector.handles(session_id).unwrap();
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].key, "h:1001");
    assert_eq!(handles[0].patient.as_deref(), Some("ui"));
    assert_eq!(handles[0].resource.as_deref(), Some("AppResource"));
    assert_eq!(handles[0].size, Some(8192));
    assert_eq!(handles[0].attached, Some(true));
    assert_eq!(handles[0].object_count, 1);

    let handle_detail = inspector.handle_detail(session_id, "h:1001").unwrap().unwrap();
    assert_eq!(handle_detail.objects, vec!["^lui:0002".to_string()]);
    assert_eq!(handle_detail.source_files, vec!["/tmp/agent.py".to_string()]);

    let resources = inspector.resources(session_id).unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].name, "AppResource");
    assert_eq!(resources[0].patient.as_deref(), Some("ui"));
    assert_eq!(resources[0].handle.as_deref(), Some("h:1001"));
    assert_eq!(resources[0].source_file.as_deref(), Some("/repo/ui.goc"));

    let resource_detail = inspector
        .resource_detail(session_id, "AppResource")
        .unwrap()
        .unwrap();
    assert_eq!(resource_detail.objects, vec!["^lui:0002".to_string()]);

    let objects = inspector.objects(session_id).unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].key, "^lui:0002");
    assert_eq!(objects[0].class_name.as_deref(), Some("GenApplication"));
    assert_eq!(objects[0].patient.as_deref(), Some("ui"));
    assert_eq!(objects[0].handle.as_deref(), Some("h:1001"));
    assert_eq!(objects[0].resource.as_deref(), Some("AppResource"));

    let object_detail = inspector.object_detail(session_id, "^lui:0002").unwrap().unwrap();
    assert_eq!(object_detail.object.address.as_deref(), Some("^lui:0002"));
    assert_eq!(object_detail.source_files, vec!["/tmp/agent.py".to_string()]);
}

#[test]
fn trace_inspector_can_view_source_files_directly() {
    let path = std::env::temp_dir().join(format!(
        "swat-source-view-{}-{}.py",
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

    let store = InMemoryStore::new();
    let inspector = TraceInspector::new(&store);
    let snippet = inspector
        .source_file_view(&path.display().to_string(), 4, 1, 1)
        .unwrap();

    assert_eq!(snippet.location.file, path.display().to_string());
    assert_eq!(snippet.focus_line, 4);
    assert_eq!(snippet.start_line, 3);
    assert_eq!(snippet.end_line, 5);
    assert!(
        snippet
            .lines
            .iter()
            .any(|line| line.text.contains("def beta"))
    );

    let _ = fs::remove_file(path);
}

#[test]
fn live_session_api_audits_trigger_mutations() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let added = {
        let mut api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
        api.add_trigger(
            session_id,
            Trigger::new(
                "pause_search",
                TriggerPredicate::SummaryContains("search".to_string()),
                vec![],
            ),
        )
        .unwrap()
    };
    assert_eq!(added.policy_events.len(), 1);
    assert!(matches!(
        added.policy_events[0].payload,
        EventPayload::Policy {
            verdict: PolicyVerdict::Allow,
            ..
        }
    ));

    let removed = {
        let mut api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
        api.remove_trigger(session_id, added.value).unwrap()
    };
    assert_eq!(removed.policy_events.len(), 1);
    assert!(matches!(
        removed.policy_events[0].payload,
        EventPayload::Policy {
            verdict: PolicyVerdict::Allow,
            ..
        }
    ));

    let err = {
        let mut api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
        api.set_trigger_enabled(session_id, TriggerId::from_raw(999_999), false)
            .unwrap_err()
    };
    assert!(err.to_string().contains("unknown trigger"));
    assert!(store.events_for_session(session_id).iter().any(|event| {
        matches!(
            event.payload,
            EventPayload::Policy {
                verdict: PolicyVerdict::Deny,
                ..
            }
        )
    }));
}

#[test]
fn live_session_api_projects_debugger_oriented_breakpoint_views() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();

    let pause_breakpoint = Trigger::new(
        "pause_search",
        TriggerPredicate::Expr(
            parse_expression(r#"kind == ModelBoundary and artifact.json $.tool == "search""#)
                .unwrap(),
        ),
        vec![TriggerAction::PauseTarget],
    )
    .fire_once();
    let pause_breakpoint_id = pause_breakpoint.trigger_id;

    let snapshot_breakpoint = Trigger::new(
        "capture_search",
        TriggerPredicate::Expr(
            parse_expression(r#"kind == ModelBoundary and artifact.json $.tool == "search""#)
                .unwrap(),
        ),
        vec![TriggerAction::CreateSnapshot {
            reason: "capture search boundary".to_string(),
        }],
    )
    .disabled();

    {
        let mut api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
        api.add_trigger(session_id, pause_breakpoint).unwrap();
        api.add_trigger(session_id, snapshot_breakpoint).unwrap();
    }

    pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();

    let api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
    let breakpoints = api.breakpoint_summaries();
    assert_eq!(breakpoints.len(), 2);

    let pause = breakpoints
        .iter()
        .find(|breakpoint| breakpoint.name == "pause_search")
        .unwrap();
    assert_eq!(pause.trigger_id, pause_breakpoint_id);
    assert_eq!(pause.state, BreakpointState::Enabled);
    assert_eq!(pause.lifetime, BreakpointLifetime::Once);
    assert_eq!(pause.disposition, BreakpointDisposition::Pause);
    assert_eq!(pause.activity, BreakpointActivity::Hit);
    assert_eq!(pause.hit_count, 1);
    assert!(pause.predicate.contains("kind == ModelBoundary"));
    assert_eq!(pause.actions, vec!["PauseTarget".to_string()]);

    let snapshot = breakpoints
        .iter()
        .find(|breakpoint| breakpoint.name == "capture_search")
        .unwrap();
    assert_eq!(snapshot.state, BreakpointState::Disabled);
    assert_eq!(snapshot.lifetime, BreakpointLifetime::Persistent);
    assert_eq!(snapshot.disposition, BreakpointDisposition::Snapshot);
    assert_eq!(snapshot.activity, BreakpointActivity::NeverHit);
    assert!(
        snapshot
            .actions
            .iter()
            .any(|action| action.contains("CreateSnapshot"))
    );

    let detail = api.breakpoint_detail(pause_breakpoint_id).unwrap();
    assert_eq!(detail.breakpoint.name, "pause_search");
    assert!(matches!(
        detail.last_hit_event.as_ref().map(|event| event.kind),
        Some(EventKind::ModelBoundary)
    ));

    let groups = api.breakpoint_groups();
    assert!(groups.iter().any(|group| {
        group.kind == BreakpointGroupKind::State
            && group.label == "enabled"
            && group.breakpoints.len() == 1
    }));
    assert!(groups.iter().any(|group| {
        group.kind == BreakpointGroupKind::State
            && group.label == "disabled"
            && group.breakpoints.len() == 1
    }));
    assert!(groups.iter().any(|group| {
        group.kind == BreakpointGroupKind::Disposition
            && group.label == "pause"
            && group.breakpoints.len() == 1
    }));
    assert!(groups.iter().any(|group| {
        group.kind == BreakpointGroupKind::Disposition
            && group.label == "snapshot"
            && group.breakpoints.len() == 1
    }));
}

#[test]
fn live_session_api_projects_named_breakpoint_predicates_and_groups() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    {
        let mut api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
        api.define_breakpoint_predicate(
            session_id,
            "search_tool",
            TriggerPredicate::Expr(
                parse_expression(r#"kind == ModelBoundary and artifact.json $.tool == "search""#)
                    .unwrap(),
            ),
        )
        .unwrap();
        api.add_trigger(
            session_id,
            Trigger::new(
                "pause_search",
                TriggerPredicate::Named("search_tool".to_string()),
                vec![TriggerAction::PauseTarget],
            )
            .in_group("search"),
        )
        .unwrap();
        api.set_breakpoint_group_enabled(session_id, "search", false)
            .unwrap();
    }

    let api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
    let predicates = api.breakpoint_predicates();
    assert_eq!(predicates.len(), 1);
    assert_eq!(predicates[0].name, "search_tool");
    assert_eq!(predicates[0].breakpoint_count, 1);
    assert!(predicates[0].predicate.contains("kind == ModelBoundary"));

    let groups = api.breakpoint_definition_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "search");
    assert!(!groups[0].enabled);
    assert_eq!(groups[0].breakpoints.len(), 1);

    let breakpoints = api.breakpoint_summaries();
    assert_eq!(breakpoints.len(), 1);
    assert_eq!(breakpoints[0].group.as_deref(), Some("search"));
    assert_eq!(breakpoints[0].group_enabled, Some(false));
    assert_eq!(breakpoints[0].configured_state, BreakpointState::Enabled);
    assert_eq!(breakpoints[0].state, BreakpointState::Disabled);
    assert_eq!(
        breakpoints[0].predicate_name.as_deref(),
        Some("search_tool")
    );
    assert_eq!(breakpoints[0].predicate, "@search_tool");
}

#[test]
fn live_session_api_projects_watchpoint_metadata() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    let trigger_id = {
        let mut api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
        api.add_watchpoint(
            session_id,
            WatchpointSpec::new("memory_turn", "agent.state")
                .at_path("$.status")
                .after_millis(250)
                .in_event_kind(EventKind::Lifecycle)
                .with_summary_contains("loaded")
                .in_group("load"),
        )
        .unwrap()
        .value
    };

    let api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
    let watchpoints = api.watchpoint_summaries();
    assert_eq!(watchpoints.len(), 1);
    assert_eq!(watchpoints[0].breakpoint.trigger_id, trigger_id);
    assert_eq!(watchpoints[0].breakpoint.group.as_deref(), Some("load"));
    assert_eq!(watchpoints[0].value_key, "agent.state");
    assert_eq!(watchpoints[0].path.as_deref(), Some("$.status"));
    assert_eq!(watchpoints[0].after_millis, Some(250));
    assert_eq!(watchpoints[0].event_kind, Some(EventKind::Lifecycle));
    assert_eq!(watchpoints[0].summary_contains.as_deref(), Some("loaded"));

    let detail = api.watchpoint_detail(trigger_id).unwrap();
    assert_eq!(detail.watchpoint.value_key, "agent.state");
    assert!(detail.last_hit_event.is_none());
}

#[test]
fn live_session_api_denies_capability_blocked_snapshot_requests() {
    let spec =
        LocalProcessAdapter::new(LocalProcessSpec::new("/bin/sh").with_args(["-c", "sleep 0.2"]));
    let mut adapter = spec;
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut engine = TriggerEngine::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let err = {
        let mut api = LiveSessionApi::new(&mut manager, &mut adapter, &mut store, &mut engine);
        api.control(
            session_id,
            ControlAction::CreateSnapshot {
                reason: "denied".to_string(),
            },
        )
        .unwrap_err()
    };
    assert!(err.to_string().contains("policy denied control"));
    assert!(store.events_for_session(session_id).iter().any(|event| {
        matches!(
            event.payload,
            EventPayload::Policy {
                verdict: PolicyVerdict::Deny,
                ..
            }
        )
    }));
}
