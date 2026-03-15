# Current Queue Summary

Generated from `PROJECT_COMPLETION_PLAN.md`. That plan remains canonical.

- Queue: `T-001..T-021`
- Progress: `19/21 done`
- Pending: `2`
- Blocked: `0`

## Task Snapshot

| Task | Status | Milestone | Branch | Commit |
|------|--------|-----------|--------|--------|
| T-001 | done | M1 | swat-rs-full-completion-plan | 986f37e0 |
| T-002 | done | M1 | swat-rs-full-completion-plan | 3a188575 |
| T-003 | done | M1 | swat-rs-full-completion-plan | 3834ac16 |
| T-004 | done | M1 | swat-rs-full-completion-plan | 780f01cf |
| T-005 | done | M1 | swat-rs-full-completion-plan | 0fef11b3 |
| T-006 | done | M1 | swat-rs-full-completion-plan | 45131efa |
| T-007 | done | M1 | swat-rs-full-completion-plan | 379a608a |
| T-008 | done | M2 | swat-rs-full-completion-plan | 81da5574 |
| T-009 | done | M2 | swat-rs-full-completion-plan | 89d654ae |
| T-010 | done | M2 | swat-rs-full-completion-plan | a89bb7fd |
| T-011 | done | M3 | swat-rs-full-completion-plan | 6a370bc4 |
| T-012 | done | M3 | swat-rs-full-completion-plan | 39e68bfb |
| T-013 | done | M3 | swat-rs-full-completion-plan | 7275b0ca |
| T-014 | done | M4 | swat-rs-full-completion-plan | cc3fb524 |
| T-015 | done | M4 | swat-rs-full-completion-plan | 1f6fe82b |
| T-016 | done | M5 | swat-rs-full-completion-plan | d29487d9 |
| T-017 | done | M5 | swat-rs-full-completion-plan | 6805b215 |
| T-018 | done | M6 | swat-rs-full-completion-plan | 36eea65b |
| T-019 | done | M6 | swat-rs-full-completion-plan | f39efc6a |
| T-020 | pending | M7 | - | - |
| T-021 | pending | M8 | - | - |

## Task Notes

| Task | Description | Notes |
|------|-------------|-------|
| T-001 | Add a shared command/help registry and first debugger-family aliases for `stack`, `source`, and `breakpoint`. | Registry is now shared by shell help and TUI discovery; ADR 0026 landed. |
| T-002 | Add frame-oriented stack inspection APIs and shell/TUI workflows on top of the current boundary-span view. | `swat-api` now projects stack frames from boundary spans, and both shell and TUI use the shared frame model. |
| T-003 | Add richer source listing and navigation workflows, including file-oriented discovery beyond event-scoped source lookup. | `swat-api` now catalogs source files and direct file views, and both shell and TUI expose `source files` / `source view` workflows. |
| T-004 | Deepen the `breakpoint` family into grouped views, richer metadata, and debugger-oriented inspection output. | `swat-api` now projects shared breakpoint summaries/details/groups; `breakpoint list/show/groups` use that model while raw `triggers` stays compatible. |
| T-005 | Add shared help/completion/history/discovery metadata across shell and TUI. | Shared registry search/completion now drives shell `rustyline` completion/history, TUI command-mode tab/history, and `help search <needle>` discovery. |
| T-006 | Add script/runtime wrappers for the first debugger command families on top of `swat-api`. | `swat-script` now wraps stack/source/breakpoint workflows through `swat-api` for both frozen script contexts and live script sessions, without shell-only shortcuts. |
| T-007 | Reconcile Milestone 1 docs, demos, and validation evidence until the milestone exit criteria are materially satisfied. | Milestone 1 closeout note, refreshed command demo coverage, and green focused/full validation now document the command-ecology exit criteria. |
| T-008 | Add grouped breakpoint definitions, reusable predicates, and richer stop-reason modeling. | Named predicate libraries, user-defined breakpoint groups, and typed stop reasons now sit on the shared trigger substrate across `swat-control`, `swat-api`, and `swat-command`. |
| T-009 | Add watchpoints for values, objects, resources, and lifecycle-aware load/time break conditions. | `swat-control` now supports value-change watchpoints plus elapsed-time and lifecycle-gated load predicates over shared value observations, with ADR 0029 documenting the stateful model. |
| T-010 | Expose advanced breakpoint/watchpoint management across shell, TUI, script, and public API. | `swat-api` now projects typed watchpoint specs/details, shell and TUI share live breakpoint/watchpoint management, `swat-script` wraps the same mutation surface, and trigger persistence v4 preserves watchpoints. |
| T-011 | Add typed frame/local/register inspection primitives for modern targets and shared APIs. | `swat-api` now projects typed frame inspections with locals/registers from structured modern-target artifacts, shell and TUI expose `stack locals` / `stack registers`, and `swat-script` wraps the same frame-binding model. |
| T-012 | Add typed patient/handle/resource/object inspection models and debugger-native presentations. | Shared value/resolver/API layers now project typed patient/handle/resource/object models, and shell/TUI/script surfaces expose debugger-native inspection without changing `swat-core`. |
| T-013 | Deepen expression/value/source/resolver traversals for debugger-native workflows rather than only event queries. | Query fields now cover typed target entities and source lines, shared APIs project value history and source-function traversal, and shell/TUI/script surfaces expose the same debugger-native workflows. |
| T-014 | Add package-oriented script loading and shared command metadata integration. | Built-in script packages now autoload over the public API, and shell help/search/completion reuse the same package metadata. |
| T-015 | Migrate the highest-value legacy Tcl command families onto the new runtime and help surface. | Legacy-style stack/source/patient/process/object helpers now ride on the shared runtime, help registry, and TUI surface. |
| T-016 | Implement PC/GEOS VM, symbol, geode, and object format readers or bridges. | `swat-format-pcgeos` now parses PC/GEOS file headers, VM containers, geode imports, and object/symbol source-resource metadata, with repository-backed VM/data fixture tests. |
| T-017 | Model patient/handle/resource/geode/source relationships over real repository artifacts. | `swat-format-pcgeos` now models real manifest and symbol fixtures into typed patient/geode/handle/resource/source relationships, with repository-backed tests and an inspection example. |
| T-018 | Implement `swat-adapter-pcgeos` host-side adapter and protocol bridge. | `swat-adapter-pcgeos` now provides a fixture-first PC/GEOS adapter over real manifests/symbol VMs, and `swat-replay` now replays any boundary-payload event rather than only model/tool kinds. |
| T-019 | Expose PC/GEOS control and inspection through shared APIs, shell, TUI, and demos. | Shared API, shell, and TUI flows now expose PC/GEOS registers, memory snapshots, stack frames, patient/handle/resource/object views, and real source navigation over the fixture adapter. |
| T-020 | Deepen shell and TUI debugger ergonomics, panes, replay views, breakpoint views, history, and completion. | Make long-session workflows practical instead of demo-oriented. |
| T-021 | Final integration, docs, fixtures, demos, clean tree, and reproducible handoff until the full Definition of Done is satisfied. | This is the final closeout gate, not a cosmetic polish task. |
