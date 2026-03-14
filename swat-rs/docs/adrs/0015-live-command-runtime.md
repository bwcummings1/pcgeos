# ADR 0015: Add a Live Command Runtime on Top of the Shared API

## Status

Accepted

## Context

`swat-rs` already had:

- live adapters
- semantic triggers
- typed values and schemas
- source lookup
- semantic resolution
- a read-only API
- a read-only script host

But it still lacked a usable operator surface for a live session. Legacy Swat
was not just a library; it was an interactive command environment layered on
top of a strong debugger core.

## Decision

We added `swat-command`.

Its first slice:

- owns a live `SessionManager`, `TargetAdapter`, and `SwatStore`
- exposes a small command grammar over the shared API
- supports attach, pump, pause, resume, step, snapshot, events, query,
  entities, correlation, boundary span, source, artifact, and script commands
- uses the same `swat-api`, `swat-resolver`, and `swat-script` layers rather
  than a separate command-only model

## Consequences

Positive:

- `swat-rs` now has the first real Phase 4 operator surface
- both human and AI clients have a concrete command substrate before any TUI
- the script host is now reachable through a live command path

Negative:

- the command grammar is intentionally small and not yet a full debugger shell
- multi-session orchestration and trigger management are still future work

## Follow-on work

- command-level trigger management
- remote/streaming command transport
- a TUI or IDE layer on top of the same command substrate
