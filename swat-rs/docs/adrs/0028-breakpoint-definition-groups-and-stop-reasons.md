# ADR 0028: Breakpoint Definition Groups and Stop Reasons

- Status: accepted
- Date: 2026-03-15

## Context

Milestone 1 gave `swat-rs` a debugger-facing `breakpoint` vocabulary, but the
underlying control model still behaved like a thin projection of ad hoc
triggers:

- predicates were always inline and could not be reused
- breakpoints had no shared group policy beyond per-breakpoint enable bits
- clients inferred "why execution stopped" from trigger-hit events and control
  text instead of a typed stop-reason model

The legacy Swat breakpoint stack was materially richer. It treated reusable
criteria, grouped control state, and stop reasons as part of the debugger
model rather than shell-local convenience.

## Decision

Keep the existing trigger engine as the execution substrate, but extend it with
debugger-grade metadata and projections:

- `swat-control` now supports named predicates through a predicate library
- breakpoints may belong to user-defined groups with shared enable policies
- controlled pump reports now carry typed stop reasons for breakpoint stops,
  lifecycle exits, and rejected control attempts
- `swat-api` projects named predicate inventory, definition groups, and both
  configured and effective breakpoint state

The command surface uses these shared models directly instead of maintaining a
shell-private breakpoint layer.

## Consequences

Benefits:

- operators can define one predicate and reuse it across multiple breakpoints
- group-level enable/disable changes no longer require mutating every member
  breakpoint
- shell and future clients can report stop causes from a typed model instead of
  scraping incidental event text

Tradeoffs:

- the runtime substrate is still trigger-backed rather than a target-native
  breakpoint/watchpoint backend
- watchpoints, load/time break conditions, and broader script/TUI management
  still land in later Milestone 2 tasks
- predicate references currently resolve on the host, so missing definitions are
  a host-side configuration error rather than an adapter capability

## Follow-on Work

- add watchpoints, load-aware breaks, and time-oriented breaks on the same
  grouped definition model
- expose the advanced breakpoint surface uniformly through script and richer TUI
  workflows
- widen stop-reason reporting once adapter-native breakpoint/watchpoint backends
  exist
