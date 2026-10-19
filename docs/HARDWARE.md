# MOD-01 current hardware contract

One machine is supported: hardware revision 2. Studio, CLI, native games and
headless execution share this deterministic production core. Other profiles,
snapshot schemas and cartridge encodings are rejected rather than migrated.

| Facility                                  |                          Value |
| ----------------------------------------- | -----------------------------: |
| Display / render cadence                  | 240×144 indexed pixels / 60 Hz |
| Update cadence                            |                    30 or 60 Hz |
| Master palette                            |              256 fixed colours |
| Shared visual / packed cartridge capacity |                  1 MiB / 1 MiB |
| Sprite axes / tiles                       |             1–128 pixels / 8×8 |
| Save block                                |                          8 KiB |
| Draw commands / work per boot or frame    |                 4,096 / 50,000 |
| Synth voices / tracker channels           |                          8 / 8 |
| Audio                                     |               48,000 Hz stereo |
| Controller ports                          |                              4 |
| Visual descriptors / allocations          |                  4,096 / 8,192 |

Framebuffers are separate from the shared visual payload. Graphics use indexed
nearest-neighbour sampling, not imported true-colour output or arbitrary alpha.

## Fixed master palette

The RGB values below are original to MOD-01. Authors can remap logical indices while drawing and
during scanout but cannot change these values. Indices 32–247 form a 6×6×6 RGB
cube (channels 0, 51, 102, 153, 204, 255); indices 248–255 use greys
26, 55, 84, 113, 142, 171, 200, 229. Remaps have 32 entries with targets 0–255;
direct indices 32–255 bypass the remap.

| Index | RGB       | Index | RGB       | Index | RGB       | Index | RGB       |
| ----: | :-------- | ----: | :-------- | ----: | :-------- | ----: | :-------- |
|     0 | `#17141f` |     8 | `#5b2938` |    16 | `#263c32` |    24 | `#243451` |
|     1 | `#292532` |     9 | `#8b3c47` |    17 | `#345f46` |    25 | `#345581` |
|     2 | `#403946` |    10 | `#bf5558` |    18 | `#4b8b58` |    26 | `#4b7db3` |
|     3 | `#5d5054` |    11 | `#ed7b69` |    19 | `#7fbd68` |    27 | `#73a9d1` |
|     4 | `#806a63` |    12 | `#5a3928` |    20 | `#203b47` |    28 | `#3e3154` |
|     5 | `#aa8b74` |    13 | `#89572e` |    21 | `#2e6571` |    29 | `#654777` |
|     6 | `#d5b992` |    14 | `#c18436` |    22 | `#43969a` |    30 | `#936397` |
|     7 | `#f4e5bd` |    15 | `#e7bd50` |    23 | `#75cbc0` |    31 | `#c38aae` |

## Byte bus

The production core exposes one current map. Native Studio/player and CLI
headless execution drive these same devices. Conformance tests cover mapped
regions, holes, permissions, timing, mixed high/low access and restore.

The address space is 22 bits: `0x000000` through `0x3fffff` (4 MiB of addresses, not 4 MiB
of work RAM). It leaves room for cartridge descriptors without taking bytes from the fixed 1 MiB
visual capacity. There are no address wraps or mirrored mappings. Every currently unmapped address
reads zero; writing one faults. Offsets below are hexadecimal; lengths and counts are decimal.

