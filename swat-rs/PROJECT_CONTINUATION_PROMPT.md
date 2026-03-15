You are continuing implementation of `swat-rs` inside
`/home/ubuntu/pcgeos/swat-rs`.

`swat-rs v1` is already complete. Your job now is to finish the whole project,
meaning close the remaining gap to legacy PC/GEOS Swat as a full debugger
system while preserving the modern Rust architecture and the already-green
`v1` surface.

Your job is not to redesign the project from scratch. Your job is to preserve
contextual coherence with legacy PC/GEOS Swat, keep the finished `v1` intact,
and continue implementation until the `Definition of Done` in
`/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md` is satisfied.

Read these first, in order:

1. `/home/ubuntu/pcgeos/swat-rs/AGENTS.md`
2. `/home/ubuntu/pcgeos/swat-rs/README.md`
3. `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md`
4. `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md`
5. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/phase-0-spec.md`
6. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/reference-map.md`
7. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/legacy-subsystem-inventory.md`
8. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/execution-strategy.md`
9. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/crate-map.md`
10. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/agent-event-protocol.md`
11. all ADRs in `/home/ubuntu/pcgeos/swat-rs/docs/adrs/` in sorted order

Then run the harness commands:

- `python3 scripts/check-implementation-status.py`
- `python3 scripts/render-implementation-status.py --check`

If you are running unattended, prefer the supervisor instead of a one-shot
prompt:

- `python3 scripts/run-continuous-agent.py`

That supervisor runs in the foreground by default and mirrors each cycle's
runner output to the terminal while preserving logs.

Then inspect the main implementation seams:

- `/home/ubuntu/pcgeos/swat-rs/swat-core/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-session/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-store/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-replay/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-control/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-command/src/main.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-script/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-resolver/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-source/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-expr/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-value/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-ui-tui/`

Then inspect the legacy reference material relevant to the current milestone.
Use:

- `/home/ubuntu/pcgeos/swat-rs/docs/architecture/reference-map.md`
- `/home/ubuntu/pcgeos/swat-rs/docs/architecture/legacy-subsystem-inventory.md`
- the explicit legacy references listed in
  `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md`

Hard constraints:

1. Do not implement inside `Tools/swat`.
2. Do not regress existing `v1` behavior while chasing parity.
3. Do not leak PC/GEOS-specific assumptions into `swat-core`.
4. Do not inline heavyweight payloads that should be artifacts.
5. Do not move UI logic into lower layers.
6. Do not bypass `swat-api` from shell, script, or TUI when shared API use is
   possible.
7. Do not claim the project is complete until every item in the `Definition of
   Done` section of `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md`
   is satisfied.
8. Do not stop to ask whether to move to the next milestone unless you are
   truly blocked by conflicting requirements or unavailable external resources.
9. Keep docs and ADRs current as implementation changes.
10. Keep the workspace green. Run narrow tests while iterating and full
    `cargo test` before closing a milestone.
11. Keep the tree clean at milestone boundaries.
12. After each completed task in the active queue:
    - update its row in `PROJECT_COMPLETION_PLAN.md`
    - refresh `docs/generated/implementation-status.json`
    - refresh `docs/generated/current-queue-summary.md`
    - commit the task
    - push the branch
    - continue immediately to the next task
13. If blocked, mark the task `blocked` in the active queue and record the
    exact blocker plus next action before pausing.

Execution order:

Follow the milestone sequence in:

- `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md`

Treat the `## Current Cycle Queue (...)` section in that file as the
authoritative execution record.

Do not skip to later milestones before the earlier ones are materially closed.

Immediate next task:

Resume with the first `pending` task in the active queue and start with the
highest-value coherent slice under that task. Do not follow stale task numbers
from old summaries; the queue in `PROJECT_COMPLETION_PLAN.md` is authoritative.

Validation discipline for every slice:

1. implement one coherent slice
2. add or update focused tests
3. add or update runnable demos if operator behavior changed
4. update docs and ADRs when architecture or public behavior changed
5. run targeted tests first
6. run full `cargo test`
7. keep the branch clean at milestone boundaries
8. only then move to the next slice

You are done only when the `Definition of Done` in:

- `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md`

is fully satisfied.

Until then, continue implementing.
