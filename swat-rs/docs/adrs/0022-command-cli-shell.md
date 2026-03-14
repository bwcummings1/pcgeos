# ADR 0022: Command CLI Shell

- Status: accepted
- Date: 2026-03-14

## Context

`swat-command` had matured into the first live operator surface, but it still
existed primarily as a library plus tests and examples. That left a gap between
the architecture and actual debugger usage: there was no first-class shell a
developer could run directly.

Legacy Swat mattered partly because it was immediately operable. `swat-rs`
needed that same property.

## Decision

Add a `swat-command` binary entry point.

The CLI now supports:

- `swat-command mock`
- `swat-command local <program> [args...]`
- `swat-command agent <program> [args...]`
- optional `--store <path>` for a durable `FileStore`

The shell reads commands from stdin, supports interactive use and piped batch
scripts, and treats `quit` or `exit` as shell-level termination commands.

## Consequences

Benefits:

- `swat-rs` now has an actual runnable shell instead of only library examples
- the operator surface can be exercised in batch form from tests and scripts
- durable store usage is available from the shell without custom wrapper code

Tradeoffs:

- adapter-selection logic now lives in the binary crate
- shell startup is intentionally simple and does not yet cover all future modes

## Follow-on work

- add richer startup flags for trigger profiles and target environment shaping
- add readline/history/completion if the shell becomes a primary workflow
- decide whether a future TUI should embed or wrap this same command runtime
