#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use swat_agent_protocol::{
    AgentEventKind, AgentEventRecord, DEFAULT_TRACE_PREFIX,
    parse_validated_prefixed_line_with_prefix,
};
use swat_core::{
    AdapterAttachment, AdapterControlResult, AdapterEmission, ArtifactAccess, ArtifactAlias,
    ArtifactBinding, ArtifactEncoding, BoundaryId, BoundaryReplayDirective, CapabilitySet,
    CausalityLink, ControlAction, ControlResponse, DeterminismClass, EventKind, EventPayload,
    PendingArtifact, PendingEvent, PolicyVerdict, ReplayMode, SwatError, SwatResult, TargetAdapter,
    TargetDescriptor, TargetId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRuntimeSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub event_prefix: String,
}

impl AgentRuntimeSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            event_prefix: DEFAULT_TRACE_PREFIX.to_string(),
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn with_event_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.event_prefix = prefix.into();
        self
    }

    pub fn command_summary(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    fn name(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug)]
enum OutputItem {
    Structured(AgentEventRecord),
    ProtocolError { line: String, error: String },
    Text(OutputStream, String),
}

pub struct AgentRuntimeAdapter {
    spec: AgentRuntimeSpec,
    descriptor: TargetDescriptor,
    child: Option<Child>,
    output_rx: Option<Receiver<OutputItem>>,
    attached: bool,
    paused: bool,
    exit_emitted: bool,
    next_alias_raw: u64,
    next_boundary_raw: u64,
    active_boundaries: BTreeMap<String, BoundaryId>,
}

impl AgentRuntimeAdapter {
    pub fn new(spec: AgentRuntimeSpec) -> Self {
        let target_name = spec.command_summary();
        Self {
            spec,
            descriptor: TargetDescriptor {
                target_id: TargetId::new(),
                adapter_name: "swat-adapter-agent".to_string(),
                target_name,
                runtime: "agent-runtime".to_string(),
                replay_mode: ReplayMode::Live,
            },
            child: None,
            output_rx: None,
            attached: false,
            paused: false,
            exit_emitted: false,
            next_alias_raw: 1,
            next_boundary_raw: 1,
            active_boundaries: BTreeMap::new(),
        }
    }

