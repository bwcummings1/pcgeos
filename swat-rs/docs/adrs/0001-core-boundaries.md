# ADR 0001: Separate `swat-rs` Workspace and Freeze Core Boundaries First

## Status

Accepted

## Context

The legacy PC/GEOS repository contains a historically rich debugger under
`Tools/swat`, but its implementation is tightly bound to GEOS, 8086-oriented
machine assumptions, custom symbol formats, and a large Tcl runtime.

The goal of `swat-rs` is broader: create a modern Rust debugger substrate that
retains Swat's strengths while supporting modern AI and systems workflows.

There were three immediate risks:

- mixing a new Rust implementation into the legacy Swat tree
- designing the protocol/store/session layers without a live target
- treating large payloads as normal event payloads

## Decision

We will:

1. create a new top-level workspace at `swat-rs/`
2. complete a Phase 0 architecture freeze before writing substrate code
3. model events and artifacts as separate first-class concepts
4. treat nondeterministic boundaries as explicit replay boundaries
5. include a mock adapter in Phase 1 before the first real adapter

## Consequences

Positive:

- the old code remains intact as a reference oracle
- the new core can stay target-neutral
- replay and storage concerns are addressed before feature growth
- the protocol and session model are tested against a real synthetic target

Negative:

- implementation begins more slowly than an immediate code-first prototype
- some decisions, especially scripting engine choice, remain deferred until
  after the host contract is frozen

## Notes

This ADR freezes the direction of travel, not the final implementation details
of every crate. The next ADRs should cover:

- protocol and transport model
- policy model
- scripting runtime selection
