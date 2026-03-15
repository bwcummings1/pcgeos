use std::path::PathBuf;

use swat_adapter_pcgeos::PcGeosAdapter;
use swat_core::{BoundaryId, ControlAction, EventKind, EventPayload};
use swat_replay::ReplayPlan;
use swat_session::SessionManager;
use swat_store::InMemoryStore;
use swat_value::decode_event_artifacts;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("geopoint-session.json")
}

#[test]
fn fixture_adapter_attaches_with_inventory_and_initial_stop() {
    let mut adapter = PcGeosAdapter::from_fixture_path(fixture_path()).unwrap();
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let attach = manager.attach(&mut adapter, &mut store).unwrap();
    let session_id = attach.session.session_id;

    assert_eq!(
        attach.session.descriptor.adapter_name,
        "swat-adapter-pcgeos"
    );
    assert_eq!(attach.session.descriptor.runtime, "pcgeos-fixture");
    assert!(attach.session.capabilities.can_inject_replay);
    assert!(attach.session.capabilities.can_resolve_source);

    let events = store.events_for_session(session_id);
    assert!(events.iter().any(|event| {
        matches!(
            &event.payload,
            EventPayload::Text { summary } if summary.contains("pc/geos fixture attached")
        )
    }));
    assert!(
        events
            .iter()
            .any(|event| event.kind == EventKind::SourceResolution)
    );

    let inventory_event = events
        .iter()
        .find(|event| {
            matches!(
                &event.payload,
                EventPayload::Value { value_key, .. } if value_key == "pcgeos.inventory"
            )
        })
        .unwrap();
    let decoded = decode_event_artifacts(&store, inventory_event).unwrap();
    let typed = decoded[0].typed_entities();
    assert!(
        typed
            .patients
            .iter()
            .any(|patient| patient.key == "geopoint")
    );
    assert!(
        typed
            .handles
            .iter()
            .any(|handle| handle.key == "geopoint.app:handle:Interface")
    );
    assert!(
        typed
            .resources
            .iter()
            .any(|resource| resource.key == "geopoint.app:Interface")
    );
    assert!(
        typed
            .objects
            .iter()
            .any(|object| object.key == "GeoPointApp")
    );

    let stop_event = events
        .iter()
        .find(|event| {
            matches!(
                &event.payload,
                EventPayload::Boundary { boundary_id, .. }
                    if *boundary_id == BoundaryId::from_raw(1002)
            )
        })
        .unwrap();
    let stop_artifacts = decode_event_artifacts(&store, stop_event).unwrap();
    assert_eq!(
        stop_artifacts[0].query_json_path("$.function").unwrap(),
        Some(swat_value::QueriedValue::String(
            "GeoPointApp::OpenDocument".to_string()
        ))
    );
    assert_eq!(
        stop_artifacts[0].query_json_path("$.file").unwrap(),
        Some(swat_value::QueriedValue::String(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("Appl/GeoPoint/show.goc")
                .canonicalize()
                .unwrap()
                .display()
                .to_string()
        ))
    );
}

#[test]
fn fixture_adapter_replays_recorded_boundary_artifacts() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();

    let mut first_adapter = PcGeosAdapter::from_fixture_path(fixture_path()).unwrap();
    let first_attach = manager.attach(&mut first_adapter, &mut store).unwrap();
    let first_session_id = first_attach.session.session_id;
    manager
        .control(
            first_session_id,
            &mut first_adapter,
            ControlAction::Resume,
            &mut store,
        )
        .unwrap();
    let first_pump = manager
        .pump(first_session_id, &mut first_adapter, &mut store)
        .unwrap();
    let recorded_boundary = first_pump
        .stored_events
        .iter()
        .find(|event| {
            matches!(
                &event.payload,
                EventPayload::Boundary { boundary_id, .. }
                    if *boundary_id == BoundaryId::from_raw(1012)
            )
        })
        .unwrap();
    let recorded_artifact_ref = recorded_boundary.artifact_refs[0].clone();

    let replay_plan = ReplayPlan::for_boundary(
        &store.events_for_session(first_session_id),
        BoundaryId::from_raw(1012),
    );
    assert_eq!(replay_plan.len(), 1);

    let mut replay_adapter = PcGeosAdapter::from_fixture_path(fixture_path()).unwrap();
    let replay_attach = manager.attach(&mut replay_adapter, &mut store).unwrap();
    let replay_session_id = replay_attach.session.session_id;
    let artifact_count_before_replay = store.artifact_count();
    manager
        .apply_replay_plan(
            replay_session_id,
            &mut replay_adapter,
            &mut store,
            &replay_plan,
        )
        .unwrap();
    manager
        .control(
            replay_session_id,
            &mut replay_adapter,
            ControlAction::Resume,
            &mut store,
        )
        .unwrap();
    let replay_pump = manager
        .pump(replay_session_id, &mut replay_adapter, &mut store)
        .unwrap();

    let replayed_boundary = replay_pump
        .stored_events
        .iter()
        .find(|event| {
            matches!(
                &event.payload,
                EventPayload::Boundary { boundary_id, .. }
                    if *boundary_id == BoundaryId::from_raw(1012)
            )
        })
        .unwrap();
    assert_eq!(replayed_boundary.artifact_refs[0], recorded_artifact_ref);
    assert_eq!(store.artifact_count(), artifact_count_before_replay + 3);
    assert!(replay_pump.stored_events.iter().any(|event| {
        matches!(
            &event.payload,
            EventPayload::Text { summary }
                if summary.contains("reused replay artifact")
        )
    }));
}
