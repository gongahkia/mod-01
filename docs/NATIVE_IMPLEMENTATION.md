# Native-only implementation contract

This is the implementation and verification ledger for the user's completed
`mod01-pico8-2026-10-18-d9918f7` review and subsequent native-only clarification.
The review contains 21 decisions. Notes override the original proposal: use one
current machine/format, remove backward compatibility, remove the browser frontend
entirely, and ship desktop games instead of HTML or web ZIPs. The shared compiler
and console core remain authoritative for native Studio, CLI, and packaged games.

Checkboxes mean the complete behaviour has been demonstrated, not merely that an
API, UI label, or test file exists. Each implementation slice receives an atomic
Git commit. The final audit must link the evidence for every row below.

| Requirement                   | Acceptance evidence                                                                                                                                                                                         | Complete |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| Native-only product           | Browser app, Wasm bridge, PWA, workers, web sharing/export and browser-only build/test dependencies removed; native build/check/package gates pass.                                                         | [x]      |
| One current format            | Studio/CLI use one canonical editable workcart and one current packed encoding; no old-format/profile migration branches; authored modules, auxiliary files, layered sprites, recipes and tests round-trip. | [x]      |
| Current machine contract      | API, hardware, assets, manual, examples, limits and metadata agree on the current 240×144 / 256-colour / 1 MiB machine.                                                                                     | [x]      |
| Math helpers                  | Typed abs/min/max/clamp/rounding/sqrt/trig/atan2 execute with specified deterministic results, faults and work costs in release/debug/headless/native.                                                      | [x]      |
| Number formatting/conversion  | Explicit numeric formatting, padding and safe parsing work in HUDs and reject invalid inputs predictably.                                                                                                   | [x]      |
| Text helpers                  | Bounded length/slice/character conversion/split operations have useful typed results and tested edge cases.                                                                                                 | [x]      |
| Bit operations                | AND/OR/XOR/NOT/shifts use explicit integer-width/overflow rules with parser, compiler and execution coverage.                                                                                               | [x]      |
| Immediate MODL playground     | Native scratchpad/REPL evaluates expressions and drawing through the actual compiler/core; ordinary cart source remains editable and recoverable.                                                           | [x]      |
| Native source editor          | Find/replace, line/function navigation, block shortcuts, completion, declaration navigation and actionable diagnostics are usable in the native UI.                                                         | [x]      |
| Native tracker                | Full eight-channel grid with keyboard note entry, pattern/sequence operations, audition and playback feedback preserves real assets.                                                                        | [x]      |
| Native game export            | CLI and Studio package a cartridge with a standalone native player and runtime; launch, input, audio, saves, source inspection and platform packaging are verified.                                         | [ ]      |
| Sprite-region blit            | Source/destination rectangles, stretching, clipping/flips, deterministic sampling and work costs are tested in a real cartridge.                                                                            | [x]      |
| Textured lines                | Bounded texture sampling with specified coordinates, wrapping/clipping and work costs supports running pseudo-3D demonstrations.                                                                            | [x]      |
| Pattern fills                 | Configurable two-colour pattern/mask state has defined transparency/reset/snapshot behaviour and real drawing tests.                                                                                        | [x]      |
| Transparency controls         | Explicit draw transparency mask defaults, state reset, palette interaction and replay/debug restoration are verified.                                                                                       | [x]      |
| Native touch controller       | Multi-touch D-pad/actions drive the ordinary controller/replay model in the native player without stuck/released-button errors.                                                                             | [x]      |
| Pause/system menu             | Common native pause UI and typed cartridge-defined actions have specified input/timing/callback/replay/debug semantics.                                                                                     | [x]      |
| Multi-cartridge bundles       | Local packaged dependencies, cartridge transitions, asset reload, state transfer, save identity and validation are implemented and tested.                                                                  | [ ]      |
| Local cartridge library       | Persistent labels/search/favourites/controller navigation/launch/source inspection cover user and bundled cartridges without a service.                                                                     | [x]      |
| Examples and games            | Documented, playable and tested 2D and software-rendered 3D games demonstrate the added APIs using public facilities.                                                                                       | [x]      |
| Full verification and handoff | Formatting/lint/type/unit/integration/native/package checks pass; native screenshots and interaction QA inspected; worktree clean; every requirement has authoritative evidence.                            | [ ]      |

