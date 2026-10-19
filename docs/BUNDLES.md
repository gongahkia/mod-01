# Local multi-cartridge games

Status: source preservation and whole-game packing/admission are implemented in
the shared Rust core. The production game scheduler implements boundary switching,
asset reload, per-ID saves, input ownership and whole-game checkpoints, with real
release/debug MODL tests. CLI `pack`, `check`, `info --source`, `watch` and
`run --headless` now retain/admit the whole game. Native Studio/player use the same
whole-game loader and scheduler, including per-ID disk saves and member-scoped
host debugger symbols. The workspace member picker edits embedded cartridges
without separating them into disk projects: switching flushes the visible source,
assets, manifest and per-member file selection; Save and Run always retain the
whole game and Run starts `main`, whichever member is selected for editing.
Asset history and source widget identities include the member alias, and invalid
asset/manifest drafts block switching without discarding text. Breakpoints, watches,
source events and source navigation are member-scoped, using the launched source
snapshot when authored code changes. MANAGE MEMBERS opens native GAME CARTRIDGES:
import a local workcart/packed cartridge or add a blank member with a distinct ID,
then edit it through the ordinary workspace. Stop execution before adding/removing
members. Removal requires confirmation, drops that member's editor history and
breakpoints, and never deletes imported files or player saves. Unsaved asset or
manifest drafts must be applied before composition changes. PNG and native desktop
exports retain the complete game with main's presentation identity. Bundle-wide
CLI analysis now distinguishes shipping storage from each member's ROM/asset
capacity, and compiler graphs keep separate member namespaces. Final cross-host
game/replay/debugger conformance is verified by the acceptance evidence below.
Desktop platform/hardware qualification and the whole-product final audit are
separate, still-open requirements.

## Authoring and admission

The enclosing `.m01w` remains the authoritative source. Its ordinary metadata,
modules, assets, files, recipes and tests define the `main` cartridge. Optional
flat `[[bundle]]` records embed additional complete current workcart strings:

```toml
[[bundle]]
alias = "arena"
source = '''
format = 2

[cartridge]
language = "MODL/1"
id = "my-game-arena"
title = "ARENA"
author = "@author"
version = "0.1.0"
entry = "src/main.modl"
update_rate = 60
hardware_revision = 2

[[module]]
path = "src/main.modl"
source = "on draw:\n  clear(7)\n"
'''
```

Aliases are 1–32 lowercase ASCII letters/digits/hyphens/underscores and start
with a letter. `main` is reserved for the enclosing workcart. All dependencies
share this flat catalog, allowing cycles between gameplay destinations without
recursive source/package embedding. Names are logical aliases, never host paths,
URLs or downloadable cartridge identifiers. There are at most 31 dependencies
plus main. No member can contain another bundle; all members use M01W/2,
M01C/2, MODL/1 and the same current machine.

Every member needs a distinct persistent cartridge ID. An alias is not save
identity: renaming one does not authorize another member to read its saves.
Unused members are still validated/compiled; missing or malformed dependencies
are admission failures, not deferred runtime downloads. `M014070` identifies
count/alias errors, `M014071` duplicate save IDs, `M014072` source resource bounds,
`M014073` forbidden nested/entry-only operations and `M014074` invalid packages.
Ordinary compiler/asset errors retain their codes and include the member alias.

Shared `workcart::add_workcart_member` and `remove_workcart_member` operations
preflight the whole candidate game before returning changed source. Addition
requires a fresh alias and a single flat cartridge; it never replaces a member.
Removal cannot remove `main` and preserves player saves. It checks retained
modules (including unused ones), stored MODL tests and sibling cartridges for
`load_cart`/`reload_assets` requests. Literal references prevent removal; computed
targets conservatively prevent it too (`M014075`). Comments, ordinary text and
transfer parameters are not destination references. Request-shaped calls are
checked conservatively using the shared lexer, not runtime string prediction.

`encode_workcart_with_bundle` and `rewrite_workcart` retain child source exactly,
including comments and CRLF. Asset deletion, map generation and CLI source
formatting retain dependencies too. No-op rewriting retains the entire enclosing
source unchanged. Actual root edits canonically rewrite root TOML without editing
the child strings. Helpers that intentionally create a fresh single cartridge
remain distinct from helpers that preserve a complete game.

## Packed representation

`game::pack_game_workcart` returns a validated `PackedGame` with complete source,
an immutable catalog, initial `entry()`, shipping bytes and game identity.
`game::decode_game` admits either an ordinary logical cartridge or its flat
composition using the same [current archive encoding](CARTRIDGE_FORMAT.md).
No JavaScript runs during admission. Members are ordinary ≤1 MiB `.m01c`
cartridges, not raw source passed directly to the runtime. The larger shipping
container never becomes the active console ROM.

