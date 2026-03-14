# IMPLEMENTATION_PLAN

This is the canonical implementation plan for completing `swat-rs v1`.

`v1` is now complete.

If the task is to complete the whole project beyond `v1` and close the
remaining legacy-Swat parity gap, use:

- `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md`

If any other planning document conflicts with this file, this file wins.

## Objective

Complete `swat-rs v1` as a modern debugger platform inspired by PC/GEOS Swat,
while preserving contextual coherence with the historical system and keeping
the new implementation target-neutral and AI-native.

This is not a source port of legacy Swat. It is a first-principles rewrite
whose design is constrained by real legacy reference material.

## Canonical reference set

Read these before major implementation:

1. `/home/ubuntu/pcgeos/swat-rs/AGENTS.md`
2. `/home/ubuntu/pcgeos/swat-rs/README.md`
3. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/phase-0-spec.md`
4. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/reference-map.md`
5. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/legacy-subsystem-inventory.md`
6. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/execution-strategy.md`
7. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/crate-map.md`
8. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/agent-event-protocol.md`
9. `/home/ubuntu/pcgeos/swat-rs/docs/adrs/` in sorted order

Legacy technical anchors live in:

- `/home/ubuntu/pcgeos/Tools/swat`
- `/home/ubuntu/pcgeos/Tools/include`
- `/home/ubuntu/pcgeos/Tools/glue`
- `/home/ubuntu/pcgeos/Tools/utils`

Do not implement the modern debugger in those trees unless explicitly asked.

## Current state

As of `2026-03-14`, `swat-rs v1` is complete and the workspace includes:

- substrate:
  - `swat-core`
  - `swat-protocol`
  - `swat-store`
  - `swat-session`
  - `swat-replay`
  - `swat-adapter-mock`
- live adapters:
  - `swat-adapter-local`
  - `swat-adapter-python`
  - `swat-adapter-agent`
- protocol SDKs:
  - `swat-agent-protocol`
  - `sdk/python/swat_agent_protocol.py`
  - `sdk/typescript/swat_agent_protocol.ts`
- inspection and control layers:
  - `swat-control`
  - `swat-value`
  - `swat-schema`
  - `swat-expr`
  - `swat-resolver`
  - `swat-source`
  - `swat-api`
  - `swat-script`
- operator surface:
  - `swat-command` runtime
  - `swat-command` CLI shell
  - `swat-ui-tui`
  - trigger persistence
  - trigger preload support
  - trigger enable/disable
  - action-aware trigger persistence
  - shell-level `until <expr>`
  - headless TUI demo mode for mock/local/agent validation

The inspection stack now also includes:

- durable snapshot inventory and replay inspection
- capability-gated public mutation/control APIs
- debugger-grade query fields for event ids, sequence numbers, correlation ids,
  boundary ids, span ids, value keys, and source locations
- artifact presentation models, source failure diagnostics, and resolver
  relation/correlation-group views

Reality check:

- relative to original PC/GEOS Swat as a full debugger system: about `40%`
- relative to the modern `swat-rs` architecture target: `100%`

Approximate subsystem completion:

- core substrate: `~100%`
- adapters and protocol parity: `~100%` for the `v1` target set
- query/value/source/resolver: `~100%`
- operator shell: `~100%`
- Swat-like debugger feel: `~60%`

## What is required for `v1 complete`

All of the following must be true:

1. the core substrate remains stable and green
2. live control and semantic breakpoints work for local, Python, and agent
   runtimes
3. snapshots and replay are durable and inspectable
4. the public API supports both observation and capability-gated mutation
5. the operator experience includes a real shell and a real TUI
6. scripting and automation use the same public API surface
7. query/value/schema/source/resolver layers are debugger-grade rather than
   trace-viewer-grade
8. docs, ADRs, demos, and tests are sufficient for another engineer to continue
   without chat history

## What is not required for `v1 complete`

These are valuable, but not blocking for `v1`:

- a full PC/GEOS target adapter
- historical symbol/object-format compatibility
- a distributed trace fabric
- a web UI
- IDE integrations
- published SDK packages to PyPI/npm

## Non-negotiable rules

1. Keep the modern implementation inside `/home/ubuntu/pcgeos/swat-rs`.
2. Do not leak target-specific details into `swat-core`.
3. Keep heavy payloads in artifacts, not inline events.
4. Keep UI logic above the substrate.
5. Treat legacy Swat as a reference oracle, not the implementation site.
6. Every meaningful architecture change must be reflected in docs and, when
   appropriate, a new ADR.
7. Do not claim completion until the `Definition of Done` section below is
   fully satisfied.

## Primary code seams

These are the files another agent is most likely to modify repeatedly:

- `/home/ubuntu/pcgeos/swat-rs/swat-core/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-session/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-store/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-replay/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-control/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-command/src/main.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-resolver/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-source/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-script/src/lib.rs`

## Milestone order

Implement in this order unless a hard blocker forces resequencing.

### Milestone 1: Control and trigger maturation

Goal:

- close the debugger-control gap between the current trace-driven shell and a
  real breakpoint/watchpoint subsystem

Required work:

- extend `TriggerAction`
  - `PauseTarget`
  - `CreateSnapshot { reason }`
  - optionally `ResumeTarget`
- add trigger enable/disable
- add hit counters and last-hit metadata
- add shell commands:
  - `trigger-enable <id>`
  - `trigger-disable <id>`
  - `trigger-snapshot <name> <expr> <reason>`
  - `until <expr>`
  - optionally `until-kind <EventKind>`
