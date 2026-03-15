# ADR 0033: Debugger-Native Query, Value, and Source Traversals

## Context

By the end of `T-012`, Milestone 3 could inspect typed frames and typed target
entities, but a practical debugger workflow still had two gaps:

- the expression engine could only filter a narrow event-centric field set
- source and value navigation still pushed operators back toward raw events
  instead of direct debugger-shaped traversals

Legacy Swat did not stop at event filtering. Its expression, value, and source
layers were all traversal tools: operators moved from names to values, from
values to objects, and from addresses to source locations without rebuilding
those workflows in every UI surface.

## Decision

Extend the shared inspection surface in three coordinated ways:

- widen `swat-expr` to query debugger-native identity and source fields:
  - `patient`
  - `handle`
  - `resource`
  - `object`
  - `source.line`
- add source-function traversal on top of the shared resolver and source
  metadata, not shell-local file scanning
- add observed-value history summaries/details on top of shared
  `EventPayload::Value` records and decoded artifact previews

Concretely:

- `swat-api` now exposes:
  - `source_functions`
  - `events_for_source_function`
  - `observed_values`
  - `observed_value_detail`
- `swat-command` and `swat-ui-tui` now expose:
  - `source functions`
  - `source function <name>`
  - `value`
  - `value show <value_key>`
- `swat-script` now exposes matching function/value traversal helpers instead
  of forcing scripts to reconstruct those paths from raw events

## Consequences

Positive:

- query expressions can target debugger-native identities directly instead of
  only generic event metadata
- source traversal can begin from functions as well as files/events
- value inspection becomes a first-class history view shared by shell, TUI, and
  script clients

Tradeoffs:

- observed-value history is intentionally based on explicit value events; it
  does not try to infer arbitrary dataflow from every artifact
- source-function traversal currently keys off structured `file`/`line`/
  `function` metadata, so adapters that omit those fields will still expose an
  empty function inventory

## Follow-up

- keep widening query/resolver traversal only where it maps cleanly onto shared
  APIs, rather than adding shell-only shortcuts
- use these same value/source traversal surfaces when migrating higher-value
  legacy command helpers in Milestone 4
- feed the same mechanisms from real PC/GEOS artifact readers in Milestone 5
