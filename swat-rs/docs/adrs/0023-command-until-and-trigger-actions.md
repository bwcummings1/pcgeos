# ADR 0023: Command-Level `until`, Trigger Action Persistence, and Runtime Trigger State

- Status: accepted
- Date: 2026-03-14

## Context

`swat-command` already exposed expression-backed pause triggers plus trigger
save/load, but Milestone 1 still had three debugger-shaping gaps:

- trigger actions were effectively pause-only at the shell surface
- operators could not disable or re-enable a trigger without deleting it
- there was no shell-level `until <expr>` command that continued execution
  until a semantic condition matched

The persisted trigger file was also too narrow for this next slice. Version 1
stored only name, expression, and `fire_once`, which meant action type and
enabled state were lost across saves.

## Decision

Extend the command layer rather than pushing shell concerns into `swat-control`.

`swat-command` now provides:

- `trigger-snapshot <name> <expr> <reason>`
- `trigger-enable <trigger_id>`
- `trigger-disable <trigger_id>`
- `until <expr>`

The shell-level `until` implementation reuses the existing trigger engine by
installing a temporary fire-once pause trigger, resuming the target, and
pumping until the temporary trigger matches or the target exits.

Trigger persistence is versioned forward:

- trigger file `format_version = 2` now stores `enabled` and `actions`
- version 1 files are still accepted for compatibility
- version 1 loads default to `enabled = true` and `PauseTarget`

Runtime trigger state is now surfaced explicitly through the command layer:

- hit counters
- last-hit event id
- last-hit sequence number

## Consequences

Benefits:

- the shell is materially closer to a real semantic-breakpoint workflow
- non-pause trigger behavior can be demonstrated where adapter capabilities
  allow it
- operators can temporarily suppress a trigger without losing its definition
- persisted trigger profiles now preserve the parts of trigger behavior that
  matter operationally

Tradeoffs:

- `until` currently depends on resume-capable targets and uses a polling loop
- trigger runtime state is intentionally in-memory and not persisted
- the file format now needs compatibility handling across versions

## Follow-on work

- expose the same trigger mutation and `until` semantics through `swat-api`
- decide whether `until-kind <EventKind>` is worth keeping as a separate shell
  convenience once the expression language grows further
- extend snapshot actions once durable snapshot inventory and replay inspection
  are complete