| Address  |     Bytes | Access | Actual backing state / reset                                                                                        |
| :------- | --------: | :----- | :------------------------------------------------------------------------------------------------------------------ |
| `00000`  |    65,536 | RW     | Work RAM, zero on cartridge load.                                                                                   |
| `10000`  |    34,560 | RW     | Front indexed framebuffer; zero before `on start`.                                                                  |
| `19000`  |    34,560 | RW     | Back indexed framebuffer; zero before `on start`.                                                                   |
| `22000`  |    34,560 | R      | Resolved indexed scanout; zero before `on start`.                                                                   |
| `50000`  |        80 | RW     | Draw camera/clip and logical palette remap; layout below.                                                           |
| `50050`  |        32 | RW     | Blit transparency bitset for all 256 source colours, bit zero set on boot/frame reset.                              |
| `50100`  |        48 | R      | Four controller ports and pointer, encoded from the machine's current/previous input frames. Reset zero.            |
| `50200`  |        64 | R      | Scheduler, deterministic time, RNG, work and fault registers, encoded from their actual owners; layout below.       |
| `50300`  |        24 | R      | Visual allocation status; six little-endian unsigned 32-bit fields, below.                                          |
| `50400`  |        48 | R      | Raster callback accumulator: two little-endian binary64 scroll values and 32 remap bytes. Reset `(0,0)` / identity. |
| `50500`  |        32 | mixed  | Save commit command and live status; layout below.                                                                  |
| `50600`  |        64 | R      | Hardware revision and immutable canonical-cartridge status; layout below.                                           |
| `51000`  |        32 | R      | Synth clock, allocation counter, active voice count and audio asset metadata.                                       |
| `51020`  |        16 | RW     | Tracker selection and position; zero means stopped.                                                                 |
| `51100`  |       512 | mixed  | Eight 64-byte voice records; controls are RW, allocation sequence/reserved tail are read-only.                      |
| `55000`  |     5,760 | RW     | 144 scanline records, 40 bytes each; layout below.                                                                  |
| `58000`  |     8,192 | RW     | Cartridge save working image, initialized from the isolated persisted block.                                        |
| `5a000`  |     8,192 | R      | Last committed save image.                                                                                          |
| `a0000`  |  variable | R      | Up to 4,096 visual asset descriptors, 32 bytes each.                                                                |
| `c0000`  |  variable | R      | Visual allocations, 24 bytes each; at most 8,192 entries.                                                           |
| `100000` | 1,048,576 | R      | Canonical cartridge ROM; absent image bytes read zero.                                                              |
| `200000` |     1,024 | R      | Immutable master palette, 256 RGBA tuples; alpha 255.                                                               |
| `210000` | 1,048,576 | RW     | Shared visual image, initialized from assets; unallocated tail zero.                                                |
| `3c0000` |  variable | R      | Audio asset descriptors, 32 bytes each; sorted by name, at most 4,096 entries.                                      |

All framebuffer bytes are palette indices 0–255. Raw writes bypass camera,
clip and logical remap, but not scanout remapping. `on start` draws and writes
against the same back buffer, publishes it before frame zero, and starts audio
commands without advancing synth time.

Global bindings initialize before `on start` inside that same start phase, after device attachment.
Initialization-time runtime calls share its work budget and live device state. The generated factory
does not execute these calls during construction. `tests/conformance/boot.modl` checks reset reads,
RNG/memory/drawing aliases, start-phase status, retained initializer attribution and the exact combined
boot-budget boundary in release/debug native and headless tests.

At each display frame's beginning, back receives front; draw state, raster records and the callback
accumulator reset to the cartridge display defaults. Drawing and bus writes then occur in program
order. Scanout reads back through the raster table after all 144 callbacks, publishes resolved pixels,
and copies back to front without changing either buffer's address. A front-only write during a frame
does not also change back and is overwritten at frame completion. Back writes affect that frame.
RAM persists until a cartridge restart or snapshot restore. Palette RGB values
are immutable; the blit transparency mask is configurable draw state.

Transparency bytes at `50050`–`5006f` use little-bit order: source colour `c`
is skipped if byte `c / 8` has bit `c % 8` set. `palt` and raw writes affect
these exact bytes. All bit patterns are valid. The mask is tested before palette
remapping by every asset blit, not by solid/text/raw framebuffer drawing. Only
bit zero is set on cartridge load and at each boot/display-frame draw-state
reset; `palt_reset` restores that default explicitly. Bus snapshots retain the
complete mask, and restore is transactional with the other devices. As with
other non-raster state, writes from `on raster` are rejected.

