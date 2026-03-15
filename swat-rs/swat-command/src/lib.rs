#![forbid(unsafe_code)]

mod registry;

use std::collections::BTreeMap;
use std::fs;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use swat_api::{
    BreakpointDefinitionGroup, BreakpointGroupKind, BreakpointPredicateSummary, BreakpointSummary,
    LiveSessionApi, StackFrame, TraceInspector, WatchpointSpec, WatchpointSummary,
};
use swat_control::{
    StopReason, StopReasonKind, Trigger, TriggerAction, TriggerEngine, TriggerMatch,
    TriggerPredicate, format_trigger_predicate, pump_with_triggers,
};
use swat_core::{
    BoundaryId, ControlAction, EventEnvelope, EventId, EventKind, EventPayload, SessionId,
    SnapshotId, SwatError, SwatResult, TargetAdapter, TriggerId,
};
use swat_expr::parse_expression;
use swat_script::ScriptHost;
use swat_session::SessionManager;
use swat_store::SwatStore;

pub use registry::{CommandSurface, command_completions, command_help, command_search};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BreakpointConditionInput {
    Expression(String),
    PredicateRef(String),
}

impl BreakpointConditionInput {
    fn as_expression(&self) -> Option<&str> {
        match self {
            Self::Expression(expr) => Some(expr.as_str()),
            Self::PredicateRef(_) => None,
        }
    }

    fn predicate_ref(&self) -> Option<&str> {
        match self {
            Self::Expression(_) => None,
            Self::PredicateRef(name) => Some(name.as_str()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Help {
        topic: Option<String>,
    },
    HelpSearch {
        needle: String,
    },
    Attach,
    Session,
    Pump,
    Pause,
    Resume,
    Step,
    Snapshot {
        reason: String,
    },
    Snapshots,
    SnapshotShow {
        snapshot_id: SnapshotId,
    },
    Events {
        kind: Option<EventKind>,
    },
    Event {
        event_id: EventId,
    },
    Artifacts {
        event_id: EventId,
    },
    ArtifactShow {
        event_id: EventId,
        index: usize,
    },
    Query {
        expr: String,
    },
    Entities {
        needle: String,
    },
    Correlation {
        correlation_id: String,
    },
    Breakpoints,
    BreakpointShow {
        trigger_id: TriggerId,
    },
    BreakpointGroups,
    BreakpointDefinitionGroups,
    BreakpointPredicates,
    BreakpointPredicateAdd {
        name: String,
        expr: String,
    },
    BreakpointPredicateRemove {
        name: String,
    },
    BreakpointGroupEnable {
        group: String,
    },
    BreakpointGroupDisable {
        group: String,
    },
    Watchpoints,
    WatchpointShow {
        trigger_id: TriggerId,
    },
    WatchpointAdd {
        spec: WatchpointSpec,
    },
    WatchpointEnable {
        trigger_id: TriggerId,
    },
    WatchpointDisable {
        trigger_id: TriggerId,
    },
    WatchpointRemove {
        trigger_id: TriggerId,
    },
    Triggers,
    TriggerExpr {
        name: String,
        condition: BreakpointConditionInput,
        fire_once: bool,
        group: Option<String>,
    },
    TriggerSnapshot {
        name: String,
        condition: BreakpointConditionInput,
        reason: String,
        group: Option<String>,
    },
    TriggerEnable {
        trigger_id: TriggerId,
    },
    TriggerDisable {
        trigger_id: TriggerId,
    },
    TriggerSave {
        path: String,
    },
    TriggerLoad {
        path: String,
    },
    TriggerRemove {
        trigger_id: TriggerId,
    },
    Until {
        expr: String,
    },
    Replay {
        selector_id: u64,
    },
    Spans,
    Frame {
        frame_index: usize,
    },
    Span {
        boundary_id: BoundaryId,
    },
    Source {
        event_id: EventId,
        before: usize,
        after: usize,
    },
    SourceFiles,
    SourceFile {
        file: String,
    },
    SourceView {
        file: String,
        line: usize,
        before: usize,
        after: usize,
    },
    Script {
        script: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    pub summary: String,
    pub lines: Vec<String>,
}

impl CommandOutput {
    pub fn new(summary: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            summary: summary.into(),
            lines,
        }
    }
}

pub struct CommandHost {
    manager: SessionManager,
    adapter: Box<dyn TargetAdapter>,
    store: Box<dyn SwatStore>,
    trigger_engine: TriggerEngine,
    trigger_specs: BTreeMap<TriggerId, PersistedTriggerSpec>,
    predicate_specs: BTreeMap<String, PersistedPredicateSpec>,
    session_id: Option<SessionId>,
}

impl CommandHost {
    pub fn new(adapter: Box<dyn TargetAdapter>, store: Box<dyn SwatStore>) -> Self {
        Self {
            manager: SessionManager::new(),
            adapter,
            store,
            trigger_engine: TriggerEngine::new(),
            trigger_specs: BTreeMap::new(),
            predicate_specs: BTreeMap::new(),
            session_id: None,
        }
    }

    pub fn current_session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    pub fn execute(&mut self, input: &str) -> SwatResult<CommandOutput> {
        self.execute_command(parse_command(input)?)
    }

    pub fn execute_command(&mut self, command: Command) -> SwatResult<CommandOutput> {
        match command {
            Command::Help { topic } => Ok(command_help(topic.as_deref(), CommandSurface::Shell)),
            Command::HelpSearch { needle } => Ok(command_search(&needle, CommandSurface::Shell)),
            Command::Attach => self.attach_or_describe(),
            Command::Session => self.describe_session(),
            Command::Pump => self.pump_once(),
            Command::Pause => self.control(ControlAction::Pause),
            Command::Resume => self.control(ControlAction::Resume),
            Command::Step => self.control(ControlAction::Step),
            Command::Snapshot { reason } => self.control(ControlAction::CreateSnapshot { reason }),
            Command::Snapshots => self.list_snapshots(),
            Command::SnapshotShow { snapshot_id } => self.show_snapshot(snapshot_id),
            Command::Events { kind } => self.list_events(kind),
            Command::Event { event_id } => self.show_event(event_id),
            Command::Artifacts { event_id } => self.show_artifacts(event_id),
            Command::ArtifactShow { event_id, index } => self.show_artifact_detail(event_id, index),
            Command::Query { expr } => self.query_events(&expr),
            Command::Entities { needle } => self.list_entities(&needle),
            Command::Correlation { correlation_id } => self.list_correlation(&correlation_id),
            Command::Breakpoints => self.list_breakpoints(),
            Command::BreakpointShow { trigger_id } => self.show_breakpoint(trigger_id),
            Command::BreakpointGroups => self.list_breakpoint_groups(),
            Command::BreakpointDefinitionGroups => self.list_breakpoint_definition_groups(),
            Command::BreakpointPredicates => self.list_breakpoint_predicates(),
            Command::BreakpointPredicateAdd { name, expr } => {
                self.add_breakpoint_predicate(&name, &expr)
            }
            Command::BreakpointPredicateRemove { name } => self.remove_breakpoint_predicate(&name),
            Command::BreakpointGroupEnable { group } => {
                self.set_breakpoint_group_enabled(&group, true)
            }
            Command::BreakpointGroupDisable { group } => {
                self.set_breakpoint_group_enabled(&group, false)
            }
            Command::Watchpoints => self.list_watchpoints(),
            Command::WatchpointShow { trigger_id } => self.show_watchpoint(trigger_id),
            Command::WatchpointAdd { spec } => self.add_watchpoint(spec),
            Command::WatchpointEnable { trigger_id } => {
                self.set_watchpoint_enabled(trigger_id, true)
            }
            Command::WatchpointDisable { trigger_id } => {
                self.set_watchpoint_enabled(trigger_id, false)
            }
            Command::WatchpointRemove { trigger_id } => self.remove_watchpoint(trigger_id),
            Command::Triggers => Ok(self.list_triggers()),
            Command::TriggerExpr {
                name,
                condition,
                fire_once,
                group,
            } => self.add_trigger(&name, condition, fire_once, group.as_deref()),
            Command::TriggerSnapshot {
                name,
                condition,
                reason,
                group,
            } => self.add_snapshot_trigger(&name, condition, &reason, group.as_deref()),
            Command::TriggerEnable { trigger_id } => self.set_trigger_enabled(trigger_id, true),
            Command::TriggerDisable { trigger_id } => self.set_trigger_enabled(trigger_id, false),
            Command::TriggerSave { path } => self.save_triggers(&path),
            Command::TriggerLoad { path } => self.load_triggers(&path),
            Command::TriggerRemove { trigger_id } => self.remove_trigger(trigger_id),
            Command::Until { expr } => self.until_expr(&expr),
            Command::Replay { selector_id } => self.replay(selector_id),
            Command::Spans => self.list_spans(),
            Command::Frame { frame_index } => self.show_frame(frame_index),
            Command::Span { boundary_id } => self.show_span(boundary_id),
            Command::Source {
                event_id,
                before,
                after,
            } => self.show_source(event_id, before, after),
            Command::SourceFiles => self.list_source_files(),
            Command::SourceFile { file } => self.list_source_file_events(&file),
            Command::SourceView {
                file,
                line,
                before,
                after,
            } => self.show_source_file_view(&file, line, before, after),
            Command::Script { script } => self.run_script(&script),
        }
    }

    fn attach_or_describe(&mut self) -> SwatResult<CommandOutput> {
        if self.session_id.is_some() {
            return self.describe_session();
        }

        let report = self
            .manager
            .attach(self.adapter.as_mut(), self.store.as_mut())?;
        self.session_id = Some(report.session.session_id);

        Ok(CommandOutput::new(
            format!(
                "attached session={} target={} runtime={}",
                report.session.session_id.raw(),
                report.session.target_id.raw(),
                report.session.descriptor.runtime
            ),
            report.stored_events.iter().map(format_event_line).collect(),
        ))
    }

    fn describe_session(&self) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let session = self
            .manager
            .session(session_id)
            .ok_or_else(|| SwatError::new("active session is missing from the session manager"))?;
        let inspector = self.inspector();
        let events = inspector.session_events(session_id);
        let event_count = events.len();
        let artifact_count = events
            .iter()
            .map(|event| event.artifact_refs.len())
            .sum::<usize>();
        let snapshot_count = inspector.session_snapshots(session_id).len();
        let trigger_count = self.trigger_engine.triggers().len();
        let last_event = events.last();
        let caps = session.capabilities;
        Ok(CommandOutput::new(
            format!(
                "session={} target={} adapter={} runtime={}",
                session.session_id.raw(),
                session.target_id.raw(),
                session.descriptor.adapter_name,
                session.descriptor.runtime
            ),
            vec![
                format!("target_name={}", session.descriptor.target_name),
                format!("replay_mode={:?}", session.descriptor.replay_mode),
                format!(
                    "counts events={} artifacts={} snapshots={} triggers={}",
                    event_count, artifact_count, snapshot_count, trigger_count
                ),
                format!(
                    "last_event={}",
                    last_event
                        .map(|event| format!(
                            "{} seq={} kind={:?}",
                            event.event_id.raw(),
                            event.sequence_no,
                            event.kind
                        ))
                        .unwrap_or_else(|| "-".to_string())
                ),
                format!(
                    "capabilities attach={} stream={} pause={} resume={} step={} read={} snapshot={} replay={} source={} schema={}",
                    caps.can_attach,
                    caps.can_stream_events,
                    caps.can_stop,
                    caps.can_resume,
                    caps.can_step,
                    caps.can_read_values,
                    caps.can_snapshot,
                    caps.can_inject_replay,
                    caps.can_resolve_source,
                    caps.can_resolve_schema
                ),
            ],
        ))
    }

    fn pump_once(&mut self) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let report = pump_with_triggers(
            &mut self.manager,
            session_id,
            self.adapter.as_mut(),
            self.store.as_mut(),
            &mut self.trigger_engine,
        )?;
        let lines = format_controlled_pump_lines(&report);
        Ok(CommandOutput::new(
            format!(
                "pumped {} event(s), {} trigger match(es), {} control action(s)",
                report.pump_report.stored_events.len(),
                report.trigger_matches.len(),
                report.control_reports.len()
            ),
            lines,
        ))
    }

    fn control(&mut self, action: ControlAction) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let report = {
            let mut api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.control(session_id, action)?
        };
        let control_report = report.value;
        let mut lines = report
            .policy_events
            .iter()
            .map(format_event_line)
            .collect::<Vec<_>>();
        lines.extend(control_report.stored_events.iter().map(format_event_line));
        if let Some(snapshot) = &control_report.snapshot {
            lines.push(format!(
                "snapshot={} reason={:?} captured_seq={}",
                snapshot.snapshot_id.raw(),
                snapshot.reason,
                snapshot.captured_sequence_no
            ));
        }
        let summary = control_report
            .snapshot
            .as_ref()
            .map(|snapshot| format!("created snapshot {}", snapshot.snapshot_id.raw()))
            .unwrap_or(control_report.response.summary);
        Ok(CommandOutput::new(summary, lines))
    }

