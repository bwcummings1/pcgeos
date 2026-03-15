#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import shlex
import subprocess
import sys
import textwrap
import time
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path

from status_truth import DEFAULT_PLAN_PATH, build_artifact


SWAT_RS_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PROMPT_PATH = SWAT_RS_ROOT / "PROJECT_CONTINUATION_PROMPT.md"
DEFAULT_LOG_ROOT = SWAT_RS_ROOT / ".runs" / "continuous-agent"
DEFAULT_EXPECTED_BRANCH = "swat-rs-full-completion-plan"
DEFAULT_MAX_STAGNANT_CYCLES = 5
DEFAULT_PAUSE_SECONDS = 2.0


@dataclass(frozen=True)
class RepoSnapshot:
    branch: str
    head: str
    dirty: bool
    status_sha256: str
    status_text: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Supervise repeated swat-rs agent runs until the queue is complete or blocked."
    )
    parser.add_argument("--plan", type=Path, default=DEFAULT_PLAN_PATH)
    parser.add_argument("--prompt", type=Path, default=DEFAULT_PROMPT_PATH)
    parser.add_argument("--expected-branch", default=DEFAULT_EXPECTED_BRANCH)
    parser.add_argument("--log-root", type=Path, default=DEFAULT_LOG_ROOT)
    parser.add_argument("--runner", choices=("codex",), default="codex")
    parser.add_argument(
        "--resume-mode",
        choices=("auto", "always", "never"),
        default="auto",
        help="How to choose between fresh and resumed sessions after the first cycle.",
    )
    parser.add_argument("--model", help="Optional runner model name.")
    parser.add_argument("--profile", help="Optional runner profile name.")
    parser.add_argument(
        "--max-cycles",
        type=int,
        default=0,
        help="Maximum cycles to run before exiting. 0 means unlimited.",
    )
    parser.add_argument(
        "--max-stagnant-cycles",
        type=int,
        default=DEFAULT_MAX_STAGNANT_CYCLES,
        help="Maximum consecutive no-progress cycles before stopping.",
    )
    parser.add_argument(
        "--pause-seconds",
        type=float,
        default=DEFAULT_PAUSE_SECONDS,
        help="Sleep duration between cycles.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Prepare the next cycle prompt and command without invoking the runner.",
    )
    parser.add_argument(
        "--print-prompt",
        action="store_true",
        help="Print the generated cycle prompt to stdout.",
    )
    parser.add_argument(
        "--stream-output",
        dest="stream_output",
        action="store_true",
        help="Stream runner output live to stdout while also saving it to the cycle log.",
    )
    parser.add_argument(
        "--no-stream-output",
        dest="stream_output",
        action="store_false",
        help="Do not mirror runner output to stdout; keep it only in the cycle log.",
    )
    parser.set_defaults(stream_output=True)
    return parser.parse_args()


def run_command(
    cmd: list[str],
    *,
    cwd: Path,
    input_text: str | None = None,
    stdout_path: Path | None = None,
    stream_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    if stdout_path is None:
        return subprocess.run(
            cmd,
            cwd=cwd,
            input=input_text,
            text=True,
            capture_output=True,
            check=False,
        )

    stdout_path.parent.mkdir(parents=True, exist_ok=True)
    with stdout_path.open("w", encoding="utf-8") as handle:
        handle.write(f"$ {shlex.join(cmd)}\n\n")
        if input_text:
            handle.write("## Prompt\n\n")
            handle.write(input_text)
            if not input_text.endswith("\n"):
                handle.write("\n")
            handle.write("\n## Runner Output\n\n")
        handle.flush()

        if stream_output:
            process = subprocess.Popen(
                cmd,
                cwd=cwd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
            )
            if input_text and process.stdin is not None:
                process.stdin.write(input_text)
            if process.stdin is not None:
                process.stdin.close()
            assert process.stdout is not None
            for line in process.stdout:
                handle.write(line)
                handle.flush()
                sys.stdout.write(line)
                sys.stdout.flush()
            result = subprocess.CompletedProcess(
                cmd,
                process.wait(),
                stdout="",
                stderr="",
            )
        else:
            result = subprocess.run(
                cmd,
                cwd=cwd,
                input=input_text,
                text=True,
                stdout=handle,
                stderr=subprocess.STDOUT,
                check=False,
            )

    return subprocess.CompletedProcess(cmd, result.returncode, stdout="", stderr="")


def repo_git_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        cwd=SWAT_RS_ROOT,
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


def repo_snapshot(cwd: Path) -> RepoSnapshot:
    branch = subprocess.run(
        ["git", "branch", "--show-current"],
        cwd=cwd,
        text=True,
        capture_output=True,
        check=True,
    ).stdout.strip()
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=cwd,
        text=True,
        capture_output=True,
        check=True,
    ).stdout.strip()
    status_text = subprocess.run(
        ["git", "status", "--short", "--branch"],
        cwd=cwd,
        text=True,
        capture_output=True,
        check=True,
    ).stdout
    return RepoSnapshot(
        branch=branch,
        head=head,
        dirty=len(status_text.strip().splitlines()) > 1,
        status_sha256=hashlib.sha256(status_text.encode("utf-8")).hexdigest(),
        status_text=status_text,
    )


