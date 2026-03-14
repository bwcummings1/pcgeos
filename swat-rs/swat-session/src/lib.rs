#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use swat_core::{
    CapabilitySet, ControlAction, ControlResponse, EventEnvelope, ReplayMode, SessionId, SwatError,
    SwatResult, TargetAdapter, TargetDescriptor, TargetId,
};
use swat_protocol::{
    Attached, CURRENT_PROTOCOL_VERSION, ControlRequest, ControlResponseEnvelope, EventBatch,
    ProtocolMessage, ReplayDirectiveEnvelope, SessionHello,
};
use swat_replay::{ReplayController, ReplayPlan};
use swat_store::SwatStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachedSession {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub descriptor: TargetDescriptor,
    pub capabilities: CapabilitySet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachReport {
    pub session: AttachedSession,
    pub messages: Vec<ProtocolMessage>,
    pub stored_events: Vec<EventEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PumpReport {
    pub messages: Vec<ProtocolMessage>,
    pub stored_events: Vec<EventEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlReport {
    pub response: ControlResponse,
    pub messages: Vec<ProtocolMessage>,
    pub stored_events: Vec<EventEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayApplyReport {
    pub directives_applied: usize,
    pub messages: Vec<ProtocolMessage>,
    pub stored_events: Vec<EventEnvelope>,
}

#[derive(Clone, Debug)]
struct SessionState {
    descriptor: TargetDescriptor,
    capabilities: CapabilitySet,
    next_sequence: u64,
}

#[derive(Default)]
pub struct SessionManager {
    sessions: BTreeMap<SessionId, SessionState>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attach<A: TargetAdapter + ?Sized, S: SwatStore + ?Sized>(
        &mut self,
        adapter: &mut A,
        store: &mut S,
    ) -> SwatResult<AttachReport> {
        let session_id = SessionId::new();
        let attachment = adapter.attach()?;
        let target_id = attachment.descriptor.target_id;
        let mut state = SessionState {
            descriptor: attachment.descriptor.clone(),
            capabilities: attachment.capabilities,
            next_sequence: 1,
        };
        let stored_events = store.ingest_emission(
            session_id,
            target_id,
            &mut state.next_sequence,
            attachment.initial_emission,
        )?;

        let session = AttachedSession {
            session_id,
            target_id,
            descriptor: state.descriptor.clone(),
            capabilities: state.capabilities,
        };
        self.sessions.insert(session_id, state);

        let mut messages = vec![
            ProtocolMessage::Hello(SessionHello {
                session_id,
                protocol_version: CURRENT_PROTOCOL_VERSION.to_string(),
                requested_replay_mode: ReplayMode::Live,
            }),
            ProtocolMessage::Attached(Attached {
                session_id,
                descriptor: session.descriptor.clone(),
                capabilities: session.capabilities,
            }),
        ];
        if !stored_events.is_empty() {
            messages.push(ProtocolMessage::EventBatch(EventBatch {
                session_id,
                target_id,
                events: stored_events.clone(),
            }));
        }

        Ok(AttachReport {
            session,
            messages,
            stored_events,
        })
    }

    pub fn pump<A: TargetAdapter + ?Sized, S: SwatStore + ?Sized>(
        &mut self,
        session_id: SessionId,
        adapter: &mut A,
        store: &mut S,
    ) -> SwatResult<PumpReport> {
        let state = self.session_state_mut(session_id)?;
        let emission = adapter.poll()?;
        let stored_events = store.ingest_emission(
            session_id,
            state.descriptor.target_id,
            &mut state.next_sequence,
            emission,
        )?;

        let messages = if stored_events.is_empty() {
            Vec::new()
        } else {
            vec![ProtocolMessage::EventBatch(EventBatch {
                session_id,
                target_id: state.descriptor.target_id,
                events: stored_events.clone(),
            })]
        };

        Ok(PumpReport {
            messages,
            stored_events,
        })
    }

    pub fn control<A: TargetAdapter + ?Sized, S: SwatStore + ?Sized>(
        &mut self,
        session_id: SessionId,
        adapter: &mut A,
        action: ControlAction,
        store: &mut S,
    ) -> SwatResult<ControlReport> {
        let state = self.session_state_mut(session_id)?;
        let request = ProtocolMessage::ControlRequest(ControlRequest {
            session_id,
            target_id: state.descriptor.target_id,
            action: action.clone(),
        });

        let result = adapter.control(action)?;
        let stored_events = store.ingest_emission(
            session_id,
            state.descriptor.target_id,
            &mut state.next_sequence,
            result.emission,
        )?;

        let mut messages = vec![
            request,
            ProtocolMessage::ControlResponse(ControlResponseEnvelope {
                session_id,
                target_id: state.descriptor.target_id,
                response: result.response.clone(),
            }),
        ];
        if !stored_events.is_empty() {
            messages.push(ProtocolMessage::EventBatch(EventBatch {
                session_id,
                target_id: state.descriptor.target_id,
                events: stored_events.clone(),
            }));
        }

        Ok(ControlReport {
            response: result.response,
            messages,
            stored_events,
        })
    }

    pub fn apply_replay_plan<A: TargetAdapter + ?Sized, S: SwatStore + ?Sized>(
        &mut self,
        session_id: SessionId,
        adapter: &mut A,
        store: &mut S,
        plan: &ReplayPlan,
    ) -> SwatResult<ReplayApplyReport> {
        let state = self.session_state_mut(session_id)?;
        if !state.capabilities.can_inject_replay && !plan.is_empty() {
            return Err(SwatError::new(format!(
                "target {} does not support replay injection",
                state.descriptor.target_id.raw()
            )));
        }
        let controller = ReplayController;
        let emission = controller.apply(adapter, plan)?;
        let stored_events = store.ingest_emission(
            session_id,
            state.descriptor.target_id,
            &mut state.next_sequence,
            emission,
        )?;

        let mut messages = plan
            .directives()
            .cloned()
            .map(|directive| {
                ProtocolMessage::ReplayDirective(ReplayDirectiveEnvelope {
                    session_id,
                    target_id: state.descriptor.target_id,
                    directive,
                })
            })
            .collect::<Vec<_>>();

        if !stored_events.is_empty() {
            messages.push(ProtocolMessage::EventBatch(EventBatch {
                session_id,
                target_id: state.descriptor.target_id,
                events: stored_events.clone(),
            }));
        }

        Ok(ReplayApplyReport {
            directives_applied: plan.len(),
            messages,
            stored_events,
        })
    }

    pub fn session(&self, session_id: SessionId) -> Option<AttachedSession> {
        self.sessions.get(&session_id).map(|state| AttachedSession {
            session_id,
            target_id: state.descriptor.target_id,
            descriptor: state.descriptor.clone(),
            capabilities: state.capabilities,
        })
    }

    pub fn record_emission<S: SwatStore + ?Sized>(
        &mut self,
        session_id: SessionId,
        store: &mut S,
        emission: swat_core::AdapterEmission,
    ) -> SwatResult<Vec<EventEnvelope>> {
        let state = self.session_state_mut(session_id)?;
        store.ingest_emission(
            session_id,
            state.descriptor.target_id,
            &mut state.next_sequence,
            emission,
        )
    }

    fn session_state_mut(&mut self, session_id: SessionId) -> SwatResult<&mut SessionState> {
        self.sessions
            .get_mut(&session_id)
            .ok_or_else(|| SwatError::new(format!("unknown session {}", session_id.raw())))
    }
}
