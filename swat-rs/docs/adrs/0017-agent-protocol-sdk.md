# ADR 0017: Add a Typed Agent Protocol SDK

## Status

Accepted

## Context

The AI-native line protocol already existed in `swat-rs`, but only as:

- prose documentation
- hand-written JSON in tests and examples
- ad hoc parsing logic inside `swat-adapter-agent`

That made the protocol easy to drift and harder for real agent runtimes to
adopt safely.

## Decision

We added `swat-agent-protocol`.

Its first slice provides:

- a typed `AgentEventRecord`
- a stable `AgentEventKind`
- prefixed line encoding/parsing helpers
- a `LineEmitter` for writing protocol records to stdout or any other writer

`swat-adapter-agent` now consumes this crate directly instead of duplicating the
protocol shape internally.

## Consequences

Positive:

- the agent protocol now has one canonical implementation point
- adapter/runtime integration is less brittle
- Rust-based agent runtimes can emit valid records without hand-written JSON

Negative:

- richer language SDKs outside Rust are still future work
- the first SDK focuses on line emission/parsing, not transport abstraction

## Follow-on work

- Python and TypeScript helper libraries
- protocol versioning and compatibility tests
- richer builder helpers for common planner/model/tool patterns
