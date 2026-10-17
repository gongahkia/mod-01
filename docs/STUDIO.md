# Studio and external workflow

## Native desktop Studio

Run `pnpm native` during development or `pnpm native:package` to make a distributable folder for
the current OS. The desktop interface is Rust/egui and contains no webview. It opens a project
directory or canonical `.m01c`, can save an imported cartridge as a new project, edits MODL modules
and the manifest, checks and packs with `modl-core`, and plays through the shared console runtime.
Keyboard, pointer, four gamepad ports, PCM audio, and cartridge save commits use native host APIs.
The bundled Node child has restricted filesystem/process permissions and a five-second response
limit. Debug mode offers source-step events and snapshot restore. Other declared assets are editable
as their source-visible JSON.

The native sprite editor draws on RGBA layers, supports rename/reorder/visibility/locking,
brushes, fill, pick, selection recolour/erase, frames with onion skinning, zoom/pan, undo/redo,
PNG import/export, and 256-colour quantization. New sprites start at 64×64, can reach 128×128, and
save both a runtime `.m01g` file and an editable `editor/sprites/*.m01paint` document. Hardware
Revision 2 cartridges retain the layered source when packed and unpacked. Colour index zero is
the sprite transparency key; the zero swatch selects the eraser. Layer pixels are base64 RGBA in
the source-visible JSON document, and the editor also reads earlier numeric-array documents.

The browser Studio below remains available for its visual map/font/audio editors, browser replay
capture, and full source debugger while the native interface covers the core project and sprite
workflow.

The browser studio is local-only. Its project repository stores the current project, ten bounded
pre-save recovery revisions, settings, and capability-scoped cartridge saves in IndexedDB. A
cartridge worker receives only a validated copy of its own integer save values; it never receives
the repository, another cartridge ID, or an IndexedDB handle.

The production app boots directly into the monitor shell. `new`, `dir`, `load`, `save`, `recover`,
`import`, `edit`, `run`, `debug`, `pack`, `cart`, `export`, `share`, `inspect`, `info`, `help`, `clear`/`cls`, and `reboot`
operate on real project/compiler/runtime paths. `import` validates an untrusted `.m01c`, reconstructs
its editable project, and also accepts a `.m01c.png` only after validating its bounded PNG chunks and
embedded canonical cartridge. It preserves the previous same-ID revision for recovery. `inspect` displays the
canonical packed metadata and all original source modules. `export` downloads one offline HTML
player with its own visible source inspector. `cart` downloads the MOD-01 320x240 cartridge-object
PNG with title/author/year/player/control identity and the byte-exact `.m01c` payload.

The monitor prints a fixed catalog of the eight bundled cartridges at boot. There is no graphical
shelf or click-to-launch path: players launch a cartridge with `run <id>`, and `dir` prints every
current local/imported project. The remaining project operations are terminal commands: `inspect <id>`,
`copy <id>`, `name <id> <title>`, `star <id>`, `save <id>`, `pack <id>`, and `remove <id>`. `remove <id>`
requires the exact command a second time; running it for a bin item restores the cartridge. Bin
restore retains the exact project revision, recovery history, and isolated save. All records are
IndexedDB-local and survive offline reload; there is no account, sync, gallery, rating, or telemetry path.
The selected cart's `SAVE` service displays stable cartridge identity and schema, exports/imports a
bounded `.m01save` with checksum validation, and requires a second action before reset or delete.
Every replacement retains a local recovery envelope. Raw alpha save bytes migrate to schema zero
without deleting the original; application code can explicitly migrate to a higher schema.
The source editor has MODL highlighting, live compiler diagnostics, completion, symbol navigation,
canonical formatting, explicit save, run, and external-revision reload controls.
Edits debounce to a 750 ms autosave and pass through a revision check before writing; an externally
newer revision stops the save and surfaces F6 reload instead of knowingly overwriting it. Explicit
F3 save and leaving the editor flush the same serialized path. Every successful write retains the
previous revision in the ten-entry recovery ring.
`folder` uses the File System Access API only after an explicit read/write directory grant. It
preflights modification times for every file before any write. F6 pulls a conflicting folder
revision; F7 is the explicit overwrite action. The handle remains session-local. Browsers without
that API continue to use import/download and IndexedDB with no reduced cartridge compatibility.
Running a project uses the Rust compiler WebAssembly bridge, a dedicated worker, indexed WebGL
output, four-port browser input, frame/work status, and isolated save flushing. Shift+Escape returns
from a cartridge to the shell. Player capture writes deterministic native or 2x-4x nearest PNG,
bounded five-second 30 fps GIF sampled from the 60 Hz indexed stream, and revision-1 `.m01rec` input.
The same player validates an imported `.m01rec`, restarts from frame zero, and applies its four-port
and pointer stream without mixing live input. GIF and replay histories are independently bounded.

