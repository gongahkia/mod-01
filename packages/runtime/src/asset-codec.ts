import { AudioAssetStore, type AudioAsset, type MusicAsset, type SoundAsset } from './audio';
import {
  VisualAssetStore,
  type DisplayConfiguration,
  type IndexedAnimation,
  type IndexedMap,
  type IndexedFont,
  type IndexedSprite,
  type IndexedTileSet,
  type VisualAsset,
} from './graphics';
import { HARDWARE } from './hardware';
import type { MapQueryAsset } from './map-query';

export type ProjectAssetKind =
  'sprite' | 'animation' | 'tile_set' | 'map' | 'font' | 'sound' | 'music';

export interface ProjectAssetDeclaration {
  readonly kind: ProjectAssetKind;
  readonly path: string;
}

export interface RuntimeAssetSource {
  readonly declarations: Readonly<Record<string, ProjectAssetDeclaration>>;
  readonly files: Readonly<Record<string, Uint8Array>>;
  readonly displayPath?: string | null;
}

/** Bounded, data-only source bank sent to the console core for authoritative decoding. */
export function isRuntimeAssetSource(value: unknown): value is RuntimeAssetSource {
  if (
    !isRecord(value) ||
    !isRecord(value.declarations) ||
    !isRecord(value.files) ||
    Array.isArray(value.declarations) ||
    Array.isArray(value.files) ||
    Object.keys(value).some((key) => !['declarations', 'files', 'displayPath'].includes(key)) ||
    Object.keys(value.declarations).length > 4096 ||
    Object.keys(value.files).length > 4096 ||
    (value.displayPath !== undefined &&
      value.displayPath !== null &&
      !canonicalAssetPath(value.displayPath))
  )
    return false;
  let bytes = typeof value.displayPath === 'string' ? value.displayPath.length : 0;
  for (const [path, data] of Object.entries(value.files)) {
    if (!canonicalAssetPath(path) || !(data instanceof Uint8Array)) return false;
    bytes += path.length + data.byteLength;
    if (bytes > 16 * 1024 * 1024) return false;
  }
  for (const [name, declaration] of Object.entries(value.declarations)) {
    if (
      !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name) ||
      !isRecord(declaration) ||
      Object.keys(declaration).length !== 2 ||
      !canonicalAssetPath(declaration.path) ||
      typeof declaration.kind !== 'string' ||
      !['sprite', 'animation', 'tile_set', 'map', 'font', 'sound', 'music'].includes(
        declaration.kind,
      )
    )
      return false;
    bytes += name.length + declaration.path.length + declaration.kind.length;
    if (bytes > 16 * 1024 * 1024) return false;
  }
  return true;
}

function canonicalAssetPath(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    value.length <= 1024 &&
    value
      .split('/')
      .every((part) => /^[A-Za-z0-9_.-]+$/.test(part) && part !== '.' && part !== '..')
  );
}

export interface RuntimeAssetBundle {
  readonly visual: readonly VisualAsset[];
  readonly audio: readonly AudioAsset[];
  readonly maps: readonly MapQueryAsset[];
  readonly display?: DisplayConfiguration;
  readonly visualBytes: number;
}

export interface DisplayAssetFile {
  readonly revision: 2;
  readonly kind: 'display';
  readonly remap: readonly number[];
  readonly raster: readonly {
    readonly line: number;
    readonly scrollX: number;
    readonly scrollY: number;
    readonly remap: readonly number[];
  }[];
}

export interface SpriteAssetFile {
  readonly revision: 2;
  readonly kind: 'sprite';
  readonly width: number;
  readonly height: number;
  readonly frames: readonly (readonly number[])[];
}

export interface TileSetAssetFile {
  readonly revision: 2;
  readonly kind: 'tile_set';
  readonly tiles: readonly (readonly number[])[];
  readonly flags: readonly number[];
}

export interface MapAssetFile {
  readonly revision: 1;
  readonly kind: 'map';
  readonly layers: readonly {
    readonly width: number;
    readonly height: number;
    readonly cells: readonly number[];
    readonly tileSet: string;
  }[];
}

export interface FontAssetFile {
  readonly revision: 1;
  readonly kind: 'font';
  readonly glyphWidth: number;
  readonly glyphHeight: number;
  readonly baseline: number;
  readonly advanceX: number;
  readonly advanceY: number;
  readonly missingGlyph: number;
  readonly glyphs: readonly {
    readonly code: number;
    readonly pixels: readonly number[];
  }[];
}

