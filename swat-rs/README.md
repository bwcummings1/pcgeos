# swat-rs

`swat-rs` is the new Rust workspace for a modern debugger inspired by
PC/GEOS Swat.

This workspace is intentionally separate from the legacy implementation under
`Tools/swat`. The legacy tree remains the reference system; `swat-rs` is the
new target-neutral architecture and implementation.

The repo also now carries the first non-Rust SDK for the agent protocol under
`sdk/python/`, and now also a TypeScript SDK under `sdk/typescript/`.

The canonical agent handoff documents now live at the workspace root:

- `AGENTS.md`
- `IMPLEMENTATION_PLAN.md`
- `CONTINUATION_PROMPT.md`
- `PROJECT_COMPLETION_PLAN.md`
- `PROJECT_CONTINUATION_PROMPT.md`

## Current status

`swat-rs v1` is complete.

The whole project is not yet complete relative to legacy PC/GEOS Swat parity.
For the remaining post-`v1` work, use `PROJECT_COMPLETION_PLAN.md`.

The workspace now spans the required end-to-end debugger surface for `v1`:

- live mock/local/Python/agent targets
- semantic control, trigger, snapshot, and replay flows
- debugger-grade query/value/schema/source/resolver layers
- shared observation plus capability-gated mutation APIs
- a live command shell and a live terminal UI
- shared scripting and cross-language agent-protocol SDK demos

## Why a separate top-level workspace

- Preserve the historical codebase as a reference oracle.
- Avoid coupling the Rust project to the legacy build system.
- Keep the option to split `swat-rs` into a standalone repository later.

## Working rules

- The event model is the spine of the system.
- Large payloads are artifacts, not inline events.
- Nondeterministic boundaries are recorded and replay-injected.
- Adapters must expose capabilities instead of leaking target assumptions.
- Human and AI clients must use the same API surface.

## Documents

- `docs/architecture/phase-0-spec.md`
- `docs/architecture/reference-map.md`
- `docs/architecture/legacy-subsystem-inventory.md`
- `docs/architecture/execution-strategy.md`
- `docs/architecture/crate-map.md`
- `docs/architecture/agent-event-protocol.md`
- `IMPLEMENTATION_PLAN.md`
- `CONTINUATION_PROMPT.md`
- `PROJECT_COMPLETION_PLAN.md`
- `PROJECT_CONTINUATION_PROMPT.md`
- `AGENTS.md`
- `docs/adrs/0001-core-boundaries.md`
- `docs/adrs/0002-phase-1-substrate-validation.md`
- `docs/adrs/0003-first-live-adapter-local-process.md`
- `docs/adrs/0004-durable-store-trait-and-file-store.md`
- `docs/adrs/0005-semantic-trigger-engine.md`
- `docs/adrs/0006-artifact-value-decoding.md`
- `docs/adrs/0007-trace-inspector-api.md`
- `docs/adrs/0008-json-schema-validation.md`
- `docs/adrs/0009-expression-engine.md`
- `docs/adrs/0010-python-runtime-adapter.md`
- `docs/adrs/0011-source-snippet-resolution.md`
- `docs/adrs/0012-read-only-script-host.md`
- `docs/adrs/0013-ai-agent-runtime-adapter.md`
- `docs/adrs/0014-semantic-trace-resolver.md`
- `docs/adrs/0015-live-command-runtime.md`
- `docs/adrs/0016-command-trigger-management.md`
- `docs/adrs/0017-agent-protocol-sdk.md`
- `docs/adrs/0018-agent-protocol-versioning.md`
- `docs/adrs/0019-command-trigger-persistence.md`
- `docs/adrs/0020-python-agent-protocol-sdk.md`
- `docs/adrs/0021-typescript-agent-protocol-sdk.md`
- `docs/adrs/0022-command-cli-shell.md`
- `docs/adrs/0023-command-until-and-trigger-actions.md`
- `docs/adrs/0024-debugger-grade-inspection-models.md`
- `docs/adrs/0025-terminal-ui-on-shared-debugger-apis.md`

## Phase 1 status

The substrate layer is implemented:

- `swat-core`
- `swat-protocol`
- `swat-store`
- `swat-replay`
- `swat-session`
- `swat-adapter-mock`

These crates are meant to prove the architecture with a real attach/event/store/
replay flow before any UI or production adapter work begins.

`swat-store` now exposes a generic `SwatStore` trait with two concrete backends:

- `InMemoryStore` for fast tests and examples
- `FileStore` for append-only durable events and artifact persistence

## Phase 2 status

The live adapter crates now in the workspace are:

- `swat-adapter-local`
- `swat-adapter-python`
- `swat-adapter-agent`

The first protocol SDK crate is also now in the workspace:

- `swat-agent-protocol`

This adapter spawns and observes a real local process, captures stdout/stderr as
artifact-backed events, and supports pause/resume control for Unix processes.
The Python adapter goes further by emitting structured function call/return/
exception events from traced Python code while still using the same event,
artifact, trigger, and query substrate.
The agent adapter adds a language-neutral AI-event protocol for planner/model/
tool/state/policy records so modern agent runtimes can emit semantic boundaries
directly instead of only generic output lines.
The protocol crate now provides the typed event record and prefixed line emitter/
parser for that same agent-runtime contract.
That contract is now also self-versioning, and protocol mismatches are surfaced
as lifecycle events with the rejected raw line preserved as an artifact.
The first Python SDK for that same contract now lives in `sdk/python/` and is
used by the agent tests and examples.
The first TypeScript SDK for that same contract now lives in `sdk/typescript/`
and is validated through the agent adapter with Bun.

