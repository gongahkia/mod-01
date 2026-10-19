# Native consolidation acceptance audit

Audit date: 2026-10-19. Tested code: `fe2eefb889312ffb82be43d3636c4b6ea2027d4b`.
All scoped requirements in the [implementation ledger](NATIVE_IMPLEMENTATION.md)
are complete. The desktop matrix now verifies actual Windows/macOS/Linux window
startup, the first real frame/save and ordinary shutdown in addition to complete
game/package equivalence. The platform receipt and hardware limitations below
define the scope of that qualification.

## Decision authority

The input is the user's exported `mod01-review-2026-10-18.json`, review ID
`mod01-pico8-2026-10-18-d9918f7`, exported `2026-10-18T16:20:21.999Z`.
Its SHA-256 is
`e91b17102f048f21adaab467d51c21263ec1e3188b974fb0d16bd51961839ba7`.
It contains 21 decisions: 17 approved, three rejected and one discussion, with
notes that override the proposals. The subsequent explicit browser-removal
instruction also applies. No feature-priority choice is outstanding.

The first rejection is not permission to keep the old packing behaviour: its
note explicitly requests one current format with no historical compatibility.
The discussion item requests documented, tested 2D/3D games, not recruitment of
a creator community. Touch is consequently implemented for native touchscreen
desktops, not a retained phone/browser player.

## All 21 original decisions

“Local proof” means the current sources and Linux execution below demonstrate
the behaviour. It does not imply physical-device or other-OS execution.

