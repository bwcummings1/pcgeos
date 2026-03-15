use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_adapter_mock::MockAdapter;
use swat_control::{
    StopReasonKind, Trigger, TriggerAction, TriggerEngine, TriggerPredicate, last_control_response,
    pump_with_triggers,
};
use swat_core::{
    AdapterEmission, ArtifactAccess, ArtifactBinding, ArtifactEncoding, ControlAction,
    EventEnvelope, EventKind, EventPayload, PendingArtifact, PendingEvent, SessionId, TargetId,
    Timestamp,
};
use swat_expr::parse_expression;
use swat_schema::SchemaNode;
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::QueriedValue;

#[test]
fn mock_boundary_artifact_can_trigger_pause() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new();
    engine.add_trigger(
        Trigger::new(
            "pause-on-call-tool",
            TriggerPredicate::ArtifactUtf8Contains("call-tool".to_string()),
            vec![TriggerAction::PauseTarget],
        )
        .fire_once(),
    );

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();

    let first = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    assert!(first.trigger_matches.is_empty());

    let second = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    assert_eq!(second.trigger_matches.len(), 1);
    assert_eq!(second.trigger_events.len(), 1);
    assert_eq!(second.control_reports.len(), 1);
    assert!(last_control_response(&second).unwrap().accepted);

    let trigger_event = &second.trigger_events[0];
    assert_eq!(trigger_event.kind, EventKind::TriggerHit);
    match &trigger_event.payload {
        EventPayload::Trigger { summary, .. } => {
            assert!(summary.contains("pause-on-call-tool"));
        }
        payload => panic!("unexpected payload: {payload:?}"),
    }

    let trigger = &engine.triggers()[0];
    let matched_event = store
        .events_for_session(session_id)
        .into_iter()
        .find(|event| event.event_id == second.trigger_matches[0].event_id)
        .unwrap();
    assert_eq!(trigger.hit_count, 1);
    assert_eq!(
        trigger.last_hit_event_id,
        Some(second.trigger_matches[0].event_id)
    );
    assert_eq!(
        trigger.last_hit_sequence_no,
        Some(matched_event.sequence_no)
    );
}

#[test]
fn mock_boundary_json_path_can_trigger_pause() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new().with_trigger(
        Trigger::new(
            "pause-on-tool-search",
            TriggerPredicate::ArtifactJsonPathEquals {
                path: "$.tool".to_string(),
                expected: QueriedValue::String("search".to_string()),
            },
            vec![TriggerAction::PauseTarget],
        )
        .fire_once(),
    );

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();

    let first = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    assert!(first.trigger_matches.is_empty());

    let second = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    assert_eq!(second.trigger_matches.len(), 1);
    assert_eq!(second.trigger_events.len(), 1);
    assert!(last_control_response(&second).unwrap().accepted);
}

#[test]
fn mock_boundary_can_trigger_snapshot_creation() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new().with_trigger(
        Trigger::new(
            "snapshot-on-tool-search",
            TriggerPredicate::ArtifactJsonPathEquals {
                path: "$.tool".to_string(),
                expected: QueriedValue::String("search".to_string()),
            },
            vec![TriggerAction::CreateSnapshot {
                reason: "capture-search-boundary".to_string(),
            }],
        )
        .fire_once(),
    );

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();

    pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    let second = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();

    assert_eq!(second.trigger_matches.len(), 1);
    assert_eq!(second.control_reports.len(), 1);
    let response = last_control_response(&second).unwrap();
    assert!(response.accepted);
    assert!(response.summary.contains("capture-search-boundary"));
    let trigger = &engine.triggers()[0];
    assert_eq!(trigger.hit_count, 1);
    assert_eq!(
        trigger.last_hit_event_id,
        Some(second.trigger_matches[0].event_id)
    );
    let matched_event = store
        .events_for_session(session_id)
        .into_iter()
        .find(|event| event.event_id == second.trigger_matches[0].event_id)
        .unwrap();
    assert_eq!(
        trigger.last_hit_sequence_no,
        Some(matched_event.sequence_no)
    );
}

