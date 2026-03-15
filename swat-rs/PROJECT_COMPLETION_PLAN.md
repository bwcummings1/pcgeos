# PROJECT_COMPLETION_PLAN

This is the canonical implementation plan for completing the full `swat-rs`
project beyond the already-finished `v1` milestone.

If the task is "complete the entire project" rather than "maintain or extend
`v1`", this file wins over `IMPLEMENTATION_PLAN.md`.

## Objective

Close the remaining gap between `swat-rs v1` and the original PC/GEOS Swat as
a full debugger system, while preserving the modern Rust architecture and the
AI-native target model established in `v1`.

This means:

- keep the finished `v1` substrate intact and green
- preserve contextual coherence with the historical Swat implementation
- add the missing debugger breadth, command ecology, control model, and
  PC/GEOS-specific capability that still account for most of the parity gap

This does not mean "port the old tree file-for-file." It means "finish the
modern rewrite until the remaining parity gap is materially closed."

## Current state

As of `2026-03-14`:

- `swat-rs v1` is complete
- relative to the modern `v1` target, the project is `100%`
- relative to original PC/GEOS Swat as a full debugger system, the project is
  still only about `40%`

The missing `~60%` is concentrated in:

- debugger command ecology and operator vocabulary
- richer breakpoint/watchpoint and control semantics
- deeper symbolic/type/source/value behavior
- script-package parity with the legacy Tcl debugger surface
- PC/GEOS object, VM, symbol, patient, handle, and source compatibility
- a real `swat-adapter-pcgeos`
- more serious operator ergonomics around history, help, completion, and
  debugger-specific views

## Continuous execution harness

The active queue in this file is the authoritative execution record for the
remaining post-`v1` work.

Use these commands:

```bash
python3 scripts/check-implementation-status.py
python3 scripts/check-implementation-status.py --write-artifact
python3 scripts/render-implementation-status.py
python3 scripts/render-implementation-status.py --check
python3 scripts/run-continuous-agent.py
```

Execution rules:

1. Work the active queue in order unless a hard dependency forces resequencing.
2. After each completed task:
   - update its queue row in this file
   - refresh the generated status artifacts
   - run targeted validation
   - run full `cargo test`
   - commit the task
   - push the branch
   - continue immediately to the next task
3. If blocked, set the task status to `blocked` and record the blocker plus the
   exact next action in the `Notes` column before pausing.
4. Do not treat a task summary or milestone summary as a stopping point.
5. The generated artifacts under `docs/generated/` must stay aligned with the
   current queue section.
6. For unattended execution, use `scripts/run-continuous-agent.py` instead of
   launching a one-shot agent manually. The supervisor is responsible for
   relaunching the agent until the queue is complete or a blocker is recorded.
   By default it streams live runner output in the same terminal while
   preserving per-cycle logs under `swat-rs/.runs/continuous-agent/`.

## Current Cycle Queue (T-001..T-021)

