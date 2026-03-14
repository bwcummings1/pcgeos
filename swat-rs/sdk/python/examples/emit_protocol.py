#!/usr/bin/env python3

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from swat_agent_protocol import LineEmitter, model, planner, tool


def main() -> None:
    emit = LineEmitter().emit
    emit(planner("draft-answer", phase="start", summary="planner started"))
    emit(
        model(
            "gpt-4.1-mini",
            phase="request",
            span_id="model-1",
            correlation_id="req-1",
            summary="model requested",
        )
    )
    emit(
        tool(
            "web_search",
            phase="start",
            span_id="tool-1",
            correlation_id="req-1",
            summary="tool started",
        )
    )


if __name__ == "__main__":
    main()