/** Decodes documented JSON asset files into validated hardware stores and query views. */
export function decodeRuntimeAssets(
  declarations: Readonly<Record<string, ProjectAssetDeclaration>>,
  files: Readonly<Record<string, Uint8Array>>,
  displayPath?: string | null,
): RuntimeAssetBundle {
  const visual: VisualAsset[] = [];
  const audio: AudioAsset[] = [];
  for (const [name, declaration] of Object.entries(declarations).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    const bytes = files[declaration.path];
    if (bytes === undefined) {
      throw new TypeError(`asset '${name}' is missing '${declaration.path}'`);
    }
    const value: unknown = JSON.parse(new TextDecoder().decode(bytes));
    switch (declaration.kind) {
      case 'sprite':
        visual.push(decodeSprite(name, value, false));
        break;
      case 'animation':
        visual.push(decodeSprite(name, value, true));
        break;
      case 'tile_set':
        visual.push(decodeTileSet(name, value));
        break;
      case 'map':
        visual.push(decodeMap(name, value));
        break;
      case 'sound':
        audio.push(decodeSound(name, value));
        break;
      case 'music':
        audio.push(decodeMusic(name, value));
        break;
      case 'font':
        visual.push(decodeFont(name, value));
        break;
    }
  }
  const visualStore = new VisualAssetStore(visual);
  new AudioAssetStore(audio);
  const tileSets = new Map(
    visual
      .filter((asset): asset is IndexedTileSet => asset.kind === 'tile_set')
      .map((asset) => [asset.name, asset]),
  );
  const maps = visual
    .filter((asset): asset is IndexedMap => asset.kind === 'map')
    .map((asset) => ({
      name: asset.name,
      layers: asset.layers.map((layer) => {
        const tileSet = tileSets.get(layer.tileSet);
        if (tileSet === undefined) {
          throw new TypeError(`map '${asset.name}' references missing tile set '${layer.tileSet}'`);
        }
        return {
          width: layer.width,
          height: layer.height,
          cells: layer.cells.slice(),
          tileFlags: tileSet.flags.slice(),
        };
      }),
    }));
  const display = decodeDisplay(displayPath, files);
  const visualBytes = visualStore.usedBytes + displayBytes(display);
  if (visualBytes > HARDWARE.visualCapacityBytes) {
    throw new RangeError('visual assets exceed the 1 MiB shared capacity');
  }
  return {
    visual,
    audio,
    maps,
    ...(display === undefined ? {} : { display }),
    visualBytes,
  };
}

export function encodeAssetFile(value: unknown): Uint8Array {
  return new TextEncoder().encode(`${JSON.stringify(value)}\n`);
}

export function encodeSoundAssetFile(asset: SoundAsset): Uint8Array {
  return encodeAssetFile({ ...asset, name: undefined, revision: 1 });
}

export function encodeMusicAssetFile(asset: MusicAsset): Uint8Array {
  return encodeAssetFile({ ...asset, name: undefined, revision: 1 });
}

function decodeSprite(
  name: string,
  value: unknown,
  animation: boolean,
): IndexedSprite | IndexedAnimation {
  if (
    !isRecord(value) ||
    value.revision !== 2 ||
    value.kind !== 'sprite' ||
    !boundedInteger(value.width, 1, HARDWARE.spriteMaximumAxis) ||
    !boundedInteger(value.height, 1, HARDWARE.spriteMaximumAxis) ||
    !Array.isArray(value.frames) ||
    value.frames.length === 0 ||
    value.frames.length > 256
  ) {
    throw new TypeError(`sprite asset '${name}' is invalid`);
  }
  const pixelCount = value.width * value.height;
  const frames = value.frames.map((frame) => {
    if (!isNumberArray(frame, pixelCount, 0, 255)) {
      throw new TypeError(`sprite asset '${name}' has invalid indexed pixels`);
    }
    return {
      kind: 'sprite' as const,
      name,
      width: value.width as number,
      height: value.height as number,
      pixels: Uint8Array.from(frame),
    };
  });
  const first = frames[0];
  if (first === undefined) {
    throw new TypeError(`sprite asset '${name}' requires a frame`);
  }
  return animation ? { kind: 'animation', name, frames } : { ...first, name };
}

function decodeTileSet(name: string, value: unknown): IndexedTileSet {
  if (
    !isRecord(value) ||
    value.revision !== 2 ||
    value.kind !== 'tile_set' ||
    !Array.isArray(value.tiles) ||
    value.tiles.length === 0 ||
    value.tiles.length > 4096 ||
    !isNumberArray(value.flags, value.tiles.length, 0, 255)
  ) {
    throw new TypeError(`tile-set asset '${name}' is invalid`);
  }
  const tiles = value.tiles.map((pixels, index) => {
    if (!isNumberArray(pixels, 64, 0, 255)) {
      throw new TypeError(`tile ${String(index)} in '${name}' has invalid indexed pixels`);
    }
    return {
      kind: 'sprite' as const,
      name: `${name}:${String(index)}`,
      width: 8,
      height: 8,
      pixels: Uint8Array.from(pixels),
    };
  });
  return { kind: 'tile_set', name, tiles, flags: Uint8Array.from(value.flags) };
}

function decodeMap(name: string, value: unknown): IndexedMap {
  if (
    !isRecord(value) ||
    value.revision !== 1 ||
    value.kind !== 'map' ||
    !Array.isArray(value.layers) ||
    value.layers.length === 0 ||
    value.layers.length > 8
  ) {
    throw new TypeError(`map asset '${name}' is invalid`);
  }
  const layers = value.layers.map((layer) => {
    if (
      !isRecord(layer) ||
      !boundedInteger(layer.width, 1, 256) ||
      !boundedInteger(layer.height, 1, 256) ||
      typeof layer.tileSet !== 'string' ||
      !isNumberArray(layer.cells, layer.width * layer.height, 0, 65_535)
    ) {
      throw new TypeError(`map asset '${name}' has an invalid layer`);
    }
    return {
      width: layer.width,
      height: layer.height,
      cells: Uint16Array.from(layer.cells),
      tileSet: layer.tileSet,
    };
  });
  return { kind: 'map', name, layers };
}