Draw-state offsets from `50000`:

| Offset     | Encoding                    | Meaning / frame reset                                              |
| :--------- | :-------------------------- | :----------------------------------------------------------------- |
| `00`, `08` | little-endian IEEE binary64 | Camera X/Y, zero.                                                  |
| `10`, `18` | little-endian IEEE binary64 | Clip X/Y, zero.                                                    |
| `20`, `28` | little-endian IEEE binary64 | Clip width/height, 240/144.                                        |
| `30`–`4f`  | 32 unsigned bytes           | Logical palette remap, identity or cartridge display-file default. |

The six binary64 fields must remain safe integers; widths/heights must also be nonnegative. This
encoding preserves the MODL `Int` coordinate range. High-level `camera`/`clip`/`pal` access these exact bytes. Low-level code can copy an
entire eight-byte field transactionally from RAM; an intermediate noninteger, infinity or NaN faults.
For example binary64 `2` has word `0x4000` at field offset six and zero in its other three words.

Raster record offsets from `55000 + line*40`:

| Offset     | Encoding                    | Meaning                                              |
| :--------- | :-------------------------- | :--------------------------------------------------- |
| `00`       | unsigned byte 0 or 1        | Whether this line changes the current scanout state. |
| `01`–`03`  | zero bytes                  | Padding; writes must keep these zero.                |
| `04`, `06` | little-endian signed 16-bit | Wrapped scanout X/Y offset.                          |
| `08`–`27`  | 32 unsigned bytes 0–255     | Complete scanout remap.                              |

With no display-file record, enable and all other bytes reset to zero. Disabled records inherit the
last enabled line, initially zero scroll and identity remap. `pal`/`raster_scroll` in a raster callback
update their accumulator and write a **complete** enabled record for that line; callback scroll values
are clamped to signed 16-bit on capture, deterministically. Direct table writes share that record, with
later writes winning. Reads are allowed during any callback; raster-time writes are restricted to
the raster table. Effects remain bounded by the normal frame work budget.

### Memory operations, faults and snapshots

All arguments/results below are MODL `Int` except the unit-returning writes. Words are unsigned
little-endian 16-bit and need not be aligned. `mem_copy` uses source bytes as they stood before the
operation, including when source and destination overlap.

| Call                                                  | Runtime work units |
| :---------------------------------------------------- | -----------------: |
| `mem_read(address)` / `mem_write(address, value)`     |                  1 |
| `mem_read16(address)` / `mem_write16(address, value)` |                  2 |
| `mem_copy(destination, source, count)`                |      `1 + 2*count` |
| `mem_fill(destination, byte, count)`                  |        `1 + count` |

The compiler also charges its existing two units per API call. Bus writes are not high-level draw
commands and do not consume that separate 4,096-command allowance. Addresses/counts and unsigned
value width are validated first; work is charged before allocating a bulk staging buffer. Every
destination's permission and resulting register value is checked before any destination changes,
including operations crossing region boundaries. Failed operations retain their charged work.
An empty copy/fill costs one unit and accepts the one-past-end address `0x400000`; no byte is accessed.

`M019020` denotes an invalid address/range, `M019021` a reserved/read-only destination, `M019022` an
invalid byte/word/register value, and `M019011` a write outside the raster facility during `on raster`.
`M019001` remains the work-limit fault. All carry the originating MODL span. Type-invalid source is rejected before execution.

Current core snapshots use revision 6 and machine snapshots revision 2. They
retain RAM, visual pixels/tail, graphics/raster, synth/tracker, saves, input,
scheduler, RNG, work attribution and terminal faults. Restore validates the
complete state and rolls back on failure. Older schemas are errors.
Snapshots are boot/frame-boundary records; live statement suspension uses
compiler-generated continuations, not serialized host generators.

