# Agent Event Protocol

`swat-adapter-agent` consumes a small line-oriented protocol from a local child
process. This is the first AI-native target contract for `swat-rs`.

The canonical Rust implementation for this contract now lives in
`swat-agent-protocol`.

It preserves the same architectural separation that made legacy Swat useful:

- host/session/control orchestration stays outside the target
- the target emits compact semantic observations
- large or structured values are attached as artifacts

Legacy anchor points for this design are the host-side event/control split in
`Tools/swat/event.c`, `Tools/swat/rpc.c`, and `Tools/swat/break.c`, the target
boundary transport in `Tools/swat/Stub/rpc.asm`, and source-oriented inspection
in `Tools/swat/src.c`.

## Transport

- The target writes newline-delimited UTF-8 to stdout/stderr.
- Ordinary stdout/stderr lines become `ValueObserved` events.
- A stdout line that starts with `__SWATAGENT__` is parsed as a JSON agent
  event record.
- The parsed JSON is persisted as a lazy JSON artifact and normalized into one
  `swat-core::EventEnvelope`.

For Rust runtimes, the intended entry point is:

- `swat_agent_protocol::AgentEventRecord`
- `swat_agent_protocol::LineEmitter`
- `swat_agent_protocol::CURRENT_AGENT_PROTOCOL_VERSION`
- `swat_agent_protocol::encode_prefixed_line*`
- `swat_agent_protocol::parse_prefixed_line*`
- `swat_agent_protocol::parse_validated_prefixed_line*`

For Python runtimes, the intended entry point is:

- `sdk/python/swat_agent_protocol.py`
- `LineEmitter`
- `planner`, `model`, `tool`, `state`, `policy`, `source`, `schema`
- `encode_prefixed_line`
- `parse_prefixed_line`

For TypeScript runtimes, the intended entry point is:

- `sdk/typescript/swat_agent_protocol.ts`
- `LineEmitter`
- `planner`, `model`, `tool`, `state`, `policy`, `source`, `schema`
- `encodePrefixedLine`
- `parsePrefixedLine`

## Required shape

`kind` is the only semantic field that is effectively required, but emitters
should also send `protocol_version` on every record. Other fields are optional
but strongly recommended because they improve summary quality, causality,
source lookup, and boundary reuse.

```json
{
  "protocol_version": "0.1.0-alpha",
  "kind": "model",
  "phase": "request",
  "span_id": "model-1",
  "correlation_id": "req-42",
  "name": "gpt-4.1-mini",
  "summary": "model requested",
  "file": "/abs/path/runtime.py",
  "line": 42,
  "function": "run_agent"
}
```

Compatibility note:

- records that omit `protocol_version` are currently treated as the current
  version for transition compatibility
- records with an unsupported `protocol_version` are rejected by
  `swat-adapter-agent`
- rejected records are surfaced as lifecycle events with the raw line attached
  as a text artifact

## Normalization rules

- `kind = "planner"` -> `EventKind::Execution`
- `kind = "model"` -> `EventKind::ModelBoundary`
- `kind = "tool"` -> `EventKind::ToolBoundary`
- `kind = "state"` -> `EventKind::StateMutation`
- `kind = "policy"` -> `EventKind::PolicyDecision`
- `kind = "source"` -> `EventKind::SourceResolution`
- `kind = "schema"` -> `EventKind::SchemaResolution`
- `kind = "lifecycle"` -> `EventKind::Lifecycle`
- any other value -> `EventKind::ValueObserved`

Payload mapping:

- model/tool records become `EventPayload::Boundary`
- policy records become `EventPayload::Policy`
- planner/state/source/schema/log style records become `EventPayload::Value`
- lifecycle records become `EventPayload::Text`

## Boundary and causality rules

- `span_id` is used to preserve one logical `BoundaryId` across start/end style
  model and tool records.
- `correlation_id` is copied into `EventEnvelope.causality.correlation_id`.
- If `correlation_id` is absent, `span_id` is used as the fallback causality id.

## Determinism rules

For model/tool boundaries:

- `determinism = "deterministic"` -> `DeterminismClass::Deterministic`
- `determinism = "replay_only"` or `replay-only` -> `DeterminismClass::ReplayOnly`
- otherwise -> `DeterminismClass::ExternalBoundary`

This keeps model/tool calls aligned with the replay-boundary rules already
frozen in the Phase 0 architecture spec.

## Validation rules

- `parse_prefixed_line*` performs raw JSON parsing only
- `parse_validated_prefixed_line*` performs parse plus protocol-version
  validation
- `swat-adapter-agent` uses the validated path so protocol drift becomes a
  visible host-side event instead of silently degrading into generic stdout
