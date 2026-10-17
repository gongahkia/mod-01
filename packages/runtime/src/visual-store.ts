import { HARDWARE } from './hardware';
import { MEMORY, type ByteMemoryRegion } from './bus';
import type {
  DisplayConfiguration,
  IndexedMap,
  IndexedMapLayer,
  IndexedSprite,
  VisualAsset,
} from './graphics';

export type StoredMapLayer = Omit<IndexedMapLayer, 'cells'> & { readonly cells: DataView };
export type StoredVisualAsset =
  | Exclude<VisualAsset, IndexedMap>
  | {
      readonly kind: 'map';
      readonly name: string;
      readonly layers: readonly StoredMapLayer[];
    };

interface Allocation {
  readonly offset: number;
  readonly bytes: Uint8Array;
  readonly kind: number;
  readonly width: number;
  readonly height: number;
  readonly reference: number;
  readonly validate?: (offset: number, bytes: Uint8Array) => boolean;
}

const LEGACY_VISUAL_BYTES = 128 * 1024;

/** A single packed, little-endian visual image; all drawing views alias these bytes. */
export class VisualAssetStore {
  private readonly entries = new Map<string, StoredVisualAsset>();
  private readonly ids = new Map<string, number>();
  private readonly bytes = new Uint8Array(HARDWARE.visualCapacityBytes);
  private readonly allocations: Allocation[] = [];
  private readonly descriptors: Uint8Array;
  private readonly allocationTable: Uint8Array;
  private readonly info = new Uint8Array(24);
  private readonly addressBase: number;
  public readonly display: DisplayConfiguration | undefined;
  public readonly usedBytes: number;

  public constructor(assets: readonly VisualAsset[] = [], display?: DisplayConfiguration) {
    if (assets.length > 4096) throw new RangeError('too many visual assets');
    const sorted = [...assets].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    const source = new Map<string, VisualAsset>();
    let usedBytes = 0;
    for (const [id, asset] of sorted.entries()) {
      if (source.has(asset.name)) throw new TypeError(`duplicate visual asset '${asset.name}'`);
      validateAsset(asset);
      usedBytes += visualAssetBytes(asset);
      source.set(asset.name, asset);
      this.ids.set(asset.name, id);
    }
    if (display !== undefined) usedBytes += 32 + display.raster.length * 38;
    if (usedBytes > HARDWARE.visualCapacityBytes)
      throw new RangeError('visual assets exceed the 1 MiB shared capacity');
    this.usedBytes = usedBytes;
    this.addressBase = usedBytes > LEGACY_VISUAL_BYTES ? MEMORY.visualExtended : MEMORY.visual;
    this.descriptors = new Uint8Array(sorted.length * MEMORY.assetStride);
    const descriptorView = new DataView(this.descriptors.buffer);
    for (const [id, asset] of sorted.entries()) {
      const first = this.allocations.length;
      const stored = this.store(asset, source);
      this.entries.set(asset.name, stored);
      const offset = id * MEMORY.assetStride;
      descriptorView.setUint32(
        offset,
        ['sprite', 'animation', 'tile_set', 'map', 'font'].indexOf(asset.kind) + 1,
        true,
      );
      descriptorView.setUint32(offset + 4, this.allocations.length - first, true);
      descriptorView.setUint32(
        offset + 8,
        MEMORY.allocations + first * MEMORY.allocationStride,
        true,
      );
      descriptorView.setUint32(offset + 12, visualAssetBytes(asset), true);
    }
    this.display = display === undefined ? undefined : this.storeDisplay(display);
    this.allocationTable = new Uint8Array(this.allocations.length * MEMORY.allocationStride);
    const allocationView = new DataView(this.allocationTable.buffer);
    for (const [index, entry] of this.allocations.entries()) {
      const offset = index * MEMORY.allocationStride;
      [
        entry.kind,
        this.addressBase + entry.offset,
        entry.bytes.length,
        entry.width,
        entry.height,
        entry.reference,
      ].forEach((value, field) => {
        allocationView.setUint32(offset + field * 4, value, true);
      });
    }
    const info = new DataView(this.info.buffer);
    [
      sorted.length,
      usedBytes,
      MEMORY.assets,
      this.allocations.length,
      MEMORY.allocations,
      display === undefined ? 0 : this.addressBase + usedBytes - 32 - display.raster.length * 38,
    ].forEach((value, field) => {
      info.setUint32(field * 4, value, true);
    });
  }

