# AGENTS

This directory is the canonical workspace for the modern `swat-rs` debugger
rewrite inspired by PC/GEOS Swat.

## First read

Before substantial implementation, read in this order:

1. `/home/ubuntu/pcgeos/swat-rs/README.md`
2. `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md`
3. `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md` when the task is
   to finish the entire project rather than maintain `v1`
4. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/phase-0-spec.md`
5. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/reference-map.md`
6. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/legacy-subsystem-inventory.md`
7. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/execution-strategy.md`
8. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/crate-map.md`
9. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/agent-event-protocol.md`
10. all ADRs in `/home/ubuntu/pcgeos/swat-rs/docs/adrs/` in sorted order

## Core rules

1. Keep all new implementation in `swat-rs`, not `Tools/swat`.
2. Treat the legacy PC/GEOS Swat tree as a reference oracle, not as the place
   to build the new debugger.
3. Do not leak adapter-specific assumptions into `swat-core`.
4. Keep heavyweight payloads in artifacts, not inline in events.
5. Keep UI logic above the substrate.
6. Keep human and AI clients on the same public API surface.

## Execution policy

- use `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md` as the canonical
  finish-line document for `v1`
- use `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md` as the
  canonical finish-line document when the task is to complete the whole
  project beyond `v1`
- implement milestones in plan order unless blocked
- when working beyond `v1`, treat the `## Current Cycle Queue (...)` section in
  `PROJECT_COMPLETION_PLAN.md` as the authoritative execution status record
- update docs and ADRs when public or architectural behavior changes
- run focused tests during iteration and full `cargo test` before closing a
  milestone
- refresh queue artifacts with:
  - `python3 scripts/check-implementation-status.py`
  - `python3 scripts/render-implementation-status.py`
- after each completed queue task: update the queue row, refresh artifacts,
  commit, push, and continue immediately to the next task
- if blocked: mark the task `blocked` and record the exact blocker and next
  action in the queue before pausing

## Validation baseline

Workspace validation command:

`cargo test`

Useful demos:

- `cargo run -p swat-session --example mock_session`
- `cargo run -p swat-session --example local_process`
- `cargo run -p swat-session --example python_trace`
- `cargo run -p swat-session --example agent_trace`
- `cargo run -p swat-command -- mock`
- `cargo run -p swat-ui-tui -- --headless --ticks 4 mock`
- `cargo run -p swat-agent-protocol --example emit_protocol`
- `python3 sdk/python/examples/emit_protocol.py`
- `bun run sdk/typescript/examples/emit_protocol.ts`
