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

Inside a logical cartridge, only these physical entries are allowed:

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

Each logical cartridge is limited to 1 MiB packed. Decoding bounds total expansion to 16 MiB
and entry count to 4,096, validates lengths, canonical paths/JSON/RLE, hashes,
metadata/source agreement and the exact entry set, and rejects trailing bytes.
Packing decodes the completed artifact before returning it.

Unpacking restores the exact workcart. Studio's no-op save/pack preserves its
authored text; actual edits preserve all unrelated modules, auxiliary files,
recipes and tests. `mod01 info FILE --source` exposes a read-only materialized
JSON view for inspection, not an additional authoring format.

## Flat game composition

The shared Rust whole-game APIs, `game::pack_game_workcart` and
`game::decode_game`, also compose up to 32 current logical cartridges. They use
the same M01C/2 header, inventory encoding, canonical RLE and hashes, not a
historical format or alternate hardware profile. Ordinary games retain exactly
the existing single-cartridge bytes.

A composition contains only `bundle.json`, the exact enclosing
`source/workcart.m01w`, and `cartridges/ALIAS.m01c` for every declared member.
The canonical index records format revision 2, kind `bundle`, entry `main`,
catalog identity and every source/member size/hash. Each member independently
passes the ordinary cartridge compiler, asset and capacity admission. Unknown,
unlisted, missing, nested or rebound members are rejected. Admission recompiles
the complete enclosing source and requires byte-for-byte package agreement;
recomputing ordinary hashes cannot authorize forged compiled JavaScript.

The shipping container is not mapped into console ROM. Only an active member's
ordinary ≤1 MiB cartridge may be mapped there. Container limits account separately
for the ≤128 MiB escaped source document, ≤32 MiB member bytes, a ≤64 KiB index
and canonical RLE/header overhead. A catalog has at most 64 MiB total member
source and 16 MiB per member. This does not enlarge any member's machine capacity.
Game identity includes the entire shipping artifact and exact enclosing source;
catalog identity covers sorted aliases and exact logical cartridge bytes.

These are implemented core APIs; CLI/Studio/runtime bundle integration remains
in progress. Single-cartridge APIs explicitly refuse bundled source instead of
silently dropping dependencies. See [bundle implementation](BUNDLES.md).

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
Flags include `--seed`, `--frames`, `--input`, `--save`, `--saves`,
`--parameter` and `--output`. `--save` supplies the entry cartridge's raw image;
`--saves` instead reads a JSON object of persistent cartridge IDs to canonical
base64 byte images. They are mutually exclusive. Missing declared IDs start with
zeroed 8 KiB images; undeclared IDs, malformed base64 or over-capacity images fail
before running code. The save-catalog file is bounded to 512 KiB. The optional
initial transfer parameter defaults to empty Text and is bounded to 1,024 UTF-8
bytes. Headless execution never writes user save files.
The current input JSON schema is revision 1: ordered, non-overlapping records
with frame, optional duration, four-port button assignments and optional
complete pointer state. An optional `menuAction` integer 1–5 records a cartridge
pause-action activation on that frame, before update/draw. Null/invalid actions
are rejected; absence means no activation. Navigation and pause duration are
host state, not extra machine frames. The current `.m01rec` envelope also uses revision 1;
other replay schemas are rejected.

The current headless request/result schema is revision 2; the input replay schema
above remains revision 1. There is no older-request fallback. Every source/raw/PNG
run uses shared whole-game Rust admission and the production game scheduler,
including one-member games. `game.identity` covers the complete shipping artifact;
`game.members` records each alias's own ID, logical ROM size and SHA-256.
`cartridge` describes the initial logical member, not an oversized bundle ROM.

Results identify each frame's producing alias, persistent ID, local frame and
global game frame, plus work usage, framebuffer/audio-command/Float32
little-endian PCM hashes and fault/span details. Following-state checkpoints may
already contain the next prepared cartridge, while the output still belongs to
the previous one. `stateSha256` covers the whole running game, input barrier and
all saves, excluding only artifact identity and compiler mode: this allows authored
tests to assert state without a self-referential hash of their own source.
`snapshotSha256` includes that identity/mode binding too. Summary fields use the
`finalStateSha256`/`finalSnapshotSha256` names. Whole-catalog save hashes and per-ID
`initialSaves`/`finalSaves` hashes make save ownership observable.

Canonical trace hashing sorts object keys by ASCII/code-unit order. Byte arrays
are represented as `{ "$bytes": "lowercase hex" }`; Float32 arrays use
`{ "$float32le": "lowercase little-endian hex" }`, avoiding host-specific JSON
encoding and per-frame million-element temporary number arrays. Per-ID save hashes
remain SHA-256 of the exact padded 8 KiB committed bytes.

Limits are 36,000 global frames, 8 MiB replay input, up to 32 declared members,
16 MiB decoded data/1 MiB logical ROM/8 KiB saves per member, and the shared
bounded game transport. A modeled frame fault is reported without applying its
pending transition or persisting that frame's save catalog; malformed host input
exits unsuccessfully.

## Reproducibility and size classes

Identical workcart bytes, metadata and compiler version produce identical
packed bytes. PNG export is deterministic too. Editable source whitespace is
part of identity; the formatter explicitly changes it when requested.

Complete-artifact classes are 4K, 16K, 64K, 256K and 1M. The exact packed byte
length counts every header, hash, source/asset payload and build product.
These are badges within one machine, not separate compatibility profiles.