| Task | Status | Milestone | Description | Branch | Commit | Notes |
|------|--------|-----------|-------------|--------|--------|-------|
| T-001 | done | M1 | Add a shared command/help registry and first debugger-family aliases for `stack`, `source`, and `breakpoint`. | swat-rs-full-completion-plan | 986f37e0 | Registry is now shared by shell help and TUI discovery; ADR 0026 landed. |
| T-002 | done | M1 | Add frame-oriented stack inspection APIs and shell/TUI workflows on top of the current boundary-span view. | swat-rs-full-completion-plan | 3a188575 | `swat-api` now projects stack frames from boundary spans, and both shell and TUI use the shared frame model. |
| T-003 | done | M1 | Add richer source listing and navigation workflows, including file-oriented discovery beyond event-scoped source lookup. | swat-rs-full-completion-plan | 3834ac16 | `swat-api` now catalogs source files and direct file views, and both shell and TUI expose `source files` / `source view` workflows. |
| T-004 | done | M1 | Deepen the `breakpoint` family into grouped views, richer metadata, and debugger-oriented inspection output. | swat-rs-full-completion-plan | 780f01cf | `swat-api` now projects shared breakpoint summaries/details/groups; `breakpoint list/show/groups` use that model while raw `triggers` stays compatible. |
| T-005 | done | M1 | Add shared help/completion/history/discovery metadata across shell and TUI. | swat-rs-full-completion-plan | 0fef11b3 | Shared registry search/completion now drives shell `rustyline` completion/history, TUI command-mode tab/history, and `help search <needle>` discovery. |
| T-006 | done | M1 | Add script/runtime wrappers for the first debugger command families on top of `swat-api`. | swat-rs-full-completion-plan | 45131efa | `swat-script` now wraps stack/source/breakpoint workflows through `swat-api` for both frozen script contexts and live script sessions, without shell-only shortcuts. |
| T-007 | done | M1 | Reconcile Milestone 1 docs, demos, and validation evidence until the milestone exit criteria are materially satisfied. | swat-rs-full-completion-plan | 379a608a | Milestone 1 closeout note, refreshed command demo coverage, and green focused/full validation now document the command-ecology exit criteria. |
| T-008 | done | M2 | Add grouped breakpoint definitions, reusable predicates, and richer stop-reason modeling. | swat-rs-full-completion-plan | 81da5574 | Named predicate libraries, user-defined breakpoint groups, and typed stop reasons now sit on the shared trigger substrate across `swat-control`, `swat-api`, and `swat-command`. |
| T-009 | done | M2 | Add watchpoints for values, objects, resources, and lifecycle-aware load/time break conditions. | swat-rs-full-completion-plan | 89d654ae | `swat-control` now supports value-change watchpoints plus elapsed-time and lifecycle-gated load predicates over shared value observations, with ADR 0029 documenting the stateful model. |
| T-010 | done | M2 | Expose advanced breakpoint/watchpoint management across shell, TUI, script, and public API. | swat-rs-full-completion-plan | a89bb7fd | `swat-api` now projects typed watchpoint specs/details, shell and TUI share live breakpoint/watchpoint management, `swat-script` wraps the same mutation surface, and trigger persistence v4 preserves watchpoints. |
| T-011 | done | M3 | Add typed frame/local/register inspection primitives for modern targets and shared APIs. | swat-rs-full-completion-plan | 6a370bc4 | `swat-api` now projects typed frame inspections with locals/registers from structured modern-target artifacts, shell and TUI expose `stack locals` / `stack registers`, and `swat-script` wraps the same frame-binding model. |
| T-012 | done | M3 | Add typed patient/handle/resource/object inspection models and debugger-native presentations. | swat-rs-full-completion-plan | 39e68bfb | Shared value/resolver/API layers now project typed patient/handle/resource/object models, and shell/TUI/script surfaces expose debugger-native inspection without changing `swat-core`. |
| T-013 | done | M3 | Deepen expression/value/source/resolver traversals for debugger-native workflows rather than only event queries. | swat-rs-full-completion-plan | 7275b0ca | Query fields now cover typed target entities and source lines, shared APIs project value history and source-function traversal, and shell/TUI/script surfaces expose the same debugger-native workflows. |
| T-014 | done | M4 | Add package-oriented script loading and shared command metadata integration. | swat-rs-full-completion-plan | cc3fb524 | Built-in script packages now autoload over the public API, and shell help/search/completion reuse the same package metadata. |
| T-015 | done | M4 | Migrate the highest-value legacy Tcl command families onto the new runtime and help surface. | swat-rs-full-completion-plan | 1f6fe82b | Legacy-style stack/source/patient/process/object helpers now ride on the shared runtime, help registry, and TUI surface. |
| T-016 | done | M5 | Implement PC/GEOS VM, symbol, geode, and object format readers or bridges. | swat-rs-full-completion-plan | d29487d9 | `swat-format-pcgeos` now parses PC/GEOS file headers, VM containers, geode imports, and object/symbol source-resource metadata, with repository-backed VM/data fixture tests. |
| T-017 | done | M5 | Model patient/handle/resource/geode/source relationships over real repository artifacts. | swat-rs-full-completion-plan | 6805b215 | `swat-format-pcgeos` now models real manifest and symbol fixtures into typed patient/geode/handle/resource/source relationships, with repository-backed tests and an inspection example. |
| T-018 | done | M6 | Implement `swat-adapter-pcgeos` host-side adapter and protocol bridge. | swat-rs-full-completion-plan | 36eea65b | `swat-adapter-pcgeos` now provides a fixture-first PC/GEOS adapter over real manifests/symbol VMs, and `swat-replay` now replays any boundary-payload event rather than only model/tool kinds. |
| T-019 | pending | M6 | Expose PC/GEOS control and inspection through shared APIs, shell, TUI, and demos. | - | - | Cover registers, memory, stack, patient, handle, and source flows. |
| T-020 | pending | M7 | Deepen shell and TUI debugger ergonomics, panes, replay views, breakpoint views, history, and completion. | - | - | Make long-session workflows practical instead of demo-oriented. |
| T-021 | pending | M8 | Final integration, docs, fixtures, demos, clean tree, and reproducible handoff until the full Definition of Done is satisfied. | - | - | This is the final closeout gate, not a cosmetic polish task. |