| Original review ID         | Applied decision                                           | Evidence and qualification                                                                                                                                                                                                                 |
| -------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `consistency-workcart`     | Reject old/new compatibility; use the one current contract | [Format proof](#format-and-native-only-proof): identical complete source/game packing, recipes/tests/auxiliary files and embedded members; old formats and migration commands rejected.                                                    |
| `consistency-contract`     | Approve current-only documentation                         | [Machine limits](LIMITS.md), [hardware](HARDWARE.md), [assets](ASSETS.md), [API](API.md), runtime constants and compiled palette conformance agree on the current machine. Numeric protocol tags do not select historical profiles.        |
| `language-math`            | Approve deterministic game maths                           | [Standard-library proof](#standard-library-proof): actual compiled assertions, defined domains/error bounds/work charges, independent numerical checks and restore replay.                                                                 |
| `language-formatting`      | Approve conversion and formatting                          | The same proof covers explicit bases, padding, fixed decimals, safe whole-input parsing and typed failure results without locale-dependent output.                                                                                         |
| `language-text`            | Approve bounded text helpers                               | The same proof covers Unicode scalar indexing/slicing/conversion, bounded splitting/padding and allocation/work failures.                                                                                                                  |
| `language-bits`            | Approve explicit bit semantics                             | The same proof covers operators and typed calls, signed/unsigned 32-bit words, precedence, shift limits and compile-time/runtime agreement.                                                                                                |
| `authoring-repl`           | Approve immediate scratchpad                               | [Native authoring proof](#native-authoring-proof): real compiler/child execution of expressions/drawing, faults, copied assets, cancellation and disposable saves.                                                                         |
| `native-code-workflow`     | Approve integrated editing baseline                        | The same proof covers find/replace, navigation, block shortcuts/undo, compiler-backed completion/declarations/diagnostics and code/state graph.                                                                                            |
| `native-tracker`           | Approve full eight-channel tracker                         | The same proof covers focused keyboard entry, patterns/orders, clipped clipboard blocks, transpose, audition/transport and actual native synth execution.                                                                                  |
| `export-native-games`      | Approve desktop packages; remove web exports               | [Export/workflow proof](#export-and-workflow-proof) and [desktop qualification](#desktop-qualification-receipt): Windows/macOS/Linux Rust/package checks and actual window/frame/save/normal-close probes pass.                            |
| `export-raspberry-pi`      | Reject                                                     | No Raspberry Pi or ARM-handheld qualification/delivery is claimed or added. Desktop macOS ARM64 is not a Pi/handheld target.                                                                                                               |
| `graphics-region-blit`     | Approve                                                    | [Graphics proof](#graphics-proof): independently checked source/destination sampling, clipping, flips, transparency/remaps and bounded atomic work failure.                                                                                |
| `graphics-textured-lines`  | Approve                                                    | The same proof covers every line octant, wrapping/skipping, fixed texture precision, clipped sampling and calibrated work bounds.                                                                                                          |
| `graphics-pattern-fills`   | Approve                                                    | The same proof covers two-colour screen-anchored pattern state, transparent holes, primitives, MMIO and reset/restore.                                                                                                                     |
| `graphics-transparency`    | Approve                                                    | The same proof covers all 256 original-index transparency entries before remapping, defaults, reset and graphics checkpoint replay.                                                                                                        |
| `player-touch-controls`    | Approve, adapted to native-only scope                      | [Player/library proof](#player-and-library-proof): actual multi-touch events feed ordinary recorded inputs; chords/taps compare real pixels/work/PCM. Physical touchscreen quality is unclaimed.                                           |
| `player-system-menu`       | Approve common typed pause menu                            | The same proof covers real 30 Hz compiled actions, frame-boundary pause, source stepping, in-flight requests, faults, replay and neutral action input.                                                                                     |
| `cartridge-multi-cart`     | Approve local bundle/load/reload                           | [Bundle acceptance](BUNDLES.md#acceptance-evidence): complete catalog admission, 32 members, parameters, fresh full loads, state-preserving asset reload, per-ID saves, frozen member debugger routing and complete source/raw/PNG/export. |
| `discovery-local-library`  | Approve offline browsing                                   | [Player/library proof](#player-and-library-proof): persistent labels/search/favourites/recents, controller/keyboard navigation, bounded scanning and launch/source inspection without losing dirty work.                                   |
| `discovery-public-catalog` | Reject                                                     | No hosted catalog, submissions, accounts, backend or public-discovery service. The library is local.                                                                                                                                       |
| `community-learning`       | Discuss; implement the explicit game/example note          | [Learning proof](#learning-proof): maintained playable 2D games, a tested public-API software-3D game, remix examples and a first-game guide. No claim to have created a third-party community.                                            |

## Format and native-only proof

The [Rust workspace](../Cargo.toml) contains core, CLI and native Studio only;
the [package scripts](../package.json) run native development and package a
desktop executable. The former browser application, Wasm bridge, web workers,
PWA and HTML/web-ZIP exporter are absent. The retained TypeScript runtime is the
single Node-hosted console implementation, not a browser frontend.

[CLI integration](../crates/mod01-cli/tests/cli.rs) explicitly rejects HTML/ZIP
exports, old directory authoring and migration commands. It creates the same
single workcart through file/directory commands and checks complete workcart
pack/export/run. [Workcart](../crates/modl-core/src/workcart.rs),
[game](../crates/modl-core/src/game.rs) and
[Studio project tests](../crates/mod01-native/src/project.rs) cover exact authored
source, recipes/tests, auxiliary files, layered assets and whole-game round trips.
Corrupt/old/nested containers are rejected without a fallback decoder.

`.m01w` is editable source; `.m01c` is its current source-retaining compiled
distribution representation; PNG is a transport wrapper for those same bytes.
They are not separate historical machine versions. See [M01W](M01W.md) and
[cartridge encoding](CARTRIDGE_FORMAT.md).

## Standard-library proof

[Compiled conformance](../packages/runtime/src/stdlib-conformance.test.ts) compiles
the actual math/text/bits/palette MODL assertions in release and debug modes and
executes them through the shared console, headless host and restricted native
child. Pixels/work and snapshot replay must agree; palette assertions exercise
all 256 colours.

[Math](../packages/runtime/src/math.test.ts),
[text/conversion](../packages/runtime/src/text.test.ts) and
[bits](../packages/runtime/src/bits.test.ts) additionally test independent
references, edge domains, Unicode, malformed inputs, costs and failure behaviour.
The [standard-library contract](STANDARD_LIBRARY.md) specifies these semantics;
fixed arithmetic algorithms are not promises of exact symbolic real arithmetic.

## Native authoring proof

[Source editor tests](../crates/mod01-native/src/source_editor.rs) drive the actual
egui text widget through shortcuts, focus, search/selection, completion acceptance
and undo. Other tests check UTF-8 source spans, imports/assets, declarations,
function/line navigation and minimum-size diagnostics. The graph uses the
compiler's model, not a hand-authored scene engine.

[Playground tests](../crates/mod01-native/src/playground.rs) compile and execute
real expressions, drawings and faults through the native bridge, preserving the
ordinary project/assets/saves. History, cancellation and layout are tested.
[Tracker tests](../crates/mod01-native/src/tracker.rs) drive the real grid and
round-trip an edited eight-channel song into the actual native synthesizer.
[Asset workbench tests](../crates/mod01-native/src/asset_tools.rs) cover undo,
metadata preservation and focus without rounding authored volume values.

## Graphics proof

[Region blits](../packages/runtime/src/graphics-region.test.ts),
[texture lines](../packages/runtime/src/graphics-texture.test.ts),
[patterns](../packages/runtime/src/graphics-pattern.test.ts) and
[graphics state](../packages/runtime/src/graphics.test.ts) check pixels and work,
including boundary/failure/reset/snapshot cases, not just API presence.
[Compiled visual conformance](../packages/runtime/src/hardware-conformance.test.ts)
executes real MODL graphics calls in both compiler modes and compares the actual
native/headless hosts. The maintained games exercise these public facilities.

## Player and library proof

[Touch tests](../crates/mod01-native/src/touch_controller.rs) feed real egui
multi-device/finger events, cancellation, short taps, resize/focus loss and
pause quarantine. A compiled cartridge compares replayed native output and PCM.
[Compiled menu conformance](../packages/runtime/src/menu-conformance.test.ts),
[pause UI](../crates/mod01-native/src/pause_menu.rs) and
[Studio menu tests](../crates/mod01-native/src/studio_menu_tests.rs) check the
actual pause/action/debug/input lifecycle.

[Library tests](../crates/mod01-native/src/library_tests.rs) exercise the actual
worker, persistent metadata, minimum-size keyboard/controller UI, safe dirty-work
choices and source/raw/PNG launch through Node. [Metadata storage](../crates/mod01-native/src/library_store.rs)
tests cover stale edits, locks, linked paths and bounded files. External cover
resources do not introduce a network browser.

## Export and workflow proof

[Studio game tests](../crates/mod01-native/src/studio_game_tests.rs) contain the
coherent edit → debug → record → save → export → reopen → release-replay test.
The unsaved root edit to colour 200 is visible while a child is selected. Both
members source-step, each accepted input records once, the previous workcart is
recoverable, and the exported bytes equal the edited whole game. Reopening those
bytes and the written recording reproduces every frame's pixels/work/member and
never persists replay saves. This test uses header fixtures only for toolchain
layout: it does **not** claim execution of those fixture executables.

Actual execution is covered independently by
[native package verification](../scripts/verify-native-game.mjs): 120 frames from
an ordinary game and a generated >1 MiB two-member game, actual PCM/parameters/
save ownership and exact packed source, launched from an unrelated directory
with empty PATH. The main ID `node` cannot overwrite the packaged runtime.
[Relay Station verification](../scripts/verify-relay-station.mjs) compares the
maintained 110-frame route across CLI/native source/raw/PNG and the actual package,
including all per-frame pixels/PCM/commands/work/routing, final canonical
checkpoint/state and distinct member saves. Neither check initializes a GUI or
physical audio device.

[Core export tests](../crates/modl-core/src/native_export.rs) additionally reject
uncompiled/rehashed foreign code, invalid unused members, mismatched binary
headers/architecture and existing output before replacing anything. Linux,
Windows and macOS layouts are tested, but layout is not execution evidence.

## Learning proof

[Maintained cartridges](../cartridges/README.md) include 2D action, exploration,
rhythm, API showcases and multi-cart Relay Station.
[Golden cartridge replays](../packages/runtime/src/bundled-cartridges.test.ts)
and CLI integration exercise real source and machine output. The
[software-3D test](../packages/runtime/src/software-3d.test.ts) completes Prism
Corridor's 416-frame winning route in release/debug, tests collisions/objectives,
save/audio/texture output and restoration/native checkpoints. It is ordinary
MODL software rendering, not a privileged scene engine.
[Software 3D](SOFTWARE_3D.md), [Relay Station](RELAY_STATION.md),
[examples](../examples/README.md), [starters](../templates/README.md) and the
[first-game tutorial](TUTORIAL.md) document controls, mechanisms and remix paths.

## Local execution receipt

The local full-gate baseline below is `7060d9b`; the later cross-platform
receipt records the additional GUI harness tests at the current tested code.

Linux/WSL, Rust 1.98.0, Node 24.21.0 and pnpm 10.32.1:

```sh
./scripts/check.sh
pnpm native:package
node scripts/verify-native-game.mjs target/release/mod01-studio
node scripts/verify-relay-station.mjs target/release/mod01-studio
```

This host needs `PKG_CONFIG_PATH=/usr/lib64/pkgconfig:/usr/share/pkgconfig` for
its installed Fedora desktop libraries; that is environment setup, not a portable
repository default. The full gate at the tested commit passed formatting, lint,
types, production host builds, 322 runtime/script tests in 43 files, 134 core
tests, 151 native tests, 34 CLI integration tests plus one CLI unit test,
all-feature Clippy/builds, ten deterministic raw/PNG cartridge pairs and all three
Linux package comparisons.

The fresh standalone GUI package was launched without arguments from `/tmp`,
with empty PATH and isolated XDG config/cache/data. On Xvfb/X11 with software GL,
inspection covered pause-menu docking, live day/night asset replacement, the
480×320 player, and SOURCE selecting `vault/src/main.modl` while expanding to
900×600 Studio. Separate 8 KiB save files reported owners 1/2. After a normal
close/relaunch, station visits increased 4 → 5 and vault visits remained 3;
both retained their clear counts. Both X11 sessions exited normally with status 0.
The maintained source and exported game remained byte-identical; the game SHA-256
was `0515cc533739fc5012dc2e8dc9494de11ba1344b54d1917ef58115882ab6837f`.
Earlier native GUI inspections also covered source/tools/library, tracker edits,
save recovery and reference-safe deletion; their chronological records are in
the implementation ledger.

Local receipts are intentionally ignored under `output/`, not artifacts available
from a clean clone: the current full-gate, release packaging/verifier and GUI
receipt directory is `output/native-acceptance-qa-4BBzhi`. Reproduce the commands
above rather than treat these local paths as a remote CI pass.

## Desktop qualification receipt

[CI run 39266764189](https://github.com/gongahkia/mod-01/actions/runs/39266764189)
at the tested code passed all four jobs: the full repository gate and native
Windows, macOS and Linux builds. The full gate passes formatting, lint, types,
production builds, 329 runtime/script tests in 45 files, 134 core tests, 151
Linux native tests, 34 CLI integration tests plus one CLI unit test, all-feature
Clippy, ten deterministic raw/PNG pairs and the Linux package comparisons.
Platform-specific native tests also pass on Windows and macOS.

Every desktop job launches the real ordinary and >1 MiB multi-member exported
games and verifies their frame/PCM/routing/save results. Each also compares the
complete maintained Relay Station route across CLI/native source/raw/PNG and
the actual standalone package. The [actual-window probe](../scripts/verify-native-window.mjs)
then verifies a no-argument, empty-PATH launch, a visible owned OS window, the
first real committed frame/save and a normal close with exit status 0:

| Platform       | Visible window | Qualification                                |
| -------------- | -------------- | -------------------------------------------- |
| Linux x86-64   | 960×650        | Xvfb/X11 with software GL                    |
| macOS ARM64    | 960×674        | Hosted runner's actual WindowServer session  |
| Windows x86-64 | 976×689        | Pinned Mesa 26.2.4 llvmpipe, CI-only staging |

The Windows `mod01-gui-qa-windows-latest` artifact contains the successful
`report.json` and `window-ready.png`. The inspected capture shows the actual
colour-200 frame, colour-7 rectangle, native HUD and cursor, rather than the
unchanged blank template. The report confirms the real 8192-byte save. LF/CRLF
fixture regressions prevent a missing initializer from silently testing the
wrong program. Earlier failing runs are superseded, not counted as passes.

The user has authorized pushing and merging the feature branch. This receipt
qualifies the named feature commit; merge/main status must be checked against
the eventual main commit, not inferred from this run. CI uploads Studio archives
and separate GUI evidence; this work does not publish a signed release.

Physical GPU, touchscreen, controller and speaker quality are not claimed by
these hosted/virtual-display checks. Signing/notarization, Pi/handheld delivery
and hosted discovery remain outside the agreed scope. Windows users still need
an OpenGL 2.0+ capable installed driver; ordinary distributed packages do not
ship the CI-only Mesa DLLs.
