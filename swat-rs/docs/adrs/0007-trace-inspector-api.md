# ADR 0007: Start the Public API Surface with a Read-Only Trace Inspector

## Status

Accepted

## Context

The architecture requires the same substrate to serve human operators and AI
agents, but until now `swat-rs` only exposed lower-level crates:

- session orchestration
- storage
- value decoding
- trigger execution

There was no stable, higher-level inspection surface for reading traces without
knowing all of those crate boundaries directly.

## Decision

We added `swat-api` as the first public API slice.

Its initial role is read-only inspection:

- list session events
- fetch events by id or kind
- decode event artifacts through `swat-value`
- search payload summaries
- search decoded artifact text

## Consequences

Positive:

- the workspace now has a clear API starting point for human and agent clients
- higher-level consumers do not need to compose store/value logic manually
- later interactive and remote APIs can build on a narrower public surface

Negative:

- the API is currently read-only and local-only
- control, mutation, and policy-facing APIs remain future work

## Follow-on work

- add control-facing APIs
- expose trigger management through the API layer
- add remote/API-server frontends over the same inspection primitives
