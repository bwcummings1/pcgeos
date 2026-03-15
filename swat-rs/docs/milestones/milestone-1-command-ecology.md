# Milestone 1 Command Ecology Closeout

Milestone 1 is the command-ecology and operator-vocabulary checkpoint for the
post-`v1` completion plan. This note records what now exists, which demos and
tests cover it, and which debugger areas are still intentionally deferred to
later milestones.

## What Landed

The command surface is now organized around debugger workflows instead of only
raw trace-inspection verbs:

- `stack`
  - frame-oriented stack views are projected through `swat-api`
- `source`
  - event-backed source lookup, file discovery, and direct file views are
    exposed through shared APIs
- `breakpoint`
  - grouped breakpoint views, detail inspection, and compatibility-trigger
    workflows are exposed through shared breakpoint models

Help, discovery, and command-entry ergonomics are now shared across the shell
and TUI:

- the registry in `swat-command` is the source of truth for help topics,
  synopsis strings, aliases, shell/TUI support markers, and completion
  candidates
- the shell now uses `rustyline` for interactive tab completion and in-session
  history
- the TUI command entry now uses the same completion metadata plus in-session
  history navigation
- `help search <needle>` exposes shared command-discovery metadata on both
  shell and TUI surfaces

The script/runtime surface now mirrors the first debugger families through
`swat-api` instead of shell-local shortcuts:

- frozen `ScriptHost` contexts expose stack/source inspection helpers
- live `LiveScriptSession` wrappers expose breakpoint, stack, and source
  helpers on top of shared public APIs

## Legacy Alignment

This milestone is the first material bridge from the old Swat command ecology
to the new Rust operator surfaces. The most relevant reference anchors remain:

- `Tools/swat/cmd.c`
- `Tools/swat/help.c`
- `Tools/swat/Doc/cmds.ms`
- `Tools/swat/lib.new/help.tcl`
- `Tools/swat/lib.new/stack.tcl`
- `Tools/swat/lib.new/srclist.tcl`

The modern implementation does not recreate Tcl command packages verbatim. It
does preserve the debugger-family shape those packages gave operators: stack,
source, breakpoint, and discovery workflows all now sit above shared APIs.

## Intentional Deferrals

Milestone 1 is materially satisfied, not globally complete.

The following remain deferred by design:

- richer breakpoint and watchpoint semantics beyond the trigger-backed model
  move to Milestone 2
- patient/process/object/handle inspection families move with the typed target
  inspection work in later milestones
- package-oriented command migration from legacy Tcl families moves to
  Milestone 4

## Demos

Useful command-ecology demos:

- `cargo run -p swat-command --example agent_commands`
- `cargo run -p swat-command --example mock_trigger_controls`
- `cargo run -p swat-command -- mock`
- `cargo run -p swat-ui-tui -- --headless --ticks 4 mock`

The command examples now cover:

- shared help and help-search discovery
- stack/source/breakpoint workflows
- grouped breakpoint inspection output
- script-backed inspection from the live command surface

## Validation Evidence

Focused validation for this milestone:

- `cargo test -p swat-command`
- `cargo test -p swat-ui-tui`
- `cargo test -p swat-script`

Workspace validation:

- `cargo test`

These test slices cover:

- registry help, search, and completion metadata
- shell breakpoint/source/stack workflows
- TUI command-entry history and completion
- script wrappers for stack/source/breakpoint families

## Milestone 1 Exit

Milestone 1 is now materially satisfied for the full-completion queue because:

1. the command surface is organized around debugger workflows
2. help and completion expose those workflows on shared metadata
3. shell and TUI both drive the expanded vocabulary on the same substrate

The queue can now move to Milestone 2 work on richer breakpoint/watchpoint and
stop-reason semantics without reopening the basic command-ecology foundation.
