# ADR 0032: Typed Target Entity Inspection

## Context

Milestone 3 had already restored frame-oriented stack inspection, source
navigation, and richer breakpoint/watchpoint workflows, but the entity model
was still trace-shaped:

- `swat-resolver` indexed correlations, spans, value keys, and modern runtime
  names such as tools or planners
- `swat-api` could search those relations, but not project debugger-native
  patients, handles, resources, or objects
- shell, TUI, and script surfaces therefore had no shared way to inspect the
  target identities that legacy Swat treated as first-class

The legacy references make that gap important. `patient.c` and `handle.c` are
not raw-kernel escape hatches; they are the abstraction layer that lets the
debugger talk about named targets, movable blocks, resources, and object-owned
state without leaking machine details everywhere else.

## Decision

Add typed patient/handle/resource/object extraction in the shared value and
inspection layers, then expose that model uniformly through resolver, API,
shell, TUI, and script surfaces.

Concretely:

- `swat-value` now extracts typed target-entity records from structured JSON
  artifacts using target-neutral fields such as `patient`, `handle`,
  `resource`, `object`, and their plural forms
- `swat-resolver` now indexes those entities and follows their relation-bearing
  fields so entity traversals can move between patients, handles, resources,
  and objects instead of only raw event kinds
- `swat-api` now exposes typed summaries/details plus event lookups for:
  - `patients` / `patient_detail`
  - `handles` / `handle_detail`
  - `resources` / `resource_detail`
  - `objects` / `object_detail`
- `swat-command`, `swat-ui-tui`, and `swat-script` now use that shared API for
  debugger-style `patient`, `handle`, `resource`, and `object` inspection

This keeps `swat-core` target-neutral. The core still only carries events,
artifacts, and capabilities; entity interpretation happens above it from
structured adapter payloads.

## Consequences

Positive:

- debugger-native identity inspection now works for modern structured runtimes
  without inventing a PC/GEOS-only core model
- relation traversals are more useful because handle/resource/object records
  contribute their linked patient and ownership identities
- shell, TUI, and script clients stay on one public inspection surface instead
  of growing incompatible local helpers

Tradeoffs:

- entity richness still depends on adapters emitting structured fields; simpler
  adapters may legitimately expose empty patient/handle/object inventories
- raw extraction may observe duplicate nested records inside one artifact, so
  higher layers remain responsible for session-level deduplication and
  presentation shaping

## Follow-up

- deepen resolver/expression/source traversals around these typed entities in
  the remaining Milestone 3 work
- feed the same inspection model from real PC/GEOS repository artifacts in
  Milestone 5 instead of only synthetic modern-target fixtures
- layer higher-value legacy patient/object helper workflows on top of this
  shared inspection surface rather than bypassing it