`tests/conformance/memory.modl` runs through the actual release/debug compiler
and production core. Bus tests cover permissions, bounds, unaligned words,
overlap, exact costs, table boundaries and transactional rollback.

### Cartridge save image and commit control

The isolated 8 KiB save device has a writable working image at `58000` and a read-only committed
latch at `5a000`. Both are byte-exact and reset from the host-provided save block. Raw writes change
only the working image. Writing byte value `1` to `50500`, or calling `save_commit()`, atomically
copies all 8,192 bytes into the committed latch and queues that complete image for the trusted host
after the frame succeeds. Writing zero is a no-op; other values and multi-byte writes that cross
into status space fault transactionally. Commit costs 8,192 work units in addition to an MMIO write
or the compiler's normal API-call charge. Save writes are forbidden during raster callbacks.

Status bytes at `50500` are:

| Offset    | Encoding | Meaning                                                                    |
| :-------- | :------- | :------------------------------------------------------------------------- |
| `00`      | u8 W     | Commit command: zero no-op, one commit. Reads zero.                        |
| `01`      | u8 R     | Bit 0 working differs from committed; bit 1 an image awaits host delivery. |
| `02`–`03` | zero R   | Reserved.                                                                  |
| `04`      | u32 R    | Capacity, always 8,192.                                                    |
| `08`      | u64 R    | Successful commit count, reset zero and exact through `2^53-1`.            |
| `10`      | u32 R    | Number of working bytes that differ from the committed image.              |
| `14`–`1f` | zero R   | Reserved.                                                                  |

The high-level `save_get_int`/`save_set_int` API uses a canonical sorted
JSON integer object in the same working image. Setting an integer commits the
image immediately. Raw edits to a valid integer image are visible through the
API; arbitrary binary images are legal but integer calls on them fault without
replacing bytes. Only a completed successful frame delivers committed data to
the trusted host. Current snapshots retain both images and pending commit state.

`tests/conformance/save.modl` checks initial host bytes, raw/API aliasing,
dirty state, commits, host output and restore/forward equality.

### Cartridge status and ROM

The trusted host supplies a cloned canonical cartridge image, mapped read-only
at `100000` with a 1 MiB limit. Absent image/tail bytes read zero; no host path,
URL or JavaScript object is exposed. Code pays normal bus read/copy costs.

The 64 read-only bytes at `50600` are:

| Offset    | Encoding | Meaning                                                                       |
| :-------- | :------- | :---------------------------------------------------------------------------- |
| `00`      | u16      | Hardware revision, always 2.                                                  |
| `02`      | u16      | Canonical cartridge format revision, 2 for a valid current header, else zero. |
| `04`      | u32      | Mapped canonical image length in bytes.                                       |
| `08`      | u32      | Fixed complete-cartridge capacity, 1,048,576.                                 |
| `0c`      | u32      | ROM base address, `100000`.                                                   |
| `10`      | u32      | Flags: bit 0 image present; bit 1 canonical current header valid.             |
| `14`      | u32      | Header entry count when bit 1 is set, otherwise zero.                         |
| `18`–`3f` | zero     | Reserved.                                                                     |

Header validation here is informational: the trusted pack/import boundary
performs full validation. The flag recognizes `MOD-01`, byte `1a`, revision 2
and a complete 12-byte header. ROM/status writes fault transactionally.
Studio packs the active source first so the debugger and cartridge observe
the actual artifact being exported.

Native debug messages allow bounded 1–256 byte bus inspection/editing only at
paused message boundaries. These uncharged operations use the actual mapped
devices and transactional validators. Release runtimes reject them.
Read-only and reserved addresses remain protected.

### Scheduler, time, RNG, work and fault registers

These read-only MMIO fields at `50200` encode actual machine state at the instant of the read. There
is no periodically synchronized register image. Multi-byte fields are little-endian. Unsigned 64-bit
fields represent exact integers through `2^53-1`; their unused upper bits are zero.

