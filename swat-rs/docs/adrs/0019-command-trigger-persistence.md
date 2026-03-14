# ADR 0019: Command Trigger Persistence

- Status: accepted
- Date: 2026-03-14

## Context

`swat-command` already exposed live semantic trigger management, but trigger
definitions were ephemeral. That was acceptable for tests and demos, but it
left the first operator surface weaker than the debugger model it is trying to
replace.

An operator should be able to preserve a useful semantic breakpoint set and
restore it in a later session without manually retyping each expression.

## Decision

Trigger persistence is implemented in `swat-command`, not in `swat-control`.

- `trigger-save <path>` writes command-managed expression triggers to a JSON
  file
- `trigger-load <path>` replaces the current command-managed trigger set from
  that JSON file
- the file format is versioned with `format_version = 1`

The persisted shape is intentionally narrow:

- name
- expression source
- `fire_once`

`swat-command` rebuilds runtime `Trigger` values from this file and currently
restores them with the same `PauseTarget` action used by command-created
triggers.

## Consequences

Benefits:

- the first operator surface now has durable semantic breakpoints
- trigger persistence stays local to the command layer instead of expanding the
  core trigger engine for one surface
- the file format has an explicit version boundary for future evolution

Tradeoffs:

- only expression-based command triggers are currently persisted
- loading replaces the current command-managed trigger set instead of merging

## Follow-on work

- support merge semantics or named trigger profiles if operator usage demands it
- extend the persisted format once non-pause trigger actions exist
- expose the same persistence shape through future TUI or remote control layers
