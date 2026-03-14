# ADR 0026: Shared Command Registry and First Debugger Command Families

- Status: accepted
- Date: 2026-03-14

## Context

`swat-command` and `swat-ui-tui` had both reached the Phase 4 operator layer,
but command discovery was still fragmented:

- shell help lived as shell-local strings
- the TUI kept a separate command parser and separate discovery text
- debugger workflows such as stack/source/breakpoint inspection still appeared
  mostly as flat commands instead of explicit command families

That kept Milestone 1 blocked on operator vocabulary rather than debugger
substance. The next slice needed a shared place to describe commands before
adding more debugger families.

## Decision

Add a shared command/help registry inside `swat-command`.

The registry is now the source of truth for:

- family-level help topics
- command synopsis and summaries
- shell versus TUI discovery markers

The first debugger-oriented command families built on top of the existing APIs
and trigger engine are:

- `stack`
  - maps to boundary-span inspection on the shared resolver APIs
- `source`
  - maps to event-backed source inspection and source-file lookups
- `breakpoint`
  - maps to the existing semantic trigger engine rather than introducing a
    second control model

The shell keeps all pre-existing commands for compatibility, but now also
parses the higher-level family aliases.

The TUI now consumes the same registry for help/discovery and uses the shared
shell parser for the subset of commands it supports today.

## Consequences

Benefits:

- shell help and TUI discovery now move together
- Milestone 1 command-family work can grow without duplicating help text
- operators can start using debugger vocabulary without regressing the stable
  `v1` command surface

Tradeoffs:

- the TUI still executes only a subset of the full shell command surface
- `stack` currently reflects boundary spans rather than full frame objects
- `breakpoint` is intentionally implemented as a debugger-facing projection of
  semantic triggers until the richer stop/break/watch model lands

## Follow-on work

- move more TUI command entry paths onto the shared command runtime where it is
  practical
- deepen `stack` from boundary spans into richer frame-oriented inspection as
  the typed inspection milestone lands
- evolve `breakpoint` from trigger-backed aliases into the fuller grouped
  breakpoint/watchpoint model planned for Milestone 2
