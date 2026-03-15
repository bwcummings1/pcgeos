#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use swat_adapter_agent::{AgentRuntimeAdapter, AgentRuntimeSpec};
use swat_adapter_local::{LocalProcessAdapter, LocalProcessSpec};
use swat_adapter_mock::MockAdapter;
use swat_adapter_pcgeos::{PcGeosAdapter, bundled_fixture_path};
use swat_api::{
    HandleSummary, LiveSessionApi, ObjectSummary, ObservedValueSummary, PatientSummary,
    ResourceSummary, SourceFunctionSummary, StackFrame, TraceInspector, WatchpointSpec,
};
use swat_command::{
    BreakpointConditionInput, Command, CommandOutput, CommandSurface,
    DEFAULT_COMMAND_HISTORY_LIMIT, DashboardLayout, command_completions, command_help,
    command_search, format_command_history_output, load_persisted_command_history, parse_command,
    store_persisted_command_history,
};
use swat_control::{Trigger, TriggerAction, TriggerEngine, TriggerPredicate, pump_with_triggers};
use swat_core::{
    BoundaryId, ControlAction, EventEnvelope, EventId, EventKind, SessionId, SwatError, SwatResult,
    TargetAdapter,
};
use swat_expr::parse_expression;
use swat_session::SessionManager;
use swat_source::SourceSnippet;
use swat_store::{FileStore, InMemoryStore, SwatStore};

const TICK_RATE: Duration = Duration::from_millis(100);
const MAX_MESSAGES: usize = 8;
const LEGACY_SLIST_BEFORE: usize = 4;
const LEGACY_SLIST_AFTER: usize = 5;
const LEGACY_VIEW_BEFORE: usize = 10;
const LEGACY_VIEW_AFTER: usize = 14;
const UNTIL_POLL_DELAY: Duration = Duration::from_millis(10);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Mock,
    Local { program: String, args: Vec<String> },
    Agent { program: String, args: Vec<String> },
    PcGeos { fixture_path: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TuiConfig {
    pub mode: Mode,
    pub store_path: Option<String>,
    pub headless: bool,
    pub headless_ticks: usize,
}

impl TuiConfig {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            store_path: None,
            headless: false,
            headless_ticks: 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EventFilter {
    All,
    Kind(EventKind),
    Query(String),
    Correlation(String),
    Boundary(BoundaryId),
    SourceFile(String),
}

impl EventFilter {
    fn label(&self) -> String {
        match self {
            Self::All => "all".to_string(),
            Self::Kind(kind) => format!("kind={kind:?}"),
            Self::Query(expr) => format!("query={expr}"),
            Self::Correlation(id) => format!("correlation={id}"),
            Self::Boundary(boundary_id) => format!("boundary={}", boundary_id.raw()),
            Self::SourceFile(file) => format!("source.file={file}"),
        }
    }
}

struct LiveRuntime {
    manager: SessionManager,
    adapter: Box<dyn TargetAdapter>,
    store: Box<dyn SwatStore>,
    trigger_engine: TriggerEngine,
    session_id: Option<SessionId>,
}

impl LiveRuntime {
    fn new(adapter: Box<dyn TargetAdapter>, store: Box<dyn SwatStore>) -> Self {
        Self {
            manager: SessionManager::new(),
            adapter,
            store,
            trigger_engine: TriggerEngine::new(),
            session_id: None,
        }
    }

    fn attach(&mut self) -> SwatResult<String> {
        if self.session_id.is_some() {
            return Ok("already attached".to_string());
        }
        let report = self
            .manager
            .attach(self.adapter.as_mut(), self.store.as_mut())?;
        self.session_id = Some(report.session.session_id);
        Ok(format!(
            "attached session={} target={} runtime={}",
            report.session.session_id.raw(),
            report.session.target_id.raw(),
            report.session.descriptor.runtime
        ))
    }

    fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    fn inspector(&self) -> TraceInspector<'_, dyn SwatStore> {
        TraceInspector::new(self.store.as_ref())
    }

    fn api(&mut self) -> LiveSessionApi<'_, dyn TargetAdapter, dyn SwatStore> {
        LiveSessionApi::new(
            &mut self.manager,
            self.adapter.as_mut(),
            self.store.as_mut(),
            &mut self.trigger_engine,
        )
    }

    fn require_session_id(&self) -> SwatResult<SessionId> {
        self.session_id
            .ok_or_else(|| SwatError::new("attach a target first"))
    }

    fn pump(&mut self) -> SwatResult<usize> {
        let Some(session_id) = self.session_id else {
            return Ok(0);
        };
        let report = self
            .manager
            .pump(session_id, self.adapter.as_mut(), self.store.as_mut())?;
        Ok(report.stored_events.len())
    }

    fn control(&mut self, action: ControlAction) -> SwatResult<String> {
        let session_id = self
            .session_id
            .ok_or_else(|| SwatError::new("attach a target first"))?;
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
        Ok(control_report
            .snapshot
            .map(|snapshot| format!("created snapshot {}", snapshot.snapshot_id.raw()))
            .unwrap_or(control_report.response.summary))
    }

    fn session_events(&self, filter: &EventFilter) -> SwatResult<Vec<EventEnvelope>> {
        let Some(session_id) = self.session_id else {
            return Ok(Vec::new());
        };
        let inspector = self.inspector();
        match filter {
            EventFilter::All => Ok(inspector.session_events(session_id)),
            EventFilter::Kind(kind) => Ok(inspector.events_by_kind(session_id, *kind)),
            EventFilter::Query(expr) => inspector.query_events_str(session_id, expr),
            EventFilter::Correlation(id) => Ok(inspector.events_for_correlation(session_id, id)),
            EventFilter::Boundary(boundary_id) => {
                Ok(inspector.boundary_span(session_id, *boundary_id))
            }
            EventFilter::SourceFile(file) => inspector.events_for_source_file(session_id, file),
        }
    }

    fn session_label(&self) -> String {
        let Some(session_id) = self.session_id else {
            return "detached".to_string();
        };
        let Some(session) = self.manager.session(session_id) else {
            return "detached".to_string();
        };
        format!(
            "session={} target={} runtime={}",
            session.session_id.raw(),
            session.descriptor.target_name,
            session.descriptor.runtime
        )
    }
}

pub struct TuiApp {
    runtime: LiveRuntime,
    filter: EventFilter,
    selected_event: usize,
    selected_frame: usize,
    dashboard_layout: DashboardLayout,
    command_mode: bool,
    command_input: String,
    command_history: VecDeque<String>,
    command_history_index: Option<usize>,
    completion_matches: Vec<String>,
    completion_index: usize,
    history_persist_error_reported: bool,
    default_patient: Option<String>,
    manual_source: Option<ManualSourceView>,
    messages: VecDeque<String>,
}

impl TuiApp {
    pub fn new(config: &TuiConfig) -> SwatResult<Self> {
        let (history, history_warning) = match load_persisted_command_history(
            CommandSurface::Tui,
            DEFAULT_COMMAND_HISTORY_LIMIT,
        ) {
            Ok(history) => (history, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        };
        let mut messages = VecDeque::from([
            "q quit | :help | Tab complete | Up/Down history | 1/2/3 dashboards | a attach | u pump | r resume | p pause | s step".to_string(),
        ]);
        if let Some(warning) = history_warning {
            messages.push_back(format!("history unavailable: {warning}"));
        }
        Ok(Self {
            runtime: LiveRuntime::new(
                build_adapter(&config.mode)?,
                build_store(config.store_path.as_deref())?,
            ),
            filter: EventFilter::All,
            selected_event: 0,
            selected_frame: 0,
            dashboard_layout: DashboardLayout::Execution,
            command_mode: false,
            command_input: String::new(),
            command_history: history.into(),
            command_history_index: None,
            completion_matches: Vec::new(),
            completion_index: 0,
            history_persist_error_reported: false,
            default_patient: None,
            manual_source: None,
            messages,
        })
    }

    pub fn attach(&mut self) -> SwatResult<()> {
        let summary = self.runtime.attach()?;
        self.push_message(summary);
        Ok(())
    }

    pub fn resume(&mut self) -> SwatResult<()> {
        let message = self.runtime.control(ControlAction::Resume)?;
        self.push_message(message);
        Ok(())
    }

    pub fn on_tick(&mut self) -> SwatResult<()> {
        let pumped = self.runtime.pump()?;
        if pumped > 0 {
            self.push_message(format!("tick captured {pumped} event(s)"));
            let len = self.runtime.session_events(&self.filter)?.len();
            if len > 0 {
                self.selected_event = len - 1;
            }
        }
        self.clamp_selection()?;
        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SwatResult<bool> {
        if self.command_mode {
            return self.handle_command_key(key).map(|_| false);
        }

        match key.code {
            KeyCode::Char('q') => Ok(true),
            KeyCode::Char(':') => {
                self.command_mode = true;
                self.command_input.clear();
                self.reset_completion_state();
                self.command_history_index = None;
                Ok(false)
            }
            KeyCode::Char('a') => {
                self.attach()?;
                Ok(false)
            }
            KeyCode::Char('u') => {
                let pumped = self.runtime.pump()?;
                self.push_message(format!("manual pump captured {pumped} event(s)"));
                self.clamp_selection()?;
                Ok(false)
            }
            KeyCode::Char('r') => {
                let message = self.runtime.control(ControlAction::Resume)?;
                self.push_message(message);
                Ok(false)
            }
            KeyCode::Char('p') => {
                let message = self.runtime.control(ControlAction::Pause)?;
                self.push_message(message);
                Ok(false)
            }
            KeyCode::Char('s') => {
                let message = self.runtime.control(ControlAction::Step)?;
                self.push_message(message);
                Ok(false)
            }
            KeyCode::Char('1') => {
                self.set_dashboard_layout(DashboardLayout::Execution);
                Ok(false)
            }
            KeyCode::Char('2') => {
                self.set_dashboard_layout(DashboardLayout::Control);
                Ok(false)
            }
            KeyCode::Char('3') => {
                self.set_dashboard_layout(DashboardLayout::Target);
                Ok(false)
            }
            KeyCode::Char('c') => {
                self.filter = EventFilter::All;
                self.push_message("cleared event filter".to_string());
                self.clamp_selection()?;
                Ok(false)
            }
            KeyCode::Up => {
                if self.selected_event > 0 {
                    self.selected_event -= 1;
                }
                Ok(false)
            }
            KeyCode::Down => {
                let len = self.runtime.session_events(&self.filter)?.len();
                if len > 0 && self.selected_event + 1 < len {
                    self.selected_event += 1;
                }
                Ok(false)
            }
            KeyCode::PageUp => {
                self.selected_event = self.selected_event.saturating_sub(10);
                Ok(false)
            }
            KeyCode::PageDown => {
                let len = self.runtime.session_events(&self.filter)?.len();
                if len > 0 {
                    self.selected_event = (self.selected_event + 10).min(len - 1);
                }
                Ok(false)
            }
            KeyCode::Home => {
                self.selected_event = 0;
                Ok(false)
            }
            KeyCode::End => {
                let len = self.runtime.session_events(&self.filter)?.len();
                if len > 0 {
                    self.selected_event = len - 1;
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let snapshot = match self.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.push_message(format!("render error: {error}"));
                ViewSnapshot::default()
            }
        };

        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(16),
                Constraint::Length(6),
                Constraint::Length(3),
            ])
            .split(frame.area());
        let content = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
            .split(root[1]);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(28),
                Constraint::Percentage(34),
                Constraint::Percentage(38),
            ])
            .split(content[1]);

