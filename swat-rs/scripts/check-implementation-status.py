#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path

from status_truth import DEFAULT_ARTIFACT_PATH, DEFAULT_PLAN_PATH, artifact_json, build_artifact, write_if_changed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate the swat-rs execution queue.")
    parser.add_argument("--plan", type=Path, default=DEFAULT_PLAN_PATH)
    parser.add_argument("--artifact", type=Path, default=DEFAULT_ARTIFACT_PATH)
    parser.add_argument(
        "--write-artifact",
        action="store_true",
        help="Write the machine-readable status artifact after validation.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    artifact = build_artifact(args.plan)

    if args.write_artifact:
        write_if_changed(args.artifact, artifact_json(artifact))
        print(f"Wrote status artifact: {args.artifact}")
        return

    print(
        f"Queue {artifact['queue']} is valid: "
        f"{artifact['summary']['done']}/{artifact['summary']['total']} done, "
        f"{artifact['summary']['pending']} pending, "
        f"{artifact['summary']['blocked']} blocked."
    )


if __name__ == "__main__":
    main()