| Offset     | Encoding      | Meaning                                                                                 |
| :--------- | :------------ | :-------------------------------------------------------------------------------------- |
| `00`       | u64           | Display frames completed; also the current callback's frame index. Reset zero.          |
| `08`       | u64           | Successful update callbacks completed. Reset zero.                                      |
| `10`       | IEEE binary64 | Cartridge seconds, exactly the machine's `frame / 60` calculation; no host clock.       |
| `18`       | u32           | Current normalized RNG state; high-level RNG calls advance this same owner.             |
| `1c`       | u8            | Configured update cadence, 30 or 60.                                                    |
| `1d`       | u8            | Phase: 0 idle, 1 start, 2 update, 3 draw, 4 raster, 5 output (scanout and mixing).      |
| `1e`       | u16           | Active raster line, 0–143; `65535` outside raster.                                      |
| `20`       | u64           | Current boot/frame work usage, including the charge for this read or copy.              |
| `28`       | u64           | Actual work limit, 50,000 in production; internal diagnostic hosts may lower it.        |
| `30`       | u8            | Status bits: 0 boot completed, 1 callback active, 2 terminal fault.                     |
| `31`–`33`  | zero          | Reserved.                                                                               |
| `34`       | u16           | Numeric `M019xxx` fault code; zero when healthy, 9199 for an unexpected host exception. |
| `36`–`37`  | zero          | Reserved.                                                                               |
| `38`, `3c` | u32           | Fault source span start/end; zero when absent or unavailable.                           |

Updates run on every frame at 60 Hz and even-indexed frames at 30 Hz. The update counter advances
only after its callback returns successfully; draw/raster see that completed count. The frame
counter advances only after all callbacks, scanout and audio mixing complete. Work resets before start and before each frame,
not when read or snapshotted. A fault retains the active phase/line, work and source span; the active
status bit clears. Ordinary fault attempts leave the original exception and message intact. Further
execution attempts fail with `M019014` before resetting devices, without replacing the original latch.
Restart or restore a healthy checkpoint to resume. Restoring a faulted checkpoint keeps it faulted.
Attempting another frame at `2^53-1` completed frames faults with `M019012`, without counter wrap.

Current machine snapshots are accepted only at idle or terminal-fault boundaries, not during a live
callback. They validate cadence, limit and counter/phase consistency before restore. This does not
implement resumable source-statement pauses. The public `tests/conformance/system.modl` exercises
all callback phases and scanlines, mixed RNG/register access, counters, status and charged reads.
Native Release/Debug tests run it at 30/60 Hz, check binary64 time and copied register bytes, and
replay complete snapshots.

### Synthesizer and tracker controls

Audio MMIO reads and writes the existing synthesizer's actual voices and tracker, not a retained
copy. A write stages the touched control record, validates it, and commits only after every target
region passes permission/value checks. Failed operations retain work charges but do not change
controls. These regions are not independently retained in the bus snapshot: the existing synth
snapshot is their owner. All fields below are little-endian; normal byte-bus costs apply. Reads are
legal during raster; writes are not. They do not consume the high-level draw-command allowance.

`audio_id(name: Text) -> Int` returns a zero-based ID, or `-1` when absent, costing `1 + name.length`
runtime units plus the normal API-call charge. Sound and music share one name-sorted ID space.
Register references use `ID + 1`, reserving zero for no asset. IDs and descriptors are immutable
for a loaded cartridge. Descriptor words at `3c0000 + ID*32` are eight u32 fields: kind (1 sound,
2 music), waveform (1 pulse, 2 triangle, 3 saw, 4 noise, 5 wavetable), default note, duration,
release frames, tracker frames-per-row, order length and loop flag. Inapplicable fields are zero.

Synth status at `51000`:

