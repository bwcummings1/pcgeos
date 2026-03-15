# ADR 0027: Supervised Continuation Runner

## Status

Accepted

## Context

`swat-rs` already had a queue-and-status harness for the remaining post-`v1`
work, but unattended agent runs were still stopping after one or two queue
tasks. The queue preserved state and made the next task unambiguous, but it did
not relaunch the agent after the agent exited.

That left the project dependent on a human operator to notice the stop, copy
the continuation prompt, and start another run. For the full-project
completion phase, that is not sufficient.

## Decision

Add a supervisor script at:

- `/home/ubuntu/pcgeos/swat-rs/scripts/run-continuous-agent.py`

The supervisor:

- treats `PROJECT_COMPLETION_PLAN.md` as the authoritative queue
- refreshes the generated queue artifacts before and after each cycle
- derives the next pending task from the queue instead of hardcoding stale task
  numbers into prompts
- launches `codex exec` for the first cycle and can resume the most recent
  session on later cycles
- records per-cycle prompts, runner logs, last messages, and state snapshots
  under `swat-rs/.runs/continuous-agent/`
- continues relaunching until:
  - the queue is complete
  - the queue is explicitly blocked
  - or repeated stagnant cycles indicate the runner is no longer making
    observable progress

## Consequences

Positive:

- unattended runs now have a real control loop instead of a prompt-only
  convention
- the authoritative next task is always read from the queue
- each cycle is auditable from the saved prompts and logs
- prompt drift is reduced because the supervisor prepends current queue and repo
  state to the canonical continuation prompt

Trade-offs:

- `swat-rs` now depends on a local `codex` CLI for the unattended supervisor
  path
- repeated stagnant cycles still require human intervention, but they now fail
  with an auditable log directory instead of silently stalling

## Notes

This does not replace the queue. The queue remains the source of truth. The
supervisor exists to keep launching fresh or resumed agent cycles until the
queue is materially advanced or explicitly blocked.