## Canonical reference set

Read these before major implementation:

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

Legacy technical anchors that matter for full completion:

- runtime/control:
  - `/home/ubuntu/pcgeos/Tools/swat/README`
  - `/home/ubuntu/pcgeos/Tools/swat/swat.c`
  - `/home/ubuntu/pcgeos/Tools/swat/rpc.c`
  - `/home/ubuntu/pcgeos/Tools/swat/event.c`
  - `/home/ubuntu/pcgeos/Tools/swat/cmd.c`
  - `/home/ubuntu/pcgeos/Tools/swat/ui.c`
- entities and target model:
  - `/home/ubuntu/pcgeos/Tools/swat/patient.c`
  - `/home/ubuntu/pcgeos/Tools/swat/patient.h`
  - `/home/ubuntu/pcgeos/Tools/swat/handle.c`
  - `/home/ubuntu/pcgeos/Tools/swat/handle.h`
  - `/home/ubuntu/pcgeos/Tools/swat/geos.h`
- control/breakpoints:
  - `/home/ubuntu/pcgeos/Tools/swat/break.c`
  - `/home/ubuntu/pcgeos/Tools/swat/break.h`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/bptutils.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/brkload.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/hwbrk.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/tbrk.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/timebrk.tcl`
- symbolic inspection:
  - `/home/ubuntu/pcgeos/Tools/swat/sym.c`
  - `/home/ubuntu/pcgeos/Tools/swat/type.c`
  - `/home/ubuntu/pcgeos/Tools/swat/expr.c`
  - `/home/ubuntu/pcgeos/Tools/swat/expr.y`
  - `/home/ubuntu/pcgeos/Tools/swat/value.c`
  - `/home/ubuntu/pcgeos/Tools/swat/var.c`
  - `/home/ubuntu/pcgeos/Tools/swat/src.c`
  - `/home/ubuntu/pcgeos/Tools/swat/file.c`
  - `/home/ubuntu/pcgeos/Tools/swat/vmsym.h`
- script/runtime surface:
  - `/home/ubuntu/pcgeos/Tools/swat/tclDebug.c`
  - `/home/ubuntu/pcgeos/Tools/swat/tcl/README`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/autoload.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/toplevel.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/help.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/stack.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/patient.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/process.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/object.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/objwatch.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/srclist.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/Doc/cmds.ms`
  - `/home/ubuntu/pcgeos/Tools/swat/makedoc.c`
