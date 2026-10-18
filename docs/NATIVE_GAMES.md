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

For display-free QA through the **native production bridge**:

```sh
target/debug/mod01-studio --verify cartridges/pocket-relay --frames 120
```

Verification loads and compiles the cartridge, checks ordered display frames and
stereo PCM sizes, then stops the process. It prints a JSON framebuffer fingerprint
and peak work. It does not open a window, initialize an audio device, read local
saves or persist save commits. Frame count is bounded to 1–36,000. This is a smoke
test, not a substitute for graphical/input/audio QA.