## Excluded

- Browser frontend and web distribution, including standalone HTML and web ZIP.
- Historical-format/profile compatibility and migration support.
- Raspberry Pi/ARM handheld packaging.
- Public online catalog, accounts, hosted submissions or a community backend.
- A general-purpose 3D scene engine: 3D examples use software rendering and the
  public console facilities.

Native touch targets touchscreen desktops. Editable source and compiled cartridge
are source/distribution representations of the same current contract, not two
historical formats. Dependencies determine implementation order; no user priority
choices remain outstanding.

## Progress

- Current workcarts now retain optional flat `[[bundle]]` members as exact UTF-8
  workcart strings. Rewrites, asset deletion, recipe generation and CLI formatting
  preserve member source, including CRLF/comments; aliases are bounded/unique and
  `main` is reserved for the enclosing cartridge. Nested/obsolete members fail
  with qualified diagnostics. Single-cart packing explicitly refuses dependencies
  until the whole-game adapters are connected, rather than silently discarding
  them. Three preservation/parser tests and all existing core checks pass.
- The preservation integration test exposed an older generator/schema mismatch:
  generators expected revision-1 tiles while the current validator requires 2.
  `generator.rs` and CLI fixtures now use only current tiles; noise/WFC tests pack
  the generated result, and the CLI test loads it through authoritative compiler
  admission. Obsolete tile sets are rejected. This correction is `fix: generate
maps from current tile assets and verify generated cartridges load`.
- Bundle container preparation shares the existing canonical M01C archive codec
  with explicit container bounds; logical cartridges retain the same 1 MiB packed,
  16 MiB decoded and 4,096-entry limits. Tests cover exact bounds, corruption,
  traversal and trailing bytes. No new hardware profile or historical format was
  added. All existing core tests and workspace Clippy pass.
- Multi-cartridge admission foundation: `modl-core::bundle` validates 1–32 current
  cartridges with a required `main` alias, bounded local aliases/source allocation,
  unique persistent cartridge IDs and ordinary per-member compiler/asset/packed
  limits. Catalog identity covers sorted aliases and every packed/source byte.
  Five tests pack/load all 32 members, prove exact source recovery and input-order
  independence, and reject invalid/unused members and identity/size ambiguities.
  This is shared in-memory admission, not a user-facing bundle format or runtime
  transition implementation; the multi-cartridge acceptance row remains open.
- Offline native library is implemented: `docs/LIBRARY.md`, `library.rs`,
  `library_preview.rs`, `library_store.rs` and their tests cover bundled/user
  workcarts, packed/PNG source inspection, labels/search/favourites/recents,
  controller/keyboard navigation, explicit bounded background folder scans,
  read-only corruption handling and safe bookmark removal. Covers are rendered
  offline with only the shipped font and no external/data resource resolution;
  full dependency notices ship in both native packaging paths.
  Real Studio/Node tests launch source/packed/PNG cartridges and preserve the
  previous workspace on cancel, invalid manifest, failed open and save failure.
  Bundled source remains an unsaved copy. Native screenshots inspected at 1280×800
  and 900×600 demonstrate search, persisted favourites/labels, keyboard launch,
  paused browsing, source inspection and the unsaved-work confirmation/cancel.
  Captures are under `output/native-qa-8iPAwp/library-*.png`; only isolated QA
  preferences were written, never game source. `a41847c` contains the browser;
  `963746d` and `3afdd8e` contain the lock and small-window/label fixes.
  Full `scripts/check.sh` passes: 230 runtime tests, 95 native tests, 22 CLI
  integrations, all core checks, warning-free workspace Clippy/build, nine
  reproducible raw/PNG pairs and 120 Linux packaged frames with no PATH dependency.
  There are now 18 completed acceptance rows; multi-cart, broader platform/device
  export verification and final audit/handoff remain open.
