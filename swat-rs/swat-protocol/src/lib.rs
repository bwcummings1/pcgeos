#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use swat_core::{
    BoundaryReplayDirective, CapabilitySet, ControlAction, ControlResponse, EventEnvelope,
    ReplayMode, SessionId, SwatError, SwatResult, TargetDescriptor, TargetId,
};

pub const CURRENT_PROTOCOL_VERSION: &str = "0.1.0-alpha";
const LENGTH_PREFIX_BYTES: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHello {
    pub session_id: SessionId,
    pub protocol_version: String,
    pub requested_replay_mode: ReplayMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attached {
    pub session_id: SessionId,
    pub descriptor: TargetDescriptor,
    pub capabilities: CapabilitySet,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBatch {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub events: Vec<EventEnvelope>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlRequest {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub action: ControlAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlResponseEnvelope {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub response: ControlResponse,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayDirectiveEnvelope {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub directive: BoundaryReplayDirective,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolMessage {
    Hello(SessionHello),
    Attached(Attached),
    EventBatch(EventBatch),
    ControlRequest(ControlRequest),
    ControlResponse(ControlResponseEnvelope),
    ReplayDirective(ReplayDirectiveEnvelope),
}

impl ProtocolMessage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hello(_) => "hello",
            Self::Attached(_) => "attached",
            Self::EventBatch(_) => "event-batch",
            Self::ControlRequest(_) => "control-request",
            Self::ControlResponse(_) => "control-response",
            Self::ReplayDirective(_) => "replay-directive",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireEnvelope {
    pub protocol_version: String,
    pub message: ProtocolMessage,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LengthPrefixedJsonCodec;

impl LengthPrefixedJsonCodec {
    pub fn encode(&self, message: &ProtocolMessage) -> SwatResult<Vec<u8>> {
        let envelope = WireEnvelope {
            protocol_version: CURRENT_PROTOCOL_VERSION.to_string(),
            message: message.clone(),
        };
        let payload = serde_json::to_vec(&envelope)
            .map_err(|err| SwatError::new(format!("failed to encode protocol frame: {err}")))?;
        let payload_len = u32::try_from(payload.len())
            .map_err(|_| SwatError::new("protocol frame exceeds u32 length prefix"))?;

        let mut frame = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
        frame.extend_from_slice(&payload_len.to_be_bytes());
        frame.extend_from_slice(&payload);
        Ok(frame)
    }

    pub fn decode(&self, frame: &[u8]) -> SwatResult<ProtocolMessage> {
        if frame.len() < LENGTH_PREFIX_BYTES {
            return Err(SwatError::new(
                "protocol frame is shorter than its length prefix",
            ));
        }

        let declared_len = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        let payload = &frame[LENGTH_PREFIX_BYTES..];
        if payload.len() != declared_len {
            return Err(SwatError::new(format!(
                "protocol frame length mismatch: declared {declared_len}, actual {}",
                payload.len()
            )));
        }

        let envelope: WireEnvelope = serde_json::from_slice(payload)
            .map_err(|err| SwatError::new(format!("failed to decode protocol frame: {err}")))?;

        if envelope.protocol_version != CURRENT_PROTOCOL_VERSION {
            return Err(SwatError::new(format!(
                "unsupported protocol version {}",
                envelope.protocol_version
            )));
        }

        Ok(envelope.message)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Attached, CURRENT_PROTOCOL_VERSION, LENGTH_PREFIX_BYTES, LengthPrefixedJsonCodec,
        ProtocolMessage, SessionHello, WireEnvelope,
    };
    use swat_core::{CapabilitySet, ReplayMode, SessionId, TargetDescriptor, TargetId};

    #[test]
    fn roundtrips_length_prefixed_frames() {
        let codec = LengthPrefixedJsonCodec;
        let message = ProtocolMessage::Hello(SessionHello {
            session_id: SessionId::from_raw(7),
            protocol_version: CURRENT_PROTOCOL_VERSION.to_string(),
            requested_replay_mode: ReplayMode::MixedBounded,
        });

        let encoded = codec.encode(&message).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn rejects_short_frames() {
        let codec = LengthPrefixedJsonCodec;
        let err = codec.decode(&[1, 2, 3]).unwrap_err();

        assert!(err.to_string().contains("shorter than its length prefix"));
    }

    #[test]
    fn rejects_version_mismatch() {
        let codec = LengthPrefixedJsonCodec;
        let payload = serde_json::to_vec(&WireEnvelope {
            protocol_version: "0.0.0-test".to_string(),
            message: ProtocolMessage::Attached(Attached {
                session_id: SessionId::from_raw(1),
                descriptor: TargetDescriptor {
                    target_id: TargetId::from_raw(2),
                    adapter_name: "mock".to_string(),
                    target_name: "target".to_string(),
                    runtime: "runtime".to_string(),
                    replay_mode: ReplayMode::Live,
                },
                capabilities: CapabilitySet::basic_observer(),
            }),
        })
        .unwrap();
        let mut frame = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);

        let err = codec.decode(&frame).unwrap_err();
        assert!(err.to_string().contains("unsupported protocol version"));
    }
}