    fn list_snapshots(&self) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let inspector = self.inspector();
        let lines = inspector
            .session_snapshots(session_id)
            .into_iter()
            .filter_map(|snapshot| inspector.snapshot_inspection(snapshot.snapshot_id))
            .map(|inspection| {
                format!(
                    "snapshot={} seq={} events={} replay={} reason={:?}",
                    inspection.snapshot.snapshot_id.raw(),
                    inspection.snapshot.captured_sequence_no,
                    inspection.captured_event_count,
                    inspection.replay_directive_count,
                    inspection.snapshot.reason
                )
            })
            .collect::<Vec<_>>();
        Ok(CommandOutput::new(
            format!("{} snapshot(s)", lines.len()),
            lines,
        ))
    }

    fn show_snapshot(&self, snapshot_id: SnapshotId) -> SwatResult<CommandOutput> {
        let inspector = self.inspector();
        let inspection = inspector
            .snapshot_inspection(snapshot_id)
            .ok_or_else(|| SwatError::new(format!("unknown snapshot {}", snapshot_id.raw())))?;
        let mut lines = vec![
            format!("session={}", inspection.snapshot.session_id.raw()),
            format!("target={}", inspection.snapshot.target_id.raw()),
            format!("reason={:?}", inspection.snapshot.reason),
            format!("captured_seq={}", inspection.snapshot.captured_sequence_no),
            format!("captured_events={}", inspection.captured_event_count),
            format!("replay_directives={}", inspection.replay_directive_count),
        ];
        if let Some(snapshot_event) = inspection.snapshot_event {
            lines.push(format_event_line(&snapshot_event));
        }
        Ok(CommandOutput::new(
            format!("snapshot {}", snapshot_id.raw()),
            lines,
        ))
    }

    fn list_events(&self, kind: Option<EventKind>) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let inspector = self.inspector();
        let events = match kind {
            Some(kind) => inspector.events_by_kind(session_id, kind),
            None => inspector.session_events(session_id),
        };
        let summary = match kind {
            Some(kind) => format!("{} event(s) with kind {:?}", events.len(), kind),
            None => format!("{} event(s)", events.len()),
        };
        Ok(CommandOutput::new(
            summary,
            events.iter().map(format_event_line).collect(),
        ))
    }

    fn show_event(&self, event_id: EventId) -> SwatResult<CommandOutput> {
        let event = self.lookup_event(event_id)?;
        let mut lines = vec![
            format_event_line(&event),
            format!(
                "correlation={}",
                event.causality.correlation_id.as_deref().unwrap_or("-")
            ),
            format!("artifacts={}", event.artifact_refs.len()),
        ];
        if let EventPayload::Boundary {
            boundary_id,
            determinism,
            ..
        } = event.payload
        {
            lines.push(format!(
                "boundary={} determinism={:?}",
                boundary_id.raw(),
                determinism
            ));
        }
        Ok(CommandOutput::new(
            format!("event {}", event_id.raw()),
            lines,
        ))
    }

    fn show_artifacts(&self, event_id: EventId) -> SwatResult<CommandOutput> {
        let event = self.lookup_event(event_id)?;
        let inspector = self.inspector();
        let decoded = inspector.decoded_artifacts(&event)?;
        let presentations = inspector.artifact_presentations(&event, 200)?;
        let lines = decoded
            .into_iter()
            .zip(presentations)
            .enumerate()
            .map(|(index, (value, presentation))| {
                format!(
                    "index={} artifact={} kind={:?} bytes={} lines={} preview={}",
                    index,
                    value.artifact_ref.artifact_id.raw(),
                    value.kind,
                    presentation.byte_len,
                    presentation.line_count,
                    presentation.preview
                )
            })
            .collect::<Vec<_>>();
        Ok(CommandOutput::new(
            format!("{} artifact(s) for event {}", lines.len(), event_id.raw()),
            lines,
        ))
    }

    fn show_artifact_detail(&self, event_id: EventId, index: usize) -> SwatResult<CommandOutput> {
        let event = self.lookup_event(event_id)?;
        let inspector = self.inspector();
        let decoded = inspector.decoded_artifacts(&event)?;
        let presentations = inspector.artifact_presentations(&event, 200)?;
        let Some((value, presentation)) = decoded.into_iter().zip(presentations).nth(index) else {
            return Err(SwatError::new(format!(
                "event {} has no artifact at index {}",
                event_id.raw(),
                index
            )));
        };

        let mut lines = vec![format!(
            "artifact={} kind={:?} bytes={} lines={}",
            value.artifact_ref.artifact_id.raw(),
            value.kind,
            presentation.byte_len,
            presentation.line_count
        )];
        lines.extend(
            presentation
                .detail
                .lines()
                .map(|line| format!("detail {line}")),
        );
        Ok(CommandOutput::new(
            format!("artifact {} for event {}", index, event_id.raw()),
            lines,
        ))
    }

    fn query_events(&self, expr: &str) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let events = self.inspector().query_events_str(session_id, expr)?;
        Ok(CommandOutput::new(
            format!("{} event(s) matched query", events.len()),
            events.iter().map(format_event_line).collect(),
        ))
    }

    fn list_entities(&self, needle: &str) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let inspector = self.inspector();
        let entities = inspector.find_entities(session_id, needle)?;
        Ok(CommandOutput::new(
            format!("{} entity match(es)", entities.len()),
            entities
                .into_iter()
                .map(|value| {
                    let relation_count = inspector
                        .related_entities(session_id, &value.entity)
                        .map(|relations| relations.len())
                        .unwrap_or(0);
                    format!(
                        "kind={:?} name={} events={} relations={}",
                        value.entity.kind,
                        value.entity.name,
                        value.event_ids.len(),
                        relation_count
                    )
                })
                .collect(),
        ))
    }

    fn list_correlation(&self, correlation_id: &str) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let inspector = self.inspector();
        let mut lines = Vec::new();
        if let Some(group) = inspector
            .correlation_groups(session_id)?
            .into_iter()
            .find(|group| group.correlation_id == correlation_id)
        {
            let span_ids = if group.span_ids.is_empty() {
                "-".to_string()
            } else {
                group.span_ids.join(",")
            };
            let boundary_ids = if group.boundary_ids.is_empty() {
                "-".to_string()
            } else {
                group
                    .boundary_ids
                    .iter()
                    .map(|boundary_id| boundary_id.raw().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            };
            lines.push(format!("span_ids={span_ids}"));
            lines.push(format!("boundary_ids={boundary_ids}"));
            lines.push(format!("entity_count={}", group.entities.len()));
        }
        let events = inspector.events_for_correlation(session_id, correlation_id);
        lines.extend(events.iter().map(format_event_line));
        Ok(CommandOutput::new(
            format!(
                "{} event(s) for correlation {}",
                events.len(),
                correlation_id
            ),
            lines,
        ))
    }

    fn list_triggers(&self) -> CommandOutput {
        CommandOutput::new(
            format!("{} trigger(s)", self.trigger_engine.triggers().len()),
            self.trigger_engine
                .triggers()
                .iter()
                .map(|trigger| {
                    let (condition, group, actions) = self
                        .trigger_specs
                        .get(&trigger.trigger_id)
                        .map(|spec| {
                            (
                                format!(
                                    " condition={}",
                                    format_persisted_trigger_condition(spec)
                                ),
                                spec.group
                                    .as_ref()
                                    .map(|group| format!(" group={group}"))
                                    .unwrap_or_default(),
                                format_persisted_trigger_actions(&spec.actions),
                            )
                        })
                        .unwrap_or_else(|| {
                            (
                                format!(
                                    " condition={}",
                                    format_trigger_predicate(&trigger.predicate)
                                ),
                                trigger
                                    .group
                                    .as_ref()
                                    .map(|group| format!(" group={group}"))
                                    .unwrap_or_default(),
                                format_persisted_trigger_actions(
                                    &trigger
                                        .actions
                                        .iter()
                                        .map(PersistedTriggerAction::from_runtime_action)
                                        .collect::<Vec<_>>(),
                                ),
                            )
                        });
                    format!(
                        "trigger={} name={} fire_once={} enabled={} hits={} last_event={} last_seq={} actions={}{}{}",
                        trigger.trigger_id.raw(),
                        trigger.name,
                        trigger.fire_once,
                        trigger.enabled,
                        trigger.hit_count,
                        trigger
                            .last_hit_event_id
                            .map(|event_id| event_id.raw().to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        trigger
                            .last_hit_sequence_no
                            .map(|sequence_no| sequence_no.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        actions,
                        group,
                        condition
                    )
                })
                .collect(),
        )
    }

    fn list_breakpoints(&mut self) -> SwatResult<CommandOutput> {
        let groups = {
            let api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.breakpoint_groups()
        };
        let state_groups = groups
            .into_iter()
            .filter(|group| group.kind == BreakpointGroupKind::State)
            .collect::<Vec<_>>();
        let breakpoint_count = state_groups
            .iter()
            .map(|group| group.breakpoints.len())
            .sum::<usize>();
        let mut lines = Vec::new();
        for group in state_groups {
            lines.push(format!(
                "group={} kind={} count={}",
                group.label,
                group.kind.label(),
                group.breakpoints.len()
            ));
            lines.extend(group.breakpoints.iter().map(format_breakpoint_summary));
        }
        Ok(CommandOutput::new(
            format!("{breakpoint_count} breakpoint(s)"),
            lines,
        ))
    }

    fn show_breakpoint(&mut self, trigger_id: TriggerId) -> SwatResult<CommandOutput> {
        let detail = {
            let api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.breakpoint_detail(trigger_id)
        }
        .ok_or_else(|| SwatError::new(format!("unknown breakpoint {}", trigger_id.raw())))?;

        let breakpoint = detail.breakpoint;
        let mut lines = vec![
            format!(
                "bp={} name={}",
                breakpoint.trigger_id.raw(),
                breakpoint.name
            ),
            format!(
                "state={} configured={} lifetime={} disposition={} activity={}",
                breakpoint.state.label(),
                breakpoint.configured_state.label(),
                breakpoint.lifetime.label(),
                breakpoint.disposition.label(),
                breakpoint.activity.label()
            ),
            format!(
                "group={} group_enabled={}",
                breakpoint.group.as_deref().unwrap_or("-"),
                breakpoint
                    .group_enabled
                    .map(|enabled| enabled.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "predicate_name={}",
                breakpoint.predicate_name.as_deref().unwrap_or("-")
            ),
            format!("actions={}", breakpoint.actions.join(",")),
            format!("when={}", breakpoint.predicate),
            format!("hits={}", breakpoint.hit_count),
            format!(
                "last_event={}",
                breakpoint
                    .last_hit_event_id
                    .map(|event_id| event_id.raw().to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "last_seq={}",
                breakpoint
                    .last_hit_sequence_no
                    .map(|sequence_no| sequence_no.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
        ];
        if let Some(event) = detail.last_hit_event.as_ref() {
            lines.push(format_event_line(event));
        }
        Ok(CommandOutput::new(
            format!("breakpoint {}", trigger_id.raw()),
            lines,
        ))
    }

    fn list_breakpoint_groups(&mut self) -> SwatResult<CommandOutput> {
        let groups = {
            let api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.breakpoint_groups()
        };
        let mut lines = Vec::new();
        for group in &groups {
            let ids = group
                .breakpoints
                .iter()
                .map(|breakpoint| breakpoint.trigger_id.raw().to_string())
                .collect::<Vec<_>>()
                .join(",");
            lines.push(format!(
                "kind={} group={} count={} ids={}",
                group.kind.label(),
                group.label,
                group.breakpoints.len(),
                ids
            ));
        }
        Ok(CommandOutput::new(
            format!("{} breakpoint group(s)", groups.len()),
            lines,
        ))
    }

    fn list_breakpoint_definition_groups(&mut self) -> SwatResult<CommandOutput> {
        let groups = {
            let api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.breakpoint_definition_groups()
        };
        let mut lines = Vec::new();
        for group in &groups {
            lines.push(format_breakpoint_definition_group(group));
            lines.extend(group.breakpoints.iter().map(format_breakpoint_summary));
        }
        Ok(CommandOutput::new(
            format!("{} definition group(s)", groups.len()),
            lines,
        ))
    }

    fn list_breakpoint_predicates(&mut self) -> SwatResult<CommandOutput> {
        let predicates = {
            let api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.breakpoint_predicates()
        };
        Ok(CommandOutput::new(
            format!("{} breakpoint predicate(s)", predicates.len()),
            predicates
                .iter()
                .map(format_breakpoint_predicate_summary)
                .collect(),
        ))
    }

    fn add_breakpoint_predicate(&mut self, name: &str, expr: &str) -> SwatResult<CommandOutput> {
        let predicate = TriggerPredicate::Expr(parse_expression(expr)?);
        let policy_lines = if let Some(session_id) = self.session_id {
            let report = {
                let mut api = LiveSessionApi::new(
                    &mut self.manager,
                    self.adapter.as_mut(),
                    self.store.as_mut(),
                    &mut self.trigger_engine,
                );
                api.define_breakpoint_predicate(session_id, name, predicate.clone())?
            };
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .collect::<Vec<_>>()
        } else {
            self.trigger_engine
                .define_predicate(name.to_string(), predicate.clone());
            Vec::new()
        };
        self.predicate_specs.insert(
            name.to_string(),
            PersistedPredicateSpec {
                name: name.to_string(),
                expr: expr.to_string(),
            },
        );
        Ok(CommandOutput::new(
            format!("defined breakpoint predicate {name}"),
            policy_lines
                .into_iter()
                .chain(std::iter::once(format!(
                    "predicate={} expr={:?}",
                    name, expr
                )))
                .collect(),
        ))
    }

    fn remove_breakpoint_predicate(&mut self, name: &str) -> SwatResult<CommandOutput> {
        if self
            .trigger_specs
            .values()
            .any(|spec| spec.predicate_ref.as_deref() == Some(name))
        {
            return Err(SwatError::new(format!(
                "cannot remove breakpoint predicate {} while breakpoints still reference it",
                name
            )));
        }
        let policy_lines = if let Some(session_id) = self.session_id {
            let report = {
                let mut api = LiveSessionApi::new(
                    &mut self.manager,
                    self.adapter.as_mut(),
                    self.store.as_mut(),
                    &mut self.trigger_engine,
                );
                api.remove_breakpoint_predicate(session_id, name)?
            };
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .collect::<Vec<_>>()
        } else {
            self.trigger_engine
                .remove_predicate(name)
                .ok_or_else(|| SwatError::new(format!("unknown breakpoint predicate {name}")))?;
            Vec::new()
        };
        self.predicate_specs.remove(name);
        Ok(CommandOutput::new(
            format!("removed breakpoint predicate {name}"),
            policy_lines
                .into_iter()
                .chain(std::iter::once(format!("predicate={name}")))
                .collect(),
        ))
    }

    fn add_trigger(
        &mut self,
        name: &str,
        condition: BreakpointConditionInput,
        fire_once: bool,
        group: Option<&str>,
    ) -> SwatResult<CommandOutput> {
        self.add_trigger_spec(PersistedTriggerSpec {
            name: name.to_string(),
            expr: condition.as_expression().map(ToString::to_string),
            predicate_ref: condition.predicate_ref().map(ToString::to_string),
            watchpoint: None,
            fire_once,
            enabled: true,
            group: group.map(ToString::to_string),
            actions: default_persisted_trigger_actions(),
        })
    }

    fn add_snapshot_trigger(
        &mut self,
        name: &str,
        condition: BreakpointConditionInput,
        reason: &str,
        group: Option<&str>,
    ) -> SwatResult<CommandOutput> {
        self.add_trigger_spec(PersistedTriggerSpec {
            name: name.to_string(),
            expr: condition.as_expression().map(ToString::to_string),
            predicate_ref: condition.predicate_ref().map(ToString::to_string),
            watchpoint: None,
            fire_once: false,
            enabled: true,
            group: group.map(ToString::to_string),
            actions: vec![PersistedTriggerAction::CreateSnapshot {
                reason: reason.to_string(),
            }],
        })
    }

    fn set_breakpoint_group_enabled(
        &mut self,
        group: &str,
        enabled: bool,
    ) -> SwatResult<CommandOutput> {
        let (previous, policy_lines) = if let Some(session_id) = self.session_id {
            let report = {
                let mut api = LiveSessionApi::new(
                    &mut self.manager,
                    self.adapter.as_mut(),
                    self.store.as_mut(),
                    &mut self.trigger_engine,
                );
                api.set_breakpoint_group_enabled(session_id, group, enabled)?
            };
            (
                report.value,
                report
                    .policy_events
                    .iter()
                    .map(format_event_line)
                    .collect::<Vec<_>>(),
            )
        } else {
            let previous = self
                .trigger_engine
                .set_group_enabled(group, enabled)
                .ok_or_else(|| SwatError::new(format!("unknown breakpoint group {group}")))?;
            (previous, Vec::new())
        };
        Ok(CommandOutput::new(
            format!(
                "{} breakpoint group {}",
                if enabled { "enabled" } else { "disabled" },
                group
            ),
            policy_lines
                .into_iter()
                .chain(std::iter::once(format!(
                    "group={} previous_enabled={} enabled={}",
                    group, previous, enabled
                )))
                .collect(),
        ))
    }

    fn list_watchpoints(&mut self) -> SwatResult<CommandOutput> {
        let watchpoints = {
            let api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.watchpoint_summaries()
        };
        Ok(CommandOutput::new(
            format!("{} watchpoint(s)", watchpoints.len()),
            watchpoints.iter().map(format_watchpoint_summary).collect(),
        ))
    }

    fn show_watchpoint(&mut self, trigger_id: TriggerId) -> SwatResult<CommandOutput> {
        let detail = {
            let api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.watchpoint_detail(trigger_id)
        }
        .ok_or_else(|| SwatError::new(format!("unknown watchpoint {}", trigger_id.raw())))?;

        let watchpoint = detail.watchpoint;
        let breakpoint = &watchpoint.breakpoint;
        let mut lines = vec![
            format!(
                "wp={} name={}",
                breakpoint.trigger_id.raw(),
                breakpoint.name
            ),
            format!(
                "state={} configured={} lifetime={} disposition={} activity={}",
                breakpoint.state.label(),
                breakpoint.configured_state.label(),
                breakpoint.lifetime.label(),
                breakpoint.disposition.label(),
                breakpoint.activity.label()
            ),
            format!(
                "group={} group_enabled={}",
                breakpoint.group.as_deref().unwrap_or("-"),
                breakpoint
                    .group_enabled
                    .map(|enabled| enabled.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!("value_key={}", watchpoint.value_key),
            format!("path={}", watchpoint.path.as_deref().unwrap_or("-")),
            format!(
                "after_millis={}",
                watchpoint
                    .after_millis
                    .map(|millis| millis.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "event_kind={}",
                watchpoint
                    .event_kind
                    .map(|kind| format!("{kind:?}"))
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "summary_contains={}",
                watchpoint.summary_contains.as_deref().unwrap_or("-")
            ),
            format!("actions={}", breakpoint.actions.join(",")),
            format!("hits={}", breakpoint.hit_count),
            format!(
                "last_event={}",
                breakpoint
                    .last_hit_event_id
                    .map(|event_id| event_id.raw().to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "last_seq={}",
                breakpoint
                    .last_hit_sequence_no
                    .map(|sequence_no| sequence_no.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
        ];
        if let Some(event) = detail.last_hit_event.as_ref() {
            lines.push(format_event_line(event));
        }
        Ok(CommandOutput::new(
            format!("watchpoint {}", trigger_id.raw()),
            lines,
        ))
    }

    fn add_watchpoint(&mut self, spec: WatchpointSpec) -> SwatResult<CommandOutput> {
        let persisted = persisted_watchpoint_trigger_spec(&spec);
        let trigger_id = if let Some(session_id) = self.session_id {
            let report = {
                let mut api = LiveSessionApi::new(
                    &mut self.manager,
                    self.adapter.as_mut(),
                    self.store.as_mut(),
                    &mut self.trigger_engine,
                );
                api.add_watchpoint(session_id, spec)?
            };
            let trigger_id = report.value;
            self.trigger_specs.insert(trigger_id, persisted.clone());
            return Ok(CommandOutput::new(
                format!("added watchpoint {}", trigger_id.raw()),
                report
                    .policy_events
                    .iter()
                    .map(format_event_line)
                    .chain(std::iter::once(format_watchpoint_spec_line(
                        trigger_id, &persisted,
                    )))
                    .collect(),
            ));
        } else {
            let trigger = build_trigger_from_spec(&persisted)?;
            let trigger_id = trigger.trigger_id;
            self.trigger_engine.add_trigger(trigger);
            trigger_id
        };
        self.trigger_specs.insert(trigger_id, persisted.clone());
        Ok(CommandOutput::new(
            format!("added watchpoint {}", trigger_id.raw()),
            vec![format_watchpoint_spec_line(trigger_id, &persisted)],
        ))
    }

    fn set_watchpoint_enabled(
        &mut self,
        trigger_id: TriggerId,
        enabled: bool,
    ) -> SwatResult<CommandOutput> {
        ensure_watchpoint_spec(self.trigger_specs.get(&trigger_id), trigger_id)?;
        self.set_trigger_enabled(trigger_id, enabled)
    }

    fn remove_watchpoint(&mut self, trigger_id: TriggerId) -> SwatResult<CommandOutput> {
        ensure_watchpoint_spec(self.trigger_specs.get(&trigger_id), trigger_id)?;
        let mut output = self.remove_trigger(trigger_id)?;
        output.summary = output.summary.replacen("trigger", "watchpoint", 1);
        Ok(output)
    }

    fn remove_trigger(&mut self, trigger_id: TriggerId) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let report = {
            let mut api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.remove_trigger(session_id, trigger_id)?
        };
        self.trigger_specs.remove(&trigger_id);
        Ok(CommandOutput::new(
            format!("removed trigger {}", trigger_id.raw()),
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .chain(std::iter::once(format!("name={}", report.value.name)))
                .collect(),
        ))
    }

    fn set_trigger_enabled(
        &mut self,
        trigger_id: TriggerId,
        enabled: bool,
    ) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let report = {
            let mut api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.set_trigger_enabled(session_id, trigger_id, enabled)?
        };
        let previous = report.value;
        if let Some(spec) = self.trigger_specs.get_mut(&trigger_id) {
            spec.enabled = enabled;
        }
        let trigger = self
            .trigger_engine
            .triggers()
            .iter()
            .find(|trigger| trigger.trigger_id == trigger_id)
            .ok_or_else(|| SwatError::new(format!("unknown trigger {}", trigger_id.raw())))?;
        let summary = if enabled {
            format!("enabled trigger {}", trigger_id.raw())
        } else {
            format!("disabled trigger {}", trigger_id.raw())
        };
        Ok(CommandOutput::new(
            summary,
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .chain(std::iter::once(format!(
                    "name={} previous_enabled={} enabled={}",
                    trigger.name, previous, enabled
                )))
                .collect(),
        ))
    }

    fn save_triggers(&self, path: &str) -> SwatResult<CommandOutput> {
        let file = PersistedTriggerFile {
            format_version: TRIGGER_FILE_FORMAT_VERSION,
            predicates: self.predicate_specs.values().cloned().collect(),
            groups: self
                .trigger_engine
                .group_policies()
                .into_iter()
                .map(|group| PersistedTriggerGroupSpec {
                    name: group.name,
                    enabled: group.enabled,
                })
                .collect(),
            triggers: self
                .trigger_engine
                .triggers()
                .iter()
                .filter_map(|trigger| self.trigger_specs.get(&trigger.trigger_id).cloned())
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|err| {
            SwatError::new(format!("failed to encode trigger file '{}': {err}", path))
        })?;
        fs::write(path, bytes)
            .map_err(|err| SwatError::new(format!("failed to write '{}': {err}", path)))?;
        Ok(CommandOutput::new(
            format!("saved {} trigger(s)", file.triggers.len()),
            vec![format!("path={path}")],
        ))
    }

    fn load_triggers(&mut self, path: &str) -> SwatResult<CommandOutput> {
        let bytes = fs::read(path)
            .map_err(|err| SwatError::new(format!("failed to read '{}': {err}", path)))?;
        let file: PersistedTriggerFile = serde_json::from_slice(&bytes).map_err(|err| {
            SwatError::new(format!("failed to decode trigger file '{}': {err}", path))
        })?;
        if file.format_version != LEGACY_TRIGGER_FILE_FORMAT_VERSION
            && file.format_version != PRE_GROUP_TRIGGER_FILE_FORMAT_VERSION
            && file.format_version != PRE_WATCHPOINT_TRIGGER_FILE_FORMAT_VERSION
            && file.format_version != TRIGGER_FILE_FORMAT_VERSION
        {
            return Err(SwatError::new(format!(
                "unsupported trigger file format version {}",
                file.format_version
            )));
        }

        self.trigger_engine = TriggerEngine::new();
        self.trigger_specs.clear();
        self.predicate_specs.clear();

        let mut lines = Vec::new();

        for predicate in &file.predicates {
            if let Some(session_id) = self.session_id {
                let report = {
                    let mut api = LiveSessionApi::new(
                        &mut self.manager,
                        self.adapter.as_mut(),
                        self.store.as_mut(),
                        &mut self.trigger_engine,
                    );
                    api.define_breakpoint_predicate(
                        session_id,
                        &predicate.name,
                        TriggerPredicate::Expr(parse_expression(&predicate.expr)?),
                    )?
                };
                lines.extend(report.policy_events.iter().map(format_event_line));
            } else {
                self.trigger_engine.define_predicate(
                    predicate.name.clone(),
                    TriggerPredicate::Expr(parse_expression(&predicate.expr)?),
                );
            }
            self.predicate_specs
                .insert(predicate.name.clone(), predicate.clone());
            lines.push(format!(
                "predicate={} expr={:?}",
                predicate.name, predicate.expr
            ));
        }

        let mut loaded_count = 0usize;
        for spec in file.triggers {
            let trigger = build_trigger_from_spec(&spec)?;
            let trigger_id = trigger.trigger_id;
            if let Some(session_id) = self.session_id {
                let report = {
                    let mut api = LiveSessionApi::new(
                        &mut self.manager,
                        self.adapter.as_mut(),
                        self.store.as_mut(),
                        &mut self.trigger_engine,
                    );
                    api.add_trigger(session_id, trigger)?
                };
                lines.extend(report.policy_events.iter().map(format_event_line));
            } else {
                self.trigger_engine.add_trigger(trigger);
            }
            self.trigger_specs.insert(trigger_id, spec.clone());
            lines.push(format!(
                "trigger={} name={} fire_once={} enabled={} actions={}{} condition={}",
                trigger_id.raw(),
                spec.name,
                spec.fire_once,
                spec.enabled,
                format_persisted_trigger_actions(&spec.actions),
                spec.group
                    .as_ref()
                    .map(|group| format!(" group={group}"))
                    .unwrap_or_default(),
                format_persisted_trigger_condition(&spec)
            ));
            loaded_count += 1;
        }

        for group in &file.groups {
            if let Some(session_id) = self.session_id {
                let report = {
                    let mut api = LiveSessionApi::new(
                        &mut self.manager,
                        self.adapter.as_mut(),
                        self.store.as_mut(),
                        &mut self.trigger_engine,
                    );
                    api.set_breakpoint_group_enabled(session_id, &group.name, group.enabled)?
                };
                lines.extend(report.policy_events.iter().map(format_event_line));
            } else {
                let _ = self
                    .trigger_engine
                    .set_group_enabled(&group.name, group.enabled);
            }
            lines.push(format!("group={} enabled={}", group.name, group.enabled));
        }

        Ok(CommandOutput::new(
            format!("loaded {} trigger(s)", loaded_count),
            lines,
        ))
    }

    fn until_expr(&mut self, expr: &str) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let session = self
            .manager
            .session(session_id)
            .ok_or_else(|| SwatError::new("active session is missing from the session manager"))?;
        if !session.capabilities.can_resume {
            return Err(SwatError::new(
                "active target cannot resume execution, so 'until' is unavailable",
            ));
        }

        let parsed = parse_expression(expr)?;
        let until_name = format!("until {:?}", expr);
        let until_trigger = Trigger::new(
            until_name,
            swat_control::TriggerPredicate::Expr(parsed),
            vec![TriggerAction::PauseTarget],
        )
        .fire_once();
        let until_trigger_id = until_trigger.trigger_id;

        let mut lines = Vec::new();
        let mut pump_count = 0usize;

        let add_report = {
            let mut api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.add_trigger(session_id, until_trigger)?
        };
        lines.extend(add_report.policy_events.iter().map(format_event_line));

        let resume_report = {
            let mut api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.control(session_id, ControlAction::Resume)?
        };
        lines.extend(resume_report.policy_events.iter().map(format_event_line));
        let resume_report = resume_report.value;
        lines.extend(resume_report.stored_events.iter().map(format_event_line));
        lines.push(format!(
            "control accepted={} summary={}",
            resume_report.response.accepted, resume_report.response.summary
        ));
        if !resume_report.response.accepted {
            let _ = {
                let mut api = LiveSessionApi::new(
                    &mut self.manager,
                    self.adapter.as_mut(),
                    self.store.as_mut(),
                    &mut self.trigger_engine,
                );
                api.remove_trigger(session_id, until_trigger_id)
            };
            return Ok(CommandOutput::new(
                format!(
                    "until could not resume target: {}",
                    resume_report.response.summary
                ),
                lines,
            ));
        }

        let matched = loop {
            let report = pump_with_triggers(
                &mut self.manager,
                session_id,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            )?;
            pump_count += 1;

            let matched = report
                .trigger_matches
                .iter()
                .any(|trigger_match| trigger_match.trigger_id == until_trigger_id);
            let exited = report
                .pump_report
                .stored_events
                .iter()
                .any(event_looks_like_target_exit);
            lines.extend(format_controlled_pump_lines(&report));

            if matched {
                break true;
            }
            if exited {
                break false;
            }

            if report_paused_target(&report) {
                let resume_report = {
                    let mut api = LiveSessionApi::new(
                        &mut self.manager,
                        self.adapter.as_mut(),
                        self.store.as_mut(),
                        &mut self.trigger_engine,
                    );
                    api.control(session_id, ControlAction::Resume)?
                };
                lines.extend(resume_report.policy_events.iter().map(format_event_line));
                let resume_report = resume_report.value;
                lines.extend(resume_report.stored_events.iter().map(format_event_line));
                lines.push(format!(
                    "control accepted={} summary={}",
                    resume_report.response.accepted, resume_report.response.summary
                ));
                if !resume_report.response.accepted {
                    let _ = {
                        let mut api = LiveSessionApi::new(
                            &mut self.manager,
                            self.adapter.as_mut(),
                            self.store.as_mut(),
                            &mut self.trigger_engine,
                        );
                        api.remove_trigger(session_id, until_trigger_id)
                    };
                    return Ok(CommandOutput::new(
                        format!(
                            "until stopped because the target could not resume: {}",
                            resume_report.response.summary
                        ),
                        lines,
                    ));
                }
            }

            if report.pump_report.stored_events.is_empty() {
                thread::sleep(UNTIL_POLL_DELAY);
            }
        };

        let _ = {
            let mut api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.remove_trigger(session_id, until_trigger_id)
        };

        let summary = if matched {
            format!("until matched after {pump_count} pump(s)")
        } else {
            format!("until stopped after target exit without a match after {pump_count} pump(s)")
        };
        Ok(CommandOutput::new(summary, lines))
    }

    fn replay(&mut self, selector_id: u64) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let inspector = self.inspector();
        let snapshot_id = SnapshotId::from_raw(selector_id);
        let (label, plan) = if inspector.snapshot_by_id(snapshot_id).is_some() {
            (
                format!("snapshot {}", snapshot_id.raw()),
                inspector
                    .replay_plan_for_snapshot(snapshot_id)
                    .unwrap_or_default(),
            )
        } else {
            let boundary_id = BoundaryId::from_raw(selector_id);
            (
                format!("boundary {}", boundary_id.raw()),
                inspector.replay_plan_for_boundary(session_id, boundary_id),
            )
        };

        let mut lines = format_replay_plan_lines(&plan);
        if plan.is_empty() {
            return Ok(CommandOutput::new(
                format!("no replay directives available for {label}"),
                lines,
            ));
        }

        let session = self
            .manager
            .session(session_id)
            .ok_or_else(|| SwatError::new("active session is missing from the session manager"))?;
        if !session.capabilities.can_inject_replay {
            return Ok(CommandOutput::new(
                format!("replay preview available for {label}, but target cannot inject replay"),
                lines,
            ));
        }

        let report = {
            let mut api = LiveSessionApi::new(
                &mut self.manager,
                self.adapter.as_mut(),
                self.store.as_mut(),
                &mut self.trigger_engine,
            );
            api.apply_replay_plan(session_id, &plan)?
        };
        lines.splice(0..0, report.policy_events.iter().map(format_event_line));
        lines.extend(report.value.stored_events.iter().map(format_event_line));
        Ok(CommandOutput::new(
            format!(
                "applied {} replay directive(s) from {label}",
                report.value.directives_applied
            ),
            lines,
        ))
    }

    fn list_spans(&self) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let inspector = self.inspector();
        let frames = inspector.stack_frames(session_id)?;
        Ok(CommandOutput::new(
            format!("{} stack frame(s)", frames.len()),
            frames
                .into_iter()
                .map(|frame| format_stack_frame_summary(&frame))
                .collect(),
        ))
    }

    fn show_frame(&self, frame_index: usize) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let inspector = self.inspector();
        let Some(frame) = inspector.stack_frame(session_id, frame_index)? else {
            return Err(SwatError::new(format!("unknown stack frame {frame_index}")));
        };
        let events = inspector.boundary_span(session_id, frame.boundary_id);
        let mut lines = format_stack_frame_detail(&frame);
        lines.push("events:".to_string());
        lines.extend(events.iter().map(format_event_line));
        Ok(CommandOutput::new(
            format!("stack frame {} {}", frame.frame_index, frame.label),
            lines,
        ))
    }

    fn show_span(&self, boundary_id: BoundaryId) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let inspector = self.inspector();
        let events = inspector.boundary_span(session_id, boundary_id);
        let Some(frame) = inspector.stack_frame_by_boundary(session_id, boundary_id)? else {
            return Ok(CommandOutput::new(
                format!(
                    "{} event(s) in boundary span {}",
                    events.len(),
                    boundary_id.raw()
                ),
                events.iter().map(format_event_line).collect(),
            ));
        };
        let mut lines = format_stack_frame_detail(&frame);
        lines.push("events:".to_string());
        lines.extend(events.iter().map(format_event_line));
        Ok(CommandOutput::new(
            format!("stack boundary {} {}", boundary_id.raw(), frame.label),
            lines,
        ))
    }

    fn show_source(
        &self,
        event_id: EventId,
        before: usize,
        after: usize,
    ) -> SwatResult<CommandOutput> {
        let event = self.lookup_event(event_id)?;
        let inspection = self.inspector().source_inspection(&event, before, after)?;
        let Some(location) = inspection.location.clone() else {
            return Ok(CommandOutput::new(
                format!("no source metadata for event {}", event_id.raw()),
                Vec::new(),
            ));
        };

        let mut lines = vec![
            format!("file={}", location.file),
            format!("line={}", location.line),
            format!(
                "function={}",
                location.function.unwrap_or_else(|| "-".to_string())
            ),
        ];

        if let Some(snippet) = inspection.snippet {
            lines.extend(snippet.lines.iter().map(|line| {
                let marker = if line.line_number == snippet.focus_line {
                    '>'
                } else {
                    ' '
                };
                format!("{marker} {:>4} {}", line.line_number, line.text)
            }));
            return Ok(CommandOutput::new(
                format!("source {}:{}", snippet.location.file, snippet.location.line),
                lines,
            ));
        }

        if let Some(failure) = inspection.failure {
            lines.push(format!("failure_kind={:?}", failure.kind));
            lines.push(format!("failure={}", failure.message));
            return Ok(CommandOutput::new(
                format!("source unresolved for event {}", event_id.raw()),
                lines,
            ));
        }

        Ok(CommandOutput::new(
            format!("source metadata for event {}", event_id.raw()),
            lines,
        ))
    }

    fn list_source_file_events(&self, file: &str) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let events = self.inspector().events_for_source_file(session_id, file)?;
        Ok(CommandOutput::new(
            format!("{} event(s) for source file {}", events.len(), file),
            events.iter().map(format_event_line).collect(),
        ))
    }

    fn list_source_files(&self) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let files = self.inspector().source_files(session_id)?;
        Ok(CommandOutput::new(
            format!("{} source file(s)", files.len()),
            files
                .into_iter()
                .map(|file| {
                    let line_range = match (file.first_line, file.last_line) {
                        (Some(first), Some(last)) => format!("{first}..{last}"),
                        _ => "-".to_string(),
                    };
                    let functions = if file.functions.is_empty() {
                        "-".to_string()
                    } else {
                        file.functions.join(",")
                    };
                    format!(
                        "file={} events={} lines={} functions={} real={}",
                        file.file, file.event_count, line_range, functions, file.is_real_path
                    )
                })
                .collect(),
        ))
    }

    fn show_source_file_view(
        &self,
        file: &str,
        line: usize,
        before: usize,
        after: usize,
    ) -> SwatResult<CommandOutput> {
        let snippet = self
            .inspector()
            .source_file_view(file, line, before, after)?;
        let mut lines = vec![
            format!("file={}", snippet.location.file),
            format!("line={}", snippet.location.line),
            format!(
                "function={}",
                snippet
                    .location
                    .function
                    .clone()
                    .unwrap_or_else(|| "-".to_string())
            ),
        ];
        lines.extend(snippet.lines.iter().map(|line| {
            let marker = if line.line_number == snippet.focus_line {
                '>'
            } else {
                ' '
            };
            format!("{marker} {:>4} {}", line.line_number, line.text)
        }));
        Ok(CommandOutput::new(
            format!("source {}:{}", snippet.location.file, snippet.location.line),
            lines,
        ))
    }

    fn run_script(&self, script: &str) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let mut host = ScriptHost::new(self.store.as_ref(), session_id);
        let value = host.eval_dynamic(script)?;
        Ok(CommandOutput::new(
            "script evaluated",
            vec![format!("result={value:?}")],
        ))
    }

    fn require_session(&self) -> SwatResult<SessionId> {
        self.session_id
            .ok_or_else(|| SwatError::new("no active session; run 'attach' first"))
    }

    fn inspector(&self) -> TraceInspector<'_, dyn SwatStore> {
        TraceInspector::new(self.store.as_ref())
    }

    fn lookup_event(&self, event_id: EventId) -> SwatResult<EventEnvelope> {
        self.inspector()
            .event_by_id(event_id)
            .ok_or_else(|| SwatError::new(format!("unknown event {}", event_id.raw())))
    }
}

