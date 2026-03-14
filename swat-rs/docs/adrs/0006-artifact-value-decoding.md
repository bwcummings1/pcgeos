# ADR 0006: Decode Artifact-Backed Values Before Full Schema/Resolver Work

## Status

Accepted

## Context

`swat-rs` could already capture artifacts, persist them, and trigger on their
content, but there was still no reusable value layer for decoding and inspecting
those artifacts as typed data.

That left a gap between transport/storage and higher-level debugger behavior:
captured JSON looked like bytes, and even plain-text payloads had no stable
inspection API.

## Decision

We added `swat-value` as the first typed inspection crate.

The initial scope includes:

- decoding UTF-8 text artifacts
- decoding JSON artifacts
- preserving binary artifacts as raw bytes
- preview rendering
- simple JSON-path lookup for decoded JSON values

## Consequences

Positive:

- stored artifacts are now inspectable through a stable API
- later schema, resolver, and expression work can build on decoded values
- tests now validate the mock boundary artifact and live process output through
  the value layer instead of reading raw bytes directly

Negative:

- the path language is intentionally minimal
- schema awareness and source/value correlation remain future work

## Follow-on work

- add richer typed formatting and pretty printers
- integrate decoded values with trigger predicates
- add schema-informed accessors and expression language support
