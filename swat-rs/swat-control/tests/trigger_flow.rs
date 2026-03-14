use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_adapter_mock::MockAdapter;
use swat_control::{
    Trigger, TriggerAction, TriggerEngine, TriggerPredicate, last_control_response,
    pump_with_triggers,
};
use swat_core::{ControlAction, EventKind, EventPayload};
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