- Full parallel verification exposed a library-lock lifetime race: closing a
  handle alone need not release a lock while a runtime child briefly inherits
  its file description before exec. The transaction guard now explicitly unlocks
  on every exit. Tests retain a duplicated handle while committing the next
  transaction. All 95 native tests and the rerun full gate pass.
- Offline-library storage foundation: five native tests prove atomic preference
  updates, favourites/labels/recents persistence, stale-instance conflict checks,
  bounded metadata and read-only corruption handling. Forgetting a bookmark never
  deletes a cartridge. Initially test-only, this foundation is now connected to
  the native browser described above. Workspace Clippy passes.
- Scope revalidated against the actual downloaded JSON; baseline is `d9918f7`.
- Working branch: `feat/native-consolidation`.
- Implementation and final verification remain open.
- Browser Studio/Wasm/PWA removal: `5e17574`; browser adapters, sharing/storage
  and worker protocol removal: `09090dd`. Web export/runner removal: `7872dac`.
- Numeric helpers: `docs/STANDARD_LIBRARY.md`, `math.test.ts`, the compiler's
  numeric type tests, and `stdlib-conformance.test.ts` demonstrate release/debug,
  native/headless execution, independent accuracy vectors, bounded costs and
  snapshot replay. The final workspace gate remains mandatory.
- Native launcher/process verification: `c33ce00`; native directory/`.app`
  packaging: `bbfa777`. Local Linux package QA completed 120 frames with an empty
  PATH and matched both canonical cartridge bytes and the original native player.
- Web player/export/loopback runner removal is implemented. Runtime, compiler,
  CLI and native tests pass. The current replay goldens were updated only after
  proving unchanged input hashes, work peaks, framebuffer checkpoints, audio
  commands and PCM; new builtins/generated code change packed/snapshot identity.
  Full visual QA, format consolidation and remaining acceptance rows are open.

- Current-format consolidation: every maintained project is `cart.m01w`;
  M01W/2 and M01C/2 are the only supported formats. Exact workcart metadata,
  auxiliary files, layers, recipes and tests round-trip. Imported build products
  are checked against recompiled source before native/CLI use.
- Current memory: one 1 MiB ROM at `0x100000`, 256-colour palette at
  `0x200000`, one 1 MiB visual bank at `0x210000`; no aliases.
  Allocation metadata is bounded to 8,192 records, below ROM. Boundary tests
  cover the full table with a full ROM present and rejection of record 8,193.
- Consolidation checks: 69 core tests, 18 CLI integration tests, 19 native
  tests, 160 runtime tests, formatting/lint/types and warning-free workspace
  Clippy. All eight games pack/PNG-export reproducibly and boot. Linux native
  package verification passes 120 frames without PATH. Golden updates prove
  unchanged input/work/pixels/checkpoints/audio/PCM, changing only packed and
  complete device-state identities. Windows/macOS execution and visual/input QA
  are not claimed by these Linux checks.

- Compiler-derived tooling: `83a4fc2` unifies the builtin metadata; `212e805`
  exposes original-file definitions, diagnostics and static call/state/asset
  relationships, including a display-free `mod01 graph` command. Native CODE
  now provides the source tools described in `docs/EDITOR.md`.
  Six source-editor tests include the actual egui widget's keyboard selection,
  undo/redo, completion acceptance and minimum-size layout with the real font.
  All 25 native tests and workspace Clippy
  pass. Native Xvfb screenshots were inspected at 1280×800 for find, imported
  F12 navigation, comments/undo, diagnostics, graph selection, completion,
  replace-all and line navigation. Captures are retained locally under
  `output/native-qa-8iPAwp/`; QA edits were in memory, not saved into game source.

- Extended-colour coverage corrected a remaining compiler-only 32-colour check
  in `ba2d0ab`. `palette_profile.rs` checks all 256 literal colours in both
  compiler modes; `palette.modl` renders every index through typed Color values
  in release/debug, native/headless conformance.
