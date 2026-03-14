#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use swat_core::{
    AdapterEmission, ControlAction, ControlResponse, EventEnvelope, EventKind, EventPayload,
    EventId, PendingEvent, SwatResult, TargetAdapter, TriggerId,
};
use swat_expr::{QueryExpr, evaluate_expression};
use swat_schema::{SchemaNode, validate_decoded_value};
use swat_session::{ControlReport, PumpReport, SessionManager};
use swat_store::SwatStore;
use swat_value::{QueriedValue, decode_artifact};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerPredicate {
    Expr(QueryExpr),
    EventKindIs(EventKind),
    SummaryContains(String),
    ArtifactUtf8Contains(String),
    ArtifactJsonPathExists(String),
    ArtifactJsonPathEquals {
        path: String,
        expected: QueriedValue,
    },
    ArtifactJsonFailsSchema(SchemaNode),
    All(Vec<TriggerPredicate>),
    Any(Vec<TriggerPredicate>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerAction {
    PauseTarget,
    CreateSnapshot { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub trigger_id: TriggerId,
    pub name: String,
    pub predicate: TriggerPredicate,
    pub actions: Vec<TriggerAction>,
    pub fire_once: bool,
    pub enabled: bool,
    pub hit_count: u64,
    pub last_hit_event_id: Option<EventId>,
    pub last_hit_sequence_no: Option<u64>,
}

impl Trigger {
    pub fn new(
        name: impl Into<String>,
        predicate: TriggerPredicate,
        actions: Vec<TriggerAction>,
    ) -> Self {
        Self {
            trigger_id: TriggerId::new(),
            name: name.into(),
            predicate,
            actions,
            fire_once: false,
            enabled: true,
            hit_count: 0,
            last_hit_event_id: None,
            last_hit_sequence_no: None,
        }
    }

    pub fn fire_once(mut self) -> Self {
        self.fire_once = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerMatch {
    pub trigger_id: TriggerId,
    pub trigger_name: String,
    pub event_id: swat_core::EventId,
    pub summary: String,
    pub actions: Vec<TriggerAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlledPumpReport {
    pub pump_report: PumpReport,
    pub trigger_events: Vec<EventEnvelope>,
    pub trigger_matches: Vec<TriggerMatch>,
    pub control_reports: Vec<ControlReport>,
}

#[derive(Default)]
pub struct TriggerEngine {
    triggers: Vec<Trigger>,
    fired_once: BTreeSet<TriggerId>,
}

impl TriggerEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_trigger(mut self, trigger: Trigger) -> Self {
        self.triggers.push(trigger);
        self
    }

    pub fn add_trigger(&mut self, trigger: Trigger) {
        self.triggers.push(trigger);
    }

    pub fn remove_trigger(&mut self, trigger_id: TriggerId) -> Option<Trigger> {
        let index = self
            .triggers
            .iter()
            .position(|trigger| trigger.trigger_id == trigger_id)?;
        self.fired_once.remove(&trigger_id);
        Some(self.triggers.remove(index))
    }

    pub fn triggers(&self) -> &[Trigger] {
        &self.triggers
    }

    pub fn set_enabled(&mut self, trigger_id: TriggerId, enabled: bool) -> Option<bool> {
        let trigger = self
            .triggers
            .iter_mut()
            .find(|trigger| trigger.trigger_id == trigger_id)?;
        let previous = trigger.enabled;
        trigger.enabled = enabled;
        Some(previous)
    }

    pub fn evaluate_event<S: SwatStore + ?Sized>(
        &mut self,
        event: &EventEnvelope,
        store: &S,
    ) -> Vec<TriggerMatch> {
        let mut matches = Vec::new();

        for trigger in &mut self.triggers {
            if !trigger.enabled {
                continue;
            }
            if trigger.fire_once && self.fired_once.contains(&trigger.trigger_id) {
                continue;
            }
            if !predicate_matches(&trigger.predicate, event, store) {
                continue;
            }

            let summary = format!(
                "trigger '{}' matched event {}",
                trigger.name,
                event.event_id.raw()
            );
            matches.push(TriggerMatch {
                trigger_id: trigger.trigger_id,
                trigger_name: trigger.name.clone(),
                event_id: event.event_id,
                summary,
                actions: trigger.actions.clone(),
            });

            trigger.hit_count += 1;
            trigger.last_hit_event_id = Some(event.event_id);
            trigger.last_hit_sequence_no = Some(event.sequence_no);

            if trigger.fire_once {
                self.fired_once.insert(trigger.trigger_id);
            }
        }

        matches
    }
}

pub fn pump_with_triggers<A: TargetAdapter + ?Sized, S: SwatStore + ?Sized>(
    manager: &mut SessionManager,
    session_id: swat_core::SessionId,
    adapter: &mut A,
    store: &mut S,
    engine: &mut TriggerEngine,
) -> SwatResult<ControlledPumpReport> {
    let pump_report = manager.pump(session_id, adapter, store)?;
    let mut trigger_matches = Vec::new();

    for event in &pump_report.stored_events {
        trigger_matches.extend(engine.evaluate_event(event, store));
    }

    let mut trigger_events = Vec::new();
    if !trigger_matches.is_empty() {
        let emission = AdapterEmission {
            pending_events: trigger_matches
                .iter()
                .map(|trigger_match| {
                    PendingEvent::new(
                        EventKind::TriggerHit,
                        EventPayload::Trigger {
                            trigger_id: trigger_match.trigger_id,
                            summary: trigger_match.summary.clone(),
                        },
                    )
                })
                .collect(),
            pending_artifacts: Vec::new(),
        };
        trigger_events = manager.record_emission(session_id, store, emission)?;
    }

    let mut control_reports = Vec::new();
    for trigger_match in &trigger_matches {
        for action in &trigger_match.actions {
            let control_action = match action {
                TriggerAction::PauseTarget => ControlAction::Pause,
                TriggerAction::CreateSnapshot { reason } => ControlAction::CreateSnapshot {
                    reason: reason.clone(),
                },
            };
            let report = manager.control(session_id, adapter, control_action, store)?;
            control_reports.push(report);
        }
    }

    Ok(ControlledPumpReport {
        pump_report,
        trigger_events,
        trigger_matches,
        control_reports,
    })
}

fn predicate_matches<S: SwatStore + ?Sized>(
    predicate: &TriggerPredicate,
    event: &EventEnvelope,
    store: &S,
) -> bool {
    match predicate {
        TriggerPredicate::Expr(expr) => evaluate_expression(store, event, expr),
        TriggerPredicate::EventKindIs(kind) => event.kind == *kind,
        TriggerPredicate::SummaryContains(needle) => payload_summary(event)
            .map(|summary| summary.contains(needle))
            .unwrap_or(false),
        TriggerPredicate::ArtifactUtf8Contains(needle) => {
            event.artifact_refs.iter().any(|artifact_ref| {
                store
                    .artifact(artifact_ref.artifact_id)
                    .and_then(|artifact| String::from_utf8(artifact.bytes).ok())
                    .map(|text| text.contains(needle))
                    .unwrap_or(false)
            })
        }
        TriggerPredicate::ArtifactJsonPathExists(path) => {
            event.artifact_refs.iter().any(|artifact_ref| {
                store
                    .artifact(artifact_ref.artifact_id)
                    .and_then(|artifact| decode_artifact(artifact).ok())
                    .and_then(|value| value.query_json_path(path).ok())
                    .flatten()
                    .is_some()
            })
        }
        TriggerPredicate::ArtifactJsonPathEquals { path, expected } => {
            event.artifact_refs.iter().any(|artifact_ref| {
                store
                    .artifact(artifact_ref.artifact_id)
                    .and_then(|artifact| decode_artifact(artifact).ok())
                    .and_then(|value| value.query_json_path(path).ok())
                    .flatten()
                    .map(|actual| actual == *expected)
                    .unwrap_or(false)
            })
        }
        TriggerPredicate::ArtifactJsonFailsSchema(schema) => {
            event.artifact_refs.iter().any(|artifact_ref| {
                store
                    .artifact(artifact_ref.artifact_id)
                    .and_then(|artifact| decode_artifact(artifact).ok())
                    .and_then(|value| validate_decoded_value(&value, schema).ok())
                    .map(|validation| !validation.is_valid())
                    .unwrap_or(false)
            })
        }
        TriggerPredicate::All(predicates) => predicates
            .iter()
            .all(|predicate| predicate_matches(predicate, event, store)),
        TriggerPredicate::Any(predicates) => predicates
            .iter()
            .any(|predicate| predicate_matches(predicate, event, store)),
    }
}

fn payload_summary(event: &EventEnvelope) -> Option<&str> {
    match &event.payload {
        EventPayload::Empty => None,
        EventPayload::Text { summary }
        | EventPayload::Control { summary, .. }
        | EventPayload::Boundary { summary, .. }
        | EventPayload::Snapshot { summary, .. }
        | EventPayload::Trigger { summary, .. }
        | EventPayload::Value { summary, .. }
        | EventPayload::Policy { summary, .. } => Some(summary.as_str()),
    }
}

pub fn last_control_response(report: &ControlledPumpReport) -> Option<&ControlResponse> {
    report.control_reports.last().map(|report| &report.response)
}