#[test]
fn disabled_trigger_can_be_reenabled_before_matching_again() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let trigger = Trigger::new(
        "pause-on-tool-search",
        TriggerPredicate::ArtifactJsonPathEquals {
            path: "$.tool".to_string(),
            expected: QueriedValue::String("search".to_string()),
        },
        vec![TriggerAction::PauseTarget],
    )
    .fire_once()
    .disabled();
    let trigger_id = trigger.trigger_id;
    let mut engine = TriggerEngine::new().with_trigger(trigger);

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();

    pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    let second = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    assert!(second.trigger_matches.is_empty());

    let boundary_event = store
        .events_for_session(session_id)
        .into_iter()
        .find(|event| event.kind == EventKind::ModelBoundary)
        .unwrap();
    assert_eq!(engine.set_enabled(trigger_id, true), Some(false));

    let matches = engine.evaluate_event(&boundary_event, &store);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].trigger_id, trigger_id);
    let trigger = &engine.triggers()[0];
    assert_eq!(trigger.hit_count, 1);
    assert_eq!(trigger.last_hit_event_id, Some(boundary_event.event_id));
    assert_eq!(
        trigger.last_hit_sequence_no,
        Some(boundary_event.sequence_no)
    );
}

#[test]
fn named_predicates_and_group_policies_gate_breakpoint_matches() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new();
    engine.define_predicate(
        "search_tool",
        TriggerPredicate::ArtifactJsonPathEquals {
            path: "$.tool".to_string(),
            expected: QueriedValue::String("search".to_string()),
        },
    );
    let trigger = Trigger::new(
        "pause-on-tool-search",
        TriggerPredicate::Named("search_tool".to_string()),
        vec![TriggerAction::PauseTarget],
    )
    .in_group("search");
    let trigger_id = trigger.trigger_id;
    engine.add_trigger(trigger);

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();

    pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();

    assert_eq!(engine.set_group_enabled("search", false), Some(true));
    let second = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    assert!(second.trigger_matches.is_empty());

    let boundary_event = store
        .events_for_session(session_id)
        .into_iter()
        .find(|event| event.kind == EventKind::ModelBoundary)
        .unwrap();
    assert_eq!(engine.set_group_enabled("search", true), Some(false));
    let matches = engine.evaluate_event(&boundary_event, &store);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].trigger_id, trigger_id);
    assert_eq!(matches[0].group.as_deref(), Some("search"));
    assert_eq!(matches[0].predicate_name.as_deref(), Some("search_tool"));
}

#[test]
fn controlled_pump_reports_breakpoint_stop_reasons() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new().with_trigger(
        Trigger::new(
            "pause-on-tool-search",
            TriggerPredicate::ArtifactJsonPathEquals {
                path: "$.tool".to_string(),
                expected: QueriedValue::String("search".to_string()),
            },
            vec![TriggerAction::PauseTarget],
        )
        .fire_once(),
    );

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();

    pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    let second = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();

    assert!(
        second
            .stop_reasons
            .iter()
            .any(|reason| reason.kind == StopReasonKind::Breakpoint
                && reason.trigger_name.as_deref() == Some("pause-on-tool-search"))
    );
}

#[test]
fn controlled_pump_reports_target_exit_stop_reasons() {
    let spec = LocalProcessSpec::new("/bin/sh").with_args(["-c", "exit 0"]);
    let mut adapter = LocalProcessAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut engine = TriggerEngine::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let report = pump_with_triggers(
            &mut manager,
            session_id,
            &mut adapter,
            &mut store,
            &mut engine,
        )
        .unwrap();
        if report
            .stop_reasons
            .iter()
            .any(|reason| reason.kind == StopReasonKind::TargetExit)
        {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }

    panic!("local process did not surface a target-exit stop reason");
}