        let title = Paragraph::new(Line::from(vec![
            Span::styled(
                "swat-ui-tui",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::raw(snapshot.session_label),
            Span::raw("  "),
            Span::styled(
                format!("filter={}", snapshot.filter_label),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                format!("dashboard={}", snapshot.dashboard_label),
                Style::default().fg(Color::Green),
            ),
        ]));
        frame.render_widget(title, root[0]);

        let event_items = snapshot
            .events
            .iter()
            .map(|line| ListItem::new(line.clone()))
            .collect::<Vec<_>>();
        let event_list = List::new(event_items)
            .block(pane_block("Events", true))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");
        let mut event_state = ListState::default();
        if !snapshot.events.is_empty() {
            event_state.select(Some(snapshot.selected_index));
        }
        frame.render_stateful_widget(event_list, content[0], &mut event_state);

        frame.render_widget(
            render_lines(&snapshot.pane_one.title, &snapshot.pane_one.lines),
            right[0],
        );
        frame.render_widget(
            render_lines(&snapshot.pane_two.title, &snapshot.pane_two.lines),
            right[1],
        );
        frame.render_widget(
            render_lines(&snapshot.pane_three.title, &snapshot.pane_three.lines),
            right[2],
        );
        frame.render_widget(render_lines("Messages", &snapshot.message_lines), root[2]);

        let input_title = if self.command_mode {
            "Command"
        } else {
            "Command (: to enter)"
        };
        let input_style = if self.command_mode {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let input = Paragraph::new(self.command_input.as_str())
            .style(input_style)
            .block(
                Block::default()
                    .title(input_title)
                    .borders(Borders::ALL)
                    .border_style(input_style),
            );
        frame.render_widget(input, root[3]);
        if self.command_mode {
            frame.set_cursor_position(Position::new(
                root[3].x + self.command_input.len() as u16 + 1,
                root[3].y + 1,
            ));
        }
    }

    fn snapshot(&mut self) -> SwatResult<ViewSnapshot> {
        let events = self.runtime.session_events(&self.filter)?;
        if events.is_empty() {
            self.selected_event = 0;
        } else if self.selected_event >= events.len() {
            self.selected_event = events.len() - 1;
        }
        let selected_event = events.get(self.selected_event).cloned();
        let event_lines = events.iter().map(format_event_line).collect::<Vec<_>>();
        let (pane_one, pane_two, pane_three) = self.dashboard_panes(selected_event.as_ref())?;

        Ok(ViewSnapshot {
            session_label: self.runtime.session_label(),
            filter_label: self.filter.label(),
            dashboard_label: self.dashboard_layout.label().to_string(),
            selected_index: self.selected_event,
            events: event_lines,
            pane_one,
            pane_two,
            pane_three,
            message_lines: self.messages.iter().cloned().collect(),
        })
    }

    fn set_dashboard_layout(&mut self, layout: DashboardLayout) {
        self.dashboard_layout = layout;
        self.push_message(format!("dashboard={}", layout.label()));
    }

    fn dashboard_panes(
        &mut self,
        selected_event: Option<&EventEnvelope>,
    ) -> SwatResult<(ViewPane, ViewPane, ViewPane)> {
        match self.dashboard_layout {
            DashboardLayout::Execution => Ok((
                ViewPane::new("Stack / Entities", self.entity_lines(selected_event)?),
                ViewPane::new("Source", self.source_lines(selected_event)?),
                ViewPane::new("Artifacts", self.artifact_lines(selected_event)?),
            )),
            DashboardLayout::Control => {
                let breakpoints = self.list_breakpoints_output()?;
                let watchpoints = self.list_watchpoints_output();
                Ok((
                    ViewPane::new(
                        "Breakpoints",
                        with_summary_line(breakpoints.summary, breakpoints.lines),
                    ),
                    ViewPane::new(
                        "Watchpoints",
                        with_summary_line(watchpoints.summary, watchpoints.lines),
                    ),
                    ViewPane::new("Snapshots / Replay", self.snapshot_replay_lines()?),
                ))
            }
            DashboardLayout::Target => Ok((
                ViewPane::new("Stack Frames", self.stack_lines(Some(8))?),
                ViewPane::new(
                    "Patients / Handles / Objects",
                    self.target_entity_catalog_lines()?,
                ),
                ViewPane::new(
                    "Source Navigation",
                    self.source_navigation_lines(selected_event)?,
                ),
            )),
        }
    }

    fn snapshot_replay_lines(&mut self) -> SwatResult<Vec<String>> {
        let Some(session_id) = self.runtime.session_id() else {
            return Ok(vec![
                "attach a target to inspect snapshots and replay".to_string(),
            ]);
        };
        let inspector = self.runtime.inspector();
        let snapshots = inspector.session_snapshots(session_id);
        let mut lines = vec![format!("snapshots={}", snapshots.len())];
        for snapshot in snapshots.iter().rev().take(4) {
            let replay_directives = inspector
                .snapshot_inspection(snapshot.snapshot_id)
                .map(|inspection| inspection.replay_directive_count)
                .unwrap_or_default();
            lines.push(format!(
                "snapshot={} seq={} replay={} reason={}",
                snapshot.snapshot_id.raw(),
                snapshot.captured_sequence_no,
                replay_directives,
                snapshot.reason
            ));
        }

        let frames = inspector.stack_frames(session_id)?;
        if let Some(frame) = frames.get(self.selected_frame.min(frames.len().saturating_sub(1))) {
            let plan = inspector.replay_plan_for_boundary(session_id, frame.boundary_id);
            lines.push(format!(
                "current-boundary={} frame={} directives={}",
                frame.boundary_id.raw(),
                frame.frame_index,
                plan.len()
            ));
            lines.extend(format_tui_replay_plan_lines(&plan).into_iter().take(4));
        } else {
            lines.push("no current boundary replay plan".to_string());
        }
        Ok(lines)
    }

    fn target_entity_catalog_lines(&self) -> SwatResult<Vec<String>> {
        let Some(session_id) = self.runtime.session_id() else {
            return Ok(vec![
                "attach a target to inspect patients and objects".to_string(),
            ]);
        };
        let inspector = self.runtime.inspector();
        let patients = inspector.patients(session_id)?;
        let handles = inspector.handles(session_id)?;
        let objects = inspector.objects(session_id)?;
        let mut lines = vec![format!(
            "patients={} handles={} objects={}",
            patients.len(),
            handles.len(),
            objects.len()
        )];
        if !patients.is_empty() {
            lines.push("patients:".to_string());
            lines.extend(patients.iter().take(3).map(format_tui_patient_summary));
        }
        if !handles.is_empty() {
            lines.push("handles:".to_string());
            lines.extend(handles.iter().take(3).map(format_tui_handle_summary));
        }
        if !objects.is_empty() {
            lines.push("objects:".to_string());
            lines.extend(objects.iter().take(3).map(format_tui_object_summary));
        }
        Ok(lines)
    }

    fn source_navigation_lines(
        &self,
        selected_event: Option<&EventEnvelope>,
    ) -> SwatResult<Vec<String>> {
        let Some(session_id) = self.runtime.session_id() else {
            return Ok(vec![
                "attach a target to inspect source catalogs".to_string(),
            ]);
        };
        let inspector = self.runtime.inspector();
        let files = inspector.source_files(session_id)?;
        let functions = inspector.source_functions(session_id)?;
        let mut lines = vec![format!(
            "files={} functions={}",
            files.len(),
            functions.len()
        )];

        if let Some(view) = &self.manual_source {
            lines.push("manual-source:".to_string());
            lines.extend(view.lines.iter().take(6).cloned());
            return Ok(lines);
        }
        if let Some(event) = selected_event {
            if let Some(location) = inspector.source_inspection(event, 1, 2)?.location {
                lines.push(format!(
                    "current={} line={} function={}",
                    location.file,
                    location.line,
                    location.function.unwrap_or_else(|| "-".to_string())
                ));
            }
        }
        if !files.is_empty() {
            lines.push("files:".to_string());
            lines.extend(files.iter().take(3).map(|file| {
                format!(
                    "file={} events={} functions={}",
                    file.file,
                    file.event_count,
                    if file.functions.is_empty() {
                        "-".to_string()
                    } else {
                        file.functions.join(",")
                    }
                )
            }));
        }
        if !functions.is_empty() {
            lines.push("functions:".to_string());
            lines.extend(functions.iter().take(3).map(|function| {
                format!(
                    "function={} file={} lines={}..{}",
                    function.function,
                    function.file,
                    function
                        .first_line
                        .map(|line| line.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    function
                        .last_line
                        .map(|line| line.to_string())
                        .unwrap_or_else(|| "-".to_string())
                )
            }));
        }
        Ok(lines)
    }

    fn entity_lines(&self, selected_event: Option<&EventEnvelope>) -> SwatResult<Vec<String>> {
        let Some(session_id) = self.runtime.session_id() else {
            return Ok(vec!["attach a target to inspect entities".to_string()]);
        };
        let inspector = self.runtime.inspector();
        let mut lines = Vec::new();
        let frames = inspector.stack_frames(session_id)?;
        lines.push(format!("stack frames={}", frames.len()));
        if frames.is_empty() {
            lines.push("no frame-oriented boundary spans".to_string());
        } else {
            let selected_event_id = selected_event.map(|event| event.event_id);
            lines.extend(frames.into_iter().take(4).map(|frame| {
                let marker = if selected_event_id
                    .map(|event_id| frame.event_ids.contains(&event_id))
                    .unwrap_or(false)
                {
                    '>'
                } else {
                    ' '
                };
                format!("{marker} {}", format_stack_frame_line(&frame))
            }));
        }

        if let Some(event) = selected_event {
            lines.push(format!(
                "selected event={} seq={} kind={:?}",
                event.event_id.raw(),
                event.sequence_no,
                event.kind
            ));
            let entities = inspector.resolve_event_entities(event)?;
            if entities.is_empty() {
                lines.push("no entities on selected event".to_string());
            } else {
                lines.push("event entities:".to_string());
                lines.extend(
                    entities
                        .into_iter()
                        .map(|entity| format!("  {:?} {}", entity.kind, entity.name)),
                );
            }
            let typed = inspector.event_typed_entities(event)?;
            if !typed.patients.is_empty() {
                lines.push("event patients:".to_string());
                lines.extend(
                    typed
                        .patients
                        .iter()
                        .map(|patient| format!("  {}", format_tui_event_patient(patient))),
                );
            }
            if !typed.handles.is_empty() {
                lines.push("event handles:".to_string());
                lines.extend(
                    typed
                        .handles
                        .iter()
                        .map(|handle| format!("  {}", format_tui_event_handle(handle))),
                );
            }
            if !typed.resources.is_empty() {
                lines.push("event resources:".to_string());
                lines.extend(
                    typed
                        .resources
                        .iter()
                        .map(|resource| format!("  {}", format_tui_event_resource(resource))),
                );
            }
            if !typed.objects.is_empty() {
                lines.push("event objects:".to_string());
                lines.extend(
                    typed
                        .objects
                        .iter()
                        .map(|object| format!("  {}", format_tui_event_object(object))),
                );
            }
        } else {
            lines.push("select an event to inspect entities".to_string());
        }

        if let Some(correlation_id) =
            selected_event.and_then(|event| event.causality.correlation_id.as_deref())
        {
            if let Some(group) = inspector
                .correlation_groups(session_id)?
                .into_iter()
                .find(|group| group.correlation_id == correlation_id)
            {
                lines.push(format!("correlation group={}", group.correlation_id));
                lines.push(format!(
                    "  spans={}",
                    if group.span_ids.is_empty() {
                        "-".to_string()
                    } else {
                        group.span_ids.join(",")
                    }
                ));
                lines.push(format!("  entities={}", group.entities.len()));
            }
        }

        Ok(lines)
    }

    fn source_lines(&self, selected_event: Option<&EventEnvelope>) -> SwatResult<Vec<String>> {
        if let Some(view) = &self.manual_source {
            return Ok(view.lines.clone());
        }
        let Some(event) = selected_event else {
            return Ok(vec!["select an event to inspect source".to_string()]);
        };
        let inspection = self.runtime.inspector().source_inspection(event, 2, 4)?;
        let Some(location) = inspection.location else {
            return Ok(vec!["no source metadata".to_string()]);
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
            lines.extend(snippet.lines.into_iter().map(|line| {
                let marker = if line.line_number == snippet.focus_line {
                    '>'
                } else {
                    ' '
                };
                format!("{marker} {:>4} {}", line.line_number, line.text)
            }));
        } else if let Some(failure) = inspection.failure {
            lines.push(format!("failure_kind={:?}", failure.kind));
            lines.push(failure.message);
        }
        Ok(lines)
    }

    fn artifact_lines(&self, selected_event: Option<&EventEnvelope>) -> SwatResult<Vec<String>> {
        let Some(event) = selected_event else {
            return Ok(vec!["select an event to inspect artifacts".to_string()]);
        };
        let presentations = self.runtime.inspector().artifact_presentations(event, 80)?;
        if presentations.is_empty() {
            return Ok(vec!["no artifacts on selected event".to_string()]);
        }
        let mut lines = Vec::new();
        for (index, presentation) in presentations.iter().enumerate() {
            lines.push(format!(
                "#{index} bytes={} lines={} preview={}",
                presentation.byte_len, presentation.line_count, presentation.preview
            ));
        }
        lines.push("detail:".to_string());
        lines.extend(
            presentations[0]
                .detail
                .lines()
                .take(8)
                .map(|line| format!("  {line}")),
        );
        Ok(lines)
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> SwatResult<()> {
        match key.code {
            KeyCode::Esc => {
                self.command_mode = false;
                self.command_input.clear();
                self.reset_completion_state();
                self.command_history_index = None;
            }
            KeyCode::Enter => {
                let command = std::mem::take(&mut self.command_input);
                self.command_mode = false;
                let trimmed = command.trim().to_string();
                self.record_command(&trimmed);
                self.reset_completion_state();
                self.command_history_index = None;
                self.execute_command(&trimmed)?;
            }
            KeyCode::Backspace => {
                self.command_input.pop();
                self.reset_completion_state();
                self.command_history_index = None;
            }
            KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::CONTROL) && ch == 'c' => {
                self.command_mode = false;
                self.command_input.clear();
                self.reset_completion_state();
                self.command_history_index = None;
            }
            KeyCode::Char(ch) => {
                self.command_input.push(ch);
                self.reset_completion_state();
                self.command_history_index = None;
            }
            KeyCode::Tab => self.complete_command_input(),
            KeyCode::Up => self.recall_history(-1),
            KeyCode::Down => self.recall_history(1),
            _ => {}
        }
        Ok(())
    }

    fn execute_command(&mut self, input: &str) -> SwatResult<()> {
        if input.is_empty() {
            return Ok(());
        }
        if input == "clear" {
            self.filter = EventFilter::All;
            self.selected_event = 0;
            self.manual_source = None;
            self.push_message("cleared event filter".to_string());
            self.clamp_selection()?;
            return Ok(());
        }

        match parse_command(input)? {
            Command::Help { topic } => self.show_help(topic.as_deref()),
            Command::HelpSearch { needle } => self.show_help_search(&needle),
            Command::Dashboard { layout } => self.set_dashboard_layout(layout),
            Command::History { limit } => {
                let history = self.command_history.iter().cloned().collect::<Vec<_>>();
                self.show_command_output(format_command_history_output(&history, limit));
            }
            Command::Attach => self.attach()?,
            Command::Session => self.push_message(self.runtime.session_label()),
            Command::Pump => {
                let pumped = self.runtime.pump()?;
                self.push_message(format!("manual pump captured {pumped} event(s)"));
            }
            Command::Pause => {
                let message = self.runtime.control(ControlAction::Pause)?;
                self.push_message(message);
            }
            Command::Resume => {
                let message = self.runtime.control(ControlAction::Resume)?;
                self.push_message(message);
            }
            Command::Step => {
                let message = self.runtime.control(ControlAction::Step)?;
                self.push_message(message);
            }
            Command::Snapshot { reason } => {
                let message = self
                    .runtime
                    .control(ControlAction::CreateSnapshot { reason })?;
                self.push_message(message);
            }
            Command::Events { kind } => {
                self.filter = kind.map_or(EventFilter::All, EventFilter::Kind);
                self.selected_event = 0;
                self.manual_source = None;
                self.push_message(format!("event filter={}", self.filter.label()));
            }
            Command::Query { expr } => {
                self.filter = EventFilter::Query(expr.clone());
                self.selected_event = 0;
                self.manual_source = None;
                self.push_message(format!("event filter=query {expr}"));
            }
            Command::Correlation { correlation_id } => {
                self.filter = EventFilter::Correlation(correlation_id.clone());
                self.selected_event = 0;
                self.manual_source = None;
                self.push_message(format!("event filter=correlation {correlation_id}"));
            }
            Command::Patients => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect patients".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let patients = self.runtime.inspector().patients(session_id)?;
                self.show_command_output(CommandOutput::new(
                    format!("{} patient(s)", patients.len()),
                    patients.iter().map(format_tui_patient_summary).collect(),
                ));
            }
            Command::PatientShow { patient } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect patients".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let detail = self
                    .runtime
                    .inspector()
                    .patient_detail(session_id, &patient)?
                    .ok_or_else(|| SwatError::new(format!("unknown patient {patient}")))?;
                self.show_command_output(CommandOutput::new(
                    format!("patient {}", detail.patient.name),
                    format_tui_patient_detail(&detail),
                ));
            }
            Command::Handles => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect handles".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let handles = self.runtime.inspector().handles(session_id)?;
                self.show_command_output(CommandOutput::new(
                    format!("{} handle(s)", handles.len()),
                    handles.iter().map(format_tui_handle_summary).collect(),
                ));
            }
            Command::HandleShow { handle } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect handles".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let detail = self
                    .runtime
                    .inspector()
                    .handle_detail(session_id, &handle)?
                    .ok_or_else(|| SwatError::new(format!("unknown handle {handle}")))?;
                self.show_command_output(CommandOutput::new(
                    format!("handle {}", detail.handle.key),
                    format_tui_handle_detail(&detail),
                ));
            }
            Command::Resources => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect resources".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let resources = self.runtime.inspector().resources(session_id)?;
                self.show_command_output(CommandOutput::new(
                    format!("{} resource(s)", resources.len()),
                    resources.iter().map(format_tui_resource_summary).collect(),
                ));
            }
            Command::ResourceShow { resource } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect resources".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let detail = self
                    .runtime
                    .inspector()
                    .resource_detail(session_id, &resource)?
                    .ok_or_else(|| SwatError::new(format!("unknown resource {resource}")))?;
                self.show_command_output(CommandOutput::new(
                    format!("resource {}", detail.resource.name),
                    format_tui_resource_detail(&detail),
                ));
            }
            Command::Objects => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect objects".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let objects = self.runtime.inspector().objects(session_id)?;
                self.show_command_output(CommandOutput::new(
                    format!("{} object(s)", objects.len()),
                    objects.iter().map(format_tui_object_summary).collect(),
                ));
            }
            Command::ObjectShow { object } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect objects".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let detail = self
                    .runtime
                    .inspector()
                    .object_detail(session_id, &object)?
                    .ok_or_else(|| SwatError::new(format!("unknown object {object}")))?;
                self.show_command_output(CommandOutput::new(
                    format!("object {}", detail.object.key),
                    format_tui_object_detail(&detail),
                ));
            }
            Command::Values => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect values".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let values = self.runtime.inspector().observed_values(session_id)?;
                self.show_command_output(CommandOutput::new(
                    format!("{} observed value(s)", values.len()),
                    values
                        .iter()
                        .map(format_tui_observed_value_summary)
                        .collect(),
                ));
            }
            Command::ValueShow { value_key } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect values".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let detail = self
                    .runtime
                    .inspector()
                    .observed_value_detail(session_id, &value_key)?
                    .ok_or_else(|| SwatError::new(format!("unknown observed value {value_key}")))?;
                self.show_command_output(CommandOutput::new(
                    format!("value {}", detail.value.value_key),
                    format_tui_observed_value_detail(&detail),
                ));
            }
            Command::Backtrace { limit } => {
                let lines = self.stack_lines(limit)?;
                self.show_command_output(CommandOutput::new("backtrace", lines));
            }
            Command::Where => {
                let output = self.where_output()?;
                self.show_command_output(output);
            }
            Command::Function { name } => {
                self.select_function(name.as_deref())?;
            }
            Command::Up { count } => {
                self.move_frame_cursor(count, true)?;
            }
            Command::Down { count } => {
                self.move_frame_cursor(count, false)?;
            }
            Command::Locals { frame_index } => {
                let frame_index = frame_index.unwrap_or(self.current_frame_index()?);
                self.selected_frame = frame_index;
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect frame locals".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let locals = self
                    .runtime
                    .inspector()
                    .stack_frame_locals(session_id, frame_index)?;
                self.show_command_output(CommandOutput::new(
                    format!("stack frame {} locals", frame_index),
                    locals.iter().map(format_tui_frame_local).collect(),
                ));
            }
            Command::Spans => {
                let lines = self.stack_lines(None)?;
                self.show_command_output(CommandOutput::new("stack", lines));
            }
            Command::Frame { frame_index } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect stack frames".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let Some(frame) = self
                    .runtime
                    .inspector()
                    .stack_frame(session_id, frame_index)?
                else {
                    return Err(SwatError::new(format!("unknown stack frame {frame_index}")));
                };
                self.selected_frame = frame_index;
                self.filter = EventFilter::Boundary(frame.boundary_id);
                self.selected_event = 0;
                self.manual_source = None;
                self.push_message(format!(
                    "event filter=frame {} boundary {}",
                    frame.frame_index,
                    frame.boundary_id.raw()
                ));
            }
            Command::FrameLocals { frame_index } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect frame locals".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                self.selected_frame = frame_index;
                let locals = self
                    .runtime
                    .inspector()
                    .stack_frame_locals(session_id, frame_index)?;
                self.show_command_output(CommandOutput::new(
                    format!("stack frame {} locals", frame_index),
                    locals.iter().map(format_tui_frame_local).collect(),
                ));
            }
            Command::FrameRegisters { frame_index } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect frame registers".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                self.selected_frame = frame_index;
                let registers = self
                    .runtime
                    .inspector()
                    .stack_frame_registers(session_id, frame_index)?;
                self.show_command_output(CommandOutput::new(
                    format!("stack frame {} registers", frame_index),
                    registers.iter().map(format_tui_frame_register).collect(),
                ));
            }
            Command::Span { boundary_id } => {
                self.filter = EventFilter::Boundary(boundary_id);
                self.selected_event = 0;
                self.manual_source = None;
                self.push_message(format!("event filter=boundary {}", boundary_id.raw()));
            }
            Command::Event { event_id } => self.select_event(event_id)?,
            Command::Source { event_id, .. } => {
                self.manual_source = None;
                self.select_event(event_id)?;
                self.push_message(format!("source event={}", event_id.raw()));
            }
            Command::SourceFiles => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to discover source files".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let files = self.runtime.inspector().source_files(session_id)?;
                self.push_message(format!("source files={}", files.len()));
                for file in files.into_iter().take(MAX_MESSAGES.saturating_sub(1)) {
                    let functions = if file.functions.is_empty() {
                        "-".to_string()
                    } else {
                        file.functions.join(",")
                    };
                    self.push_message(format!(
                        "file={} events={} functions={} real={}",
                        file.file, file.event_count, functions, file.is_real_path
                    ));
                }
            }
            Command::SourceFunctions => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to discover source functions".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let functions = self.runtime.inspector().source_functions(session_id)?;
                self.show_command_output(CommandOutput::new(
                    format!("{} source function(s)", functions.len()),
                    functions
                        .iter()
                        .map(format_tui_source_function_summary)
                        .collect(),
                ));
            }
            Command::SourceFunction { function } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect source functions".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let events = self
                    .runtime
                    .inspector()
                    .events_for_source_function(session_id, &function)?;
                self.show_command_output(CommandOutput::new(
                    format!("{} event(s) for source function {}", events.len(), function),
                    events.iter().map(format_event_line).collect(),
                ));
            }
            Command::SourceFile { file } => {
                self.filter = EventFilter::SourceFile(file.clone());
                self.selected_event = 0;
                self.manual_source = None;
                self.push_message(format!("event filter=source.file {file}"));
            }
            Command::SourceView {
                file,
                line,
                before,
                after,
            } => {
                let snippet = self
                    .runtime
                    .inspector()
                    .source_file_view(&file, line, before, after)?;
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
                self.manual_source = Some(ManualSourceView { lines });
                self.push_message(format!("source view {}:{}", file, line));
            }
            Command::SourceList { file, line } => {
                let output = self.legacy_source_output(
                    file.as_deref(),
                    line,
                    LEGACY_SLIST_BEFORE,
                    LEGACY_SLIST_AFTER,
                )?;
                self.show_command_output(output);
            }
            Command::View { file, line } => {
                let output = self.legacy_source_output(
                    file.as_deref(),
                    line,
                    LEGACY_VIEW_BEFORE,
                    LEGACY_VIEW_AFTER,
                )?;
                self.show_command_output(output);
            }
            Command::PatientDefault { patient } => match patient.as_deref() {
                None | Some("") => self.push_message(format!(
                    "patient-default={}",
                    self.default_patient.as_deref().unwrap_or("-")
                )),
                Some("off") => {
                    self.default_patient = None;
                    self.push_message("patient-default=-".to_string());
                }
                Some(patient) => {
                    self.default_patient = Some(patient.to_string());
                    self.push_message(format!("patient-default={patient}"));
                }
            },
            Command::Spawn { patient, function } => {
                let patient = self.resolve_default_patient(patient.as_deref())?;
                let mut expr = format!(r#"patient == "{}""#, escape_query_string(&patient));
                if let Some(function) = function.as_deref() {
                    expr.push_str(&format!(
                        r#" and source.function == "{}""#,
                        escape_query_string(function)
                    ));
                }
                self.run_until_expr(&format!("spawn {patient}"), &expr)?;
            }
            Command::Wakeup { patient } => {
                let patient = self.resolve_default_patient(patient.as_deref())?;
                let expr = format!(r#"patient == "{}""#, escape_query_string(&patient));
                self.run_until_expr(&format!("wakeup {patient}"), &expr)?;
            }
            Command::ObjectName { object } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect objects".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let detail = self
                    .runtime
                    .inspector()
                    .object_detail(session_id, &object)?
                    .ok_or_else(|| SwatError::new(format!("unknown object {object}")))?;
                self.show_command_output(CommandOutput::new(
                    format!("obj-name {}", detail.object.key),
                    vec![format!(
                        "{} class={} patient={} handle={} resource={}",
                        detail.object.key,
                        detail.object.class_name.as_deref().unwrap_or("-"),
                        detail.object.patient.as_deref().unwrap_or("-"),
                        detail.object.handle.as_deref().unwrap_or("-"),
                        detail.object.resource.as_deref().unwrap_or("-")
                    )],
                ));
            }
            Command::ObjectClass { object } => {
                let Some(session_id) = self.runtime.session_id() else {
                    self.push_message("attach a target to inspect objects".to_string());
                    self.clamp_selection()?;
                    return Ok(());
                };
                let detail = self
                    .runtime
                    .inspector()
                    .object_detail(session_id, &object)?
                    .ok_or_else(|| SwatError::new(format!("unknown object {object}")))?;
                self.show_command_output(CommandOutput::new(
                    format!("obj-class {}", detail.object.key),
                    vec![format!(
                        "class={}",
                        detail.object.class_name.as_deref().unwrap_or("-")
                    )],
                ));
            }
            Command::Breakpoints => {
                let output = self.list_breakpoints_output()?;
                self.show_command_output(output);
            }
            Command::BreakpointShow { trigger_id } => {
                let output = self.show_breakpoint_output(trigger_id)?;
                self.show_command_output(output);
            }
            Command::BreakpointGroups => {
                let output = self.list_breakpoint_groups_output();
                self.show_command_output(output);
            }
            Command::BreakpointDefinitionGroups => {
                let output = self.list_breakpoint_definition_groups_output();
                self.show_command_output(output);
            }
            Command::BreakpointPredicates => {
                let output = self.list_breakpoint_predicates_output();
                self.show_command_output(output);
            }
            Command::BreakpointPredicateAdd { name, expr } => {
                let output = self.add_breakpoint_predicate_output(&name, &expr)?;
                self.show_command_output(output);
            }
            Command::BreakpointPredicateRemove { name } => {
                let output = self.remove_breakpoint_predicate_output(&name)?;
                self.show_command_output(output);
            }
            Command::BreakpointGroupEnable { group } => {
                let output = self.set_breakpoint_group_enabled_output(&group, true)?;
                self.show_command_output(output);
            }
            Command::BreakpointGroupDisable { group } => {
                let output = self.set_breakpoint_group_enabled_output(&group, false)?;
                self.show_command_output(output);
            }
            Command::TriggerExpr {
                name,
                condition,
                fire_once,
                group,
            } => {
                let output =
                    self.add_breakpoint_output(&name, condition, fire_once, group, None)?;
                self.show_command_output(output);
            }
            Command::TriggerSnapshot {
                name,
                condition,
                reason,
                group,
            } => {
                let output =
                    self.add_breakpoint_output(&name, condition, false, group, Some(reason))?;
                self.show_command_output(output);
            }
            Command::TriggerEnable { trigger_id } => {
                let output = self.set_trigger_enabled_output(trigger_id, true)?;
                self.show_command_output(output);
            }
            Command::TriggerDisable { trigger_id } => {
                let output = self.set_trigger_enabled_output(trigger_id, false)?;
                self.show_command_output(output);
            }
            Command::TriggerRemove { trigger_id } => {
                let output = self.remove_trigger_output(trigger_id)?;
                self.show_command_output(output);
            }
            Command::Watchpoints => {
                let output = self.list_watchpoints_output();
                self.show_command_output(output);
            }
            Command::WatchpointShow { trigger_id } => {
                let output = self.show_watchpoint_output(trigger_id)?;
                self.show_command_output(output);
            }
            Command::WatchpointAdd { spec } => {
                let output = self.add_watchpoint_output(spec)?;
                self.show_command_output(output);
            }
            Command::WatchpointEnable { trigger_id } => {
                let output = self.set_trigger_enabled_output(trigger_id, true)?;
                self.show_command_output(output);
            }
            Command::WatchpointDisable { trigger_id } => {
                let output = self.set_trigger_enabled_output(trigger_id, false)?;
                self.show_command_output(output);
            }
            Command::WatchpointRemove { trigger_id } => {
                let mut output = self.remove_trigger_output(trigger_id)?;
                output.summary = output.summary.replacen("trigger", "watchpoint", 1);
                self.show_command_output(output);
            }
            other => {
                return Err(SwatError::new(format!(
                    "unsupported tui command '{input}' ({other:?}); use ':help' for shared discovery and the shell for shell-only workflows",
                )));
            }
        }
        self.clamp_selection()?;
        Ok(())
    }

    fn show_command_output(&mut self, output: CommandOutput) {
        self.messages.clear();
        self.push_message(output.summary);
        for line in output
            .lines
            .into_iter()
            .take(MAX_MESSAGES.saturating_sub(1))
        {
            self.push_message(line);
        }
    }

    fn breakpoint_condition_label(condition: &BreakpointConditionInput) -> String {
        match condition {
            BreakpointConditionInput::Expression(expr) => expr.clone(),
            BreakpointConditionInput::PredicateRef(name) => format!("@{name}"),
        }
    }

    fn trigger_predicate_from_condition(
        &self,
        condition: &BreakpointConditionInput,
    ) -> SwatResult<TriggerPredicate> {
        match condition {
            BreakpointConditionInput::Expression(expr) => {
                Ok(TriggerPredicate::Expr(parse_expression(expr)?))
            }
            BreakpointConditionInput::PredicateRef(name) => {
                if self.runtime.trigger_engine.predicate(name).is_none() {
                    return Err(SwatError::new(format!(
                        "unknown breakpoint predicate {}",
                        name
                    )));
                }
                Ok(TriggerPredicate::Named(name.clone()))
            }
        }
    }

    fn list_breakpoints_output(&mut self) -> SwatResult<CommandOutput> {
        let groups = self.runtime.api().breakpoint_groups();
        let state_groups = groups
            .into_iter()
            .filter(|group| group.kind == swat_api::BreakpointGroupKind::State)
            .collect::<Vec<_>>();
        let breakpoint_count = state_groups
            .iter()
            .map(|group| group.breakpoints.len())
            .sum::<usize>();
        let mut lines = Vec::new();
        for group in state_groups {
            lines.push(format!(
                "group={} count={}",
                group.label,
                group.breakpoints.len()
            ));
            lines.extend(group.breakpoints.iter().map(format_tui_breakpoint_summary));
        }
        Ok(CommandOutput::new(
            format!("{breakpoint_count} breakpoint(s)"),
            lines,
        ))
    }

    fn show_breakpoint_output(
        &mut self,
        trigger_id: swat_core::TriggerId,
    ) -> SwatResult<CommandOutput> {
        let detail = self
            .runtime
            .api()
            .breakpoint_detail(trigger_id)
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
            format!("when={}", breakpoint.predicate),
            format!("actions={}", breakpoint.actions.join(",")),
            format!(
                "group={} group_enabled={}",
                breakpoint.group.as_deref().unwrap_or("-"),
                breakpoint
                    .group_enabled
                    .map(|enabled| enabled.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!("hits={}", breakpoint.hit_count),
        ];
        if let Some(event) = detail.last_hit_event.as_ref() {
            lines.push(format_event_line(event));
        }
        Ok(CommandOutput::new(
            format!("breakpoint {}", trigger_id.raw()),
            lines,
        ))
    }

    fn list_breakpoint_groups_output(&mut self) -> CommandOutput {
        let groups = self.runtime.api().breakpoint_groups();
        CommandOutput::new(
            format!("{} breakpoint group(s)", groups.len()),
            groups
                .iter()
                .map(|group| {
                    format!(
                        "kind={} group={} count={}",
                        group.kind.label(),
                        group.label,
                        group.breakpoints.len()
                    )
                })
                .collect(),
        )
    }

    fn list_breakpoint_definition_groups_output(&mut self) -> CommandOutput {
        let groups = self.runtime.api().breakpoint_definition_groups();
        let mut lines = Vec::new();
        for group in &groups {
            lines.push(format!(
                "group={} enabled={} count={}",
                group.name,
                group.enabled,
                group.breakpoints.len()
            ));
            lines.extend(group.breakpoints.iter().map(format_tui_breakpoint_summary));
        }
        CommandOutput::new(format!("{} definition group(s)", groups.len()), lines)
    }

    fn list_breakpoint_predicates_output(&mut self) -> CommandOutput {
        let predicates = self.runtime.api().breakpoint_predicates();
        CommandOutput::new(
            format!("{} breakpoint predicate(s)", predicates.len()),
            predicates
                .iter()
                .map(|predicate| {
                    format!(
                        "predicate={} breakpoints={} when={}",
                        predicate.name, predicate.breakpoint_count, predicate.predicate
                    )
                })
                .collect(),
        )
    }

    fn add_breakpoint_predicate_output(
        &mut self,
        name: &str,
        expr: &str,
    ) -> SwatResult<CommandOutput> {
        let session_id = self.runtime.require_session_id()?;
        let predicate = TriggerPredicate::Expr(parse_expression(expr)?);
        let report = self
            .runtime
            .api()
            .define_breakpoint_predicate(session_id, name, predicate)?;
        Ok(CommandOutput::new(
            format!("defined breakpoint predicate {name}"),
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .chain(std::iter::once(format!(
                    "predicate={} expr={:?}",
                    name, expr
                )))
                .collect(),
        ))
    }

    fn remove_breakpoint_predicate_output(&mut self, name: &str) -> SwatResult<CommandOutput> {
        let session_id = self.runtime.require_session_id()?;
        let report = self
            .runtime
            .api()
            .remove_breakpoint_predicate(session_id, name)?;
        Ok(CommandOutput::new(
            format!("removed breakpoint predicate {name}"),
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .chain(std::iter::once(format!("predicate={}", report.value.name)))
                .collect(),
        ))
    }

    fn set_breakpoint_group_enabled_output(
        &mut self,
        group: &str,
        enabled: bool,
    ) -> SwatResult<CommandOutput> {
        let session_id = self.runtime.require_session_id()?;
        let report = self
            .runtime
            .api()
            .set_breakpoint_group_enabled(session_id, group, enabled)?;
        Ok(CommandOutput::new(
            format!(
                "{} breakpoint group {}",
                if enabled { "enabled" } else { "disabled" },
                group
            ),
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .chain(std::iter::once(format!(
                    "group={} previous_enabled={} enabled={}",
                    group, report.value, enabled
                )))
                .collect(),
        ))
    }

    fn add_breakpoint_output(
        &mut self,
        name: &str,
        condition: BreakpointConditionInput,
        fire_once: bool,
        group: Option<String>,
        snapshot_reason: Option<String>,
    ) -> SwatResult<CommandOutput> {
        let session_id = self.runtime.require_session_id()?;
        let predicate = self.trigger_predicate_from_condition(&condition)?;
        let actions = snapshot_reason
            .as_ref()
            .map(|reason| {
                vec![TriggerAction::CreateSnapshot {
                    reason: reason.clone(),
                }]
            })
            .unwrap_or_else(|| vec![TriggerAction::PauseTarget]);
        let mut trigger = Trigger::new(name, predicate, actions);
        if fire_once {
            trigger = trigger.fire_once();
        }
        if let Some(group) = group.as_deref() {
            trigger = trigger.in_group(group);
        }
        let report = self.runtime.api().add_trigger(session_id, trigger)?;
        Ok(CommandOutput::new(
            format!("added breakpoint {}", report.value.raw()),
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .chain(std::iter::once(format!(
                    "breakpoint={} name={} fire_once={}{} condition={}",
                    report.value.raw(),
                    name,
                    fire_once,
                    group
                        .as_ref()
                        .map(|group| format!(" group={group}"))
                        .unwrap_or_default(),
                    Self::breakpoint_condition_label(&condition)
                )))
                .collect(),
        ))
    }

    fn set_trigger_enabled_output(
        &mut self,
        trigger_id: swat_core::TriggerId,
        enabled: bool,
    ) -> SwatResult<CommandOutput> {
        let session_id = self.runtime.require_session_id()?;
        let report = self
            .runtime
            .api()
            .set_trigger_enabled(session_id, trigger_id, enabled)?;
        Ok(CommandOutput::new(
            format!(
                "{} trigger {}",
                if enabled { "enabled" } else { "disabled" },
                trigger_id.raw()
            ),
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .chain(std::iter::once(format!(
                    "trigger={} previous_enabled={} enabled={}",
                    trigger_id.raw(),
                    report.value,
                    enabled
                )))
                .collect(),
        ))
    }

    fn remove_trigger_output(
        &mut self,
        trigger_id: swat_core::TriggerId,
    ) -> SwatResult<CommandOutput> {
        let session_id = self.runtime.require_session_id()?;
        let report = self.runtime.api().remove_trigger(session_id, trigger_id)?;
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

    fn list_watchpoints_output(&mut self) -> CommandOutput {
        let watchpoints = self.runtime.api().watchpoint_summaries();
        CommandOutput::new(
            format!("{} watchpoint(s)", watchpoints.len()),
            watchpoints
                .iter()
                .map(format_tui_watchpoint_summary)
                .collect(),
        )
    }

    fn show_watchpoint_output(
        &mut self,
        trigger_id: swat_core::TriggerId,
    ) -> SwatResult<CommandOutput> {
        let detail = self
            .runtime
            .api()
            .watchpoint_detail(trigger_id)
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
            format!("value_key={}", watchpoint.value_key),
            format!("path={}", watchpoint.path.as_deref().unwrap_or("-")),
            format!(
                "after={}",
                watchpoint
                    .after_millis
                    .map(|millis| millis.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "kind={}",
                watchpoint
                    .event_kind
                    .map(|kind| format!("{kind:?}"))
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "summary={}",
                watchpoint.summary_contains.as_deref().unwrap_or("-")
            ),
            format!("actions={}", breakpoint.actions.join(",")),
        ];
        if let Some(event) = detail.last_hit_event.as_ref() {
            lines.push(format_event_line(event));
        }
        Ok(CommandOutput::new(
            format!("watchpoint {}", trigger_id.raw()),
            lines,
        ))
    }

    fn add_watchpoint_output(&mut self, spec: WatchpointSpec) -> SwatResult<CommandOutput> {
        let session_id = self.runtime.require_session_id()?;
        let report = self
            .runtime
            .api()
            .add_watchpoint(session_id, spec.clone())?;
        Ok(CommandOutput::new(
            format!("added watchpoint {}", report.value.raw()),
            report
                .policy_events
                .iter()
                .map(format_event_line)
                .chain(std::iter::once(format!(
                    "watchpoint={} name={} value_key={} path={} after={} kind={} summary={}{} fire_once={}",
                    report.value.raw(),
                    spec.name,
                    spec.value_key,
                    spec.path.as_deref().unwrap_or("-"),
                    spec.after_millis
                        .map(|millis| millis.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    spec.event_kind
                        .map(|kind| format!("{kind:?}"))
                        .unwrap_or_else(|| "-".to_string()),
                    spec.summary_contains.as_deref().unwrap_or("-"),
                    spec.group
                        .as_ref()
                        .map(|group| format!(" group={group}"))
                        .unwrap_or_default(),
                    spec.fire_once
                )))
                .collect(),
        ))
    }

    fn show_help(&mut self, topic: Option<&str>) {
        let help = command_help(topic, CommandSurface::Tui);
        self.messages.clear();
        self.push_message(help.summary);
        let available = MAX_MESSAGES.saturating_sub(1);
        if help.lines.len() <= available {
            for line in help.lines {
                self.push_message(line);
            }
            return;
        }

        let shown = available.saturating_sub(1);
        for line in help.lines.iter().take(shown) {
            self.push_message(line.clone());
        }
        self.push_message(format!(
            "... {} more line(s)",
            help.lines.len().saturating_sub(shown)
        ));
    }

    fn show_help_search(&mut self, needle: &str) {
        let help = command_search(needle, CommandSurface::Tui);
        self.messages.clear();
        self.push_message(help.summary);
        for line in help.lines.into_iter().take(MAX_MESSAGES.saturating_sub(1)) {
            self.push_message(line);
        }
    }

    fn record_command(&mut self, command: &str) {
        if command.is_empty() {
            return;
        }
        if self
            .command_history
            .back()
            .is_some_and(|last| last.as_str() == command)
        {
            return;
        }
        self.command_history.push_back(command.to_string());
        while self.command_history.len() > DEFAULT_COMMAND_HISTORY_LIMIT {
            self.command_history.pop_front();
        }
        let entries = self.command_history.iter().cloned().collect::<Vec<_>>();
        match store_persisted_command_history(CommandSurface::Tui, &entries) {
            Ok(()) => self.history_persist_error_reported = false,
            Err(error) if !self.history_persist_error_reported => {
                self.history_persist_error_reported = true;
                self.push_message(format!("history save failed: {error}"));
            }
            Err(_) => {}
        }
    }

    fn recall_history(&mut self, step: isize) {
        if self.command_history.is_empty() {
            return;
        }

        let len = self.command_history.len() as isize;
        let next = match (self.command_history_index, step) {
            (None, -1) => len - 1,
            (None, _) => return,
            (Some(index), -1) => (index as isize - 1).max(0),
            (Some(index), 1) => {
                if index + 1 >= self.command_history.len() {
                    self.command_history_index = None;
                    self.command_input.clear();
                    self.reset_completion_state();
                    return;
                }
                index as isize + 1
            }
            (Some(index), _) => index as isize,
        };

        self.command_history_index = Some(next as usize);
        self.command_input = self.command_history[next as usize].clone();
        self.reset_completion_state();
    }

    fn complete_command_input(&mut self) {
        let reusing_matches = self
            .completion_matches
            .iter()
            .any(|candidate| candidate == &self.command_input);
        if !reusing_matches {
            let seed = self.command_input.clone();
            let matches = command_completions(&seed, CommandSurface::Tui);
            if matches.is_empty() {
                self.push_message("no tui command completions".to_string());
                self.reset_completion_state();
                return;
            }
            self.completion_matches = matches;
            self.completion_index = 0;
        } else if !self.completion_matches.is_empty() {
            self.completion_index = (self.completion_index + 1) % self.completion_matches.len();
        }

        if let Some(candidate) = self.completion_matches.get(self.completion_index).cloned() {
            self.command_input = candidate.clone();
            if self.completion_matches.len() > 1 {
                self.push_message(format!(
                    "completion {}/{}: {}",
                    self.completion_index + 1,
                    self.completion_matches.len(),
                    candidate
                ));
            }
        }
    }

    fn reset_completion_state(&mut self) {
        self.completion_matches.clear();
        self.completion_index = 0;
    }

    fn current_frame_index(&mut self) -> SwatResult<usize> {
        let Some(session_id) = self.runtime.session_id() else {
            return Err(SwatError::new("attach a target to inspect stack frames"));
        };
        let frames = self.runtime.inspector().stack_frames(session_id)?;
        if frames.is_empty() {
            self.selected_frame = 0;
            return Err(SwatError::new("no stack frames are available"));
        }
        if self.selected_frame >= frames.len() {
            self.selected_frame = frames.len() - 1;
        }
        Ok(self.selected_frame)
    }

    fn stack_lines(&mut self, limit: Option<usize>) -> SwatResult<Vec<String>> {
        let Some(session_id) = self.runtime.session_id() else {
            return Ok(vec![
                "attach a target to inspect the stack view".to_string(),
            ]);
        };
        let frames = self.runtime.inspector().stack_frames(session_id)?;
        if frames.is_empty() {
            self.selected_frame = 0;
        } else if self.selected_frame >= frames.len() {
            self.selected_frame = frames.len() - 1;
        }
        let mut lines = vec![format!("stack frames={}", frames.len())];
        lines.extend(frames.into_iter().take(limit.unwrap_or(6)).map(|frame| {
            let marker = if frame.frame_index == self.selected_frame {
                '*'
            } else {
                ' '
            };
            format!("{marker} {}", format_stack_frame_line(&frame))
        }));
        Ok(lines)
    }

    fn where_output(&mut self) -> SwatResult<CommandOutput> {
        let Some(session_id) = self.runtime.session_id() else {
            return Err(SwatError::new("attach a target to inspect stack frames"));
        };
        let frames = self.runtime.inspector().stack_frames(session_id)?;
        let current = self.current_frame_index()?;
        let frame = frames
            .iter()
            .find(|frame| frame.frame_index == current)
            .cloned()
            .ok_or_else(|| SwatError::new(format!("unknown stack frame {current}")))?;
        let mut lines = self.stack_lines(None)?;
        lines.push("source:".to_string());
        lines.extend(
            self.legacy_source_output(None, None, LEGACY_SLIST_BEFORE, LEGACY_SLIST_AFTER)?
                .lines,
        );
        Ok(CommandOutput::new(
            format!("where frame {} {}", frame.frame_index, frame.label),
            lines,
        ))
    }

    fn select_function(&mut self, name: Option<&str>) -> SwatResult<()> {
        let Some(session_id) = self.runtime.session_id() else {
            self.push_message("attach a target to inspect stack frames".to_string());
            self.clamp_selection()?;
            return Ok(());
        };
        let frames = self.runtime.inspector().stack_frames(session_id)?;
        if frames.is_empty() {
            return Err(SwatError::new("no stack frames are available"));
        }
        let frame = if let Some(name) = name {
            frames
                .iter()
                .find(|frame| {
                    frame
                        .function
                        .as_deref()
                        .map(|function| function == name)
                        .unwrap_or(false)
                        || frame.label.contains(name)
                })
                .cloned()
                .ok_or_else(|| SwatError::new(format!("function {name} is not active")))?
        } else {
            let current = self.current_frame_index()?;
            frames
                .iter()
                .find(|frame| frame.frame_index == current)
                .cloned()
                .ok_or_else(|| SwatError::new(format!("unknown stack frame {current}")))?
        };
        self.execute_command(&format!("stack frame {}", frame.frame_index))
    }

    fn move_frame_cursor(&mut self, count: usize, up: bool) -> SwatResult<()> {
        let Some(session_id) = self.runtime.session_id() else {
            self.push_message("attach a target to inspect stack frames".to_string());
            self.clamp_selection()?;
            return Ok(());
        };
        let frames = self.runtime.inspector().stack_frames(session_id)?;
        if frames.is_empty() {
            return Err(SwatError::new("no stack frames are available"));
        }
        let current = self.current_frame_index()?;
        let target = if up {
            current.saturating_add(count).min(frames.len() - 1)
        } else {
            current.saturating_sub(count)
        };
        self.execute_command(&format!("stack frame {target}"))
    }

    fn legacy_source_output(
        &mut self,
        file: Option<&str>,
        line: Option<usize>,
        before: usize,
        after: usize,
    ) -> SwatResult<CommandOutput> {
        if let Some(file) = file {
            if let Ok(snippet) =
                self.runtime
                    .inspector()
                    .source_file_view(file, line.unwrap_or(1), before, after)
            {
                return Ok(CommandOutput::new(
                    format!("source {}:{}", snippet.location.file, snippet.location.line),
                    render_tui_source_snippet(&snippet),
                ));
            }
        }

        let Some(session_id) = self.runtime.session_id() else {
            return Err(SwatError::new("attach a target to inspect source"));
        };
        let frames = self.runtime.inspector().stack_frames(session_id)?;
        let current = self.current_frame_index()?;
        let frame = frames
            .iter()
            .find(|frame| frame.frame_index == current)
            .cloned()
            .ok_or_else(|| SwatError::new(format!("unknown stack frame {current}")))?;
        if let Some(line) = line {
            let file = frame
                .source_file
                .as_deref()
                .ok_or_else(|| SwatError::new("current frame has no source file"))?;
            if let Ok(snippet) = self
                .runtime
                .inspector()
                .source_file_view(file, line, before, after)
            {
                return Ok(CommandOutput::new(
                    format!("source {}:{}", snippet.location.file, snippet.location.line),
                    render_tui_source_snippet(&snippet),
                ));
            }
        }
        let event = self
            .runtime
            .inspector()
            .event_by_id(frame.last_event_id)
            .ok_or_else(|| {
                SwatError::new(format!("unknown event {}", frame.last_event_id.raw()))
            })?;
        let inspection = self
            .runtime
            .inspector()
            .source_inspection(&event, before, after)?;
        if let Some(snippet) = inspection.snippet {
            return Ok(CommandOutput::new(
                format!("source {}:{}", snippet.location.file, snippet.location.line),
                render_tui_source_snippet(&snippet),
            ));
        }
        let mut lines = vec!["source unavailable".to_string()];
        if let Some(failure) = inspection.failure {
            lines.push(format!("failure_kind={:?}", failure.kind));
            lines.push(format!("failure={}", failure.message));
        }
        Ok(CommandOutput::new(
            format!("source frame {}", frame.frame_index),
            lines,
        ))
    }

    fn resolve_default_patient(&self, patient: Option<&str>) -> SwatResult<String> {
        patient
            .filter(|patient| !patient.is_empty())
            .map(ToString::to_string)
            .or_else(|| self.default_patient.clone())
            .ok_or_else(|| SwatError::new("no patient specified and no patient-default is set"))
    }

    fn run_until_expr(&mut self, label: &str, expr: &str) -> SwatResult<()> {
        let session_id = self.runtime.require_session_id()?;
        let session =
            self.runtime.manager.session(session_id).ok_or_else(|| {
                SwatError::new("active session is missing from the session manager")
            })?;
        if !session.capabilities.can_resume {
            return Err(SwatError::new("active target cannot resume execution"));
        }

        let trigger = Trigger::new(
            format!("{label} {expr}"),
            TriggerPredicate::Expr(parse_expression(expr)?),
            vec![TriggerAction::PauseTarget],
        )
        .fire_once();
        let trigger_id = trigger.trigger_id;

        let _ = self.runtime.api().add_trigger(session_id, trigger)?;
        let resume = self
            .runtime
            .api()
            .control(session_id, ControlAction::Resume)?;
        if !resume.value.response.accepted {
            let _ = self.runtime.api().remove_trigger(session_id, trigger_id);
            return Err(SwatError::new(resume.value.response.summary));
        }

        self.push_message(format!("expr={expr}"));
        let mut pumps = 0usize;
        let matched = loop {
            let report = pump_with_triggers(
                &mut self.runtime.manager,
                session_id,
                self.runtime.adapter.as_mut(),
                self.runtime.store.as_mut(),
                &mut self.runtime.trigger_engine,
            )?;
            pumps += 1;
            if report
                .trigger_matches
                .iter()
                .any(|trigger_match| trigger_match.trigger_id == trigger_id)
            {
                break true;
            }
            if report
                .pump_report
                .stored_events
                .iter()
                .any(event_looks_like_target_exit)
            {
                break false;
            }
            if report_paused_target(&report) {
                let resume = self
                    .runtime
                    .api()
                    .control(session_id, ControlAction::Resume)?;
                if !resume.value.response.accepted {
                    let _ = self.runtime.api().remove_trigger(session_id, trigger_id);
                    return Err(SwatError::new(resume.value.response.summary));
                }
            }
            if report.pump_report.stored_events.is_empty() {
                std::thread::sleep(UNTIL_POLL_DELAY);
            }
        };
        let _ = self.runtime.api().remove_trigger(session_id, trigger_id);
        self.push_message(if matched {
            format!("{label} matched after {pumps} pump(s)")
        } else {
            format!("{label} stopped after target exit without a match")
        });
        Ok(())
    }

    fn select_event(&mut self, event_id: EventId) -> SwatResult<()> {
        self.filter = EventFilter::All;
        self.manual_source = None;
        let events = self.runtime.session_events(&self.filter)?;
        if let Some(index) = events.iter().position(|event| event.event_id == event_id) {
            self.selected_event = index;
            self.push_message(format!("selected event {}", event_id.raw()));
        } else {
            self.push_message(format!("unknown event {}", event_id.raw()));
        }
        Ok(())
    }

    fn clamp_selection(&mut self) -> SwatResult<()> {
        let len = self.runtime.session_events(&self.filter)?.len();
        if len == 0 {
            self.selected_event = 0;
        } else if self.selected_event >= len {
            self.selected_event = len - 1;
        }
        Ok(())
    }

    fn push_message(&mut self, message: String) {
        if message.is_empty() {
            return;
        }
        if self.messages.len() == MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
    }
}

struct ManualSourceView {
    lines: Vec<String>,
}

fn format_stack_frame_line(frame: &StackFrame) -> String {
    let mut line = format!(
        "frame={} depth={} boundary={} kind={:?} label={}",
        frame.frame_index,
        frame.depth,
        frame.boundary_id.raw(),
        frame.event_kind,
        frame.label
    );
    if let Some(source) = short_stack_source(frame) {
        line.push_str(&format!(" source={source}"));
    }
    line
}

fn short_stack_source(frame: &StackFrame) -> Option<String> {
    let file = frame.source_file.as_deref()?;
    match frame.source_line {
        Some(line) => Some(format!("{file}:{line}")),
        None => Some(file.to_string()),
    }
}

fn render_tui_source_snippet(snippet: &SourceSnippet) -> Vec<String> {
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
    lines
}

fn escape_query_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn event_looks_like_target_exit(event: &EventEnvelope) -> bool {
    matches!(
        &event.payload,
        swat_core::EventPayload::Text { summary }
            if event.kind == EventKind::Lifecycle && summary.contains("exited")
    )
}

fn report_paused_target(report: &swat_control::ControlledPumpReport) -> bool {
    report.control_reports.iter().any(|control_report| {
        control_report.stored_events.iter().any(|event| {
            matches!(
                &event.payload,
                swat_core::EventPayload::Control {
                    action: ControlAction::Pause,
                    ..
                }
            )
        })
    })
}

#[derive(Default)]
struct ViewPane {
    title: String,
    lines: Vec<String>,
}

impl ViewPane {
    fn new(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
        }
    }
}

#[derive(Default)]
struct ViewSnapshot {
    session_label: String,
    filter_label: String,
    dashboard_label: String,
    selected_index: usize,
    events: Vec<String>,
    pane_one: ViewPane,
    pane_two: ViewPane,
    pane_three: ViewPane,
    message_lines: Vec<String>,
}

pub fn run(config: TuiConfig) -> SwatResult<()> {
    if config.headless {
        let rendered = run_headless(config)?;
        println!("{rendered}");
        return Ok(());
    }

    let mut app = TuiApp::new(&config)?;
    app.attach()?;

    enable_raw_mode().map_err(|err| SwatError::new(format!("failed to enable raw mode: {err}")))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|err| SwatError::new(format!("failed to enter alternate screen: {err}")))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)
        .map_err(|err| SwatError::new(format!("failed to create terminal: {err}")))?;

    let mut last_tick = Instant::now();
    let result = loop {
        terminal
            .draw(|frame| app.render(frame))
            .map_err(|err| SwatError::new(format!("failed to draw tui: {err}")))?;

        let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)
            .map_err(|err| SwatError::new(format!("failed to poll terminal events: {err}")))?
        {
            if let Event::Key(key) = event::read()
                .map_err(|err| SwatError::new(format!("failed to read terminal event: {err}")))?
            {
                if app.handle_key(key)? {
                    break Ok(());
                }
            }
        }

        if last_tick.elapsed() >= TICK_RATE {
            app.on_tick()?;
            last_tick = Instant::now();
        }
    };

    disable_raw_mode()
        .map_err(|err| SwatError::new(format!("failed to disable raw mode: {err}")))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|err| SwatError::new(format!("failed to leave alternate screen: {err}")))?;
    terminal
        .show_cursor()
        .map_err(|err| SwatError::new(format!("failed to restore cursor: {err}")))?;

    result
}

