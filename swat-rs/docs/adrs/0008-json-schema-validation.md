# ADR 0008: Add JSON-Oriented Schema Inference and Validation Before a Full Type System

## Status

Accepted

## Context

`swat-rs` can now decode artifact-backed values and inspect traces through a
public API, but AI/tool debugging still needs structural validation:

- verify that tool outputs match expected shape
- reason about missing or mistyped fields
- stop automatically when a payload violates an expected contract

Waiting for a full type/resolver/expression stack before adding any schema logic
would delay one of the most practical debugging use cases.

## Decision

We added `swat-schema` as a JSON-first schema layer with:

- schema inference from decoded JSON values
- path lookup within inferred schemas
- structural validation with mismatch paths
- integration into `swat-control` through schema-failure predicates

## Consequences

Positive:

- schema mismatch is now a first-class semantic stop condition
- traces can be validated structurally instead of only searched textually
- later richer type-system work has a concrete substrate to build on

Negative:

- the schema model is intentionally JSON-oriented in this slice
- it does not yet cover source-language types, symbol data, or non-JSON object models

## Follow-on work

- extend schema coverage beyond JSON
- integrate schema and value layers with the future expression engine
- add richer mismatch reporting and schema-driven formatting
