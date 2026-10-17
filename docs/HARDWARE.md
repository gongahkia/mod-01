# MOD-01 Hardware Revision 1

## Hardware Revision 2 additions

Projects opt in with `hardware_revision = 2`; newly created native projects do so by default.
Revision 2 keeps the 240×144 display, 60 Hz rendering, save size, work budget, and first 32
palette colours. It raises the packed cartridge and shared visual capacities to 1 MiB, sprite
axes to 128 pixels, and the master palette to 256 fixed RGB entries. Indices 32–247 form a 6×6×6
RGB cube at channel values 0, 51, 102, 153, 204, and 255. Indices 248–255 are eight greys.
The native editor creates 64×64 sprites by default. Sprite file revision 2 carries the larger
dimensions and colour indices; file revision 1 retains its 64-pixel, 32-colour validation.

The legacy 128 KiB visual address window at `0x30000` still aliases the first 128 KiB. When a
visual bank grows beyond it, allocation descriptors point into a contiguous 1 MiB bank at
`0x210000`; both windows refer to the same backing pixels. The ROM begins at its original
`0x60000` address and continues at `0x100000` after the first 256 KiB. The first 32 palette
entries stay at `0x50080`; entries 32–255 are at `0x200000`. Each of the 32 logical remap entries
can target any of the 256 colours. Indices 32–255 used directly bypass the 32-entry logical
remap. Cartridge status reports the capacity for the cartridge's hardware revision.

The rest of this document records the frozen Revision 1 profile and original addresses.

This is the frozen byte-addressed machine contract used by the Worker, standalone player and
headless host. Its public limits retain the measured alpha profile; [LIMITS.md](LIMITS.md) records
both the baseline and V1 measurements.

| Facility                        |       Revision 1 value |
| ------------------------------- | ---------------------: |
| Display                         | 240x144 indexed pixels |
| Render cadence                  |                  60 Hz |
| Update cadence                  |            30 or 60 Hz |
| Master palette                  |       32 fixed colours |
| Visual asset capacity           |                128 KiB |
| Cartridge capacity              |                256 KiB |
| Cartridge save block            |                  8 KiB |
| Draw commands                   |        4,096 per frame |
| Synth voices / tracker channels |                  8 / 8 |
| Synth output                    |       48,000 Hz stereo |
| Controller ports                |                      4 |
| Work units                      |       50,000 per frame |

Framebuffer storage is double-buffered and separate from visual assets. Sprites may be 1-64 pixels
per axis; sprites, animation frames, 8x8 tiles, maps, fonts, and raster data share visual capacity.
One colour index is transparent. Scaling and rotation use deterministic nearest-neighbour sampling.
No operation exposes arbitrary alpha, bilinear filtering, imported samples, or true-colour output.

## Fixed master palette

The RGB values below are original to MOD-01. Authors can remap logical indices while drawing and
during scanout but cannot change these values.

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

The Worker-owned production core exposes the following complete Hardware Revision 1 map. Studio,
standalone HTML and native headless execution all drive that production core. Executable conformance
projects plus lower-level property tests cover mapped regions, reserved holes, permissions, timing,
mixed high/low access and cross-host behavior.

The address space is 22 bits: `0x000000` through `0x3fffff` (4 MiB of addresses, not 4 MiB
of work RAM). It leaves room for cartridge descriptors without taking bytes from the fixed 128 KiB
visual capacity. There are no address wraps or mirrored mappings. Every currently unmapped address
reads zero; writing one faults. Offsets below are hexadecimal; lengths and counts are decimal.