The composition retains exact enclosing source and canonical inventories.
Admission reconstructs every member from that source and compares the complete
package. Rebinding aliases, changing embedded source without rebuilding members,
forging compiled code, recomputing inventories, adding unknown fields/files,
nested compositions and historical headers all fail. Single-cartridge bytes are
unchanged. Core tests exercise a complete 32-member round trip and these rejection
paths; the acceptance checks below cover gameplay/replay/debugger integration.

## Deferred request API

The compiler and production console implement these typed operations for an
admitted game scheduler:

```modl
load_cart("arena", "lives=2")
let parameter = cart_parameter()
reload_assets("bank")
```

`load_cart(Text, Text)` and `reload_assets(Text)` return Unit;
`cart_parameter()` returns Text. Calls cannot be used in `on raster`. A request
does not immediately abandon its callback or execute target code. Only one
load/reload may be queued per logical display frame; a second request faults
instead of silently replacing the first. Only declared aliases are valid.
Reload targets must have been admitted as compatible banks by the scheduler.

Transfer parameters are well-formed UTF-8, up to 1,024 bytes, including empty
text. `cart_parameter` charges `1 + parameterBytes`; `load_cart` charges
`1 + aliasBytes + parameterBytes`; reload charges its pre-admitted bank-copy
cost. These are additional to ordinary compiler call costs. Work is charged
before a request is queued, so a budget failure cannot schedule a transition.
`M019017` identifies invalid aliases/parameters, incompatible banks, duplicate
requests, forbidden phases or a missing admitted scheduler.

Current console snapshots are revision 7 and carry this context-bound pending
request state (or null for a console without a game service). Restoring a
snapshot restores both the machine and request, rolling both back on validation
failure. Live statement suspensions remain non-serializable: a request may be
observed during stepping, but it cannot be saved/applied as a completed frame
until that frame finishes. An internal deferred-boot construction path prepares
target consoles without executing start; restoring an existing machine does
not run start again. These behaviours run through actual release/debug MODL
compiler output in `cartridge-control-conformance.test.ts`.

## Frame, input and save ownership

[Relay Station](RELAY_STATION.md) is the maintained playable three-member game
covering these operations, with a source-readable complete replay route.

`GameRuntime` owns one admitted catalog and one monotonically increasing display
frame/input timeline. Results identify the cartridge alias, persistent ID and
local frame that produced them, even when the next cartridge is already prepared.
Requests apply only after a successful completed frame, never while stepping a
statement, at the debug boot boundary, or after a fault. The destination is
prepared without running start; start runs on the next execution request.

A load starts fresh state, tasks, RAM, framebuffer, menu, voices and local time,
with the game's original seed and explicit transfer parameter. Only committed
save images carry over, keyed by the member's distinct persistent cartridge ID.
The old cartridge's final frame/save commit still belongs to that old ID. A
faulted frame neither applies its pending request nor updates the game's committed
save catalog. Saves latched during start are delivered after its first successful
frame, not implicitly persisted by taking a checkpoint.

Held controls on all four ports and both pointer presses are quarantined at a
load until release. Hover and unrelated new controls remain available. Debug
boot stepping cannot accidentally release this barrier, and later statement-step
messages cannot replace the first input sampled for an active logical frame.
Explicit pause-menu activations remain explicit replayable events.

## Asset-only reload

A bank is compatible when it has exactly the same declared asset names/kinds
(or matching query-only map names), and the combined configuration fits the
ordinary runtime input limits. The active program, ROM, update rate, work budget,
parameter and display configuration remain its own. The donor contributes only
the declared visual/audio assets; it is not booted. RAM, state/tasks, RNG, save
device, menu, local time, current framebuffer and input history survive. Mutable
asset bytes return to the donor's admitted source; old voices/tracker reset.
There is no input quarantine on a data-only reload. Reloading the active alias
restores that cartridge's original asset bank.

Reload work is `1 + ceil((1048576 + 8192*24 + 4096*32 + audioAssetCount*32)/64)`:
the fixed visual bank, allocation table, descriptor table and donor audio
descriptors. This synthetic copy/reset cost is charged before queuing the request,
within the current frame's normal 50,000-unit ceiling.

Whole-game checkpoints use revision 1 around the current console snapshot. They
bind the whole packed-game identity, original seed and compiler mode, active
alias/parameter/bank, every committed save image and input-owner barrier. Restore
constructs a deferred candidate, validates/restores all its devices and only then
replaces the live game. Invalid restoration leaves even a suspended source
continuation intact. Console-only checkpoints cannot substitute for these game
checkpoints. `game-runtime.test.ts` exercises actual compiler output in both modes,
replay input encoding, loads/reloads, tasks, faults, save isolation, 32 members,
counter bounds and restoration failures.

