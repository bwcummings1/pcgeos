# AGENTS

This repository contains two distinct concerns:

1. the historical PC/GEOS source tree
2. the modern `swat-rs` debugger workspace under `/home/ubuntu/pcgeos/swat-rs`

## Default routing

If the task is about the modern debugger rewrite, work in:

- `/home/ubuntu/pcgeos/swat-rs`

Read these first:

1. `/home/ubuntu/pcgeos/swat-rs/AGENTS.md`
2. `/home/ubuntu/pcgeos/swat-rs/README.md`
3. `/home/ubuntu/pcgeos/swat-rs/IMPLEMENTATION_PLAN.md`
4. `/home/ubuntu/pcgeos/swat-rs/PROJECT_COMPLETION_PLAN.md` when the goal is
   to complete the entire project rather than maintain `v1`

## Legacy tree policy

The historical debugger sources under `Tools/swat` are reference material unless
the user explicitly asks for legacy-tree modifications.

Do not intermingle new Rust implementation into:

- `/home/ubuntu/pcgeos/Tools/swat`

## Git hygiene

- keep the worktree clean at milestone boundaries
- do not revert unrelated user changes
- run the relevant test slice before finalizing changes
