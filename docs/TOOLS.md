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
mod01 graph my-cart
mod01 export png my-cart --output dist/my-cart.m01c.png
mod01 export native my-cart --output dist/my-cart-game
mod01 lsp
```

`run` launches the native game surface. Build `mod01-native` beside the CLI or set
`MOD01_PLAYER` to the player executable. `--headless` runs the deterministic
console core under Node and emits a JSON trace; `--output`, `--input` and `--save`
are headless options. Headless runs do not open a window or write local saves.

`watch` hashes the single authored workcart and repacks a canonical cartridge after changes.
It does not serve HTTP or open a browser. Invalid edits report diagnostics and
leave the last successful cartridge intact. `watch --once` packs once and exits.
Generated output and neighbouring files are not watched, including a custom
output inside the project directory. The default output remains in `dist/`.

`export png` contains the complete canonical cartridge in a validated PNG chunk.
`info` accepts raw/PNG cartridges and reports metadata, exact packed size,
section sizes, largest entries and a deterministic 60-frame production-core
profile. The profile uses empty input/save and a fixed seed; use explicit
`run --headless` input for gameplay verification.

`export native` bundles the native player, canonical cartridge, shared runtime,
Node executable and notices. Existing output directories are refused. Target
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

`graph WORKCART` emits read-only compiler JSON: original module text/IDs, typed
definitions, source-mapped diagnostics/references, callbacks, globals, routines,
and resolved call/start/read/write/asset edges. It never runs the cartridge or
requires Node. Invalid source returns diagnostics, a nonzero status and no
executable graph. It is not a scene format or an additional game engine.

See [Studio](STUDIO.md), [standard library](STANDARD_LIBRARY.md), and the
[native implementation acceptance ledger](NATIVE_IMPLEMENTATION.md).