- protocol/stub:
  - `/home/ubuntu/pcgeos/Tools/swat/rpc.h`
  - `/home/ubuntu/pcgeos/Tools/swat/Doc/stub.ms`
  - `/home/ubuntu/pcgeos/Tools/swat/Doc/rpc.g`
  - `/home/ubuntu/pcgeos/Tools/swat/Stub/`
  - `/home/ubuntu/pcgeos/Tools/swat/Stub32/`
- PC/GEOS formats:
  - `/home/ubuntu/pcgeos/Tools/include/objfmt.h`
  - `/home/ubuntu/pcgeos/Tools/include/geode.h`
  - `/home/ubuntu/pcgeos/Tools/include/lmem.h`
  - `/home/ubuntu/pcgeos/Tools/include/os90.h`
  - `/home/ubuntu/pcgeos/Tools/utils/vm.h`
  - `/home/ubuntu/pcgeos/Tools/glue/vm.c`
  - `/home/ubuntu/pcgeos/Tools/utils/objSwap.c`
  - `/home/ubuntu/pcgeos/Tools/utils/fileUtil.c`
  - `/home/ubuntu/pcgeos/Tools/utils/sttab.c`
- UI/help/history:
  - `/home/ubuntu/pcgeos/Tools/swat/help.c`
  - `/home/ubuntu/pcgeos/Tools/swat/curses.c`
  - `/home/ubuntu/pcgeos/Tools/swat/curses/`
  - `/home/ubuntu/pcgeos/Tools/swat/ntcurses/`
  - `/home/ubuntu/pcgeos/Tools/swat/x11/`
  - `/home/ubuntu/pcgeos/Tools/swat/hist/`

## Full-completion target

The project counts as complete only when all of these are true:

1. `v1` remains green and intact
2. the shell and script surfaces expose a serious debugger command ecology,
   not just a trace-inspection shell
3. the control model includes debugger-grade breakpoints and watchpoints rather
   than only generic semantic triggers
4. value/type/source/resolver behavior covers real PC/GEOS artifacts and real
   debugger workflows
5. the script runtime replaces the highest-value legacy Tcl command families
   with public-API-backed packages
6. a `swat-adapter-pcgeos` exists and can inspect/control a real or
   fixture-backed PC/GEOS session
7. operator surfaces support practical debugging workflows for both modern
   targets and the PC/GEOS target
8. docs, fixtures, ADRs, demos, and tests make the completed system
   reproducible without chat history

## What full completion does not require

These are useful, but they are not blockers for project completion:

- a web UI
- IDE integrations
- a generalized distributed trace fabric
- published SDK packages to PyPI/npm
- replacing the legacy `Tools/swat` implementation in-place

## Non-negotiable rules

1. Keep all new implementation inside `/home/ubuntu/pcgeos/swat-rs`.
2. Do not regress the existing `v1` behavior while closing the parity gap.
3. Do not leak PC/GEOS-specific details into `swat-core`.
4. Keep heavy payloads and dumps in artifacts, not inline events.
5. Keep UI logic above the substrate and public APIs.
6. Reuse the shared public API from shell, script, TUI, and future clients.
7. Treat the legacy tree as a reference oracle, not as the implementation site.
8. Every meaningful public or architectural change must update docs and, when
   appropriate, add an ADR.
9. Do not claim the project is complete until the `Definition of Done` section
   below is fully satisfied.

## Primary modern code seams

These are the files and directories most likely to absorb the remaining work:

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

Expected new landing zones during full completion:

- `swat-adapter-pcgeos`
- optionally `swat-format-pcgeos` or similar support crates for VM/symbol/object
  formats if that keeps `swat-adapter-pcgeos` clean

## Milestone order

Implement in this order unless a hard blocker forces resequencing.

### Milestone 1: Command ecology and operator vocabulary

Goal:

- close the gap between the current shell/TUI and the breadth of workflows that
  legacy Swat exposed through commands, help, and scripted helpers

Required work:

- formalize command families instead of a flat shell-only grammar
- add debugger workflows for:
  - stack/frame inspection
  - patient/process inspection
  - object/handle inspection
  - richer source listing/navigation
  - breakpoint/watchpoint inspection
