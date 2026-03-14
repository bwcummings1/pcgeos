# ADR 0020: Python Agent Protocol SDK

- Status: accepted
- Date: 2026-03-14

## Context

`swat-agent-protocol` established a typed Rust SDK for the versioned agent line
protocol, but the first real AI-native adapter is aimed directly at runtimes
like Python where agent orchestration code commonly lives.

Without a matching Python helper, those runtimes still had to hand-roll prefixed
JSON strings and manually remember the protocol version field.

## Decision

Add a small Python SDK under `sdk/python/swat_agent_protocol.py`.

The SDK provides:

- `LineEmitter`
- record builders for planner/model/tool/state/policy/source/schema/log events
- prefixed-line encode/parse helpers
- protocol-version validation

Existing Rust integration tests and demos now use this Python SDK path instead
of constructing agent records manually.

## Consequences

Benefits:

- the first AI-native adapter now has parity between its Rust and Python emitters
- protocol-version handling is centralized for Python runtimes
- the repo now has a concrete reference for future TypeScript or remote SDKs

Tradeoffs:

- the Python SDK is not packaged independently yet
- compatibility is maintained by repo co-location rather than release tooling

## Follow-on work

- package the Python SDK for external reuse if `swat-rs` splits into its own repo
- add a TypeScript SDK with the same versioned record shape
- decide whether to add property-based or cross-language golden tests