pub fn run_headless(config: TuiConfig) -> SwatResult<String> {
    let mut app = TuiApp::new(&config)?;
    app.attach()?;
    if matches!(config.mode, Mode::Mock) {
        app.resume()?;
    }
    for _ in 0..config.headless_ticks {
        app.on_tick()?;
        std::thread::sleep(Duration::from_millis(25));
    }

    let backend = TestBackend::new(140, 42);
    let mut terminal = ratatui::Terminal::new(backend)
        .map_err(|err| SwatError::new(format!("failed to create headless backend: {err}")))?;
    terminal
        .draw(|frame| app.render(frame))
        .map_err(|err| SwatError::new(format!("failed to render headless tui: {err}")))?;
    Ok(buffer_to_string(terminal.backend().buffer()))
}

pub fn build_adapter(mode: &Mode) -> SwatResult<Box<dyn TargetAdapter>> {
    match mode {
        Mode::Mock => Ok(Box::new(MockAdapter::default())),
        Mode::Local { program, args } => Ok(Box::new(LocalProcessAdapter::new(
            LocalProcessSpec::new(program.clone()).with_args(args.clone()),
        ))),
        Mode::Agent { program, args } => Ok(Box::new(AgentRuntimeAdapter::new(
            AgentRuntimeSpec::new(program.clone()).with_args(args.clone()),
        ))),
        Mode::PcGeos { fixture_path } => {
            let adapter = match fixture_path {
                Some(path) => PcGeosAdapter::from_fixture_path(path)?,
                None => PcGeosAdapter::from_fixture_path(bundled_fixture_path())?,
            };
            Ok(Box::new(adapter))
        }
    }
}

