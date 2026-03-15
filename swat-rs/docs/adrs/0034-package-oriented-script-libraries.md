# ADR 0034: Package-Oriented Script Libraries

## Context

By the start of `T-014`, `swat-script` exposed a useful `ctx.*` inspection
surface, but it still lacked the package/library model that made legacy Swat's
Tcl environment operationally powerful:

- there was no script-package catalog analogous to the old autoload families
- shell help/search/completion knew nothing about script-space libraries
- migrating higher-value legacy command helpers in Milestone 4 would have
  required either duplicating metadata or hard-coding shell-only shortcuts

Legacy Swat's `autoload.tcl`, `help.tcl`, and the family libraries under
`lib.new/` treated script-space as part of the debugger surface. The new
workspace still needs that operator shape without recreating Tcl as the
architecture.

## Decision

Add built-in package-oriented Rhai libraries in `swat-script` and make their
metadata the shared source of truth for the shell help/discovery surface.

Concretely:

- `swat-script` now defines built-in packages for:
  - `process`
  - `stack`
  - `patient`
  - `object`
  - `source`
- each package carries:
  - exported helper names
  - summary/notes metadata
  - legacy reference links
- `ScriptHost` now supports:
  - package enumeration
  - explicit package loading
  - export-driven autoload during evaluation
- `swat-command` now exposes:
  - `script packages`
  - `script package show <name>`
  - `script package load <name>`
- `swat-command` help/search/completion now reuses the same package metadata
  rather than describing a separate shell-only model

## Consequences

Positive:

- Milestone 4 now has a stable package-loading seam for migrating legacy Tcl
  command families conceptually
- the shell help surface and script library surface stay synchronized from the
  same metadata
- package helpers still route through `ctx` and shared inspection APIs, so no
  lower-layer bypass is introduced

Tradeoffs:

- the current autoload behavior is intentionally narrower than Tcl's dynamic
  loader; it targets built-in package exports rather than general filesystem
  discovery
- package helpers are still small wrappers over existing shared APIs, not full
  legacy command parity on their own

## Follow-up

- migrate the highest-value legacy command helpers onto these packages in
  `T-015`
- keep package metadata and shell help synchronized as additional command
  families land
- extend the same package approach to real PC/GEOS inspection once Milestone 5
  artifact readers are available