function decodeFont(name: string, value: unknown): IndexedFont {
  if (
    !isRecord(value) ||
    value.revision !== 1 ||
    value.kind !== 'font' ||
    !boundedInteger(value.glyphWidth, 1, 16) ||
    !boundedInteger(value.glyphHeight, 1, 16) ||
    !boundedInteger(value.baseline, 0, value.glyphHeight - 1) ||
    !boundedInteger(value.advanceX, 1, 32) ||
    !boundedInteger(value.advanceY, 1, 32) ||
    !boundedInteger(value.missingGlyph, 0, 255) ||
    !Array.isArray(value.glyphs) ||
    value.glyphs.length === 0 ||
    value.glyphs.length > 256
  ) {
    throw new TypeError(`font asset '${name}' is invalid`);
  }
  const glyphs = new Map<number, Uint8Array>();
  let previous = -1;
  for (const glyph of value.glyphs) {
    if (
      !isRecord(glyph) ||
      !boundedInteger(glyph.code, 0, 255) ||
      glyph.code <= previous ||
      !isNumberArray(glyph.pixels, value.glyphWidth * value.glyphHeight, 0, 1)
    ) {
      throw new TypeError(`font asset '${name}' has an invalid glyph map`);
    }
    previous = glyph.code;
    glyphs.set(glyph.code, Uint8Array.from(glyph.pixels));
  }
  if (!glyphs.has(value.missingGlyph))
    throw new TypeError(`font asset '${name}' is missing its fallback glyph`);
  return {
    kind: 'font',
    name,
    glyphWidth: value.glyphWidth,
    glyphHeight: value.glyphHeight,
    baseline: value.baseline,
    advanceX: value.advanceX,
    advanceY: value.advanceY,
    missingGlyph: value.missingGlyph,
    glyphs,
  };
}

function decodeSound(name: string, value: unknown): SoundAsset {
  if (!isRecord(value) || value.revision !== 1 || value.kind !== 'sound') {
    throw new TypeError(`sound asset '${name}' is invalid`);
  }
  const sound = { ...value, name } as unknown as SoundAsset;
  new AudioAssetStore([sound]);
  return sound;
}

function decodeMusic(name: string, value: unknown): MusicAsset {
  if (!isRecord(value) || value.revision !== 1 || value.kind !== 'music') {
    throw new TypeError(`music asset '${name}' is invalid`);
  }
  return { ...value, name } as unknown as MusicAsset;
}

function decodeDisplay(
  path: string | null | undefined,
  files: Readonly<Record<string, Uint8Array>>,
): DisplayConfiguration | undefined {
  if (path === undefined || path === null) return undefined;
  const bytes = files[path];
  if (bytes === undefined) {
    throw new TypeError(`display configuration is missing '${path}'`);
  }
  const value: unknown = JSON.parse(new TextDecoder().decode(bytes));
  if (
    !isRecord(value) ||
    value.revision !== 2 ||
    value.kind !== 'display' ||
    !isNumberArray(value.remap, HARDWARE.rasterPaletteSize, 0, 255) ||
    !Array.isArray(value.raster) ||
    value.raster.length > HARDWARE.height
  ) {
    throw new TypeError('display configuration is invalid');
  }
  let previousLine = -1;
  const raster = value.raster.map((state) => {
    if (
      !isRecord(state) ||
      !boundedInteger(state.line, 0, HARDWARE.height - 1) ||
      state.line <= previousLine ||
      !boundedInteger(state.scrollX, -32_768, 32_767) ||
      !boundedInteger(state.scrollY, -32_768, 32_767) ||
      !isNumberArray(state.remap, HARDWARE.rasterPaletteSize, 0, 255)
    ) {
      throw new TypeError('display configuration has invalid raster state');
    }
    previousLine = state.line;
    return {
      line: state.line,
      scrollX: state.scrollX,
      scrollY: state.scrollY,
      remap: Uint8Array.from(state.remap),
    };
  });
  return { remap: Uint8Array.from(value.remap), raster };
}

function displayBytes(display: DisplayConfiguration | undefined): number {
  return display === undefined
    ? 0
    : HARDWARE.rasterPaletteSize + display.raster.length * (HARDWARE.rasterPaletteSize + 6);
}

function isNumberArray(
  value: unknown,
  length: number,
  minimum: number,
  maximum: number,
): value is number[] {
  return (
    Array.isArray(value) &&
    value.length === length &&
    value.every((entry) => Number.isSafeInteger(entry) && entry >= minimum && entry <= maximum)
  );
}

function boundedInteger(value: unknown, minimum: number, maximum: number): value is number {
  return (
    Number.isSafeInteger(value) && typeof value === 'number' && value >= minimum && value <= maximum
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
