#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use rhai::{Dynamic, Engine, EvalAltResult, Scope};
use swat_api::{LiveSessionApi, TraceInspector};
use swat_control::{Trigger, TriggerAction, TriggerEngine, TriggerPredicate};
use swat_core::{
    ArtifactId, ControlAction, EventEnvelope, SessionId, SnapshotId, SnapshotRecord, SwatError,
    SwatResult, TargetAdapter, TriggerId,
};
use swat_expr::parse_expression;
use swat_session::SessionManager;
use swat_store::{StoredArtifact, SwatStore};

#[derive(Clone, Default)]
struct TraceSnapshotStore {
    events: Vec<EventEnvelope>,
    artifacts: BTreeMap<ArtifactId, StoredArtifact>,
    snapshots: BTreeMap<SnapshotId, SnapshotRecord>,
}

impl TraceSnapshotStore {
    fn from_store<S: SwatStore + ?Sized>(store: &S, session_id: SessionId) -> Self {
        let events = store.events_for_session(session_id);
        let mut artifacts = BTreeMap::new();
        for event in &events {
            for artifact_ref in &event.artifact_refs {
                if let Some(artifact) = store.artifact(artifact_ref.artifact_id) {
                    artifacts.insert(artifact_ref.artifact_id, artifact);
                }
            }
        }
        let snapshots = store
            .snapshots_for_session(session_id)
            .into_iter()
            .map(|snapshot| (snapshot.snapshot_id, snapshot))
            .collect();
        Self {
            events,
            artifacts,
            snapshots,
        }
    }
}

impl SwatStore for TraceSnapshotStore {
    fn ingest_emission(
        &mut self,
        _session_id: SessionId,
        _target_id: swat_core::TargetId,
        _next_sequence: &mut u64,
        _emission: swat_core::AdapterEmission,
    ) -> SwatResult<Vec<EventEnvelope>> {
        Err(SwatError::new(
            "trace snapshot store is read-only and cannot ingest emissions",
        ))
    }

    fn events(&self) -> Vec<EventEnvelope> {
        self.events.clone()
    }

    fn events_for_session(&self, session_id: SessionId) -> Vec<EventEnvelope> {
        self.events
            .iter()
            .filter(|event| event.session_id == session_id)
            .cloned()
            .collect()
    }

    fn artifact(&self, artifact_id: ArtifactId) -> Option<StoredArtifact> {
        self.artifacts.get(&artifact_id).cloned()
    }

    fn artifact_count(&self) -> usize {
        self.artifacts.len()
    }

    fn record_snapshot(&mut self, _snapshot: SnapshotRecord) -> SwatResult<()> {
        Err(SwatError::new(
            "trace snapshot store is read-only and cannot persist snapshots",
        ))
    }

    fn snapshots(&self) -> Vec<SnapshotRecord> {
        self.snapshots.values().cloned().collect()
    }

    fn snapshots_for_session(&self, session_id: SessionId) -> Vec<SnapshotRecord> {
        self.snapshots
            .values()
            .filter(|snapshot| snapshot.session_id == session_id)
            .cloned()
            .collect()
    }

    fn snapshot(&self, snapshot_id: SnapshotId) -> Option<SnapshotRecord> {
        self.snapshots.get(&snapshot_id).cloned()
    }
}

#[derive(Clone)]
pub struct ScriptContext {
    session_id: SessionId,
    store: TraceSnapshotStore,
}

impl ScriptContext {
    pub fn from_store<S: SwatStore + ?Sized>(store: &S, session_id: SessionId) -> Self {
        Self {
            session_id,
            store: TraceSnapshotStore::from_store(store, session_id),
        }
    }

    fn inspector(&self) -> TraceInspector<'_, TraceSnapshotStore> {
        TraceInspector::new(&self.store)
    }

    pub fn event_count(&mut self) -> i64 {
        self.inspector().session_events(self.session_id).len() as i64
    }

    pub fn summary_search_count(&mut self, needle: &str) -> i64 {
        self.inspector()
            .search_summaries(self.session_id, needle)
            .len() as i64
    }

    pub fn artifact_search_count(&mut self, needle: &str) -> i64 {
        self.inspector()
            .search_artifact_text(self.session_id, needle)
            .map(|matches| matches.len() as i64)
            .unwrap_or(0)
    }

    pub fn query_count(&mut self, expr: &str) -> i64 {
        self.inspector()
            .query_events_str(self.session_id, expr)
            .map(|events| events.len() as i64)
            .unwrap_or(0)
    }

    pub fn first_summary(&mut self, expr: &str) -> String {
        self.inspector()
            .query_events_str(self.session_id, expr)
            .ok()
            .and_then(|events| events.into_iter().next())
            .and_then(|event| match event.payload {
                swat_core::EventPayload::Text { summary }
                | swat_core::EventPayload::Control { summary, .. }
                | swat_core::EventPayload::Boundary { summary, .. }
                | swat_core::EventPayload::Snapshot { summary, .. }
                | swat_core::EventPayload::Trigger { summary, .. }
                | swat_core::EventPayload::Value { summary, .. }
                | swat_core::EventPayload::Policy { summary, .. } => Some(summary),
                swat_core::EventPayload::Empty => None,
            })
            .unwrap_or_default()
    }

    pub fn source_contains(&mut self, expr: &str, needle: &str, before: i64, after: i64) -> bool {
        self.inspector()
            .query_events_str(self.session_id, expr)
            .ok()
            .and_then(|events| events.into_iter().next())
            .and_then(|event| {
                self.inspector()
                    .resolve_source(&event, before.max(0) as usize, after.max(0) as usize)
                    .ok()
                    .flatten()
            })
            .map(|snippet| snippet.lines.iter().any(|line| line.text.contains(needle)))
            .unwrap_or(false)
    }
}

