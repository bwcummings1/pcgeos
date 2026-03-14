#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use swat_core::{
    AdapterAttachment, AdapterControlResult, AdapterEmission, ArtifactAccess, ArtifactAlias,
    ArtifactBinding, ArtifactEncoding, ArtifactRef, BoundaryId, BoundaryReplayDirective,
    CapabilitySet, ControlAction, ControlResponse, DeterminismClass, EventKind, EventPayload,
    PendingArtifact, PendingEvent, ReplayMode, SwatError, SwatResult, TargetAdapter,
    TargetDescriptor, TargetId,
};

pub const MOCK_BOUNDARY_ID: BoundaryId = BoundaryId::from_raw(42);
const MOCK_BOUNDARY_ARTIFACT_ALIAS: ArtifactAlias = ArtifactAlias::from_raw(7);

pub struct MockAdapter {
    attached: bool,
    paused: bool,
    tick: u64,
    replay_artifacts: BTreeMap<BoundaryId, ArtifactRef>,
    descriptor: TargetDescriptor,
}

impl MockAdapter {
    pub fn new(target_name: impl Into<String>) -> Self {
        Self {
            attached: false,
            paused: true,
            tick: 0,
            replay_artifacts: BTreeMap::new(),
            descriptor: TargetDescriptor {
                target_id: TargetId::new(),
                adapter_name: "swat-adapter-mock".to_string(),
                target_name: target_name.into(),
                runtime: "mock-runtime".to_string(),
                replay_mode: ReplayMode::Live,
            },
        }
    }

    fn ensure_attached(&self) -> SwatResult<()> {
        if self.attached {
            Ok(())
        } else {
            Err(SwatError::new("mock adapter is not attached"))
        }
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new("mock-target")
    }
}

impl TargetAdapter for MockAdapter {
    fn adapter_name(&self) -> &'static str {
        "swat-adapter-mock"
    }

    fn attach(&mut self) -> SwatResult<AdapterAttachment> {
        if self.attached {
            return Err(SwatError::new("mock adapter already attached"));
        }
        self.attached = true;
        self.paused = true;
        self.tick = 0;

        let initial_emission = AdapterEmission {
            pending_events: vec![PendingEvent::new(
                EventKind::Lifecycle,
                EventPayload::Text {
                    summary: "mock adapter attached".to_string(),
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
            can_inject_replay: true,
            ..CapabilitySet::basic_observer()
        }
    }

    fn poll(&mut self) -> SwatResult<AdapterEmission> {
        self.ensure_attached()?;
        if self.paused {
            return Ok(AdapterEmission::default());
        }

        self.tick += 1;
        let mut emission = AdapterEmission::default();
        emission.pending_events.push(PendingEvent::new(
            EventKind::Execution,
            EventPayload::Text {
                summary: format!("mock execution tick {}", self.tick),
            },
        ));

        if self.tick == 2 {
            if let Some(artifact_ref) = self.replay_artifacts.get(&MOCK_BOUNDARY_ID).cloned() {
                emission.pending_events.push(PendingEvent::new(
                    EventKind::Replay,
                    EventPayload::Text {
                        summary: format!(
                            "replay boundary {} injected with artifact {}",
                            MOCK_BOUNDARY_ID.raw(),
                            artifact_ref.artifact_id.raw()
                        ),
                    },
                ));

                emission.pending_events.push(
                    PendingEvent::new(
                        EventKind::ModelBoundary,
                        EventPayload::Boundary {
                            boundary_id: MOCK_BOUNDARY_ID,
                            determinism: DeterminismClass::ExternalBoundary,
                            summary: "mock model boundary replayed".to_string(),
                        },
                    )
                    .with_artifact(ArtifactBinding::Existing(artifact_ref)),
                );
            } else {
                emission.pending_artifacts.push(PendingArtifact {
                    alias: MOCK_BOUNDARY_ARTIFACT_ALIAS,
                    media_type: "application/json".to_string(),
                    encoding: ArtifactEncoding::Json,
                    access: ArtifactAccess::Lazy,
                    bytes: br#"{"decision":"call-tool","tool":"search"}"#.to_vec(),
                });

                emission.pending_events.push(
                    PendingEvent::new(
                        EventKind::ModelBoundary,
                        EventPayload::Boundary {
                            boundary_id: MOCK_BOUNDARY_ID,
                            determinism: DeterminismClass::ExternalBoundary,
                            summary: "mock model boundary observed".to_string(),
                        },
                    )
                    .with_artifact(ArtifactBinding::Pending(MOCK_BOUNDARY_ARTIFACT_ALIAS)),
                );
            }
        }

        Ok(emission)
    }

    fn control(&mut self, action: ControlAction) -> SwatResult<AdapterControlResult> {
        self.ensure_attached()?;
        let mut emission = AdapterEmission::default();

        let response = match &action {
            ControlAction::Pause => {
                self.paused = true;
                ControlResponse {
                    accepted: true,
                    summary: "mock target paused".to_string(),
                }
            }
            ControlAction::Resume => {
                self.paused = false;
                ControlResponse {
                    accepted: true,
                    summary: "mock target resumed".to_string(),
                }
            }
            ControlAction::Step => ControlResponse {
                accepted: true,
                summary: "mock target step requested".to_string(),
            },
            ControlAction::CreateSnapshot { reason } => ControlResponse {
                accepted: true,
                summary: format!("mock snapshot requested: {reason}"),
            },
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
        directive: BoundaryReplayDirective,
    ) -> SwatResult<AdapterEmission> {
        self.ensure_attached()?;
        self.replay_artifacts
            .insert(directive.boundary_id, directive.artifact_ref.clone());

        Ok(AdapterEmission {
            pending_events: vec![PendingEvent::new(
                EventKind::Replay,
                EventPayload::Text {
                    summary: format!(
                        "prepared replay for boundary {}",
                        directive.boundary_id.raw()
                    ),
                },
            )],
            pending_artifacts: Vec::new(),
        })
    }
}
