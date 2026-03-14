# Python Agent Protocol SDK

This directory contains the first non-Rust SDK for the `swat-rs` agent event
protocol.

Entry point:

- `swat_agent_protocol.py`

Provided surface:

- `LineEmitter`
- `planner`, `model`, `tool`, `state`, `policy`, `source`, `schema`
- `make_record`
- `encode_prefixed_line`
- `parse_prefixed_line`
- `validate_protocol_version`

The module emits the same versioned line-oriented protocol consumed by
`swat-adapter-agent`.

Runnable example:

- `python3 sdk/python/examples/emit_protocol.py`
