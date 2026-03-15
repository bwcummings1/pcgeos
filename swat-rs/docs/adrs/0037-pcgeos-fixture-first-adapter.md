# 0037. PC/GEOS Fixture-First Adapter

Date: 2026-03-15

## Status

Accepted

## Context

Milestone 6 requires a real `swat-adapter-pcgeos`, but the repository does not
yet offer a robust, automatable live PC/GEOS target or emulator workflow that
is suitable for fast, repeatable workspace tests.

At the same time, Milestone 5 already produced two critical ingredients:

- real repository artifact readers in `swat-format-pcgeos`
- real patient/geode/handle/resource/source relationship modeling over checked
  in manifests and symbol VMs

The adapter needs to start using that substrate without waiting for the full
legacy RPC bridge to be productionized.

## Decision

Implement `swat-adapter-pcgeos` as a fixture-first adapter on the shared
`swat-core::TargetAdapter` surface.

The first slice:

- loads a JSON fixture spec that points at real repository `.gp` manifests,
  symbol VMs, and source files
- builds shared inventory artifacts from `swat-format-pcgeos`
- emits recorded PC/GEOS stop frames with register/local/source metadata
- supports replay-plan injection through `swat-session` and `swat-replay`
  without adding adapter-private side channels

Live legacy RPC transport and emulator attachment remain follow-on work inside
the same crate, above `swat-core` and below the shared API/shell/TUI layers.

## Consequences

Positive:

- `swat-adapter-pcgeos` now exists and participates in the same attach/control/
  replay contracts as the existing adapters
- repository-backed tests can validate PC/GEOS session behavior without a live
  emulator dependency
- later live RPC work can reuse the same adapter-facing data model and session
  plumbing

Tradeoffs:

- the first adapter slice emits recorded stop-state fixtures rather than a live
  machine stream
- some runtime state is synthesized from repository metadata where the checked
  in tree lacks full live stop transcripts
