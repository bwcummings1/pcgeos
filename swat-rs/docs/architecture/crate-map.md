# Crate Map

## Phase 1 substrate

- `swat-core`
  Core ids, events, capabilities, policies, shared errors, and public traits.

- `swat-protocol`
  Message framing, request/reply schema, version negotiation, transport
  abstractions.

- `swat-session`
  Session lifecycle, target registry, adapter attachment, capability handshake.

- `swat-store`
  Event/artifact persistence with a generic store trait, in-memory backend, and
  filesystem backend.

- `swat-replay`
  Replay orchestration, boundary injection, deterministic trace execution.

- `swat-adapter-mock`
  Synthetic target used to validate protocol, sessioning, persistence, and
  replay before real adapters exist.

## Phase 2 first real target

- `swat-adapter-local`
  Initial live adapter for a modern local process runtime.

- `swat-adapter-python`
  First higher-level traced runtime adapter with structured Python call/return/
  exception events.

- `swat-agent-protocol`
  Typed emitter/parser SDK for the versioned agent line protocol used by
  `swat-adapter-agent`.

- `sdk/python/swat_agent_protocol.py`
  First non-Rust SDK for the same versioned agent protocol.

- `sdk/typescript/swat_agent_protocol.ts`
  Bun/TypeScript SDK for the same versioned agent protocol.

- `swat-adapter-agent`
  First AI-native runtime adapter with structured planner/model/tool/state/
  policy event normalization.

## Phase 3 debugger power layer

- `swat-value`
  Typed value model and formatting. The first artifact decoding layer is now
  implemented.

- `swat-schema`
  Schema and type system. The first JSON schema inference/validation slice is
  now implemented.

- `swat-resolver`
  Symbol, entity, and resource resolution. The first session/entity/span lookup
  layer is now implemented.

- `swat-source`
  Source and definition mapping. The first file-backed runtime source snippet
  layer is now implemented.

- `swat-expr`
  Query and expression engine. The first parsed event/artifact query language is
  now implemented.

- `swat-control`
  Stop/resume/step/snapshot/trigger execution layer. The first trigger engine
  slice is now implemented.

## Phase 4 operator surfaces

- `swat-api`
  Stable client-facing API for humans and agents. The first trace-inspection
  layer is now implemented.

- `swat-command`
  Live command runtime over a session, adapter, and store. The interactive
  operator shell is implemented with shared debugger-family commands,
  trigger/watchpoint management, snapshots/replay, persisted per-surface
  history, and dashboard-oriented long-session views.

- `swat-script`
  Sandboxed automation runtime. The first read-only embedded script host is now
  implemented.

- `swat-ui-tui`
  Terminal frontend. The live TUI is implemented with event browsing,
  execution/control/target dashboards, shared command entry, persisted command
  history, and a headless render mode for demos and tests.

## Later adapters

- `swat-adapter-http`
- `swat-adapter-pcgeos`
  Fixture-first PC/GEOS adapter over real repository manifests and symbol VMs.
  It keeps the host/target bridge on the shared `TargetAdapter` and
  `swat-session`/`swat-replay` surfaces while deferring live RPC transport work
  to later slices.

## Post-v1 full-completion targets

- `swat-format-pcgeos`
  Support crate for PC/GEOS file headers, VM containers, string tables,
  symbol/object metadata, source/resource mappings, and real repository
  patient/geode/handle/source relationship modeling. It keeps legacy
  repository-format parsing out of `swat-core` while giving the future
  `swat-adapter-pcgeos` and shared operator layers a reusable substrate.

- expanded `swat-command` and `swat-script`
  Completed landing zone for debugger command ecology, shared help/completion
  metadata, long-session history/dashboard workflows, and package-oriented
  script migration from the legacy Tcl surface.

- expanded `swat-ui-tui`
  Completed landing zone for debugger-grade panes covering stack,
  breakpoints/watchpoints, patients/handles/objects, source navigation, replay,
  and other long-session operator workflows.
