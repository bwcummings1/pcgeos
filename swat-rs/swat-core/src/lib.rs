#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_raw_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(u64);

        impl $name {
            pub fn new() -> Self {
                Self(next_raw_id())
            }

            pub const fn from_raw(raw: u64) -> Self {
                Self(raw)
            }

            pub const fn raw(self) -> u64 {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }
    };
}

define_id!(SessionId);
define_id!(TargetId);
define_id!(EventId);
define_id!(ArtifactId);
define_id!(ArtifactAlias);
define_id!(SnapshotId);
define_id!(BoundaryId);
define_id!(TriggerId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(u64);

impl Timestamp {
    pub fn now() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self(millis)
    }

    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::now()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwatError {
    message: String,
}

impl SwatError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SwatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SwatError {}

pub type SwatResult<T> = Result<T, SwatError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactEncoding {
    Utf8,
    Json,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactAccess {
    Inline,
    Lazy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub artifact_id: ArtifactId,
    pub media_type: String,
    pub encoding: ArtifactEncoding,
    pub size_hint: Option<u64>,
    pub access: ArtifactAccess,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactBinding {
    Pending(ArtifactAlias),
    Existing(ArtifactRef),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingArtifact {
    pub alias: ArtifactAlias,
    pub media_type: String,
    pub encoding: ArtifactEncoding,
    pub access: ArtifactAccess,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayMode {
    Live,
    Recorded,
    MixedBounded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeterminismClass {
    Deterministic,
    ReplayOnly,
    ExternalBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyVerdict {
    Allow,
    Deny,
    Redact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Lifecycle,
    Control,
    Execution,
    StateMutation,
    ValueObserved,
    TriggerHit,
    Snapshot,
    Replay,
    ModelBoundary,
    ToolBoundary,
    SourceResolution,
    SchemaResolution,
    PolicyDecision,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CausalityLink {
    pub parent_event_id: Option<EventId>,
    pub correlation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlAction {
    Pause,
    Resume,
    Step,
    CreateSnapshot { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPayload {
    Empty,
    Text {
        summary: String,
    },
    Control {
        action: ControlAction,
        summary: String,
    },
    Boundary {
        boundary_id: BoundaryId,
        determinism: DeterminismClass,
        summary: String,
    },
    Snapshot {
        snapshot_id: SnapshotId,
        summary: String,
    },
    Trigger {
        trigger_id: TriggerId,
        summary: String,
    },
    Value {
        value_key: String,
        summary: String,
    },
    Policy {
        verdict: PolicyVerdict,
        summary: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub sequence_no: u64,
    pub observed_at: Timestamp,
    pub kind: EventKind,
    pub causality: CausalityLink,
    pub payload: EventPayload,
    pub artifact_refs: Vec<ArtifactRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingEvent {
    pub observed_at: Timestamp,
    pub kind: EventKind,
    pub causality: CausalityLink,
    pub payload: EventPayload,
    pub artifacts: Vec<ArtifactBinding>,
}

impl PendingEvent {
    pub fn new(kind: EventKind, payload: EventPayload) -> Self {
        Self {
            observed_at: Timestamp::now(),
            kind,
            causality: CausalityLink::default(),
            payload,
            artifacts: Vec::new(),
        }
    }

    pub fn with_artifact(mut self, binding: ArtifactBinding) -> Self {
        self.artifacts.push(binding);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AdapterEmission {
    pub pending_events: Vec<PendingEvent>,
    pub pending_artifacts: Vec<PendingArtifact>,
}

impl AdapterEmission {
    pub fn is_empty(&self) -> bool {
        self.pending_events.is_empty() && self.pending_artifacts.is_empty()
    }

    pub fn extend(&mut self, mut other: Self) {
        self.pending_events.append(&mut other.pending_events);
        self.pending_artifacts.append(&mut other.pending_artifacts);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlResponse {
    pub accepted: bool,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryReplayDirective {
    pub boundary_id: BoundaryId,
    pub artifact_ref: ArtifactRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySet {
    pub can_attach: bool,
    pub can_stream_events: bool,
    pub can_stop: bool,
    pub can_resume: bool,
    pub can_step: bool,
    pub can_read_values: bool,
    pub can_write_values: bool,
    pub can_snapshot: bool,
    pub can_inject_replay: bool,
    pub can_resolve_source: bool,
    pub can_resolve_schema: bool,
}

impl CapabilitySet {
    pub const fn none() -> Self {
        Self {
            can_attach: false,
            can_stream_events: false,
            can_stop: false,
            can_resume: false,
            can_step: false,
            can_read_values: false,
            can_write_values: false,
            can_snapshot: false,
            can_inject_replay: false,
            can_resolve_source: false,
            can_resolve_schema: false,
        }
    }

    pub const fn basic_observer() -> Self {
        Self {
            can_attach: true,
            can_stream_events: true,
            can_stop: true,
            can_resume: true,
            can_step: true,
            can_read_values: true,
            can_write_values: false,
            can_snapshot: true,
            can_inject_replay: false,
            can_resolve_source: false,
            can_resolve_schema: false,
        }
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self::none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetDescriptor {
    pub target_id: TargetId,
    pub adapter_name: String,
    pub target_name: String,
    pub runtime: String,
    pub replay_mode: ReplayMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterAttachment {
    pub descriptor: TargetDescriptor,
    pub capabilities: CapabilitySet,
    pub initial_emission: AdapterEmission,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterControlResult {
    pub response: ControlResponse,
    pub emission: AdapterEmission,
}

pub trait TargetAdapter {
    fn adapter_name(&self) -> &'static str;
    fn attach(&mut self) -> SwatResult<AdapterAttachment>;
    fn capabilities(&self) -> CapabilitySet;
    fn poll(&mut self) -> SwatResult<AdapterEmission>;
    fn control(&mut self, action: ControlAction) -> SwatResult<AdapterControlResult>;
    fn inject_boundary_replay(
        &mut self,
        directive: BoundaryReplayDirective,
    ) -> SwatResult<AdapterEmission>;
}
