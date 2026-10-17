<p align="center">
  <a href="#mod-01-color-development-unit"><img src="apps/studio/public/mod01-logo.png" width="220" alt="MOD-01 M01 logo" /></a>
</p>

<p align="center">
  <a href="https://github.com/gongahkia/MOD-01/actions/workflows/ci.yml"><img src="https://github.com/gongahkia/MOD-01/actions/workflows/ci.yml/badge.svg" alt="MOD-01 CI" /></a>
</p>

# MOD-01 Color Development Unit

<p align="center"><strong>A local-first colour fantasy console from an alternate 1999.</strong></p>

MOD-01 is a local-first fantasy console presented as a technically unusual colour handheld from 1999. Its native desktop Studio uses a Rust interface with no webview, the Rust MODL compiler, and
the shared deterministic game runtime in a restricted child process. The original browser Studio
remains available for existing projects and its advanced browser tools.

Made by @gongahkia. Copyright 2026 @gongahkia. All rights reserved. This repository is private and
proprietary; cartridge authors retain ownership of their source and assets.

## Contents

- [V1 release-candidate status](#v1-release-candidate-status)
- [Quick start](#quick-start)
- [Boundaries](#boundaries)

![Fresh MOD-01 Studio boot monitor with all eight bundled cartridges](docs/images/studio-shell.png)

## V1 release-candidate status

The cohesive V1 workflow is implemented: create/import a cartridge, edit code and source-visible
assets, compile, run, debug, rewind, save/recover, pack, inspect, and export without an account or
backend. The production app is a relative-path static PWA and works offline after its first
successful load. The V1 contract is recorded in [product principles](docs/PRODUCT.md),
[limits](docs/LIMITS.md), and [release evidence](docs/V1_RELEASE_EVIDENCE.md); implementation detail
and history remain in [`docs/`](docs/PROGRESS.md).

The monitor preinstalls eight source-visible first-party cartridges. The three preserved original
games are ordinary public-facility MODL projects:

| Game                                        | Focus                                                                               |
| ------------------------------------------- | ----------------------------------------------------------------------------------- |
| [Cinder Circuit](cartridges/cinder-circuit) | Responsive tile platforming, animation, tasks, camera, and SFX                      |
| [Ashvault](cartridges/ashvault)             | Procedural fog-of-war roguelike, records/lists, enemies, and isolated save progress |
| [Raster Rush 99](cartridges/raster-rush)    | Scanline pseudo-3D racing, synth music, and simultaneous 2-4 player split screen    |

![Cinder Circuit gameplay](docs/images/cinder-circuit.png)
![Ashvault gameplay](docs/images/ashvault.png)
![Raster Rush 99 standalone player](docs/images/raster-rush.png)

The [full bundled-cartridge catalog](cartridges/README.md) also covers the Hardware Revision 1
service cart, three size-class showcases, and the FIRST SIGNAL tutorial.

## Quick start

For the native Studio, use Node.js 24 and Rust 1.98:

```sh
corepack enable
pnpm install --frozen-lockfile
pnpm native
```

`pnpm native -- path/to/project` opens a project immediately. The native Studio supports macOS,
Windows, and Linux. Its start screen opens or plays all eight bundled cartridges. It opens project
directories, `.m01c` files, cartridge PNGs, or `.m01w` workcarts; it edits MODL source and declared
assets, runs cartridges with keyboard and gamepad input, plays stereo audio, and saves/exports
projects. Its layered sprite editor defaults to 64×64, supports up to 128×128 and 256 palette
entries, and retains editable RGBA layers beside the compiled sprite asset. Debug mode supports
source stepping and snapshot restore. Its native interface uses BigBlue Terminal Plus throughout
and Kenney pixel cursors on relevant controls. `pnpm native:package` creates a desktop folder and portable
archive that include the Node runtime used for cartridge execution; packaged users do not need a
separate Node install.
See [native Studio details](docs/STUDIO.md).

For the original browser Studio and the full repository quality gate:

Requirements are Node.js 24 and Rust 1.98. `corepack enable` supplies the pinned pnpm
10.32.1; the repository's `rust-toolchain.toml` selects Rust, Clippy, rustfmt, and
`wasm32-unknown-unknown`. The one setup command installs locked dependencies, installs the pinned
`wasm-bindgen-cli` 0.2.128 when absent, installs the pinned Playwright Firefox and Chromium test
browsers, and builds the complete project:

```sh
corepack enable
make setup
pnpm dev
```

Open the printed local URL. The monitor shell starts with all built-in cartridges installed. Use `dir`,
`load raster-rush`, `run`, or `run ashvault` to launch a cartridge directly; Shift+Escape stops a game. `help` lists integrated commands. Keyboard
port one uses arrows plus Z/X/A/S, while standard gamepads populate all four ports.

Run every repository gate with:

```sh
make check
```

This includes the production build and the pinned Firefox and Chromium end-to-end workflows.

For the native external-editor workflow:

```sh
cargo run -p mod01-cli -- new my-game --title "MY GAME"
cargo run -p mod01-cli -- check my-game
cargo run -p mod01-cli -- test my-game
cargo run -p mod01-cli -- run my-game
cargo run -p mod01-cli -- run my-game --headless --frames 120 --input path/to/replay.json
cargo run -p mod01-cli -- export html my-game
```

See the [interactive/from-scratch tutorial](docs/TUTORIAL.md), [runnable examples](examples/README.md),
[starter cartridges](templates/README.md), [tool guide](docs/TOOLS.md),
[MODL language contract](docs/MODL.md), and [cartridge format](docs/CARTRIDGE_FORMAT.md).

## Boundaries

This V1 candidate has no cloud account, synchronization, hosted backend, gallery, network/multiplayer API,
third-party cartridge compatibility, raw JavaScript escape, true-colour path, samples,
physics engine, ECS, scene graph, or real 3D renderer. Worker containment is a practical browser
boundary, not process-level isolation; [SECURITY.md](docs/SECURITY.md) states the exact limits.

To refresh the Studio boot capture, run the production preview in one terminal:

```sh
pnpm build
pnpm --dir apps/studio exec vite preview --host 127.0.0.1 --port 4173 --strictPort
```

Then, in another terminal, run `node scripts/capture-v1-visuals.mjs` and copy its 1280x720 Firefox
shell capture from `output/playwright/v1-firefox-shell.png` to `docs/images/studio-shell.png`. The
checked images use exact integer scale Playwright captures; no mockups are used.
