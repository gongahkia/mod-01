# MOD-01 cartridge format

MOD-01 has one editable source format, [M01W/2](M01W.md), and one packed
distribution encoding, M01C/2. The Rust core owns parsing, linking, compilation,
packing and validation. Other format/hardware revisions are rejected; there is
no migration command or alternate machine profile.

## Editable source and metadata

A project is a UTF-8 `.m01w` document. A directory argument means
`DIRECTORY/cart.m01w`, not a split project tree. The document embeds modules,
JSON assets, auxiliary UTF-8 files, recipes and tests. Logical paths identify
embedded files; they are not host filesystem access.

Cartridge metadata selects `MODL/1`, hardware revision 2, an entry module and
30 or 60 Hz updates. Asset kinds are sprite, animation, tile_set, map, font,
sound and music. Optional label, thumbnail and display paths refer to embedded
files. Unknown fields, duplicate paths/names, invalid identities and unsafe
relative paths are errors. Paths are ASCII, relative and slash-separated;
absolute paths, backslashes, empty segments and dot segments are rejected.

Absolute dotted imports such as `import src.math as math` resolve the embedded
`src/math.modl`. Cycles, inaccessible members and callbacks outside the entry
module are compiler errors. See [MODL.md](MODL.md).

## Canonical packed bytes

The header is eight bytes `MOD-01 1A 02`, followed by a little-endian u32
entry count. Entries are strictly sorted by canonical archive path:

| Field          | Encoding                               |
| -------------- | -------------------------------------- |
| Path length    | u16 little-endian                      |
| Raw length     | u32 little-endian                      |
| Encoded length | u32 little-endian                      |
| Integrity      | 32 SHA-256 bytes over the decoded data |
| Path           | Canonical ASCII bytes                  |
| Payload        | Canonical PackBits-style RLE           |

Packets encode 1–128 literal or repeated bytes; the high tag bit denotes a
repeat. Runs of at least four bytes use repeated packets. Decoding re-encodes
each payload and rejects alternate encodings. There are no timestamps, host
paths, permissions or platform fields.

Only these physical entries are allowed:

- `manifest.json`: canonical JSON metadata and sorted size/hash inventory.
- `source/workcart.m01w`: the exact authored source, including all embedded data.
- `build/cartridge.js` and `build/cartridge.js.map`, unless
  `compile_on_load = true` omits these redundant build products.

Manifest entry/asset/presentation paths are logical paths inside the workcart.
Source and assets are stored once, not copied into redundant archive entries.
The manifest always records the compiler version and generated program length
and SHA-256. Loading recompiles the original MODL, checks that identity, and,
when build products are present, compares them with the recompiled JS/map.
Changing an archived program and recomputing its inventory does not authorize
execution of that program.

Packed bytes are limited to 1 MiB. Decoding bounds total expansion to 16 MiB
and entry count to 4,096, validates lengths, canonical paths/JSON/RLE, hashes,
metadata/source agreement and the exact entry set, and rejects trailing bytes.
Packing decodes the completed artifact before returning it.

Unpacking restores the exact workcart. Studio's no-op save/pack preserves its
authored text; actual edits preserve all unrelated modules, auxiliary files,
recipes and tests. `mod01 info FILE --source` exposes a read-only materialized
JSON view for inspection, not an additional authoring format.

## Cartridge PNG

`.m01c.png` is a deterministic 320×240 RGBA image of MOD-01's cartridge shell.
Exactly one private `moCa` chunk contains the byte-exact canonical cartridge
and one `moCm` chunk contains bounded canonical presentation metadata.
PNG CRCs and the cartridge inventory are both validated.

The image is presentation only; importing reconstructs source from the packed
payload, never from the pixels. Studio and CLI accept raw and PNG cartridges.
Use `mod01 export png PROJECT --output game.m01c.png`.

## Native distribution

`mod01 export native` bundles the same cartridge with a native player, Node
runtime and notices. It does not change cartridge semantics or embed a browser.
See [NATIVE_GAMES.md](NATIVE_GAMES.md) for target binaries, directory/`.app`
layouts, reproducibility boundaries and OS signing requirements.

## Headless and replay interchange

`mod01 run PROJECT_OR_CARTRIDGE --headless` uses the same compiler/core.
Flags include `--seed`, `--frames`, `--input`, `--save` and `--output`.
The current input JSON schema is revision 1: ordered, non-overlapping records
with frame, optional duration, four-port button assignments and optional
complete pointer state. An optional `menuAction` integer 1–5 records a cartridge
pause-action activation on that frame, before update/draw. Null/invalid actions
are rejected; absence means no activation. Navigation and pause duration are
host state, not extra machine frames. The current `.m01rec` envelope also uses revision 1;
other replay schemas are rejected.

Trace results include framebuffer, state, audio-command, Float32 little-endian
PCM and committed-save hashes, work usage and stable fault/span details.
Limits are 36,000 frames, 8 MiB trace input, 16 MiB decoded files, 1 MiB ROM
and 8 KiB save data. A modeled cartridge fault is reported in the trace;
malformed host input exits unsuccessfully.

## Reproducibility and size classes

Identical workcart bytes, metadata and compiler version produce identical
packed bytes. PNG export is deterministic too. Editable source whitespace is
part of identity; the formatter explicitly changes it when requested.

Complete-artifact classes are 4K, 16K, 64K, 256K and 1M. The exact packed byte
length counts every header, hash, source/asset payload and build product.
These are badges within one machine, not separate compatibility profiles.
