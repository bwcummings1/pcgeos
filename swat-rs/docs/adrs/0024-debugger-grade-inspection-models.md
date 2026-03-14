# ADR 0024: Deepen Query, Value, Source, and Resolver Models for Debugger-Grade Inspection

## Status

Accepted

## Context

By the end of Milestone 3, `swat-rs` already had:

- decoded artifact-backed values
- a shared query parser
- source snippet lookup
- a first semantic resolver
- a public inspection API

That stack was good enough for a trace viewer, but still shallow for a debugger.
Operators could search summary text and artifact paths, yet common workflows
still needed manual stitching:

- stop on a specific event id or sequence number
- follow a correlation/span/boundary through a live trace
- distinguish source lookup failures from simply missing metadata
- inspect artifacts in shell-friendly compact form while still keeping a richer
  multiline rendering for deeper inspection
- understand how named entities relate to each other across a session

## Decision

We deepened the Phase 3 inspection crates instead of inventing new side
surfaces.

The shared query layer in `swat-expr` now supports:

- `event.id`
- `sequence`
- `correlation`
- `boundary`
- `span`
- `value.key`
- `source.file`
- `source.function`
- unary `not`

The value layer in `swat-value` now exposes structured presentations with:

- compact previews suitable for one-line shell/TUI rendering
- multiline detail formatting for text, JSON, and binary values
- byte and line metadata for downstream UIs

The source layer in `swat-source` now exposes an inspection report with:

- optional resolved location
- optional source snippet
- classified failures such as synthetic paths and missing files

The resolver layer in `swat-resolver` now exposes:

- entity-to-entity relation edges based on shared events
- correlation-group views with related spans, boundaries, and entities
- convenience traversal for spans, value keys, and source files

`swat-api` now surfaces these richer inspection models directly so the command
runtime, scripts, and future TUI stay on one public substrate.

## Consequences

Positive:

- semantic queries now cover the identifiers operators actually navigate by
- shell and future TUI clients can render artifacts without reimplementing
  formatting heuristics
- source inspection can distinguish "no location metadata" from "location is
  known but cannot be resolved"
- entity navigation now includes relationship and grouping context instead of
  only flat search results

Negative:

- resolver relations are still artifact-driven rather than symbol-table-driven
- source failures are classified heuristically from the available runtime
  metadata
- query expressions remain intentionally simple and do not yet include numeric
  comparisons beyond equality

## Follow-on work

- drive the TUI panes from value presentations, source inspection, and resolver
  relation/group data
- add schema-aware or type-aware query predicates when real symbol data exists
- decide whether richer comparison operators belong in `swat-expr` or a later
  typed expression layer
