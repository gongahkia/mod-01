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
| Multi-cartridge bundles       | Local packaged dependencies, cartridge transitions, asset reload, state transfer, save identity and validation are implemented and tested; [acceptance evidence](BUNDLES.md#acceptance-evidence).           | [x]      |
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

- Native asset number readers now reuse the shared Rust admission helper instead
  of serde_json's storage-specific integer accessors. Regressions first reproduced
  valid `2.0` sprite rejection, `255.0` tile export as transparent black and a
  disabled `240.0`/`60.0` tracker. Real-font tile/map/font/display UI output now
  matches integral-spelling banks without source/history changes; the same float
  bank passes whole-game packing. Sprite colours, PNG alpha/colour, map fill,
  tracker note display/transposition and signed raster scroll use numeric meaning.
  Invalid PNG colours are rejected rather than silently clamped/defaulted. Four
  native regressions and the shared reader boundary test pass. All 141 native
  and 133 core tests plus warning-free all-feature workspace Clippy pass.
- Requirement-specific bundle audit is complete against the reviewed local
  dependency/transition/reload/state/save/validation contract. The bundle guide
  now links admission, scheduler, native editor/debugger and cross-host/package
  evidence, including the maintained game and night-bank rewind. Removed stale
  claims that the native handler or replay integration is unimplemented. This
  closes the multi-cartridge row only; desktop platform/hardware qualification,
  identified general tooling hardening and the full product audit remain open.
- Release artifact smoke checks now inspect the JSON result, not only CLI exit
  status. Each of ten maintained packed games must report the current result
  revision, expected identity, five requested/completed frames, five ordered
  records and no fault field. Two regressions cover incomplete/malformed traces
  and an actual faulting MODL cartridge whose CLI process returns structured JSON
  with exit status zero; the release checker rejects it with status one. Lint,
  the two tests and the strengthened artifact/package verifier pass with all
  cartridge bytes unchanged. All 320 runtime/script tests in 43 files and the
  repository format check also pass. This changes verification, not CLI fault reporting.
- A shared 40-case source-readable Rust/TypeScript replay corpus first reproduced
  compact-empty, integral-number and negative-zero differences. Both decoders now
  agree on accepted schemas and exact expanded inputs/round trips, including
  integral fraction/exponent spellings, bounded ranges, port identity, pointer
  coordinates and menu actions. Explicit null pointer is rejected like other
  invalid optional fields; strict JSON/UTF-8 admission rejects a BOM consistently.
  Numeric values are canonicalized without changing the current revision or
  adding compatibility branches. The corpus follows JSON's single numeric grammar,
  not serde_json's storage distinction between integer/floating number variants.
  The full gate passes 318 runtime, 132 core, 137 native, 34 CLI integration and
  one CLI report-unit test, formatting/lint/types/all-feature Clippy/builds, ten
  unchanged deterministic raw/PNG pairs, both 120-frame Linux package checks and
  the 110-frame CLI/native/source/raw/PNG/standalone comparison.
- Actual Studio runs the maintained Relay Station programs/assets in release and
  statement-step debug modes through all 110 replay frames. Per-frame pixels,
  work, active routing and recorded input match; selected donor source cannot
  change main's launch entry, source events identify main/vault correctly and the
  donor never executes. A real Studio checkpoint after the night-bank swap
  rewinds to frame 23 and replays to identical canonical state/checkpoint and
  per-ID saves. Both modes use isolated test save IDs and persist no commits;
  authored source remains unchanged. The regression and workspace Clippy pass.
  Opaque V8 bytes are deliberately not compared after restore: Node documents
  that equal values can serialize differently; canonical evidence is authoritative.
- CLI and native verification now share the same canonical evidence helpers,
  with unchanged existing replay goldens. Native reports bounded per-frame
  SHA-256 checks for indexed pixels, interleaved PCM and audio commands, plus
  canonical final state/checkpoint and per-ID saves. The maintained Relay Station
  verifier compares every one of 110 frames from source/raw/PNG across both hosts
  and an actual Linux packaged executable from an unrelated directory with empty
  PATH. Routing, work, all final hashes, two bank swaps, non-silent audio, six main
  and 104 vault frames, two commits each and untouched donor save agree. The
  desktop CI packaging matrix also runs this check; other-OS execution remains
  unobserved locally. Seven targeted host/headless and four native player tests,
  existing CLI replay goldens, workspace Clippy/lint/types/build and both existing
  120-frame native packages pass. Actual Studio source-stepping of this route and
  the final requirement audit remain open.
- `cartridges/relay-station` is a maintained playable 47,591-byte three-member
  game: 60 Hz station, 30 Hz vault and compatible data-only night bank. Public
  MODL implements movement, three ordered relay interactions, two bank swaps,
  menu actions, parameter handoff, task/RNG HUD and per-ID visits/clears. Its
  110-frame source-readable replay returns home with six main and 104 vault
  frames, two save commits each and non-silent production PCM; the donor never
  boots. CLI regression anchors the final image, PCM and distinct save images;
  maintained asset checks now cover every embedded member, not only main.
  Inspected `output/relay-station-qa-mFRPyN` captures show station, day/night
  sprite changes, live counters, full minimum player/touch and active-member
  source inspection. Updated bundle/format docs distinguish implemented
  integration from the still-open complete cross-host/final audit. The actual
  Linux package reproduces the 110-frame native source report, including final
  checkpoint, from an unrelated directory with empty PATH. The full gate passes
  278 runtime, 132 core, 134 native, 34 CLI integration and one CLI report-unit
  test, formatting/lint/types/all-feature Clippy/builds, ten deterministic raw/PNG
  pairs (the previous nine unchanged) and both existing 120-frame Linux package
  verifications. Direct CLI/native per-frame comparison remains open.
- Actual bundle GUI QA found SOURCE retained the 480×320 player window, leaving
  Studio unusably small. Native mode/touch transitions now update the same shared
  viewport minimum policy and grow undersized windows: Studio 900×600, player
  480×320, touch player 480×500. Unchanged modes emit no repeated resize commands,
  and disabling touch does not force shrinkage. Real-font framebuffer layout
  and real viewport-command tests pass. Inspected `output/relay-station-qa-mFRPyN`
  captures prove the full game/touch controls at 480×500 and SOURCE automatically
  reopening the active vault member at 900×600. Both GUI sessions exit normally;
  targeted player tests and workspace Clippy pass. Physical touchscreen QA is
  still separate from this layout/interaction evidence.
- Authoring the maintained bundle example exposed a real block-parser gap:
  leading comments were accepted by the lexer but rejected before a block's
  first indent. Records, enums, functions/tasks/callbacks, nested loops and match
  arms now skip comment/blank lines consistently before consuming that indent.
  Regressions first reproduced the failure and still reject comment-only empty
  bodies. All 102 core library tests and workspace Clippy pass; the new game
  checks and formats through the public CLI after this fix.
- Native display-free verification now accepts bounded current replay input via
  `--input REPLAY`, with ordinary Studio sharing the same ≤8 MiB file reader.
  Unknown/repeated/missing options and inputs beyond the requested 1–36,000 frames
  are rejected before loading a game. Reports bind consumed input and the final
  opaque whole-game checkpoint as well as the existing pixels/PCM/routing/saves.
  Real two-member tests compare neutral vs transferred play, repeat exact
  fingerprints, preserve source/replay bytes and prove input-range preflight.
  Compact current empty traces no longer depend on a trailing newline.
  Targeted replay/launch/native-verifier tests and workspace Clippy pass; both
  real 120-frame Linux package verifications retain identical fingerprints,
  including the newly checked final checkpoint, without PATH.
- Native Assets/Sprites now expose confirmed, stopped-session, member-bound
  deletion through the shared reference checker. Alias, cartridge ID, asset name,
  kind and path are rechecked before changing the selected member; other members
  retain exact source. Only the deleted asset's structured/sprite/raw-text history
  is discarded, and raw JSON widget IDs now include member/save-ID scope. Modal
  details scroll while Cancel/Confirm remain visible at 900×600. Five native
  regressions cover real pointer cancellation/reference errors/confirmation,
  stale/running targets, source/tests/auxiliary preservation, unsaved sprite
  flushing/sidecar retention, member isolation and non-resurrecting undo/redo.
  Inspected `output/native-asset-delete-qa-4hZHn8` captures demonstrate actual
  creation of a tile set/map, reference-blocked tile deletion, then map/tile
  removal with the live bank falling 3→2→0 slots. The file hash stays unchanged
  until Save; recovery retains the previous tile source exactly. The final saved
  workcart equals the initial blank QA workcart, passes CLI check/pack and five
  actual native packed frames. The GUI session closes normally with status zero.
  The full gate passes 278 runtime, 130 core, 131 native, 33 CLI integration and
  one CLI report-unit test, formatting/lint/types/all-feature Clippy/builds, nine
  unchanged raw/PNG pairs and both real 120-frame Linux package verifications.
  Multi-cartridge example/conformance and the final cross-host/platform audit
  remain open.
- Shared reference-safe deletion now validates other map/music documents with
  the same per-document schema as asset admission. Schema-invalid dependency
  records cannot be silently treated as unreferenced; the diagnostic names the
  unverifiable owner and field. Removing that malformed asset itself remains
  possible. This does not require aggregate capacity admission: a regression
  deletes an unreferenced 65-frame animation to repair an overcapacity bank.
  All six core deletion regressions and warning-free workspace Clippy pass.
- Native debugger request rejection now preserves the healthy host and Studio's
  live source/input/replay/menu context. The regression first proved a malformed
  restore killed the real suspended two-member game. Recovery is explicitly
  limited to read-only requests and proven transactional restore/edit validation;
  actual execution, unexpected write and transport failures remain fatal.
  Rejections acknowledge only their own pending request; invalid restore does
  not consume a fresh scratchpad. Canonical bounded snapshot/edit decoding
  replaces permissive byte repair. Production host tests cover foreign/deeply
  invalid checkpoints with byte-identical before/after snapshots, cross-region
  edit rollback, invalid/valid reads and edits, plus scratchpad continuation.
  The real Studio regression resumes the same arena statement/input and replays
  all seven frames after four rejected requests without changing recorded inputs.
  The full gate passes 278 runtime, 128 core, 126 native, 33 CLI integration and
  one CLI report-unit test, all formatting/lint/types/Clippy/build steps, nine
  unchanged raw/PNG pairs and both real 120-frame Linux package verifications.
- Native WAV export now passes its existing 8 MiB transport bound into shared
  renderers, which preflight exact PCM16 bytes before executing synth frames.
  No output is truncated and the renderers' default capacity is unchanged.
  Tests cover exact/header/release boundaries, invalid budgets/assets, the
  largest admitted native frame count, the next rejected frame and zero synth
  execution on oversized requests. The generated production host rejects both
  long sounds and otherwise valid tracker songs, then serves a valid WAV in the
  same process. Studio documents the limit and exposes it on the export tooltip.
  The full gate passes 278 runtime, 128 core, 125 native, 33 CLI integration and
  one CLI report-unit test, formatting/lint/types/all-feature Clippy/builds, nine
  unchanged raw/PNG pairs and both 120-frame Linux package verifications without
  PATH. Remaining cross-host/product audit and physical/platform QA stay open.
- Native sound controls no longer write fractional MIDI/frame values or clamp
  valid long envelopes on opening. Integer fields cover the full current schema,
  float fields preserve full precision on focus/Enter, and malformed source is
  left recoverable rather than repaired by repaint. Explicit waveform changes
  supply only required missing duty/table defaults; authored tables survive
  switches and expose bounded 4–32 sample editing with document undo/redo.
  A regression first demonstrated opening-source mutation. Real-font pointer/
  keyboard tests cover integer rejection/edits, unchanged opening, metadata,
  waveform selection, sample bounds and undo. Edited patches pass shared core
  validation/packing and reach the production native synth without disk writes.
  All nine asset-tool tests and all-feature workspace Clippy pass. Inspected
  `output/native-sound-qa-Djb4eW` captures show all five waveform choices and
  scrollable four-sample editing with a valid bank at 900×600. GUI edits remained
  disposable; the session closed normally with status zero.
- Asset source, sprite/tile-atlas PNG and WAV export dialogs now share portable
  `asset-NAME.EXT` suggestions, including reserved device-like IDs. Labels are
  bounded/sanitized, source extensions are checked, and explicit user-selected
  destinations remain authoritative. Asset identities, project paths and output
  payloads are unchanged. Regressions cover device names, maximum/unsafe/Unicode
  labels, all current export extensions and unsafe extension fallback. This is
  filename-default hardening, not Windows execution evidence or a unique-ID map.
  Both filename tests, all four Studio asset integrations and warning-free
  all-feature workspace Clippy pass.
- Native device callbacks now coalesce bounded first notices instead of repeatedly
  logging ALSA errors. Current CPAL classifications keep xrun/reroute/realtime
  warnings nonfatal; unusable streams are muted/cleared and released by the UI
  owner. `AUD`/`SILENT` indicators retain device detail separately from VM status.
  Game and live-synth timelines continue with no output device, and output is
  re-opened by restarting the application rather than an uncontrolled retry loop.
  Two callback-policy regressions cover all current error classes, repeated/
  concurrent notices and bounded messages; a real two-member native integration
  proves losing output cannot stop, rewind or re-identify gameplay/replay.
  Targeted audio/live-preview/game tests and workspace Clippy pass. The player
  keeps audio/status on one truncated line and moves long titles above controls
  at narrow widths. Its real-font egui regression covers long titles/statuses at
  480×320 and 960×650; the inspected 480×320 capture in
  `output/native-audio-qa-wiSy8a/player-minimum-final.png` shows the complete
  framebuffer between controls and status. All 120 native tests pass, and a fresh
  active-game GUI session exits with status zero on WM_DELETE_WINDOW without
  audio callback logging. Physical audio quality, remaining editor hardening and
  final audit remain open.
- Native saves/schema/export defaults now use the checked shared portable stem,
  without changing persistent IDs or the current save-container bytes. Recovery
  has separate per-ID directories instead of ambiguous `ID-` prefix matching:
  `abc` retention cannot prune `abc-def`. New backups use create-new plus sync,
  bounded collision retries and retention that protects the immediate pre-change
  copy against clock rollback. Only bounded, regular, valid same-ID backups count
  toward the ten-copy limit; foreign/malformed/oversized/symlink files remain
  untouched. Four new regressions cover these cases and old development-file
  preservation. All five save-manager tests, 15 real native game integrations,
  debug/replay save isolation and warning-free workspace Clippy pass. No user
  files were migrated, renamed or deleted; old filenames are not silently loaded.
  The full rerun passes 275 runtime, 128 core, 116 native, 33 CLI integration and
  one CLI report-unit test, formatting/lint/types/Clippy/builds, nine unchanged
  raw/PNG pairs and both Linux package verifications without PATH. An earlier
  gate was terminated by SIGTERM during native tests, not an assertion failure.
  Inspected `output/native-save-qa-iYFLvq` captures prove `con.foo` displays as
  its original ID, writes `cart-con_foo.bin`, and fits confirmation/reset at
  900×600. Confirmed reset leaves a zero-length active image and a recovery with
  the original 8 KiB SHA-256; stopped Studio exits normally on WM_DELETE_WINDOW.
  This WSL audio device emitted repeated ALSA I/O errors during play: physical
  audio/device-error handling is not verified by this GUI check. Asset export
  defaults, device-failure handling and final cross-host/product audit remain open.
- Core now provides a checked, injective portable cartridge filename stem:
  `cart-ID`, with dots encoded as underscores (not legal in cartridge IDs).
  Invalid IDs are rejected before path joining. CLI/Studio packed/PNG defaults,
  native output folders, executable and macOS app names all use main's same stem.
  Root IDs, title, exact packed bytes and per-member save ownership do not change.
  Tests cover maximum-length/device/dotted/trailing-dot IDs, rejected traversal/
  non-current IDs, six real CLI default export paths without Node, shared package
  layouts and child-selected Studio export. These targeted core/CLI/native tests
  and all-feature workspace Clippy pass. Save storage/recovery names, asset
  export defaults and the final cross-host audit remain open.
- Native player executables now use a separate `cart-ID[.exe]` file namespace.
  A regression first reproduced a valid `node` ID overwriting the exported
  player with bundled Node. The test checks distinct player/Node bytes, canonical
  game/host/metadata/notices and unchanged persistent IDs across 14 collision/
  device-like IDs and all three package layouts. The real Linux package verifier
  now uses `node` as its two-member game's main ID, so that collision is exercised
  with actual binaries rather than matching fake tool headers. All five core
  export tests, warning-free all-feature workspace Clippy, native/CLI builds and
  both real 120-frame Linux package verifications pass. Default export
  folder/asset/save basename portability and final cross-host audit remain open.
- CLI `info` now admits complete source/raw/PNG games and uses one revision-2
  report for every representation: shipping archive storage, each alias's own
  logical ROM/class/save ID, exact shared visual-bank/allocation usage, source
  section sizes and largest linked routines. The old entry-only CLI adapter is
  removed. `--static` needs no Node; default inspection profiles the real whole
  game once with the ordinary default seed and reports completed/faulted/
  unavailable evidence, observed member counts and unprofiled members. Startup
  parameter errors or missing Node remain explicit alongside static metadata,
  never a fabricated completed profile. Tests compare large source/raw/PNG
  reports, core-vs-CLI visual usage, real transition profiles against headless
  traces, unexecuted children, fault/unavailable cases and invalid unused-member
  rejection. The full gate passes 275 runtime, 125 core, 112 native, 32 CLI
  integration and one CLI report-unit test, formatting/lint/types/all-feature
  Clippy/builds, nine unchanged raw/PNG pairs and both packaged Linux verifications
  without PATH. The 3D conformance consumer now explicitly selects the current
  graph's main model and retains its real release/debug gameplay assertions.
  Graph and normal analysis adapters are connected; final cross-host
  and product audit remain open.
- CLI `graph` now emits a shared core compiler model for every game member, with
  explicit alias/save-ID scope instead of treating overlapping file/symbol/node
  IDs as global. Source drafts keep invalid child diagnostics and exact original
  CRLF offsets without packing or executing code. Raw/PNG inputs first pass
  authoritative whole-game admission and produce the same model. New core/CLI
  regressions cover two-member scope, invalid child source, duplicate IDs,
  identical source/raw/PNG results and all 32 members without Node. All four core
  tooling tests, three CLI graph integrations and all-feature workspace Clippy
  pass. Bundle-wide
  normal `info` analysis and final cross-host audit remain open.
- Native package export now admits the complete source/raw/PNG game before any
  output is created, preserves exact shipped bytes and selects main's package
  identity even from a child editor. Metadata records whole-game SHA-256 and every
  member's own ROM size/save ID. Core tests cover large games and Linux/Windows/
  macOS layouts (not cross-OS execution), rejection of rehashed foreign JavaScript
  and refusal to overwrite. CLI and Studio integrations cover all three input
  forms, unsaved child source, unused-member rejection and selected-child identity.
  The production package verifier now fingerprints every display frame, PCM,
  routing timeline and producer-owned save commit rather than only the final
  framebuffer, and exercises a >1 MiB two-member transfer game without PATH.
  Both members produce 60 frames/save commits and non-silent PCM in that 120-frame
  Linux package run. Inspected `output/native-package-qa-zhnM9X` captures show the
  exported executable launched without arguments from `/tmp` with PATH empty,
  the native pause action entering arena, and SOURCE opening `arena/src/main.modl`
  in a two-member Studio workspace. No source or user save files were changed.
  The full gate passes 275 runtime, 124 core, 112 native and 27 CLI tests,
  formatting/lint/types/all-feature Clippy/builds, nine unchanged raw/PNG pairs
  and both 120-frame packaged Linux runs without PATH. The capture session ended
  with external X11 window destruction (winit reported a TranslateCoordinates
  error); this does not demonstrate graceful normal-window closure or device QA.
  Bundle-wide analysis and the final cross-host/device audit remain open.
- PNG export/import now carries an admitted complete game, with separate shipping
  wrapper and logical-ROM limits. Core rejects CRC-correct invalid/obsolete payloads
  and computes CRC over slices without another payload-sized temporary buffer.
  Studio and CLI share main's presentation metadata and preserve ordinary PNG
  encoding. New core/CLI/native tests exercise >1 MiB games (each logical member
  remains <1 MiB), byte-identical shared export, exact source/raw/PNG headless traces
  across transfers, child-selected export identity, and actual native PNG launch/
  transition. The full gate passes 275 runtime, 122 core, 111 native and 26 CLI
  tests, formatting/lint/types/all-feature Clippy/builds, nine unchanged raw/PNG
  pairs and 120 packaged Linux frames without PATH. Desktop package export and
  bundle-wide analysis remain open.
- Native GAME CARTRIDGES now imports exact local source/packed members, creates
  blank members with unique IDs and confirms reference-safe removal through shared
  composition validation. Composition changes require stopped execution and valid
  applied buffers; Save still targets only the enclosing game. Removed aliases
  lose cached source/asset history and breakpoints so re-import cannot resurrect
  old edits. Three new Studio integrations verify exact source/packed imports,
  unchanged input files, duplicate/nested/running/draft/reference rejection, blank
  creation/removal and stale-history isolation. All 110 native tests and workspace
  Clippy pass. Inspected native captures at 1280×800 and 900×600 show add, successful
  remove, reachable confirmation controls and the qualified refusal for a live
  source reference. Whole-game export/analysis and cross-host audit remain open.
- Native bridge startup preflight failures no longer race the first Load command:
  the worker keeps its command receiver alive and replies with the actual missing-
  runtime diagnostic (or a normal Stop acknowledgement). A regression drains the
  startup error before submitting repeated Load/Stop commands, deterministically
  reproducing the formerly timing-dependent closed-channel failure. All five
  bridge tests and warning-free workspace Clippy pass.
- Shared composition operations now preflight local member addition/removal before
  returning changed source. Three core tests cover exact untouched source/metadata,
  nesting/alias/save-ID/invalid-code rejection, member 32 versus 33, and safe removal
  across retained modules, MODL tests and siblings. Computed request targets refuse
  removal conservatively; comments/ordinary text/transfer parameters do not count.
  These pure operations never touch disk or delete player saves. All 120 core unit/
  integration tests and core Clippy pass; native composition controls are next.
- Native source debugging now qualifies breakpoints/events by alias and original
  module, using an immutable per-member source snapshot from the admitted launch.
  Same-path/same-line breakpoints cannot hit a different cart; unsaved line edits
  do not move running locations. The debugger shows the actual running excerpt
  and opens the correct editable member/module only when its bytes still match;
  changed drafts stay intact. System-menu source inspection selects the active
  member. Restore clears stale source/watch inspection. Three new tests cover
  conditional child breakpoints after unsaved line shifts, draft-safe navigation,
  active-member source inspection and UTF-8 byte/cursor boundaries. All 106 native
  tests and warning-free workspace Clippy pass. Real native debug/source screenshots
  show the alias-qualified row, running excerpt and navigation control.
- Studio's game-member picker now provides independent embedded source/manifest/
  asset views while Save, Check and Run retain the complete game. Switching keeps
  unsaved code (including compiler-invalid drafts), exact untouched child source,
  per-member source selection and alias-scoped asset undo/redo; malformed raw JSON
  or unapplied manifest drafts block switching safely. Running titles come from
  the admitted active member rather than whichever cartridge is being edited.
  Two new integration tests exercise these behaviours; all 103 native tests pass.
  Inspected real native screenshots at 1280×800 and 900×600 show both member views,
  and Run from the selected child starts main. The complete gate passes: 275
  runtime, 117 core and 25 CLI tests, all format/lint/type/Clippy/build checks,
  nine unchanged raw/PNG pairs and 120 Linux packaged frames without PATH.
  Alias-qualified source breakpoints/navigation, composition controls and complete
  export remain open; the multi-cart acceptance row remains open.
- Native Studio/player now prepare every member through shared Rust and load one
  whole-game host command, without an entry-only fallback. Frames carry global and
  local time, producing alias/ID and next active context; snapshots bind the whole
  game. Studio preloads every admitted ID's bounded disk save and writes successful
  release commits to the admitted producer, never the editable project's current
  ID or a newly active target. Six new Rust process/Studio tests exercise release
  and source-stepped transitions, held-input release barriers, deferred target boot,
  exact replay recording, cross-cart checkpoint restore, same-path/same-symbol-ID
  debugger watches, save preload/isolation and invalid unused members. Bundle
  editing, alias-qualified source navigation and complete export remain open.
- Deferred loads now publish their separate boot framebuffer before frame zero in
  release and full-frame debug execution. Two real compiled MODL regressions
  reproduce the original failure, compare eager/loaded pixels, commands and work,
  and verify checkpoint replay. Capture resets also prevent restored boot events
  from mutating a previously returned debug trace. All 274 runtime tests (42 files),
  including the 41 targeted scheduler/control/hardware tests, lint/types and rebuilt
  production hosts pass.
- CLI `pack`/`check`/`watch`/`info --source` and headless play now admit whole games
  through shared Rust preparation and the production scheduler. Three new CLI
  integrations exercise a shipping bundle larger than one ROM (each member still
  <1 MiB), source/packed equivalence, exact per-ID save values, all 32 real MODL
  members, failed-frame save/transition isolation and invalid unused dependencies
  without Node. The full gate passes: 272 runtime tests, 117 core unit/integrations,
  95 native tests, 25 CLI integrations, all format/lint/type/Clippy/build checks,
  nine reproducible raw/PNG pairs and 120 packaged Linux frames without PATH.
  Current headless envelopes/results
  are revision 2, with member-tagged output, whole-game identity/state, per-ID save
  hashes and a separately identity/mode-bound snapshot hash. State assertions
  exclude only identity/mode so embedded tests need no self-referential source
  hash; canonical typed bytes use tagged hex instead of huge temporary arrays.
  The replay audit proves all original cartridge bytes, input hashes, work peaks,
  final/checkpoint pixels, audio commands and PCM unchanged; only the three state
  hashes are refreshed for the new whole-game representation. Native adapters,
  bundle-aware editing/debugging/export and final cross-host QA remain open.
- Scheduler/host-handoff checkpoint passes the complete `scripts/check.sh` gate:
  270 runtime tests, 117 core unit/integration tests, 95 native tests, 22 CLI
  integrations, formatting/lint/types, warning-free all-feature workspace Clippy,
  build, nine reproducible raw/PNG pairs and 120 Linux packaged frames with bundled
  Node and no PATH dependency. Existing cartridge/replay identities are unchanged.
  The multi-cart acceptance row remains open for actual CLI/Studio/host wiring.
- The host game loader validates the shared Rust payload, including every unused
  asset and save identity, before importing code; all member factories must exist
  before main boots. Eight tests cover real release/debug programs, per-alias
  debugger metadata, checkpoints across loads, invalid unused members, malformed
  source/byte transport, prototype-named file keys and pre-allocation aggregate
  data bounds. The loader/scheduler's combined 26 tests, lint and types pass.
  Native/headless command-handler integration is the next unfinished step.
- Shared Rust host preparation is implemented in `runtime_game.rs`: every admitted
  member is compiled in one mode with its own symbols/source map, declared data,
  manifest and logical ROM, plus the whole-game identity. Four tests verify both
  modes/all 32 members, exact ROM/asset transport, deterministic serialization,
  independently mapped identical module paths, source/comment identity binding,
  authoring-file exclusion and the existing 2 MiB compiled-source transport bound.
  All core tests and warning-free core Clippy pass. This is the common handoff
  for the pending native/headless adapter integration, not completion of that row.
- The production whole-game scheduler is implemented with 18 tests, including
  real release/debug MODL factories: completed-frame-only loads/reloads, deferred
  destination boot, one global frame/input timeline, explicit parameter transfer,
  per-ID committed saves, all-32-member cycling and checkpoint restoration.
  Compatible data reloads preserve state/tasks, RAM, RNG, ROM/rate/display,
  saves/menu and input history while resetting the asset bank and old voices.
  Failed candidate restore leaves a live source continuation untouched; faults
  cannot apply requests or persist their frame. All 262 runtime tests (41 files),
  rebuilt production hosts, lint/types and formatting pass. Host adapters,
  CLI/Studio authoring/debug integration and
  cross-host acceptance verification remain open; the multi-cart row stays open.
- A replayable input-owner barrier is implemented for scheduler integration:
  after a cartridge load, held buttons on all four ports and pointer presses stay
  blocked until release; unrelated fresh controls, hover and explicit menu events
  remain available. Four tests cover all ports, release/repress, defensive snapshots
  and malformed frames. Data reloads will retain this owner instead of resetting it.
- The typed request/snapshot checkpoint passes the full gate: 240 runtime tests,
  113 core unit/integration tests, 95 native tests, 22 CLI integrations, all format/
  lint/type/Clippy/build gates, nine reproducible raw/PNG pairs and 120 Linux
  packaged frames without PATH. Subsequent input-owner tests pass separately.
- Typed `load_cart`, `cart_parameter` and `reload_assets` now reach the production
  console's deferred request service; both compiler modes enforce their signatures
  and reject raster calls. Console snapshots are current revision 7 (machine 3),
  including request/context state and atomic rollback. Real MODL release/debug
  tests prove menu-driven requests remain pending during statement suspension,
  restore with VM state, and defer boot without rerunning start after restoration.
  Input hashes, work peaks, final/checkpoint framebuffers, audio command streams
  and PCM match all three original replays; only compiled-cartridge and complete
  state identity hashes changed for the new builtin table/snapshot schema.
  The actual multi-cart scheduler, host adapters and acceptance row remain open.
- Runtime request control is implemented and unit-tested: local aliases only,
  a 1,024-byte well-formed UTF-8 transfer parameter, one deferred operation per
  callback frame, pre-admitted compatible reload banks, work charged before
  mutation, and strict context-bound snapshots. Seven tests cover exact Unicode
  bounds, phase/arity/reference failures, work exhaustion and defensive restore.
  This request component is not yet the game scheduler/host integration; the
  multi-cart acceptance row remains open.
- Whole-game Rust packing/admission is implemented in `game.rs`: ordinary game
  bytes remain unchanged; a flat bundle uses the same canonical M01C/2 archive
  encoding with a bounded index, exact enclosing source and 2–32 ordinary current
  cartridges. Eight tests prove a 32-member source/packed round trip and reject
  hash-recomputed alias/source/build substitution, unknown/missing files, nested
  compositions and historical headers. A forged compiled member passes structural
  inventory checks but is rejected by authoritative whole-game reconstruction,
  without executing JavaScript. Concealed dependencies in entry-only archives
  are rejected too. Console ROM remains a single ≤1 MiB member.
  `docs/BUNDLES.md` distinguishes this completed core from pending runtime,
  CLI/Studio, replay/debugger and native integration; the acceptance row stays open.
  After this slice, the full gate passes: 230 runtime tests, 112 core unit/
  integration tests, 95 native tests, 22 CLI integrations, lint/types/format and
  warning-free all-feature workspace builds. All nine raw/PNG hashes are unchanged,
  existing replay goldens pass without refresh, and Linux's bundled player verifies
  120 frames with no PATH dependency.
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
