# MOD-01 limits

There is one current machine. Limits are enforced, not suggested targets.
Work units are deterministic accounting units, not elapsed time or CPU cycles.

| Facility                               |                                         Limit |
| -------------------------------------- | --------------------------------------------: |
| Display / scanout                      |                240×144 indexed pixels / 60 Hz |
| Update cadence                         |                                   30 or 60 Hz |
| Master palette                         |                             256 fixed colours |
| Logical drawing/raster remaps          | 32 entries, each targeting any palette colour |
| Packed cartridge                       |                                         1 MiB |
| Shared visual payload                  |                                         1 MiB |
| Visual asset descriptors / allocations |                                 4,096 / 8,192 |
| Sprite axes                            |                                  1–128 pixels |
| Tiles / map layers / map axes          |                       8×8 / 1–8 / 1–256 cells |
| Save block                             |                        8 KiB per cartridge ID |
| Draw commands / frame                  |                                         4,096 |
| Work units / boot or frame             |                                        50,000 |
| Synth voices / tracker channels        |                                         8 / 8 |
| Synth output                           |                              48,000 Hz stereo |
| Controller ports                       |                                             4 |
| Byte address space / work RAM          |                                4 MiB / 64 KiB |
| Bounded text                           |                    4,096 UTF-16 storage units |
| Headless frames                        |                                        36,000 |

Framebuffers, immutable metadata, ROM, audio state and descriptor tables do not
consume the visual payload capacity. Visual allocation metadata has its own
explicit record limit and cannot grow into ROM. See [HARDWARE.md](HARDWARE.md)
for the sole current map and [ASSETS.md](ASSETS.md) for payload accounting.

Packed-size badges use complete canonical artifact length: 4K ≤4,096 bytes,
16K ≤16,384, 64K ≤65,536, 256K ≤262,144, 1M ≤1,048,576. They are not
hardware profiles. `compile_on_load` may omit redundant build products but
never source, assets, metadata or compiled-program identity.

Use `mod01 info PROJECT` for the exact packed/section/visual accountant and
`mod01 run PROJECT --headless --frames 60` for measured runtime peaks. Measurements
depend on inputs and compiler version; old benchmark tables are not a contract.
The runtime rejects the first charge above the work ceiling at its MODL span.

## Reproduction

```sh
cargo test -p mod01-cli --test cli bundled_cartridges_compile_and_pack_within_capacity
pnpm test packages/runtime/src/bundled-cartridges.test.ts
./scripts/verify-release-artifacts.sh
```

The release-artifact verifier packs all maintained games twice, byte-compares
raw and PNG results, boots them headlessly, then verifies a native game package.
See [NATIVE_IMPLEMENTATION.md](NATIVE_IMPLEMENTATION.md) for unfinished features
and final cross-platform/visual QA requirements.
