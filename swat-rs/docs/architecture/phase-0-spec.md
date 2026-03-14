# Phase 0 Core Spec

## Purpose

Define the irreducible semantic model for `swat-rs` before implementation.

The core system is not "a debugger for one runtime." It is a programmable
control-and-observation kernel with pluggable target adapters.

## Primary goals

- Observe target execution as structured events.
- Resolve raw state into meaningful entities, locations, and values.
- Control execution safely through a capability-gated API.
- Record and replay target behavior across nondeterministic boundaries.
- Expose the same substrate to human operators and AI agents.

## Core invariants

1. Every state transition that matters operationally must have an event form.
2. Events must stay small enough to route, persist, index, and replay cheaply.
3. Large payloads must be referenced as artifacts or values, not embedded.
4. The core must not assume a machine model, language model, or runtime model.
5. Mutation must be capability-gated and policy-checked.
6. Replay must never cross live nondeterministic boundaries by accident.

## Event and artifact split

### Event

An event is a lightweight, immutable fact about target execution.

Required fields:

- `event_id`
- `session_id`
- `target_id`
- `sequence_no`
- `observed_at`
- `kind`
- `causality`
- `payload`
- `artifact_refs`

Representative Rust shape:

```rust
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
```

### Artifact

An artifact is a heavyweight blob or externally materialized value associated
with an event or snapshot.

Examples:

- prompt body
- model response payload
- tool stdout/stderr
- full source text
- checkpoint memory image
- large schema value

Representative Rust shape:

```rust
pub struct ArtifactRef {
    pub artifact_id: ArtifactId,
    pub media_type: String,
    pub encoding: ArtifactEncoding,
    pub size_hint: Option<u64>,
    pub access: ArtifactAccess,
}
```

## Nondeterminism boundary rules

The system must treat the following as replay boundaries:

- LLM calls
- tool calls that reach external state
- wall clock and timers
- RNG
- network I/O
- mutable filesystem reads, when configured as nondeterministic

During replay, the adapter must inject recorded outputs instead of executing
the real boundary again.

Representative Rust shape:

```rust
pub enum DeterminismClass {
    Deterministic,
    ReplayOnly,
    ExternalBoundary,
}

pub enum ReplayMode {
    Live,
    Recorded,
    MixedBounded,
}
```

## Core entities

### Target

Anything attachable and controllable through an adapter.

Examples:

- local process
- Python runtime
- agent orchestrator
- workflow worker
- remote tool host
- PC/GEOS system

### Session

The debugger's attachment and observation context for one or more targets.

### Entity

A meaningful object within a target.

Examples:

- process
- thread
- span
- planner state
- tool invocation
- prompt builder
- memory region
- PC/GEOS handle

### Location

A resolvable place in code, source, schema, memory, object structure, or
workflow graph.

### Value

A typed datum, structured object, or externally stored artifact-backed value.

### Trigger

A predicate over events, values, entities, or causal patterns that emits one
or more actions when matched.

### Action

A control or observation effect initiated by the operator, automation layer,
or trigger engine.

## Capability model

Capabilities must be discovered, not assumed.

Representative capability families:

- attach/detach
- stream events
- stop/resume
- step
- read values
- write values
- snapshot
- replay boundary injection
- source resolution
- schema resolution
- semantic trigger support

Representative Rust shape:

```rust
pub struct CapabilitySet {
    pub can_attach: bool,
    pub can_stream_events: bool,
    pub can_stop: bool,
    pub can_resume: bool,
    pub can_step: bool,
    pub can_read_values: bool,
    pub can_write_values: bool,
    pub can_snapshot: bool,
    pub can_resolve_source: bool,
    pub can_resolve_schema: bool,
}
```

## Adapter contract

Adapters are responsible for:

- capability declaration
- session handshake
- event emission
- query handling
- control action execution
- replay boundary interception
- policy hooks for dangerous mutations

The core is responsible for:

- protocol framing
- persistence
- indexing
- trigger evaluation
- replay orchestration
- client-facing API

## Scripting boundary

Phase 0 freezes the scripting host contract, not the final embedded language.

The scripting runtime must:

- consume the same public API as human and AI clients
- run under explicit capability and policy limits
- avoid direct unsandboxed access to adapter internals
- support lightweight interactive automation

Candidate runtimes to evaluate in a later spike:

- Rhai
- Lua via `mlua`
- WASM plugin host

## Initial event taxonomy

The first implementation must support these top-level event families:

- `Lifecycle`
- `Control`
- `Execution`
- `StateMutation`
- `ValueObserved`
- `TriggerHit`
- `Snapshot`
- `Replay`
- `ModelBoundary`
- `ToolBoundary`
- `SourceResolution`
- `SchemaResolution`
- `PolicyDecision`

## MVP boundary for implementation phases

### Phase 1

- workspace scaffold
- protocol skeleton
- event store
- artifact store
- replay engine skeleton
- mock adapter

### Phase 2

- first real adapter
- live attach
- event stream
- stop/resume
- value query
- replay of recorded boundary outputs

### Phase 3

- expression engine
- schema engine
- semantic triggers
- typed value editing