#[test]
fn value_watchpoints_fire_only_after_the_observed_value_changes() {
    let mut store = InMemoryStore::new();
    let session_id = SessionId::new();
    let target_id = TargetId::new();
    let mut next_sequence = 1;
    let mut engine = TriggerEngine::new().with_trigger(Trigger::new(
        "watch-agent-state-count",
        TriggerPredicate::ValueChanged {
            value_key: "agent.state".to_string(),
            path: Some("$.count".to_string()),
        },
        vec![TriggerAction::PauseTarget],
    ));

    let first = ingest_json_value_event(
        &mut store,
        session_id,
        target_id,
        &mut next_sequence,
        1_000,
        "agent.state",
        "state count 1",
        r#"{"count":1}"#,
    );
    assert!(engine.evaluate_event(&first, &store).is_empty());

    let second = ingest_json_value_event(
        &mut store,
        session_id,
        target_id,
        &mut next_sequence,
        1_050,
        "agent.state",
        "state count 1 again",
        r#"{"count":1}"#,
    );
    assert!(engine.evaluate_event(&second, &store).is_empty());

    let third = ingest_json_value_event(
        &mut store,
        session_id,
        target_id,
        &mut next_sequence,
        1_100,
        "agent.state",
        "state count 2",
        r#"{"count":2}"#,
    );
    let matches = engine.evaluate_event(&third, &store);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].trigger_name, "watch-agent-state-count");
}

#[test]
fn lifecycle_load_conditions_can_be_gated_by_elapsed_time() {
    let mut store = InMemoryStore::new();
    let session_id = SessionId::new();
    let target_id = TargetId::new();
    let mut next_sequence = 1;
    let mut engine = TriggerEngine::new().with_trigger(Trigger::new(
        "delayed-load-stop",
        TriggerPredicate::All(vec![
            TriggerPredicate::EventKindIs(EventKind::Lifecycle),
            TriggerPredicate::SummaryContains("loaded".to_string()),
            TriggerPredicate::ObservedAfter { millis: 500 },
        ]),
        vec![TriggerAction::PauseTarget],
    ));

    let early = ingest_text_event(
        &mut store,
        session_id,
        target_id,
        &mut next_sequence,
        1_000,
        EventKind::Lifecycle,
        "resource loaded too early",
    );
    assert!(engine.evaluate_event(&early, &store).is_empty());

    let late = ingest_text_event(
        &mut store,
        session_id,
        target_id,
        &mut next_sequence,
        1_600,
        EventKind::Lifecycle,
        "resource loaded after delay",
    );
    let matches = engine.evaluate_event(&late, &store);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].trigger_name, "delayed-load-stop");
}

#[test]
fn local_process_output_can_semantically_pause_target() {
    let spec = LocalProcessSpec::new("/bin/sh").with_args(["-c", "printf 'pause-me\\n'; sleep 2"]);
    let mut adapter = LocalProcessAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut engine = TriggerEngine::new().with_trigger(
        Trigger::new(
            "pause-on-stdout-token",
            TriggerPredicate::ArtifactUtf8Contains("pause-me".to_string()),
            vec![TriggerAction::PauseTarget],
        )
        .fire_once(),
    );

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let mut paused = false;
    let deadline = Instant::now() + Duration::from_secs(2);
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
    assert!(store.events_for_session(session_id).iter().any(|event| {
        matches!(
            &event.payload,
            EventPayload::Trigger { summary, .. } if summary.contains("pause-on-stdout-token")
        )
    }));

    let pause_window_deadline = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < pause_window_deadline {
        manager.pump(session_id, &mut adapter, &mut store).unwrap();
        thread::sleep(Duration::from_millis(25));
    }
    let exited_while_paused = store.events_for_session(session_id).iter().any(|event| {
        matches!(
            &event.payload,
            EventPayload::Text { summary } if summary.contains("exited")
        )
    });
    assert!(!exited_while_paused);

    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    let exit_deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < exit_deadline {
        manager.pump(session_id, &mut adapter, &mut store).unwrap();
        if store.events_for_session(session_id).iter().any(|event| {
            matches!(
                &event.payload,
                EventPayload::Text { summary } if summary.contains("exited")
            )
        }) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }

    panic!("local process did not exit after semantic resume");
}

