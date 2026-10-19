# Native Studio

Studio is a native Rust/egui desktop application. It has no webview, browser
frontend, PWA, JavaScript UI or network server. Rust owns source, files, saves,
input and presentation; a disposable Node child executes the shared console core.

```sh
pnpm install --frozen-lockfile
pnpm native
# Or after building:
target/debug/mod01-studio PATH
target/debug/mod01-studio --play PATH
```

The [offline library](LIBRARY.md) opens bundled and user cartridges for editing or
play, with cover art, search, labels, favourites and controller navigation. The workspace
contains code, layered sprites, structured assets, player and manifest views.
PROJECT opens/creates/saves projects and cartridges and exposes recovery.
BUILD checks, runs, debugs and packs. TOOLS provides compiler/source inspection
and save management. Its [MODL playground](PLAYGROUND.md) runs expressions and
drawing in an isolated production console, with optional copied project context.
It is available even before opening a cartridge. EXPORT offers native games and cartridge PNGs.
There is no browser export menu.

## Source, assets and recovery

The source editor uses the compiler lexer for highlighting. Module selection
keeps unsaved text synchronized into the project. Workcarts retain authored
modules, assets, auxiliary files, generation recipes and tests; source remains
inspectable when distributed as a packed cartridge or native game.
Find/replace, line/routine navigation, block shortcuts, undo/redo, compiler API
completion, cross-module definitions and clickable diagnostics are available in
CODE. GRAPH shows resolved compiler call/state/asset relationships. See
[source editor](EDITOR.md) for shortcuts and precise behaviour.

The layered RGBA sprite editor supports animation frames, layers, selection,
drawing/fill tools, PNG import/export, palette picking and undo. Structured tools
edit maps/tile atlases, bitmap fonts, synthesizer patches and tracker patterns;
raw asset text remains visible.

Asset JSON/source, sprite/atlas PNG and WAV save dialogs suggest namespaced
`asset-NAME.EXT` filenames rather than raw device-like asset names or source
basenames. The readable ASCII label is sanitized and capped at 64 characters;
source extensions are kept when safe, otherwise `.json` is suggested. These are
editable suggestions, not unique IDs: inspect the destination before replacing
an existing file. Asset IDs, declared source paths and exported contents do not
change.

Sound controls use integer MIDI/frame inputs, with duration 1–3600 and envelope/
vibrato periods 0–3600. Click to type or drag to adjust; fractional integer input
is rejected. Floating fields retain full precision when focused without an edit.
Opening a patch never clamps/rebuilds its source, including integral JSON floats
and malformed fields; invalid fields remain available in raw JSON. Pitch values
use the shared core's frequency validation rather than narrower editor-only
limits. Choosing pulse explicitly supplies a default duty only if needed;
choosing wavetable explicitly supplies a four-sample table only if needed.
Existing valid tables survive waveform switches. Wavetable samples and bounded
4–32 sample add/remove controls participate in ordinary per-asset undo/redo;
unrelated source annotations survive edits.

The music workbench shows all eight channels together, with MIDI note, patch and
optional cell volume. Click the grid to focus it: Z/S/X/D/C/V/G/B/H/N/J/M enter
chromatic notes from the selected octave; comma/L/period/semicolon/slash extend
the range. STEP controls row advance (zero repeats the same row). AUDITION ONLY
plays notes without editing. Patch choices come from declared sound assets.
Arrows, Tab/Shift-Tab, Home/End and Page Up/Down navigate; Shift+arrows selects a
rectangular block. Delete/Backspace clears; Insert inserts an eight-cell row.
Ctrl/Cmd+A/C/X/V selects/copies/cuts/pastes a block, clipping at the pattern edge.
Clipboard operations use the tracker's local block buffer, not arbitrary OS text.
Ctrl/Cmd+Z/Y (or Shift+Z) undo/redo whole edits, including sequence operations.
Enter previews the pattern from the cursor; Ctrl/Cmd+Enter starts the song at the
selected order; Escape stops preview. These keys only act while the grid owns focus.

