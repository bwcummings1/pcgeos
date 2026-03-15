# 0036. PC/GEOS Format Support Crate

Date: 2026-03-15

## Status

Accepted

## Context

Milestone 5 needs `swat-rs` to reason over real PC/GEOS repository artifacts
without pushing VM, geode, symbol, or object-format details down into
`swat-core`.

The legacy debugger spreads this knowledge across VM helpers, string-table
helpers, object-format readers, and source-map logic in the historical tools
tree. The Rust workspace needs the same substrate for later PC/GEOS adapter and
operator work, but it should stay reusable and isolated from the shared
target-neutral layers.

## Decision

Introduce a dedicated support crate, `swat-format-pcgeos`, for repository-side
PC/GEOS format handling.

The crate owns:

- PC/GEOS file-header parsing for release 1.x and 2.x headers
- VM file-header, header-block, and block-table parsing
- string-table decoding for VM-backed object/symbol metadata
- object/symbol map-block parsing, including segment/group descriptors
- source-map and line-map readers for symbol-style VM payloads

The crate depends only on shared `swat-core` error/result types. Future
PC/GEOS-specific adapters, shells, scripts, and TUI surfaces consume it above
the target-neutral core instead of reimplementing file-format details locally.

## Consequences

Positive:

- `swat-core` stays free of PC/GEOS file-format assumptions.
- the future `swat-adapter-pcgeos` gets a reusable format substrate
- repository-backed tests can validate real VM and GEOS artifacts directly from
  this tree

Tradeoffs:

- another workspace crate must be maintained
- some symbol/object readers are introduced before the live adapter that will
  eventually consume them