  public get(name: string): StoredVisualAsset | undefined {
    return this.entries.get(name);
  }
  public id(name: string): number {
    return this.ids.get(name) ?? -1;
  }

  public mapCell(name: string, layer: number, x: number, y: number): number | undefined {
    const asset = this.entries.get(name);
    if (asset?.kind !== 'map') return undefined;
    const selected = asset.layers[layer];
    if (selected === undefined || x < 0 || y < 0 || x >= selected.width || y >= selected.height)
      return undefined;
    return selected.cells.getUint16((y * selected.width + x) * 2, true);
  }

  public mapFlag(name: string, layer: number, x: number, y: number, flag: number): boolean {
    const asset = this.entries.get(name);
    if (asset?.kind !== 'map' || flag < 0 || flag > 7) return false;
    const selected = asset.layers[layer];
    if (selected === undefined) return false;
    const tile = this.mapCell(name, layer, x, y);
    const tileSet = this.entries.get(selected.tileSet);
    return (
      tile !== undefined &&
      tileSet?.kind === 'tile_set' &&
      ((tileSet.flags[tile] ?? 0) & (1 << flag)) !== 0
    );
  }

  public memoryRegions(): readonly ByteMemoryRegion[] {
    return [
      {
        name: 'visual store',
        address: MEMORY.visual,
        bytes: this.bytes.subarray(0, LEGACY_VISUAL_BYTES),
        writable: true,
        validate: (offset: number, bytes: Uint8Array) => this.validate(offset, bytes),
      },
      ...(this.addressBase === MEMORY.visualExtended
        ? [
            {
              name: 'visual store extended',
              address: MEMORY.visualExtended,
              bytes: this.bytes,
              writable: true,
              validate: (offset: number, bytes: Uint8Array) => this.validate(offset, bytes),
            },
          ]
        : []),
      {
        name: 'visual allocation status',
        address: MEMORY.visualInfo,
        bytes: this.info,
        writable: false,
      },
      ...(this.descriptors.length === 0
        ? []
        : [
            {
              name: 'visual asset descriptors',
              address: MEMORY.assets,
              bytes: this.descriptors,
              writable: false,
            },
            {
              name: 'visual allocations',
              address: MEMORY.allocations,
              bytes: this.allocationTable,
              writable: false,
            },
          ]),
      ...(this.descriptors.length === 0 && this.allocationTable.length > 0
        ? [
            {
              name: 'visual allocations',
              address: MEMORY.allocations,
              bytes: this.allocationTable,
              writable: false,
            },
          ]
        : []),
    ];
  }

  private allocate(
    kind: number,
    width: number,
    height: number,
    data: Uint8Array,
    reference = 0,
    validate?: Allocation['validate'],
  ): Uint8Array {
    const previous = this.allocations.at(-1);
    const offset = previous === undefined ? 0 : previous.offset + previous.bytes.length;
    const bytes = this.bytes.subarray(offset, offset + data.length);
    bytes.set(data);
    this.allocations.push({
      offset,
      bytes,
      kind,
      width,
      height,
      reference,
      ...(validate === undefined ? {} : { validate }),
    });
    return bytes;
  }

  private sprite(sprite: IndexedSprite): IndexedSprite {
    return {
      ...sprite,
      pixels: this.allocate(1, sprite.width, sprite.height, sprite.pixels, 0, (_offset, bytes) =>
        bytes.every((color) => color < HARDWARE.paletteSize),
      ),
    };
  }

