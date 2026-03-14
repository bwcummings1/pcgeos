#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde::Deserialize;
use swat_core::{
    AdapterAttachment, AdapterControlResult, AdapterEmission, ArtifactAccess, ArtifactAlias,
    ArtifactBinding, ArtifactEncoding, BoundaryReplayDirective, CapabilitySet, ControlAction,
    ControlResponse, EventKind, EventPayload, PendingArtifact, PendingEvent, ReplayMode, SwatError,
    SwatResult, TargetAdapter, TargetDescriptor, TargetId,
};

const TRACE_PREFIX: &str = "__SWATPY__";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PythonEntry {
    InlineCode(String),
    ScriptPath(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PythonAdapterSpec {
    pub python_binary: String,
    pub entry: PythonEntry,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
}

impl PythonAdapterSpec {
    pub fn inline(code: impl Into<String>) -> Self {
        Self {
            python_binary: "python3".to_string(),
            entry: PythonEntry::InlineCode(code.into()),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }

    pub fn script(path: impl Into<String>) -> Self {
        Self {
            python_binary: "python3".to_string(),
            entry: PythonEntry::ScriptPath(path.into()),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
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

    fn target_name(&self) -> String {
        match &self.entry {
            PythonEntry::InlineCode(_) => "python:inline".to_string(),
            PythonEntry::ScriptPath(path) => format!("python:{path}"),
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
    Trace(PythonTraceRecord),
    Text(OutputStream, String),
}

#[derive(Debug, Deserialize)]
struct PythonTraceRecord {
    kind: String,
    function: Option<String>,
    file: Option<String>,
    line: Option<u64>,
    return_type: Option<String>,
    exception_type: Option<String>,
    message: Option<String>,
}

pub struct PythonAdapter {
    spec: PythonAdapterSpec,
    descriptor: TargetDescriptor,
    child: Option<Child>,
    output_rx: Option<Receiver<OutputItem>>,
    attached: bool,
    paused: bool,
    exit_emitted: bool,
    next_alias_raw: u64,
}

impl PythonAdapter {
    pub fn new(spec: PythonAdapterSpec) -> Self {
        let target_name = spec.target_name();
        Self {
            spec,
            descriptor: TargetDescriptor {
                target_id: TargetId::new(),
                adapter_name: "swat-adapter-python".to_string(),
                target_name,
                runtime: "python-runtime".to_string(),
                replay_mode: ReplayMode::Live,
            },
            child: None,
            output_rx: None,
            attached: false,
            paused: false,
            exit_emitted: false,
            next_alias_raw: 1,
        }
    }

    fn ensure_attached(&self) -> SwatResult<()> {
        if self.attached {
            Ok(())
        } else {
            Err(SwatError::new("python adapter is not attached"))
        }
    }

    fn next_alias(&mut self) -> ArtifactAlias {
        let alias = ArtifactAlias::from_raw(self.next_alias_raw);
        self.next_alias_raw += 1;
        alias
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.spec.python_binary);
        command.arg("-u").arg("-c").arg(PYTHON_BOOTSTRAP);
        command.arg("--");
        command.args(&self.spec.args);
        if let Some(cwd) = &self.spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &self.spec.env {
            command.env(key, value);
        }

        match &self.spec.entry {
            PythonEntry::InlineCode(code) => {
                command.env("SWAT_RS_PYTHON_MODE", "inline");
                command.env("SWAT_RS_PYTHON_INLINE", code);
            }
            PythonEntry::ScriptPath(path) => {
                command.env("SWAT_RS_PYTHON_MODE", "script");
                command.env("SWAT_RS_PYTHON_SCRIPT", path);
            }
        }

        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command
    }

    fn child_pid(&self) -> SwatResult<u32> {
        self.child
            .as_ref()
            .map(Child::id)
            .ok_or_else(|| SwatError::new("python child is not running"))
    }

    fn signal_child(&self, signal: Signal) -> SwatResult<()> {
        let pid = self.child_pid()?;
        kill(Pid::from_raw(pid as i32), signal)
            .map_err(|err| SwatError::new(format!("failed to signal python process {pid}: {err}")))
    }

    fn spawn_stdout_reader(stdout: ChildStdout, tx: Sender<OutputItem>) {
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Some(payload) = line.strip_prefix(TRACE_PREFIX) {
                    match serde_json::from_str::<PythonTraceRecord>(payload) {
                        Ok(record) => {
                            if tx.send(OutputItem::Trace(record)).is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            if tx
                                .send(OutputItem::Text(OutputStream::Stdout, line))
                                .is_err()
                            {
                                break;
                            }
                        }
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
        Self::spawn_stdout_reader(stdout, tx.clone());
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
                OutputItem::Trace(record) => self.emit_trace_record(record, emission),
                OutputItem::Text(stream, text) => self.emit_text_record(stream, text, emission),
            }
        }
    }

    fn emit_trace_record(&mut self, record: PythonTraceRecord, emission: &mut AdapterEmission) {
        let alias = self.next_alias();
        let json = serde_json::json!({
            "kind": record.kind,
            "function": record.function,
            "file": record.file,
            "line": record.line,
            "return_type": record.return_type,
            "exception_type": record.exception_type,
            "message": record.message,
        });
        let summary = python_trace_summary(&json);

        emission.pending_artifacts.push(PendingArtifact {
            alias,
            media_type: "application/json".to_string(),
            encoding: ArtifactEncoding::Json,
            access: ArtifactAccess::Lazy,
            bytes: serde_json::to_vec(&json).unwrap_or_else(|_| b"{}".to_vec()),
        });
        emission.pending_events.push(
            PendingEvent::new(
                EventKind::Execution,
                EventPayload::Value {
                    value_key: "python.trace".to_string(),
                    summary,
                },
            )
            .with_artifact(ArtifactBinding::Pending(alias)),
        );
    }

    fn emit_text_record(
        &mut self,
        stream: OutputStream,
        text: String,
        emission: &mut AdapterEmission,
    ) {
        let alias = self.next_alias();
        let summary = format!("captured python {} line", stream.name());
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
                EventPayload::Value {
                    value_key: format!("python.{}", stream.name()),
                    summary,
                },
            )
            .with_artifact(ArtifactBinding::Pending(alias)),
        );
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
            .map_err(|err| SwatError::new(format!("failed to poll python child status: {err}")))?
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

impl TargetAdapter for PythonAdapter {
    fn adapter_name(&self) -> &'static str {
        "swat-adapter-python"
    }

    fn attach(&mut self) -> SwatResult<AdapterAttachment> {
        if self.attached {
            return Err(SwatError::new("python adapter already attached"));
        }

        let mut child = self
            .command()
            .spawn()
            .map_err(|err| SwatError::new(format!("failed to spawn python runtime: {err}")))?;
        let pid = child.id();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SwatError::new("python stdout pipe was not available"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SwatError::new("python stderr pipe was not available"))?;

        self.attach_readers(stdout, stderr);
        self.child = Some(child);
        self.attached = true;
        self.paused = false;
        self.exit_emitted = false;
        self.next_alias_raw = 1;

        Ok(AdapterAttachment {
            descriptor: self.descriptor.clone(),
            capabilities: self.capabilities(),
            initial_emission: AdapterEmission {
                pending_events: vec![PendingEvent::new(
                    EventKind::Lifecycle,
                    EventPayload::Text {
                        summary: format!("spawned python runtime pid={pid}"),
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
                summary: "python runtime already exited".to_string(),
            }
        } else {
            match &action {
                ControlAction::Pause => {
                    self.signal_child(Signal::SIGSTOP)?;
                    self.paused = true;
                    ControlResponse {
                        accepted: true,
                        summary: "python runtime paused".to_string(),
                    }
                }
                ControlAction::Resume => {
                    self.signal_child(Signal::SIGCONT)?;
                    self.paused = false;
                    ControlResponse {
                        accepted: true,
                        summary: "python runtime resumed".to_string(),
                    }
                }
                ControlAction::Step => ControlResponse {
                    accepted: false,
                    summary: "python step is not supported yet".to_string(),
                },
                ControlAction::CreateSnapshot { .. } => ControlResponse {
                    accepted: false,
                    summary: "python snapshots are not supported yet".to_string(),
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
            "python adapter does not support replay boundary injection",
        ))
    }
}

impl Drop for PythonAdapter {
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

fn python_trace_summary(record: &serde_json::Value) -> String {
    let kind = record
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("trace");
    let function = record
        .get("function")
        .and_then(|value| value.as_str())
        .unwrap_or("<unknown>");
    let file = record
        .get("file")
        .and_then(|value| value.as_str())
        .unwrap_or("<unknown>");
    let line = record
        .get("line")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    match kind {
        "call" => format!("python call {function} at {file}:{line}"),
        "return" => format!("python return {function} at {file}:{line}"),
        "exception" => format!("python exception in {function} at {file}:{line}"),
        _ => format!("python trace {kind} {function} at {file}:{line}"),
    }
}

fn exit_summary(status: &ExitStatus) -> String {
    if let Some(code) = status.code() {
        format!("python runtime exited with code {code}")
    } else {
        "python runtime exited without a code".to_string()
    }
}

const PYTHON_BOOTSTRAP: &str = r#"
import json
import os
import runpy
import sys

PREFIX = "__SWATPY__"

def emit(payload):
    sys.stdout.write(PREFIX + json.dumps(payload, separators=(",", ":")) + "\n")
    sys.stdout.flush()

mode = os.environ.get("SWAT_RS_PYTHON_MODE", "inline")
trace_file = os.environ.get("SWAT_RS_PYTHON_SCRIPT", "<swat-rs-inline>")

def should_trace(filename):
    return filename == trace_file

def tracer(frame, event, arg):
    if not should_trace(frame.f_code.co_filename):
        return tracer

    payload = {
        "kind": event,
        "function": frame.f_code.co_name,
        "file": frame.f_code.co_filename,
        "line": frame.f_lineno,
    }
    if event == "return":
        payload["return_type"] = type(arg).__name__
    elif event == "exception":
        exc_type, exc_value, _tb = arg
        payload["exception_type"] = getattr(exc_type, "__name__", str(exc_type))
        payload["message"] = str(exc_value)
    emit(payload)
    return tracer

sys.settrace(tracer)

if mode == "inline":
    code = os.environ["SWAT_RS_PYTHON_INLINE"]
    compiled = compile(code, trace_file, "exec")
    globals_dict = {"__name__": "__main__", "__file__": trace_file}
    exec(compiled, globals_dict, globals_dict)
else:
    script = os.environ["SWAT_RS_PYTHON_SCRIPT"]
    sys.argv = [script] + sys.argv[1:]
    runpy.run_path(script, run_name="__main__")
"#;
