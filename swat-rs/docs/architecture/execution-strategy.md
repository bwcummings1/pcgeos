# Execution Strategy

## Chosen path

Build `swat-rs` as a new top-level Rust workspace and execute the program as a
de-risking sequence:

1. freeze semantics
2. build substrate
3. validate against a mock target
4. validate against one real modern target
5. expand only after replay, control, and typed inspection are sound

This is intentionally not a source port of legacy Swat.

## Why this path is optimal

- It preserves the legacy Swat tree as a stable reference.
- It prevents the old GEOS/8086 assumptions from contaminating the new core.
- It proves the architecture on a modern target before spending effort on
  historical compatibility.
- It reduces the risk of building UI or scripting on a weak substrate.

## Revised phase plan

### Phase 0: architecture freeze

Deliverables:

- core semantic model
- event vs artifact boundary
- nondeterminism rules
- adapter contract
- scripting host contract
- initial crate map
- ADR set

Exit criteria:

- the core types and invariants are documented
- the replay boundary model is frozen
- the first implementation slice is explicitly bounded

### Phase 1: substrate plus mock target

Deliverables:

- `swat-core`
- `swat-protocol`
- `swat-session`
- `swat-store`
- `swat-replay`
- `swat-adapter-mock`

Exit criteria:

- a synthetic target can attach
- the session manager can stream events
- the store can persist events and artifacts
- replay can inject recorded boundary outputs

Current implementation note:

- the substrate now has both `InMemoryStore` and durable `FileStore`
  backends under a shared `SwatStore` trait.

### Phase 2: first real target adapter

Preferred first target:

- local process or AI runtime adapter

Deferred:

- PC/GEOS adapter

Exit criteria:

- attach to a live target
- stream meaningful events
- stop and resume execution
- query current state
- replay a captured trace without crossing live boundaries

Current implementation note:

- `swat-adapter-local` now satisfies the first four criteria for a real process
  target. Replay boundaries remain deferred to later AI-native adapters.
- `swat-adapter-python` now adds structured runtime events on top of the same
  substrate, making the first higher-level live adapter path operational.
- `swat-adapter-agent` now adds a language-neutral AI event protocol for
  planner/model/tool/state/policy events on top of the same process/control
  substrate, giving `swat-rs` its first direct agent-runtime adapter.
- `swat-agent-protocol` now provides the first typed SDK for emitting and
  parsing that agent-runtime protocol from Rust code.
- the agent-runtime protocol is now explicitly versioned and validated at the
  adapter boundary so drift becomes a visible lifecycle event instead of an
  implicit parse fallback.
- `sdk/python/swat_agent_protocol.py` now provides the first non-Rust emitter
  SDK for the same protocol, and the agent tests/examples use it directly.
- `sdk/typescript/swat_agent_protocol.ts` now provides the Bun/TypeScript SDK
  for the same protocol, and the agent adapter is validated against it.

### Phase 3: debugger power layer

Deliverables:

- expression/query engine
- schema and type layer
- resolver and source mapper
- semantic trigger engine
- typed value inspection and editing

Exit criteria:

- semantic breakpoints work
- values can be resolved and inspected meaningfully
- source or workflow locations can be mapped from runtime state

Current implementation note:

- the first semantic trigger engine exists in `swat-control`, with artifact and
  summary predicates that can emit `TriggerHit` events and pause live targets.
- the first typed artifact inspection layer exists in `swat-value`, with JSON
  decoding and path-based lookup.
- the first read-only inspection API exists in `swat-api`, with event lookup,
  summary search, and decoded artifact search.
- the first schema layer exists in `swat-schema`, with JSON schema inference,
  validation, and schema-failure trigger predicates.
- the first parsed query language exists in `swat-expr`, and both `swat-api`
  and `swat-control` consume it.
- the first semantic resolver layer exists in `swat-resolver`, and `swat-api`
  can now resolve correlation ids, spans, boundary groups, and named agent
  entities from stored traces.
- the first source-resolution layer exists in `swat-source`, and `swat-api`
  can now resolve structured runtime events back to file-backed source snippets.
- the first scripting layer exists in `swat-script`, using a read-only embedded
  runtime over frozen trace snapshots and the public inspection API.

### Phase 4: operator surfaces

Deliverables:

- command runtime
- AI API
- TUI

Current implementation notes:

- `swat-command` now supports live trigger management plus trigger-save and
  trigger-load over a versioned JSON trigger-file format.
- `swat-command` also now ships as a runnable shell over `mock`, `local`, and
  `agent` targets, with optional durable file-store backing.
- the same shell can now preload saved trigger files at startup through
  `--triggers <path>`.

Exit criteria:

- human and AI clients use the same stable API
- the TUI depends on the substrate rather than embedding logic

Current implementation note:

- the first live operator surface now exists in `swat-command`, which drives a
  live adapter through `SessionManager` while using `swat-api`,
  `swat-resolver`, `swat-source`, and `swat-script` for inspection.
- `swat-command` now also exposes the first live semantic trigger-management
  path by routing `pump` through `swat-control`.

## Operating rules

- Event log before UI.
- Capability model before adapter proliferation.
- Replay before mutation-heavy features.
- Event/artifact split before large-value targets.
- Mock adapter before first real adapter.
- Script host contract before script engine choice.
- No adapter-specific fast path through the core.

## Validation loop

Each completed phase must produce:

- one runnable demo
- one replay trace
- one ADR or ADR update
- one explicit failure-mode test set
