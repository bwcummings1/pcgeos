# ADR 0031: Typed Frame Locals and Registers

## Context

Milestone 1 made stack frames visible again, but only as structural spans:

- `swat-api` could project frame labels, depth, source location, and summaries
- `swat-command` and `swat-ui-tui` could list frames and inspect boundary spans
- scripts could count frames and read frame labels

That still left a debugger-shaped gap. Operators could see that a frame existed,
but not inspect the typed data bound to it. For modern targets we already carry
structured JSON artifacts on frame-related events, especially through the agent
adapter, so the missing piece was a shared typed projection rather than a new
transport.

## Decision

Add typed frame-local and frame-register inspection to `swat-api` and expose it
uniformly through the shell, TUI, and script wrappers.

Concretely:

- `swat-api` now exposes:
  - `InspectedValueKind`
  - `FrameLocal`
  - `FrameRegister`
  - `StackFrameInspection`
  - `stack_frame_inspection`
  - `stack_frame_locals`
  - `stack_frame_registers`
- locals and registers are extracted from structured artifact JSON on frame
  events, starting with top-level `locals` and `registers` objects
- `swat-command` now supports `stack locals <index>` and
  `stack registers <index>`, and `stack frame <index>` includes local/register
  counts plus the typed bindings inline
- `swat-script` and `swat-ui-tui` use the same shared inspection model rather
  than reconstructing frame bindings locally

This keeps `swat-core` target-neutral: the substrate only carries artifacts and
events, while the higher-level inspection layer interprets structured frame
payloads where adapters provide them.

## Consequences

Positive:

- debugger workflows can inspect frame-native data instead of only generic
  event metadata
- shell, TUI, and script surfaces stay synchronized on the same typed frame
  model
- modern structured runtimes can opt into richer inspection by adding
  `locals`/`registers` fields to existing artifacts without introducing a new
  core protocol

Tradeoffs:

- locals/registers are currently only available where adapters emit structured
  frame payloads; older or simpler adapters may still return empty sets
- register grouping is intentionally lightweight for now and is derived from the
  artifact payload rather than a dedicated core type system

## Follow-up

- widen the same typed inspection model to patient, handle, resource, and
  object entities in the remaining Milestone 3 work
- let future PC/GEOS adapters populate the same frame inspection surface with
  real machine registers and symbolic locals instead of creating a separate API
