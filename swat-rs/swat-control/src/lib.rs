#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use swat_core::{
    AdapterEmission, ControlAction, ControlResponse, EventEnvelope, EventId, EventKind,
    EventPayload, PendingEvent, SwatResult, TargetAdapter, Timestamp, TriggerId,
};
use swat_expr::{QueryExpr, evaluate_expression, format_expression};
use swat_schema::{SchemaNode, validate_decoded_value};
use swat_session::{ControlReport, PumpReport, SessionManager};
use swat_store::SwatStore;
use swat_value::{QueriedValue, decode_artifact};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerPredicate {
    Expr(QueryExpr),
    Named(String),
    ValueChanged {
        value_key: String,
        path: Option<String>,
    },
    ObservedAfter {
        millis: u64,
    },
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
pub struct TriggerPredicateDefinition {
    pub name: String,
    pub predicate: TriggerPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ValueWatchKey {
    value_key: String,
    path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum WatchedValue {
    Text(String),
    Query(QueriedValue),
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
    pub group: Option<String>,
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
            group: None,
            predicate,
            actions,
            fire_once: false,
            enabled: true,
            hit_count: 0,
            last_hit_event_id: None,
            last_hit_sequence_no: None,
        }
    }

    pub fn in_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
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
pub struct TriggerGroupPolicy {
    pub name: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerMatch {
    pub trigger_id: TriggerId,
    pub trigger_name: String,
    pub group: Option<String>,
    pub predicate_name: Option<String>,
    pub event_id: EventId,
    pub sequence_no: u64,
    pub summary: String,
    pub actions: Vec<TriggerAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReasonKind {
    Breakpoint,
    TargetExit,
    ControlRejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopReason {
    pub kind: StopReasonKind,
    pub summary: String,
    pub trigger_id: Option<TriggerId>,
    pub trigger_name: Option<String>,
    pub group: Option<String>,
    pub event_id: Option<EventId>,
    pub sequence_no: Option<u64>,
    pub control_action: Option<ControlAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlledPumpReport {
    pub pump_report: PumpReport,
    pub trigger_events: Vec<EventEnvelope>,
    pub trigger_matches: Vec<TriggerMatch>,
    pub control_reports: Vec<ControlReport>,
    pub stop_reasons: Vec<StopReason>,
}

#[derive(Default)]
pub struct TriggerEngine {
    triggers: Vec<Trigger>,
    fired_once: BTreeSet<TriggerId>,
    predicate_library: BTreeMap<String, TriggerPredicate>,
    group_policies: BTreeMap<String, bool>,
    first_observed_at: Option<Timestamp>,
    watched_values: BTreeMap<ValueWatchKey, WatchedValue>,
}

impl TriggerEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_trigger(mut self, trigger: Trigger) -> Self {
        self.add_trigger(trigger);
        self
    }

    pub fn add_trigger(&mut self, trigger: Trigger) {
        if let Some(group) = trigger.group.as_ref() {
            self.group_policies.entry(group.clone()).or_insert(true);
        }
        self.triggers.push(trigger);
    }

    pub fn remove_trigger(&mut self, trigger_id: TriggerId) -> Option<Trigger> {
        let index = self
            .triggers
            .iter()
            .position(|trigger| trigger.trigger_id == trigger_id)?;
        self.fired_once.remove(&trigger_id);
        let trigger = self.triggers.remove(index);
        if let Some(group) = trigger.group.as_deref() {
            if !self
                .triggers
                .iter()
                .any(|other| other.group.as_deref() == Some(group))
            {
                self.group_policies.remove(group);
            }
        }
        Some(trigger)
    }

    pub fn triggers(&self) -> &[Trigger] {
        &self.triggers
    }

    pub fn predicate(&self, name: &str) -> Option<&TriggerPredicate> {
        self.predicate_library.get(name)
    }

    pub fn predicate_definitions(&self) -> Vec<TriggerPredicateDefinition> {
        self.predicate_library
            .iter()
            .map(|(name, predicate)| TriggerPredicateDefinition {
                name: name.clone(),
                predicate: predicate.clone(),
            })
            .collect()
    }

    pub fn define_predicate(
        &mut self,
        name: impl Into<String>,
        predicate: TriggerPredicate,
    ) -> Option<TriggerPredicate> {
        self.predicate_library.insert(name.into(), predicate)
    }

    pub fn remove_predicate(&mut self, name: &str) -> Option<TriggerPredicate> {
        self.predicate_library.remove(name)
    }

    pub fn group_enabled(&self, group: &str) -> Option<bool> {
        self.group_policies.get(group).copied()
    }

    pub fn group_policies(&self) -> Vec<TriggerGroupPolicy> {
        self.group_policies
            .iter()
            .map(|(name, enabled)| TriggerGroupPolicy {
                name: name.clone(),
                enabled: *enabled,
            })
            .collect()
    }

    pub fn set_group_enabled(&mut self, group: &str, enabled: bool) -> Option<bool> {
        let policy = self.group_policies.get_mut(group)?;
        let previous = *policy;
        *policy = enabled;
        Some(previous)
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
        if self.first_observed_at.is_none() {
            self.first_observed_at = Some(event.observed_at);
        }
        let mut matches = Vec::new();
        let mut pending_value_updates = BTreeMap::new();

        for trigger in &mut self.triggers {
            if !trigger.enabled {
                continue;
            }
            if let Some(group) = trigger.group.as_deref() {
                if !self.group_policies.get(group).copied().unwrap_or(true) {
                    continue;
                }
            }
            if trigger.fire_once && self.fired_once.contains(&trigger.trigger_id) {
                continue;
            }
            if !predicate_matches(
                &trigger.predicate,
                event,
                store,
                &self.predicate_library,
                self.first_observed_at,
                &self.watched_values,
                &mut pending_value_updates,
                &mut BTreeSet::new(),
            ) {
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
                group: trigger.group.clone(),
                predicate_name: top_level_predicate_name(&trigger.predicate),
                event_id: event.event_id,
                sequence_no: event.sequence_no,
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

        self.watched_values.extend(pending_value_updates);

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

    let stop_reasons = collect_stop_reasons(&pump_report, &trigger_matches, &control_reports);

    Ok(ControlledPumpReport {
        pump_report,
        trigger_events,
        trigger_matches,
        control_reports,
        stop_reasons,
    })
}

pub fn format_trigger_predicate(predicate: &TriggerPredicate) -> String {
    match predicate {
        TriggerPredicate::Expr(expr) => format_expression(expr),
        TriggerPredicate::Named(name) => format!("@{name}"),
        TriggerPredicate::ValueChanged { value_key, path } => match path {
            Some(path) => format!("watch {value_key} {path} changed"),
            None => format!("watch {value_key} changed"),
        },
        TriggerPredicate::ObservedAfter { millis } => format!("after {millis}ms"),
        TriggerPredicate::EventKindIs(kind) => format!("kind == {kind:?}"),
        TriggerPredicate::SummaryContains(needle) => format!("summary contains {needle:?}"),
        TriggerPredicate::ArtifactUtf8Contains(needle) => {
            format!("artifact.text contains {needle:?}")
        }
        TriggerPredicate::ArtifactJsonPathExists(path) => {
            format!("artifact.json {path} exists")
        }
        TriggerPredicate::ArtifactJsonPathEquals { path, expected } => {
            format!("artifact.json {path} == {}", format_queried_value(expected))
        }
        TriggerPredicate::ArtifactJsonFailsSchema(_) => "artifact.json fails <schema>".to_string(),
        TriggerPredicate::All(predicates) => format_joined_predicates(predicates, "and"),
        TriggerPredicate::Any(predicates) => format_joined_predicates(predicates, "or"),
    }
}

fn collect_stop_reasons(
    pump_report: &PumpReport,
    trigger_matches: &[TriggerMatch],
    control_reports: &[ControlReport],
) -> Vec<StopReason> {
    let mut stop_reasons = Vec::new();

    for trigger_match in trigger_matches {
        if trigger_match
            .actions
            .iter()
            .any(|action| matches!(action, TriggerAction::PauseTarget))
        {
            stop_reasons.push(StopReason {
                kind: StopReasonKind::Breakpoint,
                summary: trigger_match.summary.clone(),
                trigger_id: Some(trigger_match.trigger_id),
                trigger_name: Some(trigger_match.trigger_name.clone()),
                group: trigger_match.group.clone(),
                event_id: Some(trigger_match.event_id),
                sequence_no: Some(trigger_match.sequence_no),
                control_action: Some(ControlAction::Pause),
            });
        }
    }

    for control_report in control_reports {
        if control_report.response.accepted {
            continue;
        }
        stop_reasons.push(StopReason {
            kind: StopReasonKind::ControlRejected,
            summary: control_report.response.summary.clone(),
            trigger_id: None,
            trigger_name: None,
            group: None,
            event_id: None,
            sequence_no: None,
            control_action: control_action_from_report(control_report),
        });
    }

    for event in &pump_report.stored_events {
        let Some(summary) = target_exit_summary(event) else {
            continue;
        };
        stop_reasons.push(StopReason {
            kind: StopReasonKind::TargetExit,
            summary: summary.to_string(),
            trigger_id: None,
            trigger_name: None,
            group: None,
            event_id: Some(event.event_id),
            sequence_no: Some(event.sequence_no),
            control_action: None,
        });
    }

    stop_reasons
}

fn predicate_matches<S: SwatStore + ?Sized>(
    predicate: &TriggerPredicate,
    event: &EventEnvelope,
    store: &S,
    predicate_library: &BTreeMap<String, TriggerPredicate>,
    first_observed_at: Option<Timestamp>,
    watched_values: &BTreeMap<ValueWatchKey, WatchedValue>,
    pending_value_updates: &mut BTreeMap<ValueWatchKey, WatchedValue>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    match predicate {
        TriggerPredicate::Expr(expr) => evaluate_expression(store, event, expr),
        TriggerPredicate::Named(name) => {
            if !visiting.insert(name.clone()) {
                return false;
            }
            let matched = predicate_library
                .get(name)
                .map(|predicate| {
                    predicate_matches(
                        predicate,
                        event,
                        store,
                        predicate_library,
                        first_observed_at,
                        watched_values,
                        pending_value_updates,
                        visiting,
                    )
                })
                .unwrap_or(false);
            visiting.remove(name);
            matched
        }
        TriggerPredicate::ValueChanged { value_key, path } => {
            let key = ValueWatchKey {
                value_key: value_key.clone(),
                path: path.clone(),
            };
            let Some(current) = watched_value_for_event(event, store, &key) else {
                return false;
            };
            pending_value_updates.insert(key.clone(), current.clone());
            watched_values
                .get(&key)
                .map(|previous| previous != &current)
                .unwrap_or(false)
        }
        TriggerPredicate::ObservedAfter { millis } => first_observed_at
            .map(|first_observed_at| {
                event
                    .observed_at
                    .as_millis()
                    .saturating_sub(first_observed_at.as_millis())
                    >= *millis
            })
            .unwrap_or(false),
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
        TriggerPredicate::All(predicates) => predicates.iter().all(|predicate| {
            predicate_matches(
                predicate,
                event,
                store,
                predicate_library,
                first_observed_at,
                watched_values,
                pending_value_updates,
                visiting,
            )
        }),
        TriggerPredicate::Any(predicates) => predicates.iter().any(|predicate| {
            predicate_matches(
                predicate,
                event,
                store,
                predicate_library,
                first_observed_at,
                watched_values,
                pending_value_updates,
                visiting,
            )
        }),
    }
}

fn control_action_from_report(report: &ControlReport) -> Option<ControlAction> {
    report
        .stored_events
        .iter()
        .find_map(|event| match &event.payload {
            EventPayload::Control { action, .. } => Some(action.clone()),
            _ => None,
        })
}

fn format_joined_predicates(predicates: &[TriggerPredicate], operator: &str) -> String {
    predicates
        .iter()
        .map(|predicate| format!("({})", format_trigger_predicate(predicate)))
        .collect::<Vec<_>>()
        .join(&format!(" {operator} "))
}

fn format_queried_value(value: &QueriedValue) -> String {
    match value {
        QueriedValue::Null => "null".to_string(),
        QueriedValue::Bool(value) => value.to_string(),
        QueriedValue::Number(value) => value.clone(),
        QueriedValue::String(value) => format!("{value:?}"),
        QueriedValue::Json(value) => value.clone(),
    }
}

fn watched_value_for_event<S: SwatStore + ?Sized>(
    event: &EventEnvelope,
    store: &S,
    key: &ValueWatchKey,
) -> Option<WatchedValue> {
    let EventPayload::Value {
        value_key: event_value_key,
        ..
    } = &event.payload
    else {
        return None;
    };
    if event_value_key != &key.value_key {
        return None;
    }

    if let Some(path) = key.path.as_deref() {
        return event.artifact_refs.iter().find_map(|artifact_ref| {
            store
                .artifact(artifact_ref.artifact_id)
                .and_then(|artifact| decode_artifact(artifact).ok())
                .and_then(|value| value.query_json_path(path).ok())
                .flatten()
                .map(WatchedValue::Query)
        });
    }

    event
        .artifact_refs
        .iter()
        .find_map(|artifact_ref| {
            store
                .artifact(artifact_ref.artifact_id)
                .and_then(|artifact| decode_artifact(artifact).ok())
                .map(|value| WatchedValue::Text(value.detail()))
        })
        .or_else(|| payload_summary(event).map(|summary| WatchedValue::Text(summary.to_string())))
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

fn target_exit_summary(event: &EventEnvelope) -> Option<&str> {
    match &event.payload {
        EventPayload::Text { summary }
            if event.kind == EventKind::Lifecycle && summary.contains("exited") =>
        {
            Some(summary.as_str())
        }
        _ => None,
    }
}

fn top_level_predicate_name(predicate: &TriggerPredicate) -> Option<String> {
    match predicate {
        TriggerPredicate::Named(name) => Some(name.clone()),
        _ => None,
    }
}

pub fn last_control_response(report: &ControlledPumpReport) -> Option<&ControlResponse> {
    report.control_reports.last().map(|report| &report.response)
}