| Address  |    Bytes | Access | Actual backing state / reset                                                                                        |
| :------- | -------: | :----- | :------------------------------------------------------------------------------------------------------------------ |
| `00000`  |   65,536 | RW     | Work RAM, zero on cartridge load.                                                                                   |
| `10000`  |   34,560 | RW     | Front indexed framebuffer; zero before `on start`.                                                                  |
| `19000`  |   34,560 | RW     | Back indexed framebuffer; zero before `on start`.                                                                   |
| `22000`  |   34,560 | R      | Resolved indexed scanout; zero before `on start`.                                                                   |
| `30000`  |  131,072 | RW     | Packed visual image, initialized from cartridge assets; unallocated tail is zero.                                   |
| `50000`  |       80 | RW     | Draw camera/clip and logical palette remap; layout below.                                                           |
| `50050`  |        1 | R      | Actual sprite transparency index, fixed at zero.                                                                    |
| `50080`  |      128 | R      | Immutable master palette, 32 RGBA byte tuples; alpha always 255.                                                    |
| `50100`  |       48 | R      | Four controller ports and pointer, encoded from the machine's current/previous input frames. Reset zero.            |
| `50200`  |       64 | R      | Scheduler, deterministic time, RNG, work and fault registers, encoded from their actual owners; layout below.       |
| `50300`  |       24 | R      | Visual allocation status; six little-endian unsigned 32-bit fields, below.                                          |
| `50400`  |       48 | R      | Raster callback accumulator: two little-endian binary64 scroll values and 32 remap bytes. Reset `(0,0)` / identity. |
| `50500`  |       32 | mixed  | Save commit command and live status; layout below.                                                                  |
| `50600`  |       64 | R      | Hardware revision and immutable canonical-cartridge status; layout below.                                           |
| `51000`  |       32 | R      | Synth clock, allocation counter, active voice count and audio asset metadata.                                       |
| `51020`  |       16 | RW     | Tracker selection and position; zero means stopped.                                                                 |
| `51100`  |      512 | mixed  | Eight 64-byte voice records; controls are RW, allocation sequence/reserved tail are read-only.                      |
| `55000`  |    5,760 | RW     | 144 scanline records, 40 bytes each; layout below.                                                                  |
| `58000`  |    8,192 | RW     | Cartridge save working image, initialized from the isolated persisted block.                                        |
| `5a000`  |    8,192 | R      | Last committed save image.                                                                                          |
| `60000`  | variable | R      | Exact canonical `.m01c` bytes supplied by the trusted host, up to 256 KiB.                                          |
| `a0000`  | variable | R      | Up to 4,096 visual asset descriptors, 32 bytes each.                                                                |
| `c0000`  | variable | R      | Visual allocations, 24 bytes each; at most 131,072 entries.                                                         |
| `3c0000` | variable | R      | Audio asset descriptors, 32 bytes each; sorted by name, at most 4,096 entries.                                      |

All framebuffer bytes must be indices 0–31. Raw writes bypass draw camera, clip and logical remap;
they do not bypass scanout remapping. `on start` now executes both high-level drawing and bus writes
against the same back buffer and publishes it once before frame zero. It starts synth commands
without advancing audio time. Alpha discarded start-time draw/audio commands; preserving them is the
explicit V1 boot semantic required for coherent mixed access. The three original game traces remain
the compatibility boundary for their actual behavior.

Global bindings initialize before `on start` inside that same start phase, after device attachment.
Initialization-time runtime calls share its work budget and live device state. The generated factory
does not execute these calls during construction. `tests/conformance/boot.modl` checks reset reads,
RNG/memory/drawing aliases, start-phase status, retained initializer attribution and the exact combined
boot-budget boundary in native Release/Debug tests; Firefox runs it through Wasm and the Worker.

At each display frame's beginning, back receives front; draw state, raster records and the callback
accumulator reset to the cartridge display defaults. Drawing and bus writes then occur in program
order. Scanout reads back through the raster table after all 144 callbacks, publishes resolved pixels,
and copies back to front without changing either buffer's address. A front-only write during a frame
does not also change back and is overwritten at frame completion. Back writes affect that frame.
RAM persists until a cartridge restart or snapshot restore. Palette RGB/transparency are immutable.

Draw-state offsets from `50000`:

| Offset     | Encoding                    | Meaning / frame reset                                              |
| :--------- | :-------------------------- | :----------------------------------------------------------------- |
| `00`, `08` | little-endian IEEE binary64 | Camera X/Y, zero.                                                  |
| `10`, `18` | little-endian IEEE binary64 | Clip X/Y, zero.                                                    |
| `20`, `28` | little-endian IEEE binary64 | Clip width/height, 240/144.                                        |
| `30`–`4f`  | 32 unsigned bytes           | Logical palette remap, identity or cartridge display-file default. |

The six binary64 fields must remain safe integers; widths/heights must also be nonnegative. This
encoding preserves the existing MODL `Int` coordinate range instead of silently narrowing old camera
and clip calls. High-level `camera`/`clip`/`pal` access these exact bytes. Low-level code can copy an
entire eight-byte field transactionally from RAM; an intermediate noninteger, infinity or NaN faults.
For example binary64 `2` has word `0x4000` at field offset six and zero in its other three words.

Raster record offsets from `55000 + line*40`:

| Offset     | Encoding                    | Meaning                                              |
| :--------- | :-------------------------- | :--------------------------------------------------- |
| `00`       | unsigned byte 0 or 1        | Whether this line changes the current scanout state. |
| `01`–`03`  | zero bytes                  | Padding; writes must keep these zero.                |
| `04`, `06` | little-endian signed 16-bit | Wrapped scanout X/Y offset.                          |
| `08`–`27`  | 32 unsigned bytes 0–31      | Complete scanout remap.                              |