| Offset     | Encoding | Meaning / reset                                            |
| :--------- | :------- | :--------------------------------------------------------- |
| `00`       | u64      | Completed audio frames, zero; boot does not advance audio. |
| `08`       | u64      | Next voice-allocation sequence, one.                       |
| `10`       | u8       | Active voices, zero.                                       |
| `11`, `12` | u8       | Voice/tracker channel capacities, both eight.              |
| `13`       | u8       | Tracker active, zero.                                      |
| `14`       | u32      | Sample rate, 48,000.                                       |
| `18`       | u32      | Number of audio assets.                                    |
| `1c`       | u32      | Audio descriptor base address, `3c0000`.                   |

Tracker controls at `51020` are four u32 fields: music reference, order index, row and frame-in-row.
Selecting a music asset from stopped state starts at `(0,0,0)`; positions must exist in that asset.
Use a single 16-byte copy to change selection and position together when an old position would be
invalid for a new song. Writing music reference zero stops and clears the position; position writes
while stopped are discarded. `music` selects and resets position, and `music_stop` clears this same
state. The sequencer emits notes and advances its position at output time, before mixing that frame.

Voice record at `51100 + slot*64`, for slots 0–7:

| Offset    | Encoding / access | Meaning / reset                                                    |
| :-------- | :---------------- | :----------------------------------------------------------------- |
| `00`      | u8 RW             | Active, 0 or 1; reset zero.                                        |
| `01`–`03` | zero RW           | Reserved; writes must keep zero.                                   |
| `04`      | u32 RW            | Sound reference; reset zero. Music/unknown references are invalid. |
| `08`      | binary64 RW       | Note 0–127; reset zero.                                            |
| `10`      | binary64 RW       | Volume scale 0–1; reset one.                                       |
| `18`      | u64 RW            | Voice age in frames; reset zero.                                   |
| `20`      | binary64 RW       | Oscillator phase in `[0,1)`; reset zero.                           |
| `28`      | u32 RW            | Nonzero noise state; reset one.                                    |
| `2c`–`2f` | zero RW           | Reserved; writes must keep zero.                                   |
| `30`      | u64 R             | Allocation sequence; reset zero.                                   |
| `38`–`3f` | zero R            | Reserved.                                                          |

Integer counters are exact through `2^53-1`; floats must be finite and within their stated ranges.
An active voice needs a valid sound and age below duration plus release. `sfx` initializes the first
free voice, otherwise steals the oldest allocation, and writes these same fields immediately.
Raw changes do not implicitly restart phase/age or allocate a sequence. Setting active to zero
stops the voice; manually activating a configured inactive voice retains its prior allocation sequence.
Raw note/volume/phase changes affect the next output mix. Tracker notes may subsequently allocate
or steal voices in that same frame. Rendering uses the existing exact binary64 state and oscillator
calculations.

Audio-frame and allocation-counter exhaustion fault with `M019012` before the overflowing increment.
An `sfx` fault carries its source span; output-stage hardware faults have span `(0,0)`. Output faults
are latched inside the machine's frame boundary, retaining phase 5 and not advancing its completed
frame counter. Earlier successful commands may remain in the fault snapshot, just as with callback
faults; restoring a healthy checkpoint is the recovery path.

`tests/conformance/audio` is an ordinary source-visible cartridge. Native Release/Debug tests verify
all eight voices, descriptors, command/register aliases, alternating raw-muted/audible PCM and full
replay. Studio's paused memory panel exposes these registers and watchpoints.

### Visual image and allocation descriptors

`visual_id(name: Text) -> Int` returns a visual asset's zero-based descriptor ID, or `-1` if absent.
IDs follow ascending ASCII manifest-name order, independent of declaration order. The lookup costs
`1 + name.length` runtime units plus the usual compiler call charge and is legal in raster callbacks.
Sound/music are not visual assets. Custom bitmap fonts are visual assets and the built-in system
font remains separate and immutable.

