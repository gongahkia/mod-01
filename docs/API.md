# MOD-01 cartridge API

This reference covers the currently implemented MODL/1 console surface. Calls are statically typed,
charged to the synthetic work budget, and unavailable as raw host functions. Coordinates and sizes
are `Int`; drawing outside the active clip is discarded.

## Display and drawing

The display is a persistent, double-buffered 240x144 array of palette indices. Drawing state resets
at the start of each frame. `Color` is always one of the 256 immutable master-palette indices.
Logical index 0 is transparent by default while blitting assets and remains an
ordinary drawable framebuffer colour.

| MODL call                                 | Behavior                                                                   |
| ----------------------------------------- | -------------------------------------------------------------------------- |
| `clear(color)`                            | Fill the full back buffer, independent of camera and clip.                 |
| `pixel(x, y, color)`                      | Write one indexed pixel.                                                   |
| `line(x0, y0, x1, y1, color)`             | Draw an inclusive Bresenham line.                                          |
| `rect(x, y, width, height, color)`        | Draw a rectangle outline. Non-positive sizes draw nothing.                 |
| `rect_fill(x, y, width, height, color)`   | Draw a filled rectangle.                                                   |
| `circle(x, y, radius, color)`             | Draw an integer midpoint-circle outline.                                   |
| `circle_fill(x, y, radius, color)`        | Draw a filled midpoint circle.                                             |
| `triangle(x0, y0, x1, y1, x2, y2, color)` | Draw a filled integer triangle.                                            |
| `print(text, x, y, color)`                | Draw text using the original built-in 5x7 bitmap font.                     |
| `font_print(font, text, x, y, color)`     | Draw text with a declared custom bitmap `Font`, using its advances.        |
| `camera(x, y)`                            | Subtract an integer world-space camera origin from later draws.            |
| `clip(x, y, width, height)`               | Restrict later writes to a screen-space rectangle.                         |
| `clip_reset()`                            | Restore the full-screen clip.                                              |
| `pal(from, to)`                           | Remap a logical colour for later draws, or display scanout in `on raster`. |
| `pal_reset()`                             | Restore the identity drawing remap.                                        |
| `palt(color, transparent)`                | Set the blit-transparency bit of any source colour (0–255).                |
| `palt_reset()`                            | Restore the blit mask: only source colour zero is transparent.             |
| `dither(x, y, first, second, level)`      | Select a colour from a fixed 4x4 Bayer pattern; level is clamped to 0-16.  |

`sprite(asset, x, y)` draws a named 1-128-pixel indexed sprite. `animation(asset, frame, x, y)`
selects a wrapped animation frame. `sprite_xform(asset, x, y, scale, quarter_turns, flip_x, flip_y)`
uses deterministic nearest-neighbour integer scaling from 1x through 16x and rotation in exact
quarter turns. Transformed output is charged more heavily than an ordinary blit.

`sprite_region(asset, sx, sy, sw, sh, dx, dy, dw, dh, flip_x, flip_y)` takes a
typed `Sprite`, eight `Int` rectangle fields and two explicit `Bool` flips.
Source and destination rectangles use top-left pixel coordinates and extents.
Nonpositive extents draw nothing; positive source axes are at most 128 and
destination axes at most 2048. Destination pixel `(i,j)` samples source offset
`(floor(i*sw/dw), floor(j*sh/dh))`; flipping reverses that offset within the
requested source rectangle before adding `(sx,sy)`. Sampling is nearest-neighbour,
including unequal scaling and shrinking. Source samples outside the actual
sprite are skipped, preserving destination alignment; they are not treated as
colour zero. The camera, clip, blit-transparency mask and palette remap apply
normally. Primitive fill patterns do not affect asset blits.

Region work is `max(1, ceil(dw*dh/4))` device units for positive extents, otherwise
one. The whole destination area is charged even if clipped or offscreen, before
any pixels change; exhausting the work budget faults with `M019001`. Invalid
geometry/types fault with `M019009` at the source call. Visible iteration is
clipped to the screen, and all sampling products fit exact integer arithmetic.
Each call consumes one draw command and is forbidden in `on raster`.