pub fn build_store(path: Option<&str>) -> SwatResult<Box<dyn SwatStore>> {
    match path {
        Some(path) => Ok(Box::new(FileStore::open(path.to_string())?)),
        None => Ok(Box::new(InMemoryStore::new())),
    }
}

fn format_event_line(event: &EventEnvelope) -> String {
    format!(
        "#{:>4} {:<16} {}",
        event.sequence_no,
        format!("{:?}", event.kind),
        payload_summary(event)
    )
}

fn format_tui_replay_plan_lines(plan: &swat_replay::ReplayPlan) -> Vec<String> {
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

fn format_tui_breakpoint_summary(breakpoint: &swat_api::BreakpointSummary) -> String {
    format!(
        "bp={} state={} hits={} name={}{} predicate={} actions={}",
        breakpoint.trigger_id.raw(),
        breakpoint.state.label(),
        breakpoint.hit_count,
        breakpoint.name,
        breakpoint
            .group
            .as_ref()
            .map(|group| format!(" group={group}"))
            .unwrap_or_default(),
        breakpoint.predicate_name.as_deref().unwrap_or("inline"),
        breakpoint.actions.join(","),
    )
}

fn format_tui_watchpoint_summary(watchpoint: &swat_api::WatchpointSummary) -> String {
    let breakpoint = &watchpoint.breakpoint;
    format!(
        "wp={} state={} hits={} name={}{} value_key={} path={} after={} kind={} summary={}",
        breakpoint.trigger_id.raw(),
        breakpoint.state.label(),
        breakpoint.hit_count,
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
    )
}

fn format_tui_frame_local(local: &swat_api::FrameLocal) -> String {
    format!(
        "local={} type={} kind={} preview={}",
        local.name,
        local.type_name.as_deref().unwrap_or("-"),
        local.value_kind.label(),
        local.preview
    )
}

fn format_tui_frame_register(register: &swat_api::FrameRegister) -> String {
    format!(
        "register={} group={} type={} kind={} preview={}",
        register.name,
        register.group.as_deref().unwrap_or("-"),
        register.type_name.as_deref().unwrap_or("-"),
        register.value_kind.label(),
        register.preview
    )
}

fn format_tui_patient_summary(patient: &PatientSummary) -> String {
    format!(
        "patient={} handles={} resources={} objects={} status={} runtime={}",
        patient.name,
        patient.handle_count,
        patient.resource_count,
        patient.object_count,
        patient.status.as_deref().unwrap_or("-"),
        patient.runtime.as_deref().unwrap_or("-")
    )
}

fn format_tui_patient_detail(detail: &swat_api::PatientDetail) -> Vec<String> {
    vec![
        format!("patient={}", detail.patient.name),
        format!("role={}", detail.patient.role.as_deref().unwrap_or("-")),
        format!("status={}", detail.patient.status.as_deref().unwrap_or("-")),
        format!(
            "runtime={}",
            detail.patient.runtime.as_deref().unwrap_or("-")
        ),
        format!("path={}", detail.patient.path.as_deref().unwrap_or("-")),
        format!("handles={}", join_tui_values(&detail.handles)),
        format!("resources={}", join_tui_values(&detail.resources)),
        format!("objects={}", join_tui_values(&detail.objects)),
        format!("sources={}", join_tui_values(&detail.source_files)),
    ]
}

fn format_tui_handle_summary(handle: &HandleSummary) -> String {
    format!(
        "handle={} patient={} resource={} state={} objects={}",
        handle.key,
        handle.patient.as_deref().unwrap_or("-"),
        handle.resource.as_deref().unwrap_or("-"),
        join_tui_values(&handle.state_flags),
        handle.object_count
    )
}

fn format_tui_handle_detail(detail: &swat_api::HandleDetail) -> Vec<String> {
    vec![
        format!("handle={}", detail.handle.key),
        format!(
            "patient={}",
            detail.handle.patient.as_deref().unwrap_or("-")
        ),
        format!(
            "resource={}",
            detail.handle.resource.as_deref().unwrap_or("-")
        ),
        format!("kind={}", detail.handle.kind.as_deref().unwrap_or("-")),
        format!(
            "address={}",
            detail.handle.address.as_deref().unwrap_or("-")
        ),
        format!(
            "size={}",
            detail
                .handle
                .size
                .map(|size| size.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "attached={}",
            detail
                .handle
                .attached
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
        format!("state={}", join_tui_values(&detail.handle.state_flags)),
        format!("objects={}", join_tui_values(&detail.objects)),
        format!("sources={}", join_tui_values(&detail.source_files)),
    ]
}

fn format_tui_resource_summary(resource: &ResourceSummary) -> String {
    format!(
        "resource={} patient={} handle={} kind={} objects={}",
        resource.name,
        resource.patient.as_deref().unwrap_or("-"),
        resource.handle.as_deref().unwrap_or("-"),
        resource.kind.as_deref().unwrap_or("-"),
        resource.object_count
    )
}

fn format_tui_resource_detail(detail: &swat_api::ResourceDetail) -> Vec<String> {
    vec![
        format!("resource={}", detail.resource.name),
        format!(
            "patient={}",
            detail.resource.patient.as_deref().unwrap_or("-")
        ),
        format!(
            "handle={}",
            detail.resource.handle.as_deref().unwrap_or("-")
        ),
        format!("kind={}", detail.resource.kind.as_deref().unwrap_or("-")),
        format!(
            "source_file={}",
            detail.resource.source_file.as_deref().unwrap_or("-")
        ),
        format!("objects={}", join_tui_values(&detail.objects)),
        format!("sources={}", join_tui_values(&detail.source_files)),
    ]
}

fn format_tui_object_summary(object: &ObjectSummary) -> String {
    format!(
        "object={} class={} patient={} handle={} resource={}",
        object.key,
        object.class_name.as_deref().unwrap_or("-"),
        object.patient.as_deref().unwrap_or("-"),
        object.handle.as_deref().unwrap_or("-"),
        object.resource.as_deref().unwrap_or("-")
    )
}

fn format_tui_object_detail(detail: &swat_api::ObjectDetail) -> Vec<String> {
    vec![
        format!("object={}", detail.object.key),
        format!(
            "class={}",
            detail.object.class_name.as_deref().unwrap_or("-")
        ),
        format!(
            "patient={}",
            detail.object.patient.as_deref().unwrap_or("-")
        ),
        format!("handle={}", detail.object.handle.as_deref().unwrap_or("-")),
        format!(
            "resource={}",
            detail.object.resource.as_deref().unwrap_or("-")
        ),
        format!(
            "address={}",
            detail.object.address.as_deref().unwrap_or("-")
        ),
        format!("state={}", join_tui_values(&detail.object.state_flags)),
        format!("sources={}", join_tui_values(&detail.source_files)),
    ]
}

fn format_tui_observed_value_summary(value: &ObservedValueSummary) -> String {
    format!(
        "value_key={} events={} last_seq={} preview={}",
        value.value_key,
        value.event_count,
        value
            .last_sequence_no
            .map(|sequence_no| sequence_no.to_string())
            .unwrap_or_else(|| "-".to_string()),
        value.preview.as_deref().unwrap_or("-")
    )
}

fn format_tui_observed_value_detail(detail: &swat_api::ObservedValueDetail) -> Vec<String> {
    let mut lines = vec![
        format!("value_key={}", detail.value.value_key),
        format!("events={}", detail.value.event_count),
        format!(
            "last_summary={}",
            detail.value.last_summary.as_deref().unwrap_or("-")
        ),
        format!("preview={}", detail.value.preview.as_deref().unwrap_or("-")),
        "history:".to_string(),
    ];
    lines.extend(detail.history.iter().map(|sample| {
        format!(
            "event={} seq={} summary={} preview={}",
            sample.event_id.raw(),
            sample.sequence_no,
            sample.summary,
            sample.preview.as_deref().unwrap_or("-")
        )
    }));
    lines
}

fn format_tui_source_function_summary(function: &SourceFunctionSummary) -> String {
    let line_range = match (function.first_line, function.last_line) {
        (Some(first), Some(last)) => format!("{first}..{last}"),
        _ => "-".to_string(),
    };
    format!(
        "function={} file={} events={} lines={}",
        function.function, function.file, function.event_count, line_range
    )
}

fn format_tui_event_patient(patient: &swat_value::PatientArtifactRecord) -> String {
    format!(
        "patient={} handles={} resources={} objects={}",
        patient.name,
        patient.handle_ids.len(),
        patient.resource_names.len(),
        patient.object_ids.len()
    )
}

fn format_tui_event_handle(handle: &swat_value::HandleArtifactRecord) -> String {
    format!(
        "handle={} patient={} resource={} state={}",
        handle.key,
        handle.patient.as_deref().unwrap_or("-"),
        handle.resource.as_deref().unwrap_or("-"),
        join_tui_values(&handle.state_flags)
    )
}

fn format_tui_event_resource(resource: &swat_value::ResourceArtifactRecord) -> String {
    format!(
        "resource={} patient={} handle={} objects={}",
        resource.name,
        resource.patient.as_deref().unwrap_or("-"),
        resource.handle.as_deref().unwrap_or("-"),
        resource.object_ids.len()
    )
}

fn format_tui_event_object(object: &swat_value::ObjectArtifactRecord) -> String {
    format!(
        "object={} class={} handle={} resource={}",
        object.key,
        object.class_name.as_deref().unwrap_or("-"),
        object.handle.as_deref().unwrap_or("-"),
        object.resource.as_deref().unwrap_or("-")
    )
}

fn join_tui_values(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(",")
    }
}

fn with_summary_line(summary: String, lines: Vec<String>) -> Vec<String> {
    let mut combined = vec![summary];
    combined.extend(lines);
    combined
}

fn payload_summary(event: &EventEnvelope) -> String {
    match &event.payload {
        swat_core::EventPayload::Empty => "<empty>".to_string(),
        swat_core::EventPayload::Text { summary }
        | swat_core::EventPayload::Control { summary, .. }
        | swat_core::EventPayload::Boundary { summary, .. }
        | swat_core::EventPayload::Snapshot { summary, .. }
        | swat_core::EventPayload::Trigger { summary, .. }
        | swat_core::EventPayload::Value { summary, .. }
        | swat_core::EventPayload::Policy { summary, .. } => summary.clone(),
    }
}

fn pane_block(title: &str, accent: bool) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if accent {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        })
}

