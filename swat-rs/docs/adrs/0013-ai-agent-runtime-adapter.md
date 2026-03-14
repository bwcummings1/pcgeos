# ADR 0013: Add an AI-Native Agent Runtime Adapter and Structured Event Protocol

## Status

Accepted

## Context

`swat-rs` already had:

- substrate/session/store/replay validation
- a local-process adapter
- a traced Python adapter
- semantic triggers, typed values, schema checks, source lookup, and a public
  inspection API

But it still lacked the most important modern proving ground: an adapter that
speaks in planner/model/tool/state terms directly instead of forcing AI
workflows to look like generic stdout or raw runtime traces.

This is the modern analogue of the legacy Swat split between:

- host-side event/control orchestration in `Tools/swat/event.c`,
  `Tools/swat/rpc.c`, and `Tools/swat/break.c`
- target-side boundary reporting in `Tools/swat/Stub/rpc.asm`
- source-oriented inspection in `Tools/swat/src.c`

## Decision

We added `swat-adapter-agent`.

Its initial design:

- spawns a local child process
- treats stdout lines with a reserved prefix as structured agent events
- persists each structured event as a JSON artifact
- normalizes those records into `Execution`, `ModelBoundary`,
  `ToolBoundary`, `StateMutation`, `PolicyDecision`, and related event kinds
- preserves `span_id`/`correlation_id` as causality metadata
- reuses boundary ids across start/end style model and tool spans
- still captures non-protocol stdout/stderr as ordinary observed values
- supports pause/resume using the same Unix signal control model as the other
  live process adapters

## Consequences

Positive:

- `swat-rs` now has an AI-native event surface instead of only runtime-level
  traces
- semantic triggers can stop on model/tool/planner/state failures using the
  same shared event/query substrate
- source resolution works for agent events as long as the runtime includes
  `file` and `line` metadata in the structured record
- later framework-specific adapters can target this semantic shape rather than
  inventing their own incompatible event vocabularies

Negative:

- this first slice is still local-process based and Unix oriented
- replay injection is still deferred
- the structured event protocol is intentionally small, so richer streaming or
  side-channel artifact transport remains future work

## Follow-on work

- adapter SDKs for Python/Rust/TypeScript runtimes that emit the protocol
- replay-aware agent boundaries for model/tool responses
- richer agent stack frames, planner locals, and live mutation controls
