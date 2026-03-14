# ADR 0012: Start `swat-script` with a Read-Only Embedded Runtime Over Frozen Traces

## Status

Accepted

## Context

`swat-rs` now has:

- a shared query language
- a public trace-inspection API
- source lookup
- semantic triggers

That is enough substrate to support automation, but exposing live mutable state
directly to a script engine this early would blur the policy and capability
boundaries we have been deliberately preserving.

## Decision

We added `swat-script` as a read-only embedded script host.

The initial host:

- snapshots a session trace into an owned read-only store
- embeds a small scripting runtime
- exposes inspection helpers over that frozen trace:
  - `event_count`
  - `summary_search_count`
  - `artifact_search_count`
  - `query_count`
  - `first_summary`
  - `source_contains`

## Consequences

Positive:

- the project now has a scripting boundary without exposing live adapter internals
- scripts consume the same query/API/source semantics as other clients
- future UI and agent automation can build on a tested script host

Negative:

- the host is currently inspection-only
- scripts cannot yet initiate control actions or mutate target state

## Follow-on work

- add policy-gated control helpers
- surface trigger management through the script runtime
- evaluate whether this embedded runtime remains the right long-term choice