The six `50300` status words are asset count, declared visual bytes, descriptor base `a0000`,
allocation count, allocation base `c0000`, and display-default byte address (zero if absent).
All descriptor fields below are unaligned-safe, little-endian unsigned 32-bit values.

| Record               | Offset           | Meaning                                                                              |
| :------------------- | :--------------- | :----------------------------------------------------------------------------------- |
| Asset, 32 bytes      | `00`             | Kind: sprite 1, animation 2, tile set 3, map 4, font 5.                              |
| Asset                | `04`, `08`, `0c` | Allocation count, first allocation descriptor address, total payload bytes.          |
| Asset                | `10`–`1f`        | Reserved zero.                                                                       |
| Allocation, 24 bytes | `00`             | Payload kind: pixels 1, map cells 2, flags 3, display remap 4, raster row 5, font 6. |
| Allocation           | `04`, `08`       | Payload byte address and length.                                                     |
| Allocation           | `0c`, `10`       | Pixel/cell width and height; byte-width and 1 for flags/display records.             |
| Allocation           | `14`             | Map tileset asset ID plus one, otherwise zero.                                       |

Assets are packed consecutively in ID order with **no alignment padding**: a sprite's pixels;
an animation's frames in order; a tileset's tile pixels in order followed by its flag bytes;
a map's layers in order; a font as one header and sorted glyph table. Map cells are unsigned little-endian 16-bit indices regardless of host
endianness. The renderer and `map_cell` read these exact words; `map_flag` reads the current mapped
tile flags. Pixel bytes must remain 0–255 and map words must reference an existing tile. A partial
word write validates the resulting whole word. Multi-allocation writes validate before any changes.
Sprite/map/flag changes affect subsequent high-level calls immediately. Raster callbacks may read
these regions but cannot write them. Assets and allocation metadata cannot be resized at runtime.

A font allocation starts with six bytes for glyph width, glyph height, baseline, horizontal
advance, vertical advance and fallback code, then a little-endian u16 glyph count. Each strictly
ascending glyph record is a little-endian u16 code followed by `width * height` one-byte mask
values. Font allocation width/height repeat the glyph dimensions and its reference is the fallback
code. Header/code bytes are immutable; glyph masks accept only 0 or 1 and affect later
`font_print` calls immediately.

Optional display defaults follow the named assets: 32 remap bytes then 38 bytes per configured
raster row (LE u16 line, LE i16 X/Y scroll, 32 remap bytes). The line bytes must retain their original
value; scroll and remap are writable. Defaults are latched at frame start, so changes affect the next
frame rather than its already initialized live registers. These bytes count toward the same 1 MiB
capacity. The unallocated tail is writable byte scratch space; descriptor
`used` counts declared payload, not nonzero bytes. Restart reloads source; snapshots retain the tail.

Descriptor bytes are machine metadata outside the visual payload budget.
There are at most 4,096 assets and 8,192 allocations. The allocation table ends
at `f0000`, before ROM; exceeding its count faults at load, even when pixel
bytes still fit. Only actual records are mapped. There are no mirrored banks
or alternate address windows.

`tests/conformance/visual/cart.m01w` exercises descriptor discovery, unaligned
cells, sprite/tile/flag writes and mixed high-level drawing/query calls.
The service and hardware-gauntlet workcarts expose diagnostic pages.

### Controller and pointer registers

The input region is read-only memory-mapped I/O (MMIO): reads encode the actual input frames used
by `btn`, `btnp` and pointer calls. There is no separately refreshed register image. Machine
snapshots already retain these frames, so restoring them immediately changes both API and bus
observations without a new snapshot revision. Byte/word/copy costs are the ordinary bus costs;
reading input does not consume or acknowledge an edge. Writes fault with `M019021` in every phase.

Each of four ports occupies eight bytes at `50100 + (port-1)*8`:

| Offset | LE unsigned 16-bit mask                                |
| :----- | :----------------------------------------------------- |
| `00`   | Current held buttons.                                  |
| `02`   | Previous display frame's held buttons.                 |
| `04`   | Pressed this display frame: current AND NOT previous.  |
| `06`   | Released this display frame: previous AND NOT current. |

Bits 0–11 are up, down, left, right, A, B, X, Y, L, R, start, menu; bits 12–15 read zero.
The pointer follows at `50120`: current X/Y at offsets `00`/`02`, previous X/Y at `04`/`06`
(LE u16). Offsets `08`/`09` are current/previous flags, `0a`/`0b` are pressed/released flag
edges. Flag bits 0/1/2 mean primary, secondary and inside. Offsets `0c`–`0f` read zero.
Coordinates remain within 0–239 and 0–143; invalid frames, missing ports or malformed button sets
are rejected with `M019008` by the core before any device frame reset.

Inputs are sampled once at the start of **each 60 Hz display frame**, before callbacks. They remain
stable throughout that frame's update/draw/raster calls. In particular: in a 30 Hz cartridge a press first sampled on an odd, skipped-update frame is visible to
that frame's drawing but is not latched for the next update. A held button is still visible through
`btn`. The focused scheduler test reproduces this exact sequence and its restore behavior; it is
not inferred from host key timing. Input edges are not update-latched.

`tests/conformance/input.modl` checks all twelve buttons on every port, high/low API equivalence,
held/press/release transitions, pointer state, reset and copying registers to RAM. Native tests run
36-frame scripted traces at both 30/60 Hz in Release/Debug with restore/forward checks. Native input adapters provide keyboard and physical gamepad ports.

## Synthetic work model

Native Studio, native games and CLI headless runs use deterministic seed
`604772761` by default. `--seed` overrides it in headless runs; internal native
conformance loads can supply the same explicit seed. Equal source/assets/input,
save image and seed produce equal devices across these hosts.

Work units are deterministic accounting units, not real CPU instructions and not evidence for a
fictional clock speed. The compiler currently charges 8 units at each function/callback entry, 4 at
loop back-edges, 4 per task-state transition, 4 per allocation or fixed-capacity write, one per
materialized range element, and 2 before each console API call. Charging a range before allocation
lets the frame budget reject an extreme dynamic range without first constructing it.
Enum payload equality additionally charges one unit per payload slot or traversed nested field;
payload-free enum comparisons keep their existing work cost and survive snapshot cloning.
The runtime charges 1,080 units for `clear`; one for a pixel; `max(abs(dx), abs(dy)) + 1` for a line;
`2*abs(width) + 2*abs(height)` for an outline rectangle; `ceil(abs(width*height)/4)` for a filled
rectangle; `8*abs(radius)` for a circle outline; `ceil(3*radius^2/4)` for a filled circle; and
`ceil(abs(cross-product)/8)` for a filled triangle. A normal sprite/animation costs 32 units, a
transformed sprite costs `max(32, 32*scale^2)`, a map draw costs 128, printed text costs six per
character, and an audio command costs eight. State-only graphics calls cost one. These are simple
deterministic estimates, not timing predictions. Charges retain source spans for profiler attribution
and budget faults.

The per-frame work-unit ceiling is 50,000. The runtime stops at the first charge that exceeds it and
reports the responsible source span. Measure actual paths with `mod01 info` or `mod01 run --headless`; inputs determine peaks.

The protocol and shared core reject a configuration above the fixed ceiling before constructing a
cartridge; internal diagnostic tests may use a stricter limit. Studio takes its normal limit from the
same `HARDWARE.workUnitsPerFrame` constant. A faulted work counter saturates at the largest safe MODL
integer (`2^53-1`) if an enormous charge would overflow it; the charge still faults at its source
span before a bulk operation proceeds. Attribution retains the same saturated total. Ordinary
in-budget accounting and first-party frame costs are unchanged.
