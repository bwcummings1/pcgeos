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
use swat_api::{LiveSessionApi, StackFrame, TraceInspector};
use swat_command::{
    Command, CommandSurface, command_completions, command_help, command_search, parse_command,
};
use swat_control::TriggerEngine;
use swat_core::{
    BoundaryId, ControlAction, EventEnvelope, EventId, EventKind, SessionId, SwatError, SwatResult,
    TargetAdapter,
};
use swat_session::SessionManager;
use swat_store::{FileStore, InMemoryStore, SwatStore};

const TICK_RATE: Duration = Duration::from_millis(100);
const MAX_MESSAGES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Mock,
    Local { program: String, args: Vec<String> },
    Agent { program: String, args: Vec<String> },
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
    command_mode: bool,
    command_input: String,
    command_history: VecDeque<String>,
    command_history_index: Option<usize>,
    completion_matches: Vec<String>,
    completion_index: usize,
    manual_source: Option<ManualSourceView>,
    messages: VecDeque<String>,
}

impl TuiApp {
    pub fn new(config: &TuiConfig) -> SwatResult<Self> {
        Ok(Self {
            runtime: LiveRuntime::new(
                build_adapter(&config.mode)?,
                build_store(config.store_path.as_deref())?,
            ),
            filter: EventFilter::All,
            selected_event: 0,
            command_mode: false,
            command_input: String::new(),
            command_history: VecDeque::new(),
            command_history_index: None,
            completion_matches: Vec::new(),
            completion_index: 0,
            manual_source: None,
            messages: VecDeque::from([
                "q quit | :help | Tab complete | Up/Down history | a attach | u pump | r resume | p pause | s step".to_string(),
            ]),
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
            render_lines("Stack / Entities", &snapshot.entity_lines),
            right[0],
        );
        frame.render_widget(render_lines("Source", &snapshot.source_lines), right[1]);
        frame.render_widget(
            render_lines("Artifacts", &snapshot.artifact_lines),
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

        Ok(ViewSnapshot {
            session_label: self.runtime.session_label(),
            filter_label: self.filter.label(),
            selected_index: self.selected_event,
            events: event_lines,
            entity_lines: self.entity_lines(selected_event.as_ref())?,
            source_lines: self.source_lines(selected_event.as_ref())?,
            artifact_lines: self.artifact_lines(selected_event.as_ref())?,
            message_lines: self.messages.iter().cloned().collect(),
        })
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
            Command::Spans => {
                for line in self.stack_lines()? {
                    self.push_message(line);
                }
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
                self.filter = EventFilter::Boundary(frame.boundary_id);
                self.selected_event = 0;
                self.manual_source = None;
                self.push_message(format!(
                    "event filter=frame {} boundary {}",
                    frame.frame_index,
                    frame.boundary_id.raw()
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
            other => {
                return Err(SwatError::new(format!(
                    "unsupported tui command '{input}' ({other:?}); use ':help' for shared discovery and the shell for shell-only workflows",
                )));
            }
        }
        self.clamp_selection()?;
        Ok(())
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
        while self.command_history.len() > 64 {
            self.command_history.pop_front();
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

    fn stack_lines(&self) -> SwatResult<Vec<String>> {
        let Some(session_id) = self.runtime.session_id() else {
            return Ok(vec![
                "attach a target to inspect the stack view".to_string(),
            ]);
        };
        let frames = self.runtime.inspector().stack_frames(session_id)?;
        let mut lines = vec![format!("stack frames={}", frames.len())];
        lines.extend(
            frames
                .into_iter()
                .take(6)
                .map(|frame| format_stack_frame_line(&frame)),
        );
        Ok(lines)
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

#[derive(Default)]
struct ViewSnapshot {
    session_label: String,
    filter_label: String,
    selected_index: usize,
    events: Vec<String>,
    entity_lines: Vec<String>,
    source_lines: Vec<String>,
    artifact_lines: Vec<String>,
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
        assert!(app.messages.iter().any(|line| line.contains("[shell]")));

        app.execute_command("stack").unwrap();
        assert!(
            app.messages
                .iter()
                .any(|line| line.contains("stack frames="))
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

        let _ = fs::remove_file(path);
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
    }
}
