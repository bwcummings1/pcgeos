#![forbid(unsafe_code)]

use swat_control::{Trigger, TriggerEngine};
use swat_core::{
    BoundaryId, ControlAction, EventEnvelope, EventId, EventKind, EventPayload, PendingEvent,
    PolicyVerdict, SessionId, SnapshotId, SnapshotRecord, SwatError, SwatResult, TargetAdapter,
    TriggerId,
};
use swat_expr::{QueryExpr, evaluate_expression, parse_expression};
use swat_replay::ReplayPlan;
use swat_resolver::{
    CorrelationGroup, EntityRef, EntityRelation, ResolvedEntity, TraceIndex, TraceResolver,
};
use swat_session::{ControlReport, ReplayApplyReport, SessionManager};
use swat_source::{SourceInspection, SourceSnippet, inspect_event_source, resolve_event_source};
use swat_store::SwatStore;
use swat_value::{DecodedValue, ValuePresentation, decode_event_artifacts};

#[derive(Clone, Debug, PartialEq)]
pub struct TraceMatch {
    pub event: EventEnvelope,
    pub matched_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotInspection {
    pub snapshot: SnapshotRecord,
    pub snapshot_event: Option<EventEnvelope>,
    pub captured_event_count: usize,
    pub replay_directive_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutationReport<T> {
    pub value: T,
    pub policy_events: Vec<EventEnvelope>,
}

pub struct TraceInspector<'a, S: SwatStore + ?Sized> {
    store: &'a S,
}

impl<'a, S: SwatStore + ?Sized> TraceInspector<'a, S> {
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    pub fn session_events(&self, session_id: SessionId) -> Vec<EventEnvelope> {
        self.store.events_for_session(session_id)
    }

    pub fn event_by_id(&self, event_id: EventId) -> Option<EventEnvelope> {
        self.store
            .events()
            .into_iter()
            .find(|event| event.event_id == event_id)
    }

    pub fn events_by_kind(&self, session_id: SessionId, kind: EventKind) -> Vec<EventEnvelope> {
        self.session_events(session_id)
            .into_iter()
            .filter(|event| event.kind == kind)
            .collect()
    }

    pub fn decoded_artifacts(&self, event: &EventEnvelope) -> SwatResult<Vec<DecodedValue>> {
        decode_event_artifacts(self.store, event)
    }

    pub fn artifact_presentations(
        &self,
        event: &EventEnvelope,
        preview_limit: usize,
    ) -> SwatResult<Vec<ValuePresentation>> {
        Ok(self
            .decoded_artifacts(event)?
            .into_iter()
            .map(|value| value.presentation(preview_limit))
            .collect())
    }

    pub fn search_summaries(&self, session_id: SessionId, needle: &str) -> Vec<TraceMatch> {
        self.session_events(session_id)
            .into_iter()
            .filter_map(|event| {
                let matched_text = payload_summary(&event)
                    .filter(|summary| summary.contains(needle))
                    .map(ToString::to_string);
                matched_text.map(|matched_text| TraceMatch {
                    event,
                    matched_text,
                })
            })
            .collect()
    }

    pub fn search_artifact_text(
        &self,
        session_id: SessionId,
        needle: &str,
    ) -> SwatResult<Vec<TraceMatch>> {
        let mut matches = Vec::new();
        for event in self.session_events(session_id) {
            for decoded in self.decoded_artifacts(&event)? {
                let preview = decoded.preview(512);
                if preview.contains(needle) {
                    matches.push(TraceMatch {
                        event: event.clone(),
                        matched_text: preview,
                    });
                    break;
                }
            }
        }
        Ok(matches)
    }

    pub fn query_events(&self, session_id: SessionId, expr: &QueryExpr) -> Vec<EventEnvelope> {
        self.session_events(session_id)
            .into_iter()
            .filter(|event| evaluate_expression(self.store, event, expr))
            .collect()
    }

    pub fn query_events_str(
        &self,
        session_id: SessionId,
        expr: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        let parsed = parse_expression(expr)?;
        Ok(self.query_events(session_id, &parsed))
    }

    pub fn resolve_source(
        &self,
        event: &EventEnvelope,
        before: usize,
        after: usize,
    ) -> SwatResult<Option<SourceSnippet>> {
        resolve_event_source(self.store, event, before, after)
    }

    pub fn source_inspection(
        &self,
        event: &EventEnvelope,
        before: usize,
        after: usize,
    ) -> SwatResult<SourceInspection> {
        inspect_event_source(self.store, event, before, after)
    }

    pub fn resolve_event_entities(&self, event: &EventEnvelope) -> SwatResult<Vec<EntityRef>> {
        TraceResolver::new(self.store).event_entities(event)
    }

    pub fn entity_index(&self, session_id: SessionId) -> SwatResult<TraceIndex> {
        TraceResolver::new(self.store).index_session(session_id)
    }

    pub fn find_entities(
        &self,
        session_id: SessionId,
        needle: &str,
    ) -> SwatResult<Vec<ResolvedEntity>> {
        TraceResolver::new(self.store).find_entities(session_id, needle)
    }