def sync_status_artifacts(plan_path: Path) -> dict:
    subprocess.run(
        [
            sys.executable,
            "scripts/check-implementation-status.py",
            "--plan",
            str(plan_path),
            "--write-artifact",
        ],
        cwd=SWAT_RS_ROOT,
        check=True,
        text=True,
    )
    subprocess.run(
        [sys.executable, "scripts/render-implementation-status.py", "--plan", str(plan_path)],
        cwd=SWAT_RS_ROOT,
        check=True,
        text=True,
    )
    return build_artifact(plan_path)


def next_pending_task(artifact: dict) -> dict | None:
    for task in artifact["tasks"]:
        if task["status"] == "pending":
            return task
    return None


def blocked_tasks(artifact: dict) -> list[dict]:
    return [task for task in artifact["tasks"] if task["status"] == "blocked"]


def queue_fingerprint(artifact: dict) -> str:
    return hashlib.sha256(
        json.dumps(artifact["tasks"], sort_keys=True).encode("utf-8")
    ).hexdigest()


def progress_happened(
    before_artifact: dict,
    after_artifact: dict,
    before_repo: RepoSnapshot,
    after_repo: RepoSnapshot,
) -> bool:
    if queue_fingerprint(before_artifact) != queue_fingerprint(after_artifact):
        return True
    if before_repo.head != after_repo.head:
        return True
    if before_repo.status_sha256 != after_repo.status_sha256:
        return True
    return False


def build_supervisor_prompt(
    *,
    base_prompt: str,
    artifact: dict,
    repo: RepoSnapshot,
    cycle_number: int,
    stagnant_cycles: int,
    using_resume: bool,
) -> str:
    next_task = next_pending_task(artifact)
    lines = [
        f"This is unattended supervised run cycle {cycle_number}.",
        f"Runner mode for this cycle: `{'resume-last' if using_resume else 'fresh'}`.",
        "",
        "Supervisor state:",
        f"- current branch: `{repo.branch}`",
        f"- current head: `{repo.head}`",
        f"- worktree dirty: `{'yes' if repo.dirty else 'no'}`",
        f"- queue progress: `{artifact['summary']['done']}/{artifact['summary']['total']} done`",
        f"- queue blocked count: `{artifact['summary']['blocked']}`",
    ]

    if next_task is not None:
        lines.extend(
            [
                f"- next pending task: `{next_task['task']}`",
                f"- next task milestone: `{next_task['milestone']}`",
                f"- next task description: {next_task['description']}",
                f"- next task notes: {next_task['notes']}",
            ]
        )

    if stagnant_cycles > 0:
        lines.extend(
            [
                "",
                f"Important: the previous {stagnant_cycles} supervised cycle(s) ended without observable queue or repo progress.",
                "Do not stop at a progress summary. Make actual progress, update the queue, commit, push, and continue.",
            ]
        )

    if repo.dirty:
        lines.extend(
            [
                "",
                "The worktree is dirty. Preserve the changes, validate them, and carry them through to a clean committed checkpoint.",
            ]
        )

    lines.extend(
        [
            "",
            "Supervisor requirements for this cycle:",
            "1. Continue from the authoritative next pending queue task.",
            "2. Do not stop after a slice summary or milestone summary.",
            "3. After each completed queue task: update `PROJECT_COMPLETION_PLAN.md`, refresh generated status artifacts, run targeted tests, run full `cargo test`, commit, push, and continue.",
            "4. If you are truly blocked, mark the task `blocked` with the exact blocker and next action before pausing.",
            "5. Only stop when the full project Definition of Done is satisfied or a real blocker is recorded in the queue.",
            "",
            "Canonical base prompt follows.",
            "",
            base_prompt.rstrip(),
            "",
        ]
    )
    return "\n".join(lines)


def build_runner_command(
    *,
    args: argparse.Namespace,
    use_resume: bool,
    last_message_path: Path,
) -> list[str]:
    cmd = ["codex", "exec"]
    if use_resume:
        cmd.extend(["resume", "--last"])
    cmd.append("--dangerously-bypass-approvals-and-sandbox")
    if args.model:
        cmd.extend(["--model", args.model])
    if args.profile:
        cmd.extend(["--profile", args.profile])
    cmd.extend(["--output-last-message", str(last_message_path), "-"])
    return cmd