fn ingest_json_value_event(
    store: &mut InMemoryStore,
    session_id: SessionId,
    target_id: TargetId,
    next_sequence: &mut u64,
    observed_at: u64,
    value_key: &str,
    summary: &str,
    json: &str,
) -> EventEnvelope {
    let alias = swat_core::ArtifactAlias::new();
    store
        .ingest_emission(
            session_id,
            target_id,
            next_sequence,
            AdapterEmission {
                pending_events: vec![PendingEvent {
                    observed_at: Timestamp::from_millis(observed_at),
                    kind: EventKind::ValueObserved,
                    causality: Default::default(),
                    payload: EventPayload::Value {
                        value_key: value_key.to_string(),
                        summary: summary.to_string(),
                    },
                    artifacts: vec![ArtifactBinding::Pending(alias)],
                }],
                pending_artifacts: vec![PendingArtifact {
                    alias,
                    media_type: "application/json".to_string(),
                    encoding: ArtifactEncoding::Json,
                    access: ArtifactAccess::Inline,
                    bytes: json.as_bytes().to_vec(),
                }],
            },
        )
        .unwrap()
        .pop()
        .unwrap()
}

fn ingest_text_event(
    store: &mut InMemoryStore,
    session_id: SessionId,
    target_id: TargetId,
    next_sequence: &mut u64,
    observed_at: u64,
    kind: EventKind,
    summary: &str,
) -> EventEnvelope {
    store
        .ingest_emission(
            session_id,
            target_id,
            next_sequence,
            AdapterEmission {
                pending_events: vec![PendingEvent {
                    observed_at: Timestamp::from_millis(observed_at),
                    kind,
                    causality: Default::default(),
                    payload: EventPayload::Text {
                        summary: summary.to_string(),
                    },
                    artifacts: Vec::new(),
                }],
                pending_artifacts: Vec::new(),
            },
        )
        .unwrap()
        .pop()
        .unwrap()
}

#[test]
fn parsed_expression_can_trigger_pause() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let expr = parse_expression(
        r#"kind == ModelBoundary and artifact.json $.tool == "search" and summary contains "observed""#,
    )
    .unwrap();
    let mut engine = TriggerEngine::new().with_trigger(
        Trigger::new(
            "pause-on-query-expr",
            TriggerPredicate::Expr(expr),
            vec![TriggerAction::PauseTarget],
        )
        .fire_once(),
    );

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();

    let second = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    assert_eq!(second.trigger_matches.len(), 1);
    assert!(last_control_response(&second).unwrap().accepted);
}

#[test]
fn schema_mismatch_can_trigger_pause() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let mut engine = TriggerEngine::new().with_trigger(
        Trigger::new(
            "pause-on-schema-mismatch",
            TriggerPredicate::ArtifactJsonFailsSchema(SchemaNode::Object(
                [
                    ("decision".to_string(), SchemaNode::String),
                    ("tool".to_string(), SchemaNode::String),
                    ("count".to_string(), SchemaNode::Number),
                ]
                .into_iter()
                .collect(),
            )),
            vec![TriggerAction::PauseTarget],
        )
        .fire_once(),
    );

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();

    let second = pump_with_triggers(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        &mut engine,
    )
    .unwrap();
    assert_eq!(second.trigger_matches.len(), 1);
    assert_eq!(second.control_reports.len(), 1);
    assert!(last_control_response(&second).unwrap().accepted);
}