pub fn parse_command(input: &str) -> SwatResult<Command> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(SwatError::new("command is empty"));
    }
    if trimmed == "help search" {
        return Err(SwatError::new("help search requires a non-empty pattern"));
    }
    if let Some(rest) = trimmed.strip_prefix("help search ") {
        let needle = rest.trim();
        if needle.is_empty() {
            return Err(SwatError::new("help search requires a non-empty pattern"));
        }
        return Ok(Command::HelpSearch {
            needle: needle.to_string(),
        });
    }
    if trimmed == "help" {
        return Ok(Command::Help { topic: None });
    }
    if let Some(rest) = trimmed.strip_prefix("help ") {
        return Ok(Command::Help {
            topic: Some(rest.trim().to_string()),
        });
    }
    if trimmed == "attach" {
        return Ok(Command::Attach);
    }
    if matches!(trimmed, "session" | "status") {
        return Ok(Command::Session);
    }
    if trimmed == "stack" || trimmed == "stack list" {
        return Ok(Command::Spans);
    }
    if let Some(rest) = trimmed.strip_prefix("stack ") {
        return parse_stack_command(rest);
    }
    if matches!(trimmed, "breakpoint" | "breakpoints" | "breakpoint list") {
        return Ok(Command::Breakpoints);
    }
    if let Some(rest) = trimmed.strip_prefix("breakpoint ") {
        return parse_breakpoint_command(rest);
    }
    if matches!(trimmed, "watchpoint" | "watchpoints" | "watchpoint list") {
        return Ok(Command::Watchpoints);
    }
    if let Some(rest) = trimmed.strip_prefix("watchpoint ") {
        return parse_watchpoint_command(rest);
    }
    if trimmed == "pump" {
        return Ok(Command::Pump);
    }
    if trimmed == "pause" {
        return Ok(Command::Pause);
    }
    if trimmed == "resume" {
        return Ok(Command::Resume);
    }
    if trimmed == "step" {
        return Ok(Command::Step);
    }
    if let Some(reason) = trimmed.strip_prefix("snapshot ") {
        return Ok(Command::Snapshot {
            reason: reason.trim().to_string(),
        });
    }
    if trimmed == "snapshots" {
        return Ok(Command::Snapshots);
    }
    if let Some(rest) = trimmed.strip_prefix("snapshot-show ") {
        return Ok(Command::SnapshotShow {
            snapshot_id: SnapshotId::from_raw(parse_u64(rest.trim(), "snapshot id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("events") {
        let rest = rest.trim();
        return Ok(Command::Events {
            kind: if rest.is_empty() {
                None
            } else {
                Some(parse_event_kind(rest)?)
            },
        });
    }
    if let Some(rest) = trimmed.strip_prefix("event ") {
        return Ok(Command::Event {
            event_id: EventId::from_raw(parse_u64(rest.trim(), "event id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("artifacts ") {
        return Ok(Command::Artifacts {
            event_id: EventId::from_raw(parse_u64(rest.trim(), "event id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("artifact-show ") {
        let mut parts = rest.split_whitespace();
        let event_id = parts
            .next()
            .ok_or_else(|| SwatError::new("artifact-show requires an event id"))?;
        let index = parts
            .next()
            .map(|value| parse_usize(value, "artifact index"))
            .transpose()?
            .unwrap_or(0);
        return Ok(Command::ArtifactShow {
            event_id: EventId::from_raw(parse_u64(event_id, "event id")?),
            index,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("query ") {
        return Ok(Command::Query {
            expr: rest.trim().to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("entities ") {
        return Ok(Command::Entities {
            needle: rest.trim().to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("correlation ") {
        return Ok(Command::Correlation {
            correlation_id: rest.trim().to_string(),
        });
    }
    if trimmed == "triggers" {
        return Ok(Command::Triggers);
    }
    if let Some(rest) = trimmed.strip_prefix("trigger-expr-once ") {
        return parse_trigger_expr(rest, true);
    }
    if let Some(rest) = trimmed.strip_prefix("trigger-expr ") {
        return parse_trigger_expr(rest, false);
    }
    if let Some(rest) = trimmed.strip_prefix("trigger-snapshot ") {
        return parse_trigger_snapshot(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("trigger-enable ") {
        return Ok(Command::TriggerEnable {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "trigger id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("trigger-disable ") {
        return Ok(Command::TriggerDisable {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "trigger id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("trigger-save ") {
        return parse_trigger_path(rest, "trigger-save").map(|path| Command::TriggerSave { path });
    }
    if let Some(rest) = trimmed.strip_prefix("trigger-load ") {
        return parse_trigger_path(rest, "trigger-load").map(|path| Command::TriggerLoad { path });
    }
    if let Some(rest) = trimmed.strip_prefix("trigger-remove ") {
        return Ok(Command::TriggerRemove {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "trigger id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("until ") {
        let expr = rest.trim();
        if expr.is_empty() {
            return Err(SwatError::new("until requires a non-empty expression"));
        }
        return Ok(Command::Until {
            expr: expr.to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("replay ") {
        return Ok(Command::Replay {
            selector_id: parse_u64(rest.trim(), "snapshot or boundary id")?,
        });
    }
    if trimmed == "spans" {
        return Ok(Command::Spans);
    }
    if let Some(rest) = trimmed.strip_prefix("span ") {
        return Ok(Command::Span {
            boundary_id: BoundaryId::from_raw(parse_u64(rest.trim(), "boundary id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("source ") {
        let rest = rest.trim();
        if rest == "files" {
            return Ok(Command::SourceFiles);
        }
        if let Some(rest) = rest.strip_prefix("show ") {
            return parse_source_show(rest);
        }
        if let Some(rest) = rest.strip_prefix("view ") {
            return parse_source_view(rest);
        }
        if let Some(rest) = rest.strip_prefix("file ") {
            let file = rest.trim();
            if file.is_empty() {
                return Err(SwatError::new("source file requires a path"));
            }
            return Ok(Command::SourceFile {
                file: file.to_string(),
            });
        }
        return parse_source_show(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("script ") {
        return Ok(Command::Script {
            script: rest.to_string(),
        });
    }

    Err(SwatError::new(format!("unknown command: {trimmed}")))
}

fn parse_stack_command(rest: &str) -> SwatResult<Command> {
    let trimmed = rest.trim();
    if trimmed.is_empty() || trimmed == "list" {
        return Ok(Command::Spans);
    }
    if let Some(rest) = trimmed.strip_prefix("frame ") {
        return Ok(Command::Frame {
            frame_index: parse_usize(rest.trim(), "frame index")?,
        });
    }
    let boundary = trimmed
        .strip_prefix("show ")
        .map(str::trim)
        .unwrap_or(trimmed);
    Ok(Command::Span {
        boundary_id: BoundaryId::from_raw(parse_u64(boundary, "boundary id")?),
    })
}

fn parse_breakpoint_command(rest: &str) -> SwatResult<Command> {
    let trimmed = rest.trim();
    if trimmed.is_empty() || trimmed == "list" {
        return Ok(Command::Breakpoints);
    }
    if trimmed == "groups" {
        return Ok(Command::BreakpointGroups);
    }
    if trimmed == "predicates" {
        return Ok(Command::BreakpointPredicates);
    }
    if trimmed == "group list" {
        return Ok(Command::BreakpointDefinitionGroups);
    }
    if let Some(rest) = trimmed.strip_prefix("show ") {
        return Ok(Command::BreakpointShow {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "breakpoint id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("predicate add ") {
        return parse_breakpoint_predicate_add(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("predicate remove ") {
        let name = rest.trim();
        if name.is_empty() {
            return Err(SwatError::new(
                "breakpoint predicate remove requires a predicate name",
            ));
        }
        return Ok(Command::BreakpointPredicateRemove {
            name: name.to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("group enable ") {
        return Ok(Command::BreakpointGroupEnable {
            group: parse_breakpoint_group_name(rest, "breakpoint group enable")?,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("group disable ") {
        return Ok(Command::BreakpointGroupDisable {
            group: parse_breakpoint_group_name(rest, "breakpoint group disable")?,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("add ") {
        return parse_trigger_expr(rest, false);
    }
    if let Some(rest) = trimmed.strip_prefix("once ") {
        return parse_trigger_expr(rest, true);
    }
    if let Some(rest) = trimmed.strip_prefix("snapshot ") {
        return parse_trigger_snapshot(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("enable ") {
        return Ok(Command::TriggerEnable {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "breakpoint id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("disable ") {
        return Ok(Command::TriggerDisable {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "breakpoint id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("save ") {
        return parse_trigger_path(rest, "breakpoint save")
            .map(|path| Command::TriggerSave { path });
    }
    if let Some(rest) = trimmed.strip_prefix("load ") {
        return parse_trigger_path(rest, "breakpoint load")
            .map(|path| Command::TriggerLoad { path });
    }
    if let Some(rest) = trimmed.strip_prefix("remove ") {
        return Ok(Command::TriggerRemove {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "breakpoint id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("until ") {
        let expr = rest.trim();
        if expr.is_empty() {
            return Err(SwatError::new(
                "breakpoint until requires a non-empty expression",
            ));
        }
        return Ok(Command::Until {
            expr: expr.to_string(),
        });
    }

    Err(SwatError::new(format!(
        "unknown breakpoint command: {trimmed}"
    )))
}

fn parse_watchpoint_command(rest: &str) -> SwatResult<Command> {
    let trimmed = rest.trim();
    if trimmed.is_empty() || trimmed == "list" {
        return Ok(Command::Watchpoints);
    }
    if let Some(rest) = trimmed.strip_prefix("show ") {
        return Ok(Command::WatchpointShow {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "watchpoint id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("add ") {
        return Ok(Command::WatchpointAdd {
            spec: parse_watchpoint_spec(rest, false)?,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("once ") {
        return Ok(Command::WatchpointAdd {
            spec: parse_watchpoint_spec(rest, true)?,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("snapshot ") {
        return Ok(Command::WatchpointAdd {
            spec: parse_watchpoint_snapshot_spec(rest)?,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("enable ") {
        return Ok(Command::WatchpointEnable {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "watchpoint id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("disable ") {
        return Ok(Command::WatchpointDisable {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "watchpoint id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("remove ") {
        return Ok(Command::WatchpointRemove {
            trigger_id: TriggerId::from_raw(parse_u64(rest.trim(), "watchpoint id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("save ") {
        return parse_trigger_path(rest, "watchpoint save")
            .map(|path| Command::TriggerSave { path });
    }
    if let Some(rest) = trimmed.strip_prefix("load ") {
        return parse_trigger_path(rest, "watchpoint load")
            .map(|path| Command::TriggerLoad { path });
    }

    Err(SwatError::new(format!(
        "unknown watchpoint command: {trimmed}"
    )))
}

fn parse_source_show(rest: &str) -> SwatResult<Command> {
    let mut parts = rest.split_whitespace();
    let event_id = parts
        .next()
        .ok_or_else(|| SwatError::new("source requires an event id"))?;
    let before = parts
        .next()
        .map(|value| parse_usize(value, "before context"))
        .transpose()?
        .unwrap_or(2);
    let after = parts
        .next()
        .map(|value| parse_usize(value, "after context"))
        .transpose()?
        .unwrap_or(2);
    Ok(Command::Source {
        event_id: EventId::from_raw(parse_u64(event_id, "event id")?),
        before,
        after,
    })
}

fn parse_source_view(rest: &str) -> SwatResult<Command> {
    let mut parts = rest.split_whitespace();
    let file = parts
        .next()
        .ok_or_else(|| SwatError::new("source view requires a path"))?;
    let line = parts
        .next()
        .map(|value| parse_usize(value, "source line"))
        .transpose()?
        .unwrap_or(1);
    let before = parts
        .next()
        .map(|value| parse_usize(value, "before context"))
        .transpose()?
        .unwrap_or(2);
    let after = parts
        .next()
        .map(|value| parse_usize(value, "after context"))
        .transpose()?
        .unwrap_or(8);
    Ok(Command::SourceView {
        file: file.to_string(),
        line,
        before,
        after,
    })
}

fn parse_trigger_expr(rest: &str, fire_once: bool) -> SwatResult<Command> {
    let trimmed = rest.trim();
    let Some((name, tail)) = trimmed.split_once(char::is_whitespace) else {
        return Err(SwatError::new(
            "trigger-expr requires a name followed by an expression",
        ));
    };
    let (group, condition) = parse_optional_breakpoint_group(tail)?;
    if matches!(&condition, BreakpointConditionInput::Expression(expr) if expr.is_empty()) {
        return Err(SwatError::new(
            "trigger-expr requires a non-empty expression",
        ));
    }
    Ok(Command::TriggerExpr {
        name: name.to_string(),
        condition,
        fire_once,
        group,
    })
}

fn parse_trigger_snapshot(rest: &str) -> SwatResult<Command> {
    let trimmed = rest.trim();
    let Some((name, tail)) = trimmed.split_once(char::is_whitespace) else {
        return Err(SwatError::new(
            "trigger-snapshot requires a name, expression, and reason",
        ));
    };
    let tail = tail.trim();
    if tail.is_empty() {
        return Err(SwatError::new(
            "trigger-snapshot requires a name, expression, and reason",
        ));
    }

    let (group, tail) = split_optional_breakpoint_group(tail);
    let tail = tail.trim();
    if let Some((predicate, reason)) = split_predicate_ref_and_reason(tail) {
        return Ok(Command::TriggerSnapshot {
            name: name.to_string(),
            condition: BreakpointConditionInput::PredicateRef(predicate.to_string()),
            reason: reason.to_string(),
            group,
        });
    }

    for (index, ch) in tail.char_indices().rev() {
        if !ch.is_whitespace() {
            continue;
        }
        let expr = tail[..index].trim_end();
        let reason = tail[index..].trim_start();
        if expr.is_empty() || reason.is_empty() {
            continue;
        }
        if parse_expression(expr).is_ok() {
            return Ok(Command::TriggerSnapshot {
                name: name.to_string(),
                condition: BreakpointConditionInput::Expression(expr.to_string()),
                reason: reason.to_string(),
                group,
            });
        }
    }

    Err(SwatError::new(
        "trigger-snapshot requires a valid expression followed by a reason",
    ))
}

fn parse_trigger_path(rest: &str, command: &str) -> SwatResult<String> {
    let path = rest.trim();
    if path.is_empty() {
        return Err(SwatError::new(format!("{command} requires a file path")));
    }
    Ok(path.to_string())
}

fn parse_breakpoint_group_name(rest: &str, command: &str) -> SwatResult<String> {
    let group = rest.trim();
    if group.is_empty() {
        return Err(SwatError::new(format!("{command} requires a group name")));
    }
    Ok(group.to_string())
}

fn parse_watchpoint_spec(rest: &str, fire_once: bool) -> SwatResult<WatchpointSpec> {
    let trimmed = rest.trim();
    let Some((name, tail)) = trimmed.split_once(char::is_whitespace) else {
        return Err(SwatError::new(
            "watchpoint requires a name followed by a value key",
        ));
    };
    let tail = tail.trim();
    let Some((value_key, options)) = tail
        .split_once(char::is_whitespace)
        .map(|(value_key, options)| (value_key, options.trim()))
        .or_else(|| (!tail.is_empty()).then_some((tail, "")))
    else {
        return Err(SwatError::new(
            "watchpoint requires a name followed by a value key",
        ));
    };
    let mut spec = WatchpointSpec::new(name, value_key);
    if fire_once {
        spec = spec.fire_once();
    }
    apply_watchpoint_options(spec, options)
}

fn parse_watchpoint_snapshot_spec(rest: &str) -> SwatResult<WatchpointSpec> {
    let trimmed = rest.trim();
    let Some((definition, reason)) = trimmed.split_once(" -- ") else {
        return Err(SwatError::new(
            "watchpoint snapshot requires options followed by ' -- <reason>'",
        ));
    };
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(SwatError::new(
            "watchpoint snapshot requires a non-empty reason",
        ));
    }
    Ok(parse_watchpoint_spec(definition, false)?.create_snapshot(reason))
}

fn apply_watchpoint_options(mut spec: WatchpointSpec, options: &str) -> SwatResult<WatchpointSpec> {
    for token in options.split_whitespace() {
        if let Some(group) = token.strip_prefix("group=") {
            if group.is_empty() {
                return Err(SwatError::new("watchpoint group cannot be empty"));
            }
            spec.group = Some(group.to_string());
            continue;
        }
        if let Some(path) = token.strip_prefix("path=") {
            if path.is_empty() {
                return Err(SwatError::new("watchpoint path cannot be empty"));
            }
            spec.path = Some(path.to_string());
            continue;
        }
        if let Some(after) = token.strip_prefix("after=") {
            spec.after_millis = Some(parse_u64(after, "watchpoint after value")?);
            continue;
        }
        if let Some(kind) = token.strip_prefix("kind=") {
            spec.event_kind = Some(parse_event_kind(kind)?);
            continue;
        }
        if let Some(summary) = token.strip_prefix("summary=") {
            if summary.is_empty() {
                return Err(SwatError::new("watchpoint summary cannot be empty"));
            }
            spec.summary_contains = Some(summary.to_string());
            continue;
        }
        return Err(SwatError::new(format!(
            "unknown watchpoint option '{token}'"
        )));
    }
    Ok(spec)
}

fn parse_breakpoint_predicate_add(rest: &str) -> SwatResult<Command> {
    let trimmed = rest.trim();
    let Some((name, expr)) = trimmed.split_once(char::is_whitespace) else {
        return Err(SwatError::new(
            "breakpoint predicate add requires a name followed by an expression",
        ));
    };
    let expr = expr.trim();
    if expr.is_empty() {
        return Err(SwatError::new(
            "breakpoint predicate add requires a non-empty expression",
        ));
    }
    parse_expression(expr)?;
    Ok(Command::BreakpointPredicateAdd {
        name: name.to_string(),
        expr: expr.to_string(),
    })
}

fn parse_optional_breakpoint_group(
    rest: &str,
) -> SwatResult<(Option<String>, BreakpointConditionInput)> {
    let (group, tail) = split_optional_breakpoint_group(rest);
    parse_breakpoint_condition_input(tail).map(|condition| (group, condition))
}

fn split_optional_breakpoint_group(rest: &str) -> (Option<String>, &str) {
    let trimmed = rest.trim();
    let Some((head, tail)) = trimmed.split_once(char::is_whitespace) else {
        if let Some(group) = trimmed.strip_prefix("group=") {
            return (Some(group.to_string()), "");
        }
        return (None, trimmed);
    };
    if let Some(group) = head.strip_prefix("group=") {
        (Some(group.to_string()), tail.trim_start())
    } else {
        (None, trimmed)
    }
}

fn parse_breakpoint_condition_input(rest: &str) -> SwatResult<BreakpointConditionInput> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return Err(SwatError::new(
            "breakpoint condition requires an expression or @predicate reference",
        ));
    }
    if let Some(predicate) = trimmed.strip_prefix('@') {
        if predicate.is_empty() || predicate.contains(char::is_whitespace) {
            return Err(SwatError::new(
                "predicate references must use a single token like @search_tool",
            ));
        }
        return Ok(BreakpointConditionInput::PredicateRef(
            predicate.to_string(),
        ));
    }
    parse_expression(trimmed)?;
    Ok(BreakpointConditionInput::Expression(trimmed.to_string()))
}

fn split_predicate_ref_and_reason(rest: &str) -> Option<(&str, &str)> {
    let (head, tail) = rest.split_once(char::is_whitespace)?;
    let predicate = head.strip_prefix('@')?;
    let reason = tail.trim();
    if predicate.is_empty() || reason.is_empty() {
        None
    } else {
        Some((predicate, reason))
    }
}

fn parse_u64(value: &str, label: &str) -> SwatResult<u64> {
    value
        .parse::<u64>()
        .map_err(|err| SwatError::new(format!("invalid {label} '{value}': {err}")))
}

fn parse_usize(value: &str, label: &str) -> SwatResult<usize> {
    value
        .parse::<usize>()
        .map_err(|err| SwatError::new(format!("invalid {label} '{value}': {err}")))
}

fn parse_event_kind(kind: &str) -> SwatResult<EventKind> {
    match kind {
        "Lifecycle" => Ok(EventKind::Lifecycle),
        "Control" => Ok(EventKind::Control),
        "Execution" => Ok(EventKind::Execution),
        "StateMutation" => Ok(EventKind::StateMutation),
        "ValueObserved" => Ok(EventKind::ValueObserved),
        "TriggerHit" => Ok(EventKind::TriggerHit),
        "Snapshot" => Ok(EventKind::Snapshot),
        "Replay" => Ok(EventKind::Replay),
        "ModelBoundary" => Ok(EventKind::ModelBoundary),
        "ToolBoundary" => Ok(EventKind::ToolBoundary),
        "SourceResolution" => Ok(EventKind::SourceResolution),
        "SchemaResolution" => Ok(EventKind::SchemaResolution),
        "PolicyDecision" => Ok(EventKind::PolicyDecision),
        _ => Err(SwatError::new(format!("unknown event kind '{kind}'"))),
    }
}

fn format_event_line(event: &EventEnvelope) -> String {
    format!(
        "event={} seq={} kind={:?} summary={}",
        event.event_id.raw(),
        event.sequence_no,
        event.kind,
        payload_summary(event).unwrap_or("<none>")
    )
}

fn format_trigger_match(trigger_match: &TriggerMatch) -> String {
    let mut parts = vec![
        format!("trigger={}", trigger_match.trigger_id.raw()),
        format!("name={}", trigger_match.trigger_name),
        format!("event={}", trigger_match.event_id.raw()),
        format!("seq={}", trigger_match.sequence_no),
    ];
    if let Some(group) = &trigger_match.group {
        parts.push(format!("group={group}"));
    }
    if let Some(predicate_name) = &trigger_match.predicate_name {
        parts.push(format!("predicate={predicate_name}"));
    }
    parts.join(" ")
}

fn format_controlled_pump_lines(report: &swat_control::ControlledPumpReport) -> Vec<String> {
    let mut lines = report
        .pump_report
        .stored_events
        .iter()
        .map(format_event_line)
        .collect::<Vec<_>>();
    lines.extend(report.trigger_events.iter().map(format_event_line));
    lines.extend(report.trigger_matches.iter().map(format_trigger_match));
    lines.extend(report.stop_reasons.iter().map(format_stop_reason));
    for control_report in &report.control_reports {
        lines.extend(control_report.stored_events.iter().map(format_event_line));
        lines.push(format!(
            "control accepted={} summary={}",
            control_report.response.accepted, control_report.response.summary
        ));
    }
    lines
}

fn format_breakpoint_summary(breakpoint: &BreakpointSummary) -> String {
    format!(
        "bp={} state={} configured={} lifetime={} disposition={} hits={} last_event={} last_seq={} name={}{} predicate={} actions={} when={}",
        breakpoint.trigger_id.raw(),
        breakpoint.state.label(),
        breakpoint.configured_state.label(),
        breakpoint.lifetime.label(),
        breakpoint.disposition.label(),
        breakpoint.hit_count,
        breakpoint
            .last_hit_event_id
            .map(|event_id| event_id.raw().to_string())
            .unwrap_or_else(|| "-".to_string()),
        breakpoint
            .last_hit_sequence_no
            .map(|sequence_no| sequence_no.to_string())
            .unwrap_or_else(|| "-".to_string()),
        breakpoint.name,
        breakpoint
            .group
            .as_ref()
            .map(|group| format!(" group={group}"))
            .unwrap_or_default(),
        breakpoint.predicate_name.as_deref().unwrap_or("inline"),
        breakpoint.actions.join(","),
        breakpoint.predicate
    )
}

fn format_watchpoint_summary(watchpoint: &WatchpointSummary) -> String {
    let breakpoint = &watchpoint.breakpoint;
    format!(
        "wp={} state={} configured={} lifetime={} disposition={} hits={} last_event={} last_seq={} name={}{} value_key={} path={} after={} kind={} summary={} actions={}",
        breakpoint.trigger_id.raw(),
        breakpoint.state.label(),
        breakpoint.configured_state.label(),
        breakpoint.lifetime.label(),
        breakpoint.disposition.label(),
        breakpoint.hit_count,
        breakpoint
            .last_hit_event_id
            .map(|event_id| event_id.raw().to_string())
            .unwrap_or_else(|| "-".to_string()),
        breakpoint
            .last_hit_sequence_no
            .map(|sequence_no| sequence_no.to_string())
            .unwrap_or_else(|| "-".to_string()),
        breakpoint.name,
        breakpoint
            .group
            .as_ref()
            .map(|group| format!(" group={group}"))
            .unwrap_or_default(),
        watchpoint.value_key,
        watchpoint.path.as_deref().unwrap_or("-"),
        watchpoint
            .after_millis
            .map(|millis| millis.to_string())
            .unwrap_or_else(|| "-".to_string()),
        watchpoint
            .event_kind
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_else(|| "-".to_string()),
        watchpoint.summary_contains.as_deref().unwrap_or("-"),
        breakpoint.actions.join(","),
    )
}

fn format_watchpoint_spec_line(trigger_id: TriggerId, spec: &PersistedTriggerSpec) -> String {
    let watchpoint = spec.watchpoint.as_ref();
    format!(
        "watchpoint={} name={} fire_once={} enabled={} value_key={} path={} after={} kind={} summary={} actions={}{} condition={}",
        trigger_id.raw(),
        spec.name,
        spec.fire_once,
        spec.enabled,
        watchpoint
            .map(|watchpoint| watchpoint.value_key.as_str())
            .unwrap_or("-"),
        watchpoint
            .and_then(|watchpoint| watchpoint.path.as_deref())
            .unwrap_or("-"),
        watchpoint
            .and_then(|watchpoint| watchpoint.after_millis)
            .map(|millis| millis.to_string())
            .unwrap_or_else(|| "-".to_string()),
        watchpoint
            .and_then(|watchpoint| watchpoint.event_kind)
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_else(|| "-".to_string()),
        watchpoint
            .and_then(|watchpoint| watchpoint.summary_contains.as_deref())
            .unwrap_or("-"),
        format_persisted_trigger_actions(&spec.actions),
        spec.group
            .as_ref()
            .map(|group| format!(" group={group}"))
            .unwrap_or_default(),
        format_persisted_trigger_condition(spec),
    )
}

fn format_breakpoint_definition_group(group: &BreakpointDefinitionGroup) -> String {
    format!(
        "definition_group={} enabled={} count={}",
        group.name,
        group.enabled,
        group.breakpoints.len()
    )
}

fn format_breakpoint_predicate_summary(predicate: &BreakpointPredicateSummary) -> String {
    format!(
        "predicate={} breakpoints={} when={}",
        predicate.name, predicate.breakpoint_count, predicate.predicate
    )
}

fn format_stop_reason(stop_reason: &StopReason) -> String {
    match stop_reason.kind {
        StopReasonKind::Breakpoint => format!(
            "stop kind=breakpoint trigger={} name={}{} event={} seq={} summary={}",
            stop_reason
                .trigger_id
                .map(|trigger_id| trigger_id.raw().to_string())
                .unwrap_or_else(|| "-".to_string()),
            stop_reason.trigger_name.as_deref().unwrap_or("-"),
            stop_reason
                .group
                .as_ref()
                .map(|group| format!(" group={group}"))
                .unwrap_or_default(),
            stop_reason
                .event_id
                .map(|event_id| event_id.raw().to_string())
                .unwrap_or_else(|| "-".to_string()),
            stop_reason
                .sequence_no
                .map(|sequence_no| sequence_no.to_string())
                .unwrap_or_else(|| "-".to_string()),
            stop_reason.summary
        ),
        StopReasonKind::TargetExit => format!(
            "stop kind=target_exit event={} seq={} summary={}",
            stop_reason
                .event_id
                .map(|event_id| event_id.raw().to_string())
                .unwrap_or_else(|| "-".to_string()),
            stop_reason
                .sequence_no
                .map(|sequence_no| sequence_no.to_string())
                .unwrap_or_else(|| "-".to_string()),
            stop_reason.summary
        ),
        StopReasonKind::ControlRejected => format!(
            "stop kind=control_rejected action={} summary={}",
            stop_reason
                .control_action
                .as_ref()
                .map(|action| format!("{action:?}"))
                .unwrap_or_else(|| "-".to_string()),
            stop_reason.summary
        ),
    }
}

fn format_replay_plan_lines(plan: &swat_replay::ReplayPlan) -> Vec<String> {
    plan.directives()
        .map(|directive| {
            format!(
                "boundary={} artifact={}",
                directive.boundary_id.raw(),
                directive.artifact_ref.artifact_id.raw()
            )
        })
        .collect()
}

fn format_stack_frame_summary(frame: &StackFrame) -> String {
    let mut parts = vec![
        format!("frame={}", frame.frame_index),
        format!("boundary={}", frame.boundary_id.raw()),
        format!("depth={}", frame.depth),
        format!("kind={:?}", frame.event_kind),
        format!("events={}", frame.event_ids.len()),
        format!("seq={}..{}", frame.sequence_start, frame.sequence_end),
        format!("label={}", frame.label),
    ];
    if let Some(span_id) = &frame.span_id {
        parts.push(format!("span={span_id}"));
    }
    if let Some(correlation_id) = &frame.correlation_id {
        parts.push(format!("correlation={correlation_id}"));
    }
    if let Some(source) = short_stack_frame_source(frame) {
        parts.push(format!("source={source}"));
    }
    parts.push(format!("latest={}", frame.latest_summary));
    parts.join(" ")
}

fn format_stack_frame_detail(frame: &StackFrame) -> Vec<String> {
    let mut lines = vec![
        format!("frame={}", frame.frame_index),
        format!("boundary={}", frame.boundary_id.raw()),
        format!("depth={}", frame.depth),
        format!("kind={:?}", frame.event_kind),
        format!("label={}", frame.label),
        format!("events={}", frame.event_ids.len()),
        format!("seq={}..{}", frame.sequence_start, frame.sequence_end),
        format!("entry={}", frame.entry_summary),
        format!("latest={}", frame.latest_summary),
    ];
    lines.push(format!(
        "correlation={}",
        frame.correlation_id.as_deref().unwrap_or("-")
    ));
    lines.push(format!("span={}", frame.span_id.as_deref().unwrap_or("-")));
    lines.push(format!(
        "function={}",
        frame.function.as_deref().unwrap_or("-")
    ));
    lines.push(format!(
        "source={}",
        short_stack_frame_source(frame).unwrap_or_else(|| "-".to_string())
    ));
    lines
}

fn short_stack_frame_source(frame: &StackFrame) -> Option<String> {
    let file = frame.source_file.as_deref()?;
    match frame.source_line {
        Some(line) => Some(format!("{file}:{line}")),
        None => Some(file.to_string()),
    }
}

fn event_looks_like_target_exit(event: &EventEnvelope) -> bool {
    matches!(
        &event.payload,
        EventPayload::Text { summary }
            if event.kind == EventKind::Lifecycle && summary.contains("exited")
    )
}

fn report_paused_target(report: &swat_control::ControlledPumpReport) -> bool {
    report.control_reports.iter().any(|control_report| {
        control_report.stored_events.iter().any(|event| {
            matches!(
                &event.payload,
                EventPayload::Control {
                    action: ControlAction::Pause,
                    ..
                }
            )
        })
    })
}

const LEGACY_TRIGGER_FILE_FORMAT_VERSION: u32 = 1;
const PRE_GROUP_TRIGGER_FILE_FORMAT_VERSION: u32 = 2;
const PRE_WATCHPOINT_TRIGGER_FILE_FORMAT_VERSION: u32 = 3;
const TRIGGER_FILE_FORMAT_VERSION: u32 = 4;
const UNTIL_POLL_DELAY: Duration = Duration::from_millis(10);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedTriggerFile {
    format_version: u32,
    #[serde(default)]
    predicates: Vec<PersistedPredicateSpec>,
    #[serde(default)]
    groups: Vec<PersistedTriggerGroupSpec>,
    triggers: Vec<PersistedTriggerSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedPredicateSpec {
    name: String,
    expr: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedTriggerGroupSpec {
    name: String,
    #[serde(default = "default_trigger_enabled")]
    enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PersistedTriggerAction {
    PauseTarget,
    CreateSnapshot { reason: String },
}

impl PersistedTriggerAction {
    fn to_runtime_action(&self) -> TriggerAction {
        match self {
            Self::PauseTarget => TriggerAction::PauseTarget,
            Self::CreateSnapshot { reason } => TriggerAction::CreateSnapshot {
                reason: reason.clone(),
            },
        }
    }

    fn from_runtime_action(action: &TriggerAction) -> Self {
        match action {
            TriggerAction::PauseTarget => Self::PauseTarget,
            TriggerAction::CreateSnapshot { reason } => Self::CreateSnapshot {
                reason: reason.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedTriggerSpec {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    predicate_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    watchpoint: Option<PersistedWatchpointSpec>,
    fire_once: bool,
    #[serde(default = "default_trigger_enabled")]
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    #[serde(default = "default_persisted_trigger_actions")]
    actions: Vec<PersistedTriggerAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedWatchpointSpec {
    value_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_kind: Option<EventKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary_contains: Option<String>,
}

fn default_trigger_enabled() -> bool {
    true
}

fn default_persisted_trigger_actions() -> Vec<PersistedTriggerAction> {
    vec![PersistedTriggerAction::PauseTarget]
}

fn build_trigger_from_spec(spec: &PersistedTriggerSpec) -> SwatResult<Trigger> {
    let predicate = build_trigger_predicate(spec)?;
    let mut trigger = Trigger::new(
        &spec.name,
        predicate,
        spec.actions
            .iter()
            .map(PersistedTriggerAction::to_runtime_action)
            .collect(),
    );
    if let Some(group) = spec.group.as_deref() {
        trigger = trigger.in_group(group);
    }
    if spec.fire_once {
        trigger = trigger.fire_once();
    }
    if !spec.enabled {
        trigger = trigger.disabled();
    }
    Ok(trigger)
}

fn format_persisted_trigger_actions(actions: &[PersistedTriggerAction]) -> String {
    actions
        .iter()
        .map(|action| match action {
            PersistedTriggerAction::PauseTarget => "PauseTarget".to_string(),
            PersistedTriggerAction::CreateSnapshot { reason } => {
                format!("CreateSnapshot({reason:?})")
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn build_trigger_predicate(spec: &PersistedTriggerSpec) -> SwatResult<TriggerPredicate> {
    if let Some(predicate_ref) = spec.predicate_ref.as_deref() {
        return Ok(TriggerPredicate::Named(predicate_ref.to_string()));
    }
    if let Some(watchpoint) = spec.watchpoint.as_ref() {
        let mut predicates = vec![TriggerPredicate::ValueChanged {
            value_key: watchpoint.value_key.clone(),
            path: watchpoint.path.clone(),
        }];
        if let Some(after_millis) = watchpoint.after_millis {
            predicates.push(TriggerPredicate::ObservedAfter {
                millis: after_millis,
            });
        }
        if let Some(event_kind) = watchpoint.event_kind {
            predicates.push(TriggerPredicate::EventKindIs(event_kind));
        }
        if let Some(summary_contains) = watchpoint.summary_contains.as_ref() {
            predicates.push(TriggerPredicate::SummaryContains(summary_contains.clone()));
        }
        return Ok(if predicates.len() == 1 {
            predicates.remove(0)
        } else {
            TriggerPredicate::All(predicates)
        });
    }
    let expr = spec
        .expr
        .as_deref()
        .ok_or_else(|| SwatError::new(format!("trigger {} is missing a condition", spec.name)))?;
    Ok(TriggerPredicate::Expr(parse_expression(expr)?))
}

fn format_persisted_trigger_condition(spec: &PersistedTriggerSpec) -> String {
    spec.predicate_ref
        .as_ref()
        .map(|name| format!("@{name}"))
        .or_else(|| {
            spec.watchpoint
                .as_ref()
                .map(format_persisted_watchpoint_condition)
        })
        .or_else(|| spec.expr.clone())
        .unwrap_or_else(|| "<missing>".to_string())
}

fn format_persisted_watchpoint_condition(watchpoint: &PersistedWatchpointSpec) -> String {
    let mut parts = vec![format!("watch {}", watchpoint.value_key)];
    if let Some(path) = watchpoint.path.as_deref() {
        parts.push(format!("path={path}"));
    }
    if let Some(after_millis) = watchpoint.after_millis {
        parts.push(format!("after={after_millis}"));
    }
    if let Some(event_kind) = watchpoint.event_kind {
        parts.push(format!("kind={event_kind:?}"));
    }
    if let Some(summary_contains) = watchpoint.summary_contains.as_deref() {
        parts.push(format!("summary={summary_contains}"));
    }
    parts.join(" ")
}

fn persisted_watchpoint_trigger_spec(spec: &WatchpointSpec) -> PersistedTriggerSpec {
    PersistedTriggerSpec {
        name: spec.name.clone(),
        expr: None,
        predicate_ref: None,
        watchpoint: Some(PersistedWatchpointSpec {
            value_key: spec.value_key.clone(),
            path: spec.path.clone(),
            after_millis: spec.after_millis,
            event_kind: spec.event_kind,
            summary_contains: spec.summary_contains.clone(),
        }),
        fire_once: spec.fire_once,
        enabled: true,
        group: spec.group.clone(),
        actions: spec
            .snapshot_reason
            .as_ref()
            .map(|reason| {
                vec![PersistedTriggerAction::CreateSnapshot {
                    reason: reason.clone(),
                }]
            })
            .unwrap_or_else(default_persisted_trigger_actions),
    }
}

fn ensure_watchpoint_spec(
    spec: Option<&PersistedTriggerSpec>,
    trigger_id: TriggerId,
) -> SwatResult<()> {
    match spec {
        Some(spec) if spec.watchpoint.is_some() => Ok(()),
        Some(_) | None => Err(SwatError::new(format!(
            "unknown watchpoint {}",
            trigger_id.raw()
        ))),
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

impl CommandHost {
    fn add_trigger_spec(&mut self, spec: PersistedTriggerSpec) -> SwatResult<CommandOutput> {
        if let Some(predicate_ref) = spec.predicate_ref.as_deref() {
            if self.trigger_engine.predicate(predicate_ref).is_none()
                && !self.predicate_specs.contains_key(predicate_ref)
            {
                return Err(SwatError::new(format!(
                    "unknown breakpoint predicate {}",
                    predicate_ref
                )));
            }
        }
        let trigger = build_trigger_from_spec(&spec)?;
        let trigger_id = trigger.trigger_id;
        let actions = format_persisted_trigger_actions(&spec.actions);
        let policy_lines = if let Some(session_id) = self.session_id {
            let report = {
                let mut api = LiveSessionApi::new(
                    &mut self.manager,
                    self.adapter.as_mut(),
                    self.store.as_mut(),
                    &mut self.trigger_engine,
                );
                api.add_trigger(session_id, trigger)?
            };
            debug_assert_eq!(report.value, trigger_id);
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .collect::<Vec<_>>()
        } else {
            self.trigger_engine.add_trigger(trigger);
            Vec::new()
        };
        self.trigger_specs.insert(trigger_id, spec.clone());
        Ok(CommandOutput::new(
            format!("added trigger {}", trigger_id.raw()),
            policy_lines
                .into_iter()
                .chain(std::iter::once(format!(
                    "trigger={} name={} fire_once={} enabled={} actions={}{} condition={}",
                    trigger_id.raw(),
                    spec.name,
                    spec.fire_once,
                    spec.enabled,
                    actions,
                    spec.group
                        .as_ref()
                        .map(|group| format!(" group={group}"))
                        .unwrap_or_default(),
                    format_persisted_trigger_condition(&spec)
                )))
                .collect(),
        ))
    }
}