    fn ensure_attached(&self) -> SwatResult<()> {
        if self.attached {
            Ok(())
        } else {
            Err(SwatError::new("agent runtime adapter is not attached"))
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.spec.program);
        command.args(&self.spec.args);
        if let Some(cwd) = &self.spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &self.spec.env {
            command.env(key, value);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command
    }

    fn next_alias(&mut self) -> ArtifactAlias {
        let alias = ArtifactAlias::from_raw(self.next_alias_raw);
        self.next_alias_raw += 1;
        alias
    }

    fn next_boundary(&mut self) -> BoundaryId {
        let boundary_id = BoundaryId::from_raw(self.next_boundary_raw);
        self.next_boundary_raw += 1;
        boundary_id
    }

    fn child_pid(&self) -> SwatResult<u32> {
        self.child
            .as_ref()
            .map(Child::id)
            .ok_or_else(|| SwatError::new("agent runtime child is not running"))
    }

    fn signal_child(&self, signal: Signal) -> SwatResult<()> {
        let pid = self.child_pid()?;
        kill(Pid::from_raw(pid as i32), signal)
            .map_err(|err| SwatError::new(format!("failed to signal agent runtime {pid}: {err}")))
    }

    fn spawn_stdout_reader(stdout: ChildStdout, tx: Sender<OutputItem>, prefix: String) {
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if line.starts_with(&prefix) {
                    match parse_validated_prefixed_line_with_prefix(&line, &prefix) {
                        Some(Ok(record)) => {
                            if tx.send(OutputItem::Structured(record)).is_err() {
                                break;
                            }
                        }
                        Some(Err(error)) => {
                            if tx
                                .send(OutputItem::ProtocolError {
                                    line,
                                    error: error.to_string(),
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        None => {}
                    }
                } else if tx
                    .send(OutputItem::Text(OutputStream::Stdout, line))
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    fn spawn_stderr_reader(stderr: ChildStderr, tx: Sender<OutputItem>) {
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if tx
                    .send(OutputItem::Text(OutputStream::Stderr, line))
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    fn attach_readers(&mut self, stdout: ChildStdout, stderr: ChildStderr) {
        let (tx, rx) = mpsc::channel();
        Self::spawn_stdout_reader(stdout, tx.clone(), self.spec.event_prefix.clone());
        Self::spawn_stderr_reader(stderr, tx);
        self.output_rx = Some(rx);
    }

    fn drain_output(&mut self, emission: &mut AdapterEmission) {
        let mut drained = Vec::new();
        if let Some(rx) = &self.output_rx {
            while let Ok(item) = rx.try_recv() {
                drained.push(item);
            }
        }

        for item in drained {
            match item {
                OutputItem::Structured(record) => self.emit_structured_record(record, emission),
                OutputItem::ProtocolError { line, error } => {
                    self.emit_protocol_error(line, &error, emission)
                }
                OutputItem::Text(stream, text) => self.emit_text_record(stream, text, emission),
            }
        }
    }

    fn emit_structured_record(&mut self, record: AgentEventRecord, emission: &mut AdapterEmission) {
        let alias = self.next_alias();
        let summary = agent_record_summary(&record);
        let kind = agent_event_kind(&record);
        let payload = self.agent_payload_for_record(&record, &summary);
        let mut event =
            PendingEvent::new(kind, payload).with_artifact(ArtifactBinding::Pending(alias));
        event.causality = CausalityLink {
            parent_event_id: None,
            correlation_id: correlation_id(&record),
        };

        emission.pending_artifacts.push(PendingArtifact {
            alias,
            media_type: "application/json".to_string(),
            encoding: ArtifactEncoding::Json,
            access: ArtifactAccess::Lazy,
            bytes: serde_json::to_vec(&record).unwrap_or_else(|_| b"{}".to_vec()),
        });
        emission.pending_events.push(event);
    }

    fn emit_text_record(
        &mut self,
        stream: OutputStream,
        text: String,
        emission: &mut AdapterEmission,
    ) {
        let alias = self.next_alias();
        let value_key = format!("agent.{}", stream.name());
        let summary = format!("captured agent {} line", stream.name());
        emission.pending_artifacts.push(PendingArtifact {
            alias,
            media_type: "text/plain".to_string(),
            encoding: ArtifactEncoding::Utf8,
            access: ArtifactAccess::Lazy,
            bytes: text.into_bytes(),
        });
        emission.pending_events.push(
            PendingEvent::new(
                EventKind::ValueObserved,
                EventPayload::Value { value_key, summary },
            )
            .with_artifact(ArtifactBinding::Pending(alias)),
        );
    }

    fn emit_protocol_error(&mut self, line: String, error: &str, emission: &mut AdapterEmission) {
        let alias = self.next_alias();
        emission.pending_artifacts.push(PendingArtifact {
            alias,
            media_type: "text/plain".to_string(),
            encoding: ArtifactEncoding::Utf8,
            access: ArtifactAccess::Lazy,
            bytes: line.into_bytes(),
        });
        emission.pending_events.push(
            PendingEvent::new(
                EventKind::Lifecycle,
                EventPayload::Text {
                    summary: format!("agent protocol error: {error}"),
                },
            )
            .with_artifact(ArtifactBinding::Pending(alias)),
        );
    }

    fn agent_payload_for_record(
        &mut self,
        record: &AgentEventRecord,
        summary: &str,
    ) -> EventPayload {
        match agent_event_kind(record) {
            EventKind::ModelBoundary | EventKind::ToolBoundary => EventPayload::Boundary {
                boundary_id: self.boundary_id_for_record(record),
                determinism: determinism_for_record(record),
                summary: summary.to_string(),
            },
            EventKind::PolicyDecision => EventPayload::Policy {
                verdict: verdict_for_record(record),
                summary: summary.to_string(),
            },
            EventKind::Lifecycle => EventPayload::Text {
                summary: summary.to_string(),
            },
            EventKind::SourceResolution | EventKind::SchemaResolution => EventPayload::Value {
                value_key: format!("agent.{}", record.kind_name()),
                summary: summary.to_string(),
            },
            EventKind::Execution | EventKind::StateMutation | EventKind::ValueObserved => {
                EventPayload::Value {
                    value_key: value_key_for_record(record),
                    summary: summary.to_string(),
                }
            }
            EventKind::Control
            | EventKind::TriggerHit
            | EventKind::Snapshot
            | EventKind::Replay => EventPayload::Text {
                summary: summary.to_string(),
            },
        }
    }

    fn boundary_id_for_record(&mut self, record: &AgentEventRecord) -> BoundaryId {
        let Some(key) = boundary_key(record) else {
            return self.next_boundary();
        };

        if let Some(boundary_id) = self.active_boundaries.get(&key).copied() {
            if boundary_phase_is_terminal(record) {
                self.active_boundaries.remove(&key);
            }
            return boundary_id;
        }

        let boundary_id = self.next_boundary();
        if !boundary_phase_is_terminal(record) {
            self.active_boundaries.insert(key, boundary_id);
        }
        boundary_id
    }

    fn maybe_emit_exit(&mut self, emission: &mut AdapterEmission) -> SwatResult<()> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        if self.exit_emitted {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|err| SwatError::new(format!("failed to poll agent child status: {err}")))?
        {
            self.exit_emitted = true;
            self.paused = false;
            emission.pending_events.push(PendingEvent::new(
                EventKind::Lifecycle,
                EventPayload::Text {
                    summary: exit_summary(&status),
                },
            ));
        }
        Ok(())
    }
}

impl TargetAdapter for AgentRuntimeAdapter {
    fn adapter_name(&self) -> &'static str {
        "swat-adapter-agent"
    }

    fn attach(&mut self) -> SwatResult<AdapterAttachment> {
        if self.attached {
            return Err(SwatError::new("agent runtime adapter already attached"));
        }

        let mut child = self
            .command()
            .spawn()
            .map_err(|err| SwatError::new(format!("failed to spawn agent runtime: {err}")))?;
        let pid = child.id();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SwatError::new("agent runtime stdout pipe was not available"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SwatError::new("agent runtime stderr pipe was not available"))?;

        self.attach_readers(stdout, stderr);
        self.child = Some(child);
        self.attached = true;
        self.paused = false;
        self.exit_emitted = false;
        self.next_alias_raw = 1;
        self.next_boundary_raw = 1;
        self.active_boundaries.clear();

        Ok(AdapterAttachment {
            descriptor: self.descriptor.clone(),
            capabilities: self.capabilities(),
            initial_emission: AdapterEmission {
                pending_events: vec![PendingEvent::new(
                    EventKind::Lifecycle,
                    EventPayload::Text {
                        summary: format!(
                            "spawned agent runtime pid={} cmd={}",
                            pid,
                            self.spec.command_summary()
                        ),
                    },
                )],
                pending_artifacts: Vec::new(),
            },
        })
    }

    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet {
            can_attach: true,
            can_stream_events: true,
            can_stop: true,
            can_resume: true,
            can_step: false,
            can_read_values: true,
            can_write_values: false,
            can_snapshot: false,
            can_inject_replay: false,
            can_resolve_source: false,
            can_resolve_schema: false,
        }
    }

    fn poll(&mut self) -> SwatResult<AdapterEmission> {
        self.ensure_attached()?;
        let mut emission = AdapterEmission::default();
        self.drain_output(&mut emission);
        self.maybe_emit_exit(&mut emission)?;
        Ok(emission)
    }

    fn control(&mut self, action: ControlAction) -> SwatResult<AdapterControlResult> {
        self.ensure_attached()?;
        let mut emission = AdapterEmission::default();
        let response = if self.exit_emitted {
            ControlResponse {
                accepted: false,
                summary: "agent runtime already exited".to_string(),
            }
        } else {
            match &action {
                ControlAction::Pause => {
                    self.signal_child(Signal::SIGSTOP)?;
                    self.paused = true;
                    ControlResponse {
                        accepted: true,
                        summary: "agent runtime paused".to_string(),
                    }
                }
                ControlAction::Resume => {
                    self.signal_child(Signal::SIGCONT)?;
                    self.paused = false;
                    ControlResponse {
                        accepted: true,
                        summary: "agent runtime resumed".to_string(),
                    }
                }
                ControlAction::Step => ControlResponse {
                    accepted: false,
                    summary: "agent stepping is not supported yet".to_string(),
                },
                ControlAction::CreateSnapshot { .. } => ControlResponse {
                    accepted: false,
                    summary: "agent snapshots are not supported yet".to_string(),
                },
            }
        };

        emission.pending_events.push(PendingEvent::new(
            EventKind::Control,
            EventPayload::Control {
                action,
                summary: response.summary.clone(),
            },
        ));

        Ok(AdapterControlResult { response, emission })
    }

    fn inject_boundary_replay(
        &mut self,
        _directive: BoundaryReplayDirective,
    ) -> SwatResult<AdapterEmission> {
        self.ensure_attached()?;
        Err(SwatError::new(
            "agent runtime adapter does not support replay boundary injection",
        ))
    }
}

impl Drop for AgentRuntimeAdapter {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                Err(_) => {}
            }
        }
    }
}

