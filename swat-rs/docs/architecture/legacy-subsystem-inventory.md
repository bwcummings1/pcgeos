# Legacy Subsystem Inventory

This document is the detailed technical index for the original Swat material
that `swat-rs` is designed against. It exists to keep the Rust implementation
contextually coherent with the real system instead of drifting into a generic
debugger architecture.

The legacy implementation is not being ported file-for-file, but each major
semantic area below has a clear Rust landing zone and an implementation phase.

## 1. Runtime spine, dispatch, and top-level control

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/README`
- `/home/ubuntu/pcgeos/Tools/swat/swat.c`
- `/home/ubuntu/pcgeos/Tools/swat/rpc.c`
- `/home/ubuntu/pcgeos/Tools/swat/event.c`
- `/home/ubuntu/pcgeos/Tools/swat/cmd.c`
- `/home/ubuntu/pcgeos/Tools/swat/ui.c`
- `/home/ubuntu/pcgeos/Tools/swat/shell.c`

Technical facts to preserve:

- Swat execution centers on the dispatch loop in `rpc.c`, not on the UI.
- The top-level command loop waits on RPC, timers, and local input together.
- Event dispatch is internal infrastructure, not just a logging surface.
- The Tcl command environment sits on top of that event loop.

Rust landing zone:

- `swat-core`
- `swat-protocol`
- `swat-session`
- `swat-replay`
- later `swat-api`

## 2. Event model and programmable hooks

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/event.c`
- `/home/ubuntu/pcgeos/Tools/swat/event.h`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/event.tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/swat.tcl`

Technical facts to preserve:

- Legacy Swat exposes debugger events both in C and at the scripting layer.
- The event system supports registration, deletion, numeric event identifiers,
  and Tcl-level callbacks.
- `FULLSTOP` is a key semantic stop event, not just a machine trap.

Rust landing zone:

- `swat-core` event taxonomy
- future `swat-control` trigger engine
- future `swat-script`

## 3. Target/session identity and entity model

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/patient.c`
- `/home/ubuntu/pcgeos/Tools/swat/patient.h`
- `/home/ubuntu/pcgeos/Tools/swat/handle.c`
- `/home/ubuntu/pcgeos/Tools/swat/handle.h`
- `/home/ubuntu/pcgeos/Tools/swat/geos.h`

Technical facts to preserve:

- `Patient` is a named debug target/module identity, not merely a process id.
- `Handle` abstracts movable memory/resources and supports interest callbacks.
- Handle and patient tracking are how Swat avoids leaking raw kernel details
  throughout the debugger.

Rust landing zone:

- `swat-core` target and identity primitives
- `swat-session`
- future `swat-resolver`
- future `swat-adapter-pcgeos`

## 4. Machine-dependent adapter logic

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/ibm.c`
- `/home/ubuntu/pcgeos/Tools/swat/ibm86.c`
- `/home/ubuntu/pcgeos/Tools/swat/ibmCmd.c`
- `/home/ubuntu/pcgeos/Tools/swat/ibmCache.c`
- `/home/ubuntu/pcgeos/Tools/swat/i86Opc.c`
- `/home/ubuntu/pcgeos/Tools/swat/ibmXms.c`
- `/home/ubuntu/pcgeos/Tools/swat/mouse.c`

Technical facts to preserve:

- Machine-dependent logic belongs behind a boundary.
- Instruction decoding, register mapping, memory behavior, and target-specific
  state caches are adapter concerns.
- The core debugger should ask what the target can do, not assume one machine
  model.

Rust landing zone:

- `swat-core::TargetAdapter`
- later `swat-adapter-local`
- later `swat-adapter-python`
- later `swat-adapter-agent`
- later `swat-adapter-pcgeos`

## 5. Breakpoints, stepping, and control transitions

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/break.c`
- `/home/ubuntu/pcgeos/Tools/swat/break.h`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/bptutils.tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/brkload.tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/hwbrk.tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/tbrk.tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/timebrk.tcl`

Technical facts to preserve:

- Breakpoints in Swat are richer than address traps; they include conditional,
  load-aware, and time-oriented behavior.
- Control flow is event-driven and tightly coupled to stop reasons.
- The scripting layer contributes real control semantics.

Rust landing zone:

- future `swat-control`
- future `swat-expr`
- future `swat-script`

## 6. Symbols, types, expressions, values, and source mapping

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/sym.c`
- `/home/ubuntu/pcgeos/Tools/swat/type.c`
- `/home/ubuntu/pcgeos/Tools/swat/expr.c`
- `/home/ubuntu/pcgeos/Tools/swat/expr.y`
- `/home/ubuntu/pcgeos/Tools/swat/value.c`
- `/home/ubuntu/pcgeos/Tools/swat/var.c`
- `/home/ubuntu/pcgeos/Tools/swat/src.c`
- `/home/ubuntu/pcgeos/Tools/swat/file.c`
- `/home/ubuntu/pcgeos/Tools/swat/vmsym.h`

Technical facts to preserve:

- Swat derives power from typed symbolic inspection, not from raw memory dumps.
- Expression parsing is a major subsystem, not a UI convenience.
- Source mapping, symbol lookup, and typed values must work together.

