# ADR 0010: Add a Traced Python Runtime Adapter as the First Higher-Level Live Target

## Status

Accepted

## Context

`swat-rs` already had:

- a validated substrate
- a local-process adapter
- semantic triggers
- value decoding
- schema validation
- a shared query language

But it still lacked a higher-level runtime adapter that emitted structured
semantic events from inside the target process itself. For AI development, a
runtime like Python is a more realistic proving ground than raw process stdout.

## Decision

We added `swat-adapter-python`.

Its initial design:

- spawns `python3 -u -c <bootstrap>`
- injects a tracing bootstrap via environment variables
- emits structured JSON trace records for Python `call`, `return`, and
  `exception` events
- converts those trace records into artifact-backed `Execution` events
- still captures plain stdout/stderr through the existing value/event path
- supports pause/resume using the same Unix signal model as `swat-adapter-local`

## Consequences

Positive:

- `swat-rs` now has a real higher-level runtime target
- the trigger, schema, value, and expression layers are exercised against a
  nontrivial runtime event stream
- Python becomes a practical bridge toward AI workflow adapters

Negative:

- this first slice is Unix-oriented and process-based
- replay injection and source-level stepping are still not implemented

## Follow-on work

- richer Python state capture
- Python frame/local inspection
- agent/runtime adapters that emit model/tool/planner events directly
