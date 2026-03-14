# ADR 0018: Agent Protocol Versioning

- Status: accepted
- Date: 2026-03-14

## Context

`swat-adapter-agent` now has a real typed protocol SDK in
`swat-agent-protocol`, but the original line protocol still had one weak
boundary: records were structurally typed without a declared protocol version.

That made the first AI-native adapter vulnerable to silent drift. A target could
emit a changed shape and the host would either misinterpret it or quietly fall
back to generic stdout handling.

## Decision

The agent line protocol is now explicitly self-versioning.

- `swat-agent-protocol::AgentEventRecord` carries `protocol_version`
- `swat-agent-protocol` exposes `CURRENT_AGENT_PROTOCOL_VERSION`
- `LineEmitter` serializes the version on every emitted record
- `parse_validated_prefixed_line*` validates the decoded version against the
  current host-supported version
- `swat-adapter-agent` uses the validated parser instead of the raw parser

Unsupported versions are surfaced as explicit host-side lifecycle events, with
the raw offending line preserved as a text artifact for inspection.

## Consequences

Benefits:

- the first AI-native adapter now fails visibly on protocol drift
- operator/API layers can inspect the exact raw record that was rejected
- future SDKs in other languages have a concrete compatibility target

Tradeoffs:

- protocol evolution now needs a conscious versioning story
- the adapter must preserve compatibility during transition from older emitters

## Compatibility rule

For the current transition window, missing `protocol_version` fields deserialize
to the current version. This keeps older hand-written emitters working while
making new emitters self-describing.

## Follow-on work

- add explicit compatibility tests for future protocol revisions
- add Python/TypeScript SDKs that emit the same versioned record shape
- decide when omission of `protocol_version` should stop being accepted
