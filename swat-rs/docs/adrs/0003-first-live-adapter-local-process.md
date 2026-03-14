# ADR 0003: Use a Local Process as the First Live `swat-rs` Target

## Status

Accepted

## Context

After validating the substrate with `swat-adapter-mock`, `swat-rs` needed a
real target that could exercise the same session, store, and protocol layers
without importing Python runtime semantics, AI workflow semantics, or PC/GEOS
format complexity too early.

The first live target had to prove:

- attach against a real operating-system process
- lifecycle observation
- real artifact capture
- stop/resume control
- compatibility with the existing event/artifact split

## Decision

The first live adapter will be `swat-adapter-local`, which spawns a local
process and treats it as a target.

The adapter will initially support:

- spawn-on-attach
- stdout/stderr capture as artifact-backed `ValueObserved` events
- lifecycle events for spawn and exit
- pause/resume via Unix signals
- capability reporting through `CapabilitySet`

The adapter will not initially support:

- replay injection
- source resolution
- schema resolution
- typed value inspection beyond captured stream artifacts
- snapshots
- single-step execution

## Consequences

Positive:

- the real attach/control path is proven on top of the existing substrate
- the event/artifact split is exercised against real process output
- pause/resume semantics are validated before more complex runtimes are added

Negative:

- the adapter is intentionally Unix-oriented in this first slice
- it does not yet model AI-specific boundaries such as model/tool calls

## Follow-on work

- add a richer process-state model
- add a Python or agent-runtime adapter
- introduce replay-aware live adapters that expose nondeterministic boundaries