With no display-file record, enable and all other bytes reset to zero. Disabled records inherit the
last enabled line, initially zero scroll and identity remap. `pal`/`raster_scroll` in a raster callback
update their accumulator and write a **complete** enabled record for that line; callback scroll values
are clamped to signed 16-bit on capture, matching alpha. Direct table writes share that record, with
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
`M019001` remains the work-limit fault. All carry the originating MODL span. Type-invalid source is
rejected by the compiler before execution. New memory built-ins are resolved on first use to retain
old programs' symbol IDs and existing user functions with those names.

Internal core snapshot revision 6 retains RAM and all mutable/retained bus regions, including the
visual image, plus save working/committed images and commit state in their authoritative owner. Its
revision-2 machine snapshot also retains update cadence, work limit/usage/attribution,
boot status, completed update count and terminal execution/fault state. Its existing
framebuffer projection must agree with the memory image. Restore checks region layout and values
before mutation and rolls back device, machine, save and pending-write state on a failure. Revision-2
frame snapshots migrate with zero RAM and source-initialized visuals; revision-3 snapshots retain
their existing bus state and initialize the newly mapped visuals from source. Revision-4 snapshots
retain their complete device/bus images. All four legacy formats require a revision-1 machine snapshot:
missing work/attribution become zero/empty, completed updates are derived from frame/cadence, and
phase/fault become idle/none. Existing boot status is preserved, or set true when a legacy frame is
nonzero. Raw alpha revision-1 snapshots still restore only their original machine/save fields, plus
these explicit metadata defaults. Bus snapshots remain frame-boundary records; source-statement
suspension is represented by compiler-generated debug continuations, and public revision-0 replays
migrate to the revision-1 `.m01rec` envelope before execution.

`tests/conformance/memory.modl` runs through the native compiler and shared production core in release
and debug tests, and through Wasm and the actual Worker in the browser matrix. Lower-level bus tests
cover all mapped regions, all 144 raster rows, mixed high/low writes, reset, permissions, bounds,
unaligned words, overlaps, exact work charges and rollback.

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

The alpha `save_get_int`/`save_set_int` API is retained. Its canonical sorted JSON integer object
occupies the same working bytes; `save_set_int` also commits immediately so existing cartridges keep
their persistence behavior. Raw edits to a valid integer image are visible to `save_get_int`.
Arbitrary binary images are legal, but integer API calls then fault without replacing those bytes.
The host receives only a successful committed frame, never a partial write. Snapshot revision 6
retains working bytes, the committed latch, dirty/pending status, count, and compatible legacy
pending integer-write reports. Restores validate everything before mutation; alpha snapshot
revisions 1–5 migrate their integer object into both images without changing archived fixtures.

`tests/conformance/save.modl` is ordinary MODL source. Native Release/Debug runs cover initial host
bytes, raw/high-level aliasing, dirty state, explicit and compatibility commits, host output and
restore/forward equality. Lower-level tests cover binary images, permissions, capacity, work and
counter faults, sparse/malformed snapshots, complete-image cloning and transactional rollback.

### Cartridge status and ROM

The trusted host may provide the exact canonical `.m01c` image when it constructs a runtime. The
Worker clones it, maps those immutable bytes at `60000`, and never exposes a host file, URL or
JavaScript object. A configuration over the fixed 256 KiB cartridge limit is rejected before the
machine starts. Hosts that have no canonical image leave the ROM range reserved; the status block
still describes that absence. Cartridge code pays normal bus read/copy costs.

The 64 read-only bytes at `50600` are:

| Offset    | Encoding | Meaning                                                                  |
| :-------- | :------- | :----------------------------------------------------------------------- |
| `00`      | u16      | Hardware revision, currently 1.                                          |
| `02`      | u16      | Canonical cartridge format revision, 1 for a valid V1 header, else zero. |
| `04`      | u32      | Mapped canonical image length in bytes.                                  |
| `08`      | u32      | Fixed complete-cartridge capacity, 262,144.                              |
| `0c`      | u32      | ROM base address, `60000`.                                               |
| `10`      | u32      | Flags: bit 0 image present; bit 1 canonical V1 header valid.             |
| `14`      | u32      | Header entry count when bit 1 is set, otherwise zero.                    |
| `18`–`3f` | zero     | Reserved.                                                                |

Header validation here is informational and deliberately small: the trusted pack/import boundary
does complete cartridge validation. The status flag recognizes `MOD-01`, byte `1a`, revision 1 and
a complete 12-byte header. ROM and status writes fault transactionally. The Studio run and debugger
pack the active project first and pass the resulting bytes, so cartridge code and the debugger see
the same artifact the user would export. `tests/conformance/memory.modl` reads the status and magic
through public bus calls in native Release/Debug and Firefox.

