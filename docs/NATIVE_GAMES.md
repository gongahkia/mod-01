# Native games

Studio and the standalone game surface use the same executable, compiler, Node
child-process bridge, indexed display, input, audio, save location and source
inspector. A game is not a browser window or a second implementation of MOD-01.

From a checkout:

```sh
pnpm --dir packages/runtime build
cargo build -p mod01-native
target/debug/mod01-studio --play cartridges/pocket-relay
```

`--play` opens the game surface. Escape or PAUSE suspends frame submission at a
frame boundary; RESUME continues without advancing cartridge time while paused.
RESTART reboots with the cartridge's persisted save. SOURCE stops the game and
opens its authored modules/assets in Studio. No network service is involved.
`--studio PATH` explicitly opens editable/source inspection mode.

When `game.m01c` is beside the executable, launching without arguments opens that
game. The native packager supplies the executable, canonical cartridge, runtime
bundle and target-platform Node executable together. The executable finds runtime
files relative to itself, independent of the current working directory.

## Export a game

```sh
cargo build -p mod01-cli -p mod01-native
target/debug/mod01 export native cartridges/pocket-relay --output dist/pocket-relay-game
```

Use a **new** output directory. Linux/Windows produce a game executable directory;
macOS produces an application bundle inside the output directory. All include
`game.m01c`, Node, the shared runtime and font/cursor/Node notices. No Node install
is needed on the recipient's machine. Native system libraries and OS trust/signing
rules still apply; build on the oldest supported OS baseline. Linux CI builds on
Ubuntu 24.04. These are unsigned publisher artifacts, not notarized installers.

The player executable is named `cart-ID` (`cart-ID.exe` on Windows), separate
from reserved package files such as `node`, `game.m01c` and `mod01-game.json`.
This namespace also keeps the executable out of Windows's
[reserved device basenames](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file),
which remain reserved even with extensions. The cartridge's actual ID, save
identity and title do not change with its executable filename.
Dots in IDs are encoded as underscores in filesystem stems, making trailing-dot
IDs portable too; IDs cannot contain underscores, so this mapping is injective.
The default output folder is `dist/cart-ID-native` (or Studio's chosen parent),
and macOS's app directory is `cart-ID.app`. Packed/PNG default names use the same
stem. These are filename rules, not a cartridge-format change or ID migration.

Source workcarts, raw `.m01c` and cartridge PNG inputs all preserve the **complete
game**, including every embedded dependency and its exact authored source. Core
revalidates/recompiles every member before creating the output directory. Shipping
games may exceed 1 MiB; each logical cartridge still has its own 1 MiB ROM limit.
Studio uses main's identity for the output folder and title, even when another
member is selected. `mod01-game.json` records the whole-game SHA-256 identity,
shipping byte count, main ID and a catalog of alias → persistent ID / logical ROM
bytes. The README lists every separate save identity. Transfers resolve only
against the packaged catalog; no loose dependency files or downloads are needed.
Native on-disk save filenames use the same portable stem; IDs inside save data
remain unchanged. Existing development save files are not automatically renamed
or migrated; see [save storage and recovery](STUDIO.md#source-assets-and-recovery).

`--player`, `--node`, `--host`, and `--node-license` select explicit toolchain files.
`--platform linux|windows|macos` supports packaging existing target binaries, **not**
cross-compilation. Player and Node platform/architecture headers must match. Only
64-bit desktop binaries are supported. A Node distribution's full LICENSE must be
available beside its executable or supplied explicitly. macOS universal binaries
must first be built/provided as a target-specific 64-bit executable.

`node scripts/verify-native-game.mjs` tests a native export from an unrelated
working directory with an empty PATH: bundled Node must run, 120 display frames
and their PCM, routing timeline and save commits must match the original player,
and packed bytes must equal the normal CLI pack. It checks the requested ordinary
cartridge and a generated >1 MiB two-cartridge game that transfers every frame,
mixes 60/30 Hz updates, passes parameters, plays actual sound and asserts separate
save ownership. Its main cartridge deliberately uses ID `node` to catch accidental
player/runtime overwrites. It must produce 60 frames from each member; shipped source
must remain inspectable from outside the checkout. The macOS/Windows/Linux
packaging matrix runs this same verification on each OS; cross-target file-layout
tests alone do not demonstrate execution on another OS.

`node scripts/verify-relay-station.mjs` also compares the maintained three-member
game's complete 110-frame replay across CLI and native source/raw/PNG execution
and the actual standalone package with empty PATH. Each frame must agree on
producer/local/global cursor, work, pixels, interleaved PCM and audio commands;
the final canonical state/checkpoint and every distinct save image must agree.
The route exercises two asset-bank reloads, 60/30 Hz execution, task/RNG state,
menu transfer and held-input quarantine, then returns home with a committed clear.
This check is part of both the full gate and the desktop packaging matrix.

For display-free QA through the **native production bridge**:

```sh
target/debug/mod01-studio --verify cartridges/pocket-relay --frames 120
target/debug/mod01-studio --verify GAME.m01c --frames 240 --input run.m01rec
```

Verification loads and compiles the cartridge, checks ordered display frames and
stereo PCM sizes, then stops the process. It prints JSON fingerprints covering all
framebuffers, stereo PCM, producer-owned save commits and the routing timeline,
plus per-member frame counts, whole-game identity and peak work. Optional `--input`
uses the same bounded current `.m01rec` decoder as Studio; unspecified frames use
neutral input, and events beyond `--frames` are rejected before game loading.
The report also fingerprints consumed input and the final opaque whole-game
checkpoint (the latter is a same-Node-build comparison, not a portable snapshot
file format). Verification accepts 1–36,000 frames and replay files up to 8 MiB.
`frameChecks` contains bounded per-frame SHA-256 evidence using the CLI's shared
encoding, and final SHA-256 fields cover canonical state, checkpoint and saves.
These permit direct cross-host comparisons; they do not change game execution
or turn JSON reports into restorable checkpoints.
Use canonical SHA-256 fields for restore/replay equivalence. Opaque V8 checkpoint
bytes are transport data, not a canonical state encoding: even equal values can
produce different serialized bytes, as documented by
[Node's serialization API](https://github.com/nodejs/node/blob/main/doc/api/v8.md#serialization-api).
Studio replay imports use that same bounded reader. It does not open
a window, initialize an audio device, read local saves or persist save commits.
Frame count is bounded to 1–36,000. This is a smoke
test, not a substitute for graphical/input/audio QA.