def should_use_resume(
    *,
    cycle_number: int,
    resume_mode: str,
    previous_progress: bool,
) -> bool:
    if cycle_number == 1:
        return False
    if resume_mode == "always":
        return True
    if resume_mode == "never":
        return False
    return previous_progress


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def timestamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def main() -> None:
    args = parse_args()
    git_root = repo_git_root()
    current_branch = repo_snapshot(git_root).branch
    if args.expected_branch and current_branch != args.expected_branch:
        raise SystemExit(
            f"Expected branch {args.expected_branch}, found {current_branch}. Switch branches before running the supervisor."
        )

    base_prompt = args.prompt.read_text(encoding="utf-8")
    run_root = args.log_root / timestamp()
    run_root.mkdir(parents=True, exist_ok=True)

    stagnant_cycles = 0
    previous_progress = True
    cycle_number = 0

    while True:
        before_artifact = sync_status_artifacts(args.plan)
        before_repo = repo_snapshot(git_root)
        pending = next_pending_task(before_artifact)
        blocked = blocked_tasks(before_artifact)

        if before_artifact["summary"]["done"] == before_artifact["summary"]["total"]:
            print("Queue is complete. Nothing left to supervise.")
            return
        if blocked:
            print("Queue is blocked. Resolve the recorded blocker before supervising further.")
            for task in blocked:
                print(f"- {task['task']}: {task['notes']}")
            raise SystemExit(2)

        cycle_number += 1
        if args.max_cycles and cycle_number > args.max_cycles:
            print(
                f"Reached max cycles ({args.max_cycles}) before queue completion. Last pending task: {pending['task'] if pending else 'none'}."
            )
            raise SystemExit(3)

        use_resume = should_use_resume(
            cycle_number=cycle_number,
            resume_mode=args.resume_mode,
            previous_progress=previous_progress,
        )
        prompt_text = build_supervisor_prompt(
            base_prompt=base_prompt,
            artifact=before_artifact,
            repo=before_repo,
            cycle_number=cycle_number,
            stagnant_cycles=stagnant_cycles,
            using_resume=use_resume,
        )

        cycle_dir = run_root / f"cycle-{cycle_number:03d}"
        cycle_dir.mkdir(parents=True, exist_ok=True)
        prompt_path = cycle_dir / "prompt.md"
        stdout_path = cycle_dir / "runner.log"
        last_message_path = cycle_dir / "last-message.md"
        state_path = cycle_dir / "state.json"
        prompt_path.write_text(prompt_text, encoding="utf-8")

        cmd = build_runner_command(
            args=args,
            use_resume=use_resume,
            last_message_path=last_message_path,
        )
        state = {
            "cycle": cycle_number,
            "timestamp": timestamp(),
            "runnerCommand": cmd,
            "runnerMode": "resume-last" if use_resume else "fresh",
            "repoBefore": asdict(before_repo),
            "artifactBefore": before_artifact,
            "pendingTaskBefore": pending,
            "promptPath": str(prompt_path.relative_to(SWAT_RS_ROOT)),
        }

        if args.print_prompt:
            print(prompt_text)

        if args.dry_run:
            state["dryRun"] = True
            write_json(state_path, state)
            print(f"Dry run cycle prepared at {cycle_dir}")
            print(f"Planned runner command: {shlex.join(cmd)}")
            return

        print(
            f"[cycle {cycle_number}] next task {pending['task']} | mode={'resume' if use_resume else 'fresh'} | log={stdout_path}"
        )
        if args.stream_output:
            print(f"[cycle {cycle_number}] streaming live runner output below")
        result = run_command(
            cmd,
            cwd=git_root,
            input_text=prompt_text,
            stdout_path=stdout_path,
            stream_output=args.stream_output,
        )
        state["runnerExitCode"] = result.returncode

        after_artifact = sync_status_artifacts(args.plan)
        after_repo = repo_snapshot(git_root)
        state["artifactAfter"] = after_artifact
        state["repoAfter"] = asdict(after_repo)
        state["runnerLogPath"] = str(stdout_path.relative_to(SWAT_RS_ROOT))
        state["lastMessagePath"] = (
            str(last_message_path.relative_to(SWAT_RS_ROOT))
            if last_message_path.exists()
            else None
        )
        write_json(state_path, state)

        if after_artifact["summary"]["done"] == after_artifact["summary"]["total"]:
            print(f"[cycle {cycle_number}] queue complete.")
            return

        if blocked_tasks(after_artifact):
            print(f"[cycle {cycle_number}] queue entered blocked state; stopping.")
            raise SystemExit(2)

        previous_progress = progress_happened(
            before_artifact,
            after_artifact,
            before_repo,
            after_repo,
        )
        if previous_progress:
            stagnant_cycles = 0
            next_task = next_pending_task(after_artifact)
            print(
                f"[cycle {cycle_number}] progress recorded. Next pending task: {next_task['task'] if next_task else 'none'}."
            )
        else:
            stagnant_cycles += 1
            print(
                f"[cycle {cycle_number}] no queue or repo progress detected ({stagnant_cycles}/{args.max_stagnant_cycles})."
            )
            if stagnant_cycles >= args.max_stagnant_cycles:
                print(
                    textwrap.dedent(
                        f"""\
                        Stopping after {stagnant_cycles} consecutive stagnant cycles.
                        Inspect the latest supervisor run under:
                          {run_root}
                        """
                    ).rstrip()
                )
                raise SystemExit(4)

        if args.pause_seconds > 0:
            time.sleep(args.pause_seconds)


if __name__ == "__main__":
    main()
