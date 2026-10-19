# MOD-01 tools

The native `mod01` CLI uses the same Rust compiler and asset contract as Studio.

```sh
mod01 new my-cart --title "MY CART"
mod01 fmt my-cart
mod01 check my-cart
mod01 build my-cart --debug
mod01 test my-cart
mod01 watch my-cart
mod01 run my-cart
mod01 run my-cart --headless --frames 120 --input replay.json
mod01 pack my-cart --output dist/my-cart.m01c
mod01 info dist/my-cart.m01c
mod01 info dist/my-cart.m01c --static
mod01 graph my-cart
mod01 export png my-cart --output dist/my-cart.m01c.png
mod01 export native my-cart --output dist/my-cart-game
mod01 lsp
```

`run` launches the native game surface. Build `mod01-native` beside the CLI or set
`MOD01_PLAYER` to the player executable. `--headless` runs the deterministic
console/game core under Node and emits a current revision-2 JSON trace;
`--output`, `--input`, `--save`, `--saves` and `--parameter` are headless options.
`--save` loads the entry's ≤8 KiB raw image; mutually exclusive `--saves` accepts
a ≤512 KiB JSON catalog of declared persistent IDs to canonical base64 images.
`--parameter` is initial Text, empty by default and ≤1,024 UTF-8 bytes. Headless
runs do not open a window or write local saves. Replay input remains revision 1.
See [trace identity and state hashes](CARTRIDGE_FORMAT.md).

`watch` hashes the single authored workcart and repacks a canonical cartridge after changes.
It does not serve HTTP or open a browser. Invalid edits report diagnostics and
leave the last successful cartridge intact. `watch --once` packs once and exits.
Generated output and neighbouring files are not watched, including a custom
output inside the project directory. The default output remains in `dist/`.

Default distribution names use main's shared portable stem: `cart-ID.m01c`,
`cart-ID.m01c.png` and `cart-ID-native`. Dots in the ID become underscores only
in this filename stem, avoiding reserved basenames and trailing-dot issues on
desktop filesystems. IDs themselves cannot contain underscores, so distinct IDs
remain distinct filenames. Explicit `--output` paths are respected. Source bytes,
cartridge IDs and save ownership are unaffected.

`export png` contains the complete canonical cartridge in a validated PNG chunk.
`info` accepts source/raw/PNG games and emits the entry's packed metadata plus
the current revision-2 `analysis` JSON. `shipping` measures the complete canonical
game archive; `members`, keyed by alias, reports each logical ROM's own size
class, manifest/save ID, section sizes, largest entries/linked routines and fixed
limits. `assetUsage` comes from the shared core's actual visual-bank byte and
allocation accounting, not the length of asset JSON. A shipping container larger
than 1 MiB is not an oversized logical cartridge.

`info GAME --static` never requires Node or runs cartridge code. Ordinary `info`
adds one deterministic 60-frame **whole-game** production profile, using the
ordinary default seed, empty input/saves and an empty initial parameter. It reports
`completed`, `faulted` or `unavailable` explicitly, plus per-member observed frame
counts and `notProfiledMembers`; static mode reports `not_requested`. An unused
dependency is admitted/analyzed but is not independently booted with an invented
parameter. Runtime startup failures (including parameter assertions) or missing
Node remain visible in the JSON alongside valid static evidence; they are never
reported as successful gameplay verification. Use `run --headless` with explicit
inputs/parameters for gameplay verification. All input representations produce
the same report. Inspection never changes authored files or player saves.

`export native` accepts source/raw/PNG games and bundles the native player, complete
canonical game (all logical cartridges and their source), shared runtime, Node
executable and notices. Main owns the package title/ID; each member retains its own
save ID and ROM capacity. Existing output directories are refused. Target
binaries must match the selected desktop platform and architecture; see
[native games](NATIVE_GAMES.md). There is no HTML or web-ZIP export.

## Cartridge tests and language service

Source tests use ordinary MODL assertions. Expected compiler-failure tests declare
a stable diagnostic; scripted replay tests specify deterministic input and
expected device/state hashes. Recipes and authored tests stay with workcarts,
not in a hosted service. Source-only tests are not release entry points.

`mod01 lsp` provides diagnostics, completion, hover, signature help, definition,
references/rename, formatting and document/workspace symbols over stdio.
MODL source is ASCII-only, so source byte columns match LSP UTF-16 columns.
API completion/hover/signatures are derived from the compiler's complete builtin
registrations, including memory, math, text and bit helpers. Standard API names
are reserved consistently at module scope; there is no lazy historical-name path.

`graph GAME` accepts source workcarts, raw cartridges and cartridge PNGs. Its
current revision-1 JSON has `entry: "main"` and `members`, keyed by local alias.
Each member carries `cartridgeId` and its compiler `model`: original module
text/IDs, typed definitions, source-mapped diagnostics/references, callbacks,
globals, routines and resolved call/start/read/write/asset edges. Source, node and
symbol IDs belong to that member, not a merged global namespace. It never runs
the cartridge or requires Node. Invalid source drafts return member-scoped
diagnostics and a nonzero status; packed/PNG games must first pass whole-game
admission. It is not a scene format or an additional game engine.

See [Studio](STUDIO.md), [standard library](STANDARD_LIBRARY.md), and the
[native implementation acceptance ledger](NATIVE_IMPLEMENTATION.md).
