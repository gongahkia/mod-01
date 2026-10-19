# MOD-01 source debugging

Choose BUILD → DEBUG in native Studio with a cartridge loaded. Studio compiles the complete project in
debug mode from the same typed IR as a release build and starts a fresh restricted Node child. Debug
builds add resumable statement boundaries and original-module source metadata; release cartridges
contain neither. Debug sessions operate on a copy of the cartridge save, so inspecting or rewinding
cannot silently overwrite player progress.

## Pause and stepping semantics

- The initial cursor is before the first global initializer. `on start` is fully step-able. A
  separate `BOOT COMPLETE` boundary follows it and still precedes frame 0.
- `IN` executes up to the next MODL statement, including statements inside a called function or
  task. `OVER` continues until the next boundary at the same or shallower call depth. `OUT`
  continues until the current routine returns or the frame finishes.
- `FRAME` advances until a whole display frame completes, unless a source breakpoint, condition, or
  memory watchpoint stops it first. `RUN` repeats that operation on the native frame schedule.
- Breakpoints are keyed by original module and line. They persist locally per project. On a later
  debug build, Studio anchors each breakpoint to its trimmed source statement and moves it to the
  nearest matching line; entries whose statement was deleted are discarded.
- A condition is evaluated at the pre-statement boundary. The evaluator is a restricted,
  non-mutating MODL expression interpreter, never JavaScript `eval`. It supports literals, names,
  fields, collection indices, parentheses, `not`, arithmetic, comparisons, `and`, and `or`. Calls,
  assignments, prototype fields, and a non-`Bool` result are rejected.

Input is sampled exactly once when a display frame begins and is held across every pause in that
frame. Cartridge time, frame/update counters, raster scanout, PCM generation, save commits, and
replay-journal advancement do not progress while stopped. Graphics/audio commands already executed
inside the unfinished frame remain runtime-local and are not presented or queued to native audio until
the frame completes. Restoring or rewinding while suspended discards the live continuation and
returns all devices to the last completed boot/frame snapshot.

## Source, state, tasks, and watches

The compiler emits source-map v3 data with every reachable original `.modl` module and maps yielded
statement spans and call-stack frames back through import removal and namespace rewriting. SOURCE
automatically follows the current module. STATE combines live globals and locals using stable symbol
IDs. TASKS shows the live call stack with module/line positions plus serialized task program
counters, waits, and locals.

`ADD` retains up to four read-only watch expressions. The same restricted evaluator is used for
watches and conditions. Compiler Explorer exposes tokens, AST, typed IR, generated JavaScript, and
the source-map JSON for advanced inspection.

## Memory and device watchpoints

MEMO reads 1–64 bytes from the actual current hardware bus, labels the containing region and
permissions, and switches between hexadecimal and decimal display. Changed bytes are marked. A
single byte can be edited only in a debug runtime at a paused runtime message boundary; validation is
transactional and read-only/reserved regions reject the write. Up to eight byte watchpoints stop at
the next statement boundary after the byte changes. `MAN` opens the current hardware reference.

## Replay, audio, and profiling

`-1F`, the timeline, and `RST` restore the nearest retained full device snapshot and replay the exact
four-port/pointer input trace. Every replayed frame is fingerprinted across output, commands, saves,
and runtime state; the first divergence is reported. Continuing from an earlier cursor creates a new
branch. Snapshots are recorded before boot/frame 0 and every 30 completed frames in the bounded
3,600-frame journal.

The native audio control enables output. Paused partial-frame audio is never enqueued.
AUDIO shows all eight deterministic voice states and tracker position. PROFILE aggregates the same
synthetic work charges enforced by the machine; it is cumulative for the current debug session and
is not rewound with the replay cursor.

## Intentional limits

Generator continuations themselves are deliberately not serialized. Rewind restores a completed
boundary and deterministically re-executes inputs instead. Breakpoint anchors use exact trimmed
statement text rather than a semantic tree diff. The profiler currently reports linked-code work
spans, while statement stops and stack frames use original modules. These limits do not change
cartridge execution or captured output.
