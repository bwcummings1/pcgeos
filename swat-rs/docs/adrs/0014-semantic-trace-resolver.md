# ADR 0014: Add the First Semantic Trace Resolver Layer

## Status

Accepted

## Context

`swat-rs` already had:

- structured events and artifacts
- typed artifact decoding
- schema validation
- source lookup
- a shared inspection API
- an AI-native agent adapter

What it still lacked was the semantic lookup layer that turns a pile of events
into named entities and relationships. In legacy Swat terms, this is the first
step toward the role played by `Tools/swat/sym.c`, `Tools/swat/patient.c`, and
`Tools/swat/handle.c`: not raw transport, but meaningful names and groupings.

## Decision

We added `swat-resolver` and integrated it into `swat-api`.

The first resolver slice can:

- build a session index of semantic entities
- resolve correlation ids, span ids, boundary ids, value keys, source files,
  function names, and agent-level names for model/tool/planner/state/policy
  records
- group repeated model/tool events into logical boundary spans
- return all events for a named entity or a correlation id

## Consequences

Positive:

- the public API now has a first-class semantic lookup surface
- agent traces can be navigated by tool/model/span/correlation names instead of
  only by event ids or free-text search
- later UI and scripting work can reuse a stable resolver substrate

Negative:

- this first resolver is artifact-driven, so richer target-native symbol spaces
  remain future work
- entity extraction is currently optimized for the event shapes emitted by the
  first adapters rather than a fully general schema registry

## Follow-on work

- richer entity graphs and causality traversal
- cross-session and distributed target resolution
- schema-aware entity extraction for non-agent targets