    pub fn events_for_correlation(
        &self,
        session_id: SessionId,
        correlation_id: &str,
    ) -> Vec<EventEnvelope> {
        TraceResolver::new(self.store).events_for_correlation(session_id, correlation_id)
    }

    pub fn boundary_span(
        &self,
        session_id: SessionId,
        boundary_id: BoundaryId,
    ) -> Vec<EventEnvelope> {
        TraceResolver::new(self.store).boundary_span(session_id, boundary_id)
    }

    pub fn entity_relations(&self, session_id: SessionId) -> SwatResult<Vec<EntityRelation>> {
        TraceResolver::new(self.store).entity_relations(session_id)
    }

    pub fn related_entities(
        &self,
        session_id: SessionId,
        entity: &EntityRef,
    ) -> SwatResult<Vec<EntityRelation>> {
        TraceResolver::new(self.store).related_entities(session_id, entity)
    }

    pub fn correlation_groups(&self, session_id: SessionId) -> SwatResult<Vec<CorrelationGroup>> {
        TraceResolver::new(self.store).correlation_groups(session_id)
    }

    pub fn events_for_span(
        &self,
        session_id: SessionId,
        span_id: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        TraceResolver::new(self.store).events_for_span(session_id, span_id)
    }

    pub fn events_for_value_key(
        &self,
        session_id: SessionId,
        value_key: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        TraceResolver::new(self.store).events_for_value_key(session_id, value_key)
    }

    pub fn events_for_source_file(
        &self,
        session_id: SessionId,
        file: &str,
    ) -> SwatResult<Vec<EventEnvelope>> {
        TraceResolver::new(self.store).events_for_source_file(session_id, file)
    }

    pub fn session_snapshots(&self, session_id: SessionId) -> Vec<SnapshotRecord> {
        self.store.snapshots_for_session(session_id)
    }

    pub fn snapshot_by_id(&self, snapshot_id: SnapshotId) -> Option<SnapshotRecord> {
        self.store.snapshot(snapshot_id)
    }

    pub fn snapshot_inspection(&self, snapshot_id: SnapshotId) -> Option<SnapshotInspection> {
        let snapshot = self.snapshot_by_id(snapshot_id)?;
        let events = self.session_events(snapshot.session_id);
        let snapshot_event = events
            .iter()
            .find(|event| event.event_id == snapshot.snapshot_event_id)
            .cloned();
        let captured_event_count = events
            .iter()
            .filter(|event| event.sequence_no <= snapshot.captured_sequence_no)
            .count();
        let replay_plan = ReplayPlan::from_events_up_to(&events, snapshot.captured_sequence_no);

        Some(SnapshotInspection {
            snapshot,
            snapshot_event,
            captured_event_count,
            replay_directive_count: replay_plan.len(),
        })
    }

    pub fn replay_plan_for_snapshot(&self, snapshot_id: SnapshotId) -> Option<ReplayPlan> {
        let snapshot = self.snapshot_by_id(snapshot_id)?;
        Some(ReplayPlan::from_events_up_to(
            &self.session_events(snapshot.session_id),
            snapshot.captured_sequence_no,
        ))
    }

    pub fn replay_plan_for_boundary(
        &self,
        session_id: SessionId,
        boundary_id: BoundaryId,
    ) -> ReplayPlan {
        ReplayPlan::for_boundary(&self.session_events(session_id), boundary_id)
    }
}

pub struct LiveSessionApi<'a, A: TargetAdapter + ?Sized, S: SwatStore + ?Sized> {
    manager: &'a mut SessionManager,
    adapter: &'a mut A,
    store: &'a mut S,
    trigger_engine: &'a mut TriggerEngine,
}

impl<'a, A: TargetAdapter + ?Sized, S: SwatStore + ?Sized> LiveSessionApi<'a, A, S> {
    pub fn new(
        manager: &'a mut SessionManager,
        adapter: &'a mut A,
        store: &'a mut S,
        trigger_engine: &'a mut TriggerEngine,
    ) -> Self {
        Self {
            manager,
            adapter,
            store,
            trigger_engine,
        }
    }

