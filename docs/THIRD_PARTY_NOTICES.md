# Third-party notices

MOD-01 is private proprietary software. This notice inventories dependencies; it does not grant a
license to MOD-01 source, cartridges, identity, art, font, music, or other first-party material.

The Rust workspace includes clap, serde/serde_json, toml, eframe/egui, cpal,
gilrs, directories, dunce, rfd, image and base64. The complete transitive graph,
versions and declared licenses are available with `cargo metadata` against
`Cargo.lock`; do not infer the complete native dependency inventory from a
short list of compiler crates.

Windows host-launch path compatibility uses pinned `dunce` 1.0.5. Its upstream
CC0-1.0 licence is retained in `crates/mod01-native/assets/LIBRARY-LICENSES.txt`
and travels with the existing native-library notices in Studio/game packages.

The JavaScript development graph's direct packages are TypeScript 6.0.3 (Apache-2.0), Vite 8.2.2
(MIT), Vitest 5.0.0 (MIT), ESLint 10.10.0 (MIT), `@eslint/js` 10.0.1 (MIT), typescript-eslint
8.69.0 (MIT), Prettier 3.9.6 (MIT). Locked transitives
also declare MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, MPL-2.0, and BlueOak-1.0.0. Exact
names, versions, integrity values, and edges are in `pnpm-lock.yaml`; `pnpm licenses list`
reproduces the inventory. Build/test packages are not cartridge assets or embedded runtime dependencies.

The native Studio embeds BigBlue Terminal Plus by VileR under CC BY-SA 4.0 and five images from
Kenney's Cursor Pixel Pack under CC0. Source attribution and complete license texts are in
`crates/mod01-native/assets/` and copied to `visual-credits` in native packages. The console
palette, runtime bitmap font, icon, labels, graphics, and synth compositions are original;
`mod01.ttf` is generated from the same first-party glyph matrix. Native packages include Node and its complete upstream LICENSE, including
third-party redistribution notices, in `licenses/Node.txt`. The source is the
LICENSE accompanying the actual Node binary being packaged. No HTML player
or browser binaries are distributed.

Installed upstream package directories contain their full licenses and notices. Any external binary
distribution must accompany notices required by those terms; this repository task does not publish
or deploy one. This file is the repository's workspace-level dependency inventory.