The debug-only Worker protocol permits bounded 1–256 byte inspection and editing only at idle
message boundaries. Inspection is uncharged and reports the real mapped-region names, bounds and
permissions; edits are uncharged, transactional, and go through the same device validators as
cartridge writes. Release runtimes reject both operations, and read-only/reserved ranges remain
protected. Studio's MEMO panel exposes 1–64 byte hexadecimal/decimal views, changed-byte markers,
up to eight byte-change watchpoints, safe paused edits, and a direct Hardware manual link. This is a
view onto the production bus, not a duplicated visualization image.

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
replay complete snapshots. Firefox E2E compiles/runs it in the Worker and rewinds a debug frame.

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
calculations; the three alpha PCM traces remain unchanged.

Audio-frame and allocation-counter exhaustion fault with `M019012` before the overflowing increment.
An `sfx` fault carries its source span; output-stage hardware faults have span `(0,0)`. Output faults
are latched inside the machine's frame boundary, retaining phase 5 and not advancing its completed
frame counter. Earlier successful commands may remain in the fault snapshot, just as with callback
faults; restoring a healthy checkpoint is the recovery path.

`tests/conformance/audio` is an ordinary source-visible cartridge. Native Release/Debug tests verify
all eight voices, descriptors, command/register aliases, alternating raw-muted/audible PCM and full
replay. The browser matrix imports, compiles and runs it through the Worker; standalone exports use
the same inlined Worker/core. Studio's paused memory panel exposes these registers and watchpoints.

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
tile flags. Pixel bytes must remain 0–31 and map words must reference an existing tile. A partial
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
frame rather than its already initialized live registers. These bytes count toward the same 128 KiB
capacity, exactly as in alpha. The unallocated tail is writable byte scratch space; descriptor
`used` counts declared payload, not nonzero bytes. Restart reloads source; snapshots retain the tail.

Descriptor bytes are fixed machine metadata, outside the 128 KiB payload budget. Every allocation
consumes at least one payload byte, bounding the table at 131,072 records (3 MiB, ending at `3c0000`).
Only actual descriptor records are mapped; unused slots remain reserved zero/read-only holes.
`tests/conformance/visual/` is an ordinary source-visible project exercising descriptor discovery,
unaligned cells, sprite/tile/flag writes and mixed high-level drawing/query calls in Release/Debug
core tests and the browser matrix. The bundled source-visible `mod01-service` and
`hardware-gauntlet` cartridges expose the author-facing diagnostic pages.

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
stable throughout that frame's update/draw/raster calls. This deliberately preserves alpha's `btnp`
timing: in a 30 Hz cartridge a press first sampled on an odd, skipped-update frame is visible to
that frame's drawing but is not latched for the next update. A held button is still visible through
`btn`. The focused scheduler test reproduces this exact sequence and its restore behavior; it is
not inferred from browser key timing. Hardware Revision 1 does not introduce update-latched edges.

`tests/conformance/input.modl` checks all twelve buttons on every port, high/low API equivalence,
held/press/release transitions, pointer state, reset and copying registers to RAM. Native tests run
36-frame scripted traces at both 30/60 Hz in Release/Debug with restore/forward checks. The browser
matrix also compiles and executes it; locally stored controller profiles cover four keyboard ports,
physical-gamepad assignment, conflict handling and disconnect/reconnect behavior.

## Synthetic work model

Work units are deterministic accounting units, not real CPU instructions and not evidence for a
fictional clock speed. The compiler currently charges 8 units at each function/callback entry, 4 at
loop back-edges, 4 per task-state transition, 4 per allocation or fixed-capacity write, one per
materialized range element, and 2 before each console API call. Charging a range before allocation
lets the frame budget reject an extreme dynamic range without first constructing it.
V1 enum payload equality additionally charges one unit per payload slot or traversed nested field;
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
reports the responsible source span. The highest measured bundled path is Raster Rush with four
simultaneous views at about 31,682 units, leaving headroom for input-dependent variation while still
making the limit visible during ordinary development.

The protocol and shared core reject a configuration above the fixed ceiling before constructing a
cartridge; internal diagnostic tests may use a stricter limit. Studio takes its normal limit from the
same `HARDWARE.workUnitsPerFrame` constant. A faulted work counter saturates at the largest safe MODL
integer (`2^53-1`) if an enormous charge would overflow it; the charge still faults at its source
span before a bulk operation proceeds. Attribution retains the same saturated total. Ordinary
in-budget accounting and first-party frame costs are unchanged.
