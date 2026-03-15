# ADR 0030: Shared Advanced Breakpoint and Watchpoint Surfaces

## Context

ADRs 0028 and 0029 moved the control substrate beyond simple semantic triggers:

- `swat-control` can now evaluate named predicates, grouped breakpoint policies,
  stateful value-change watchpoints, and elapsed-time or lifecycle gates
- `swat-api` already projected grouped breakpoint state and predicate inventory
- `swat-command` exposed most breakpoint management from the shell

The remaining Milestone 2 gap was surface-level rather than substrate-level:

- watchpoints were still implicit trigger predicates instead of a first-class
  public API concept
- the TUI could discover many advanced breakpoint commands but still rejected
  them at execution time
- `swat-script` did not yet wrap the richer predicate/group/watchpoint workflows
- persisted trigger files could not faithfully encode watchpoint definitions

That left the shared architecture in an awkward state where the control model
was richer than the operator and automation surfaces built on top of it.

## Decision

Expose advanced breakpoint and watchpoint management uniformly through the
shared public layers instead of introducing shell-local or TUI-local control
paths.

Concretely:

- `swat-api` now models watchpoints explicitly with:
  - `WatchpointSpec`
  - `WatchpointSummary`
  - `WatchpointDetail`
  - `add_watchpoint`
  - `watchpoint_summaries`
  - `watchpoint_detail`
- `swat-command` now adds a first-class `watchpoint` command family and extends
  trigger persistence to a new file-format revision that can encode watchpoint
  definitions and their lifecycle/time gates
- `swat-ui-tui` now executes the advanced breakpoint and watchpoint commands it
  advertises, using `swat-api` rather than a private UI-only runtime path
- `swat-script` now wraps named predicate inventory, breakpoint-group policies,
  and watchpoint mutation/inspection through `LiveScriptSession`

The shared trigger substrate remains the single runtime backend; the new
surface types are projections and constructors over that substrate.

## Consequences

Positive:

- shell, TUI, and script automation now operate on materially the same
  breakpoint/watchpoint concepts
- watchpoints are now inspectable as typed public API objects instead of opaque
  `TriggerPredicate` combinations
- persisted trigger files preserve watchpoint workflows alongside existing
  breakpoint definitions
- future PC/GEOS-facing breakpoint/watchpoint commands can stay above the same
  `swat-api` layer

Tradeoffs:

- trigger persistence format `4` is now required for round-tripping watchpoint
  definitions, though older formats remain readable
- watchpoint command syntax is intentionally constrained to target-neutral
  `value_key`, JSON-path, event-kind, elapsed-time, and summary-gate concepts
  rather than legacy target-specific hardware nomenclature

## Follow-up

- widen the same shared command and script surfaces to the typed frame/local,
  patient, handle, resource, and object inspection layers in Milestones 3-5
- map future PC/GEOS object/resource observers onto the `WatchpointSpec`
  projection instead of creating a second watchpoint API
