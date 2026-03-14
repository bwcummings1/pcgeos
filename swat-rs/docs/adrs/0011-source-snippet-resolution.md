# ADR 0011: Add File-Backed Source Snippet Resolution from Structured Runtime Events

## Status

Accepted

## Context

The Python adapter now emits structured artifacts with:

- `file`
- `line`
- `function`

But without a source layer, that metadata still forced callers to reimplement
file lookup and snippet extraction themselves. A Swat-inspired debugger needs a
stable path from runtime event to source context.

## Decision

We added `swat-source`.

Its initial role is:

- extract source locations from decoded event artifacts
- distinguish real file paths from synthetic inline-code markers
- load surrounding source snippets from disk
- integrate that lookup into `swat-api`

## Consequences

Positive:

- runtime events can now resolve directly to source context
- the public API becomes more operator-friendly and more useful for future UIs
- source mapping is now exercised against a real Python script path

Negative:

- this first slice is file-backed and line-oriented
- synthetic inline sources are not yet materialized into virtual source buffers

## Follow-on work

- add virtual-source handling for inline or generated code
- integrate source locations with future resolver/type layers
- add richer source navigation and definition lookup
