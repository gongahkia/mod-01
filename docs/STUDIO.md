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

The sample picker opens bundled cartridges for editing or play. The workspace
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

Saving keeps bounded local recovery revisions. The save manager exports/imports
checksummed cartridge-bound save containers and retains recovery before
replacement. Local saves are keyed by validated cartridge identity.

## Running and debugging

Keyboard port one uses arrows plus Z/X/A/S, Q/W and Enter. Native gamepads fill
four logical ports. Pointer coordinates map to indexed display pixels. Runtime
frames return indexed pixels, stereo PCM and complete committed save bytes.
The host never receives a cartridge-selected filesystem path.
Runtime loads, stops and isolated previews carry private native session generations;
queued replies/commands from an abandoned session cannot update a new cartridge's
frame, audio, debugger or committed save. The child-process JSON protocol is
unchanged. Missing-bundle startup errors remain visible across generation changes.

Debug compilation supports source statement stepping, breakpoint conditions,
watches, memory inspection/editing, state/tasks and snapshot restore. It uses the
same compiler IR and device implementation as ordinary play. A paused debugger
does not advance machine frame time.

The standalone game surface is available through `--play PATH` or OPEN GAME VIEW.
It uses the same execution/input/audio/save devices. PAUSE/Escape suspends frame
submission; RESTART reboots; SOURCE opens the authored modules/assets in Studio.
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