- Native and CLI now use the same default seed, 604772761, rather than native
  silently using zero. The seeded production-host test compiles a real random
  image and compares every frame's pixels and work for the default and seed 1,
  in release/debug. The shared/native/headless helper/palette suite and native
  host tests pass together (11 tests); explicit seed zero remains supported.

- Native playground: `docs/PLAYGROUND.md` specifies fresh-machine expression and
  drawing evaluations through the ordinary Rust compiler and production host.
  Tests cover actual values, Unicode/formatting, rendered pixels, frame-boundary
  state, copied unsaved project/asset context, no save commits, deterministic
  seeds, runtime/type/work faults, cancellation and history restoration.
  All 32 native tests and workspace Clippy pass. Real native screenshots were
  inspected for evaluation, drawing, error byte spans, history restoration,
  exact wrapped source and 900×600 layout; retained in
  `output/native-qa-8iPAwp/`. No project was written during this visual QA.

- Configurable transparency: `palt`/`palt_reset` use a writable 256-bit source
  mask, before remapping, with frame/boot defaults and transactional bus restore.
  Graphics tests cover all 256 colours, sprites/transforms/animation/maps, solid
  primitives, raw writes, explicit reset and raster rejection. The real MODL
  visual conformance cartridge asserts the API/MMIO alias and replays complete
  release/debug snapshots; its native-host pixels/work match shared-core output
  and headless execution also passes. The focused graphics/bus/hardware suite
  passes 41 tests. Master RGB data is now shared by native tools and the runtime
  (`f0d0e30`), fixing native colour 25 without changing console palette values.

- Primitive patterns: typed `fill_pattern`/`fill_pattern_reset`, 4×4 low-bit-first
  screen anchoring, two remapped colours, transparent holes and an eight-byte
  validated RW record. Reset, invalid writes, raster restrictions and complete
  snapshot restore are exercised alongside all primitive kinds. Two compiler
  tests cover both modes, invalid types and forbidden raster calls; the focused
  graphics/bus/runtime/hardware suite passes 58 tests, plus lint/types/Clippy.
  The updated visual workcart asserts real drawing and MMIO in release/debug,
  native/headless and replay. Its running native-player frame was inspected in
  `output/native-qa-8iPAwp/pattern-visual-cartridge.png` (960×650 window).

- Sprite regions: `sprite_region` provides typed source/destination rectangles,
  nearest stretching/shrinking, independent flips, preserved out-of-source holes,
  camera/clip/mask/remap and charged destination-area work before mutation.
  An independent sampler compares 180 combinations; tests include integer-boundary
  camera coordinates, transparent source colour zero, pattern exclusion, empty
  rectangles, invalid limits and pre-draw work faults. Three compiler drawing
  tests and 38 focused graphics/hardware tests pass, with lint/types/Clippy.
  Real visual-cart assertions run in release/debug, native/headless and full
  snapshot replay. Final cross-platform packaging/input/audio QA remains open.

- Textured-line API implementation: typed `texture_line` samples Sprite assets
  with bounded affine coordinates, exact 16-fractional-bit accumulation,
  inclusive Bresenham coverage, explicit wrap/skip and shared mask/remap/clip.
  Tests cover all octants, reversed endpoints, fractional/negative inputs,
  clipping phase, extreme camera coordinates, bounds, work and real MODL/native/
  headless replay. All 178 runtime tests, lint/types/formatting and workspace
  Clippy pass; four compiler drawing tests pass. The textured-line acceptance
  row remains open until a running source-visible pseudo-3D example uses it.

