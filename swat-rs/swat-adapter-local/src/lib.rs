#![forbid(unsafe_code)]

use std::io::Read;
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use swat_core::{
    AdapterAttachment, AdapterControlResult, AdapterEmission, ArtifactAccess, ArtifactAlias,
    ArtifactBinding, ArtifactEncoding, BoundaryReplayDirective, CapabilitySet, ControlAction,
    ControlResponse, EventKind, EventPayload, PendingArtifact, PendingEvent, ReplayMode, SwatError,
    SwatResult, TargetAdapter, TargetDescriptor, TargetId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
}

impl LocalProcessSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
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

    pub fn command_summary(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessStream {
    Stdout,
    Stderr,
}

impl ProcessStream {
    fn name(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug)]
struct OutputChunk {
    stream: ProcessStream,
    bytes: Vec<u8>,
}

pub struct LocalProcessAdapter {
    spec: LocalProcessSpec,
    descriptor: TargetDescriptor,
    child: Option<Child>,
    output_rx: Option<Receiver<OutputChunk>>,
    attached: bool,
    paused: bool,
    exit_emitted: bool,
    next_alias_raw: u64,
}

impl LocalProcessAdapter {
    pub fn new(spec: LocalProcessSpec) -> Self {
        let target_name = spec.command_summary();
        Self {
            spec,
            descriptor: TargetDescriptor {
                target_id: TargetId::new(),
                adapter_name: "swat-adapter-local".to_string(),
                target_name,
                runtime: "local-process".to_string(),
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
            Err(SwatError::new("local process adapter is not attached"))
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

    fn child_pid(&self) -> SwatResult<u32> {
        self.child
            .as_ref()
            .map(Child::id)
            .ok_or_else(|| SwatError::new("local process child is not running"))
    }

    fn signal_child(&self, signal: Signal) -> SwatResult<()> {
        let pid = self.child_pid()?;
        kill(Pid::from_raw(pid as i32), signal)
            .map_err(|err| SwatError::new(format!("failed to signal process {pid}: {err}")))
    }

    fn reader_thread<T>(stream: ProcessStream, reader: T, tx: Sender<OutputChunk>)
    where
        T: Read + Send + 'static,
    {
        thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0_u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(read_len) => {
                        if tx
                            .send(OutputChunk {
                                stream,
                                bytes: buf[..read_len].to_vec(),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    fn attach_readers(&mut self, stdout: ChildStdout, stderr: ChildStderr) {
        let (tx, rx) = mpsc::channel();
        Self::reader_thread(ProcessStream::Stdout, stdout, tx.clone());
        Self::reader_thread(ProcessStream::Stderr, stderr, tx);
        self.output_rx = Some(rx);
    }

    fn drain_output(&mut self, emission: &mut AdapterEmission) {
        let mut drained = Vec::new();
        if let Some(rx) = &self.output_rx {
            while let Ok(chunk) = rx.try_recv() {
                drained.push(chunk);
            }
        }

        for chunk in drained {
            let alias = self.next_alias();
            let encoding = match std::str::from_utf8(&chunk.bytes) {
                Ok(_) => ArtifactEncoding::Utf8,
                Err(_) => ArtifactEncoding::Binary,
            };
            let media_type = match chunk.stream {
                ProcessStream::Stdout | ProcessStream::Stderr => "text/plain".to_string(),
            };
            let summary = format!(
                "captured {} chunk ({} bytes)",
                chunk.stream.name(),
                chunk.bytes.len()
            );

            emission.pending_artifacts.push(PendingArtifact {
                alias,
                media_type,
                encoding,
                access: ArtifactAccess::Lazy,
                bytes: chunk.bytes,
            });
            emission.pending_events.push(
                PendingEvent::new(
                    EventKind::ValueObserved,
                    EventPayload::Value {
                        value_key: chunk.stream.name().to_string(),
                        summary,
                    },
                )
                .with_artifact(ArtifactBinding::Pending(alias)),
            );
        }
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
            .map_err(|err| SwatError::new(format!("failed to poll child status: {err}")))?
        {
            self.paused = false;
            self.exit_emitted = true;
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

impl TargetAdapter for LocalProcessAdapter {
    fn adapter_name(&self) -> &'static str {
        "swat-adapter-local"
    }

    fn attach(&mut self) -> SwatResult<AdapterAttachment> {
        if self.attached {
            return Err(SwatError::new("local process adapter already attached"));
        }

        let mut child = self
            .command()
            .spawn()
            .map_err(|err| SwatError::new(format!("failed to spawn local process: {err}")))?;
        let pid = child.id();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SwatError::new("local process stdout pipe was not available"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SwatError::new("local process stderr pipe was not available"))?;

        self.attach_readers(stdout, stderr);
        self.child = Some(child);
        self.attached = true;
        self.paused = false;
        self.exit_emitted = false;
        self.next_alias_raw = 1;

        let initial_emission = AdapterEmission {
            pending_events: vec![PendingEvent::new(
                EventKind::Lifecycle,
                EventPayload::Text {
                    summary: format!(
                        "spawned local process pid={} cmd={}",
                        pid,
                        self.spec.command_summary()
                    ),
                },
            )],
            pending_artifacts: Vec::new(),
        };

        Ok(AdapterAttachment {
            descriptor: self.descriptor.clone(),
            capabilities: self.capabilities(),
            initial_emission,
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
                summary: "local process already exited".to_string(),
            }
        } else {
            match &action {
                ControlAction::Pause => {
                    self.signal_child(Signal::SIGSTOP)?;
                    self.paused = true;
                    ControlResponse {
                        accepted: true,
                        summary: "local process paused".to_string(),
                    }
                }
                ControlAction::Resume => {
                    self.signal_child(Signal::SIGCONT)?;
                    self.paused = false;
                    ControlResponse {
                        accepted: true,
                        summary: "local process resumed".to_string(),
                    }
                }
                ControlAction::Step => ControlResponse {
                    accepted: false,
                    summary: "step is not supported for local processes".to_string(),
                },
                ControlAction::CreateSnapshot { .. } => ControlResponse {
                    accepted: false,
                    summary: "snapshots are not supported for local processes yet".to_string(),
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
            "local process adapter does not support replay boundary injection",
        ))
    }
}

impl Drop for LocalProcessAdapter {
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

fn exit_summary(status: &ExitStatus) -> String {
    if let Some(code) = status.code() {
        format!("local process exited with code {code}")
    } else {
        "local process exited without a code".to_string()
    }
}
