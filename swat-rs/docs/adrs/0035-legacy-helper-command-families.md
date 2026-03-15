# ADR 0035: Legacy Helper Command Families

## Context

By the end of `T-014`, the workspace had package-oriented script libraries and
shared help metadata, but Milestone 4 still lacked the legacy-style command
entry points operators actually reached for first:

- stack helpers such as `backtrace`, `where`, `func`, `up`, `down`, and
  `locals`
- source helpers such as `slist` and `view`
- patient/process/object helpers such as `patient-default`, `spawn`,
  `wakeup`, `obj-name`, and `obj-class`

The shared APIs already exposed the right data, but the shell and TUI still
forced operators to translate those habits into the newer `stack` / `source`
family grammar by hand.

## Decision

Add legacy-style helper commands on top of the existing shared APIs and keep
them inside the same command/help/runtime surface as the rest of `swat-rs`.

Concretely:

- `swat-command` now exposes:
  - `backtrace`, `where`, `func`, `up`, `down`, `locals`
  - `slist`, `view`
  - `patient-default`, `spawn`, `wakeup`
  - `obj-name`, `obj-class`
- the live command runtime now keeps:
  - a selected frame cursor
  - a default patient binding
- `swat-ui-tui` now understands the same helper commands rather than treating
  them as shell-only aliases
- all of these helpers route through the shared stack/source/patient/object
  inspection APIs or the existing `until` machinery instead of recreating Tcl
  event hooks or target-specific logic

## Consequences

Positive:

- operators can use debugger-native entry points without fragmenting the public
  API surface
- legacy habits now map onto the same models the TUI and script packages use
- process-oriented helpers stay capability-aware because they reuse the shared
  resume/pump/trigger path

Tradeoffs:

- these helpers preserve the conceptual workflow, not the exact historical
  Tcl semantics of PC/GEOS thread internals
- the selected-frame and default-patient state live in the operator surfaces,
  not in lower substrate crates, because they are interaction concerns

## Follow-up

- continue filling in higher-value legacy helper gaps only when they map
  cleanly onto shared APIs
- feed the same helpers from real PC/GEOS artifact readers and adapters in
  Milestones 5 and 6
- keep help/registry metadata synchronized as additional legacy command
  migrations land