- preserve or version trigger-file persistence

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/break.c`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/bptutils.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/hwbrk.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/timebrk.tcl`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-control/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/main.rs`

Exit criteria:

- triggers can be toggled without deletion/recreation
- users can run until a semantic condition matches
- snapshot actions from triggers work where capabilities allow them
- tests and shell coverage exist

### Milestone 2: Snapshot and replay maturation

Goal:

- make snapshots and replay first-class, durable debugger concepts

Required work:

- define richer snapshot metadata if needed
- persist snapshot inventory in `swat-store`
- expose snapshot list and inspection in `swat-api`
- add shell commands:
  - `snapshots`
  - `snapshot-show <snapshot_id>`
  - `replay <snapshot_id|boundary_id>`
- strengthen mock-adapter snapshot and replay coverage

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/rpc.c`
  - `/home/ubuntu/pcgeos/Tools/swat/Doc/stub.ms`
  - `/home/ubuntu/pcgeos/Tools/swat/Stub/rpc.asm`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-core/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-store/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-replay/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-session/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`

Exit criteria:

- snapshots are durable and enumerable
- replay is inspectable from public APIs
- shell users can create and inspect snapshots

### Milestone 3: Mutation-capable API and policy controls

Goal:

- move beyond read-only observation while preserving capability and safety
  boundaries

Required work:

- extend `swat-api` with control and trigger CRUD
- add capability-gated mutation entry points
- emit audit/policy events for mutation attempts
- extend `swat-script` to use public mutation APIs

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/event.c`
  - `/home/ubuntu/pcgeos/Tools/swat/tclDebug.c`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-core/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-script/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`

Exit criteria:

- public APIs expose control and trigger management
- mutation paths are capability-gated and auditable
- scripting uses the same public substrate as the shell

### Milestone 4: Query/value/source/resolver deepening

Goal:

- make the inspection layers feel like debugger primitives rather than just
  structured log helpers

Required work:

- deepen `swat-expr` with more debugger-relevant fields:
  - event id
  - sequence number
  - correlation id
  - boundary id
  - span id
  - value key
  - source file/function
  - optionally negation
- deepen `swat-resolver` with stronger entity relations and grouping
- deepen `swat-source` snippet and failure handling
- deepen `swat-value` preview/formatting behavior
- extend `swat-api` to expose the richer model

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/sym.c`
  - `/home/ubuntu/pcgeos/Tools/swat/type.c`
  - `/home/ubuntu/pcgeos/Tools/swat/expr.c`
  - `/home/ubuntu/pcgeos/Tools/swat/value.c`
  - `/home/ubuntu/pcgeos/Tools/swat/src.c`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-expr/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-value/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-resolver/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-source/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`

Exit criteria:

- users can express debugger-grade semantic queries
- source/entity lookup covers common runtime workflows
- tests cover new fields and relations

### Milestone 5: Operator surfaces

Goal:

- deliver a serious human-facing debugger surface

Required work:

- harden `swat-command`
  - better session introspection
  - improved help and command ergonomics
  - shell-friendly output where needed
- build `swat-ui-tui`
  - event list
  - entity/span pane
  - source/artifact preview
  - command entry
  - attach to at least `mock`, `local`, and `agent`

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/ui.c`
  - `/home/ubuntu/pcgeos/Tools/swat/help.c`
  - `/home/ubuntu/pcgeos/Tools/swat/curses.c`
  - `/home/ubuntu/pcgeos/Tools/swat/hist/*`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/main.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-ui-tui/` (new)

Exit criteria:

- shell workflows are stable and testable
- the TUI exists and depends on public APIs instead of duplicating logic

### Milestone 6: Quality and finish-line validation

Goal:

- leave a coherent, handoff-safe `v1`

Required work:

- update docs and ADRs for every architectural change
- ensure demos exist for:
  - mock
  - local
  - python
  - agent
  - shell
  - TUI
- ensure cross-language protocol demos still work
- ensure full workspace tests are green

Exit criteria:

- the Definition of Done section is fully satisfied

## Validation commands

Focused validation will vary by slice, but the baseline commands are:

```bash
cargo test
cargo run -p swat-session --example mock_session
cargo run -p swat-session --example local_process
cargo run -p swat-session --example python_trace
cargo run -p swat-session --example agent_trace
cargo run -p swat-command -- mock
cargo run -p swat-agent-protocol --example emit_protocol
bun run sdk/typescript/examples/emit_protocol.ts
```

## Definition of Done

Do not call `swat-rs` complete until all of the following are true:

1. `cargo test` passes for the full workspace
2. Rust, Python, and TypeScript/Bun protocol demos all run
3. `swat-command` shell can:
   - attach
   - query
   - inspect artifacts
   - manage triggers
   - preload trigger profiles
   - stop `until` a semantic condition
   - inspect snapshots
4. `swat-ui-tui` exists and can inspect at least one live target
5. snapshots and replay are durable and exposed through public APIs
6. `swat-api` supports observation plus capability-gated mutation/control
7. scripting uses the same public substrate for reads and allowed mutations
8. docs and ADRs match the actual codebase state

## Recommended execution loop

Repeat this loop until the Definition of Done is satisfied:

1. choose the next milestone in order
2. inspect only the files relevant to that milestone
3. implement one coherent slice
4. add or update focused tests and demos
5. update docs and ADRs if behavior changed
6. run targeted tests first
7. run full `cargo test`
8. then move to the next slice

## Immediate next task

Start with Milestone 1.

The highest-value first slice is:

- extend `TriggerAction` beyond pause-only behavior
- add trigger enable/disable
- add shell-level `until <expr>`

That work is already close to the active code seams and closes a major debugger
experience gap.