- PRISM CORRIDOR closes that demonstration gap: a complete ordinary-MODL maze
  with grid raycasting, textured walls/floor, patterned sky, radius collision,
  editable Map, minimap, prism/exit objective, synth cues and best-time save.
  The 416-frame public trace wins in release/debug; streaming native replies
  match shared-core pixel checkpoints, peak work, PCM cues and committed saves.
  Both full-route tests pass, including complete snapshot replay; the CLI route
  completes below the work ceiling (36,553 peak). Native title/movement/map/
  heading screenshots were inspected in `output/native-qa-8iPAwp/`.
  Nine cartridges now pack/PNG-export reproducibly and boot, and Linux bundled
  game verification still passes 120 frames without PATH. Actual speaker and
  physical-controller QA, and Windows/macOS execution, remain unclaimed.

- Current checkpoint: the complete `scripts/check.sh` gate passes: formatting,
  lint, types, 181 runtime tests, warning-free all-feature workspace Clippy,
  20 CLI / 32 native / 82 core unit/integration tests, builds, all nine
  reproducible raw/PNG artifacts and the Linux bundled-game verification.
  PRISM CORRIDOR also independently passes its own 120-frame native package
  check without PATH. This is a checkpoint, not completion of the still-open
  tracker, touch, menus, bundles, library or cross-platform/input/audio QA rows.

- Live native synth preview uses the production eight-voice engine, read-only
  unsaved asset copies and a disposable child, with clock-paced PCM and actual
  order/row/tick/voice/peak telemetry. Cancellation/replacement and repaint-rate
  tests pass; all 36 native tests and warning-free workspace Clippy pass.
  Song playback, stop controls and wrapped 900×600 controls were visually checked
  under Xvfb (`live-audio-song.png`, `live-audio-small.png`). This does not claim
  physical speaker QA or completion of the full tracker grid.

- Native tracker grid: eight-channel keyboard entry, focus-safe navigation,
  clipped rectangular copy/cut/paste, atomic transpose, cell patch/volume,
  bounded row operations, reference-safe pattern rename/delete/duplicate and
  order insert/replace/remove/move are implemented. Eight tracker/workbench
  tests include actual egui keyboard/undo/redo, off-screen Home/End navigation,
  malformed-source retention and a round-tripped eight-note asset playing all
  eight voices in the actual native synth. Native Xvfb QA covered eight-channel
  entry, copy/paste, undo/redo, pattern playback, scrolling and 900×600 horizontal
  navigation. Screenshots include `tracker-eight-notes-final.png`,
  `tracker-pattern-playback.png` and `tracker-minimum-scrolled.png`.
  Studio help documents the exact shortcuts and local clipboard semantics.

- Per-asset sessions now preserve structured and layered-sprite undo across asset
  switches and ordinary save/sync, with stale-byte/kind/path/layered-source checks
  and a bounded LRU cache. Imports are undoable; raw text undo is independently
  scoped to each asset/project. Five cache/Studio-level tests cover real
  music/sound/sprite/display transitions, unchanged manifests, external edits,
  cache expiry, invalid raw buffers and zero source-file writes. Native visual
  QA verified returning to a newly added pattern, undoing it, visiting Display
  and returning with redo still available (`asset-history-returned.png`,
  `asset-history-undo.png`). First-use Display also now safely inserts its
  manifest key. The master palette has 256 colours; the display/drawing remap
  remains the documented 32 logical entries (with targets 0–255).

- Shared Rust asset admission now checks every current schema, unused music/map
  references, tile bounds, exact visual bytes and 8,192 allocation slots before
  pack and packed import. `mod01 assets` exposes its structured report without
  Node; `check` reports independent failures and pack writes nothing on rejection.
  Seven core tests cover maintained projects, precise fields, all oscillator kinds,
  boundaries and no source mutation. Thirty-nine cross-language cases agree with
  the actual production runtime on acceptance, byte counts and allocation slots;
  CLI regression checks prove reference diagnostics without PATH/Node and retain
  a separate valid-schema packed-capacity test. Native live-meter integration is
  the next slice, not claimed by these backend tests.

