# MOD-01 source debugging

Choose BUILD → DEBUG in native Studio with a cartridge loaded. Studio compiles
the complete project in debug mode from the same typed IR as a release build
and starts a fresh restricted Node child. Debug builds add resumable statement
boundaries and original-module source metadata; release cartridges do not.
Debugging reads the current cartridge save into the disposable machine, but
debug save commits never write player progress to disk. Loaded replay playback
also does not persist its save commits; ordinary gameplay still does.

## Stepping and input

STEP advances to the next MODL statement boundary, including called functions,
tasks, global initializers and `on start`. A separate boot-complete response
precedes frame zero. CONTINUE repeats source steps; PAUSE stops that sequence.
The native controls currently do not provide step-over, step-out or a separate
run-to-frame button.

The first step of a display frame chooses one four-port/pointer input and holds
it across every subsequent statement pause. Boot steps do not consume a display
frame input. Cartridge time, frame/update counters, raster scanout and PCM output
do not advance while no runtime step is submitted. Partial-frame graphics and
audio remain runtime-local until a complete frame is returned.

The common [system menu](MENU.md) stops both normal frames and debug continuation.
Cartridge actions can be queued only at completed debug boundaries, not during a
suspended handler. Selecting an action preserves whether the debugger was already
continuing; otherwise STEP/CONTINUE executes it. A breakpoint reached by an
in-flight step while the menu opens still requires explicit continuation.

SOURCE accepts up to 32 original-module/line breakpoints, optionally with a
condition. A condition is evaluated at the pre-statement boundary by a restricted,
non-mutating expression interpreter, never JavaScript `eval`. Literals, names,
fields, collection indices, parentheses, `not`, arithmetic, comparisons, `and`
and `or` are supported; calls, assignments and prototype access are rejected.
A condition must return `Bool`; evaluation errors also stop continuation so a
broken condition cannot silently skip a breakpoint. Breakpoints are session-local
line positions, not persistent semantic anchors.

Bundled games qualify breakpoints and source events by cartridge alias as well
as module and line. Identical paths or compiler symbol IDs in different members
remain independent. Locations use a frozen copy of the launched member's source,
not an edited workspace's line numbers. The SOURCE tab shows a read-only excerpt
and OPEN EDITABLE SOURCE selects that member/module and cursor. If the authored
module has changed since launch, the debugger keeps the draft and the read-only
running excerpt instead of navigating to a misleading offset. The system menu's
INSPECT SOURCE stops play and selects the active member, not the last edited one.

## Inspection and memory

SOURCE shows the latest source event and up to four read-only watch expressions.
STATE shows captured globals; TASKS shows captured tasks and call-stack data.
These values update at source boundaries. Compiler inspection is separately
available from TOOLS, including tokens, AST, typed IR, JavaScript and source map.

MEMORY reads 1–256 bytes of the actual hardware bus as hexadecimal rows. WRITE HEX
uses the selected address and accepts up to 256 bytes; the runtime validates
permissions and performs transactional writes. Source pauses are message
boundaries, so no runtime instruction executes concurrently with a memory edit.
There are currently no native byte-watchpoint or decimal-diff controls.

PROFILE shows the latest completed frame's work count and a bounded trace of up
to 4,096 source events. It also lists manually captured snapshots. The AUDIO tab
is currently informational, not an eight-voice debugger inspector. The asset
workbench's separate live preview does provide actual voice/tracker telemetry.

## Snapshots and input recording

SNAPSHOT is available only before boot or at a completed boot/frame boundary,
not inside an unfinished statement-stepped frame. Studio retains up to 120
manual snapshots in the current debug session. RESTORE, or a saved snapshot's
RESTORE BEFORE FRAME control, restores the complete machine/device state,
discards any suspended generator and queued audio, pauses continuation and uses
the runtime's authoritative next-frame cursor. Any later recording entries are
discarded before continuing that branch. The displayed framebuffer refreshes
when the next complete frame returns.

Whole-game snapshots include the active alias, transfer parameter, loaded asset
bank, all committed member saves and input-release ownership. The public replay
cursor continues across cartridge loads; a member's local VM clock restarts.
Restoring a boundary before a target boots does not record boot statement steps
as input frames. Stale source/watch inspection is cleared until a fresh event.

EXPORT → RECORD REPLAY starts a fresh run from the current cartridge/save and
clears loaded playback. Exactly one input is recorded for each completed frame,
including frame zero; boot and individual statement steps are not entries.
SAVE REPLAY writes the bounded public input trace (up to 36,000 frames).
LOAD REPLAY restarts the cartridge using those inputs. An input-only trace does
not embed the initial machine/save: reproduce it with the same source/assets,
seed and initial save. It is not a portable full-session snapshot.

The runtime library also contains a separately tested fingerprinted
`ReplayJournal` for deterministic snapshot/re-execution. Native Studio does not
currently expose its automatic timeline, one-frame rewind or periodic snapshots.
Generator continuations themselves are deliberately not serialized; restoring
always returns to a completed boundary. Outstanding native requests are counted
so an unrelated reply cannot advance a source step ahead of a queued restore.
