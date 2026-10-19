# Product principles

MOD-01 Color Development Unit is a local, native fantasy console and game-making
toolkit: Rust Studio, CLI and standalone desktop games share one compiler and
deterministic console core. The restrained 1999 colour-handheld fiction appears
in tools and cartridge labels, never in fabricated performance or compatibility
claims.

## Current contract

The approved scope and evidence ledger is
[NATIVE_IMPLEMENTATION.md](NATIVE_IMPLEMENTATION.md). Checkboxes require working,
verified behaviour; planned features are not release claims. The machine uses
240×144 indexed graphics, 256 fixed colours, 1 MiB visual/packed capacities,
four controllers, eight-channel audio and a 50,000-unit frame budget.
See [HARDWARE.md](HARDWARE.md) and [LIMITS.md](LIMITS.md).

One editable workcart and one packed encoding preserve original source and assets.
There is no browser frontend, web distribution, historical-format migration,
account service or public hosted catalog. Native desktop games are the shipping
target; Raspberry Pi/handheld packages and a general-purpose 3D engine are excluded.

## Principles

- Source remains understandable and recoverable, including through packed/PNG exports.
- Constraints agree across Studio, CLI, debugger, headless and packaged games.
- Determinism is observable through frame time, owned RNG, snapshots and replay.
- Local ownership comes first: projects, saves and recovery stay on the device.
- Tools expose compiler-derived meaning rather than inventing a scene engine.
- Native screenshots, interactions and measured checks support completion claims.

MODL is statically typed and intentionally bounded. The primary audience includes
game developers, size coders, demo authors, language-tool enthusiasts and curious
programmers. MOD-01 does not import or emulate another fantasy console's cartridges.
