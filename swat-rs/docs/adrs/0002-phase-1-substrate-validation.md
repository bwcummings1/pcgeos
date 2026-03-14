# ADR 0002: Validate the Substrate with Protocol Frames, a Mock Adapter, and Replay Tests

## Status

Accepted

## Context

`swat-rs` finished `Phase 0` with a semantic model and crate map, but the
initial implementation could still fail in several predictable ways:

- protocol types could exist without a real frame format
- session/state management could drift away from the documented event model
- replay could remain theoretical until exercised against a live adapter
- large artifacts could accidentally be treated as inline event data

The original Swat architecture under `Tools/swat` centered execution on the
dispatch loop and RPC substrate before any operator surface. `swat-rs` needs a
similarly verified substrate before expanding into live adapters, scripting, or
UI.

## Decision

For `Phase 1`, we will require the following concrete validation artifacts:

1. a runnable mock-session demo
2. explicit failure-mode tests for session/store/protocol invariants
3. a wire-frame codec in `swat-protocol`
4. replay exercised against `swat-adapter-mock`

## Consequences

Positive:

- the protocol crate is no longer just a type catalog
- replay behavior is proven before real adapters are introduced
- failure handling for unknown sessions and bad artifact references is tested
- the implementation stays aligned with the execution strategy

Negative:

- `Phase 1` gains slightly more code before the first live adapter exists
- protocol encoding decisions may still evolve when the first networked
  transport arrives

## Follow-on work

- add a concrete transport trait and at least one real transport binding
- add a durable event/artifact store implementation
- add the first live target adapter for a modern local runtime
