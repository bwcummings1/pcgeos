# ADR 0021: TypeScript Agent Protocol SDK

- Status: accepted
- Date: 2026-03-14

## Context

`swat-rs` already had versioned Rust and Python protocol emitters for the
AI-native agent adapter. That still left a major gap for modern agent runtimes,
because TypeScript is a common orchestration layer for tool-using systems.

Without a TypeScript SDK, Bun/Node runtimes still had to hand-roll prefixed JSON
and manually track `protocol_version`.

## Decision

Add a TypeScript SDK under `sdk/typescript/`.

The SDK provides:

- typed `AgentEventRecord` and `AgentEventKind`
- record builders for planner/model/tool/state/policy/source/schema/lifecycle/log
- prefixed-line encode/parse helpers
- protocol-version validation
- `LineEmitter`

The Bun example and Rust integration test now validate that `swat-adapter-agent`
accepts events emitted through this SDK.

## Consequences

Benefits:

- `swat-rs` now has agent-protocol parity across Rust, Python, and TypeScript
- Bun/TypeScript runtimes can emit the same versioned protocol without manual
  string assembly
- cross-language drift is reduced because the SDKs share the same invariants

Tradeoffs:

- the SDK is repo-local rather than published as an npm package
- validation depends on Bun being present when the Bun-backed integration test runs

## Follow-on work

- publish the SDK if `swat-rs` is split into its own repository
- add golden cross-language parity fixtures for Rust/Python/TypeScript outputs
- add Node-target packaging if Bun-specific execution becomes too narrow
