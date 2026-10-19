# MOD-01 Color Development Unit

A local-first colour fantasy console from an alternate 1999.

MOD-01 has a native Rust desktop Studio, a native CLI, a shared Rust MODL compiler,
and one deterministic console runtime hosted in a restricted Node child process.
There is no browser frontend, webview, PWA, web sharing or HTML/web-ZIP export.
Standalone games ship with the native player and their own Node runtime.

Made by @gongahkia. Copyright 2026 @gongahkia. All rights reserved. This repository
is private and proprietary; cartridge authors retain ownership of their source
and assets.

## Run locally

Use Node.js 24, Rust 1.98 and the pinned pnpm:

```sh
corepack enable
pnpm install --frozen-lockfile
pnpm native
```

On Linux, install development packages for ALSA, udev, X11/Wayland, xkbcommon,
OpenGL and GTK3 first. CI's exact Ubuntu package list is in
[ci.yml](.github/workflows/ci.yml). Fedora uses the corresponding
`alsa-lib-devel systemd-devel libX11-devel libXrandr-devel libXi-devel
wayland-devel libxkbcommon-devel mesa-libGL-devel gtk3-devel` packages.
If pkg-config does not search system libraries, add the system pkgconfig directories
to `PKG_CONFIG_PATH`.

Or build once and launch directly:

```sh
pnpm build
cargo build -p mod01-native -p mod01-cli
target/debug/mod01-studio
target/debug/mod01-studio --play cartridges/pocket-relay
```

Use `target/debug/mod01-studio.exe` on Windows. Studio's sample picker opens the
bundled cartridges for play or source inspection. Source, layered sprites, maps,
fonts, audio, compiler inspection and debugger tools are native interfaces.
The UI uses BigBlue Terminal Plus and Kenney pixel cursors.
TOOLS → [MODL playground](docs/PLAYGROUND.md) evaluates expressions and drawing
without modifying your cartridge or saves.
For a playable textured first-person maze, open `cartridges/prism-corridor`;
its [software 3D walkthrough](docs/SOFTWARE_3D.md) explains the ordinary MODL
projection, map, controls and tested completion route.

## CLI and game distribution

```sh
target/debug/mod01 new my-game --title "MY GAME"
target/debug/mod01 check my-game
target/debug/mod01 assets my-game
target/debug/mod01 import png my-game hero art/hero.png
target/debug/mod01 test my-game
target/debug/mod01 run my-game
target/debug/mod01 run my-game --headless --frames 120
target/debug/mod01 pack my-game --output dist/my-game.m01c
target/debug/mod01 export png my-game --output dist/my-game.m01c.png
target/debug/mod01 export native my-game --output dist/my-game-native
```

Native export requires the built native player; the output directory must not
already exist. It bundles the cartridge, runtime, target-platform Node and notices.
See [native games](docs/NATIVE_GAMES.md) for packaging, source inspection, save
behaviour, target binaries and display-free verification. `pnpm native:package`
builds and packages Studio and the CLI for the current desktop OS.

## Quality and implementation status

```sh
make setup
make check
node scripts/verify-native-game.mjs
```

The quality gate runs formatting, lint, types, runtime/compiler/native tests,
builds and artifact verification. The native packaging workflow also runs on
Windows and macOS; local Linux verification does not substitute for those jobs.

The approved native-consolidation work is tracked honestly in
[the acceptance ledger](docs/NATIVE_IMPLEMENTATION.md). Unchecked rows are still
in progress. [Standard-library semantics](docs/STANDARD_LIBRARY.md) include
deterministic math, safe number conversion, bounded text and explicit bit ops.

## Boundaries

No cloud accounts, hosted catalog, public community backend or network/multiplayer
API is part of the product. Software-rendered 3D examples use the public console
facilities; there is no general-purpose 3D scene engine. Node permission flags and
runtime budgets are defence in depth, not an OS sandbox for arbitrary JavaScript.
See [security boundaries](docs/SECURITY.md).