## Phase 3 status

The first semantic-control crate is now in the workspace:

- `swat-control`

This crate evaluates trigger predicates over events and artifact content, emits
`TriggerHit` records, and drives follow-up control actions back through the
session manager.

The first typed value crate is also now in the workspace:

- `swat-value`

This crate decodes artifact-backed values as UTF-8 text, JSON, or binary data,
supports simple JSON-path querying over decoded JSON payloads, and now exposes
operator-friendly compact previews plus multiline detail rendering for shell and
future TUI surfaces.

The first schema crate is also now in the workspace:

- `swat-schema`

This crate infers JSON-oriented schemas from decoded values, validates decoded
artifacts against expected structure, and now feeds schema-failure predicates
into the trigger layer.

The first expression/query crate is also now in the workspace:

- `swat-expr`

This crate parses a semantic query language for event kind, event id, sequence,
correlation id, boundary id, span id, value key, source file/function, summary
text, artifact text, and artifact JSON-path predicates, including `not`. Both
`swat-control` and `swat-api` now use it.

The first source-mapping crate is also now in the workspace:

- `swat-source`

This crate extracts file/line/function locations from structured runtime
artifacts, resolves them to surrounding source snippets, and now surfaces
diagnostic failure reports for synthetic or missing source paths. `swat-api`
now exposes both the snippet lookup path and the richer inspection report.

The first script host is also now in the workspace:

- `swat-script`

This crate provides a read-only embedded scripting surface over frozen trace
snapshots, with helpers for summary search, artifact search, expression-backed
queries, and source lookup.

The first API crate is also now in the workspace:

- `swat-api`

This crate exposes trace inspection primitives over sessions, events, decoded
artifacts, semantic relations, source reports, snapshots, replay plans, and
capability-gated live mutation so later human and agent clients can share the
same substrate.

The first resolver crate is also now in the workspace:

- `swat-resolver`

This crate adds semantic entity indexing for correlation ids, boundary ids,
span ids, tool/model names, planner names, state keys, and source names, plus
cross-entity relation edges and correlation-group views for common runtime
investigation flows.

The first live command runtime is also now in the workspace:

- `swat-command`

This crate provides a small operator shell over a live adapter/store/session,
using the same API, resolver, and script layers that later TUI or agent clients
will use. It now also exposes the first live semantic trigger-management path.

## Phase 4 status

The operator-surface crates now in the workspace are:

- `swat-command`
- `swat-ui-tui`

This crate provides a live command grammar for attach/pump/control/query/entity/
span/source/script operations over the shared substrate.
It now also supports trigger enable/disable, hit counters and last-hit
metadata, action-aware trigger persistence, shell-level `until <expr>`, grouped
help topics, status/session introspection, richer artifact rendering, source
failure reporting, and a runnable CLI shell.

The TUI crate provides a terminal dashboard over the same debugger substrate,
with an event list, entity/span context pane, source preview, artifact preview,
live command entry, and support for mock, local, and agent runtimes. It also
supports a headless render mode for demos and validation.

## Demo

Run the Phase 1 demo with:

`cargo run -p swat-session --example mock_session`

Run the local-process demo with:

`cargo run -p swat-session --example local_process`

Run the Python trace demo with:

`cargo run -p swat-session --example python_trace`

Run the agent trace demo with:

`cargo run -p swat-session --example agent_trace`

Run the live command demo with:

`cargo run -p swat-command --example agent_commands`

Run the trigger-control demo with:

`cargo run -p swat-command --example mock_trigger_controls`

Run the live command shell with:

`cargo run -p swat-command -- mock`

Preload a trigger file on shell startup with:

`cargo run -p swat-command -- --triggers /abs/path/triggers.json mock`

Run the protocol emitter demo with:

`cargo run -p swat-agent-protocol --example emit_protocol`

Run the Python protocol demo with:

`python3 sdk/python/examples/emit_protocol.py`

Run the TypeScript protocol demo with:

`bun run sdk/typescript/examples/emit_protocol.ts`

Run the TUI in interactive mode with:

`cargo run -p swat-ui-tui -- mock`

Run the TUI headless demos with:

`cargo run -p swat-ui-tui -- --headless --ticks 4 mock`

`cargo run -p swat-ui-tui -- --headless --ticks 6 local python3 -u -c "import time; print('hello from local tui'); time.sleep(0.1)"`

`cargo run -p swat-ui-tui -- --headless --ticks 6 agent python3 -u -c "import json,sys,time; P='__SWATAGENT__'; emit=lambda r:(sys.stdout.write(P+json.dumps(r)+'\\n'), sys.stdout.flush()); emit({'kind':'model','phase':'request','span_id':'model-1','correlation_id':'req-9','name':'gpt-4.1-mini','summary':'model requested'}); emit({'kind':'tool','phase':'start','span_id':'tool-1','correlation_id':'req-9','name':'web_search','summary':'tool started'}); emit({'kind':'tool','phase':'end','span_id':'tool-1','correlation_id':'req-9','name':'web_search','summary':'tool completed'}); time.sleep(0.1)"`
