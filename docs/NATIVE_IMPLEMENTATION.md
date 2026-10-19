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
| Native touch controller       | Multi-touch D-pad/actions drive the ordinary controller/replay model in the native player without stuck/released-button errors.                                                                             | [ ]      |
| Pause/system menu             | Common native pause UI and typed cartridge-defined actions have specified input/timing/callback/replay/debug semantics.                                                                                     | [ ]      |
| Multi-cartridge bundles       | Local packaged dependencies, cartridge transitions, asset reload, state transfer, save identity and validation are implemented and tested.                                                                  | [ ]      |
| Local cartridge library       | Persistent labels/search/favourites/controller navigation/launch/source inspection cover user and bundled cartridges without a service.                                                                     | [ ]      |
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