- deepen help, completion, history, and discoverability
- keep the TUI on the same command/API substrate

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/cmd.c`
  - `/home/ubuntu/pcgeos/Tools/swat/help.c`
  - `/home/ubuntu/pcgeos/Tools/swat/Doc/cmds.ms`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/help.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/stack.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/patient.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/process.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/object.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/objwatch.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/srclist.tcl`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-script/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-ui-tui/`

Exit criteria:

- the command surface is organized around debugger workflows, not just raw API
  calls
- help and completion expose those workflows
- shell and TUI can both drive the expanded vocabulary

### Milestone 2: Advanced breakpoint, watchpoint, and stop-reason model

Goal:

- move from semantic triggers to a full debugger control system

Required work:

- extend the trigger model to support:
  - conditional breakpoints with reusable predicates
  - load-aware breakpoints
  - object/value watchpoints
  - time-based break conditions
  - grouped breakpoint state and enable policies
- model richer stop reasons and stop-state inspection
- expose advanced breakpoint management from shell, TUI, script, and public API

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/break.c`
  - `/home/ubuntu/pcgeos/Tools/swat/break.h`
  - `/home/ubuntu/pcgeos/Tools/swat/event.c`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/bptutils.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/brkload.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/hwbrk.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/tbrk.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/timebrk.tcl`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-control/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-ui-tui/`

Exit criteria:

- advanced break/watch workflows are possible without ad hoc code changes
- stop reasons are first-class inspection objects
- tests cover grouped and lifecycle-aware breakpoint behavior

### Milestone 3: Typed inspection, entities, and source parity

Goal:

- make symbolic inspection feel like a debugger again, not just a structured
  trace browser

Required work:

- deepen `swat-expr` beyond the current query grammar where needed
- add typed presentations for frames, locals, registers, patients, handles,
  resources, and objects
- deepen resolver/source support for debugger-oriented traversals
- add stack/frame APIs and shell/TUI affordances
- preserve operator-friendly formatting across shell and TUI

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/sym.c`
  - `/home/ubuntu/pcgeos/Tools/swat/type.c`
  - `/home/ubuntu/pcgeos/Tools/swat/expr.c`
  - `/home/ubuntu/pcgeos/Tools/swat/expr.y`
  - `/home/ubuntu/pcgeos/Tools/swat/value.c`
  - `/home/ubuntu/pcgeos/Tools/swat/var.c`
  - `/home/ubuntu/pcgeos/Tools/swat/src.c`
  - `/home/ubuntu/pcgeos/Tools/swat/file.c`
  - `/home/ubuntu/pcgeos/Tools/swat/patient.c`
  - `/home/ubuntu/pcgeos/Tools/swat/handle.c`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-expr/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-value/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-resolver/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-source/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`

Exit criteria:

- users can inspect debugger-native entities rather than only generic events
- stack/source/type workflows are first-class
- tests cover entity, frame, and source traversals

### Milestone 4: Script-package parity and command-library migration

Goal:

- recover the operator power that legacy Swat derived from its Tcl library set

Required work:

- support autoloadable or package-oriented scripts on top of the public API
- migrate the highest-value legacy command families conceptually into the Rust
  scripting/runtime model
- keep command docs and help metadata synchronized with the script/runtime
  surface
