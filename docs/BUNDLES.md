# Local multi-cartridge games

Status: source preservation and whole-game packing/admission are implemented in
the shared Rust core. Runtime switching, asset reload, state/save handling,
CLI/Studio controls and cross-host verification are still being implemented.
This document does not claim that those runtime workflows are available yet.

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
paths; gameplay/replay/debugger parity remains an open acceptance requirement.

The 32-member model and separate loading/data-reload requirements were researched
against the primary [PICO-8 manual](https://www.lexaloffle.com/dl/docs/pico-8_manual.html),
sections 3.4 and 6.1/6.7. MOD-01 does not import PICO-8 cartridges or add online/BBS
access.
