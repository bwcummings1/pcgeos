# Current Queue Summary

Generated from `PROJECT_COMPLETION_PLAN.md`. That plan remains canonical.

- Queue: `T-001..T-021`
- Progress: `7/21 done`
- Pending: `14`
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
| T-008 | pending | M2 | - | - |
| T-009 | pending | M2 | - | - |
| T-010 | pending | M2 | - | - |
| T-011 | pending | M3 | - | - |
| T-012 | pending | M3 | - | - |
| T-013 | pending | M3 | - | - |
| T-014 | pending | M4 | - | - |
| T-015 | pending | M4 | - | - |
| T-016 | pending | M5 | - | - |
| T-017 | pending | M5 | - | - |
| T-018 | pending | M6 | - | - |
| T-019 | pending | M6 | - | - |
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
| T-008 | Add grouped breakpoint definitions, reusable predicates, and richer stop-reason modeling. | Replace the current trigger-backed feel with a debugger-grade break model. |
| T-009 | Add watchpoints for values, objects, resources, and lifecycle-aware load/time break conditions. | Use the legacy breakpoint Tcl/C stack as reference, not as implementation. |
| T-010 | Expose advanced breakpoint/watchpoint management across shell, TUI, script, and public API. | No adapter-private or UI-private control channels. |
| T-011 | Add typed frame/local/register inspection primitives for modern targets and shared APIs. | Start modern-target-first, then widen toward PC/GEOS-specific entities. |
| T-012 | Add typed patient/handle/resource/object inspection models and debugger-native presentations. | Keep `swat-core` target-neutral while enriching shared value/resolver layers. |
| T-013 | Deepen expression/value/source/resolver traversals for debugger-native workflows rather than only event queries. | Preserve operator-friendly formatting across shell and TUI. |
| T-014 | Add package-oriented script loading and shared command metadata integration. | Recover conceptual autoload parity without recreating Tcl chaos. |
| T-015 | Migrate the highest-value legacy Tcl command families onto the new runtime and help surface. | Focus on stack/patient/process/object/source helpers first. |
| T-016 | Implement PC/GEOS VM, symbol, geode, and object format readers or bridges. | Fixture-backed first if live target automation is fragile. |
| T-017 | Model patient/handle/resource/geode/source relationships over real repository artifacts. | Use real repository fixtures rather than synthetic placeholders. |
| T-018 | Implement `swat-adapter-pcgeos` host-side adapter and protocol bridge. | Support replay-fixture mode first if necessary, then live target/emulator paths. |
| T-019 | Expose PC/GEOS control and inspection through shared APIs, shell, TUI, and demos. | Cover registers, memory, stack, patient, handle, and source flows. |
| T-020 | Deepen shell and TUI debugger ergonomics, panes, replay views, breakpoint views, history, and completion. | Make long-session workflows practical instead of demo-oriented. |
| T-021 | Final integration, docs, fixtures, demos, clean tree, and reproducible handoff until the full Definition of Done is satisfied. | This is the final closeout gate, not a cosmetic polish task. |