    pub fn inspector(&self) -> TraceInspector<'_, S> {
        TraceInspector::new(self.store)
    }

    pub fn triggers(&self) -> &[Trigger] {
        self.trigger_engine.triggers()
    }

    pub fn control(
        &mut self,
        session_id: SessionId,
        action: ControlAction,
    ) -> SwatResult<MutationReport<ControlReport>> {
        let session = self
            .manager
            .session(session_id)
            .ok_or_else(|| SwatError::new(format!("unknown session {}", session_id.raw())))?;
        if !control_allowed(session.capabilities, &action) {
            let summary = format!(
                "policy denied control {:?} for target {}",
                action,
                session.target_id.raw()
            );
            let _ = self.record_policy_event(session_id, PolicyVerdict::Deny, summary.clone());
            return Err(SwatError::new(summary));
        }

        let policy_events = self.record_policy_event(
            session_id,
            PolicyVerdict::Allow,
            format!(
                "policy allowed control {:?} for target {}",
                action,
                session.target_id.raw()
            ),
        )?;
        let value = self
            .manager
            .control(session_id, self.adapter, action, self.store)?;
        Ok(MutationReport {
            value,
            policy_events,
        })
    }

    pub fn apply_replay_plan(
        &mut self,
        session_id: SessionId,
        plan: &ReplayPlan,
    ) -> SwatResult<MutationReport<ReplayApplyReport>> {
        let session = self
            .manager
            .session(session_id)
            .ok_or_else(|| SwatError::new(format!("unknown session {}", session_id.raw())))?;
        if !session.capabilities.can_inject_replay {
            let summary = format!(
                "policy denied replay injection for target {}",
                session.target_id.raw()
            );
            let _ = self.record_policy_event(session_id, PolicyVerdict::Deny, summary.clone());
            return Err(SwatError::new(summary));
        }

        let policy_events = self.record_policy_event(
            session_id,
            PolicyVerdict::Allow,
            format!(
                "policy allowed replay injection for target {} with {} directive(s)",
                session.target_id.raw(),
                plan.len()
            ),
        )?;
        let value = self
            .manager
            .apply_replay_plan(session_id, self.adapter, self.store, plan)?;
        Ok(MutationReport {
            value,
            policy_events,
        })
    }

    pub fn add_trigger(
        &mut self,
        session_id: SessionId,
        trigger: Trigger,
    ) -> SwatResult<MutationReport<TriggerId>> {
        let trigger_id = trigger.trigger_id;
        let policy_events = self.record_policy_event(
            session_id,
            PolicyVerdict::Allow,
            format!(
                "policy allowed trigger add {} ({})",
                trigger_id.raw(),
                trigger.name
            ),
        )?;
        self.trigger_engine.add_trigger(trigger);
        Ok(MutationReport {
            value: trigger_id,
            policy_events,
        })
    }

    pub fn remove_trigger(
        &mut self,
        session_id: SessionId,
        trigger_id: TriggerId,
    ) -> SwatResult<MutationReport<Trigger>> {
        let Some(trigger) = self.trigger_engine.remove_trigger(trigger_id) else {
            let summary = format!(
                "policy denied trigger removal for unknown {}",
                trigger_id.raw()
            );
            let _ = self.record_policy_event(session_id, PolicyVerdict::Deny, summary.clone());
            return Err(SwatError::new(format!(
                "unknown trigger {}",
                trigger_id.raw()
            )));
        };
        let policy_events = self.record_policy_event(
            session_id,
            PolicyVerdict::Allow,
            format!(
                "policy allowed trigger removal {} ({})",
                trigger_id.raw(),
                trigger.name
            ),
        )?;
        Ok(MutationReport {
            value: trigger,
            policy_events,
        })
    }

    pub fn set_trigger_enabled(
        &mut self,
        session_id: SessionId,
        trigger_id: TriggerId,
        enabled: bool,
    ) -> SwatResult<MutationReport<bool>> {
        let Some(previous) = self.trigger_engine.set_enabled(trigger_id, enabled) else {
            let summary = format!(
                "policy denied trigger state change for unknown {}",
                trigger_id.raw()
            );
            let _ = self.record_policy_event(session_id, PolicyVerdict::Deny, summary.clone());
            return Err(SwatError::new(format!(
                "unknown trigger {}",
                trigger_id.raw()
            )));
        };
        let policy_events = self.record_policy_event(
            session_id,
            PolicyVerdict::Allow,
            format!(
                "policy allowed trigger {} to be {}",
                trigger_id.raw(),
                if enabled { "enabled" } else { "disabled" }
            ),
        )?;
        Ok(MutationReport {
            value: previous,
            policy_events,
        })
    }

    fn record_policy_event(
        &mut self,
        session_id: SessionId,
        verdict: PolicyVerdict,
        summary: String,
    ) -> SwatResult<Vec<EventEnvelope>> {
        self.manager.record_emission(
            session_id,
            self.store,
            swat_core::AdapterEmission {
                pending_events: vec![PendingEvent::new(
                    EventKind::PolicyDecision,
                    EventPayload::Policy { verdict, summary },
                )],
                pending_artifacts: Vec::new(),
            },
        )
    }
}

fn control_allowed(capabilities: swat_core::CapabilitySet, action: &ControlAction) -> bool {
    match action {
        ControlAction::Pause => capabilities.can_stop,
        ControlAction::Resume => capabilities.can_resume,
        ControlAction::Step => capabilities.can_step,
        ControlAction::CreateSnapshot { .. } => capabilities.can_snapshot,
    }
}

fn payload_summary(event: &EventEnvelope) -> Option<&str> {
    match &event.payload {
        swat_core::EventPayload::Empty => None,
        swat_core::EventPayload::Text { summary }
        | swat_core::EventPayload::Control { summary, .. }
        | swat_core::EventPayload::Boundary { summary, .. }
        | swat_core::EventPayload::Snapshot { summary, .. }
        | swat_core::EventPayload::Trigger { summary, .. }
        | swat_core::EventPayload::Value { summary, .. }
        | swat_core::EventPayload::Policy { summary, .. } => Some(summary.as_str()),
    }
}
