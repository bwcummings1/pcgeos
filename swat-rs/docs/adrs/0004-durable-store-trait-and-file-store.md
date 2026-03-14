# ADR 0004: Generalize Storage Behind `SwatStore` and Add a Durable `FileStore`

## Status

Accepted

## Context

The initial `swat-rs` substrate used only `InMemoryStore`. That was enough to
validate session orchestration and replay logic, but it left one critical gap:
traces vanished when the process exited.

For AI and systems debugging, durable traces are not optional. Replay,
post-mortem analysis, and multi-session inspection all depend on persisted
events and artifacts.

## Decision

`swat-store` now exposes a `SwatStore` trait. The existing `InMemoryStore`
implements it, and a new `FileStore` provides append-only persistence for:

- event envelopes in `events.jsonl`
- artifact metadata in `artifacts.jsonl`
- artifact bytes under `artifacts/<artifact_id>.bin`

The session manager is generic over `SwatStore`, so it no longer depends on one
concrete store backend.

## Consequences

Positive:

- traces can survive process restarts
- the session and replay layers now depend on a storage contract rather than a
  single in-memory implementation
- future backends such as SQLite or object storage can fit the same boundary

Negative:

- the file store is intentionally simple and append-only in this slice
- event queries are still memory-backed after load, not indexed on disk

## Follow-on work

- add richer indexes for large traces
- add snapshot persistence
- add retention and compaction policies
