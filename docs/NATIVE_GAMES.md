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

`--player`, `--node`, `--host`, and `--node-license` select explicit toolchain files.
`--platform linux|windows|macos` supports packaging existing target binaries, **not**
cross-compilation. Player and Node platform/architecture headers must match. Only
64-bit desktop binaries are supported. A Node distribution's full LICENSE must be
available beside its executable or supplied explicitly. macOS universal binaries
must first be built/provided as a target-specific 64-bit executable.

`node scripts/verify-native-game.mjs` tests a native export from an unrelated
working directory with an empty PATH: bundled Node must run, 120 display frames
must match the original player, and packed bytes must equal the normal CLI pack.
The macOS/Windows/Linux packaging matrix runs the same verification on each OS.

For display-free QA through the **native production bridge**:

```sh
target/debug/mod01-studio --verify cartridges/pocket-relay --frames 120
```

Verification loads and compiles the cartridge, checks ordered display frames and
stereo PCM sizes, then stops the process. It prints a JSON framebuffer fingerprint
and peak work. It does not open a window, initialize an audio device, read local
saves or persist save commits. Frame count is bounded to 1–36,000. This is a smoke
test, not a substitute for graphical/input/audio QA.