SEQUENCE / BLOCK TOOLS exposes row deletion, block operations, atomic semitone
transpose and the order list. Pattern controls create or duplicate patterns,
rename every order reference together, and refuse deletion while referenced.
Order entries can be inserted/replaced/removed/reordered; the final entry cannot
be removed. Patterns retain 1–256 rows; tempo supports the full 1–240 frames/row.
Existing optional fields/annotations survive unrelated edits and duplicate/copy
operations. Invalid shapes remain recoverable as raw JSON without silent repair.

Structured assets and layered sprites keep separate editor sessions when you
switch assets, including undo/redo and selection/tool state. Synchronization and
ordinary saves do not erase that history; importing a structured asset is one
undoable replacement. Each editor retains up to 32 edits. Inactive sessions use
a 64-session / conservative 64 MiB allocation budget; least-recently-used history
expires at the limit, never the asset stored in project memory. A session too
large for that budget is not cached. History is session-local, not an on-disk
format: opening/recovering/creating another cartridge clears it. Changed asset
kind/path/bytes or layered source invalidate cached state. Raw JSON text widgets
also use separate asset/project identities so their text undo cannot cross assets.

ASSET BANK shows the shared Rust validator's live visual-byte/allocation-slot
usage in Assets and Sprites, including unsaved JSON, structured edits and sprite
frame changes. It refreshes at most four times per second; REFRESH requests the
next check immediately. Reports never apply buffers or change project dirty state.
Invalid banks are visibly marked, with bounded, scrollable file/JSON-field
diagnostics; click an asset name to select its editor. Any diagnostic means the
bank cannot load, even if its partial usage is low. CLI `mod01 assets PATH` exposes
the same report, and `check`/pack/packed import enforce the same rules.

Sound/music previews run the production synth in a separate, disposable native
runtime: LIVE SONG starts the order list; PATTERN FROM CURSOR plays a non-looping
copy of the selected pattern; AUDITION CELL/PREVIEW PATCH triggers the actual
patch and eight-voice allocator. Frames are paced at 60 Hz, not per UI repaint.
The collapsible LIVE TELEMETRY reports real order/row/tick, voice notes and stereo
PCM peaks; the grid highlights playback using the frozen preview asset, not later
source edits. Channel headings remain visible during vertical scrolling. Unsaved
edits are copied into previews, never written to the cartridge or save memory.
Restart playback to hear later edits; successive auditions can overlap while
their patch bank is unchanged. STOP AUDIO, leaving Assets, opening another
project or running the cartridge stops and discards queued preview audio.
Without an audio device the same preview runs silently. EXPORT WAV is a separate
offline render and never starts playback.