fn correlation_id(record: &AgentEventRecord) -> Option<String> {
    record
        .correlation_id()
        .or_else(|| record.span_id())
        .map(ToString::to_string)
}

fn boundary_key(record: &AgentEventRecord) -> Option<String> {
    record.span_id().map(ToString::to_string)
}

fn boundary_phase_is_terminal(record: &AgentEventRecord) -> bool {
    record.is_terminal_phase()
}

fn determinism_for_record(record: &AgentEventRecord) -> DeterminismClass {
    match record.determinism() {
        Some("deterministic") => DeterminismClass::Deterministic,
        Some("replay_only" | "replay-only") => DeterminismClass::ReplayOnly,
        _ => DeterminismClass::ExternalBoundary,
    }
}

fn verdict_for_record(record: &AgentEventRecord) -> PolicyVerdict {
    match record.verdict().or_else(|| record.status()) {
        Some("deny" | "denied" | "block" | "blocked") => PolicyVerdict::Deny,
        Some("redact" | "redacted") => PolicyVerdict::Redact,
        _ => PolicyVerdict::Allow,
    }
}

fn value_key_for_record(record: &AgentEventRecord) -> String {
    match record.kind {
        AgentEventKind::Planner => "agent.planner".to_string(),
        AgentEventKind::State => "agent.state".to_string(),
        AgentEventKind::Source => "agent.source".to_string(),
        AgentEventKind::Schema => "agent.schema".to_string(),
        AgentEventKind::Lifecycle => "agent.lifecycle".to_string(),
        AgentEventKind::Log => "agent.log".to_string(),
        AgentEventKind::Model => "agent.model".to_string(),
        AgentEventKind::Tool => "agent.tool".to_string(),
        AgentEventKind::Policy => "agent.policy".to_string(),
    }
}

