You are continuing implementation of `swat-rs` inside `/home/ubuntu/pcgeos/swat-rs`.

This prompt is the historical `v1` continuation prompt.

`swat-rs v1` is now complete. If the task is to finish the whole project
beyond `v1`, use:

- `/home/ubuntu/pcgeos/swat-rs/PROJECT_CONTINUATION_PROMPT.md`

Your job is not to redesign the project. Your job is to preserve contextual
coherence with legacy PC/GEOS Swat, consume the existing plan/docs/code, and
continue implementation until `swat-rs v1` is actually complete.

Read these first, in order:

1. `/home/ubuntu/pcgeos/swat-rs/AGENTS.md`
2. `/home/ubuntu/pcgeos/swat-rs/README.md`
3. `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md`
4. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/phase-0-spec.md`
5. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/reference-map.md`
6. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/legacy-subsystem-inventory.md`
7. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/execution-strategy.md`
8. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/crate-map.md`
9. `/home/ubuntu/pcgeos/swat-rs/docs/architecture/agent-event-protocol.md`
10. all ADRs in `/home/ubuntu/pcgeos/swat-rs/docs/adrs/` in sorted order

Then inspect the current implementation seams:

- `/home/ubuntu/pcgeos/swat-rs/swat-core/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-session/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-store/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-replay/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-control/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-command/src/main.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-resolver/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-source/src/lib.rs`
- `/home/ubuntu/pcgeos/swat-rs/swat-script/src/lib.rs`

Hard constraints:

1. Do not implement inside `Tools/swat`.
2. Do not leak adapter-specific assumptions into `swat-core`.
3. Do not inline heavyweight payloads that should be artifacts.
4. Do not move UI logic into lower layers.
5. Do not claim the project is complete until every item in the Definition of
   Done section of `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md` is
   satisfied.
6. Do not stop to ask whether to move to the next milestone unless you are
   truly blocked by conflicting requirements or unavailable external resources.
7. Keep docs and ADRs current as implementation changes.
8. Keep the workspace green. Run narrow tests while iterating and full
   `cargo test` before closing a milestone.

Execution order:

Follow the milestone sequence in:

- `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md`

Do not skip to stretch goals before required milestones are complete.

Immediate next task:

Start with Milestone 1, and begin with the highest-value first slice:

- extend `TriggerAction` beyond pause-only behavior
- add trigger enable/disable
- add shell-level `until <expr>`

Validation discipline for every slice:

1. implement one coherent slice
2. add or update focused tests
3. add or update runnable demos if operator behavior changed
4. update docs and ADRs when architecture or public behavior changed
5. run targeted tests first
6. run full `cargo test`
7. only then move to the next slice

You are done only when the Definition of Done in:

- `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md`

is fully satisfied.

Until then, continue implementing.