The `AUD`/`SILENT` status indicator reports native device availability; hover it
for the latest device notice. The current
[CPAL error classifications](https://docs.rs/cpal/0.18.2/cpal/enum.ErrorKind.html)
keep underruns, automatically rerouted devices and realtime-scheduling notices
nonfatal. These notices are coalesced rather than logged from the audio callback.
An unusable stream is released on the UI thread and playback/preview continues
silently without stopping the VM, changing saves or rewinding replay. Restore the
device and restart Studio/player to re-open output; there is no automatic retry
loop. Native synth telemetry and offline WAV export remain available without a
working output device. This does not replace physical-device/audio-quality QA.

Saving keeps bounded local recovery revisions. The save manager exports/imports
checksummed cartridge-bound save containers and retains recovery before
replacement. Local saves are keyed by validated cartridge identity.

On disk, native saves use the shared portable stem under the application data
directory: `saves/cart-ID.bin` and `cart-ID.schema`. Dots in an ID become
underscores only in filenames; the ID inside the game/save container is unchanged.
Recovery lives in `saves/recovery/cart-ID.saves/TIMESTAMP.m01save`. Pruning retains
ten valid backups for that exact ID, never a different ID with a shared name
prefix. The newly created backup is retained even if the clock moves backwards;
foreign, malformed, oversized and linked files are not pruned. Recovery creation
uses [atomic create-new](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new)
so timestamp collisions cannot overwrite an existing file.

There is no old-path fallback or automatic data migration. Existing unprefixed
development `.bin`/`.schema` files and flat recovery files are left untouched.
To retain an earlier development save, export/import its current `.m01save`
container, or—while Studio and players are stopped—copy the raw `.bin` and any
matching `.schema` sidecar to the new names. Keep the originals until verified.

## Running and debugging

Keyboard port one uses arrows plus Z/X/A/S, Q/W and Enter. Native gamepads fill
four logical ports. Pointer coordinates map to indexed display pixels. Runtime
frames return indexed pixels, stereo PCM and complete committed save bytes.
Keyboard events are sampled on UI passes independently of runtime replies:
short taps remain latched until one display-frame input accepts them. Source
boot/partial steps do not consume later frame taps. Pause, session/restore,
text editing and focus loss discard pending taps and block held keys until
release; Ctrl/Cmd/Alt shortcuts do not become gameplay. Escape is host-only.
The host never receives a cartridge-selected filesystem path.
Runtime loads, stops and isolated previews carry private native session generations;
queued replies/commands from an abandoned session cannot update a new cartridge's
frame, audio, debugger or committed save. The child-process JSON protocol is
unchanged. Missing-bundle startup errors remain visible across generation changes.

Debug compilation supports source statement stepping, breakpoint conditions,
watches, memory inspection/editing, state/tasks and snapshot restore. It uses the
same compiler IR and device implementation as ordinary play. A paused debugger
does not advance machine frame time.

RECORD REPLAY starts a fresh run (using the current save), clears loaded playback
and records one input per completed display frame, including frame zero exactly
once. Source stepping locks that input for the entire frame; boot and individual
statements do not add trace entries. Reproducing an input-only trace requires the
same cartridge, seed and initial save. SNAPSHOT is available only before boot or
at completed boot/frame boundaries. RESTORE uses the runtime's next-frame cursor,
clears the suspended continuation and trims any later recorded inputs. Native
request tracking waits for all outstanding replies before advancing debug steps.

The standalone game surface is available through `--play PATH` or OPEN GAME VIEW.
It uses the same execution/input/audio/save devices. Studio and that game surface
share the native system menu: MENU/PAUSE, Escape, controller Select or touch MENU
opens it without advancing machine time. Up/down and Z/Enter (controller A/Start)
select; X/controller B/Escape resumes. Resume/restart/source inspection and the
cartridge's typed actions use this common path. RESTART preserves debug mode;
SOURCE stops execution and opens authored modules/assets unchanged in Studio.
`menu_item` and `on menu(action: Int)` are documented in [MENU.md](MENU.md), with
the runnable `examples/menu-actions` demonstration. Actions use one neutral input
frame, are recorded/replayed and cannot be selected during replay or inside a
partial debug frame. Gamepad taps are latched across response latency and other
controllers disconnecting do not renumber surviving logical ports. Software
events/model tests do not claim physical-controller QA.
[native games](NATIVE_GAMES.md) documents package layout and bridge verification.

TOUCH enables an optional native port-one D-pad, A/B/X/Y, L/R and Start pad in
that game surface. It uses independent device/finger IDs (up to 32 contacts),
including diagonal D-pad corners. Fingers must start inside their controller
zone; moving out releases its held bits and moving back can press again. An
ended short tap remains latched until one input submission; cancellation drops
an undelivered tap. Releasing one finger never releases another finger's buttons.
Mouse emulation is available for desktop QA, with touch-generated mouse aliases
ignored. Owned controller gestures never inject game-pointer clicks.

MENU pauses/resumes the native player and is not delivered as a game button.
Pause, focus loss, hide/disable, project stop/reload, snapshot restore or layout
change clears controller ownership; lift and touch again after interruption.
Paused/debugger/replay playback controls cannot queue live gameplay input.
The overlay merges into the ordinary controller frame before replay recording,
without a separate simulation/input format. At 900×600 it retains a 2× game view.
The renderer does not submit additional VM frames while paused; an already
submitted frame may finish, but its PCM is discarded while paused.

## Presentation and verification

BigBlue Terminal Plus and Kenney pixel cursors are embedded in the native UI;
their attribution/license files ship with native distributions. Presentation
does not change the console's indexed pixels or replay state.

Run `make check` for the repository gate. Display-free native verification is
`mod01-studio --verify PATH --frames 120`. It verifies process/device execution,
not graphical layout, multi-touch, physical controller or audible output quality.
Native visual/interaction QA is separately required by the
[implementation ledger](NATIVE_IMPLEMENTATION.md).