The headless and native Studio/player adapters use this scheduler for every game.
Whole-game PNG export/import is available in Studio and CLI, including shipping
games larger than a single ROM. Source/raw/PNG headless replay results match through
actual transitions and native PNG launches use the same whole-game scheduler.
Native desktop export packages these same complete game bytes and records main's
identity plus every member's logical ROM size and persistent ID. CLI `info` now
admits/analyzes every member, then optionally profiles the actual whole-game
timeline instead of independently booting children with invalid parameters.
`--static` inspects without Node. The graph command uses member-scoped compiler
models; valid source/raw/PNG games produce identical graphs and analysis reports.

`runtime_game::prepare_runtime_game` provides the shared Rust-to-host handoff
from a privately admitted `PackedGame`. It compiles **every** member in the chosen
release/debug mode and retains each alias's own symbols, source map, manifest,
declared asset/display bytes and canonical ≤1 MiB logical ROM. Asset bytes and
ROM use base64 transport; authored source/artwork/auxiliary files are not module
imports. The whole-game identity and compiler mode are attached once to this
internal revision-1 host payload, not a new cartridge format. Headless and native
adapters consume this same handoff rather than rebuilding an entry-only request.
Compiled JavaScript has an inclusive 2 MiB-per-member transport
bound, matching the existing native host limit; overflow is `M014072` and is
alias-qualified. This helper does not execute JavaScript or boot any member.

`game-loader.ts` reads that one payload shape for host integration. It validates
all members' configuration/assets/save identities before importing any member,
then waits for every module factory before booting main. Base64 must be canonical,
including padding bits; per-file and aggregate member byte bounds are preflighted
before decoding. The existing 16 MiB data-source bound applies per member; source
maps are bounded to 128 MiB and total code/map/data/ROM transport to 512 MiB.
Metadata is retained separately per alias, so identical source paths/symbol IDs in
different cartridges cannot silently share one debugger map. Tests exercise real
release/debug compiler output, unused invalid dependencies, deferred boot,
prototype-named paths, metadata copying and malformed/bounded transport. This
loader is connected to both headless execution and the native command handler.
CLI integration tests pack/run a shipping game larger than one ROM,
cycle all 32 real MODL members, inspect whole source without Node, verify exact
per-ID save values, and reject invalid unused members before starting Node.

## Acceptance evidence

The multi-cartridge feature is implemented and verified, not only exposed by APIs:

| Contract                                                                         | Reproducible evidence                                                                                                                                                                                                                                                                                                                                                                      |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Complete local dependencies, source retention, limits and invalid unused members | [Rust game admission](../crates/modl-core/src/game.rs), [bundle tests](../crates/modl-core/src/bundle.rs) and [CLI integration](../crates/mod01-cli/tests/cli.rs) cover exact source/raw/PNG round trips, 32 members, each ROM's capacity and whole-catalog rejection before execution.                                                                                                    |
| Frame-boundary transitions, explicit parameters and asset-only reload            | [Real MODL request conformance](../packages/runtime/src/cartridge-control-conformance.test.ts) and [game scheduler tests](../packages/runtime/src/game-runtime.test.ts) exercise release/debug code, held-input quarantine, deferred boot, compatible banks, retained RAM/tasks/RNG/time/menu, donor non-execution and reset voices.                                                       |
| Separate save identities, producer ownership, faults and restore                 | The same scheduler tests plus [Studio game tests](../crates/mod01-native/src/studio_game_tests.rs) cover all 32 save identities, failed-frame non-persistence, preloaded disk saves, disposable replay/debug commits and transactional checkpoint rejection.                                                                                                                               |
| Native member authoring and debugger routing                                     | Studio tests cover member-scoped drafts/history, safe import/removal, main-only entry selection, frozen source maps, overlapping source/symbol IDs, watches, breakpoints and active-member source inspection.                                                                                                                                                                              |
| End-to-end representations and packaged execution                                | [Relay Station verification](../scripts/verify-relay-station.mjs) compares all 110 frames' pixels, PCM, commands, work and producer cursors, then final canonical state/checkpoint and separate saves across CLI/native source/raw/PNG and the actual standalone executable with empty PATH. This has passed on Linux; CI also defines Windows/macOS runs, which are not locally verified. |
| Actual Studio replay and source stepping                                         | `relay_station_studio_source_steps_and_night_bank_checkpoint_replay_match_release_frames` runs the maintained game through all 110 release/debug frames, restores the live night-bank checkpoint at frame 23, and reaches the same canonical final checkpoint without persisting saves or changing source.                                                                                 |

These checks are in the full quality gate. Linux GUI inspection additionally
covered actual day/night sprite changes, full minimum player/touch layouts and
SOURCE reopening the active vault member at Studio's minimum size. This does not
claim physical touchscreen/controller/audio or other-OS execution qualification.

The 32-member model and separate loading/data-reload requirements were researched
against the primary [PICO-8 manual](https://www.lexaloffle.com/dl/docs/pico-8_manual.html),
sections 3.4 and 6.1/6.7. MOD-01 does not import PICO-8 cartridges or add online/BBS
access.
