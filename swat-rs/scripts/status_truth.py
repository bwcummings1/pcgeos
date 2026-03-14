#!/usr/bin/env python3

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PLAN_PATH = REPO_ROOT / "PROJECT_COMPLETION_PLAN.md"
DEFAULT_ARTIFACT_PATH = REPO_ROOT / "docs" / "generated" / "implementation-status.json"
DEFAULT_SUMMARY_PATH = REPO_ROOT / "docs" / "generated" / "current-queue-summary.md"

QUEUE_HEADING_PATTERN = re.compile(
    r"^##\s+Current Cycle Queue\s+\(T-(\d{3})\.\.T-(\d{3})\)\s*$"
)
TASK_PATTERN = re.compile(r"^T-(\d{3})$")
ALLOWED_STATUSES = {"pending", "done", "blocked"}
EXPECTED_HEADERS = [
    "Task",
    "Status",
    "Milestone",
    "Description",
    "Branch",
    "Commit",
    "Notes",
]


@dataclass(frozen=True)
class QueueSection:
    heading: str
    start: int
    end: int
    rows: list[dict[str, str]]


def split_markdown_row(line: str) -> list[str]:
    trimmed = line.strip()
    if not trimmed.startswith("|") or not trimmed.endswith("|"):
        return []
    return [cell.strip() for cell in trimmed[1:-1].split("|")]


def find_current_cycle_queue(markdown: str) -> QueueSection:
    lines = markdown.splitlines()
    heading_index = None
    heading = ""
    start = 0
    end = 0

    for index, line in enumerate(lines):
        match = QUEUE_HEADING_PATTERN.match(line.strip())
        if match:
            heading_index = index
            heading = line.strip()
            start = int(match.group(1))
            end = int(match.group(2))

    if heading_index is None:
        raise ValueError(
            'Missing active queue section header. Expected "## Current Cycle Queue (T-XXX..T-YYY)".'
        )

    header_row = None
    divider_row = None
    rows: list[dict[str, str]] = []

    for index in range(heading_index + 1, len(lines)):
        line = lines[index]
        if not line.strip():
            continue
        cells = split_markdown_row(line)
        if not cells:
            if rows:
                break
            continue
        if header_row is None:
            header_row = cells
            continue
        if divider_row is None:
            divider_row = cells
            continue
        if len(cells) != len(header_row):
            raise ValueError(
                f"Malformed queue row under {heading}: expected {len(header_row)} cells, got {len(cells)}."
            )
        rows.append(dict(zip(header_row, cells)))

    if header_row != EXPECTED_HEADERS:
        raise ValueError(
            f"Unexpected queue table headers: expected {EXPECTED_HEADERS}, got {header_row}."
        )
    if divider_row is None or len(divider_row) != len(EXPECTED_HEADERS):
        raise ValueError(f"Missing divider row under {heading}.")
    if not rows:
        raise ValueError(f"Queue table under {heading} is empty.")

    return QueueSection(heading=heading, start=start, end=end, rows=rows)


def validate_queue(queue: QueueSection) -> None:
    expected_ids = list(range(queue.start, queue.end + 1))
    actual_ids: list[int] = []

    for row in queue.rows:
        task = row["Task"]
        match = TASK_PATTERN.match(task)
        if not match:
            raise ValueError(f"Invalid task id {task}. Expected format T-XXX.")
        task_id = int(match.group(1))
        actual_ids.append(task_id)

        status = row["Status"]
        if status not in ALLOWED_STATUSES:
            raise ValueError(
                f"Invalid status {status} for {task}. Expected one of {sorted(ALLOWED_STATUSES)}."
            )

        for key in EXPECTED_HEADERS[2:]:
            if row[key] == "":
                raise ValueError(f"Queue row {task} has an empty {key} cell.")

    if actual_ids != expected_ids:
        expected = [f"T-{task_id:03d}" for task_id in expected_ids]
        actual = [f"T-{task_id:03d}" for task_id in actual_ids]
        raise ValueError(
            f"Queue tasks do not match the heading range {queue.heading}. Expected {expected}, got {actual}."
        )


def build_artifact(plan_path: Path = DEFAULT_PLAN_PATH) -> dict:
    markdown = plan_path.read_text(encoding="utf-8")
    queue = find_current_cycle_queue(markdown)
    validate_queue(queue)

    summary = {
        "total": len(queue.rows),
        "done": sum(1 for row in queue.rows if row["Status"] == "done"),
        "pending": sum(1 for row in queue.rows if row["Status"] == "pending"),
        "blocked": sum(1 for row in queue.rows if row["Status"] == "blocked"),
    }

    return {
        "plan": str(plan_path.relative_to(REPO_ROOT)),
        "queue": f"T-{queue.start:03d}..T-{queue.end:03d}",
        "queueHeading": queue.heading,
        "summary": summary,
        "tasks": [
            {
                "task": row["Task"],
                "status": row["Status"],
                "milestone": row["Milestone"],
                "description": row["Description"],
                "branch": row["Branch"],
                "commit": row["Commit"],
                "notes": row["Notes"],
            }
            for row in queue.rows
        ],
    }


def render_summary_markdown(artifact: dict) -> str:
    lines = [
        "# Current Queue Summary",
        "",
        "Generated from `PROJECT_COMPLETION_PLAN.md`. That plan remains canonical.",
        "",
        f"- Queue: `{artifact['queue']}`",
        f"- Progress: `{artifact['summary']['done']}/{artifact['summary']['total']} done`",
        f"- Pending: `{artifact['summary']['pending']}`",
        f"- Blocked: `{artifact['summary']['blocked']}`",
        "",
        "## Task Snapshot",
        "",
        "| Task | Status | Milestone | Branch | Commit |",
        "|------|--------|-----------|--------|--------|",
    ]

    for task in artifact["tasks"]:
        lines.append(
            f"| {task['task']} | {task['status']} | {task['milestone']} | {task['branch']} | {task['commit']} |"
        )

    lines.extend(
        [
            "",
            "## Task Notes",
            "",
            "| Task | Description | Notes |",
            "|------|-------------|-------|",
        ]
    )

    for task in artifact["tasks"]:
        lines.append(
            f"| {task['task']} | {task['description']} | {task['notes']} |"
        )

    lines.append("")
    return "\n".join(lines)


def write_if_changed(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    current = path.read_text(encoding="utf-8") if path.exists() else None
    if current != content:
        path.write_text(content, encoding="utf-8")


def artifact_json(artifact: dict) -> str:
    return json.dumps(artifact, indent=2) + "\n"