fn render_lines<'a>(title: &'a str, lines: &'a [String]) -> Paragraph<'a> {
    let text = if lines.is_empty() {
        vec![Line::from(String::new())]
    } else {
        lines.iter().cloned().map(Line::from).collect()
    };
    Paragraph::new(text)
        .block(pane_block(title, false))
        .wrap(Wrap { trim: false })
}

fn buffer_to_string(buffer: &Buffer) -> String {
    let area = buffer.area();
    let mut lines = Vec::new();
    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        lines.push(line.trim_end().to_string());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn tui_command_entry_uses_shared_help_and_stack_commands() {
        let mut app = TuiApp::new(&TuiConfig::new(Mode::Mock)).unwrap();
        app.attach().unwrap();
        app.resume().unwrap();
        app.on_tick().unwrap();
        app.on_tick().unwrap();

        app.execute_command("help breakpoint").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("semantic breakpoints"))
        );
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("breakpoint list"))
        );

        app.execute_command("stack").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("stack frames="))
        );

        app.execute_command("backtrace 1").unwrap();
        assert!(app.messages.iter().any(|line| line.contains("frame=0")));

        app.execute_command("locals").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("stack frame 0 locals"))
        );

        app.execute_command("stack locals 0").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("stack frame 0 locals"))
        );

        app.execute_command("help patient").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("patient show <name>"))
        );
    }

    #[test]
    fn tui_command_entry_can_manage_breakpoints_and_watchpoints() {
        let mut app = TuiApp::new(&TuiConfig::new(Mode::Mock)).unwrap();
        app.attach().unwrap();

        app.execute_command(
            r#"breakpoint predicate add search_tool kind == ModelBoundary and artifact.json $.tool == "search""#,
        )
        .unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("defined breakpoint predicate search_tool"))
        );

        app.execute_command("breakpoint add pause_search group=search @search_tool")
            .unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("added breakpoint"))
        );

        app.execute_command(
            "watchpoint add cache_turn agent.state after=25 kind=Lifecycle summary=loaded",
        )
        .unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("added watchpoint"))
        );

        app.execute_command("watchpoint list").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("watchpoint(s)"))
        );
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("value_key=agent.state"))
        );
    }

    #[test]
    fn tui_command_entry_can_view_source_files_directly() {
        let path = std::env::temp_dir().join(format!(
            "swat-ui-source-view-{}-{}.py",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            "def alpha():\n    return 1\n\ndef beta():\n    return alpha()\n",
        )
        .unwrap();

        let mut app = TuiApp::new(&TuiConfig::new(Mode::Mock)).unwrap();
        app.execute_command(&format!("source view {} 4 1 1", path.display()))
            .unwrap();

        let lines = app.source_lines(None).unwrap();
        assert!(lines.iter().any(|line| line.contains("def beta")));

        app.execute_command(&format!("view {} 4", path.display()))
            .unwrap();
        let lines = app.source_lines(None).unwrap();
        assert!(lines.iter().any(|line| line.contains("def beta")));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn tui_command_entry_can_show_typed_patient_handle_and_object_views() {
        let code = r#"
import json
import sys
import time

PREFIX = "__SWATAGENT__"

def emit(record):
    sys.stdout.write(PREFIX + json.dumps(record) + "\n")
    sys.stdout.flush()

emit({"kind": "tool", "phase": "start", "span_id": "tool-1", "correlation_id": "req-11", "name": "web_search", "summary": "tool started", "file": "/tmp/agent.py", "line": 14, "function": "run", "patient": {"name": "ui", "handles": [{"id": "h:1001", "resource": "AppResource", "objects": [{"id": "^lui:0002", "class": "GenApplication"}]}], "resources": [{"name": "AppResource", "handle": "h:1001", "objects": ["^lui:0002"]}], "objects": [{"id": "^lui:0002", "class": "GenApplication", "handle": "h:1001", "resource": "AppResource"}]}})
emit({"kind": "tool", "phase": "end", "span_id": "tool-1", "correlation_id": "req-11", "name": "web_search", "summary": "tool completed", "file": "/tmp/agent.py", "line": 18, "function": "run", "handle": {"id": "h:1001", "patient": "ui", "resource": "AppResource", "attached": True}})
emit({"kind": "state", "phase": "update", "name": "memory.turn", "summary": "memory updated"})
time.sleep(0.1)
"#;

        let mut app = TuiApp::new(&TuiConfig::new(Mode::Agent {
            program: "python3".to_string(),
            args: vec!["-u".to_string(), "-c".to_string(), code.to_string()],
        }))
        .unwrap();
        app.attach().unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            app.on_tick().unwrap();
            if app
                .messages
                .iter()
                .any(|line| line.contains("agent runtime exited"))
            {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }

        app.execute_command("patient").unwrap();
        assert!(app.messages.iter().any(|line| line.contains("patient=ui")));

        app.execute_command("patient-default ui").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("patient-default=ui"))
        );

        app.execute_command("handle show h:1001").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("attached=true"))
        );

        app.execute_command("object show ^lui:0002").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("class=GenApplication"))
        );

        app.execute_command("obj-name ^lui:0002").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("GenApplication"))
        );

        app.execute_command("obj-class ^lui:0002").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("class=GenApplication"))
        );

        app.execute_command("value").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("value_key=agent.state"))
        );

        app.execute_command("source functions").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("function=run"))
        );
    }

    #[test]
    fn tui_command_entry_can_inspect_pcgeos_fixture_state() {
        let show_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("Appl/GeoPoint/show.goc")
            .canonicalize()
            .unwrap();

        let mut app = TuiApp::new(&TuiConfig::new(Mode::PcGeos { fixture_path: None })).unwrap();
        app.attach().unwrap();

        app.execute_command("patient show geopoint").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("patient=geopoint"))
        );

        app.execute_command("stack").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("GeoPointApp::OpenDocument"))
        );

        app.execute_command("stack registers 0").unwrap();
        assert!(app.messages.iter().any(|line| line.contains("register=ax")));

        app.resume().unwrap();
        app.on_tick().unwrap();

        app.execute_command("value show pcgeos.memory.slide:0x0020")
            .unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("captured slide view state"))
        );

        app.execute_command("source file /home/ubuntu/pcgeos/Appl/GeoPoint/show.goc")
            .unwrap();
        let lines = app.source_lines(None).unwrap();
        assert!(!lines.is_empty());
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains(show_path.display().to_string().as_str()))
        );
    }

    #[test]
    fn tui_dashboard_switches_between_execution_control_and_target_views() {
        let mut app = TuiApp::new(&TuiConfig::new(Mode::PcGeos { fixture_path: None })).unwrap();
        app.attach().unwrap();

        let execution = app.snapshot().unwrap();
        assert_eq!(execution.dashboard_label, "execution");
        assert_eq!(execution.pane_one.title, "Stack / Entities");

        app.execute_command("dashboard control").unwrap();
        let control = app.snapshot().unwrap();
        assert_eq!(control.dashboard_label, "control");
        assert_eq!(control.pane_one.title, "Breakpoints");
        assert!(
            control
                .pane_three
                .lines
                .iter()
                .any(|line| line.contains("current-boundary="))
        );

        app.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE))
            .unwrap();
        let target = app.snapshot().unwrap();
        assert_eq!(target.dashboard_label, "target");
        assert_eq!(target.pane_two.title, "Patients / Handles / Objects");
        assert!(
            target
                .pane_two
                .lines
                .iter()
                .any(|line| line.contains("patient=geopoint"))
        );
        assert!(
            target
                .pane_three
                .lines
                .iter()
                .any(|line| line.contains("show.goc"))
        );
    }

    #[test]
    fn tui_command_entry_supports_completion_and_history_navigation() {
        let mut app = TuiApp::new(&TuiConfig::new(Mode::Mock)).unwrap();

        app.command_mode = true;
        app.command_input = "help br".to_string();
        app.handle_command_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.command_input, "help breakpoint");

        app.handle_command_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        app.command_mode = true;
        app.command_input = "help source".to_string();
        app.handle_command_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();

        app.command_mode = true;
        app.handle_command_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.command_input, "help source");

        app.handle_command_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.command_input, "help breakpoint");

        app.handle_command_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.command_input, "help source");

        app.execute_command("history 2").unwrap();
        assert!(app.messages.iter().any(|line| line.contains("help source")));
    }
}
