# Legacy Reference Map

This document maps the original Swat technical material in the PC/GEOS tree to
the planned `swat-rs` crates and implementation choices. The goal is not to
port these files directly; it is to preserve contextual coherence while the new
system is implemented from first principles.

## Runtime spine and control loop

Reference material:

- `/home/ubuntu/pcgeos/Tools/swat/README`
- `/home/ubuntu/pcgeos/Tools/swat/swat.c`
- `/home/ubuntu/pcgeos/Tools/swat/rpc.c`
- `/home/ubuntu/pcgeos/Tools/swat/event.c`
- `/home/ubuntu/pcgeos/Tools/swat/break.c`
- `/home/ubuntu/pcgeos/Tools/swat/ui.c`

Modern mapping:

- `swat-core`
- `swat-protocol`
- `swat-session`
- `swat-replay`
- `swat-store`

Design takeaway:

The old system was event-driven and transport-centered, with the debugger loop
organized around wait/dispatch rather than a UI main loop. `swat-rs` keeps that
ordering by making the event model and session protocol the substrate before
building frontends.

See also:

- `docs/architecture/legacy-subsystem-inventory.md`

## Typed symbolic inspection

Reference material:

- `/home/ubuntu/pcgeos/Tools/swat/sym.c`
- `/home/ubuntu/pcgeos/Tools/swat/type.c`
- `/home/ubuntu/pcgeos/Tools/swat/expr.c`
- `/home/ubuntu/pcgeos/Tools/swat/value.c`
- `/home/ubuntu/pcgeos/Tools/swat/src.c`
- `/home/ubuntu/pcgeos/Tools/swat/file.c`

Modern mapping:

- future `swat-schema`
- future `swat-resolver`
- future `swat-source`
- future `swat-expr`
- future `swat-value`

Design takeaway:

Swat was powerful because it resolved raw machine state into meaningful values,
locations, and structures. The modern system must do the same for AI runtimes,
tool graphs, workflow state, and source artifacts.

## Target adapter boundary

Reference material:

- `/home/ubuntu/pcgeos/Tools/swat/ibm.c`
- `/home/ubuntu/pcgeos/Tools/swat/ibm86.c`
- `/home/ubuntu/pcgeos/Tools/swat/handle.c`
- `/home/ubuntu/pcgeos/Tools/swat/geos.h`

Modern mapping:

- `swat-core::TargetAdapter`
- `swat-session`
- adapters such as `swat-adapter-mock` and later `swat-adapter-local`,
  `swat-adapter-python`, `swat-adapter-agent`, `swat-adapter-pcgeos`

Design takeaway:

GEOS-specific and 8086-specific logic belongs in adapters, not in the core.
The core asks what a target can do through capabilities instead of assuming a
single machine model.

## Host scripting and programmable debugger surface

Reference material:

- `/home/ubuntu/pcgeos/Tools/swat/tclDebug.c`
- `/home/ubuntu/pcgeos/Tools/swat/tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/swat.tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/toplevel.tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/autoload.tcl`
- `/home/ubuntu/pcgeos/Tools/swat/lib.new/help.tcl`

Modern mapping:

- future `swat-script`
- future `swat-api`
- future `swat-ui-tui`

Design takeaway:

The new system must preserve programmability, but the script engine should sit
on top of the public API rather than becoming the architecture itself.

## Wire protocol and stub model

Reference material:

- `/home/ubuntu/pcgeos/Tools/swat/rpc.h`
- `/home/ubuntu/pcgeos/Tools/swat/Doc/stub.ms`
- `/home/ubuntu/pcgeos/Tools/swat/Doc/rpc.g`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/rpc.asm`
- `/home/ubuntu/pcgeos/Tools/swat/Stub/kernel.asm`

Modern mapping:

- `swat-protocol`
- `swat-replay`

Design takeaway:

The original Swat protocol was explicit about call/reply, stop reasons,
boundary crossing, and the split between host and target. `swat-rs` keeps the
host/target split but modernizes it around event envelopes, control actions,
replay directives, and a length-prefixed JSON wire frame in `swat-protocol`.

## Symbol and artifact formats

Reference material:

- `/home/ubuntu/pcgeos/Tools/include/objfmt.h`
- `/home/ubuntu/pcgeos/Tools/glue/vm.c`
- `/home/ubuntu/pcgeos/Tools/utils/vm.h`
- `/home/ubuntu/pcgeos/Tools/utils/objSwap.c`

Modern mapping:

- future `swat-adapter-pcgeos`
- future `swat-source`
- future `swat-resolver`

Design takeaway:

These files are reference material for the eventual PC/GEOS adapter and symbol
reader. They are not Phase 1 implementation dependencies.