`project`, `sprite`, `map`, `palette`, `font`, `sfx`, and `music` open cartridge settings and source-visible
asset editors in the same 240x144 display. The sprite tool provides frames, onion skinning,
selection transforms, palette painting, undo/redo, and capacity feedback. The map tool provides
every declared tileset, ordered/visible layers, tile flags, region select/copy/stamp/move, fill,
transactional resize, undo/redo, and whole-project shared-capacity feedback. Visibility is an editor
view aid; revision-1 map files preserve runtime layer order and tileset references without adding
host-only fields. Old one-tileset maps open directly and gain no incompatible wrapper.
Palette/raster editing covers all 144 lines as sparse keyframes with enable/disable, exact cost,
default or per-row remaps, signed scroll, copy/paste/fill/range interpolation, undo/redo, and a live
scanout preview. It exposes only the scroll/remap state present in the Hardware Revision 1 raster
table. The font tool edits canonical byte-code glyph maps, baseline and advances, required fallback,
selection transforms and preview text while leaving `print`'s system font unchanged. Sound patches and eight-channel tracker
patterns can be previewed after a browser audio gesture. The tracker edits named patterns and an
ordered playback sequence, preserves existing patch references, loops on request, and provides
bounded note/pattern undo/redo. The shell, editors, controls, and cartridges use the same original
MOD-01 glyph design; the Studio font is generated locally from the runtime's glyph matrix.
`manual`, `man <topic>`, and `help <symbol>` search built-in syntax/API/hardware/diagnostic help;
F1 in the source editor opens help for the symbol at the cursor. `load modl-tutorial` starts the
ordinary public-API FIRST SIGNAL lesson. The compact runnable API tour and genre starters are
indexed in `examples/README.md` and `templates/README.md`. `explore` exposes tokens, AST, symbols,
typed IR, JavaScript, source maps, diagnostics, and exact size accounting.
`debug` opens source breakpoints and trace stepping, state/task/watch inspection,
synthetic-work profiling, hardware inspectors, and deterministic frame rewind. Its precise
source stepping and pause semantics are documented in [DEBUGGING.md](DEBUGGING.md). Escape
returns from a creation tool; F3 saves asset changes.
Creation-tool changes show a dirty state and autosave through the same 750 ms serialized path as an
explicit F3 save. A newer stored revision rejects the write and asks the author to reopen instead
of overwriting another tab.

## Native commands

From the repository, use `cargo run --package mod01-cli --` in place of an installed `mod01`:

```sh
mod01 new my-game --title "MY GAME"
mod01 check my-game
mod01 build my-game
mod01 test my-game
mod01 watch my-game
mod01 pack my-game
mod01 export html my-game
mod01 export png my-game --output dist/my-game.m01c.png
mod01 export zip my-game --output dist/my-game-itch.zip
mod01 run my-game
mod01 run my-game --headless --frames 120 --input tests/replays/my-game.json
mod01 info my-game/dist/my-game.m01c
mod01 lsp
```

`watch --once` performs the same initial deterministic pack and exits for CI checks. Otherwise it
serves a loopback-only offline player, rebuilds only when project bytes change, and reloads after a
successful revision. A compiler failure stays in the terminal without replacing the last good
player. `fmt --check` reports source
that differs from canonical two-space formatting. `run` writes the same standalone HTML artifact to
`dist/` and opens it with `xdg-open`; `--no-open` performs only the validated export for CI or a
headless environment. `run --headless` instead drives the production console core under Node and
emits revisioned JSON framebuffer/state/audio/PCM/save hashes. It accepts a project directory or
`.m01c`, an explicit seed/frame count, compact controller trace and optional raw save image.

The HTML player has pause/reset/fullscreen/source controls, presentation metadata, and a `#embed`
mode. Studio `EXPORT ZIP` stores the same single HTML as `index.html`. `SHARE` offers only complete
carts up to 6,000 bytes as an 8,192-character maximum `#m01c=` fragment, displays the exact count
before copy, and never sends cartridge bytes in a request or query string.

The browser production build runs `scripts/build-wasm.sh`, which builds the Rust compiler for
`wasm32-unknown-unknown` and generates pinned web bindings before Vite bundles it. Cartridge
compilation therefore does not depend on a backend or a second TypeScript compiler implementation.
The production build also emits a generated same-origin service worker containing the exact hashed
build inventory. After its first successful load, the Studio boots and operates offline. The web
app manifest and maskable icon are static repository assets.

## Language-server clients

The stdio server supports full-document synchronization, dependency-aware diagnostics, project
completion, hover/signature help, cross-file definition/references/rename, formatting, and document
and workspace symbols. MODL/1 is ASCII-only, so LSP UTF-16 columns and compiler byte columns
coincide for valid files.

Generic Neovim setup:

```lua
vim.api.nvim_create_autocmd('FileType', {
  pattern = 'modl',
  callback = function()
    vim.lsp.start({ name = 'mod01', cmd = { 'mod01', 'lsp' }, root_dir = vim.fn.getcwd() })
  end,
})
```

For another LSP-capable editor, register `.modl` as MODL and configure the server command as
`mod01 lsp`. Clients send full-document changes; the server invalidates the edited module and its
direct importers without reanalyzing unrelated open modules.
