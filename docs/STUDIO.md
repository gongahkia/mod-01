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
raw asset text remains visible. The full tracker enhancements are
tracked in the acceptance ledger rather than claimed complete here.

Sound/music previews run the production synth in a separate, disposable native
runtime: LIVE SONG starts the order list; PATTERN FROM CURSOR plays a non-looping
copy of the selected pattern; AUDITION CELL/PREVIEW PATCH triggers the actual
patch and eight-voice allocator. Frames are paced at 60 Hz, not per UI repaint.
Telemetry reports real order/row/tick, voice notes and stereo PCM peaks. Unsaved
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

Debug compilation supports source statement stepping, breakpoint conditions,
watches, memory inspection/editing, state/tasks and snapshot restore. It uses the
same compiler IR and device implementation as ordinary play. A paused debugger
does not advance machine frame time.

The standalone game surface is available through `--play PATH` or OPEN GAME VIEW.
It uses the same execution/input/audio/save devices. PAUSE/Escape suspends frame
submission; RESTART reboots; SOURCE opens the authored modules/assets in Studio.
[native games](NATIVE_GAMES.md) documents package layout and bridge verification.

## Presentation and verification

BigBlue Terminal Plus and Kenney pixel cursors are embedded in the native UI;
their attribution/license files ship with native distributions. Presentation
does not change the console's indexed pixels or replay state.

Run `make check` for the repository gate. Display-free native verification is
`mod01-studio --verify PATH --frames 120`. It verifies process/device execution,
not graphical layout, multi-touch, physical controller or audible output quality.
Native visual/interaction QA is separately required by the
[implementation ledger](NATIVE_IMPLEMENTATION.md).
