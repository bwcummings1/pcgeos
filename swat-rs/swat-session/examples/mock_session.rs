use swat_adapter_mock::MockAdapter;
use swat_core::ControlAction;
use swat_protocol::LengthPrefixedJsonCodec;
use swat_replay::ReplayPlan;
use swat_session::SessionManager;
use swat_store::InMemoryStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = InMemoryStore::new();
    let mut session_manager = SessionManager::new();
    let mut adapter = MockAdapter::new("phase-1-demo");
    let codec = LengthPrefixedJsonCodec;

    let attach = session_manager.attach(&mut adapter, &mut store)?;
    println!(
        "attached session {} to target {}",
        attach.session.session_id.raw(),
        attach.session.target_id.raw()
    );

    let hello_frame = codec.encode(&attach.messages[0])?;
    let decoded = codec.decode(&hello_frame)?;
    println!(
        "encoded {} bytes for protocol message {}",
        hello_frame.len(),
        decoded.name()
    );

    session_manager.control(
        attach.session.session_id,
        &mut adapter,
        ControlAction::Resume,
        &mut store,
    )?;

    let first_tick = session_manager.pump(attach.session.session_id, &mut adapter, &mut store)?;
    println!(
        "first tick emitted {} event(s)",
        first_tick.stored_events.len()
    );

    let boundary_tick =
        session_manager.pump(attach.session.session_id, &mut adapter, &mut store)?;
    println!(
        "second tick emitted {} event(s) and artifact_count={}",
        boundary_tick.stored_events.len(),
        store.artifact_count()
    );

    let recorded_events = store.events();
    let replay_plan = ReplayPlan::from_events(&recorded_events);
    println!("replay plan contains {} directive(s)", replay_plan.len());

    let mut replay_adapter = MockAdapter::new("phase-1-demo-replay");
    let replay_attach = session_manager.attach(&mut replay_adapter, &mut store)?;
    session_manager.apply_replay_plan(
        replay_attach.session.session_id,
        &mut replay_adapter,
        &mut store,
        &replay_plan,
    )?;
    session_manager.control(
        replay_attach.session.session_id,
        &mut replay_adapter,
        ControlAction::Resume,
        &mut store,
    )?;
    session_manager.pump(
        replay_attach.session.session_id,
        &mut replay_adapter,
        &mut store,
    )?;
    let replay_boundary = session_manager.pump(
        replay_attach.session.session_id,
        &mut replay_adapter,
        &mut store,
    )?;

    println!(
        "replay target emitted {} boundary tick event(s) with artifact_count={}",
        replay_boundary.stored_events.len(),
        store.artifact_count()
    );

    Ok(())
}
