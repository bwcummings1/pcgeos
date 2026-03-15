# ADR 0029: Stateful Watchpoint and Time Predicates

- Status: accepted
- Date: 2026-03-15

## Context

After ADR 0028, the breakpoint model could express reusable predicates, group
policies, and typed stop reasons, but Milestone 2 still lacked the stateful
predicate forms needed for watchpoints and time/load break conditions:

- value changes had to be inferred manually outside the trigger engine
- lifecycle/load delays could only be approximated through one-off shell logic
- future object/resource watchpoints needed a target-neutral stateful substrate

Legacy Swat exposed tally, timing, and load-aware breakpoint families as part
of the debugger model. The Rust rewrite needs equivalent concepts without
embedding adapter-private or PC/GEOS-specific semantics into `swat-core`.

## Decision

Extend `swat-control::TriggerPredicate` with stateful, target-neutral forms:

- `ValueChanged { value_key, path }`
  - watches shared `EventPayload::Value` observations and optional JSON-path
    projections over decoded artifacts
- `ObservedAfter { millis }`
  - gates predicates on elapsed observed time relative to the first event seen
    by the trigger engine

These predicates compose with existing `All`, `Any`, named predicates, and
summary/event-kind filters, so lifecycle-aware load conditions can be modeled
without introducing a second watchpoint runtime.

State needed for these predicates remains in `swat-control`, not `swat-core`.

## Consequences

Benefits:

- watchpoints now build on the same shared event/value substrate as breakpoints
- future object/resource watchpoints can project onto `value_key` plus JSON-path
  conventions instead of requiring core target assumptions
- time-gated lifecycle breaks are expressible with composable predicates

Tradeoffs:

- observed-time conditions depend on event timestamps, so they are only as
  precise as the adapter emission cadence
- watchpoints currently trigger on observed value events, not direct target-side
  memory traps
- richer shell/TUI/script workflows still land in the next task

## Follow-on Work

- expose watchpoint and time/load predicate construction across shell, TUI,
  script, and higher-level API helpers
- add richer persistence and presentation for non-expression predicates
- map future PC/GEOS object/resource observers onto the same watchpoint model
