use swat_adapter_mock::{MOCK_BOUNDARY_ID, MockAdapter};
use swat_core::{ControlAction, EventKind, EventPayload};
use swat_replay::ReplayPlan;
use swat_session::SessionManager;
use swat_store::InMemoryStore;

fn run_initial_trace(
    store: &mut InMemoryStore,
) -> (
    SessionManager,
    swat_core::SessionId,
    swat_core::ArtifactId,
    swat_core::ArtifactRef,
) {
    let mut session_manager = SessionManager::new();
    let mut adapter = MockAdapter::default();

    let attach_report = session_manager.attach(&mut adapter, store).unwrap();
    assert_eq!(attach_report.stored_events.len(), 1);
    assert_eq!(attach_report.messages.len(), 3);

    let session_id = attach_report.session.session_id;

    let control_report = session_manager
        .control(session_id, &mut adapter, ControlAction::Resume, store)
        .unwrap();
    assert!(control_report.response.accepted);
    assert_eq!(control_report.stored_events.len(), 1);

    let first_pump = session_manager
        .pump(session_id, &mut adapter, store)
        .unwrap();
    assert_eq!(first_pump.stored_events.len(), 1);
    assert_eq!(first_pump.stored_events[0].kind, EventKind::Execution);

    let second_pump = session_manager
        .pump(session_id, &mut adapter, store)
        .unwrap();
    assert_eq!(second_pump.stored_events.len(), 2);

    let boundary_event = second_pump
        .stored_events
        .iter()
        .find(|event| event.kind == EventKind::ModelBoundary)
        .expect("expected model boundary event");

    let EventPayload::Boundary { boundary_id, .. } = &boundary_event.payload else {
        panic!("expected boundary payload");
    };
    assert_eq!(*boundary_id, MOCK_BOUNDARY_ID);
    assert_eq!(boundary_event.artifact_refs.len(), 1);

    let artifact_ref = boundary_event.artifact_refs[0].clone();
    let artifact = store.artifact(artifact_ref.artifact_id).unwrap();
    assert_eq!(
        std::str::from_utf8(&artifact.bytes).unwrap(),
        r#"{"decision":"call-tool","tool":"search"}"#
    );

    (
        session_manager,
        session_id,
        artifact_ref.artifact_id,
        artifact_ref,
    )
}

#[test]
fn attach_resume_and_persist_boundary_artifact() {
    let mut store = InMemoryStore::new();
    let (_session_manager, session_id, artifact_id, artifact_ref) = run_initial_trace(&mut store);

    assert_eq!(store.artifact_count(), 1);
    assert!(store.artifact(artifact_id).is_some());

    let session_events = store.events_for_session(session_id);
    assert!(
        session_events
            .iter()
            .any(|event| event.kind == EventKind::Lifecycle)
    );
    assert!(
        session_events
            .iter()
            .any(|event| event.kind == EventKind::ModelBoundary)
    );
    assert_eq!(artifact_ref.artifact_id, artifact_id);
}

#[test]
fn replay_plan_reuses_recorded_artifacts_without_duplication() {
    let mut store = InMemoryStore::new();
    let (mut session_manager, _first_session_id, artifact_id, artifact_ref) =
        run_initial_trace(&mut store);

    let recorded_events = store.events();
    let replay_plan = ReplayPlan::from_events(&recorded_events);
    assert_eq!(replay_plan.len(), 1);

    let mut replay_adapter = MockAdapter::new("mock-replay-target");
    let replay_attach = session_manager
        .attach(&mut replay_adapter, &mut store)
        .unwrap();
    let replay_session_id = replay_attach.session.session_id;

    let before_replay_artifact_count = store.artifact_count();
    let replay_apply = session_manager
        .apply_replay_plan(
            replay_session_id,
            &mut replay_adapter,
            &mut store,
            &replay_plan,
        )
        .unwrap();
    assert_eq!(replay_apply.directives_applied, 1);

    session_manager
        .control(
            replay_session_id,
            &mut replay_adapter,
            ControlAction::Resume,
            &mut store,
        )
        .unwrap();
    session_manager
        .pump(replay_session_id, &mut replay_adapter, &mut store)
        .unwrap();
    let replay_boundary_pump = session_manager
        .pump(replay_session_id, &mut replay_adapter, &mut store)
        .unwrap();

    assert_eq!(store.artifact_count(), before_replay_artifact_count);

    let replayed_boundary = replay_boundary_pump
        .stored_events
        .iter()
        .find(|event| event.kind == EventKind::ModelBoundary)
        .expect("expected replayed model boundary");
    assert_eq!(replayed_boundary.artifact_refs.len(), 1);
    assert_eq!(replayed_boundary.artifact_refs[0].artifact_id, artifact_id);
    assert_eq!(replayed_boundary.artifact_refs[0], artifact_ref);

    let replay_texts = replay_boundary_pump
        .stored_events
        .iter()
        .filter_map(|event| match &event.payload {
            EventPayload::Text { summary } => Some(summary.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        replay_texts
            .iter()
            .any(|summary| summary.contains("replay"))
    );
}

#[test]
fn accepted_snapshot_controls_create_snapshot_inventory_and_events() {
    let mut store = InMemoryStore::new();
    let (mut session_manager, session_id, _artifact_id, _artifact_ref) = run_initial_trace(&mut store);
    let mut adapter = MockAdapter::new("mock-snapshot-target");

    let reattach = session_manager.attach(&mut adapter, &mut store).unwrap();
    let live_session_id = reattach.session.session_id;
    session_manager
        .control(live_session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    session_manager.pump(live_session_id, &mut adapter, &mut store).unwrap();
    session_manager.pump(live_session_id, &mut adapter, &mut store).unwrap();

    let snapshot = session_manager
        .control(
            live_session_id,
            &mut adapter,
            ControlAction::CreateSnapshot {
                reason: "checkpoint after search".to_string(),
            },
            &mut store,
        )
        .unwrap();
    assert!(snapshot.response.accepted);
    assert!(snapshot.snapshot.is_some());
    assert!(snapshot.stored_events.iter().any(|event| event.kind == EventKind::Snapshot));

    let snapshot_record = snapshot.snapshot.unwrap();
    assert_eq!(snapshot_record.reason, "checkpoint after search");
    assert!(store.snapshot(snapshot_record.snapshot_id).is_some());
    assert_eq!(store.snapshots_for_session(live_session_id).len(), 1);
    assert_eq!(store.snapshots_for_session(session_id).len(), 0);

    let replay_plan = ReplayPlan::from_events_up_to(
        &store.events_for_session(live_session_id),
        snapshot_record.captured_sequence_no,
    );
    assert_eq!(replay_plan.len(), 1);
}
