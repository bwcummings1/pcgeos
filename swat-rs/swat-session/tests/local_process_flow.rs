use std::thread;
use std::time::{Duration, Instant};

use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_core::{
    ArtifactAccess, ArtifactEncoding, ArtifactId, ArtifactRef, BoundaryId, ControlAction,
    DeterminismClass, EventEnvelope, EventId, EventKind, EventPayload, ReplayMode, SessionId,
    Timestamp,
};
use swat_replay::ReplayPlan;
use swat_session::SessionManager;
use swat_store::InMemoryStore;

fn pump_until(
    manager: &mut SessionManager,
    session_id: SessionId,
    adapter: &mut LocalProcessAdapter,
    store: &mut InMemoryStore,
    timeout: Duration,
    predicate: impl Fn(&[EventEnvelope], &InMemoryStore) -> bool,
) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        manager.pump(session_id, adapter, store).unwrap();
        let session_events = store.events_for_session(session_id);
        if predicate(&session_events, store) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }

    panic!("condition was not met before timeout");
}

#[test]
fn captures_stdout_and_stderr_as_artifacts() {
    let spec = LocalProcessSpec::new("/bin/sh")
        .with_args(["-c", "printf 'alpha'; printf 'beta' 1>&2"])
        .with_env("SWAT_RS_TEST", "capture");
    let mut adapter = LocalProcessAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;
    assert!(!attach.session.capabilities.can_inject_replay);

    pump_until(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        Duration::from_secs(2),
        |events, store| {
            let saw_exit = events.iter().any(|event| {
                matches!(
                    &event.payload,
                    EventPayload::Text { summary } if summary.contains("exited")
                )
            });
            saw_exit && store.artifact_count() >= 2
        },
    );

    let mut payloads = Vec::new();
    for event in store.events_for_session(session_id) {
        for artifact_ref in event.artifact_refs {
            let bytes = &store.artifact(artifact_ref.artifact_id).unwrap().bytes;
            payloads.push(String::from_utf8_lossy(bytes).to_string());
        }
    }

    assert!(payloads.iter().any(|payload| payload.contains("alpha")));
    assert!(payloads.iter().any(|payload| payload.contains("beta")));
}

#[test]
fn pause_blocks_exit_until_resume() {
    let spec = LocalProcessSpec::new("/bin/sleep").with_args(["2"]);
    let mut adapter = LocalProcessAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    let pause = manager
        .control(session_id, &mut adapter, ControlAction::Pause, &mut store)
        .unwrap();
    assert!(pause.response.accepted);

    let wait_deadline = Instant::now() + Duration::from_millis(1200);
    while Instant::now() < wait_deadline {
        manager.pump(session_id, &mut adapter, &mut store).unwrap();
        thread::sleep(Duration::from_millis(25));
    }

    let pre_resume_exit = store
        .events_for_session(session_id)
        .into_iter()
        .any(|event| {
            matches!(
                &event.payload,
                EventPayload::Text { summary } if summary.contains("exited")
            )
        });
    assert!(!pre_resume_exit);

    let resume = manager
        .control(session_id, &mut adapter, ControlAction::Resume, &mut store)
        .unwrap();
    assert!(resume.response.accepted);

    pump_until(
        &mut manager,
        session_id,
        &mut adapter,
        &mut store,
        Duration::from_secs(3),
        |events, _| {
            events.iter().any(|event| {
                matches!(
                    &event.payload,
                    EventPayload::Text { summary } if summary.contains("exited")
                )
            })
        },
    );
}

#[test]
fn rejects_replay_plans_for_targets_without_replay_capability() {
    let spec = LocalProcessSpec::new("/bin/sh").with_args(["-c", "printf 'noop'"]);
    let mut adapter = LocalProcessAdapter::new(spec);
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let attach = manager.attach(&mut adapter, &mut store).unwrap();

    let boundary_event = EventEnvelope {
        event_id: EventId::from_raw(1),
        session_id: attach.session.session_id,
        target_id: attach.session.target_id,
        sequence_no: 1,
        observed_at: Timestamp::from_millis(1),
        kind: EventKind::ModelBoundary,
        causality: Default::default(),
        payload: EventPayload::Boundary {
            boundary_id: BoundaryId::from_raw(9),
            determinism: DeterminismClass::ExternalBoundary,
            summary: "synthetic boundary".to_string(),
        },
        artifact_refs: vec![ArtifactRef {
            artifact_id: ArtifactId::from_raw(10),
            media_type: "application/json".to_string(),
            encoding: ArtifactEncoding::Json,
            size_hint: Some(4),
            access: ArtifactAccess::Lazy,
        }],
    };
    let replay_plan = ReplayPlan::from_events(&[boundary_event]);
    assert_eq!(attach.session.descriptor.replay_mode, ReplayMode::Live);

    let err = manager
        .apply_replay_plan(
            attach.session.session_id,
            &mut adapter,
            &mut store,
            &replay_plan,
        )
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("does not support replay injection")
    );
}
