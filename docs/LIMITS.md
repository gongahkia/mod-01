# MOD-01 limits and calibration

## Hardware Revision 2

New native projects use `hardware_revision = 2`. The display stays 240×144 at 60 Hz, while sprite
assets default to 64×64 and may reach 128×128. The master palette has 256 indexed colours, the
shared visual bank holds 1 MiB, and a packed `.m01c` may hold 1 MiB. The first 32 palette colours,
the original MMIO addresses, the 8 KiB save block, and the 50,000-work-unit frame limit remain
stable. The legacy profile below still applies to revision-1 cartridges.

The public profile was frozen during alpha after building and running all three original game cartridges through
the public compiler, packer, worker, indexed renderer, and asset decoder. Work units are synthetic
deterministic costs, not elapsed time or CPU instructions. Measurements below are representative
active-play frames observed in the production Studio on 2026-09-07; control flow can vary slightly,
and the enforced ceiling remains the authority.

| Cartridge      | Packed `.m01c` | Visual store | Release JS | Representative work/frame |
| -------------- | -------------: | -----------: | ---------: | ------------------------: |
| Cinder Circuit |       42,121 B |      9,766 B |   13,896 B |                     3,342 |
| Ashvault       |       40,532 B |        288 B |   18,832 B |                    12,031 |
| Raster Rush 1P |       37,311 B |        160 B |   16,546 B |                    28,034 |
| Raster Rush 4P |       37,311 B |        160 B |   16,546 B |                    31,682 |

Raster Rush four-player is the governing frame case at about 63% of the 50,000-unit ceiling. It
executes four independent controller paths and view projections plus 144 raster callbacks while
remaining below the limit. The 256 KiB packed and 128 KiB visual ceilings have substantial headroom
for these deliberately compact games; they remain useful authoring ceilings rather than targets.
Cinder's four 2,048-pixel circuits at its configured 30 Hz update rate establish a roughly 4 1/2
minute uninterrupted minimum traversal before jumps or life resets. The renderer culls its 256x18
map to the visible camera/clip region.

## Frozen revision-1 profile

- 240x144 indexed display, fixed 32-colour palette, 60 Hz rendering, 30/60 Hz updates.
- 50,000 deterministic work units and 4,096 draw commands per frame.
- 256 KiB packed cartridge and 128 KiB shared visual assets.
- 8 KiB isolated save data per cartridge.
- Eight synth voices, eight tracker channels, and 48 kHz stereo host output.
- Four controller ports, 1-64 pixel sprite axes, 8x8 tiles, integer nearest-neighbor transforms.

## V1 complete-artifact size classes

The class meter uses exact canonical `.m01c` length: 4K is <=4,096 bytes, 16K <=16,384, 64K
<=65,536, and 256K <=262,144. It includes source, assets, metadata, hashes, compression and container
overhead. `compile_on_load = true` may remove archived generated JS only while retaining the original
source plus expected generated-program length/hash.

| Cartridge         | Complete `.m01c` | Class | Five-frame headless work peak |
| ----------------- | ---------------: | ----: | ----------------------------: |
| Signal 4K         |          2,364 B |    4K |                         5,529 |
| Pocket Relay      |          4,609 B |   16K |                         4,743 |
| Hardware Gauntlet |          4,619 B |   16K |                         8,266 |

Hardware Gauntlet is the required <=64K stress showcase; its compact public-API implementation also
qualifies for the stricter 16K class. This is reported as measured rather than padded to a badge.

The final V1 section accountant reports the following empty-input 60-frame profiles. Intentional
gameplay compatibility paths remain separately frozen at 10,477/19,140/31,722 peak work,
90/120/470 draw commands and 4/5/5 active voices.

| Cartridge         | `.m01c` | Source | Release JS | Visual |   Work | Draw | Voices | Bus mapped |
| ----------------- | ------: | -----: | ---------: | -----: | -----: | ---: | -----: | ---------: |
| Cinder Circuit    |  42,904 |  5,163 |     14,689 | 10,695 | 10,477 |   90 |      0 |    366,905 |
| Ashvault          |  41,315 |  6,574 |     19,623 |    908 | 13,485 |   84 |      0 |    365,204 |
| Raster Rush 99    |  38,091 |  6,355 |     17,337 |    440 | 11,326 |   21 |      0 |    361,868 |
| Service cartridge |  27,869 |  5,244 |     15,060 |      0 |  5,130 |   12 |      1 |    351,382 |
| Signal 4K         |   2,364 |    600 |      4,699 |      0 |  5,769 |   20 |      1 |    325,877 |
| Pocket Relay      |   4,609 |  1,354 |      5,965 |    711 |  4,743 |   18 |      0 |    328,210 |
| Hardware Gauntlet |   4,619 |  1,941 |      8,543 |    407 |  8,282 |  150 |      1 |    328,188 |
| First Signal      |  11,706 |  1,558 |      6,025 |      0 |  1,413 |    6 |      0 |    335,219 |

No limit required adjustment from the brief's initial values; the previously unspecified work
ceiling is frozen at 50,000. Later format revisions may change a profile only with a revisioned,
measured compatibility decision.

## Reproduction

```sh
node scripts/generate-cartridge-assets.mjs
cargo test -p mod01-cli --test cli bundled_cartridges_compile_and_pack_within_capacity
pnpm test -- --run packages/runtime/src/bundled-cartridges.test.ts
pnpm --filter @mod01/studio build
./scripts/verify-release-artifacts.sh
```

The frame figures come from the Studio's visible `W` counter during active play. Use `debug`, choose
`PROFILE`, and advance frames for per-source-span attribution.