- ensure script-space can participate in control, breakpoint, and inspection
  workflows without bypassing policy/capability rules

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/tclDebug.c`
  - `/home/ubuntu/pcgeos/Tools/swat/tcl/README`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/autoload.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/toplevel.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/help.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/stack.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/patient.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/process.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/object.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/objwatch.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/lib.new/srclist.tcl`
  - `/home/ubuntu/pcgeos/Tools/swat/makedoc.c`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-script/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/src/lib.rs`

Exit criteria:

- key legacy command families exist in the new runtime
- script packages load without bypassing the public API
- help/docs are generated or synchronized from the same source of truth

### Milestone 5: PC/GEOS object, VM, symbol, and source support

Goal:

- add the repository-specific technical substrate needed for real PC/GEOS
  debugging instead of only modern-target inspection

Required work:

- implement Rust readers or bridges for the necessary PC/GEOS formats:
  - VM
  - symbol metadata
  - geode/object metadata
  - source and resource mappings
- model patient/handle/resource/geode relationships on top of those formats
- add fixture-backed tests using real repository artifacts where feasible

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/include/objfmt.h`
  - `/home/ubuntu/pcgeos/Tools/include/geode.h`
  - `/home/ubuntu/pcgeos/Tools/include/lmem.h`
  - `/home/ubuntu/pcgeos/Tools/include/os90.h`
  - `/home/ubuntu/pcgeos/Tools/utils/vm.h`
  - `/home/ubuntu/pcgeos/Tools/glue/vm.c`
  - `/home/ubuntu/pcgeos/Tools/utils/objSwap.c`
  - `/home/ubuntu/pcgeos/Tools/utils/fileUtil.c`
  - `/home/ubuntu/pcgeos/Tools/utils/sttab.c`
  - `/home/ubuntu/pcgeos/Tools/swat/vmsym.h`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-resolver/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-source/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-value/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`
  - future `swat-adapter-pcgeos`
  - optional future support crate for PC/GEOS formats

Exit criteria:

- `swat-rs` can reason over real PC/GEOS artifacts from this repository
- patients, handles, resources, and source locations become inspectable objects
- tests use fixture-backed repository data rather than synthetic placeholders

### Milestone 6: PC/GEOS target adapter and protocol bridge

Goal:

- finish the historical target side rather than only the modern-target side

Required work:

- implement `swat-adapter-pcgeos`
- bridge or reimplement the necessary host/target protocol semantics for
  attach, pause, resume, step, register access, memory access, stop reasons,
  and target-originated events
- use captured transcript fixtures where direct live automation is too fragile,
  but also support a real target/emulator path when available
- surface PC/GEOS-specific entities through shared APIs, not adapter-private
  side channels

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/rpc.h`
  - `/home/ubuntu/pcgeos/Tools/swat/Doc/stub.ms`
  - `/home/ubuntu/pcgeos/Tools/swat/Doc/rpc.g`
  - `/home/ubuntu/pcgeos/Tools/swat/Stub/`
  - `/home/ubuntu/pcgeos/Tools/swat/Stub32/`
  - `/home/ubuntu/pcgeos/Tools/swat/ibm.c`
  - `/home/ubuntu/pcgeos/Tools/swat/ibm86.c`
  - `/home/ubuntu/pcgeos/Tools/swat/ibmCmd.c`
  - `/home/ubuntu/pcgeos/Tools/swat/ibmCache.c`
  - `/home/ubuntu/pcgeos/Tools/swat/i86Opc.c`
  - `/home/ubuntu/pcgeos/Tools/swat/patient.c`
  - `/home/ubuntu/pcgeos/Tools/swat/handle.c`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-core/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-protocol/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-session/src/lib.rs`
  - `/home/ubuntu/pcgeos/swat-rs/swat-replay/src/lib.rs`
  - future `swat-adapter-pcgeos`

Exit criteria:

- `swat-rs` can attach to a PC/GEOS target or a replayable stub-session fixture
- register, memory, stack, patient, handle, and source inspection work through
  shared APIs
- control flows are exercised by tests or scripted fixture demos

### Milestone 7: Operator parity and debugger-grade ergonomics

Goal:

- make the shell and TUI feel like serious debuggers rather than demos plus
  diagnostics

Required work:

- deepen history, completion, and help behavior
- add dedicated views for:
  - breakpoints/watchpoints
  - stack frames
  - patients/handles/objects
  - snapshots/replay
  - source navigation
- harden command output and navigation for larger sessions
- ensure TUI and shell reuse the same help/command metadata where practical

Primary references:

- legacy:
  - `/home/ubuntu/pcgeos/Tools/swat/ui.c`
  - `/home/ubuntu/pcgeos/Tools/swat/help.c`
  - `/home/ubuntu/pcgeos/Tools/swat/curses.c`
  - `/home/ubuntu/pcgeos/Tools/swat/curses/`
  - `/home/ubuntu/pcgeos/Tools/swat/ntcurses/`
  - `/home/ubuntu/pcgeos/Tools/swat/x11/`
  - `/home/ubuntu/pcgeos/Tools/swat/hist/`
- modern:
  - `/home/ubuntu/pcgeos/swat-rs/swat-command/`
  - `/home/ubuntu/pcgeos/swat-rs/swat-ui-tui/`
  - `/home/ubuntu/pcgeos/swat-rs/swat-api/src/lib.rs`

Exit criteria:

- shell and TUI both support practical long-session workflows
- users can navigate PC/GEOS and modern targets without UI-specific logic forks
- operator ergonomics are tested instead of treated as polish

### Milestone 8: Final integration, proof, and handoff

Goal:

- leave a reproducible full-completion checkpoint, not only a plausible design

Required work:

- keep `v1` demos green
- add fixture-backed or live demos for the PC/GEOS adapter path
- update docs and ADRs to reflect the completed architecture
- ensure the completion prompt can drive another agent without chat history
- leave a clean, reproducible git checkpoint

Exit criteria:

- the `Definition of Done` section below is fully satisfied

## Validation baseline

The `v1` validation set must remain green:

```bash
cargo test
cargo run -p swat-session --example mock_session
cargo run -p swat-session --example local_process
cargo run -p swat-session --example python_trace
cargo run -p swat-session --example agent_trace
cargo run -p swat-command -- mock
cargo run -p swat-ui-tui -- --headless --ticks 4 mock
cargo run -p swat-agent-protocol --example emit_protocol
python3 sdk/python/examples/emit_protocol.py
bun run sdk/typescript/examples/emit_protocol.ts
```

Full-completion validation must also add:

- fixture-backed PC/GEOS format tests
- fixture-backed or live `swat-adapter-pcgeos` demos
- shell/TUI workflows for patients, handles, frames, breakpoints, and source
- script-package loading and help-surface validation

## Definition of Done

Do not call the whole project complete until all of the following are true:

1. all `v1` validation still passes
2. `swat-command` exposes debugger-grade workflows for:
   - stack/frame inspection
   - patient/process inspection
   - object/handle/resource inspection
   - source listing/navigation
   - advanced breakpoint/watchpoint management
   - snapshots/replay
3. `swat-script` supports package-oriented command libraries on the shared
   public API and replaces the highest-value legacy Tcl families conceptually
4. `swat-value`, `swat-expr`, `swat-resolver`, and `swat-source` support real
   PC/GEOS artifact workflows rather than only synthetic modern traces
5. `swat-adapter-pcgeos` exists and can control/inspect a real or
   replay-fixture PC/GEOS session through shared APIs
6. shell and TUI both operate over shared help/command/inspection surfaces and
   support practical debugger workflows
7. docs, ADRs, fixtures, demos, and prompts are sufficient for another engineer
   or agent to continue without chat history
8. the workspace and git tree are left in a clean, reproducible state

## Recommended execution loop

Repeat this loop until the `Definition of Done` above is satisfied:

1. choose the next pending or blocked task in the current queue
2. inspect only the files relevant to that task
3. implement one coherent slice
4. add or update focused tests and demos
5. update docs and ADRs if public behavior or architecture changed
6. update the queue row in this file
7. run `python3 scripts/render-implementation-status.py`
8. run targeted tests first
9. run full `cargo test`
10. commit and push the task
11. keep the tree clean at task and milestone boundaries
12. then move directly to the next slice

## Immediate next task

Resume with the first `pending` task in the active queue and begin with the
highest-value coherent slice under that task.

For unattended runs, use:

`python3 scripts/run-continuous-agent.py`
