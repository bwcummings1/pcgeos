from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from typing import Any, Mapping, MutableMapping, TextIO

DEFAULT_TRACE_PREFIX = "__SWATAGENT__"
CURRENT_AGENT_PROTOCOL_VERSION = "0.1.0-alpha"


class AgentProtocolError(ValueError):
    pass


def validate_protocol_version(version: str) -> None:
    if version != CURRENT_AGENT_PROTOCOL_VERSION:
        raise AgentProtocolError(
            f"unsupported agent protocol version {version}; "
            f"expected {CURRENT_AGENT_PROTOCOL_VERSION}"
        )


def _compact_mapping(record: Mapping[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in record.items() if value is not None}


def make_record(kind: str, **fields: Any) -> dict[str, Any]:
    record = {
        "protocol_version": fields.pop(
            "protocol_version", CURRENT_AGENT_PROTOCOL_VERSION
        ),
        "kind": kind,
    }
    record.update(fields)
    return _compact_mapping(record)


def planner(name: str, **fields: Any) -> dict[str, Any]:
    return make_record("planner", name=name, **fields)


def model(name: str, **fields: Any) -> dict[str, Any]:
    return make_record("model", name=name, **fields)


def tool(name: str, **fields: Any) -> dict[str, Any]:
    return make_record("tool", name=name, **fields)


def state(name: str, **fields: Any) -> dict[str, Any]:
    return make_record("state", name=name, **fields)


def policy(name: str, **fields: Any) -> dict[str, Any]:
    return make_record("policy", name=name, **fields)


def source(name: str, **fields: Any) -> dict[str, Any]:
    return make_record("source", name=name, **fields)


def schema(name: str, **fields: Any) -> dict[str, Any]:
    return make_record("schema", name=name, **fields)


def lifecycle(name: str | None = None, **fields: Any) -> dict[str, Any]:
    if name is not None:
        fields.setdefault("name", name)
    return make_record("lifecycle", **fields)


def log(**fields: Any) -> dict[str, Any]:
    return make_record("log", **fields)


def normalize_record(record: Mapping[str, Any]) -> dict[str, Any]:
    normalized = dict(record)
    normalized.setdefault("protocol_version", CURRENT_AGENT_PROTOCOL_VERSION)
    if "kind" not in normalized:
        raise AgentProtocolError("agent protocol record requires 'kind'")
    validate_protocol_version(str(normalized["protocol_version"]))
    return _compact_mapping(normalized)


def encode_prefixed_line(
    record: Mapping[str, Any], prefix: str = DEFAULT_TRACE_PREFIX
) -> str:
    normalized = normalize_record(record)
    return prefix + json.dumps(normalized, separators=(",", ":"))


def parse_prefixed_line(
    line: str, prefix: str = DEFAULT_TRACE_PREFIX
) -> dict[str, Any] | None:
    if not line.startswith(prefix):
        return None
    payload = json.loads(line[len(prefix) :])
    if not isinstance(payload, MutableMapping):
        raise AgentProtocolError("agent protocol payload must be a JSON object")
    return normalize_record(payload)


@dataclass
class LineEmitter:
    writer: TextIO = sys.stdout
    prefix: str = DEFAULT_TRACE_PREFIX

    def emit(self, record: Mapping[str, Any]) -> None:
        self.writer.write(encode_prefixed_line(record, self.prefix))
        self.writer.write("\n")
        self.writer.flush()


__all__ = [
    "AgentProtocolError",
    "CURRENT_AGENT_PROTOCOL_VERSION",
    "DEFAULT_TRACE_PREFIX",
    "LineEmitter",
    "encode_prefixed_line",
    "lifecycle",
    "log",
    "make_record",
    "model",
    "parse_prefixed_line",
    "planner",
    "policy",
    "schema",
    "source",
    "state",
    "tool",
    "validate_protocol_version",
]