  private store(asset: VisualAsset, source: ReadonlyMap<string, VisualAsset>): StoredVisualAsset {
    switch (asset.kind) {
      case 'sprite':
        return this.sprite(asset);
      case 'animation':
        return { ...asset, frames: asset.frames.map((frame) => this.sprite(frame)) };
      case 'tile_set':
        return {
          ...asset,
          tiles: asset.tiles.map((tile) => this.sprite(tile)),
          flags: this.allocate(3, asset.flags.length, 1, asset.flags),
        };
      case 'map':
        return {
          ...asset,
          layers: asset.layers.map((layer) => {
            const tileSet = source.get(layer.tileSet);
            if (
              tileSet?.kind !== 'tile_set' ||
              layer.cells.some((tile) => tile >= tileSet.tiles.length)
            )
              throw new TypeError(`map '${asset.name}' references an invalid tile set or tile`);
            const encoded = new Uint8Array(layer.cells.length * 2);
            const view = new DataView(encoded.buffer);
            for (let index = 0; index < layer.cells.length; index += 1)
              view.setUint16(index * 2, layer.cells[index] ?? 0, true);
            const bytes = this.allocate(
              2,
              layer.width,
              layer.height,
              encoded,
              (this.ids.get(layer.tileSet) ?? -1) + 1,
              (offset, part) => {
                const byte = (index: number): number =>
                  (index >= offset && index < offset + part.length
                    ? part[index - offset]
                    : bytes[index]) ?? 0;
                for (let index = offset - (offset % 2); index < offset + part.length; index += 2)
                  if (byte(index) + byte(index + 1) * 256 >= tileSet.tiles.length) return false;
                return true;
              },
            );
            return {
              ...layer,
              cells: new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength),
            };
          }),
        };
      case 'font': {
        const glyphs = [...asset.glyphs.entries()].sort(([left], [right]) => left - right);
        const glyphBytes = asset.glyphWidth * asset.glyphHeight;
        const encoded = new Uint8Array(8 + glyphs.length * (2 + glyphBytes));
        encoded.set(
          [
            asset.glyphWidth,
            asset.glyphHeight,
            asset.baseline,
            asset.advanceX,
            asset.advanceY,
            asset.missingGlyph,
          ],
          0,
        );
        new DataView(encoded.buffer).setUint16(6, glyphs.length, true);
        const bitmapOffsets = new Set<number>();
        for (const [index, [code, pixels]] of glyphs.entries()) {
          const offset = 8 + index * (2 + glyphBytes);
          new DataView(encoded.buffer).setUint16(offset, code, true);
          encoded.set(pixels, offset + 2);
          for (let byte = offset + 2; byte < offset + 2 + glyphBytes; byte += 1)
            bitmapOffsets.add(byte);
        }
        const bytes = this.allocate(
          6,
          asset.glyphWidth,
          asset.glyphHeight,
          encoded,
          asset.missingGlyph,
          (offset, part) =>
            part.every((value, index) => {
              const absolute = offset + index;
              return bitmapOffsets.has(absolute) ? value <= 1 : value === encoded[absolute];
            }),
        );
        return {
          ...asset,
          glyphs: new Map(
            glyphs.map(([code], index) => {
              const offset = 8 + index * (2 + glyphBytes) + 2;
              return [code, bytes.subarray(offset, offset + glyphBytes)];
            }),
          ),
        };
      }
    }
  }

  private storeDisplay(display: DisplayConfiguration): DisplayConfiguration {
    if (display.remap.length !== HARDWARE.rasterPaletteSize || display.raster.length > 144)
      throw new TypeError('invalid display defaults');
    const remap = this.allocate(4, 32, 1, display.remap, 0, (_offset, bytes) =>
      bytes.every((value) => value < HARDWARE.paletteSize),
    );
    let previous = -1;
    const raster = display.raster.map((row) => {
      if (
        !Number.isInteger(row.line) ||
        row.line <= previous ||
        row.line >= 144 ||
        ![row.scrollX, row.scrollY].every(
          (value) => Number.isInteger(value) && value >= -32768 && value <= 32767,
        ) ||
        row.remap.length !== HARDWARE.rasterPaletteSize
      )
        throw new TypeError('invalid display raster defaults');
      previous = row.line;
      const encoded = new Uint8Array(38);
      const view = new DataView(encoded.buffer);
      view.setUint16(0, row.line, true);
      view.setInt16(2, row.scrollX, true);
      view.setInt16(4, row.scrollY, true);
      encoded.set(row.remap, 6);
      const bytes = this.allocate(5, 38, 1, encoded, 0, (offset, part) =>
        part.every((value, index) =>
          offset + index < 2
            ? value === encoded[offset + index]
            : offset + index < 6 || value < HARDWARE.paletteSize,
        ),
      );
      const stored = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      return {
        line: row.line,
        get scrollX() {
          return stored.getInt16(2, true);
        },
        get scrollY() {
          return stored.getInt16(4, true);
        },
        remap: bytes.subarray(6),
      };
    });
    return { remap, raster };
  }

  private validate(offset: number, bytes: Uint8Array): boolean {
    let low = 0;
    let high = this.allocations.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      const entry = this.allocations[mid];
      if (entry !== undefined && entry.offset + entry.bytes.length <= offset) low = mid + 1;
      else high = mid;
    }
    for (let index = low; index < this.allocations.length; index += 1) {
      const entry = this.allocations[index];
      if (entry === undefined || entry.offset >= offset + bytes.length) break;
      const start = Math.max(offset, entry.offset);
      const end = Math.min(offset + bytes.length, entry.offset + entry.bytes.length);
      if (
        entry.validate?.(start - entry.offset, bytes.subarray(start - offset, end - offset)) ===
        false
      )
        return false;
    }
    return true;
  }
}

