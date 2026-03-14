#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path

from status_truth import (
    DEFAULT_ARTIFACT_PATH,
    DEFAULT_PLAN_PATH,
    DEFAULT_SUMMARY_PATH,
    artifact_json,
    build_artifact,
    render_summary_markdown,
    write_if_changed,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Render or verify the swat-rs queue status artifacts."
    )
    parser.add_argument("--plan", type=Path, default=DEFAULT_PLAN_PATH)
    parser.add_argument("--artifact", type=Path, default=DEFAULT_ARTIFACT_PATH)
    parser.add_argument("--summary", type=Path, default=DEFAULT_SUMMARY_PATH)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail if the generated status artifacts drift from the canonical plan.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    artifact = build_artifact(args.plan)
    expected_json = artifact_json(artifact)
    expected_summary = render_summary_markdown(artifact)

    if args.check:
        artifact_text = args.artifact.read_text(encoding="utf-8")
        summary_text = args.summary.read_text(encoding="utf-8")
        if artifact_text != expected_json:
            raise SystemExit(f"Status artifact is stale: {args.artifact}")
        if summary_text != expected_summary:
            raise SystemExit(f"Queue summary is stale: {args.summary}")
        print("Implementation status artifacts are up to date.")
        return

    write_if_changed(args.artifact, expected_json)
    write_if_changed(args.summary, expected_summary)
    print(f"Wrote status artifact: {args.artifact}")
    print(f"Wrote queue summary: {args.summary}")


if __name__ == "__main__":
    main()
