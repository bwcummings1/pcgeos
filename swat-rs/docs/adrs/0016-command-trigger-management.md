# ADR 0016: Add Command-Level Semantic Trigger Management

## Status

Accepted

## Context

`swat-command` provided a live operator shell, but its `pump` path still only
streamed events. The semantic trigger engine already existed in `swat-control`,
yet it was not exposed through the operator surface, which meant the most
important debugger control path remained library-only.

## Decision

We extended `swat-command` so that:

- `pump` always routes through `swat-control::pump_with_triggers`
- the command runtime owns a live `TriggerEngine`
- operators can add, list, and remove expression-based pause triggers
- trigger matches and follow-up control actions are surfaced in the command
  output directly

The first command grammar for this is:

- `triggers`
- `trigger-expr <name> <expr>`
- `trigger-expr-once <name> <expr>`
- `trigger-remove <trigger_id>`

## Consequences

Positive:

- semantic breakpoints are now reachable from the live operator surface
- `swat-command` is materially closer to the original Swat interaction model
- later TUI or agent clients can reuse the same trigger-management substrate

Negative:

- trigger creation is currently expression-only
- action handling is currently limited to pause

## Follow-on work

- richer trigger predicates from command syntax
- non-pause actions
- trigger persistence and multi-session scope
