# MOD-01 asset files

Assets are UTF-8 JSON embedded in a workcart. Manifest names become typed MODL
handles such as `#hero` and `#theme`. Sprite/tile-set/display JSON use current
revision 2; map/font/sound/music each use their own current revision 1 schema.
These are independent asset schemas, not compatibility profiles.

Integer fields use bounded numeric values under [JSON's number grammar](https://www.rfc-editor.org/rfc/rfc8259.html#section-6),
not a number's spelling: `2`, `2.0` and `2e0` mean the same thing. Native sprite,
tile/map/font/display and tracker tools share the validator's integer reader.
Opening/rendering accepted numbers does not normalize the authored JSON; explicit
edits can write ordinary integer values. Fractions, booleans, numeric strings and
out-of-range values are not repaired into valid integers. Tile PNG export refuses
invalid colours instead of silently exporting black or clamping them.

Stored noise/WFC map recipes consume current revision-2 tile sets and emit current
revision-1 maps. Generated results pass the same asset/compiler/packed admission;
obsolete revision-1 tile sets are rejected instead of producing an un-packable game.

Native sprite/tile PNG import uses the shared Rust decoder and palette converter.
It checks PNG source size (16 MiB), IHDR geometry and purpose before pixel
decoding, with decoder allocation limits as well: sprites are 1–128 on each axis;
atlases contain 1–4,096 complete 8×8 tiles in row-major order. Alpha 0–127 becomes
index zero. Other pixels use squared-RGB nearest colour among indices 1–255,
choosing the lowest index on ties, without dithering. Original RGBA layer data
remains editable; sprite resize/import is one undo transaction. Tile import
preserves authored metadata and existing flags, truncating/extending flags with
the new tile count. `M014065` identifies failed PNG/conversion admission.

The CLI uses that same pipeline and writes one current workcart atomically:

```sh
mod01 import png my-game hero art/hero.png
mod01 import png my-game tiles art/atlas.png --kind tile-set
mod01 import png my-game hero art/revised.png --replace
mod01 import png my-game hero art/hero.png --output new-cart.m01w
```

Use `--replace` only to replace an entire existing asset. A sprite replacement
also replaces its editable RGBA sidecar, not just the indexed pixels; native PNG
IN instead targets the current layer/frame. Both paths produce identical files
and packed bytes for the same single-layer context. Other modules, auxiliary
files, recipes and tests are retained. Metadata and existing tile flags survive.
The CLI validates the complete asset bank, compiler output and packed capacity
before writing; shrinking an atlas cannot leave invalid map references behind.
Existing `--output` paths and implicit asset replacement are refused (`M014066`).
Every admission failure leaves the input unchanged. PNG reads are byte-bounded,
not merely checked after an unbounded file read.

`mod01 assets PATH` prints the shared Rust validation/capacity report as JSON
without needing Node or a display. Its exit status is nonzero for an invalid
bank. `mod01 check PATH` prints independent asset failures alongside source
diagnostics, and packing/loading a packed cartridge enforces the same validation.
Schema/reference failures do not prevent opening structurally valid workcarts
for repair. Reports identify the asset,
file and JSON field: `M014060` schema, `M014061` reference, `M014062` capacity and
`M014063` missing file. At most 128 diagnostics are retained; the JSON report's
`diagnosticsTruncated` flag states whether more were suppressed.

Usage counts schema-valid payloads only: any diagnostic means the bank is not
loadable, even if its partial byte count is below capacity. Visual bytes and
allocation slots are independent limits. `sourceBytes` is unique authored asset
JSON bytes, not console RAM or packed cartridge size. Runtime admission validates
untrusted data again; cross-language conformance tests compare both boundaries
and exact layout costs, including display storage.

Indexed colours are 0–255; sprite colour 0 defaults to transparent. The fixed
master RGB palette cannot be replaced. The 32 logical drawing/raster remaps
can each target any colour; direct indices 32–255 bypass this remap.

## Graphics

A `sprite` file contains a 1-128 pixel width and height and one or more row-major frames. Declare a
single-frame file as `sprite` or a multi-frame file as `animation` in the workcart:

```json
{ "revision": 2, "kind": "sprite", "width": 2, "height": 2, "frames": [[0, 23, 23, 0]] }
```

A `tile_set` contains 8x8 row-major tiles and one eight-bit flag value per tile:

```json
{
  "revision": 2,
  "kind": "tile_set",
  "tiles": [
    [
      1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
      1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
      1, 1
    ]
  ],
  "flags": [1]
}
```

A `map` has 1-8 layers. Each layer has a 1-256 cell width and height, row-major unsigned tile
indices, and the manifest name of its tile set:

```json
{
  "revision": 1,
  "kind": "map",
  "layers": [{ "width": 2, "height": 1, "cells": [0, 0], "tileSet": "tiles" }]
}
```

Sprite pixels, animation pixels, tile pixels and flags, two bytes per map cell, and display state
share the 1 MiB visual capacity. The loader rejects missing tile sets and out-of-range tile
indices before execution.

Payloads occupy the bus image at `0x210000`. Allocation descriptors and
`visual_id` expose actual renderer storage. At most 4,096 visual assets and
8,192 allocations fit, independently of byte capacity. See
[HARDWARE.md](HARDWARE.md#visual-image-and-allocation-descriptors).

## Bitmap fonts

A `font` contains fixed-size 1-16 pixel glyph masks, explicit horizontal and vertical advances, a
baseline for editor/layout alignment, and a required fallback code. Glyph codes are byte values
0-255, must be unique and strictly increasing in the source file, and each row-major pixel is 0 or

1. A font contains 1-256 glyphs and the fallback code must be present:

```json
{
  "revision": 1,
  "kind": "font",
  "glyphWidth": 3,
  "glyphHeight": 5,
  "baseline": 4,
  "advanceX": 4,
  "advanceY": 6,
  "missingGlyph": 63,
  "glyphs": [{ "code": 63, "pixels": [1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0] }]
}
```

The exact visual cost is `8 + glyph_count * (2 + glyph_width * glyph_height)` bytes. The packed
header and glyph codes are immutable at runtime; mask bytes alias the visual bus and may be changed
to 0 or 1 while paused or by legal cartridge bus writes. `font_print` renders a selected font;
ordinary `print` deliberately retains the unchanged built-in system font.

## Display state

The optional top-level `display = "assets/display.m01p"` manifest key names a default palette and
raster configuration. `remap` sets the initial logical drawing palette. Raster rows are strictly
increasing scanline changes with signed 16-bit scroll offsets and a complete 32-entry display
remap:

```json
{
  "revision": 2,
  "kind": "display",
  "remap": [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31
  ],
  "raster": [
    {
      "line": 96,
      "scrollX": 0,
      "scrollY": 0,
      "remap": [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31
      ]
    }
  ]
}
```

MODL `pal`, `pal_reset`, and `on raster` commands may override these defaults while a cartridge is
running. Display state is charged as 32 bytes plus 38 bytes per raster row.

## Synth patches and tracker music

A `sound` is an oscillator patch. Notes are MIDI integers 0-127, duration and envelope time are in
frames, volume and sustain are 0-1, and pan is -1 to 1. Pulse duty must be between zero and one.
Numeric fields must be present and finite, and the waveform must be one of the five supported
oscillators. Pitch effects are admitted only when the worst-case frequency remains finite across
all notes 0–127 and the full duration plus release: `127 + abs(slide)*lifetime + abs(vibrato)` is
the upper-note bound used for validation. Malformed/non-finite oscillator patches are rejected at load. Tracker orders/rows/tables must be dense
arrays with valid shapes, boolean looping and finite cell volumes. Loaded patches are copied into
the synthesizer's asset store; snapshot restore rejects an active voice beyond its patch lifetime.
Wavetables contain 4-32 samples in the range -1 to 1.

```json
{
  "revision": 1,
  "kind": "sound",
  "waveform": "pulse",
  "note": 60,
  "durationFrames": 18,
  "volume": 0.6,
  "pan": 0,
  "duty": 0.5,
  "envelope": { "attackFrames": 1, "decayFrames": 3, "sustainLevel": 0.65, "releaseFrames": 4 },
  "pitch": { "slideSemitonesPerFrame": 0, "vibratoDepthSemitones": 0, "vibratoPeriodFrames": 0 }
}
```

A `music` file supplies an order list and named patterns. Each row has exactly eight cells; a cell
is null or a note naming a declared sound patch. Patterns have 1-256 rows and `framesPerRow` is
1-240.

```json
{
  "revision": 1,
  "kind": "music",
  "framesPerRow": 6,
  "order": ["00"],
  "patterns": {
    "00": { "rows": [[{ "note": 60, "sound": "blip" }, null, null, null, null, null, null, null]] }
  },
  "loop": true
}
```

Arbitrary PCM data is intentionally not representable.
