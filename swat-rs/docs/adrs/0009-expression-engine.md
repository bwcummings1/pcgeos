# ADR 0009: Add a Shared Parsed Query Language Before Scripting

## Status

Accepted

## Context

By this point `swat-rs` had several semantic layers:

- value decoding
- schema validation
- trigger predicates
- read-only trace inspection

But those layers were still expressing their logic independently. Without a
shared query language, `swat-control` and `swat-api` risked drifting into
separate semantics, which would make later scripting and UI work much harder.

## Decision

We added `swat-expr` as the first shared expression engine.

Its initial language supports:

- `kind == ModelBoundary`
- `summary contains "text"`
- `artifact.text contains "text"`
- `artifact.json exists $.path`
- `artifact.json $.path == "value"`
- `and` / `or`
- parenthesized grouping

`swat-control` now supports expression-backed triggers and `swat-api` supports
expression-backed event queries through the same evaluator.

## Consequences

Positive:

- trigger semantics and inspection semantics now share one parsed query model
- the workspace has a concrete substrate for later scripting and TUI queries
- expression logic is validated against real and synthetic traces

Negative:

- the language is intentionally small
- schema-aware queries beyond path equality are still handled outside the parser

## Follow-on work

- extend the expression language with schema/value-aware predicates
- add sequence-aware and stateful query forms
- expose expressions directly through future scripting and UI layers