pub struct ScriptHost {
    engine: Engine,
    scope: Scope<'static>,
}

impl ScriptHost {
    pub fn new<S: SwatStore + ?Sized>(store: &S, session_id: SessionId) -> Self {
        let mut engine = Engine::new();
        engine.register_type::<ScriptContext>();
        engine.register_fn("event_count", ScriptContext::event_count);
        engine.register_fn("summary_search_count", ScriptContext::summary_search_count);
        engine.register_fn(
            "artifact_search_count",
            ScriptContext::artifact_search_count,
        );
        engine.register_fn("query_count", ScriptContext::query_count);
        engine.register_fn("first_summary", ScriptContext::first_summary);
        engine.register_fn("source_contains", ScriptContext::source_contains);

        let mut scope = Scope::new();
        scope.push("ctx", ScriptContext::from_store(store, session_id));

        Self { engine, scope }
    }

    pub fn eval_dynamic(&mut self, script: &str) -> SwatResult<Dynamic> {
        self.engine
            .eval_with_scope::<Dynamic>(&mut self.scope, script)
            .map_err(script_error)
    }

    pub fn eval_i64(&mut self, script: &str) -> SwatResult<i64> {
        self.engine
            .eval_with_scope::<i64>(&mut self.scope, script)
            .map_err(script_error)
    }

    pub fn eval_bool(&mut self, script: &str) -> SwatResult<bool> {
        self.engine
            .eval_with_scope::<bool>(&mut self.scope, script)
            .map_err(script_error)
    }

    pub fn eval_string(&mut self, script: &str) -> SwatResult<String> {
        self.engine
            .eval_with_scope::<String>(&mut self.scope, script)
            .map_err(script_error)
    }
}

fn script_error(err: Box<EvalAltResult>) -> SwatError {
    SwatError::new(format!("script evaluation failed: {err}"))
}

pub struct LiveScriptSession<'a, A: TargetAdapter + ?Sized, S: SwatStore + ?Sized> {
    session_id: SessionId,
    manager: &'a mut SessionManager,
    adapter: &'a mut A,
    store: &'a mut S,
    trigger_engine: &'a mut TriggerEngine,
}

impl<'a, A: TargetAdapter + ?Sized, S: SwatStore + ?Sized> LiveScriptSession<'a, A, S> {
    pub fn new(
        session_id: SessionId,
        manager: &'a mut SessionManager,
        adapter: &'a mut A,
        store: &'a mut S,
        trigger_engine: &'a mut TriggerEngine,
    ) -> Self {
        Self {
            session_id,
            manager,
            adapter,
            store,
            trigger_engine,
        }
    }

    pub fn event_count(&mut self) -> i64 {
        let session_id = self.session_id;
        self.api().inspector().session_events(session_id).len() as i64
    }

    pub fn query_count(&mut self, expr: &str) -> SwatResult<i64> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .inspector()
            .query_events_str(session_id, expr)?
            .len() as i64)
    }

    pub fn trigger_count(&mut self) -> i64 {
        self.api().triggers().len() as i64
    }

    pub fn pump_once(&mut self) -> SwatResult<usize> {
        Ok(self
            .manager
            .pump(self.session_id, self.adapter, self.store)?
            .stored_events
            .len())
    }

    pub fn resume(&mut self) -> SwatResult<String> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .control(session_id, ControlAction::Resume)?
            .value
            .response
            .summary)
    }

    pub fn pause(&mut self) -> SwatResult<String> {
        let session_id = self.session_id;
        Ok(self
            .api()
            .control(session_id, ControlAction::Pause)?
            .value
            .response
            .summary)
    }

    pub fn snapshot(&mut self, reason: &str) -> SwatResult<SnapshotId> {
        let session_id = self.session_id;
        self.api()
            .control(
                session_id,
                ControlAction::CreateSnapshot {
                    reason: reason.to_string(),
                },
            )?
            .value
            .snapshot
            .map(|snapshot| snapshot.snapshot_id)
            .ok_or_else(|| SwatError::new("snapshot request did not create a snapshot"))
    }

    pub fn add_trigger_expr(
        &mut self,
        name: &str,
        expr: &str,
        fire_once: bool,
    ) -> SwatResult<TriggerId> {
        let session_id = self.session_id;
        let parsed = parse_expression(expr)?;
        let mut trigger = Trigger::new(name, TriggerPredicate::Expr(parsed), vec![TriggerAction::PauseTarget]);
        if fire_once {
            trigger = trigger.fire_once();
        }
        Ok(self.api().add_trigger(session_id, trigger)?.value)
    }

    pub fn enable_trigger(&mut self, trigger_id: TriggerId) -> SwatResult<bool> {
        let session_id = self.session_id;
        self.api()
            .set_trigger_enabled(session_id, trigger_id, true)
            .map(|report| report.value)
    }

    pub fn disable_trigger(&mut self, trigger_id: TriggerId) -> SwatResult<bool> {
        let session_id = self.session_id;
        self.api()
            .set_trigger_enabled(session_id, trigger_id, false)
            .map(|report| report.value)
    }

    pub fn remove_trigger(&mut self, trigger_id: TriggerId) -> SwatResult<String> {
        let session_id = self.session_id;
        self.api()
            .remove_trigger(session_id, trigger_id)
            .map(|report| report.value.name)
    }

    fn api(&mut self) -> LiveSessionApi<'_, A, S> {
        LiveSessionApi::new(
            self.manager,
            self.adapter,
            self.store,
            self.trigger_engine,
        )
    }
}
