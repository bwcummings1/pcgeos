use swat_adapter_mock::MockAdapter;
use swat_core::{ControlAction, SessionId};
use swat_replay::ReplayPlan;
use swat_session::SessionManager;
use swat_store::InMemoryStore;

#[test]
fn rejects_operations_for_unknown_sessions() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();
    let unknown_session = SessionId::from_raw(404);
    let replay_plan = ReplayPlan::default();

    let pump_err = manager
        .pump(unknown_session, &mut adapter, &mut store)
        .unwrap_err();
    assert!(pump_err.to_string().contains("unknown session 404"));

    let control_err = manager
        .control(
            unknown_session,
            &mut adapter,
            ControlAction::Resume,
            &mut store,
        )
        .unwrap_err();
    assert!(control_err.to_string().contains("unknown session 404"));

    let replay_err = manager
        .apply_replay_plan(unknown_session, &mut adapter, &mut store, &replay_plan)
        .unwrap_err();
    assert!(replay_err.to_string().contains("unknown session 404"));
}

#[test]
fn rejects_double_attach_on_the_same_adapter() {
    let mut manager = SessionManager::new();
    let mut store = InMemoryStore::new();
    let mut adapter = MockAdapter::default();

    manager.attach(&mut adapter, &mut store).unwrap();
    let err = manager.attach(&mut adapter, &mut store).unwrap_err();

    assert!(err.to_string().contains("already attached"));
}
