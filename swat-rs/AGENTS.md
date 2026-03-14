# AGENTS

This directory is the canonical workspace for the modern `swat-rs` debugger
rewrite inspired by PC/GEOS Swat.

## First read

Before substantial implementation, read in this order:

1. `/home/ubuntu/pcgeos/swat-rs/README.md`
2. `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md`
3. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/phase-0-spec.md`
4. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/reference-map.md`
5. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/legacy-subsystem-inventory.md`
6. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/execution-strategy.md`
7. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/crate-map.md`
8. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/agent-event-protocol.md`
9. all ADRs in `/home/ubuntu/pcgeos/swat-rs/docs/adrs/` in sorted order

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
  finish-line document
- implement milestones in plan order unless blocked
- update docs and ADRs when public or architectural behavior changes
- run focused tests during iteration and full `cargo test` before closing a
  milestone

## Validation baseline

Workspace validation command:

`cargo test`

Useful demos:

- `cargo run -p swat-session --example mock_session`
- `cargo run -p swat-session --example local_process`
- `cargo run -p swat-session --example python_trace`
- `cargo run -p swat-session --example agent_trace`
- `cargo run -p swat-command -- mock`
- `cargo run -p swat-agent-protocol --example emit_protocol`
- `bun run sdk/typescript/examples/emit_protocol.ts`