function validateAsset(asset: VisualAsset): void {
  if (asset.name.length === 0) throw new TypeError('visual asset names cannot be empty');
  switch (asset.kind) {
    case 'sprite':
      validateSprite(asset);
      return;
    case 'animation':
      if (asset.frames.length === 0)
        throw new RangeError(`animation '${asset.name}' has no frames`);
      asset.frames.forEach(validateSprite);
      return;
    case 'tile_set':
      if (asset.tiles.length === 0 || asset.flags.length !== asset.tiles.length)
        throw new RangeError(`tile set '${asset.name}' has incoherent tiles or flags`);
      for (const tile of asset.tiles) {
        validateSprite(tile);
        if (tile.width !== 8 || tile.height !== 8)
          throw new RangeError(`tile set '${asset.name}' contains a non-8x8 tile`);
      }
      return;
    case 'map':
      if (asset.layers.length === 0) throw new RangeError(`map '${asset.name}' has no layers`);
      for (const layer of asset.layers)
        if (
          !Number.isSafeInteger(layer.width) ||
          !Number.isSafeInteger(layer.height) ||
          layer.width <= 0 ||
          layer.height <= 0 ||
          layer.cells.length !== layer.width * layer.height ||
          layer.tileSet.length === 0
        )
          throw new RangeError(`map '${asset.name}' has an invalid layer`);
      return;
    case 'font':
      if (
        !Number.isSafeInteger(asset.glyphWidth) ||
        !Number.isSafeInteger(asset.glyphHeight) ||
        asset.glyphWidth < 1 ||
        asset.glyphWidth > 16 ||
        asset.glyphHeight < 1 ||
        asset.glyphHeight > 16 ||
        !Number.isSafeInteger(asset.baseline) ||
        asset.baseline < 0 ||
        asset.baseline >= asset.glyphHeight ||
        !Number.isSafeInteger(asset.advanceX) ||
        asset.advanceX < 1 ||
        asset.advanceX > 32 ||
        !Number.isSafeInteger(asset.advanceY) ||
        asset.advanceY < 1 ||
        asset.advanceY > 32 ||
        !Number.isSafeInteger(asset.missingGlyph) ||
        asset.missingGlyph < 0 ||
        asset.missingGlyph > 255 ||
        asset.glyphs.size === 0 ||
        asset.glyphs.size > 256 ||
        !asset.glyphs.has(asset.missingGlyph)
      )
        throw new RangeError(`font '${asset.name}' has invalid metrics or glyph map`);
      for (const [code, pixels] of asset.glyphs) {
        if (
          !Number.isSafeInteger(code) ||
          code < 0 ||
          code > 255 ||
          pixels.length !== asset.glyphWidth * asset.glyphHeight ||
          pixels.some((value) => value > 1)
        )
          throw new RangeError(`font '${asset.name}' has an invalid glyph`);
      }
      return;
  }
}

function validateSprite(sprite: IndexedSprite): void {
  if (
    !Number.isSafeInteger(sprite.width) ||
    !Number.isSafeInteger(sprite.height) ||
    sprite.width < 1 ||
    sprite.width > HARDWARE.spriteMaximumAxis ||
    sprite.height < 1 ||
    sprite.height > HARDWARE.spriteMaximumAxis ||
    sprite.pixels.length !== sprite.width * sprite.height ||
    sprite.pixels.some((color) => color >= HARDWARE.paletteSize)
  )
    throw new RangeError(`sprite '${sprite.name}' is outside MOD-01 limits`);
}

export function visualAssetBytes(asset: VisualAsset): number {
  switch (asset.kind) {
    case 'sprite':
      return asset.pixels.byteLength;
    case 'animation':
      return asset.frames.reduce((total, frame) => total + frame.pixels.byteLength, 0);
    case 'tile_set':
      return (
        asset.flags.byteLength +
        asset.tiles.reduce((total, tile) => total + tile.pixels.byteLength, 0)
      );
    case 'map':
      return asset.layers.reduce((total, layer) => total + layer.cells.byteLength, 0);
    case 'font':
      return 8 + asset.glyphs.size * (2 + asset.glyphWidth * asset.glyphHeight);
  }
}