fn agent_event_kind(record: &AgentEventRecord) -> EventKind {
    match record.kind {
        AgentEventKind::Planner => EventKind::Execution,
        AgentEventKind::Model => EventKind::ModelBoundary,
        AgentEventKind::Tool => EventKind::ToolBoundary,
        AgentEventKind::State => EventKind::StateMutation,
        AgentEventKind::Policy => EventKind::PolicyDecision,
        AgentEventKind::Source => EventKind::SourceResolution,
        AgentEventKind::Schema => EventKind::SchemaResolution,
        AgentEventKind::Lifecycle => EventKind::Lifecycle,
        AgentEventKind::Log => EventKind::ValueObserved,
    }
}

fn agent_record_summary(record: &AgentEventRecord) -> String {
    if let Some(summary) = record.summary() {
        return summary.to_string();
    }

    let kind = record.kind_name();
    let phase = record.phase().unwrap_or("update");
    let name = record.name().unwrap_or("<unnamed>");
    let status = record.status();

    match status {
        Some(status) => format!("agent {kind} {phase} {name} status={status}"),
        None => format!("agent {kind} {phase} {name}"),
    }
}

fn exit_summary(status: &ExitStatus) -> String {
    if let Some(code) = status.code() {
        format!("agent runtime exited with code {code}")
    } else {
        "agent runtime exited without a code".to_string()
    }
}
