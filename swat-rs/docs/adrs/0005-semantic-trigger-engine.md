# ADR 0005: Add a Host-Side Semantic Trigger Engine Before Rich Value/Schema Work

## Status

Accepted

## Context

After the substrate, local-process adapter, and durable store were in place,
`swat-rs` could observe and persist real execution but still lacked one of the
main ideas that differentiates Swat-inspired debugging from ordinary logging:
programmable stop behavior driven by meaning rather than raw transport events.

The first implementation slice needed to prove that:

- predicates can inspect event payloads and persisted artifacts
- trigger hits become part of the trace
- trigger actions can feed back into the same session/control pipeline

## Decision

We added `swat-control` as the first Phase 3 slice.

The initial engine supports:

- `EventKindIs`
- `SummaryContains`
- `ArtifactUtf8Contains`
- `All` / `Any` predicate composition
- `PauseTarget` actions
- host-generated `TriggerHit` events recorded through the session/store path

## Consequences

Positive:

- the architecture now supports semantic pause behavior over real targets
- trigger decisions are part of the durable trace instead of side-channel state
- the control layer builds on top of the substrate rather than bypassing it

Negative:

- predicate coverage is still narrow
- there is not yet a user-facing DSL or query language for trigger authoring

## Follow-on work

- add stateful and sequence-aware predicates
- integrate triggers with source/schema/value resolution
- add user-facing APIs and scripting bindings for trigger definition