Rust landing zone:

- `swat-value`
- `swat-schema`
- `swat-resolver`
- `swat-source`
- `swat-expr`

## 7. Protocol and resident stub architecture

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/rpc.h`
- `/home/ubuntu/pcgeos/Tools/swat/Doc/stub.ms`
- `/home/ubuntu/pcgeos/Tools/swat/Doc/rpc.g`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/main.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/kernel.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/rpc.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/break.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/cbreak.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/tbreak.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/timebreak.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/intelbpt.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/com.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/netware.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/wincom.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub32/main.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub32/rpc.asm`

Technical facts to preserve:

- The original protocol is a host/target split, not an in-process debugger.
- The resident stub owns stop-state capture, resume/step behavior, interrupt
  interception, block/thread state notifications, and memory/register access.
- Protocol design must account for call/reply semantics, stop reasons, and
  target-originated events.

Rust landing zone:

- `swat-protocol`
- `swat-session`
- `swat-replay`
- later target adapters

## 8. Script runtime and command environment

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/tclDebug.c`
- `/home/ubuntu/pcgeos/Tools/swat/tcl/README`
- `/home/ubuntu/pcgeos/Tools/swat/tcl/tcl.c`
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
- `/home/ubuntu/pcgeos/Tools/swat/Doc/cmds.ms`

Technical facts to preserve:

- Much of Swat's operator power lives in script-space, not only in C.
- Autoloading, command help, breakpoint utilities, object/process helpers, and
  source navigation are all part of the debugger surface.
- The new script/runtime boundary must preserve power without letting the
  scripting layer become the architecture.

Rust landing zone:

- future `swat-api`
- future `swat-script`
- future `swat-ui-tui`

## 9. UI and operator interaction layers

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/ui.c`
- `/home/ubuntu/pcgeos/Tools/swat/help.c`
- `/home/ubuntu/pcgeos/Tools/swat/curses.c`
- `/home/ubuntu/pcgeos/Tools/swat/curses/*`
- `/home/ubuntu/pcgeos/Tools/swat/ntcurses/*`
- `/home/ubuntu/pcgeos/Tools/swat/x11/*`
- `/home/ubuntu/pcgeos/Tools/swat/hist/*`

Technical facts to preserve:

- UI is an operator surface layered on the debugger core, not the control
  center of the system.
- Input handling still routes through the dispatch loop.
- Help/history/completion are operational features, not polish.

Rust landing zone:

- future `swat-ui-tui`
- future IDE or web clients on `swat-api`

## 10. Transport implementations

Primary references:

- `/home/ubuntu/pcgeos/Tools/swat/serial.asm`
- `/home/ubuntu/pcgeos/Tools/swat/netware.c`
- `/home/ubuntu/pcgeos/Tools/swat/win32.md/ntserial.c`
- `/home/ubuntu/pcgeos/Tools/swat/win32.md/npipe.c`
- `/home/ubuntu/pcgeos/Tools/swat/rpc.h`

Technical facts to preserve:

- Transport is pluggable beneath the protocol.
- The host/target split should not be tied to a single medium.
- Retries, timeouts, and backpressure are part of the substrate, not an afterthought.

Rust landing zone:

- `swat-protocol`
- later adapter transports

## 11. External format and runtime dependencies outside `Tools/swat`

Primary references:

- `/home/ubuntu/pcgeos/Tools/include/objfmt.h`
- `/home/ubuntu/pcgeos/Tools/include/geode.h`
- `/home/ubuntu/pcgeos/Tools/include/lmem.h`
- `/home/ubuntu/pcgeos/Tools/include/os90.h`
- `/home/ubuntu/pcgeos/Tools/utils/vm.h`
- `/home/ubuntu/pcgeos/Tools/glue/vm.c`
- `/home/ubuntu/pcgeos/Tools/utils/objSwap.c`
- `/home/ubuntu/pcgeos/Tools/utils/fileUtil.c`
- `/home/ubuntu/pcgeos/Tools/utils/sttab.c`
- `/home/ubuntu/pcgeos/Tools/pmake/lib/lst/lst.h`

Technical facts to preserve:

- Symbol reading and source mapping rely on repository-wide object and VM
  formats, not only files under `Tools/swat`.
- The eventual PC/GEOS adapter and symbol reader must use these as reference
  material even though they are not Phase 1 dependencies.

Rust landing zone:

- later `swat-adapter-pcgeos`
- later `swat-resolver`
- later `swat-source`

## Phase alignment summary

- Phase 0: semantic model, replay boundaries, adapter contract, crate map.
- Phase 1: `swat-core`, `swat-protocol`, `swat-store`, `swat-replay`,
  `swat-session`, `swat-adapter-mock`.
- Phase 2: first live adapter for a modern target.
- Phase 3: typed values, schemas, resolver, source mapping, expressions,
  semantic control.
- Phase 4: API, scripting, and TUI.

This inventory should be updated whenever a new `swat-rs` crate is added or a
legacy Swat subsystem begins active implementation.
