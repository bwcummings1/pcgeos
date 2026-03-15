#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use swat_control::{
    Trigger, TriggerAction, TriggerEngine, TriggerPredicate, TriggerPredicateDefinition,
    format_trigger_predicate,
};
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
use swat_source::{
    SourceInspection, SourceLocation, SourceSnippet, extract_event_source_location,
    inspect_event_source, is_real_source_path, load_source_snippet, resolve_event_source,
};
use swat_store::SwatStore;
use swat_value::{DecodedValue, QueriedValue, ValuePresentation, decode_event_artifacts};

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
pub struct StackFrame {
    pub frame_index: usize,
    pub depth: usize,
    pub boundary_id: BoundaryId,
    pub event_kind: EventKind,
    pub label: String,
    pub event_ids: Vec<EventId>,
    pub first_event_id: EventId,
    pub last_event_id: EventId,
    pub sequence_start: u64,
    pub sequence_end: u64,
    pub correlation_id: Option<String>,
    pub span_id: Option<String>,
    pub function: Option<String>,
    pub source_file: Option<String>,
    pub source_line: Option<u64>,
    pub entry_summary: String,
    pub latest_summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFileSummary {
    pub file: String,
    pub event_count: usize,
    pub first_line: Option<usize>,
    pub last_line: Option<usize>,
    pub functions: Vec<String>,
    pub is_real_path: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BreakpointState {
    Enabled,
    Disabled,
}

impl BreakpointState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BreakpointLifetime {
    Persistent,
    Once,
}

impl BreakpointLifetime {
    pub fn label(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::Once => "once",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BreakpointDisposition {
    Passive,
    Pause,
    Snapshot,
    Mixed,
}

impl BreakpointDisposition {
    pub fn label(self) -> &'static str {
        match self {
            Self::Passive => "passive",
            Self::Pause => "pause",
            Self::Snapshot => "snapshot",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BreakpointActivity {
    NeverHit,
    Hit,
}

impl BreakpointActivity {
    pub fn label(self) -> &'static str {
        match self {
            Self::NeverHit => "never-hit",
            Self::Hit => "hit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BreakpointGroupKind {
    State,
    Lifetime,
    Disposition,
    Activity,
}

impl BreakpointGroupKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Lifetime => "lifetime",
            Self::Disposition => "disposition",
            Self::Activity => "activity",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreakpointSummary {
    pub trigger_id: TriggerId,
    pub name: String,
    pub group: Option<String>,
    pub group_enabled: Option<bool>,
    pub configured_state: BreakpointState,
    pub predicate_name: Option<String>,
    pub predicate: String,
    pub actions: Vec<String>,
    pub state: BreakpointState,
    pub lifetime: BreakpointLifetime,
    pub disposition: BreakpointDisposition,
    pub activity: BreakpointActivity,
    pub hit_count: u64,
    pub last_hit_event_id: Option<EventId>,
    pub last_hit_sequence_no: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreakpointDetail {
    pub breakpoint: BreakpointSummary,
    pub last_hit_event: Option<EventEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreakpointGroup {
    pub kind: BreakpointGroupKind,
    pub label: String,
    pub breakpoints: Vec<BreakpointSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreakpointDefinitionGroup {
    pub name: String,
    pub enabled: bool,
    pub breakpoints: Vec<BreakpointSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BreakpointPredicateSummary {
    pub name: String,
    pub predicate: String,
    pub breakpoint_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchpointSummary {
    pub breakpoint: BreakpointSummary,
    pub value_key: String,
    pub path: Option<String>,
    pub after_millis: Option<u64>,
    pub event_kind: Option<EventKind>,
    pub summary_contains: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchpointDetail {
    pub watchpoint: WatchpointSummary,
    pub last_hit_event: Option<EventEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchpointSpec {
    pub name: String,
    pub value_key: String,
    pub path: Option<String>,
    pub after_millis: Option<u64>,
    pub event_kind: Option<EventKind>,
    pub summary_contains: Option<String>,
    pub fire_once: bool,
    pub group: Option<String>,
    pub snapshot_reason: Option<String>,
}

impl WatchpointSpec {
    pub fn new(name: impl Into<String>, value_key: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value_key: value_key.into(),
            path: None,
            after_millis: None,
            event_kind: None,
            summary_contains: None,
            fire_once: false,
            group: None,
            snapshot_reason: None,
        }
    }

    pub fn at_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn after_millis(mut self, millis: u64) -> Self {
        self.after_millis = Some(millis);
        self
    }

    pub fn in_event_kind(mut self, kind: EventKind) -> Self {
        self.event_kind = Some(kind);
        self
    }

    pub fn with_summary_contains(mut self, needle: impl Into<String>) -> Self {
        self.summary_contains = Some(needle.into());
        self
    }

    pub fn fire_once(mut self) -> Self {
        self.fire_once = true;
        self
    }

    pub fn in_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn create_snapshot(mut self, reason: impl Into<String>) -> Self {
        self.snapshot_reason = Some(reason.into());
        self
    }
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

    pub fn stack_frames(&self, session_id: SessionId) -> SwatResult<Vec<StackFrame>> {
        let mut boundary_events: BTreeMap<BoundaryId, Vec<EventEnvelope>> = BTreeMap::new();
        for event in self.session_events(session_id) {
            if let EventPayload::Boundary { boundary_id, .. } = &event.payload {
                boundary_events.entry(*boundary_id).or_default().push(event);
            }
        }

        let mut frames = boundary_events
            .into_iter()
            .map(|(boundary_id, events)| self.build_stack_frame(boundary_id, events))
            .collect::<SwatResult<Vec<_>>>()?;

        frames.sort_by_key(|frame| {
            (
                frame.sequence_start,
                std::cmp::Reverse(frame.sequence_end),
                frame.boundary_id.raw(),
            )
        });

        for index in 0..frames.len() {
            let depth = frames
                .iter()
                .take(index)
                .filter(|candidate| {
                    candidate.sequence_start <= frames[index].sequence_start
                        && candidate.sequence_end >= frames[index].sequence_end
                        && (candidate.sequence_start != frames[index].sequence_start
                            || candidate.sequence_end != frames[index].sequence_end)
                })
                .count();
            frames[index].depth = depth;
        }

        frames.sort_by(|left, right| {
            right
                .depth
                .cmp(&left.depth)
                .then(right.sequence_start.cmp(&left.sequence_start))
                .then(right.sequence_end.cmp(&left.sequence_end))
                .then(right.boundary_id.raw().cmp(&left.boundary_id.raw()))
        });

        for (frame_index, frame) in frames.iter_mut().enumerate() {
            frame.frame_index = frame_index;
        }

        Ok(frames)
    }

    pub fn stack_frame(
        &self,
        session_id: SessionId,
        frame_index: usize,
    ) -> SwatResult<Option<StackFrame>> {
        Ok(self
            .stack_frames(session_id)?
            .into_iter()
            .find(|frame| frame.frame_index == frame_index))
    }

    pub fn stack_frame_by_boundary(
        &self,
        session_id: SessionId,
        boundary_id: BoundaryId,
    ) -> SwatResult<Option<StackFrame>> {
        Ok(self
            .stack_frames(session_id)?
            .into_iter()
            .find(|frame| frame.boundary_id == boundary_id))
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

    pub fn source_files(&self, session_id: SessionId) -> SwatResult<Vec<SourceFileSummary>> {
        let mut files = BTreeMap::<String, SourceFileSummaryBuilder>::new();

        for event in self.session_events(session_id) {
            let Some(location) = extract_event_source_location(self.store, &event)? else {
                continue;
            };
            let entry = files
                .entry(location.file.clone())
                .or_insert_with(|| SourceFileSummaryBuilder::new(&location.file));
            entry.event_count += 1;
            entry.first_line = Some(
                entry
                    .first_line
                    .map_or(location.line, |line| line.min(location.line)),
            );
            entry.last_line = Some(
                entry
                    .last_line
                    .map_or(location.line, |line| line.max(location.line)),
            );
            if let Some(function) = location.function {
                entry.functions.insert(function);
            }
        }

        Ok(files
            .into_values()
            .map(SourceFileSummaryBuilder::build)
            .collect())
    }

    pub fn source_file_view(
        &self,
        file: &str,
        line: usize,
        before: usize,
        after: usize,
    ) -> SwatResult<SourceSnippet> {
        load_source_snippet(
            SourceLocation {
                file: file.to_string(),
                line,
                function: None,
            },
            before,
            after,
        )
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

    fn build_stack_frame(
        &self,
        boundary_id: BoundaryId,
        events: Vec<EventEnvelope>,
    ) -> SwatResult<StackFrame> {
        let first = events
            .first()
            .cloned()
            .ok_or_else(|| SwatError::new("cannot build a stack frame without boundary events"))?;
        let last = events
            .last()
            .cloned()
            .ok_or_else(|| SwatError::new("cannot build a stack frame without boundary events"))?;
        let metadata = self.stack_frame_metadata(&events)?;
        let entry_summary = payload_summary(&first)
            .map(ToString::to_string)
            .unwrap_or_else(|| "<none>".to_string());
        let latest_summary = payload_summary(&last)
            .map(ToString::to_string)
            .unwrap_or_else(|| "<none>".to_string());
        let label = metadata
            .name
            .clone()
            .or_else(|| metadata.function.clone())
            .unwrap_or_else(|| latest_summary.clone());

        Ok(StackFrame {
            frame_index: 0,
            depth: 0,
            boundary_id,
            event_kind: first.kind,
            label,
            event_ids: events.iter().map(|event| event.event_id).collect(),
            first_event_id: first.event_id,
            last_event_id: last.event_id,
            sequence_start: first.sequence_no,
            sequence_end: last.sequence_no,
            correlation_id: metadata
                .correlation_id
                .or_else(|| first.causality.correlation_id.clone())
                .or_else(|| last.causality.correlation_id.clone()),
            span_id: metadata.span_id,
            function: metadata.function,
            source_file: metadata.source_file,
            source_line: metadata.source_line,
            entry_summary,
            latest_summary,
        })
    }

    fn stack_frame_metadata(&self, events: &[EventEnvelope]) -> SwatResult<StackFrameMetadata> {
        let mut metadata = StackFrameMetadata::default();

        for event in events {
            if metadata.correlation_id.is_none() {
                metadata.correlation_id = event.causality.correlation_id.clone();
            }

            for decoded in self.decoded_artifacts(event)? {
                maybe_set_string(
                    &mut metadata.correlation_id,
                    decoded.query_json_path("$.correlation_id")?,
                );
                maybe_set_string(&mut metadata.span_id, decoded.query_json_path("$.span_id")?);
                maybe_set_string(&mut metadata.name, decoded.query_json_path("$.name")?);
                maybe_set_string(
                    &mut metadata.function,
                    decoded.query_json_path("$.function")?,
                );
                maybe_set_string(
                    &mut metadata.source_file,
                    decoded.query_json_path("$.file")?,
                );
                maybe_set_u64(
                    &mut metadata.source_line,
                    decoded.query_json_path("$.line")?,
                )?;
            }
        }

        Ok(metadata)
    }
}

#[derive(Default)]
struct StackFrameMetadata {
    correlation_id: Option<String>,
    span_id: Option<String>,
    name: Option<String>,
    function: Option<String>,
    source_file: Option<String>,
    source_line: Option<u64>,
}

struct SourceFileSummaryBuilder {
    file: String,
    event_count: usize,
    first_line: Option<usize>,
    last_line: Option<usize>,
    functions: BTreeSet<String>,
}

impl SourceFileSummaryBuilder {
    fn new(file: &str) -> Self {
        Self {
            file: file.to_string(),
            event_count: 0,
            first_line: None,
            last_line: None,
            functions: BTreeSet::new(),
        }
    }

    fn build(self) -> SourceFileSummary {
        SourceFileSummary {
            file: self.file.clone(),
            event_count: self.event_count,
            first_line: self.first_line,
            last_line: self.last_line,
            functions: self.functions.into_iter().collect(),
            is_real_path: is_real_source_path(&self.file),
        }
    }
}

fn maybe_set_string(slot: &mut Option<String>, value: Option<QueriedValue>) {
    if slot.is_some() {
        return;
    }
    if let Some(QueriedValue::String(value)) = value {
        *slot = Some(value);
    }
}

fn maybe_set_u64(slot: &mut Option<u64>, value: Option<QueriedValue>) -> SwatResult<()> {
    if slot.is_some() {
        return Ok(());
    }
    let Some(QueriedValue::Number(raw)) = value else {
        return Ok(());
    };
    let parsed = raw.parse::<u64>().map_err(|err| {
        SwatError::new(format!(
            "failed to parse numeric frame metadata '{raw}': {err}"
        ))
    })?;
    *slot = Some(parsed);
    Ok(())
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

    pub fn breakpoint_summaries(&self) -> Vec<BreakpointSummary> {
        self.trigger_engine
            .triggers()
            .iter()
            .map(|trigger| build_breakpoint_summary(trigger, self.trigger_engine))
            .collect()
    }

    pub fn breakpoint_detail(&self, trigger_id: TriggerId) -> Option<BreakpointDetail> {
        let breakpoint = self
            .trigger_engine
            .triggers()
            .iter()
            .find(|trigger| trigger.trigger_id == trigger_id)
            .map(|trigger| build_breakpoint_summary(trigger, self.trigger_engine))?;
        let last_hit_event = breakpoint
            .last_hit_event_id
            .and_then(|event_id| self.inspector().event_by_id(event_id));
        Some(BreakpointDetail {
            breakpoint,
            last_hit_event,
        })
    }

    pub fn breakpoint_groups(&self) -> Vec<BreakpointGroup> {
        let breakpoints = self.breakpoint_summaries();
        let mut groups = Vec::new();
        groups.extend(select_breakpoint_groups(
            &breakpoints,
            BreakpointGroupKind::State,
            &[
                ("enabled", |breakpoint: &BreakpointSummary| {
                    breakpoint.state == BreakpointState::Enabled
                }),
                ("disabled", |breakpoint: &BreakpointSummary| {
                    breakpoint.state == BreakpointState::Disabled
                }),
            ],
        ));
        groups.extend(select_breakpoint_groups(
            &breakpoints,
            BreakpointGroupKind::Lifetime,
            &[
                ("persistent", |breakpoint: &BreakpointSummary| {
                    breakpoint.lifetime == BreakpointLifetime::Persistent
                }),
                ("once", |breakpoint: &BreakpointSummary| {
                    breakpoint.lifetime == BreakpointLifetime::Once
                }),
            ],
        ));
        groups.extend(select_breakpoint_groups(
            &breakpoints,
            BreakpointGroupKind::Disposition,
            &[
                ("pause", |breakpoint: &BreakpointSummary| {
                    breakpoint.disposition == BreakpointDisposition::Pause
                }),
                ("snapshot", |breakpoint: &BreakpointSummary| {
                    breakpoint.disposition == BreakpointDisposition::Snapshot
                }),
                ("mixed", |breakpoint: &BreakpointSummary| {
                    breakpoint.disposition == BreakpointDisposition::Mixed
                }),
                ("passive", |breakpoint: &BreakpointSummary| {
                    breakpoint.disposition == BreakpointDisposition::Passive
                }),
            ],
        ));
        groups.extend(select_breakpoint_groups(
            &breakpoints,
            BreakpointGroupKind::Activity,
            &[
                ("hit", |breakpoint: &BreakpointSummary| {
                    breakpoint.activity == BreakpointActivity::Hit
                }),
                ("never-hit", |breakpoint: &BreakpointSummary| {
                    breakpoint.activity == BreakpointActivity::NeverHit
                }),
            ],
        ));
        groups
    }

    pub fn breakpoint_definition_groups(&self) -> Vec<BreakpointDefinitionGroup> {
        let breakpoints = self.breakpoint_summaries();
        self.trigger_engine
            .group_policies()
            .into_iter()
            .filter_map(|policy| {
                let grouped = breakpoints
                    .iter()
                    .filter(|breakpoint| breakpoint.group.as_deref() == Some(policy.name.as_str()))
                    .cloned()
                    .collect::<Vec<_>>();
                if grouped.is_empty() {
                    None
                } else {
                    Some(BreakpointDefinitionGroup {
                        name: policy.name,
                        enabled: policy.enabled,
                        breakpoints: grouped,
                    })
                }
            })
            .collect()
    }

    pub fn breakpoint_predicates(&self) -> Vec<BreakpointPredicateSummary> {
        let breakpoints = self.breakpoint_summaries();
        self.trigger_engine
            .predicate_definitions()
            .into_iter()
            .map(|definition| BreakpointPredicateSummary {
                breakpoint_count: breakpoints
                    .iter()
                    .filter(|breakpoint| {
                        breakpoint.predicate_name.as_deref() == Some(definition.name.as_str())
                    })
                    .count(),
                name: definition.name,
                predicate: format_trigger_predicate(&definition.predicate),
            })
            .collect()
    }

    pub fn watchpoint_summaries(&self) -> Vec<WatchpointSummary> {
        self.trigger_engine
            .triggers()
            .iter()
            .filter_map(|trigger| build_watchpoint_summary(trigger, self.trigger_engine))
            .collect()
    }

    pub fn watchpoint_detail(&self, trigger_id: TriggerId) -> Option<WatchpointDetail> {
        let watchpoint = self
            .trigger_engine
            .triggers()
            .iter()
            .find(|trigger| trigger.trigger_id == trigger_id)
            .and_then(|trigger| build_watchpoint_summary(trigger, self.trigger_engine))?;
        let last_hit_event = watchpoint
            .breakpoint
            .last_hit_event_id
            .and_then(|event_id| self.inspector().event_by_id(event_id));
        Some(WatchpointDetail {
            watchpoint,
            last_hit_event,
        })
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

    pub fn add_watchpoint(
        &mut self,
        session_id: SessionId,
        spec: WatchpointSpec,
    ) -> SwatResult<MutationReport<TriggerId>> {
        self.add_trigger(session_id, build_watchpoint_trigger(spec))
    }

    pub fn define_breakpoint_predicate(
        &mut self,
        session_id: SessionId,
        name: impl Into<String>,
        predicate: TriggerPredicate,
    ) -> SwatResult<MutationReport<bool>> {
        let name = name.into();
        let replaced = self
            .trigger_engine
            .define_predicate(name.clone(), predicate.clone())
            .is_some();
        let policy_events = self.record_policy_event(
            session_id,
            PolicyVerdict::Allow,
            format!(
                "policy allowed breakpoint predicate {} to be {}",
                name,
                if replaced { "updated" } else { "added" }
            ),
        )?;
        Ok(MutationReport {
            value: replaced,
            policy_events,
        })
    }

    pub fn remove_breakpoint_predicate(
        &mut self,
        session_id: SessionId,
        name: &str,
    ) -> SwatResult<MutationReport<TriggerPredicateDefinition>> {
        let Some(predicate) = self.trigger_engine.remove_predicate(name) else {
            let summary = format!(
                "policy denied breakpoint predicate removal for unknown {}",
                name
            );
            let _ = self.record_policy_event(session_id, PolicyVerdict::Deny, summary.clone());
            return Err(SwatError::new(format!(
                "unknown breakpoint predicate {}",
                name
            )));
        };
        let policy_events = self.record_policy_event(
            session_id,
            PolicyVerdict::Allow,
            format!("policy allowed breakpoint predicate removal {}", name),
        )?;
        Ok(MutationReport {
            value: TriggerPredicateDefinition {
                name: name.to_string(),
                predicate,
            },
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

    pub fn set_breakpoint_group_enabled(
        &mut self,
        session_id: SessionId,
        group: &str,
        enabled: bool,
    ) -> SwatResult<MutationReport<bool>> {
        let Some(previous) = self.trigger_engine.set_group_enabled(group, enabled) else {
            let summary = format!(
                "policy denied breakpoint group state change for unknown {}",
                group
            );
            let _ = self.record_policy_event(session_id, PolicyVerdict::Deny, summary.clone());
            return Err(SwatError::new(format!(
                "unknown breakpoint group {}",
                group
            )));
        };
        let policy_events = self.record_policy_event(
            session_id,
            PolicyVerdict::Allow,
            format!(
                "policy allowed breakpoint group {} to be {}",
                group,
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

fn build_breakpoint_summary(trigger: &Trigger, engine: &TriggerEngine) -> BreakpointSummary {
    let actions = trigger
        .actions
        .iter()
        .map(format_trigger_action)
        .collect::<Vec<_>>();
    let group_enabled = trigger
        .group
        .as_deref()
        .and_then(|group| engine.group_enabled(group));
    let configured_state = if trigger.enabled {
        BreakpointState::Enabled
    } else {
        BreakpointState::Disabled
    };
    let state = if trigger.enabled && group_enabled.unwrap_or(true) {
        BreakpointState::Enabled
    } else {
        BreakpointState::Disabled
    };
    BreakpointSummary {
        trigger_id: trigger.trigger_id,
        name: trigger.name.clone(),
        group: trigger.group.clone(),
        group_enabled,
        configured_state,
        predicate_name: breakpoint_predicate_name(&trigger.predicate),
        predicate: format_trigger_predicate(&trigger.predicate),
        actions,
        state,
        lifetime: if trigger.fire_once {
            BreakpointLifetime::Once
        } else {
            BreakpointLifetime::Persistent
        },
        disposition: breakpoint_disposition(&trigger.actions),
        activity: if trigger.hit_count == 0 {
            BreakpointActivity::NeverHit
        } else {
            BreakpointActivity::Hit
        },
        hit_count: trigger.hit_count,
        last_hit_event_id: trigger.last_hit_event_id,
        last_hit_sequence_no: trigger.last_hit_sequence_no,
    }
}

#[derive(Default)]
struct WatchpointDescriptor {
    value_key: Option<String>,
    path: Option<String>,
    after_millis: Option<u64>,
    event_kind: Option<EventKind>,
    summary_contains: Option<String>,
}

fn build_watchpoint_summary(
    trigger: &Trigger,
    engine: &TriggerEngine,
) -> Option<WatchpointSummary> {
    let descriptor = extract_watchpoint_descriptor(&trigger.predicate, engine)?;
    Some(WatchpointSummary {
        breakpoint: build_breakpoint_summary(trigger, engine),
        value_key: descriptor.value_key?,
        path: descriptor.path,
        after_millis: descriptor.after_millis,
        event_kind: descriptor.event_kind,
        summary_contains: descriptor.summary_contains,
    })
}

fn build_watchpoint_trigger(spec: WatchpointSpec) -> Trigger {
    let mut predicates = vec![TriggerPredicate::ValueChanged {
        value_key: spec.value_key,
        path: spec.path,
    }];
    if let Some(millis) = spec.after_millis {
        predicates.push(TriggerPredicate::ObservedAfter { millis });
    }
    if let Some(kind) = spec.event_kind {
        predicates.push(TriggerPredicate::EventKindIs(kind));
    }
    if let Some(summary_contains) = spec.summary_contains {
        predicates.push(TriggerPredicate::SummaryContains(summary_contains));
    }
    let predicate = if predicates.len() == 1 {
        predicates.remove(0)
    } else {
        TriggerPredicate::All(predicates)
    };
    let actions = spec
        .snapshot_reason
        .map(|reason| vec![TriggerAction::CreateSnapshot { reason }])
        .unwrap_or_else(|| vec![TriggerAction::PauseTarget]);
    let mut trigger = Trigger::new(spec.name, predicate, actions);
    if spec.fire_once {
        trigger = trigger.fire_once();
    }
    if let Some(group) = spec.group {
        trigger = trigger.in_group(group);
    }
    trigger
}

fn extract_watchpoint_descriptor(
    predicate: &TriggerPredicate,
    engine: &TriggerEngine,
) -> Option<WatchpointDescriptor> {
    match predicate {
        TriggerPredicate::Named(name) => engine
            .predicate(name)
            .and_then(|predicate| extract_watchpoint_descriptor(predicate, engine)),
        TriggerPredicate::ValueChanged { value_key, path } => Some(WatchpointDescriptor {
            value_key: Some(value_key.clone()),
            path: path.clone(),
            after_millis: None,
            event_kind: None,
            summary_contains: None,
        }),
        TriggerPredicate::ObservedAfter { millis } => Some(WatchpointDescriptor {
            value_key: None,
            path: None,
            after_millis: Some(*millis),
            event_kind: None,
            summary_contains: None,
        }),
        TriggerPredicate::EventKindIs(kind) => Some(WatchpointDescriptor {
            value_key: None,
            path: None,
            after_millis: None,
            event_kind: Some(*kind),
            summary_contains: None,
        }),
        TriggerPredicate::SummaryContains(summary) => Some(WatchpointDescriptor {
            value_key: None,
            path: None,
            after_millis: None,
            event_kind: None,
            summary_contains: Some(summary.clone()),
        }),
        TriggerPredicate::All(predicates) => {
            let mut descriptor = WatchpointDescriptor::default();
            for predicate in predicates {
                let next = extract_watchpoint_descriptor(predicate, engine)?;
                merge_watchpoint_descriptor(&mut descriptor, next)?;
            }
            descriptor.value_key.as_ref()?;
            Some(descriptor)
        }
        _ => None,
    }
}

fn merge_watchpoint_descriptor(
    descriptor: &mut WatchpointDescriptor,
    next: WatchpointDescriptor,
) -> Option<()> {
    if let Some(value_key) = next.value_key {
        if descriptor.value_key.replace(value_key).is_some() {
            return None;
        }
    }
    if let Some(path) = next.path {
        if descriptor.path.replace(path).is_some() {
            return None;
        }
    }
    if let Some(after_millis) = next.after_millis {
        if descriptor.after_millis.replace(after_millis).is_some() {
            return None;
        }
    }
    if let Some(event_kind) = next.event_kind {
        if descriptor.event_kind.replace(event_kind).is_some() {
            return None;
        }
    }
    if let Some(summary_contains) = next.summary_contains {
        if descriptor
            .summary_contains
            .replace(summary_contains)
            .is_some()
        {
            return None;
        }
    }
    Some(())
}

fn breakpoint_disposition(actions: &[swat_control::TriggerAction]) -> BreakpointDisposition {
    let mut saw_pause = false;
    let mut saw_snapshot = false;
    for action in actions {
        match action {
            swat_control::TriggerAction::PauseTarget => saw_pause = true,
            swat_control::TriggerAction::CreateSnapshot { .. } => saw_snapshot = true,
        }
    }

    match (saw_pause, saw_snapshot) {
        (false, false) => BreakpointDisposition::Passive,
        (true, false) => BreakpointDisposition::Pause,
        (false, true) => BreakpointDisposition::Snapshot,
        (true, true) => BreakpointDisposition::Mixed,
    }
}

fn format_trigger_action(action: &swat_control::TriggerAction) -> String {
    match action {
        swat_control::TriggerAction::PauseTarget => "PauseTarget".to_string(),
        swat_control::TriggerAction::CreateSnapshot { reason } => {
            format!("CreateSnapshot({reason:?})")
        }
    }
}

fn breakpoint_predicate_name(predicate: &swat_control::TriggerPredicate) -> Option<String> {
    match predicate {
        swat_control::TriggerPredicate::Named(name) => Some(name.clone()),
        _ => None,
    }
}

fn select_breakpoint_groups(
    breakpoints: &[BreakpointSummary],
    kind: BreakpointGroupKind,
    selectors: &[(&str, fn(&BreakpointSummary) -> bool)],
) -> Vec<BreakpointGroup> {
    selectors
        .iter()
        .filter_map(|(label, predicate)| {
            let grouped = breakpoints
                .iter()
                .filter(|breakpoint| predicate(breakpoint))
                .cloned()
                .collect::<Vec<_>>();
            if grouped.is_empty() {
                None
            } else {
                Some(BreakpointGroup {
                    kind,
                    label: (*label).to_string(),
                    breakpoints: grouped,
                })
            }
        })
        .collect()
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
