# TypeScript Agent Protocol SDK

This directory contains the TypeScript SDK for the versioned `swat-rs` agent
event protocol.

Entry points:

- `swat_agent_protocol.ts`
- `examples/emit_protocol.ts`

Provided surface:

- `LineEmitter`
- `planner`, `model`, `tool`, `state`, `policy`, `source`, `schema`
- `makeRecord`
- `encodePrefixedLine`
- `parsePrefixedLine`
- `validateProtocolVersion`

Run the example with:

`bun run sdk/typescript/examples/emit_protocol.ts`
