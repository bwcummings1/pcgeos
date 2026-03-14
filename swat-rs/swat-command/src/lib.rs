#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};
use swat_api::TraceInspector;
use swat_control::{Trigger, TriggerAction, TriggerEngine, TriggerMatch, pump_with_triggers};
use swat_core::{
    BoundaryId, ControlAction, EventEnvelope, EventId, EventKind, EventPayload, SessionId,
    SwatError, SwatResult, TargetAdapter, TriggerId,
};
use swat_expr::parse_expression;
use swat_script::ScriptHost;
use swat_session::SessionManager;
use swat_store::SwatStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Attach,
    Session,
    Pump,
    Pause,
    Resume,
    Step,
    Snapshot {
        reason: String,
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
    Query {
        expr: String,
    },
    Entities {
        needle: String,
    },
    Correlation {
        correlation_id: String,
    },
    Triggers,
    TriggerExpr {
        name: String,
        expr: String,
        fire_once: bool,
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
    Spans,
    Span {
        boundary_id: BoundaryId,
    },
    Source {
        event_id: EventId,
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
    fn new(summary: impl Into<String>, lines: Vec<String>) -> Self {
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
            Command::Help => Ok(CommandOutput::new(
                "available commands",
                vec![
                    "attach".to_string(),
                    "session".to_string(),
                    "pump".to_string(),
                    "pause | resume | step".to_string(),
                    "snapshot <reason>".to_string(),
                    "events [EventKind]".to_string(),
                    "event <event_id>".to_string(),
                    "artifacts <event_id>".to_string(),
                    "query <expr>".to_string(),
                    "entities <needle>".to_string(),
                    "correlation <id>".to_string(),
                    "triggers".to_string(),
                    "trigger-expr <name> <expr>".to_string(),
                    "trigger-expr-once <name> <expr>".to_string(),
                    "trigger-save <path>".to_string(),
                    "trigger-load <path>".to_string(),
                    "trigger-remove <trigger_id>".to_string(),
                    "spans".to_string(),
                    "span <boundary_id>".to_string(),
                    "source <event_id> [before] [after]".to_string(),
                    "script <rhai>".to_string(),
                ],
            )),
            Command::Attach => self.attach_or_describe(),
            Command::Session => self.describe_session(),
            Command::Pump => self.pump_once(),
            Command::Pause => self.control(ControlAction::Pause),
            Command::Resume => self.control(ControlAction::Resume),
            Command::Step => self.control(ControlAction::Step),
            Command::Snapshot { reason } => self.control(ControlAction::CreateSnapshot { reason }),
            Command::Events { kind } => self.list_events(kind),
            Command::Event { event_id } => self.show_event(event_id),
            Command::Artifacts { event_id } => self.show_artifacts(event_id),
            Command::Query { expr } => self.query_events(&expr),
            Command::Entities { needle } => self.list_entities(&needle),
            Command::Correlation { correlation_id } => self.list_correlation(&correlation_id),
            Command::Triggers => Ok(self.list_triggers()),
            Command::TriggerExpr {
                name,
                expr,
                fire_once,
            } => self.add_trigger(&name, &expr, fire_once),
            Command::TriggerSave { path } => self.save_triggers(&path),
            Command::TriggerLoad { path } => self.load_triggers(&path),
            Command::TriggerRemove { trigger_id } => self.remove_trigger(trigger_id),
            Command::Spans => self.list_spans(),
            Command::Span { boundary_id } => self.show_span(boundary_id),
            Command::Source {
                event_id,
                before,
                after,
            } => self.show_source(event_id, before, after),
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
        let mut lines = report
            .pump_report
            .stored_events
            .iter()
            .map(format_event_line)
            .collect::<Vec<_>>();
        lines.extend(report.trigger_events.iter().map(format_event_line));
        lines.extend(report.trigger_matches.iter().map(format_trigger_match));
        for control_report in &report.control_reports {
            lines.extend(control_report.stored_events.iter().map(format_event_line));
            lines.push(format!(
                "control accepted={} summary={}",
                control_report.response.accepted, control_report.response.summary
            ));
        }
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
        let report = self.manager.control(
            session_id,
            self.adapter.as_mut(),
            action,
            self.store.as_mut(),
        )?;
        Ok(CommandOutput::new(
            report.response.summary,
            report.stored_events.iter().map(format_event_line).collect(),
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
        let decoded = self.inspector().decoded_artifacts(&event)?;
        let lines = decoded
            .into_iter()
            .map(|value| {
                format!(
                    "artifact={} kind={:?} preview={}",
                    value.artifact_ref.artifact_id.raw(),
                    value.kind,
                    value.preview(200)
                )
            })
            .collect::<Vec<_>>();
        Ok(CommandOutput::new(
            format!("{} artifact(s) for event {}", lines.len(), event_id.raw()),
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
        let entities = self.inspector().find_entities(session_id, needle)?;
        Ok(CommandOutput::new(
            format!("{} entity match(es)", entities.len()),
            entities
                .into_iter()
                .map(|entity| {
                    format!(
                        "kind={:?} name={} events={}",
                        entity.entity.kind,
                        entity.entity.name,
                        entity.event_ids.len()
                    )
                })
                .collect(),
        ))
    }

    fn list_correlation(&self, correlation_id: &str) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let events = self
            .inspector()
            .events_for_correlation(session_id, correlation_id);
        Ok(CommandOutput::new(
            format!(
                "{} event(s) for correlation {}",
                events.len(),
                correlation_id
            ),
            events.iter().map(format_event_line).collect(),
        ))
    }

    fn list_triggers(&self) -> CommandOutput {
        CommandOutput::new(
            format!("{} trigger(s)", self.trigger_engine.triggers().len()),
            self.trigger_engine
                .triggers()
                .iter()
                .map(|trigger| {
                    let expr = self
                        .trigger_specs
                        .get(&trigger.trigger_id)
                        .map(|spec| format!(" expr={:?}", spec.expr))
                        .unwrap_or_default();
                    format!(
                        "trigger={} name={} fire_once={} enabled={} actions={}{}",
                        trigger.trigger_id.raw(),
                        trigger.name,
                        trigger.fire_once,
                        trigger.enabled,
                        trigger.actions.len(),
                        expr
                    )
                })
                .collect(),
        )
    }

    fn add_trigger(
        &mut self,
        name: &str,
        expr: &str,
        fire_once: bool,
    ) -> SwatResult<CommandOutput> {
        let parsed = parse_expression(expr)?;
        let mut trigger = Trigger::new(
            name,
            swat_control::TriggerPredicate::Expr(parsed),
            vec![TriggerAction::PauseTarget],
        );
        if fire_once {
            trigger = trigger.fire_once();
        }
        let trigger_id = trigger.trigger_id;
        self.trigger_engine.add_trigger(trigger);
        self.trigger_specs.insert(
            trigger_id,
            PersistedTriggerSpec {
                name: name.to_string(),
                expr: expr.to_string(),
                fire_once,
            },
        );
        Ok(CommandOutput::new(
            format!("added trigger {}", trigger_id.raw()),
            vec![format!(
                "trigger={} name={} fire_once={} action=PauseTarget",
                trigger_id.raw(),
                name,
                fire_once
            )],
        ))
    }

    fn remove_trigger(&mut self, trigger_id: TriggerId) -> SwatResult<CommandOutput> {
        let removed = self
            .trigger_engine
            .remove_trigger(trigger_id)
            .ok_or_else(|| SwatError::new(format!("unknown trigger {}", trigger_id.raw())))?;
        self.trigger_specs.remove(&trigger_id);
        Ok(CommandOutput::new(
            format!("removed trigger {}", trigger_id.raw()),
            vec![format!("name={}", removed.name)],
        ))
    }

    fn save_triggers(&self, path: &str) -> SwatResult<CommandOutput> {
        let file = PersistedTriggerFile {
            format_version: TRIGGER_FILE_FORMAT_VERSION,
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
        if file.format_version != TRIGGER_FILE_FORMAT_VERSION {
            return Err(SwatError::new(format!(
                "unsupported trigger file format version {}",
                file.format_version
            )));
        }

        self.trigger_engine = TriggerEngine::new();
        self.trigger_specs.clear();

        let mut lines = Vec::new();
        for spec in file.triggers {
            let parsed = parse_expression(&spec.expr)?;
            let mut trigger = Trigger::new(
                &spec.name,
                swat_control::TriggerPredicate::Expr(parsed),
                vec![TriggerAction::PauseTarget],
            );
            if spec.fire_once {
                trigger = trigger.fire_once();
            }
            let trigger_id = trigger.trigger_id;
            self.trigger_engine.add_trigger(trigger);
            self.trigger_specs.insert(trigger_id, spec.clone());
            lines.push(format!(
                "trigger={} name={} fire_once={} expr={:?}",
                trigger_id.raw(),
                spec.name,
                spec.fire_once,
                spec.expr
            ));
        }

        Ok(CommandOutput::new(
            format!("loaded {} trigger(s)", lines.len()),
            lines,
        ))
    }

    fn list_spans(&self) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let index = self.inspector().entity_index(session_id)?;
        Ok(CommandOutput::new(
            format!("{} boundary span(s)", index.boundary_spans.len()),
            index
                .boundary_spans
                .into_iter()
                .map(|span| {
                    format!(
                        "boundary={} events={}",
                        span.boundary_id.raw(),
                        span.event_ids.len()
                    )
                })
                .collect(),
        ))
    }

    fn show_span(&self, boundary_id: BoundaryId) -> SwatResult<CommandOutput> {
        let session_id = self.require_session()?;
        let events = self.inspector().boundary_span(session_id, boundary_id);
        Ok(CommandOutput::new(
            format!(
                "{} event(s) in boundary span {}",
                events.len(),
                boundary_id.raw()
            ),
            events.iter().map(format_event_line).collect(),
        ))
    }

    fn show_source(
        &self,
        event_id: EventId,
        before: usize,
        after: usize,
    ) -> SwatResult<CommandOutput> {
        let event = self.lookup_event(event_id)?;
        let snippet = self
            .inspector()
            .resolve_source(&event, before, after)?
            .ok_or_else(|| {
                SwatError::new(format!(
                    "no source snippet available for event {}",
                    event_id.raw()
                ))
            })?;
        let lines = snippet
            .lines
            .iter()
            .map(|line| {
                let marker = if line.line_number == snippet.focus_line {
                    '>'
                } else {
                    ' '
                };
                format!("{marker} {:>4} {}", line.line_number, line.text)
            })
            .collect::<Vec<_>>();
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
    if trimmed == "help" {
        return Ok(Command::Help);
    }
    if trimmed == "attach" {
        return Ok(Command::Attach);
    }
    if trimmed == "session" {
        return Ok(Command::Session);
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
    if trimmed == "spans" {
        return Ok(Command::Spans);
    }
    if let Some(rest) = trimmed.strip_prefix("span ") {
        return Ok(Command::Span {
            boundary_id: BoundaryId::from_raw(parse_u64(rest.trim(), "boundary id")?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("source ") {
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
        return Ok(Command::Source {
            event_id: EventId::from_raw(parse_u64(event_id, "event id")?),
            before,
            after,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("script ") {
        return Ok(Command::Script {
            script: rest.to_string(),
        });
    }

    Err(SwatError::new(format!("unknown command: {trimmed}")))
}

fn parse_trigger_expr(rest: &str, fire_once: bool) -> SwatResult<Command> {
    let trimmed = rest.trim();
    let Some((name, expr)) = trimmed.split_once(char::is_whitespace) else {
        return Err(SwatError::new(
            "trigger-expr requires a name followed by an expression",
        ));
    };
    let expr = expr.trim();
    if expr.is_empty() {
        return Err(SwatError::new(
            "trigger-expr requires a non-empty expression",
        ));
    }
    Ok(Command::TriggerExpr {
        name: name.to_string(),
        expr: expr.to_string(),
        fire_once,
    })
}

fn parse_trigger_path(rest: &str, command: &str) -> SwatResult<String> {
    let path = rest.trim();
    if path.is_empty() {
        return Err(SwatError::new(format!("{command} requires a file path")));
    }
    Ok(path.to_string())
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
    format!(
        "trigger={} name={} event={}",
        trigger_match.trigger_id.raw(),
        trigger_match.trigger_name,
        trigger_match.event_id.raw()
    )
}

const TRIGGER_FILE_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedTriggerFile {
    format_version: u32,
    triggers: Vec<PersistedTriggerSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedTriggerSpec {
    name: String,
    expr: String,
    fire_once: bool,
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
