# 0038. Shared Operator Dashboards and Persisted History

Date: 2026-03-15

## Status

Accepted

## Context

By the end of Milestone 6, the shell and TUI exposed the same shared debugger
command families, but the day-to-day operator experience was still closer to a
demo than a serious debugger:

- the shell had readline history only for the current process
- the TUI had transient command history and one fixed pane layout
- replay, breakpoint, patient/object, and source workflows existed, but they
  were spread across commands rather than consolidated into long-session views

Milestone 7 required stronger operator parity without pushing UI-specific state
down into `swat-core` or bypassing the shared public APIs.

## Decision

Keep dashboard and history behavior in the operator layer and share as much of
the surface as possible through `swat-command`.

The completed design:

- adds shared `dashboard` and `history` commands to the command registry and
  parser
- persists shell and TUI command history per surface in the user state
  directory rather than in lower debugger layers
- keeps shell dashboard rendering on top of `swat-api` inspection helpers
- keeps TUI dashboard layout state in `swat-ui-tui`, while reusing the same
  shared command/help/completion metadata and inspection APIs

## Consequences

Positive:

- shell and TUI now expose aligned debugger workflows for execution, control,
  and target-navigation views
- long-session operator history survives restarts without contaminating
  `swat-core` or adapter traits with UI concerns
- completion/help/search remain shared because dashboard/history are part of the
  common command surface

Tradeoffs:

- persisted history is intentionally per surface, so shell and TUI retain
  separate command logs
- dashboard layout selection is TUI state, while shell dashboards remain
  rendered snapshots rather than a persistent terminal layout