`texture_line(asset, x0, y0, x1, y1, u, v, du, dv, repeat)` draws an inclusive
Bresenham line using a typed `Sprite` texture. Endpoints are `Int`; texture
coordinates and deltas are `Num` measured in **pixels**, not tiles. Every raster
sample takes `floor(u), floor(v)` and then advances by `du,dv`, including clipped
and transparent samples. With `repeat = true`, positive modulo wraps both axes
to the sprite's actual dimensions. Otherwise out-of-source samples are skipped,
even if colour zero is opaque. Camera, clip, blit mask and palette remap apply;
primitive fill patterns do not.

Starts and deltas must be finite and within ±32768. Each is converted once by
`trunc(value*65536)`; integer accumulation then retains exactly 16 fractional
bits without repeated floating-point rounding. Negative samples use floor, not
truncation. At most 4096 raster samples are allowed per call, keeping coordinates
and the accumulated values within exact integer arithmetic. The sample count is
`max(abs(x1-x0), abs(y1-y0)) + 1`; device work is `2 + ceil(samples/2)` for the
complete requested line, regardless of clipping. The work charge happens before
drawing. Invalid inputs fault with `M019009` at the source span; budget exhaustion
is `M019001`. Each call consumes one draw command and is forbidden in `on raster`.
The sampler has no persistent cursor: each call supplies its complete state.
Perspective projection and per-row/per-column deltas belong to ordinary MODL
source, not a host scene engine.

The `palt` mask affects sprites, transformed sprites, animation and map tiles.
It tests the original indexed source colour **before** the drawing palette remap.
For example, remapping opaque colour 2 to transparent colour 0 still draws it;
masking colour 2 skips it regardless of its remapped target. Solid primitives,
text, clear and direct framebuffer writes ignore this blit mask. `pal_reset`
does not change transparency. Every boot/frame resets the mask to only colour
zero transparent. Both transparency calls cost one device work unit and consume
one draw command; they are not legal in `on raster`. See the hardware reference
for the writable 256-bit mask and snapshot behaviour.

`map(asset, x, y)` draws every layer of a named tile map in order using 8x8 tiles. A cartridge can
read bounded map data with `map_cell(asset, layer, x, y)`, which returns `-1` outside the map, and
`map_flag(asset, layer, x, y, flag)`, which returns `false` outside the map or for a flag outside
0-7. Sprite pixels, animation frames, tiles, map cells, fonts, and raster data share 1 MiB.
Custom fonts do not replace the built-in console font: `print` always uses the fixed system glyphs,
while `font_print` uses the supplied asset and substitutes its declared missing glyph for unsupported
characters. Newlines reset X and move by the font's vertical advance.
The project display file can supply an initial logical remap and scanline states; source-level
palette and raster commands override those defaults. Asset file schemas are documented in
[ASSETS.md](ASSETS.md).

### Primitive fill patterns

`fill_pattern(mask: Int, second: Color, transparent: Bool)` enables a repeating
4×4 pattern for `pixel`, `line`, `rect`, `rect_fill`, `circle`, `circle_fill`
and `triangle`, including outlines. The mask must be an integer 0–65535. Bit
`(screen_y % 4) * 4 + screen_x % 4` selects the pixel: zero uses the drawing
call's colour, one uses `second`. Both selected colours pass through `pal`.
The pattern is anchored to the screen **after** camera subtraction; clipping
does not shift it. With `transparent = true`, one-bits leave existing pixels
unchanged instead of drawing `second`. The blit-transparency mask is unrelated
and does not suppress either primitive colour.

`fill_pattern_reset()` disables the pattern. Boot/frame reset also disables it;
`pal_reset` and `palt_reset` leave it alone. Clear, text, asset blits and raw
framebuffer writes ignore the pattern. Setting/resetting costs one device work
unit and one draw command; subsequent primitives retain their ordinary work
costs. Invalid arguments fault with `M019009` at the call's source span, without
changing pattern registers. Pattern calls are forbidden in `on raster`.
The [hardware reference](HARDWARE.md) defines raw bytes and snapshot restore.

## Raster display list

