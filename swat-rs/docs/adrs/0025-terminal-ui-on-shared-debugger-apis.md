# ADR 0025: Add a Terminal UI on the Shared Debugger Substrate

## Status

Accepted

## Context

By this point `swat-rs` already had:

- a live shell in `swat-command`
- shared inspection and mutation APIs in `swat-api`
- durable snapshots and replay plans
- debugger-grade query, value, source, and resolver layers

The Definition of Done for `v1` still required a real TUI. The missing piece was
not another control substrate, but a human-facing terminal dashboard that could
inspect a live target without bypassing the shared debugger model.

## Decision

We added `swat-ui-tui` as a new workspace crate.

Its first slice provides:

- an event list
- an entity/span pane
- a source pane
- an artifact pane
- a live command-entry field
- support for `mock`, `local`, and `agent` targets

The TUI is intentionally built above the existing session/control/store stack
and uses `swat-api` inspection/mutation surfaces instead of introducing a new
parallel debugger protocol.

We also added a headless render mode so the TUI can be validated in automated
tests and demo runs without requiring an interactive terminal.

## Consequences

Positive:

- `swat-rs` now has both a shell and a TUI over the same debugger substrate
- event, source, and artifact rendering reuse the richer Phase 3 inspection
  models instead of creating UI-only logic
- headless rendering makes terminal UI validation practical in CI-like
  environments

Negative:

- the command-entry grammar is intentionally smaller than the full shell grammar
- the first TUI focuses on inspection and basic live control, not full trigger
  management or scripting workflows
- the TUI currently uses polling rather than a more elaborate async event loop

## Follow-on work

- decide whether more of `swat-command`'s grammar should be shared directly with
  the TUI command-entry layer
- add richer navigation between entity relations, correlation groups, and
  snapshots
- evaluate whether replay and trigger management deserve dedicated TUI views