- Studio's live asset meter now uses that exact report on a read-only overlay of
  unsaved structured/raw JSON and sprite frame changes. Four-Hz refresh pacing,
  exact animation-kind/cost changes, malformed JSON visibility and zero project
  mutation have native tests. All 51 native tests and workspace Clippy pass.
  Xvfb QA inspected the valid meter, an unsaved missing-sound reference with its
  JSON field, and wrapping at 900×600 (`asset-meter-valid.png`,
  `asset-meter-diagnostic.png`, `asset-meter-small.png`). Diagnostic expansion is
  height-bounded so a large failure list cannot consume the entire editor.

- Shared PNG conversion now lives in `modl-core`: source/IHDR/decoder limits,
  alpha threshold, exact master palette, deterministic RGB distance/ties and
  row-major tile layout. Both native importers and layered-sprite flattening use
  it; duplicate native quantizers were removed. Core tests cover palette/alpha,
  malformed/oversized headers, preserved RGBA/metadata/flags and tile order.
  A native regression compares actual imported pixels against that converter and
  proves resize/import is one undo transaction with no mutation on rejection.
  CLI workcart import remains the next integration slice.

- `mod01 import png` now atomically adds/replaces sprites or tile sets, preserving
  source/auxiliary files, recipes/tests, metadata and flags, and retaining original
  RGBA in the current layered-source schema. Replacement is explicit and full-bank
  validation/compilation/capacity must pass before a write. Bounded source reading
  is shared with native dialogs. CLI tests prove occupied-output/implicit-replace
  refusal, no-op repeat determinism, invalid tile-reference rejection, malformed
  PNG rollback and no temporary-file leaks. A native/CLI integration test proves
  byte-identical indexed files, layered source, metadata and packed cartridge bytes
  for the same import context. A core test verifies explicit sprite replacement
  updates stale layered source without changing other authored data.

- Checkpoint at `405bdca`: the complete `scripts/check.sh` gate passes with
  224 runtime tests, 22 CLI tests, 54 native tests and 94 core unit/integration
  tests, formatting/lint/types, warning-free all-feature Clippy and workspace
  builds. All nine raw/PNG cartridge pairs remain reproducible, and Linux's
  bundled player verifies 120 frames without PATH. No replay golden refresh was
  needed for the tracker/asset-tooling work. The remaining touch/menu/bundle/
  library/platform acceptance rows are still open.

- Native runtime session boundaries now discard superseded queued commands and
  responses before they reach Studio. This closes the reload race where an old
  frame/save/audio reply could be applied after selecting a new project. Tests
  cover already-queued stale payloads and stop/snapshot replies, missing-host
  startup errors, and rapid real cartridge loads with distinct framebuffer/save
  commits. Existing preview/playground cancellation tests still pass. This is
  host isolation infrastructure, not completion of multi-cartridge gameplay.

- Native touch controller: optional multi-contact port-one pad, diagonals,
  A/B/X/Y/L/R/Start, host-only MENU, short-tap latching, independent device/finger
  ownership and cancellation/focus/layout/session release rules. Six tests cover
  those rules plus actual egui touch injection and real native MODL/replay
  equality for pixels/work/stereo PCM. All 63 native tests and workspace Clippy
  pass. Native Xvfb QA used mouse emulation for start, held movement/highlight,
  menu pause and 900×600 2× layout (`touch-player-moving.png`,
  `touch-player-paused.png`, `touch-player-minimum-final.png`). These are software
  event/visual checks, not physical touchscreen or audible-speaker claims. The
  common typed cartridge-action pause menu remains its separate open requirement.

- Native input/replay sequencing now distinguishes the next input frame from
  the last displayed frame, locks inputs across source steps, records only
  completed frames and starts recording from a fresh machine. Snapshot/restore
  replies carry authoritative frame/boot cursors; restore trims the recording
  branch and clears suspended input. Pending debug commands are counted rather
  than cleared by the first unrelated reply. Four real Studio/Node tests cover
  release and stepped three-frame button traces, record restart and snapshot
  rewind without duplicate frame zero/statement entries. All 67 native tests,
  the native-host test, lint and type checks pass. This does not claim completion
  of the typed menu or platform verification requirements.