`on raster(line: Int)` runs once for each scanline from 0 through 143 after `on draw`. Only
`pal(from, to)`, `raster_scroll(x, y)` and memory operations are legal there. Memory writes
are restricted to raster-table addresses. Palette/scroll state is captured for that line and
remains in effect until changed by a later scanline. Scroll wraps the already drawn indexed back
buffer; palette changes happen during display resolution and do not alter framebuffer indices.

## Byte memory access

`mem_read(address)` and `mem_read16(address)` read unsigned bytes / little-endian 16-bit words.
`mem_write(address, value)` and `mem_write16(address, value)` write those widths; `mem_copy(dst,
src, count)` handles overlapping ranges and `mem_fill(dst, byte, count)` fills bytes. All arguments
are `Int`, writes return unit, and the normal compiler/runtime work budget applies. The complete current map
covers RAM, framebuffers, visuals, graphics/raster, input, system, ROM, saves
and synth/tracker state.
`visual_id(name: Text) -> Int` returns the visual descriptor ID or `-1`; its runtime cost is one plus
the name length. Descriptors expose sprite/frame/tile/map addresses, dimensions and flags. These
bytes are the backing state used by drawing and map queries. See the exact current
addresses, field encodings, timing, permissions and fault behavior in [HARDWARE.md](HARDWARE.md).
Studio, the native standalone player, and `mod01 run --headless` all execute this surface through
the same production console core.

## Input and deterministic utilities

Four controller values are available as `pad1` through `pad4`. Button values are `up`, `down`,
`left`, `right`, `a`, `b`, `x`, `y`, `l`, `r`, `start_button`, and `menu`.

- `btn(controller, button)` reports the current frame state.
- `btnp(controller, button)` reports a false-to-true transition in the current display frame.
- `rng_num()` returns a deterministic value in `[0, 1)`.
- `rng_int(minimum, maximum)` returns an unbiased deterministic integer in the half-open range.

The read-only system bus at `0x50200` exposes the actual frame/update counters, frame-derived time,
RNG state, work ledger and fault/phase status. Reads share ordinary bus costs and do not advance RNG
or time. See [HARDWARE.md](HARDWARE.md#scheduler-time-rng-work-and-fault-registers) for the byte layout,
fault latch and snapshot semantics.

The native host combines keyboard, pointer and gamepad input. Pointer coordinates
are clamped to the virtual display and recorded alongside all four controller ports.
These calls and the input bus read the same current/previous frames. Edges are not consumed by reads
and are not latched across a skipped 30 Hz update; see [input timing](HARDWARE.md#controller-and-pointer-registers).
`pointer_x()`/`pointer_y()` return that indexed-pixel position, `pointer_inside()` reports whether
the pointer is over the display, and `pointer_primary()`/`pointer_secondary()` report press
transitions for mouse, pen, or touch buttons.

## Audio

`sfx(sound)` starts a named synthesizer patch. `music(music)` starts its eight-channel tracker order
list, and `music_stop()` stops tracker sequencing. The public asset representation can express pulse,
triangle, saw, noise, and 4-32-entry wavetable oscillators; frame-based attack/decay/sustain/release,
pitch slide, triangle vibrato, volume, and pan. It cannot represent imported PCM samples. Voice
allocation is deterministic and steals the oldest voice when all eight are active.

`audio_id(name: Text) -> Int` returns an immutable audio descriptor ID or `-1`, costing one plus
the name length. The voice/tracker bus reads and controls the same synthesizer state as these calls;
see [audio registers](HARDWARE.md#synthesizer-and-tracker-controls) for layouts, numeric constraints,
timing, raw activation/volume changes and output-stage faults.

## Persistence

`save_get_int(key, fallback)` reads the cartridge's current integer save image.
`save_set_int(key, value)` changes it and commits the complete image. Keys are
1–64 canonical ASCII characters. Raw writes affect the same 8 KiB working image;
`save_commit()` commits it explicitly. The trusted native host persists only
committed data from successful frames, under the validated cartridge ID.
Snapshots include working, committed and pending state.

## Math, text and bit operations

See [STANDARD_LIBRARY.md](STANDARD_LIBRARY.md) for deterministic arithmetic,
trig, safe parsing/formatting, bounded Unicode text and explicit 32-bit operations.