- Debug and loaded-replay save commits are now explicitly disposable at Studio's
  persistence boundary; ordinary gameplay still writes its committed bytes.
  A native regression exercises all three cases using an exclusively test-owned
  save identity. The debugger guide now describes the actual native controls
  and separates runtime-library replay capabilities from UI features that are
  not present; it no longer promises an automatic native timeline, watchpoints
  or voice inspector. This does not remove or narrow the remaining acceptance
  requirements in the table.

- Typed pause-action core: `menu_item(Int, Text)` maintains five bounded slots;
  `on menu(action: Int)` is a compiler-derived callback, dispatched before update
  even on skipped-update 30 Hz frames. Registration, action input, fault/work
  rules, source stepping and complete snapshot restoration are tested. The
  public JS/Rust replay codecs retain and validate the same action field. The
  real MODL fixture compares shared release/debug, source-stepped execution,
  headless replay and restricted native-host frames/restores for pixels, work,
  stereo PCM and committed saves. The three existing game goldens retain exact
  input/work/pixels/checkpoints/audio/PCM; only compiler-derived cartridge and
  complete snapshot identities changed. `docs/MENU.md` specifies the current
  typed contract. Common native menu UI/input integration remains open.
  Verification passes formatting/lint/types, all 230 runtime / 69 native / 95
  core / 22 CLI tests, all-feature warning-free workspace Clippy and builds,
  all nine reproducible raw/PNG pairs and 120 bundled Linux native frames
  without PATH. Windows/macOS and physical-device execution are not claimed.

- Native keyboard capture now runs independently of frame replies, retaining
  short taps until one accepted logical frame and preserving new taps during a
  source-stepped partial frame. Focus/text/shortcut/pause/session/restore rules
  prevent phantom or stuck presses; Escape is host-only. Three actual egui
  sampler tests and two real Studio/Node tests cover latency, boot, source-step
  ownership and exact recorded input. All 74 native tests, native Clippy and
  builds pass. Native Xvfb QA verified a 12 ms Z tap starts PRISM CORRIDOR;
  before/after screenshots were inspected under `output/native-qa-8iPAwp/`.

- Common native menu UI now closes its acceptance row: Studio and standalone
  play share resume/restart/source and five typed cartridge-action slots,
  keyboard/controller/pointer navigation, neutral recorded activation frames,
  dynamic labels/removal and pending/replay/partial-debug guards. Native source
  continuation is scheduled after UI input handling and remains stopped while
  paused, including an already-in-flight breakpoint. Four-port gamepad events
  latch short taps, keep surviving port identities on disconnect and reserve
  physical Select for the host menu. Seven new model/widget/real Studio tests
  cover those cases, packed-source execution, exact action replay, zero paused
  frame submissions and unchanged source inspection. All 81 native tests and
  native Clippy pass. `examples/menu-actions` is a source-visible running demo.
  Native Xvfb screenshots were inspected for both game and Studio menus,
  GRID: ON/OFF, removed actions, source inspection and 900×600 layout:
  `system-menu-player.png`, `system-menu-minimum.png`,
  `system-menu-removed-action.png`, `system-menu-source-inspection.png` and
  `system-menu-studio-final.png` under `output/native-qa-8iPAwp/`. Real physical
  controller/touch/speaker and Windows/macOS QA remain unclaimed.
  The complete repository gate also passes: formatting/lint/types, 230 runtime,
  81 native, 95 core and 22 CLI tests, all-feature workspace Clippy/builds,
  all nine reproducible raw/PNG pairs and the 120-frame bundled Linux player
  verification without PATH. No further golden changes were required.

- Fault/pause race follow-up: a runtime fault from an already-submitted frame
  now dismisses the pause state, clears queued actions/continuation and keeps the
  actual error visible. A stopped/faulted game's header/touch MENU cannot mask it
  again with a paused label. A real Studio/Node regression faults precisely after
  opening the menu around the second submitted frame; no extra input is accepted.
  All 82 native tests, all-feature workspace Clippy, native build and the
  120-frame bundled Linux verification pass after this correction.
