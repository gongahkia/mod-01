import { createInterface } from 'node:readline';
import { deserialize, serialize } from 'node:v8';
//#region src/errors.ts
var RuntimeFault = class extends Error {
  code;
  sourceSpan;
  constructor(code, message, sourceSpan) {
    super(message);
    this.name = 'RuntimeFault';
    this.code = code;
    this.sourceSpan = sourceSpan;
  }
};
var BudgetExceeded = class extends RuntimeFault {
  used;
  limit;
  constructor(used, limit, sourceSpan) {
    super(
      'M019001',
      `frame used ${String(used)} synthetic work units; limit is ${String(limit)}`,
      sourceSpan,
    );
    this.name = 'BudgetExceeded';
    this.used = used;
    this.limit = limit;
  }
};
//#endregion
//#region src/palette.json
var palette_default = [
  [23, 20, 31],
  [41, 37, 50],
  [64, 57, 70],
  [93, 80, 84],
  [128, 106, 99],
  [170, 139, 116],
  [213, 185, 146],
  [244, 229, 189],
  [91, 41, 56],
  [139, 60, 71],
  [191, 85, 88],
  [237, 123, 105],
  [90, 57, 40],
  [137, 87, 46],
  [193, 132, 54],
  [231, 189, 80],
  [38, 60, 50],
  [52, 95, 70],
  [75, 139, 88],
  [127, 189, 104],
  [32, 59, 71],
  [46, 101, 113],
  [67, 150, 154],
  [117, 203, 192],
  [36, 52, 81],
  [52, 85, 129],
  [75, 125, 179],
  [115, 169, 209],
  [62, 49, 84],
  [101, 71, 119],
  [147, 99, 151],
  [195, 138, 174],
  [0, 0, 0],
  [0, 0, 51],
  [0, 0, 102],
  [0, 0, 153],
  [0, 0, 204],
  [0, 0, 255],
  [0, 51, 0],
  [0, 51, 51],
  [0, 51, 102],
  [0, 51, 153],
  [0, 51, 204],
  [0, 51, 255],
  [0, 102, 0],
  [0, 102, 51],
  [0, 102, 102],
  [0, 102, 153],
  [0, 102, 204],
  [0, 102, 255],
  [0, 153, 0],
  [0, 153, 51],
  [0, 153, 102],
  [0, 153, 153],
  [0, 153, 204],
  [0, 153, 255],
  [0, 204, 0],
  [0, 204, 51],
  [0, 204, 102],
  [0, 204, 153],
  [0, 204, 204],
  [0, 204, 255],
  [0, 255, 0],
  [0, 255, 51],
  [0, 255, 102],
  [0, 255, 153],
  [0, 255, 204],
  [0, 255, 255],
  [51, 0, 0],
  [51, 0, 51],
  [51, 0, 102],
  [51, 0, 153],
  [51, 0, 204],
  [51, 0, 255],
  [51, 51, 0],
  [51, 51, 51],
  [51, 51, 102],
  [51, 51, 153],
  [51, 51, 204],
  [51, 51, 255],
  [51, 102, 0],
  [51, 102, 51],
  [51, 102, 102],
  [51, 102, 153],
  [51, 102, 204],
  [51, 102, 255],
  [51, 153, 0],
  [51, 153, 51],
  [51, 153, 102],
  [51, 153, 153],
  [51, 153, 204],
  [51, 153, 255],
  [51, 204, 0],
  [51, 204, 51],
  [51, 204, 102],
  [51, 204, 153],
  [51, 204, 204],
  [51, 204, 255],
  [51, 255, 0],
  [51, 255, 51],
  [51, 255, 102],
  [51, 255, 153],
  [51, 255, 204],
  [51, 255, 255],
  [102, 0, 0],
  [102, 0, 51],
  [102, 0, 102],
  [102, 0, 153],
  [102, 0, 204],
  [102, 0, 255],
  [102, 51, 0],
  [102, 51, 51],
  [102, 51, 102],
  [102, 51, 153],
  [102, 51, 204],
  [102, 51, 255],
  [102, 102, 0],
  [102, 102, 51],
  [102, 102, 102],
  [102, 102, 153],
  [102, 102, 204],
  [102, 102, 255],
  [102, 153, 0],
  [102, 153, 51],
  [102, 153, 102],
  [102, 153, 153],
  [102, 153, 204],
  [102, 153, 255],
  [102, 204, 0],
  [102, 204, 51],
  [102, 204, 102],
  [102, 204, 153],
  [102, 204, 204],
  [102, 204, 255],
  [102, 255, 0],
  [102, 255, 51],
  [102, 255, 102],
  [102, 255, 153],
  [102, 255, 204],
  [102, 255, 255],
  [153, 0, 0],
  [153, 0, 51],
  [153, 0, 102],
  [153, 0, 153],
  [153, 0, 204],
  [153, 0, 255],
  [153, 51, 0],
  [153, 51, 51],
  [153, 51, 102],
  [153, 51, 153],
  [153, 51, 204],
  [153, 51, 255],
  [153, 102, 0],
  [153, 102, 51],
  [153, 102, 102],
  [153, 102, 153],
  [153, 102, 204],
  [153, 102, 255],
  [153, 153, 0],
  [153, 153, 51],
  [153, 153, 102],
  [153, 153, 153],
  [153, 153, 204],
  [153, 153, 255],
  [153, 204, 0],
  [153, 204, 51],
  [153, 204, 102],
  [153, 204, 153],
  [153, 204, 204],
  [153, 204, 255],
  [153, 255, 0],
  [153, 255, 51],
  [153, 255, 102],
  [153, 255, 153],
  [153, 255, 204],
  [153, 255, 255],
  [204, 0, 0],
  [204, 0, 51],
  [204, 0, 102],
  [204, 0, 153],
  [204, 0, 204],
  [204, 0, 255],
  [204, 51, 0],
  [204, 51, 51],
  [204, 51, 102],
  [204, 51, 153],
  [204, 51, 204],
  [204, 51, 255],
  [204, 102, 0],
  [204, 102, 51],
  [204, 102, 102],
  [204, 102, 153],
  [204, 102, 204],
  [204, 102, 255],
  [204, 153, 0],
  [204, 153, 51],
  [204, 153, 102],
  [204, 153, 153],
  [204, 153, 204],
  [204, 153, 255],
  [204, 204, 0],
  [204, 204, 51],
  [204, 204, 102],
  [204, 204, 153],
  [204, 204, 204],
  [204, 204, 255],
  [204, 255, 0],
  [204, 255, 51],
  [204, 255, 102],
  [204, 255, 153],
  [204, 255, 204],
  [204, 255, 255],
  [255, 0, 0],
  [255, 0, 51],
  [255, 0, 102],
  [255, 0, 153],
  [255, 0, 204],
  [255, 0, 255],
  [255, 51, 0],
  [255, 51, 51],
  [255, 51, 102],
  [255, 51, 153],
  [255, 51, 204],
  [255, 51, 255],
  [255, 102, 0],
  [255, 102, 51],
  [255, 102, 102],
  [255, 102, 153],
  [255, 102, 204],
  [255, 102, 255],
  [255, 153, 0],
  [255, 153, 51],
  [255, 153, 102],
  [255, 153, 153],
  [255, 153, 204],
  [255, 153, 255],
  [255, 204, 0],
  [255, 204, 51],
  [255, 204, 102],
  [255, 204, 153],
  [255, 204, 204],
  [255, 204, 255],
  [255, 255, 0],
  [255, 255, 51],
  [255, 255, 102],
  [255, 255, 153],
  [255, 255, 204],
  [255, 255, 255],
  [26, 26, 26],
  [55, 55, 55],
  [84, 84, 84],
  [113, 113, 113],
  [142, 142, 142],
  [171, 171, 171],
  [200, 200, 200],
  [229, 229, 229],
];
//#endregion
//#region src/hardware.ts
/** Default seed shared by native play and CLI headless runs; explicit seeds remain supported. */
var DEFAULT_SEED = 604772761;
var HARDWARE = Object.freeze({
  width: 240,
  height: 144,
  frameRate: 60,
  paletteSize: 256,
  rasterPaletteSize: 32,
  transparentColor: 0,
  visualCapacityBytes: 1048576,
  visualAllocations: 8192,
  saveCapacityBytes: 8192,
  cartridgeCapacityBytes: 1048576,
  drawCommandsPerFrame: 4096,
  workUnitsPerFrame: 5e4,
  audioVoices: 8,
  audioSampleRate: 48e3,
  trackerChannels: 8,
  controllerPorts: 4,
  spriteMaximumAxis: 128,
  tileSize: 8,
});
/** Shared immutable 256-colour data, also embedded by modl-core for native tools. */
var MASTER_PALETTE = Object.freeze(
  palette_default.map(
    (rgb) => `#${rgb.map((value) => value.toString(16).padStart(2, '0')).join('')}`,
  ),
);
var MASTER_PALETTE_RGBA = Object.freeze(
  MASTER_PALETTE.flatMap((hex) => [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
    255,
  ]),
);
//#endregion
//#region src/budget.ts
function isWorkBudgetSnapshot(value) {
  if (
    !isRecord$8(value) ||
    Object.keys(value).length !== 4 ||
    value.revision !== 1 ||
    typeof value.limit !== 'number' ||
    !Number.isSafeInteger(value.limit) ||
    value.limit < 1 ||
    value.limit > HARDWARE.workUnitsPerFrame ||
    typeof value.used !== 'number' ||
    !Number.isSafeInteger(value.used) ||
    value.used < 0 ||
    !Array.isArray(value.attribution) ||
    value.attribution.length > HARDWARE.workUnitsPerFrame + 1
  )
    return false;
  let total = 0;
  let previousUnits = Infinity;
  let previousStart = -1;
  const spans = /* @__PURE__ */ new Set();
  for (const entry of value.attribution) {
    if (
      !isRecord$8(entry) ||
      Object.keys(entry).length !== 2 ||
      typeof entry.units !== 'number' ||
      !Number.isSafeInteger(entry.units) ||
      entry.units < 0 ||
      entry.units > value.used - total ||
      !isRecord$8(entry.sourceSpan) ||
      Object.keys(entry.sourceSpan).length !== 2 ||
      typeof entry.sourceSpan.start !== 'number' ||
      !Number.isSafeInteger(entry.sourceSpan.start) ||
      entry.sourceSpan.start < 0 ||
      typeof entry.sourceSpan.end !== 'number' ||
      !Number.isSafeInteger(entry.sourceSpan.end) ||
      entry.sourceSpan.end < entry.sourceSpan.start ||
      entry.units > previousUnits ||
      (entry.units === previousUnits && entry.sourceSpan.start < previousStart)
    )
      return false;
    const key = `${String(entry.sourceSpan.start)}:${String(entry.sourceSpan.end)}`;
    if (spans.has(key)) return false;
    spans.add(key);
    total += entry.units;
    previousUnits = entry.units;
    previousStart = entry.sourceSpan.start;
  }
  return total === value.used;
}
function isRecord$8(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
/** Per-frame synthetic execution budget and source-span attribution. */
var WorkBudget = class {
  frameLimit;
  usedUnits = 0;
  bySpan = /* @__PURE__ */ new Map();
  constructor(frameLimit) {
    if (
      !Number.isSafeInteger(frameLimit) ||
      frameLimit <= 0 ||
      frameLimit > HARDWARE.workUnitsPerFrame
    )
      throw new RangeError('work-unit limit must be an integer between 1 and 50,000');
    this.frameLimit = frameLimit;
  }
  get limit() {
    return this.frameLimit;
  }
  get used() {
    return this.usedUnits;
  }
  beginFrame() {
    this.usedUnits = 0;
    this.bySpan.clear();
  }
  snapshot() {
    return structuredClone({
      revision: 1,
      limit: this.frameLimit,
      used: this.usedUnits,
      attribution: this.attribution(),
    });
  }
  restore(value) {
    if (!isWorkBudgetSnapshot(value) || value.limit !== this.frameLimit)
      throw new TypeError('invalid or mismatched work-budget snapshot');
    const entries = structuredClone(value.attribution);
    this.usedUnits = value.used;
    this.bySpan.clear();
    for (const entry of entries)
      this.bySpan.set(`${String(entry.sourceSpan.start)}:${String(entry.sourceSpan.end)}`, entry);
  }
  charge(units, sourceSpan) {
    if (!Number.isInteger(units) || units < 0)
      throw new RangeError('work-unit charge must be a non-negative finite integer');
    const charged = Math.min(units, Number.MAX_SAFE_INTEGER - this.usedUnits);
    this.usedUnits += charged;
    const key = `${String(sourceSpan.start)}:${String(sourceSpan.end)}`;
    const previous = this.bySpan.get(key);
    this.bySpan.set(key, {
      sourceSpan,
      units: (previous?.units ?? 0) + charged,
    });
    if (charged !== units || this.usedUnits > this.frameLimit)
      throw new BudgetExceeded(this.usedUnits, this.frameLimit, sourceSpan);
  }
  attribution() {
    return [...this.bySpan.values()].sort(
      (left, right) => right.units - left.units || left.sourceSpan.start - right.sourceSpan.start,
    );
  }
};
//#endregion
//#region src/font.ts
var BITMAP_FONT = Object.freeze({
  glyphWidth: 5,
  glyphHeight: 7,
  advanceX: 6,
  advanceY: 8,
});
var GLYPHS = Object.freeze({
  ' ': [0, 0, 0, 0, 0, 0, 0],
  '!': [4, 4, 4, 4, 4, 0, 4],
  '"': [10, 10, 10, 0, 0, 0, 0],
  '#': [10, 31, 10, 10, 31, 10, 0],
  $: [4, 15, 20, 14, 5, 30, 4],
  '%': [25, 26, 4, 4, 11, 19, 0],
  '&': [12, 18, 20, 8, 21, 18, 13],
  "'": [4, 4, 8, 0, 0, 0, 0],
  '(': [2, 4, 8, 8, 8, 4, 2],
  ')': [8, 4, 2, 2, 2, 4, 8],
  '*': [0, 21, 14, 31, 14, 21, 0],
  '+': [0, 4, 4, 31, 4, 4, 0],
  ',': [0, 0, 0, 0, 4, 4, 8],
  '-': [0, 0, 0, 31, 0, 0, 0],
  '.': [0, 0, 0, 0, 0, 12, 12],
  '/': [1, 2, 2, 4, 8, 8, 16],
  0: [14, 17, 19, 21, 25, 17, 14],
  1: [4, 12, 4, 4, 4, 4, 14],
  2: [14, 17, 1, 2, 4, 8, 31],
  3: [30, 1, 1, 14, 1, 1, 30],
  4: [2, 6, 10, 18, 31, 2, 2],
  5: [31, 16, 16, 30, 1, 1, 30],
  6: [14, 16, 16, 30, 17, 17, 14],
  7: [31, 1, 2, 4, 8, 8, 8],
  8: [14, 17, 17, 14, 17, 17, 14],
  9: [14, 17, 17, 15, 1, 1, 14],
  ':': [0, 12, 12, 0, 12, 12, 0],
  ';': [0, 12, 12, 0, 4, 4, 8],
  '<': [2, 4, 8, 16, 8, 4, 2],
  '=': [0, 0, 31, 0, 31, 0, 0],
  '>': [8, 4, 2, 1, 2, 4, 8],
  '?': [14, 17, 1, 2, 4, 0, 4],
  '@': [14, 17, 23, 21, 23, 16, 14],
  A: [14, 17, 17, 31, 17, 17, 17],
  B: [30, 17, 17, 30, 17, 17, 30],
  C: [14, 17, 16, 16, 16, 17, 14],
  D: [28, 18, 17, 17, 17, 18, 28],
  E: [31, 16, 16, 30, 16, 16, 31],
  F: [31, 16, 16, 30, 16, 16, 16],
  G: [14, 17, 16, 23, 17, 17, 15],
  H: [17, 17, 17, 31, 17, 17, 17],
  I: [14, 4, 4, 4, 4, 4, 14],
  J: [7, 2, 2, 2, 2, 18, 12],
  K: [17, 18, 20, 24, 20, 18, 17],
  L: [16, 16, 16, 16, 16, 16, 31],
  M: [17, 27, 21, 21, 17, 17, 17],
  N: [17, 25, 25, 21, 19, 19, 17],
  O: [14, 17, 17, 17, 17, 17, 14],
  P: [30, 17, 17, 30, 16, 16, 16],
  Q: [14, 17, 17, 17, 21, 18, 13],
  R: [30, 17, 17, 30, 20, 18, 17],
  S: [15, 16, 16, 14, 1, 1, 30],
  T: [31, 4, 4, 4, 4, 4, 4],
  U: [17, 17, 17, 17, 17, 17, 14],
  V: [17, 17, 17, 17, 17, 10, 4],
  W: [17, 17, 17, 21, 21, 21, 10],
  X: [17, 17, 10, 4, 10, 17, 17],
  Y: [17, 17, 10, 4, 4, 4, 4],
  Z: [31, 1, 2, 4, 8, 16, 31],
  '[': [14, 8, 8, 8, 8, 8, 14],
  '\\': [16, 8, 8, 4, 2, 2, 1],
  ']': [14, 2, 2, 2, 2, 2, 14],
  '^': [4, 10, 17, 0, 0, 0, 0],
  _: [0, 0, 0, 0, 0, 0, 31],
  '`': [8, 4, 2, 0, 0, 0, 0],
  '{': [3, 4, 4, 24, 4, 4, 3],
  '|': [4, 4, 4, 4, 4, 4, 4],
  '}': [24, 4, 4, 3, 4, 4, 24],
  '~': [0, 0, 9, 22, 0, 0, 0],
});
var FALLBACK = GLYPHS['?'];
/** Lowercase has a compact small-cap treatment in revision 1 of the original console font. */
function glyphRows(character) {
  if (character.length !== 1) return FALLBACK;
  return GLYPHS[character] ?? GLYPHS[character.toUpperCase()] ?? FALLBACK;
}
//#endregion
//#region src/bus.ts
var MEMORY = Object.freeze({
  size: 4194304,
  ram: 0,
  ramBytes: 65536,
  front: 65536,
  back: 102400,
  display: 139264,
  visual: 2162688,
  draw: 327680,
  transparency: 327760,
  pattern: 327792,
  palette: 2097152,
  input: 327936,
  inputBytes: 48,
  system: 328192,
  systemBytes: 64,
  rasterLive: 328704,
  visualInfo: 328448,
  saveControl: 328960,
  cartridgeInfo: 329216,
  save: 360448,
  saveCommitted: 368640,
  cartridgeRom: 1048576,
  audio: 331776,
  audioTracker: 331808,
  audioVoices: 332032,
  voiceStride: 64,
  audioAssets: 3932160,
  audioAssetStride: 32,
  raster: 348160,
  rasterStride: 40,
  assets: 655360,
  assetStride: 32,
  allocations: 786432,
  allocationStride: 24,
});
function regionLength(region) {
  return 'bytes' in region ? region.bytes.length : region.length;
}
function isMemorySnapshot(value) {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('revision' in value) ||
    value.revision !== 1 ||
    !('regions' in value) ||
    !Array.isArray(value.regions) ||
    Object.keys(value).length !== 2 ||
    value.regions.length > 4096
  )
    return false;
  let end = 0;
  for (const region of value.regions) {
    if (
      typeof region !== 'object' ||
      region === null ||
      Object.keys(region).length !== 2 ||
      !('address' in region) ||
      typeof region.address !== 'number' ||
      !Number.isSafeInteger(region.address) ||
      region.address < end ||
      !('bytes' in region) ||
      !(region.bytes instanceof Uint8Array) ||
      region.bytes.length === 0 ||
      region.bytes.length > MEMORY.size - region.address
    )
      return false;
    end = region.address + region.bytes.length;
  }
  return true;
}
/** Byte-addressed access to real device storage; reserved holes read zero and reject writes. */
var MemoryBus = class {
  regions;
  charge;
  constructor(regions, charge) {
    this.regions = [...regions].sort((left, right) => left.address - right.address);
    this.charge = charge;
    let end = 0;
    for (const region of this.regions) {
      const length = regionLength(region);
      if (
        !Number.isSafeInteger(region.address) ||
        region.address < end ||
        !Number.isSafeInteger(length) ||
        length <= 0 ||
        length > MEMORY.size - region.address
      )
        throw new TypeError('invalid or overlapping hardware region');
      end = region.address + length;
    }
  }
  read(address, width, span) {
    this.range(address, width, span);
    this.charge(width, span);
    const low = this.byte(address);
    return width === 1 ? low : low + this.byte(address + 1) * 256;
  }
  write(address, value, width, span, raster = false) {
    this.range(address, width, span);
    this.value(value, width === 1 ? 255 : 65535, span);
    this.charge(width, span);
    const bytes = width === 1 ? Uint8Array.of(value) : Uint8Array.of(value & 255, value >>> 8);
    this.store(address, bytes, span, raster);
  }
  fill(address, value, length, span, raster = false) {
    this.range(address, length, span);
    this.value(value, 255, span);
    this.charge(1 + length, span);
    this.store(address, new Uint8Array(length).fill(value), span, raster);
  }
  copy(destination, source, length, span, raster = false) {
    this.range(destination, length, span);
    this.range(source, length, span);
    this.charge(1 + 2 * length, span);
    const bytes = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) bytes[index] = this.byte(source + index);
    this.store(destination, bytes, span, raster);
  }
  /** Bounded debugger read at an idle host message boundary; does not charge cartridge work. */
  inspect(address, length) {
    this.range(address, length, {
      start: 0,
      end: 0,
    });
    const bytes = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) bytes[index] = this.byte(address + index);
    return bytes;
  }
  /** Transactional debugger edit at an idle host message boundary; does not charge work. */
  edit(address, bytes) {
    const span = {
      start: 0,
      end: 0,
    };
    this.range(address, bytes.length, span);
    this.store(address, bytes.slice(), span, false, true);
  }
  describe() {
    return this.regions.map((region) => ({
      name: region.name,
      address: region.address,
      length: regionLength(region),
      writable: region.writable,
    }));
  }
  snapshot() {
    return {
      revision: 1,
      regions: this.retained().map(({ address, bytes }) => ({
        address,
        bytes: bytes.slice(),
      })),
    };
  }
  restore(snapshot) {
    const retained = this.retained();
    if (!isMemorySnapshot(snapshot) || snapshot.regions.length !== retained.length)
      throw new TypeError('invalid hardware memory snapshot');
    for (let index = 0; index < retained.length; index += 1) {
      const target = retained[index];
      const source = snapshot.regions[index];
      if (
        target === void 0 ||
        source === void 0 ||
        target.address !== source.address ||
        target.bytes.length !== source.bytes.length ||
        target.validate?.(0, source.bytes) === false
      )
        throw new TypeError('hardware memory snapshot does not match this cartridge');
    }
    for (let index = 0; index < retained.length; index += 1)
      retained[index]?.bytes.set(snapshot.regions[index]?.bytes ?? []);
  }
  retained() {
    return this.regions.filter(
      (region) => 'bytes' in region && (region.retained ?? region.writable),
    );
  }
  byte(address) {
    const region = this.regions.find(
      (entry) => address >= entry.address && address < entry.address + regionLength(entry),
    );
    if (region === void 0) return 0;
    return 'bytes' in region
      ? (region.bytes[address - region.address] ?? 0)
      : region.readByte(address - region.address);
  }
  store(address, bytes, span, raster, debugEdit = false) {
    const writes = [];
    for (let index = 0; index < bytes.length;) {
      const cursor = address + index;
      const region = this.regions.find(
        (entry) => cursor >= entry.address && cursor < entry.address + regionLength(entry),
      );
      if (region === void 0 || !region.writable)
        throw new RuntimeFault('M019021', 'write to read-only or reserved hardware memory', span);
      if (raster && !region.rasterWritable)
        throw new RuntimeFault(
          'M019011',
          'hardware write is not valid in the raster callback',
          span,
        );
      const offset = cursor - region.address;
      const count = Math.min(bytes.length - index, regionLength(region) - offset);
      const part = bytes.subarray(index, index + count);
      const commit =
        'bytes' in region
          ? region.validate?.(offset, part) === false
            ? void 0
            : () => {
                region.bytes.set(part, offset);
              }
          : region.prepareWrite(offset, part, span, debugEdit);
      if (commit === void 0)
        throw new RuntimeFault(
          'M019022',
          `invalid value for hardware region '${region.name}'`,
          span,
        );
      writes.push(commit);
      index += count;
    }
    for (const write of writes) write();
  }
  range(address, length, span) {
    if (
      !Number.isSafeInteger(address) ||
      !Number.isSafeInteger(length) ||
      address < 0 ||
      length < 0 ||
      address > MEMORY.size ||
      length > MEMORY.size - address
    )
      throw new RuntimeFault('M019020', 'hardware address or range is out of bounds', span);
  }
  value(value, maximum, span) {
    if (!Number.isSafeInteger(value) || value < 0 || value > maximum)
      throw new RuntimeFault(
        'M019022',
        'hardware value is outside the unsigned operation width',
        span,
      );
  }
};
//#endregion
//#region src/visual-store.ts
/** A single packed, little-endian visual image; all drawing views alias these bytes. */
var VisualAssetStore = class {
  entries = /* @__PURE__ */ new Map();
  ids = /* @__PURE__ */ new Map();
  bytes = new Uint8Array(HARDWARE.visualCapacityBytes);
  allocations = [];
  descriptors;
  allocationTable;
  info = /* @__PURE__ */ new Uint8Array(24);
  display;
  usedBytes;
  constructor(assets = [], display) {
    if (assets.length > 4096) throw new RangeError('too many visual assets');
    const sorted = [...assets].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    const source = /* @__PURE__ */ new Map();
    let usedBytes = 0;
    for (const [id, asset] of sorted.entries()) {
      if (source.has(asset.name)) throw new TypeError(`duplicate visual asset '${asset.name}'`);
      validateAsset(asset);
      usedBytes += visualAssetBytes(asset);
      source.set(asset.name, asset);
      this.ids.set(asset.name, id);
    }
    if (display !== void 0) usedBytes += 32 + display.raster.length * 38;
    if (usedBytes > HARDWARE.visualCapacityBytes)
      throw new RangeError('visual assets exceed the 1 MiB shared capacity');
    this.usedBytes = usedBytes;
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
    this.display = display === void 0 ? void 0 : this.storeDisplay(display);
    this.allocationTable = new Uint8Array(this.allocations.length * MEMORY.allocationStride);
    const allocationView = new DataView(this.allocationTable.buffer);
    for (const [index, entry] of this.allocations.entries()) {
      const offset = index * MEMORY.allocationStride;
      [
        entry.kind,
        MEMORY.visual + entry.offset,
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
      display === void 0 ? 0 : MEMORY.visual + usedBytes - 32 - display.raster.length * 38,
    ].forEach((value, field) => {
      info.setUint32(field * 4, value, true);
    });
  }
  get(name) {
    return this.entries.get(name);
  }
  id(name) {
    return this.ids.get(name) ?? -1;
  }
  mapCell(name, layer, x, y) {
    const asset = this.entries.get(name);
    if (asset?.kind !== 'map') return void 0;
    const selected = asset.layers[layer];
    if (selected === void 0 || x < 0 || y < 0 || x >= selected.width || y >= selected.height)
      return void 0;
    return selected.cells.getUint16((y * selected.width + x) * 2, true);
  }
  mapFlag(name, layer, x, y, flag) {
    const asset = this.entries.get(name);
    if (asset?.kind !== 'map' || flag < 0 || flag > 7) return false;
    const selected = asset.layers[layer];
    if (selected === void 0) return false;
    const tile = this.mapCell(name, layer, x, y);
    const tileSet = this.entries.get(selected.tileSet);
    return (
      tile !== void 0 &&
      tileSet?.kind === 'tile_set' &&
      ((tileSet.flags[tile] ?? 0) & (1 << flag)) !== 0
    );
  }
  memoryRegions() {
    return [
      {
        name: 'visual store',
        address: MEMORY.visual,
        bytes: this.bytes,
        writable: true,
        validate: (offset, bytes) => this.validate(offset, bytes),
      },
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
  allocate(kind, width, height, data, reference = 0, validate) {
    if (this.allocations.length >= HARDWARE.visualAllocations)
      throw new RangeError('visual allocation descriptors exceed 8192 slots');
    const previous = this.allocations.at(-1);
    const offset = previous === void 0 ? 0 : previous.offset + previous.bytes.length;
    const bytes = this.bytes.subarray(offset, offset + data.length);
    bytes.set(data);
    this.allocations.push({
      offset,
      bytes,
      kind,
      width,
      height,
      reference,
      ...(validate === void 0 ? {} : { validate }),
    });
    return bytes;
  }
  sprite(sprite) {
    return {
      ...sprite,
      pixels: this.allocate(1, sprite.width, sprite.height, sprite.pixels, 0, (_offset, bytes) =>
        bytes.every((color) => color < HARDWARE.paletteSize),
      ),
    };
  }
  store(asset, source) {
    switch (asset.kind) {
      case 'sprite':
        return this.sprite(asset);
      case 'animation':
        return {
          ...asset,
          frames: asset.frames.map((frame) => this.sprite(frame)),
        };
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
                const byte = (index) =>
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
        const bitmapOffsets = /* @__PURE__ */ new Set();
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
  storeDisplay(display) {
    if (display.remap.length !== HARDWARE.rasterPaletteSize || display.raster.length > 144)
      throw new TypeError('invalid display defaults');
    const remap = this.allocate(4, 32, 1, display.remap, 0, (_offset, bytes) =>
      bytes.every((value) => value < HARDWARE.paletteSize),
    );
    let previous = -1;
    return {
      remap,
      raster: display.raster.map((row) => {
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
        const encoded = /* @__PURE__ */ new Uint8Array(38);
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
      }),
    };
  }
  validate(offset, bytes) {
    let low = 0;
    let high = this.allocations.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      const entry = this.allocations[mid];
      if (entry !== void 0 && entry.offset + entry.bytes.length <= offset) low = mid + 1;
      else high = mid;
    }
    for (let index = low; index < this.allocations.length; index += 1) {
      const entry = this.allocations[index];
      if (entry === void 0 || entry.offset >= offset + bytes.length) break;
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
};
function validateAsset(asset) {
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
      for (const [code, pixels] of asset.glyphs)
        if (
          !Number.isSafeInteger(code) ||
          code < 0 ||
          code > 255 ||
          pixels.length !== asset.glyphWidth * asset.glyphHeight ||
          pixels.some((value) => value > 1)
        )
          throw new RangeError(`font '${asset.name}' has an invalid glyph`);
      return;
  }
}
function validateSprite(sprite) {
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
function visualAssetBytes(asset) {
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
//#endregion
//#region src/graphics.ts
function isGraphicsSnapshot(value) {
  const pixels = HARDWARE.width * HARDWARE.height;
  return (
    isRecord$7(value) &&
    value.revision === 1 &&
    value.front instanceof Uint8Array &&
    value.front.length === pixels &&
    value.front.every((color) => color < HARDWARE.paletteSize) &&
    value.resolved instanceof Uint8Array &&
    value.resolved.length === pixels &&
    value.resolved.every((color) => color < HARDWARE.paletteSize)
  );
}
/** Deterministic indexed immediate-mode rasterizer with double-buffered storage. */
var IndexedGraphics = class {
  front = new Uint8Array(HARDWARE.width * HARDWARE.height);
  back = new Uint8Array(HARDWARE.width * HARDWARE.height);
  resolved = new Uint8Array(HARDWARE.width * HARDWARE.height);
  assets;
  display;
  drawRegisters = /* @__PURE__ */ new Uint8Array(80);
  transparency = new Uint8Array(HARDWARE.paletteSize / 8);
  pattern = /* @__PURE__ */ new Uint8Array(8);
  state = new MemoryDrawState(this.drawRegisters);
  rasterBytes = new Uint8Array(HARDWARE.height * MEMORY.rasterStride);
  rasterView = new DataView(this.rasterBytes.buffer);
  rasterRemaps = Array.from({ length: HARDWARE.height }, (_, line) =>
    this.rasterBytes.subarray(line * MEMORY.rasterStride + 8, (line + 1) * MEMORY.rasterStride),
  );
  rasterLive = /* @__PURE__ */ new Uint8Array(48);
  rasterLiveView = new DataView(this.rasterLive.buffer);
  displayRemap = this.rasterLive.subarray(16);
  commandCount = 0;
  activeFrame = false;
  constructor(assets = new VisualAssetStore(), display) {
    this.assets = assets;
    this.display =
      display === void 0
        ? (assets.display ?? copyDisplayConfiguration())
        : copyDisplayConfiguration(display);
    this.state.clipWidth = HARDWARE.width;
    this.state.clipHeight = HARDWARE.height;
    this.state.remap.set(this.display.remap);
    this.displayRemap.set(identityRemap());
    this.resetTransparency();
  }
  executeFrame(commands) {
    if (commands.length > HARDWARE.drawCommandsPerFrame)
      throw new RangeError('draw-command ceiling exceeded');
    this.beginFrame();
    for (const command of commands) this.executeCommand(command);
    return this.finishFrame();
  }
  beginFrame() {
    this.back.set(this.front);
    this.drawRegisters.fill(0);
    this.resetTransparency();
    this.state.clipWidth = HARDWARE.width;
    this.pattern.fill(0);
    this.state.clipHeight = HARDWARE.height;
    this.state.remap.set(this.display.remap);
    this.rasterBytes.fill(0);
    this.rasterLive.fill(0);
    this.displayRemap.set(identityRemap());
    this.commandCount = 0;
    this.activeFrame = true;
    for (const raster of this.display.raster) {
      this.rasterRemaps[raster.line]?.set(raster.remap);
      this.rasterView.setInt16(raster.line * MEMORY.rasterStride + 4, raster.scrollX, true);
      this.rasterView.setInt16(raster.line * MEMORY.rasterStride + 6, raster.scrollY, true);
      this.rasterBytes[raster.line * MEMORY.rasterStride] = 1;
    }
  }
  executeCommand(command) {
    if (!this.activeFrame) throw new Error('graphics frame has not begun');
    if (this.commandCount >= HARDWARE.drawCommandsPerFrame)
      throw new RangeError('draw-command ceiling exceeded');
    this.commandCount += 1;
    if (command.rasterLine === void 0) {
      const writesState = ['camera', 'clip', 'clip_reset', 'pal', 'pal_reset'].includes(
        command.name,
      );
      this.executeDraw(command, writesState ? this.state : this.state.capture());
      return;
    }
    const line = command.rasterLine;
    if (line < 0 || line >= HARDWARE.height)
      throw new RangeError('raster command has an invalid scanline');
    if (command.name === 'pal') {
      const [from, to] = expectIntegers(command, 2);
      this.displayRemap[expectRemappableColor(from)] = expectColor(to);
    } else if (command.name === 'raster_scroll') {
      const [x, y] = expectIntegers(command, 2);
      this.rasterLiveView.setFloat64(0, x, true);
      this.rasterLiveView.setFloat64(8, y, true);
    } else throw new TypeError(`'${command.name}' is not valid during raster display`);
    this.rasterView.setInt16(
      line * MEMORY.rasterStride + 4,
      clampInt16(this.rasterLiveView.getFloat64(0, true)),
      true,
    );
    this.rasterView.setInt16(
      line * MEMORY.rasterStride + 6,
      clampInt16(this.rasterLiveView.getFloat64(8, true)),
      true,
    );
    this.rasterRemaps[line]?.set(this.displayRemap);
    this.rasterBytes[line * MEMORY.rasterStride] = 1;
  }
  finishFrame() {
    if (!this.activeFrame) throw new Error('graphics frame has not begun');
    let previousRemap = identityRemap();
    let previousScrollX = 0;
    let previousScrollY = 0;
    for (let y = 0; y < HARDWARE.height; y += 1) {
      const lineRemap = this.rasterRemaps[y];
      if (lineRemap !== void 0 && this.rasterBytes[y * MEMORY.rasterStride] === 1) {
        previousRemap = lineRemap;
        previousScrollX = this.rasterView.getInt16(y * MEMORY.rasterStride + 4, true);
        previousScrollY = this.rasterView.getInt16(y * MEMORY.rasterStride + 6, true);
      }
      for (let x = 0; x < HARDWARE.width; x += 1) {
        const sourceX = wrap(x + previousScrollX, HARDWARE.width);
        const sourceY = wrap(y + previousScrollY, HARDWARE.height);
        const color = this.back[sourceY * HARDWARE.width + sourceX] ?? 0;
        this.resolved[y * HARDWARE.width + x] = previousRemap[color] ?? color;
      }
    }
    this.front.set(this.back);
    this.activeFrame = false;
    return {
      indexedPixels: this.resolved.slice(),
      commands: this.commandCount,
    };
  }
  snapshot() {
    return {
      revision: 1,
      front: this.front.slice(),
      resolved: this.resolved.slice(),
    };
  }
  memoryRegions() {
    const validate = (_offset, bytes) => bytes.every((color) => color < HARDWARE.paletteSize);
    return [
      {
        name: 'front',
        address: MEMORY.front,
        bytes: this.front,
        writable: true,
        validate,
      },
      {
        name: 'back',
        address: MEMORY.back,
        bytes: this.back,
        writable: true,
        validate,
      },
      {
        name: 'display',
        address: MEMORY.display,
        bytes: this.resolved,
        writable: false,
        retained: true,
        validate,
      },
      {
        name: 'draw state',
        address: MEMORY.draw,
        bytes: this.drawRegisters,
        writable: true,
        validate: (offset, bytes) => {
          const candidate = this.drawRegisters.slice();
          candidate.set(bytes, offset);
          const view = new DataView(candidate.buffer);
          for (let field = 0; field < 48; field += 8)
            if (!Number.isSafeInteger(view.getFloat64(field, true))) return false;
          return (
            view.getFloat64(32, true) >= 0 &&
            view.getFloat64(40, true) >= 0 &&
            candidate.subarray(48).every((color) => color < HARDWARE.paletteSize)
          );
        },
      },
      {
        name: 'blit transparency mask',
        address: MEMORY.transparency,
        bytes: this.transparency,
        writable: true,
      },
      {
        name: 'primitive fill pattern',
        address: MEMORY.pattern,
        bytes: this.pattern,
        writable: true,
        validate: (offset, bytes) => {
          const candidate = this.pattern.slice();
          candidate.set(bytes, offset);
          return (
            (candidate[3] ?? 0) <= 1 &&
            (candidate[4] ?? 0) <= 1 &&
            candidate.subarray(5).every((value) => value === 0)
          );
        },
      },
      {
        name: 'raster callback state',
        address: MEMORY.rasterLive,
        bytes: this.rasterLive,
        writable: false,
        retained: true,
        validate: (_offset, bytes) => {
          const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
          return (
            bytes.length === 48 &&
            Number.isSafeInteger(view.getFloat64(0, true)) &&
            Number.isSafeInteger(view.getFloat64(8, true)) &&
            bytes.subarray(16).every((color) => color < HARDWARE.paletteSize)
          );
        },
      },
      {
        name: 'raster table',
        address: MEMORY.raster,
        bytes: this.rasterBytes,
        writable: true,
        rasterWritable: true,
        validate: (offset, bytes) =>
          bytes.every((value, index) => {
            const field = (offset + index) % MEMORY.rasterStride;
            return field === 0
              ? value <= 1
              : field < 4
                ? value === 0
                : field < 8 || value < HARDWARE.paletteSize;
          }),
      },
    ];
  }
  restore(snapshot) {
    if (!isGraphicsSnapshot(snapshot)) throw new TypeError('invalid indexed graphics snapshot');
    this.front.set(snapshot.front);
    this.back.set(snapshot.front);
    this.resolved.set(snapshot.resolved);
    this.activeFrame = false;
  }
  executeDraw(command, state) {
    switch (command.name) {
      case 'clear': {
        const [color] = expectIntegers(command, 1);
        this.back.fill(state.remap[expectColor(color)] ?? color);
        return;
      }
      case 'pixel': {
        const [x, y, color] = expectIntegers(command, 3);
        this.plot(x, y, color, state, true);
        return;
      }
      case 'line': {
        const [x0, y0, x1, y1, color] = expectIntegers(command, 5);
        this.line(x0, y0, x1, y1, color, state);
        return;
      }
      case 'rect':
      case 'rect_fill': {
        const [x, y, width, height, color] = expectIntegers(command, 5);
        this.rectangle(x, y, width, height, color, command.name === 'rect_fill', state);
        return;
      }
      case 'circle':
      case 'circle_fill': {
        const [x, y, radius, color] = expectIntegers(command, 4);
        this.circle(x, y, radius, color, command.name === 'circle_fill', state);
        return;
      }
      case 'triangle': {
        const [x0, y0, x1, y1, x2, y2, color] = expectIntegers(command, 7);
        this.triangle(x0, y0, x1, y1, x2, y2, color, state);
        return;
      }
      case 'camera':
        [state.cameraX, state.cameraY] = expectIntegers(command, 2);
        return;
      case 'clip': {
        const [x, y, width, height] = expectIntegers(command, 4);
        state.clipX = x;
        state.clipY = y;
        state.clipWidth = Math.max(0, width);
        state.clipHeight = Math.max(0, height);
        return;
      }
      case 'clip_reset':
        expectIntegers(command, 0);
        state.clipX = 0;
        state.clipY = 0;
        state.clipWidth = HARDWARE.width;
        state.clipHeight = HARDWARE.height;
        return;
      case 'pal': {
        const [from, to] = expectIntegers(command, 2);
        state.remap[expectRemappableColor(from)] = expectColor(to);
        return;
      }
      case 'pal_reset':
        expectIntegers(command, 0);
        state.remap.set(identityRemap());
        return;
      case 'palt': {
        if (command.arguments.length !== 2) throw new TypeError('palt expects two arguments');
        const color = expectColor(expectInteger$1(command.arguments[0]));
        const transparent = expectBoolean(command.arguments[1]);
        const byte = color >>> 3;
        const bit = 1 << (color & 7);
        this.transparency[byte] = transparent
          ? (this.transparency[byte] ?? 0) | bit
          : (this.transparency[byte] ?? 0) & ~bit;
        return;
      }
      case 'palt_reset':
        expectIntegers(command, 0);
        this.resetTransparency();
        return;
      case 'fill_pattern': {
        if (command.arguments.length !== 3)
          throw new TypeError('fill_pattern expects three arguments');
        const mask = expectInteger$1(command.arguments[0]);
        const second = expectColor(expectInteger$1(command.arguments[1]));
        const transparent = expectBoolean(command.arguments[2]);
        if (mask < 0 || mask > 65535)
          throw new RangeError('fill pattern mask must be between 0 and 65535');
        this.pattern.set([mask & 255, mask >>> 8, second, 1, Number(transparent), 0, 0, 0]);
        return;
      }
      case 'fill_pattern_reset':
        expectIntegers(command, 0);
        this.pattern.fill(0);
        return;
      case 'sprite': {
        const [handle, x, y] = command.arguments;
        this.drawSprite(
          readAssetName$1(handle, 'Sprite'),
          expectInteger$1(x),
          expectInteger$1(y),
          state,
        );
        return;
      }
      case 'animation': {
        const [handle, frame, x, y] = command.arguments;
        this.drawAnimation(
          readAssetName$1(handle, 'Animation'),
          expectInteger$1(frame),
          expectInteger$1(x),
          expectInteger$1(y),
          state,
        );
        return;
      }
      case 'sprite_xform': {
        const [handle, x, y, scale, quarterTurns, flipX, flipY] = command.arguments;
        this.drawTransformedSprite(
          readAssetName$1(handle, 'Sprite'),
          expectInteger$1(x),
          expectInteger$1(y),
          expectInteger$1(scale),
          expectInteger$1(quarterTurns),
          expectBoolean(flipX),
          expectBoolean(flipY),
          state,
        );
        return;
      }
      case 'sprite_region':
        if (command.arguments.length !== 11)
          throw new TypeError('sprite_region expects eleven arguments');
        this.drawSpriteRegion(
          readAssetName$1(command.arguments[0], 'Sprite'),
          expectInteger$1(command.arguments[1]),
          expectInteger$1(command.arguments[2]),
          expectInteger$1(command.arguments[3]),
          expectInteger$1(command.arguments[4]),
          expectInteger$1(command.arguments[5]),
          expectInteger$1(command.arguments[6]),
          expectInteger$1(command.arguments[7]),
          expectInteger$1(command.arguments[8]),
          expectBoolean(command.arguments[9]),
          expectBoolean(command.arguments[10]),
          state,
        );
        return;
      case 'texture_line':
        if (command.arguments.length !== 10)
          throw new TypeError('texture_line expects ten arguments');
        this.textureLine(
          readAssetName$1(command.arguments[0], 'Sprite'),
          expectInteger$1(command.arguments[1]),
          expectInteger$1(command.arguments[2]),
          expectInteger$1(command.arguments[3]),
          expectInteger$1(command.arguments[4]),
          textureFixed(command.arguments[5]),
          textureFixed(command.arguments[6]),
          textureFixed(command.arguments[7]),
          textureFixed(command.arguments[8]),
          expectBoolean(command.arguments[9]),
          state,
        );
        return;
      case 'map': {
        const [handle, x, y] = command.arguments;
        this.drawMap(readAssetName$1(handle, 'Map'), expectInteger$1(x), expectInteger$1(y), state);
        return;
      }
      case 'raster_scroll':
        throw new TypeError('raster_scroll is only valid in the raster callback');
      case 'print': {
        const [text, x, y, color] = command.arguments;
        this.print(
          expectText(text),
          expectInteger$1(x),
          expectInteger$1(y),
          expectInteger$1(color),
          state,
        );
        return;
      }
      case 'font_print': {
        const [handle, text, x, y, color] = command.arguments;
        this.printFont(
          readAssetName$1(handle, 'Font'),
          expectText(text),
          expectInteger$1(x),
          expectInteger$1(y),
          expectInteger$1(color),
          state,
        );
        return;
      }
      default:
        throw new TypeError(`unknown graphics command '${command.name}'`);
    }
  }
  plot(x, y, color, state, patterned = false) {
    const screenX = x - state.cameraX;
    const screenY = y - state.cameraY;
    if (
      screenX < 0 ||
      screenX >= HARDWARE.width ||
      screenY < 0 ||
      screenY >= HARDWARE.height ||
      screenX < state.clipX ||
      screenX >= state.clipX + state.clipWidth ||
      screenY < state.clipY ||
      screenY >= state.clipY + state.clipHeight
    )
      return;
    if (patterned && this.pattern[3] === 1) {
      if (
        (((this.pattern[0] ?? 0) | ((this.pattern[1] ?? 0) << 8)) &
          (1 << (wrap(screenY, 4) * 4 + wrap(screenX, 4)))) !==
        0
      ) {
        if (this.pattern[4] === 1) return;
        color = this.pattern[2] ?? 0;
      }
    }
    this.back[screenY * HARDWARE.width + screenX] = state.remap[expectColor(color)] ?? color;
  }
  line(startX, startY, endX, endY, color, state) {
    let x = startX;
    let y = startY;
    const deltaX = Math.abs(endX - startX);
    const stepX = startX < endX ? 1 : -1;
    const deltaY = -Math.abs(endY - startY);
    const stepY = startY < endY ? 1 : -1;
    let error = deltaX + deltaY;
    for (;;) {
      this.plot(x, y, color, state, true);
      if (x === endX && y === endY) return;
      const doubled = error * 2;
      if (doubled >= deltaY) {
        error += deltaY;
        x += stepX;
      }
      if (doubled <= deltaX) {
        error += deltaX;
        y += stepY;
      }
    }
  }
  rectangle(x, y, width, height, color, filled, state) {
    if (width <= 0 || height <= 0) return;
    if (!filled) {
      this.line(x, y, x + width - 1, y, color, state);
      this.line(x, y + height - 1, x + width - 1, y + height - 1, color, state);
      this.line(x, y, x, y + height - 1, color, state);
      this.line(x + width - 1, y, x + width - 1, y + height - 1, color, state);
      return;
    }
    for (let row = 0; row < height; row += 1)
      this.line(x, y + row, x + width - 1, y + row, color, state);
  }
  circle(centerX, centerY, radius, color, filled, state) {
    if (radius < 0) return;
    let x = radius;
    let y = 0;
    let error = 1 - radius;
    while (x >= y) {
      if (filled) {
        this.line(centerX - x, centerY + y, centerX + x, centerY + y, color, state);
        this.line(centerX - x, centerY - y, centerX + x, centerY - y, color, state);
        this.line(centerX - y, centerY + x, centerX + y, centerY + x, color, state);
        this.line(centerX - y, centerY - x, centerX + y, centerY - x, color, state);
      } else
        for (const [plotX, plotY] of [
          [centerX + x, centerY + y],
          [centerX + y, centerY + x],
          [centerX - y, centerY + x],
          [centerX - x, centerY + y],
          [centerX - x, centerY - y],
          [centerX - y, centerY - x],
          [centerX + y, centerY - x],
          [centerX + x, centerY - y],
        ])
          this.plot(plotX, plotY, color, state, true);
      y += 1;
      if (error < 0) error += 2 * y + 1;
      else {
        x -= 1;
        error += 2 * (y - x) + 1;
      }
    }
  }
  triangle(x0, y0, x1, y1, x2, y2, color, state) {
    const minimumX = Math.min(x0, x1, x2);
    const maximumX = Math.max(x0, x1, x2);
    const minimumY = Math.min(y0, y1, y2);
    const maximumY = Math.max(y0, y1, y2);
    const area = edge(x0, y0, x1, y1, x2, y2);
    if (area === 0) {
      this.line(x0, y0, x1, y1, color, state);
      this.line(x1, y1, x2, y2, color, state);
      return;
    }
    for (let y = minimumY; y <= maximumY; y += 1)
      for (let x = minimumX; x <= maximumX; x += 1) {
        const first = edge(x1, y1, x2, y2, x, y);
        const second = edge(x2, y2, x0, y0, x, y);
        const third = edge(x0, y0, x1, y1, x, y);
        if (
          (area > 0 && first >= 0 && second >= 0 && third >= 0) ||
          (area < 0 && first <= 0 && second <= 0 && third <= 0)
        )
          this.plot(x, y, color, state, true);
      }
  }
  drawSprite(name, x, y, state) {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'sprite') throw new TypeError(`missing Sprite asset '${name}'`);
    this.blit(asset, x, y, 1, 0, false, false, state);
  }
  drawAnimation(name, frame, x, y, state) {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'animation' || asset.frames.length === 0)
      throw new TypeError(`missing Animation asset '${name}'`);
    const selected = asset.frames[wrap(frame, asset.frames.length)];
    if (selected !== void 0) this.blit(selected, x, y, 1, 0, false, false, state);
  }
  drawTransformedSprite(name, x, y, scale, quarterTurns, flipX, flipY, state) {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'sprite') throw new TypeError(`missing Sprite asset '${name}'`);
    if (scale < 1 || scale > 16) throw new RangeError('sprite scale must be between 1 and 16');
    this.blit(asset, x, y, scale, wrap(quarterTurns, 4), flipX, flipY, state);
  }
  blit(sprite, x, y, scale, quarterTurns, flipX, flipY, state) {
    const outputWidth = (quarterTurns % 2 === 0 ? sprite.width : sprite.height) * scale;
    const outputHeight = (quarterTurns % 2 === 0 ? sprite.height : sprite.width) * scale;
    for (let outputY = 0; outputY < outputHeight; outputY += 1)
      for (let outputX = 0; outputX < outputWidth; outputX += 1) {
        let sourceX = Math.floor(outputX / scale);
        let sourceY = Math.floor(outputY / scale);
        [sourceX, sourceY] = unrotate(sourceX, sourceY, sprite.width, sprite.height, quarterTurns);
        if (flipX) sourceX = sprite.width - 1 - sourceX;
        if (flipY) sourceY = sprite.height - 1 - sourceY;
        const color = sprite.pixels[sourceY * sprite.width + sourceX] ?? HARDWARE.transparentColor;
        this.plotTexture(x + outputX, y + outputY, color, state);
      }
  }
  drawSpriteRegion(
    name,
    sourceX,
    sourceY,
    sourceWidth,
    sourceHeight,
    x,
    y,
    width,
    height,
    flipX,
    flipY,
    state,
  ) {
    const sprite = this.assets.get(name);
    if (sprite?.kind !== 'sprite') throw new TypeError(`missing Sprite asset '${name}'`);
    if (sourceWidth <= 0 || sourceHeight <= 0 || width <= 0 || height <= 0) return;
    if (
      sourceWidth > HARDWARE.spriteMaximumAxis ||
      sourceHeight > HARDWARE.spriteMaximumAxis ||
      width > 2048 ||
      height > 2048
    )
      throw new RangeError(
        'sprite region source axes must be at most 128 and destination axes at most 2048',
      );
    if (
      sourceX >= sprite.width ||
      sourceY >= sprite.height ||
      sourceX <= -sourceWidth ||
      sourceY <= -sourceHeight
    )
      return;
    const left = x - state.cameraX;
    const top = y - state.cameraY;
    const screenState = {
      ...state,
      cameraX: 0,
      cameraY: 0,
    };
    const firstX = Math.max(0, Math.max(0, state.clipX) - left);
    const firstY = Math.max(0, Math.max(0, state.clipY) - top);
    const lastX = Math.min(width, Math.min(HARDWARE.width, state.clipX + state.clipWidth) - left);
    const lastY = Math.min(height, Math.min(HARDWARE.height, state.clipY + state.clipHeight) - top);
    for (let row = firstY; row < lastY; row++) {
      let sy = Math.floor((row * sourceHeight) / height);
      if (flipY) sy = sourceHeight - 1 - sy;
      sy += sourceY;
      if (sy < 0 || sy >= sprite.height) continue;
      for (let column = firstX; column < lastX; column++) {
        let sx = Math.floor((column * sourceWidth) / width);
        if (flipX) sx = sourceWidth - 1 - sx;
        sx += sourceX;
        if (sx < 0 || sx >= sprite.width) continue;
        this.plotTexture(
          left + column,
          top + row,
          sprite.pixels[sy * sprite.width + sx] ?? 0,
          screenState,
        );
      }
    }
  }
  plotTexture(x, y, color, state) {
    if (((this.transparency[color >>> 3] ?? 0) & (1 << (color & 7))) === 0)
      this.plot(x, y, color, state);
  }
  textureLine(name, startX, startY, endX, endY, u, v, du, dv, repeat, state) {
    const sprite = this.assets.get(name);
    if (sprite?.kind !== 'sprite') throw new TypeError(`missing Sprite asset '${name}'`);
    const dx = Math.abs(endX - startX),
      dy = -Math.abs(endY - startY);
    if (Math.max(dx, -dy) >= 4096)
      throw new RangeError('texture line must contain at most 4096 raster samples');
    let x = startX - state.cameraX,
      y = startY - state.cameraY;
    const targetX = endX - state.cameraX,
      targetY = endY - state.cameraY;
    const left = Math.max(0, state.clipX),
      right = Math.min(HARDWARE.width, state.clipX + state.clipWidth);
    const top = Math.max(0, state.clipY),
      bottom = Math.min(HARDWARE.height, state.clipY + state.clipHeight);
    if (
      Math.max(x, targetX) < left ||
      Math.min(x, targetX) >= right ||
      Math.max(y, targetY) < top ||
      Math.min(y, targetY) >= bottom
    )
      return;
    const stepX = startX < endX ? 1 : -1,
      stepY = startY < endY ? 1 : -1;
    let error = dx + dy;
    const screenState = {
      ...state,
      cameraX: 0,
      cameraY: 0,
    };
    for (;;) {
      let sx = Math.floor(u / 65536),
        sy = Math.floor(v / 65536);
      if (repeat) {
        sx = wrap(sx, sprite.width);
        sy = wrap(sy, sprite.height);
      }
      if (sx >= 0 && sx < sprite.width && sy >= 0 && sy < sprite.height)
        this.plotTexture(x, y, sprite.pixels[sy * sprite.width + sx] ?? 0, screenState);
      if (x === targetX && y === targetY) return;
      const doubled = error * 2;
      if (doubled >= dy) {
        error += dy;
        x += stepX;
      }
      if (doubled <= dx) {
        error += dx;
        y += stepY;
      }
      u += du;
      v += dv;
    }
  }
  drawMap(name, x, y, state) {
    const map = this.assets.get(name);
    if (map?.kind !== 'map') throw new TypeError(`missing Map asset '${name}'`);
    for (const layer of map.layers) {
      const tileSet = this.assets.get(layer.tileSet);
      if (tileSet?.kind !== 'tile_set')
        throw new TypeError(`missing TileSet asset '${layer.tileSet}'`);
      const clipRight = Math.min(HARDWARE.width, state.clipX + state.clipWidth);
      const clipBottom = Math.min(HARDWARE.height, state.clipY + state.clipHeight);
      const firstColumn = Math.max(
        0,
        Math.floor((state.clipX + state.cameraX - x) / HARDWARE.tileSize),
      );
      const lastColumn = Math.min(
        layer.width,
        Math.ceil((clipRight + state.cameraX - x) / HARDWARE.tileSize),
      );
      const firstRow = Math.max(
        0,
        Math.floor((state.clipY + state.cameraY - y) / HARDWARE.tileSize),
      );
      const lastRow = Math.min(
        layer.height,
        Math.ceil((clipBottom + state.cameraY - y) / HARDWARE.tileSize),
      );
      for (let row = firstRow; row < lastRow; row += 1)
        for (let column = firstColumn; column < lastColumn; column += 1) {
          const tile = tileSet.tiles[layer.cells.getUint16((row * layer.width + column) * 2, true)];
          if (tile !== void 0)
            this.blit(
              tile,
              x + column * HARDWARE.tileSize,
              y + row * HARDWARE.tileSize,
              1,
              0,
              false,
              false,
              state,
            );
        }
    }
  }
  print(text, x, y, color, state) {
    let cursorX = x;
    let cursorY = y;
    for (const character of text) {
      if (character === '\n') {
        cursorX = x;
        cursorY += BITMAP_FONT.advanceY;
        continue;
      }
      glyphRows(character).forEach((bits, row) => {
        for (let column = 0; column < BITMAP_FONT.glyphWidth; column += 1)
          if ((bits & (1 << (BITMAP_FONT.glyphWidth - 1 - column))) !== 0)
            this.plot(cursorX + column, cursorY + row, color, state);
      });
      cursorX += BITMAP_FONT.advanceX;
    }
  }
  printFont(name, text, x, y, color, state) {
    const font = this.assets.get(name);
    if (font?.kind !== 'font') throw new TypeError(`missing Font asset '${name}'`);
    let cursorX = x;
    let cursorY = y;
    for (const character of text) {
      if (character === '\n') {
        cursorX = x;
        cursorY += font.advanceY;
        continue;
      }
      const code = character.codePointAt(0) ?? font.missingGlyph;
      const glyph = font.glyphs.get(code) ?? font.glyphs.get(font.missingGlyph);
      if (glyph !== void 0) {
        for (let row = 0; row < font.glyphHeight; row += 1)
          for (let column = 0; column < font.glyphWidth; column += 1)
            if (glyph[row * font.glyphWidth + column] === 1)
              this.plot(cursorX + column, cursorY + row, color, state);
      }
      cursorX += font.advanceX;
    }
  }
  resetTransparency() {
    this.transparency.fill(0);
    this.transparency[HARDWARE.transparentColor >>> 3] = 1 << (HARDWARE.transparentColor & 7);
  }
};
/** Deterministic four-by-four Bayer choice between two palette indices. */
function orderedDither(x, y, first, second, level) {
  return ([0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5][wrap(y, 4) * 4 + wrap(x, 4)] ??
    0) < Math.max(0, Math.min(16, level))
    ? expectColor(second)
    : expectColor(first);
}
var MemoryDrawState = class {
  view;
  remap;
  constructor(bytes) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.remap = bytes.subarray(48);
  }
  capture() {
    return {
      cameraX: this.cameraX,
      cameraY: this.cameraY,
      clipX: this.clipX,
      clipY: this.clipY,
      clipWidth: this.clipWidth,
      clipHeight: this.clipHeight,
      remap: this.remap,
    };
  }
  get cameraX() {
    return this.view.getFloat64(0, true);
  }
  set cameraX(value) {
    this.view.setFloat64(0, value, true);
  }
  get cameraY() {
    return this.view.getFloat64(8, true);
  }
  set cameraY(value) {
    this.view.setFloat64(8, value, true);
  }
  get clipX() {
    return this.view.getFloat64(16, true);
  }
  set clipX(value) {
    this.view.setFloat64(16, value, true);
  }
  get clipY() {
    return this.view.getFloat64(24, true);
  }
  set clipY(value) {
    this.view.setFloat64(24, value, true);
  }
  get clipWidth() {
    return this.view.getFloat64(32, true);
  }
  set clipWidth(value) {
    this.view.setFloat64(32, value, true);
  }
  get clipHeight() {
    return this.view.getFloat64(40, true);
  }
  set clipHeight(value) {
    this.view.setFloat64(40, value, true);
  }
};
function copyDisplayConfiguration(display) {
  if (display === void 0)
    return {
      remap: identityRemap(),
      raster: [],
    };
  if (!validRemap(display.remap))
    throw new TypeError('display configuration has an invalid base remap');
  let previousLine = -1;
  const raster = display.raster.map((state) => {
    if (
      !Number.isInteger(state.line) ||
      state.line <= previousLine ||
      state.line >= HARDWARE.height ||
      !Number.isSafeInteger(state.scrollX) ||
      !Number.isSafeInteger(state.scrollY) ||
      state.scrollX < -32768 ||
      state.scrollX > 32767 ||
      state.scrollY < -32768 ||
      state.scrollY > 32767 ||
      !validRemap(state.remap)
    )
      throw new TypeError('display configuration has invalid raster state');
    previousLine = state.line;
    return {
      ...state,
      remap: state.remap.slice(),
    };
  });
  return {
    remap: display.remap.slice(),
    raster,
  };
}
function validRemap(remap) {
  return (
    remap.length === HARDWARE.rasterPaletteSize &&
    remap.every((color) => color < HARDWARE.paletteSize)
  );
}
function isRecord$7(value) {
  return typeof value === 'object' && value !== null;
}
function identityRemap() {
  return Uint8Array.from({ length: HARDWARE.rasterPaletteSize }, (_, index) => index);
}
function expectIntegers(command, count) {
  if (command.arguments.length !== count)
    throw new TypeError(
      `${command.name} expected ${String(count)} arguments, received ${String(command.arguments.length)}`,
    );
  return command.arguments.map(expectInteger$1);
}
function expectInteger$1(value) {
  if (typeof value !== 'number' || !Number.isSafeInteger(value))
    throw new TypeError('graphics arguments must be safe integers');
  return value;
}
function textureFixed(value) {
  if (typeof value !== 'number' || !Number.isFinite(value) || Math.abs(value) > 32768)
    throw new RangeError(
      'texture coordinates and deltas must be finite numbers within -32768..32768',
    );
  return Math.trunc(value * 65536);
}
function expectBoolean(value) {
  if (typeof value !== 'boolean') throw new TypeError('graphics argument must be Bool');
  return value;
}
function expectText(value) {
  if (typeof value !== 'string') throw new TypeError('graphics argument must be Text');
  return value;
}
function expectColor(value) {
  if (value < 0 || value >= HARDWARE.paletteSize)
    throw new RangeError('palette index must be between 0 and 255');
  return value;
}
function expectRemappableColor(value) {
  if (value < 0 || value >= HARDWARE.rasterPaletteSize)
    throw new RangeError('palette remap source must be between 0 and 31');
  return value;
}
function readAssetName$1(value, expectedKind) {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('name' in value) ||
    typeof value.name !== 'string' ||
    !('kind' in value) ||
    value.kind !== expectedKind
  )
    throw new TypeError(`expected a ${expectedKind} asset handle`);
  return value.name;
}
function edge(firstX, firstY, secondX, secondY, pointX, pointY) {
  return (pointX - firstX) * (secondY - firstY) - (pointY - firstY) * (secondX - firstX);
}
function unrotate(x, y, width, height, quarterTurns) {
  switch (quarterTurns) {
    case 1:
      return [y, height - 1 - x];
    case 2:
      return [width - 1 - x, height - 1 - y];
    case 3:
      return [width - 1 - y, x];
    default:
      return [x, y];
  }
}
function wrap(value, modulus) {
  return ((value % modulus) + modulus) % modulus;
}
function clampInt16(value) {
  return Math.max(-32768, Math.min(32767, value));
}
//#endregion
//#region src/menu.ts
function isMenuAction(value) {
  return typeof value === 'number' && Number.isInteger(value) && value >= 1 && value <= 5;
}
function isMenuLabel(value) {
  return typeof value === 'string' && value.length <= 24 && !/[^\x20-\x7e]/.test(value);
}
function isMenuSnapshot(value) {
  return (
    Array.isArray(value) &&
    value.length === 5 &&
    Object.keys(value).length === 5 &&
    Array.from(value).every((label) => label === null || (isMenuLabel(label) && label.length > 0))
  );
}
/** Cartridge-owned, bounded registrations. The host menu never executes a cartridge callback. */
var CartridgeMenu = class {
  labels = [null, null, null, null, null];
  set(slot, label, span, charge) {
    if (!isMenuAction(slot) || !isMenuLabel(label))
      throw new RuntimeFault(
        'M019016',
        'menu_item needs slot 1–5 and 0–24 printable ASCII characters',
        span,
      );
    charge(1 + label.length, span);
    this.labels[slot - 1] = label.length === 0 ? null : label;
  }
  requireAction(slot) {
    if (!isMenuAction(slot) || this.labels[slot - 1] === null)
      throw new RuntimeFault('M019016', 'selected menu action is not registered', {
        start: 0,
        end: 0,
      });
  }
  items() {
    return this.labels.flatMap((label, index) =>
      label === null
        ? []
        : [
            {
              slot: index + 1,
              label,
            },
          ],
    );
  }
  snapshot() {
    return [...this.labels];
  }
  restore(labels) {
    this.labels = [...labels];
  }
};
//#endregion
//#region src/input.ts
var BUTTONS = ['up', 'down', 'left', 'right', 'a', 'b', 'x', 'y', 'l', 'r', 'start', 'menu'];
function emptyInputFrame() {
  const controller = () => ({
    buttons: Object.fromEntries(BUTTONS.map((button) => [button, false])),
  });
  return {
    controllers: [controller(), controller(), controller(), controller()],
    pointer: {
      x: 0,
      y: 0,
      primary: false,
      secondary: false,
      inside: false,
    },
  };
}
/** Little-endian controller/pointer MMIO over the same frames used by the high-level API. */
function inputRegisterByte(current, previous, offset) {
  if (offset < 0 || offset >= 48 || !Number.isInteger(offset)) return 0;
  if (offset < 32) {
    const port = Math.floor(offset / 8);
    const mask = (frame) =>
      BUTTONS.reduce(
        (bits, button, bit) => bits | (frame.controllers[port]?.buttons[button] ? 1 << bit : 0),
        0,
      );
    const held = mask(current);
    const before = mask(previous);
    const field = offset % 8;
    return (
      ((field < 2 ? held : field < 4 ? before : field < 6 ? held & ~before : before & ~held) >>>
        ((offset % 2) * 8)) &
      255
    );
  }
  const field = offset - 32;
  if (field < 8) {
    const pointer = field < 4 ? current.pointer : previous.pointer;
    return ((field % 4 < 2 ? pointer.x : pointer.y) >>> ((field % 2) * 8)) & 255;
  }
  const flags = (pointer) =>
    Number(pointer.primary) | (Number(pointer.secondary) << 1) | (Number(pointer.inside) << 2);
  const held = flags(current.pointer);
  const before = flags(previous.pointer);
  return field === 8
    ? held
    : field === 9
      ? before
      : field === 10
        ? held & ~before
        : field === 11
          ? before & ~held
          : 0;
}
function isButton(value) {
  return typeof value === 'string' && BUTTONS.includes(value);
}
function isInputFrame(value) {
  if (
    !isRecord$6(value) ||
    !Object.hasOwn(value, 'controllers') ||
    !Object.hasOwn(value, 'pointer') ||
    !Object.keys(value).every((key) => ['controllers', 'pointer', 'menuAction'].includes(key)) ||
    (Object.hasOwn(value, 'menuAction') && !isMenuAction(value.menuAction)) ||
    !Array.isArray(value.controllers) ||
    value.controllers.length !== 4
  )
    return false;
  if (
    Object.keys(value.controllers).length !== 4 ||
    !Array.from(value.controllers).every(isControllerState) ||
    !isRecord$6(value.pointer)
  )
    return false;
  const pointer = value.pointer;
  return (
    hasExactKeys$1(pointer, ['x', 'y', 'primary', 'secondary', 'inside']) &&
    typeof pointer.x === 'number' &&
    Number.isSafeInteger(pointer.x) &&
    pointer.x >= 0 &&
    pointer.x < HARDWARE.width &&
    typeof pointer.y === 'number' &&
    Number.isSafeInteger(pointer.y) &&
    pointer.y >= 0 &&
    pointer.y < HARDWARE.height &&
    typeof pointer.primary === 'boolean' &&
    typeof pointer.secondary === 'boolean' &&
    typeof pointer.inside === 'boolean'
  );
}
Object.freeze({
  ArrowUp: [0, 'up'],
  ArrowDown: [0, 'down'],
  ArrowLeft: [0, 'left'],
  ArrowRight: [0, 'right'],
  KeyZ: [0, 'a'],
  KeyX: [0, 'b'],
  KeyA: [0, 'x'],
  KeyS: [0, 'y'],
  KeyQ: [0, 'l'],
  KeyW: [0, 'r'],
  Enter: [0, 'start'],
  Escape: [0, 'menu'],
  KeyI: [1, 'up'],
  KeyK: [1, 'down'],
  KeyJ: [1, 'left'],
  KeyL: [1, 'right'],
  KeyF: [1, 'a'],
  KeyG: [1, 'b'],
  KeyR: [1, 'x'],
  KeyT: [1, 'y'],
  KeyV: [1, 'l'],
  KeyB: [1, 'r'],
  Digit1: [1, 'start'],
  Backquote: [1, 'menu'],
  Numpad8: [2, 'up'],
  Numpad5: [2, 'down'],
  Numpad4: [2, 'left'],
  Numpad6: [2, 'right'],
  Numpad1: [2, 'a'],
  Numpad2: [2, 'b'],
  Numpad7: [2, 'x'],
  Numpad9: [2, 'y'],
  NumpadAdd: [2, 'l'],
  NumpadSubtract: [2, 'r'],
  NumpadEnter: [2, 'start'],
  NumpadDecimal: [2, 'menu'],
  KeyY: [3, 'up'],
  KeyH: [3, 'down'],
  KeyU: [3, 'left'],
  KeyO: [3, 'right'],
  KeyC: [3, 'a'],
  KeyD: [3, 'b'],
  KeyE: [3, 'x'],
  KeyM: [3, 'y'],
  KeyN: [3, 'l'],
  KeyP: [3, 'r'],
  Digit2: [3, 'start'],
  Digit3: [3, 'menu'],
});
Object.freeze({
  up: 12,
  down: 13,
  left: 14,
  right: 15,
  a: 0,
  b: 1,
  x: 2,
  y: 3,
  l: 4,
  r: 5,
  start: 9,
  menu: 8,
});
function isControllerState(value) {
  if (!isRecord$6(value) || !hasExactKeys$1(value, ['buttons']) || !isRecord$6(value.buttons))
    return false;
  const buttons = value.buttons;
  return (
    hasExactKeys$1(buttons, BUTTONS) &&
    BUTTONS.every((button) => typeof buttons[button] === 'boolean')
  );
}
function isRecord$6(value) {
  return typeof value === 'object' && value !== null;
}
function hasExactKeys$1(value, expected) {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => keys.includes(key));
}
//#endregion
//#region src/rng.ts
var NON_ZERO_FALLBACK = 1295003697;
/** Console-owned xorshift32 stream with an explicit serializable state. */
var DeterministicRng = class {
  current;
  constructor(seed = NON_ZERO_FALLBACK) {
    this.current = normalizeSeed(seed);
  }
  get state() {
    return this.current >>> 0;
  }
  restore(state) {
    this.current = normalizeSeed(state);
  }
  nextU32() {
    let value = this.current >>> 0;
    value ^= value << 13;
    value ^= value >>> 17;
    value ^= value << 5;
    this.current = value >>> 0;
    return this.current;
  }
  nextNum() {
    return this.nextU32() / 4294967296;
  }
  nextInt(minimum, maximumExclusive) {
    if (
      !Number.isSafeInteger(minimum) ||
      !Number.isSafeInteger(maximumExclusive) ||
      maximumExclusive <= minimum
    )
      throw new RangeError(
        'rng_int bounds must be safe integers with maximum greater than minimum',
      );
    const range = maximumExclusive - minimum;
    if (range > 4294967296) throw new RangeError('rng_int range must not exceed 2^32');
    const rejectionLimit = 4294967296 - (4294967296 % range);
    let sample;
    do sample = this.nextU32();
    while (sample >= rejectionLimit);
    return minimum + (sample % range);
  }
};
function normalizeSeed(seed) {
  if (!Number.isSafeInteger(seed)) throw new RangeError('RNG seed must be a safe integer');
  const normalized = seed >>> 0;
  return normalized === 0 ? NON_ZERO_FALLBACK : normalized;
}
//#endregion
//#region src/math.ts
var PI = 3.141592653589793;
var HALF_PI = PI / 2;
var TAU = PI * 2;
var MATH_COSTS = {
  abs: [1, 4],
  min: [2, 4],
  max: [2, 4],
  clamp: [3, 4],
  floor: [1, 4],
  ceil: [1, 4],
  round: [1, 4],
  trunc: [1, 4],
  sqrt: [1, 640],
  sin: [1, 32],
  cos: [1, 32],
  atan2: [2, 96],
};
function isMathCall(name) {
  return Object.hasOwn(MATH_COSTS, name);
}
/** Arithmetic-only transcendental algorithms: no host libm, clock or locale. */
function mathCall(name, args, span, work) {
  const spec = MATH_COSTS[name];
  if (
    spec === void 0 ||
    args.length !== spec[0] ||
    !args.every((value) => typeof value === 'number' && Number.isFinite(value))
  )
    throw new RuntimeFault('M019030', `${name} requires finite numeric arguments`, span);
  work(spec[1], span);
  const [a = 0, b = 0, c = 0] = args;
  let result;
  switch (name) {
    case 'abs':
      result = Math.abs(a);
      break;
    case 'min':
      result = Math.min(a, b);
      break;
    case 'max':
      result = Math.max(a, b);
      break;
    case 'clamp':
      if (b > c) throw new RuntimeFault('M019030', 'clamp minimum exceeds maximum', span);
      result = Math.max(b, Math.min(a, c));
      break;
    case 'floor':
      result = Math.floor(a);
      break;
    case 'ceil':
      result = Math.ceil(a);
      break;
    case 'round':
      result = Math.round(a);
      break;
    case 'trunc':
      result = Math.trunc(a);
      break;
    case 'sqrt':
      if (a < 0) throw new RuntimeFault('M019030', 'sqrt requires a nonnegative value', span);
      result = squareRoot(a);
      break;
    case 'sin':
    case 'cos':
      if (Math.abs(a) > 1e6)
        throw new RuntimeFault('M019030', 'trig angle must be within +/-1000000 radians', span);
      result = sine(name === 'cos' ? a + HALF_PI : a);
      break;
    case 'atan2':
      result = angle(a, b);
      break;
    default:
      throw new RuntimeFault('M019030', 'unknown numeric helper', span);
  }
  if (
    !Number.isFinite(result) ||
    (['floor', 'ceil', 'round', 'trunc'].includes(name) && !Number.isSafeInteger(result))
  )
    throw new RuntimeFault('M019030', 'numeric result is outside its finite/safe range', span);
  return result === 0 ? 0 : result;
}
function squareRoot(value) {
  if (value === 0) return 0;
  let normalized = value;
  let scale = 1;
  while (normalized >= 4) {
    normalized /= 4;
    scale *= 2;
  }
  while (normalized < 1) {
    normalized *= 4;
    scale /= 2;
  }
  let root = (normalized + 1) / 2;
  for (let iteration = 0; iteration < 8; iteration++) root = (root + normalized / root) / 2;
  return root * scale;
}
function sine(angle) {
  let x = angle % TAU;
  if (x > PI) x -= TAU;
  if (x < -3.141592653589793) x += TAU;
  if (x > HALF_PI) x = PI - x;
  if (x < -1.5707963267948966) x = -3.141592653589793 - x;
  let term = x;
  let sum = term;
  for (let n = 1; n <= 10; n++) {
    term *= (-x * x) / (2 * n * (2 * n + 1));
    sum += term;
  }
  return sum;
}
function arctangent(value) {
  const reduced = value > 0.41421356237309503;
  const x = reduced ? (value - 1) / (value + 1) : value;
  let term = x;
  let sum = term;
  for (let n = 1; n <= 24; n++) {
    term *= -x * x;
    sum += term / (2 * n + 1);
  }
  return (reduced ? PI / 4 : 0) + sum;
}
function angle(y, x) {
  if (x === 0 && y === 0) return 0;
  const ax = Math.abs(x),
    ay = Math.abs(y);
  let result = ay > ax ? HALF_PI - arctangent(ax / ay) : arctangent(ay / ax);
  if (x < 0) result = PI - result;
  return y < 0 ? -result : result;
}
//#endregion
//#region src/text.ts
var TEXT_LIMIT = 4096;
var ARITIES = {
  format_int: 3,
  format_num: 3,
  parse_int: 2,
  parse_num: 1,
  text_length: 1,
  text_slice: 3,
  text_code: 2,
  text_char: 1,
  text_split: 2,
  text_pad: 3,
  list_length: 1,
  text_at: 2,
};
function isTextCall(name) {
  return Object.hasOwn(ARITIES, name);
}
/** Bounded, locale-free conversions. Options use the compiler's ordinary value representation. */
function textCall(name, args, span, work) {
  const fail = (message) => {
    throw new RuntimeFault('M019031', message, span);
  };
  if (!isTextCall(name) || args.length !== ARITIES[name]) fail(`${name}: invalid argument count`);
  let units = 4;
  for (const value of args)
    if (typeof value === 'string') {
      if (value.length > 4096) fail('text exceeds 4096 UTF-16 code units');
      units += value.length;
    }
  work(units, span);
  const text = (index) => {
    const value = args[index];
    return typeof value === 'string' && value.isWellFormed()
      ? value
      : fail('expected well-formed Text');
  };
  const integer = (index) => {
    const value = args[index];
    return typeof value === 'number' && Number.isSafeInteger(value)
      ? value
      : fail('expected safe Int');
  };
  const bounded = (index, maximum) => {
    const value = integer(index);
    return value >= 0 && value <= maximum
      ? value
      : fail(`argument must be between 0 and ${String(maximum)}`);
  };
  const some = (value) => ({
    __some: true,
    value,
  });
  const base = (index) => {
    const value = integer(index);
    return [2, 8, 10, 16].includes(value) ? value : fail('base must be 2, 8, 10 or 16');
  };
  switch (name) {
    case 'format_int': {
      const value = integer(0),
        radix = base(1),
        width = bounded(2, 64);
      const digits = Math.abs(value).toString(radix);
      const sign = value < 0 ? '-' : '';
      return sign + digits.padStart(Math.max(0, width - sign.length), '0');
    }
    case 'format_num': {
      const value = args[0];
      if (typeof value !== 'number' || !Number.isFinite(value) || Math.abs(value) >= 1e21)
        return fail('format_num requires a finite magnitude below 1e21');
      const decimals = bounded(1, 12),
        width = bounded(2, 64);
      return (value === 0 ? 0 : value).toFixed(decimals).padStart(width, ' ');
    }
    case 'parse_int': {
      let input = text(0);
      const radix = base(1);
      if (input.length === 0 || input.length > 256) return null;
      let sign = 1;
      if (input[0] === '-' || input[0] === '+') {
        sign = input[0] === '-' ? -1 : 1;
        input = input.slice(1);
      }
      if (input.length === 0) return null;
      let result = 0;
      for (const character of input.toLowerCase()) {
        const digit = '0123456789abcdef'.indexOf(character);
        if (digit < 0 || digit >= radix) return null;
        result = result * radix + digit;
        if (!Number.isSafeInteger(result)) return null;
      }
      return some(result === 0 ? 0 : sign * result);
    }
    case 'parse_num': {
      const input = text(0);
      if (
        input.length > 256 ||
        input.trim() !== input ||
        !/^[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?$/.test(input)
      )
        return null;
      const value = Number(input);
      return Number.isFinite(value) ? some(value === 0 ? 0 : value) : null;
    }
    case 'text_length':
      return Array.from(text(0)).length;
    case 'text_slice': {
      const chars = Array.from(text(0));
      const start = bounded(1, chars.length),
        end = bounded(2, chars.length);
      if (end < start) return fail('text slice end precedes start');
      return chars.slice(start, end).join('');
    }
    case 'text_at':
    case 'text_code': {
      const value = Array.from(text(0))[integer(1)]?.codePointAt(0);
      if (name === 'text_code') return value === void 0 ? null : some(value);
      return value === void 0 ? fail('text index is outside its length') : value;
    }
    case 'text_char': {
      const value = integer(0);
      return value < 0 || value > 1114111 || (value >= 55296 && value <= 57343)
        ? null
        : some(String.fromCodePoint(value));
    }
    case 'text_split': {
      const input = text(0),
        separator = text(1);
      if (separator.length === 0) return fail('split separator must not be empty');
      const parts = input.split(separator, 257);
      if (parts.length > 256) return fail('split exceeds 256 parts');
      return parts;
    }
    case 'text_pad': {
      const input = text(0),
        width = bounded(1, TEXT_LIMIT),
        padding = text(2);
      if (Array.from(padding).length !== 1) return fail('padding must be one Unicode scalar');
      const resultLength =
        input.length + Math.max(0, width - Array.from(input).length) * padding.length;
      if (resultLength > 4096) return fail('padded text exceeds 4096 UTF-16 code units');
      work(resultLength, span);
      return padding.repeat(Math.max(0, width - Array.from(input).length)) + input;
    }
    case 'list_length': {
      const value = args[0];
      return Array.isArray(value) && value.length <= 65535
        ? value.length
        : fail('expected bounded list or array');
    }
    default:
      return fail('unknown text helper');
  }
}
//#endregion
//#region src/bits.ts
var CALLS = /* @__PURE__ */ new Set([
  'bit_and',
  'bit_or',
  'bit_xor',
  'bit_not',
  'bit_shl',
  'bit_shr',
  'bit_ushr',
]);
function isBitCall(name) {
  return CALLS.has(name);
}
/** Explicit 32-bit two's-complement words; large safe Ints never silently lose high bits. */
function bitCall(name, args, span) {
  const fail = () => {
    throw new RuntimeFault(
      'M019032',
      'bit operations require 32-bit words and shift counts 0-31',
      span,
    );
  };
  if (!isBitCall(name) || args.length !== (name === 'bit_not' ? 1 : 2)) return fail();
  const word = (value) =>
    typeof value === 'number' &&
    Number.isInteger(value) &&
    value >= -2147483648 &&
    value <= 4294967295
      ? value
      : fail();
  const a = word(args[0]);
  if (name === 'bit_not') return ~a;
  const b = word(args[1]);
  if (name.startsWith('bit_s') || name === 'bit_ushr') {
    if (b < 0 || b > 31) return fail();
  }
  switch (name) {
    case 'bit_and':
      return a & b;
    case 'bit_or':
      return a | b;
    case 'bit_xor':
      return a ^ b;
    case 'bit_shl':
      return a << b;
    case 'bit_shr':
      return a >> b;
    case 'bit_ushr':
      return a >>> b;
    default:
      return fail();
  }
}
//#endregion
//#region src/system.ts
var EXECUTION_PHASES = ['idle', 'start', 'update', 'draw', 'raster', 'output', 'menu'];
function isExecutionSnapshot(value, frame, rate, budget) {
  if (
    !isRecord$5(value) ||
    Object.keys(value).length !== 5 ||
    typeof value.booted !== 'boolean' ||
    typeof value.updates !== 'number' ||
    !Number.isSafeInteger(value.updates) ||
    value.updates < 0 ||
    typeof value.phase !== 'string' ||
    !EXECUTION_PHASES.includes(value.phase) ||
    (value.phase === 'raster'
      ? typeof value.rasterLine !== 'number' ||
        !Number.isInteger(value.rasterLine) ||
        value.rasterLine < 0 ||
        value.rasterLine >= HARDWARE.height
      : value.rasterLine !== null) ||
    (value.fault !== null && !isMachineFault(value.fault))
  )
    return false;
  const base = rate === 60 ? frame : Math.ceil(frame / 2);
  const updated =
    (value.phase === 'draw' || value.phase === 'raster' || value.phase === 'output') &&
    (rate === 60 || frame % 2 === 0);
  if (
    value.updates !== base + Number(updated) ||
    (value.phase !== 'idle' && frame === Number.MAX_SAFE_INTEGER) ||
    (!value.booted && (frame !== 0 || (value.phase !== 'idle' && value.phase !== 'start'))) ||
    (value.phase === 'start' && value.booted) ||
    (value.phase === 'update' && rate === 30 && frame % 2 !== 0)
  )
    return false;
  return value.fault !== null || (value.phase === 'idle' && budget.used <= budget.limit);
}
function isMachineFault(value) {
  return (
    isRecord$5(value) &&
    Object.keys(value).length === 2 &&
    typeof value.code === 'number' &&
    Number.isInteger(value.code) &&
    value.code >= 9e3 &&
    value.code <= 9999 &&
    isFaultSpan(value.sourceSpan)
  );
}
function isFaultSpan(value) {
  return (
    isRecord$5(value) &&
    Object.keys(value).length === 2 &&
    typeof value.start === 'number' &&
    Number.isInteger(value.start) &&
    value.start >= 0 &&
    typeof value.end === 'number' &&
    Number.isInteger(value.end) &&
    value.end >= value.start &&
    value.end <= 4294967295
  );
}
function isRecord$5(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
/** Wire encoding of current device-owned state, not a retained register image. */
function systemRegisterByte(state, offset) {
  if (!Number.isInteger(offset) || offset < 0 || offset >= 64) return 0;
  const byte = (value, index) => Math.floor(value / 2 ** (index * 8)) & 255;
  if (offset < 8) return byte(state.frame, offset);
  if (offset < 16) return byte(state.updates, offset - 8);
  if (offset < 24) {
    const encoded = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(8));
    encoded.setFloat64(0, state.seconds, true);
    return encoded.getUint8(offset - 16);
  }
  if (offset < 28) return byte(state.rngState, offset - 24);
  if (offset === 28) return state.updateRate;
  if (offset === 29) return EXECUTION_PHASES.indexOf(state.phase);
  if (offset < 32) return byte(state.rasterLine ?? 65535, offset - 30);
  if (offset < 40) return byte(state.used, offset - 32);
  if (offset < 48) return byte(state.limit, offset - 40);
  if (offset === 48)
    return (
      Number(state.booted) |
      (Number(state.phase !== 'idle' && state.fault === null) << 1) |
      (Number(state.fault !== null) << 2)
    );
  if (offset < 52) return 0;
  if (offset < 54) return byte(state.fault?.code ?? 0, offset - 52);
  if (offset < 56) return 0;
  if (offset < 60) return byte(state.fault?.sourceSpan.start ?? 0, offset - 56);
  return byte(state.fault?.sourceSpan.end ?? 0, offset - 60);
}
//#endregion
//#region src/machine.ts
/** Deterministic callback scheduler and the only API surface visible to generated cartridge code. */
var DeterministicMachine = class {
  budget;
  rng;
  updateRate;
  cartridge;
  hooks;
  debugEnabled;
  menu = new CartridgeMenu();
  currentFrame = 0;
  booted = false;
  input = emptyInputFrame();
  previousInput = emptyInputFrame();
  phase = 'idle';
  rasterLine;
  completedUpdates = 0;
  lastFault = null;
  debugInput;
  constructor(factory, configuration, hooks = {}) {
    if (![30, 60].includes(configuration.updateRate))
      throw new RangeError('update rate must be 30 or 60 Hz');
    this.budget = new WorkBudget(configuration.workUnitsPerFrame);
    this.rng = new DeterministicRng(configuration.seed);
    this.updateRate = configuration.updateRate;
    this.debugEnabled = configuration.debug === true;
    this.hooks = hooks;
    this.cartridge = factory(this);
  }
  get frame() {
    return this.currentFrame;
  }
  get cartridgeTimeSeconds() {
    return this.currentFrame / 60;
  }
  boot() {
    this.assertRunnable();
    if (this.booted) return;
    this.budget.beginFrame();
    this.phase = 'start';
    this.rasterLine = void 0;
    try {
      this.completeDebugCallback(() => this.cartridge.start());
      this.booted = true;
      this.phase = 'idle';
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }
  runFrame(input) {
    if (!isInputFrame(input))
      throw new RuntimeFault('M019008', 'invalid controller input frame', {
        start: 0,
        end: 0,
      });
    this.assertRunnable();
    if (this.debugEnabled)
      for (;;) {
        const step = this.stepDebug(input);
        if (step.report !== void 0) return step.report;
      }
    if (!this.booted) this.boot();
    this.previousInput = this.input;
    this.input = structuredClone(input);
    this.budget.beginFrame();
    try {
      const action = this.input.menuAction;
      if (action !== void 0) {
        this.phase = 'menu';
        this.menu.requireAction(action);
        this.completeDebugCallback(() => this.cartridge.menu?.(action));
      }
      if (this.updateRate === 60 || this.currentFrame % 2 === 0) {
        this.phase = 'update';
        this.completeDebugCallback(() => this.cartridge.update());
        this.completedUpdates += 1;
      }
      this.phase = 'draw';
      this.completeDebugCallback(() => this.cartridge.draw());
      for (let line = 0; line < 144; line += 1) {
        this.phase = 'raster';
        this.rasterLine = line;
        this.completeDebugCallback(() => this.cartridge.raster(line));
      }
      this.rasterLine = void 0;
      this.phase = 'output';
      this.hooks.completeFrame?.();
      const report = {
        frame: this.currentFrame,
        workUnits: this.budget.used,
        attribution: this.budget.attribution(),
      };
      this.currentFrame += 1;
      this.phase = 'idle';
      return report;
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }
  /** Advances a debug build to exactly one statement boundary or one completed frame. */
  stepDebug(input) {
    if (!this.debugEnabled)
      throw new RuntimeFault('M019015', 'statement stepping requires a debug cartridge', {
        start: 0,
        end: 0,
      });
    if (!isInputFrame(input))
      throw new RuntimeFault('M019008', 'invalid controller input frame', {
        start: 0,
        end: 0,
      });
    if (this.lastFault !== null) this.assertRunnable();
    try {
      if (this.phase === 'idle') {
        this.debugInput = structuredClone(input);
        this.budget.beginFrame();
        if (!this.booted) this.phase = 'start';
        else this.beginDebugFrame();
      }
      for (;;) {
        const step = this.stepDebugPhase();
        if (step.event !== void 0) {
          this.probe(step.event.id, step.event.sourceSpan, step.event.locals);
          return { event: structuredClone(step.event) };
        }
        if (!step.done) throw new TypeError('invalid generated debug step');
        const boundary = this.advanceDebugPhase();
        if (boundary === 'booted') return { booted: true };
        if (boundary !== void 0) return { report: boundary };
      }
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }
  beginDebugFrame() {
    const input = this.debugInput ?? emptyInputFrame();
    this.previousInput = this.input;
    this.input = structuredClone(input);
    if (this.input.menuAction !== void 0) {
      this.phase = 'menu';
      this.menu.requireAction(this.input.menuAction);
    } else this.phase = this.updateRate === 60 || this.currentFrame % 2 === 0 ? 'update' : 'draw';
  }
  stepDebugPhase() {
    switch (this.phase) {
      case 'start':
        return this.cartridge.start() ?? { done: true };
      case 'menu': {
        const action = this.input.menuAction;
        if (action === void 0) throw new TypeError('menu phase requires a selected action');
        return this.cartridge.menu?.(action) ?? { done: true };
      }
      case 'update':
        return this.cartridge.update() ?? { done: true };
      case 'draw':
        return this.cartridge.draw() ?? { done: true };
      case 'raster':
        return this.cartridge.raster(this.rasterLine ?? 0) ?? { done: true };
      case 'idle':
      case 'output':
        throw new TypeError(`cannot step machine phase ${this.phase}`);
    }
  }
  advanceDebugPhase() {
    switch (this.phase) {
      case 'start':
        this.booted = true;
        this.phase = 'idle';
        this.debugInput = void 0;
        return 'booted';
      case 'menu':
        this.phase = this.updateRate === 60 || this.currentFrame % 2 === 0 ? 'update' : 'draw';
        return;
      case 'update':
        this.completedUpdates += 1;
        this.phase = 'draw';
        return;
      case 'draw':
        this.phase = 'raster';
        this.rasterLine = 0;
        return;
      case 'raster': {
        if ((this.rasterLine ?? 0) < 143) {
          this.rasterLine = (this.rasterLine ?? 0) + 1;
          return;
        }
        this.rasterLine = void 0;
        this.phase = 'output';
        this.hooks.completeFrame?.();
        const report = {
          frame: this.currentFrame,
          workUnits: this.budget.used,
          attribution: this.budget.attribution(),
        };
        this.currentFrame += 1;
        this.phase = 'idle';
        this.debugInput = void 0;
        return report;
      }
      case 'idle':
      case 'output':
        throw new TypeError(`cannot advance machine phase ${this.phase}`);
    }
  }
  completeDebugCallback(callback) {
    for (;;) {
      const step = callback();
      if (step === void 0 || step.done) return;
      if (step.event !== void 0)
        this.probe(step.event.id, step.event.sourceSpan, step.event.locals);
    }
  }
  snapshot() {
    if (this.phase !== 'idle' && this.lastFault === null)
      throw new TypeError('machine snapshots require a completed frame or fault boundary');
    return structuredClone({
      revision: 3,
      frame: this.currentFrame,
      rngState: this.rng.state,
      cartridge: this.cartridge.snapshot(),
      input: this.input,
      previousInput: this.previousInput,
      menu: this.menu.snapshot(),
      updateRate: this.updateRate,
      budget: this.budget.snapshot(),
      execution: {
        booted: this.booted,
        updates: this.completedUpdates,
        phase: this.phase,
        rasterLine: this.rasterLine ?? null,
        fault: this.lastFault,
      },
    });
  }
  restore(value) {
    if (!isMachineSnapshot(value)) throw new TypeError('invalid MOD-01 machine snapshot');
    const snapshot = value;
    if (snapshot.updateRate !== this.updateRate || snapshot.budget.limit !== this.budget.limit)
      throw new TypeError('machine snapshot does not match the execution configuration');
    const previous = structuredClone(this.cartridge.snapshot());
    try {
      this.cartridge.restore(structuredClone(snapshot.cartridge));
    } catch (error) {
      this.cartridge.restore(previous);
      throw error;
    }
    this.budget.restore(snapshot.budget);
    this.booted = snapshot.execution.booted;
    this.completedUpdates = snapshot.execution.updates;
    this.phase = snapshot.execution.phase;
    this.rasterLine = snapshot.execution.rasterLine ?? void 0;
    this.lastFault = structuredClone(snapshot.execution.fault);
    this.currentFrame = snapshot.frame;
    this.rng.restore(snapshot.rngState);
    this.input = structuredClone(snapshot.input);
    this.previousInput = structuredClone(snapshot.previousInput);
    this.menu.restore(snapshot.menu);
    this.debugInput = void 0;
  }
  inspect() {
    return this.cartridge.inspect();
  }
  inspectMenu() {
    return this.menu.items();
  }
  readInputByte(offset) {
    return inputRegisterByte(this.input, this.previousInput, offset);
  }
  readSystemByte(offset) {
    return systemRegisterByte(
      {
        frame: this.currentFrame,
        updates: this.completedUpdates,
        seconds: this.cartridgeTimeSeconds,
        rngState: this.rng.state,
        updateRate: this.updateRate,
        phase: this.phase,
        rasterLine: this.rasterLine,
        booted: this.booted,
        fault: this.lastFault,
        used: this.budget.used,
        limit: this.budget.limit,
      },
      offset,
    );
  }
  assertRunnable() {
    if (this.lastFault !== null)
      throw new RuntimeFault(
        'M019014',
        'cartridge faulted; restart or restore a healthy checkpoint',
        this.lastFault.sourceSpan,
      );
    if (this.phase !== 'idle')
      throw new RuntimeFault('M019014', 'cartridge is already executing', {
        start: 0,
        end: 0,
      });
    if (this.currentFrame === Number.MAX_SAFE_INTEGER)
      this.fault('M019012', 'display-frame counter is exhausted', {
        start: 0,
        end: 0,
      });
  }
  work(units, sourceSpan) {
    try {
      this.budget.charge(units, sourceSpan);
    } catch (error) {
      this.rememberFault(error, sourceSpan);
      throw error;
    }
  }
  call(name, arguments_, sourceSpan) {
    if (isBitCall(name)) {
      this.work(1, sourceSpan);
      return bitCall(name, arguments_, sourceSpan);
    }
    if (isTextCall(name))
      return textCall(name, arguments_, sourceSpan, (units, span) => {
        this.work(units, span);
      });
    if (isMathCall(name))
      return mathCall(name, arguments_, sourceSpan, (units, span) => {
        this.work(units, span);
      });
    switch (name) {
      case 'menu_item':
        expectArguments$1(name, arguments_, 2, sourceSpan);
        if (this.phase === 'raster')
          return this.fault(
            'M019016',
            'menu registrations are not allowed during raster',
            sourceSpan,
          );
        this.menu.set(arguments_[0], arguments_[1], sourceSpan, (units, span) => {
          this.work(units, span);
        });
        return;
      case 'rng_num':
        expectArguments$1(name, arguments_, 0, sourceSpan);
        return this.rng.nextNum();
      case 'rng_int': {
        expectArguments$1(name, arguments_, 2, sourceSpan);
        const minimum = expectInteger(arguments_[0], sourceSpan);
        const maximum = expectInteger(arguments_[1], sourceSpan);
        try {
          return this.rng.nextInt(minimum, maximum);
        } catch (error) {
          return this.fault(
            'M019007',
            error instanceof Error ? error.message : 'invalid RNG bounds',
            sourceSpan,
          );
        }
      }
      case 'Vec2':
        expectArguments$1(name, arguments_, 2, sourceSpan);
        return {
          x: expectNumber(arguments_[0], sourceSpan),
          y: expectNumber(arguments_[1], sourceSpan),
        };
      case 'Rect':
        expectArguments$1(name, arguments_, 4, sourceSpan);
        return {
          x: expectNumber(arguments_[0], sourceSpan),
          y: expectNumber(arguments_[1], sourceSpan),
          w: expectNumber(arguments_[2], sourceSpan),
          h: expectNumber(arguments_[3], sourceSpan),
        };
      case 'dither':
        expectArguments$1(name, arguments_, 5, sourceSpan);
        return orderedDither(
          expectInteger(arguments_[0], sourceSpan),
          expectInteger(arguments_[1], sourceSpan),
          expectInteger(arguments_[2], sourceSpan),
          expectInteger(arguments_[3], sourceSpan),
          expectInteger(arguments_[4], sourceSpan),
        );
      case 'btn':
      case 'btnp': {
        expectArguments$1(name, arguments_, 2, sourceSpan);
        const port = expectInteger(arguments_[0], sourceSpan);
        const button = arguments_[1];
        if (port < 0 || port >= 4 || !isButton(button))
          return this.fault('M019008', 'invalid controller port or button', sourceSpan);
        const pressed = this.input.controllers[port]?.buttons[button] ?? false;
        if (name === 'btn') return pressed;
        return pressed && !(this.previousInput.controllers[port]?.buttons[button] ?? false);
      }
      case 'pointer_x':
        expectArguments$1(name, arguments_, 0, sourceSpan);
        return this.input.pointer.x;
      case 'pointer_y':
        expectArguments$1(name, arguments_, 0, sourceSpan);
        return this.input.pointer.y;
      case 'pointer_inside':
        expectArguments$1(name, arguments_, 0, sourceSpan);
        return this.input.pointer.inside;
      case 'pointer_primary':
      case 'pointer_secondary': {
        expectArguments$1(name, arguments_, 0, sourceSpan);
        const button = name === 'pointer_primary' ? 'primary' : 'secondary';
        return this.input.pointer[button] && !this.previousInput.pointer[button];
      }
      default: {
        const context = {
          frame: this.currentFrame,
          phase: this.phase,
          ...(this.rasterLine === void 0 ? {} : { rasterLine: this.rasterLine }),
        };
        const result = this.hooks.call?.(name, arguments_, sourceSpan, context);
        if (this.hooks.call === void 0)
          return this.fault('M019004', `console API call '${name}' is unavailable`, sourceSpan);
        return result;
      }
    }
  }
  fault(code, message, sourceSpan) {
    const error = new RuntimeFault(code, message, sourceSpan);
    this.rememberFault(error);
    throw error;
  }
  rememberFault(
    error,
    fallback = {
      start: 0,
      end: 0,
    },
  ) {
    const span = error instanceof RuntimeFault ? error.sourceSpan : fallback;
    this.lastFault = {
      code:
        error instanceof RuntimeFault && /^M019\d{3}$/.test(error.code)
          ? Number(error.code.slice(3))
          : 9199,
      sourceSpan: isFaultSpan(span)
        ? { ...span }
        : {
            start: 0,
            end: 0,
          },
    };
  }
  probe(id, sourceSpan, locals) {
    this.hooks.probe?.(id, sourceSpan, locals);
  }
  enter(name, sourceSpan) {
    this.hooks.enter?.(name, sourceSpan);
  }
  leave() {
    this.hooks.leave?.();
  }
};
function isMachineSnapshot(value) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const candidate = value;
  if (!(
    Number.isSafeInteger(candidate.frame) &&
    typeof candidate.frame === 'number' &&
    candidate.frame >= 0 &&
    Number.isSafeInteger(candidate.rngState) &&
    isInputFrame(candidate.input) &&
    isInputFrame(candidate.previousInput) &&
    typeof candidate.cartridge === 'object' &&
    candidate.cartridge !== null
  ))
    return false;
  return (
    candidate.revision === 3 &&
    Object.keys(candidate).length === 10 &&
    isMenuSnapshot(candidate.menu) &&
    typeof candidate.rngState === 'number' &&
    candidate.rngState > 0 &&
    candidate.rngState <= 4294967295 &&
    (candidate.updateRate === 30 || candidate.updateRate === 60) &&
    isWorkBudgetSnapshot(candidate.budget) &&
    isExecutionSnapshot(
      candidate.execution,
      candidate.frame,
      candidate.updateRate,
      candidate.budget,
    )
  );
}
function expectArguments$1(name, arguments_, count, sourceSpan) {
  if (arguments_.length !== count)
    throw new RuntimeFault(
      'M019009',
      `${name} expected ${String(count)} arguments, received ${String(arguments_.length)}`,
      sourceSpan,
    );
}
function expectNumber(value, sourceSpan) {
  if (typeof value !== 'number' || !Number.isFinite(value))
    throw new RuntimeFault('M019009', 'expected a finite number', sourceSpan);
  return value;
}
function expectInteger(value, sourceSpan) {
  const number = expectNumber(value, sourceSpan);
  if (!Number.isSafeInteger(number))
    throw new RuntimeFault('M019009', 'expected a safe integer', sourceSpan);
  return number;
}
//#endregion
//#region src/audio.ts
function isSynthSnapshot(value) {
  if (
    !audioRecord(value) ||
    Object.keys(value).length !== 5 ||
    value.revision !== 1 ||
    !audioCounter(value.frame) ||
    !audioCounter(value.nextSequence) ||
    value.nextSequence < 1 ||
    !denseAudioArray(value.voices) ||
    value.voices.length !== HARDWARE.audioVoices
  )
    return false;
  const nextSequence = value.nextSequence;
  if (!value.voices.every((voice, slot) => isVoice(voice, slot, nextSequence))) return false;
  return (
    value.tracker === null ||
    (audioRecord(value.tracker) &&
      Object.keys(value.tracker).length === 4 &&
      typeof value.tracker.music === 'string' &&
      value.tracker.music.length > 0 &&
      value.tracker.music.length <= HARDWARE.cartridgeCapacityBytes &&
      audioCounter(value.tracker.orderIndex) &&
      audioCounter(value.tracker.row) &&
      audioCounter(value.tracker.frameInRow))
  );
}
function isVoice(voice, slot, nextSequence) {
  return (
    audioRecord(voice) &&
    Object.keys(voice).length === 9 &&
    typeof voice.active === 'boolean' &&
    voice.slot === slot &&
    typeof voice.sound === 'string' &&
    voice.sound.length <= HARDWARE.cartridgeCapacityBytes &&
    audioNumber(voice.note, 0, 127) &&
    audioNumber(voice.volumeScale, 0, 1) &&
    audioCounter(voice.ageFrames) &&
    audioNumber(voice.phase, 0, 1) &&
    voice.phase < 1 &&
    audioCounter(voice.noiseState) &&
    voice.noiseState > 0 &&
    voice.noiseState <= 4294967295 &&
    audioCounter(voice.sequence) &&
    voice.sequence < nextSequence
  );
}
function audioCounter(value) {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
function audioNumber(value, minimum, maximum) {
  return (
    typeof value === 'number' && Number.isFinite(value) && value >= minimum && value <= maximum
  );
}
function audioRecord(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
/** Validated oscillator and tracker data. Arbitrary PCM samples are intentionally unrepresentable. */
var AudioAssetStore = class {
  entries = /* @__PURE__ */ new Map();
  ordered;
  ids = /* @__PURE__ */ new Map();
  descriptors;
  constructor(assets = []) {
    for (const asset of assets) {
      validateAudioAsset(asset);
      if (this.entries.has(asset.name))
        throw new TypeError(`duplicate audio asset '${asset.name}'`);
      this.entries.set(asset.name, structuredClone(asset));
    }
    for (const asset of this.entries.values()) {
      if (asset.kind !== 'music') continue;
      for (const pattern of Object.values(asset.patterns))
        for (const row of pattern.rows)
          for (const cell of row)
            if (cell !== null && this.entries.get(cell.sound)?.kind !== 'sound')
              throw new TypeError(`music '${asset.name}' references missing sound '${cell.sound}'`);
    }
    this.ordered = [...this.entries.values()].sort((left, right) =>
      left.name < right.name ? -1 : left.name > right.name ? 1 : 0,
    );
    this.descriptors = new Uint8Array(this.ordered.length * MEMORY.audioAssetStride);
    const view = new DataView(this.descriptors.buffer);
    for (const [id, asset] of this.ordered.entries()) {
      this.ids.set(asset.name, id);
      const fields =
        asset.kind === 'sound'
          ? [
              1,
              ['pulse', 'triangle', 'saw', 'noise', 'wavetable'].indexOf(asset.waveform) + 1,
              asset.note,
              asset.durationFrames,
              asset.envelope.releaseFrames,
              0,
              0,
              0,
            ]
          : [2, 0, 0, 0, 0, asset.framesPerRow, asset.order.length, Number(asset.loop)];
      for (const [field, value] of fields.entries())
        view.setUint32(id * MEMORY.audioAssetStride + field * 4, value, true);
    }
  }
  get(name) {
    return this.entries.get(name);
  }
  id(name) {
    return this.ids.get(name) ?? -1;
  }
  byId(id) {
    return this.ordered[id];
  }
  get count() {
    return this.ordered.length;
  }
  memoryRegions() {
    return this.descriptors.length === 0
      ? []
      : [
          {
            name: 'audio asset descriptors',
            address: MEMORY.audioAssets,
            bytes: this.descriptors,
            writable: false,
          },
        ];
  }
};
/** Eight-voice deterministic synthesizer and eight-channel order/pattern tracker. */
var Synthesizer = class {
  assets;
  voices;
  frame = 0;
  nextSequence = 1;
  tracker = null;
  constructor(assets = new AudioAssetStore()) {
    this.assets = assets;
    this.voices = Array.from({ length: HARDWARE.audioVoices }, (_, slot) => emptyVoice(slot));
  }
  executeFrame(commands) {
    for (const command of commands) this.executeCommand(command);
    return this.finishFrame();
  }
  finishFrame() {
    if (this.frame === Number.MAX_SAFE_INTEGER)
      throw new RuntimeFault('M019012', 'audio-frame counter is exhausted', {
        start: 0,
        end: 0,
      });
    this.advanceTracker();
    const sampleCount = HARDWARE.audioSampleRate / HARDWARE.frameRate;
    const left = new Float32Array(sampleCount);
    const right = new Float32Array(sampleCount);
    for (const voice of this.voices) if (voice.active) this.mixVoice(voice, left, right);
    for (const voice of this.voices)
      if (voice.active) {
        voice.ageFrames += 1;
        const sound = this.assets.get(voice.sound);
        if (
          sound?.kind !== 'sound' ||
          voice.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames
        )
          voice.active = false;
      }
    const inspection = this.inspectTracker();
    this.frame += 1;
    return {
      left,
      right,
      activeVoices: this.voices.filter((voice) => voice.active).length,
      tracker: inspection,
    };
  }
  snapshot() {
    return structuredClone({
      revision: 1,
      frame: this.frame,
      nextSequence: this.nextSequence,
      voices: this.voices,
      tracker: this.tracker,
    });
  }
  restore(snapshot) {
    if (!isSynthSnapshot(snapshot)) throw new TypeError('invalid MOD-01 synthesizer snapshot');
    for (const voice of snapshot.voices) {
      const sound = this.assets.get(voice.sound);
      if ((voice.active || voice.sound !== '') && sound?.kind !== 'sound')
        throw new TypeError('snapshot references a missing sound');
      if (
        voice.active &&
        sound?.kind === 'sound' &&
        voice.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames
      )
        throw new TypeError('snapshot contains a voice beyond its sound lifetime');
    }
    if (snapshot.tracker !== null) {
      const tracker = snapshot.tracker;
      const music = this.assets.get(tracker.music);
      if (
        music?.kind !== 'music' ||
        !validTrackerPosition(music, tracker.orderIndex, tracker.row, tracker.frameInRow)
      )
        throw new TypeError('snapshot references an invalid tracker position');
    }
    this.frame = snapshot.frame;
    this.nextSequence = snapshot.nextSequence;
    for (let index = 0; index < this.voices.length; index += 1)
      Object.assign(this.voices[index], structuredClone(snapshot.voices[index]));
    this.tracker = structuredClone(snapshot.tracker);
  }
  inspectVoices() {
    return structuredClone(this.voices);
  }
  /** Native instrument audition uses the tracker's real note override and voice allocator. */
  audition(soundName, note, volumeScale = 1) {
    if (!audioInteger(note, 0, 127) || !audioNumber(volumeScale, 0, 1))
      throw new RangeError('audition requires a note 0-127 and volume 0-1');
    this.trigger(soundName, note, volumeScale);
  }
  inspectTracker() {
    return this.tracker === null ? null : { ...this.tracker };
  }
  /** Start or seek the real sequencer without resetting voices or the synth clock. */
  playMusic(name, orderIndex = 0, row = 0) {
    const music = this.assets.get(name);
    if (music?.kind !== 'music') throw new TypeError(`missing Music asset '${name}'`);
    if (!validTrackerPosition(music, orderIndex, row, 0))
      throw new RangeError('invalid tracker order or row');
    this.tracker = {
      music: name,
      orderIndex,
      row,
      frameInRow: 0,
    };
  }
  memoryRegions() {
    return [
      ...this.assets.memoryRegions(),
      {
        name: 'synth status',
        address: MEMORY.audio,
        length: 32,
        writable: false,
        readByte: (offset) => this.statusByte(offset),
      },
      {
        name: 'tracker controls',
        address: MEMORY.audioTracker,
        length: 16,
        writable: true,
        readByte: (offset) => this.trackerByte(offset),
        prepareWrite: (offset, bytes) => this.prepareTrackerWrite(offset, bytes),
      },
      ...this.voices.flatMap((voice) => [
        {
          name: `voice ${String(voice.slot)} controls`,
          address: MEMORY.audioVoices + voice.slot * MEMORY.voiceStride,
          length: 48,
          writable: true,
          readByte: (offset) => this.voiceByte(voice, offset),
          prepareWrite: (offset, bytes) => this.prepareVoiceWrite(voice, offset, bytes),
        },
        {
          name: `voice ${String(voice.slot)} allocation`,
          address: MEMORY.audioVoices + voice.slot * MEMORY.voiceStride + 48,
          length: 16,
          writable: false,
          readByte: (offset) => (offset < 8 ? integerByte(voice.sequence, offset) : 0),
        },
      ]),
    ];
  }
  statusByte(offset) {
    if (offset < 8) return integerByte(this.frame, offset);
    if (offset < 16) return integerByte(this.nextSequence, offset - 8);
    if (offset === 16) return this.voices.filter((voice) => voice.active).length;
    if (offset === 17) return HARDWARE.audioVoices;
    if (offset === 18) return HARDWARE.trackerChannels;
    if (offset === 19) return Number(this.tracker !== null);
    if (offset < 24) return integerByte(HARDWARE.audioSampleRate, offset - 20);
    if (offset < 28) return integerByte(this.assets.count, offset - 24);
    return integerByte(MEMORY.audioAssets, offset - 28);
  }
  trackerByte(offset) {
    if (this.tracker === null) return 0;
    return integerByte(
      [
        this.assets.id(this.tracker.music) + 1,
        this.tracker.orderIndex,
        this.tracker.row,
        this.tracker.frameInRow,
      ][Math.floor(offset / 4)] ?? 0,
      offset % 4,
    );
  }
  prepareTrackerWrite(offset, bytes) {
    const staged = Uint8Array.from({ length: 16 }, (_, index) => this.trackerByte(index));
    staged.set(bytes, offset);
    const view = new DataView(staged.buffer);
    const id = view.getUint32(0, true);
    if (id === 0)
      return () => {
        this.tracker = null;
      };
    const music = this.assets.byId(id - 1);
    if (music?.kind !== 'music') return void 0;
    const next = {
      music: music.name,
      orderIndex: view.getUint32(4, true),
      row: view.getUint32(8, true),
      frameInRow: view.getUint32(12, true),
    };
    if (!validTrackerPosition(music, next.orderIndex, next.row, next.frameInRow)) return void 0;
    return () => {
      this.tracker = next;
    };
  }
  voiceByte(voice, offset) {
    if (offset === 0) return Number(voice.active);
    if (offset < 4) return 0;
    if (offset < 8) return integerByte(this.assets.id(voice.sound) + 1, offset - 4);
    if (offset < 16) return floatByte(voice.note, offset - 8);
    if (offset < 24) return floatByte(voice.volumeScale, offset - 16);
    if (offset < 32) return integerByte(voice.ageFrames, offset - 24);
    if (offset < 40) return floatByte(voice.phase, offset - 32);
    if (offset < 44) return integerByte(voice.noiseState, offset - 40);
    return 0;
  }
  prepareVoiceWrite(voice, offset, bytes) {
    const staged = Uint8Array.from({ length: 48 }, (_, index) => this.voiceByte(voice, index));
    staged.set(bytes, offset);
    if (staged[0] !== 0 && staged[0] !== 1) return void 0;
    if ([1, 2, 3, 44, 45, 46, 47].some((index) => staged[index] !== 0)) return void 0;
    const view = new DataView(staged.buffer);
    const id = view.getUint32(4, true);
    const sound = this.assets.byId(id - 1);
    if (id !== 0 && sound?.kind !== 'sound') return void 0;
    const next = {
      ...voice,
      active: staged[0] === 1,
      sound: sound?.name ?? '',
      note: view.getFloat64(8, true),
      volumeScale: view.getFloat64(16, true),
      ageFrames: Number(view.getBigUint64(24, true)),
      phase: view.getFloat64(32, true),
      noiseState: view.getUint32(40, true),
    };
    if (
      !isVoice(next, voice.slot, this.nextSequence) ||
      (next.active &&
        (sound?.kind !== 'sound' ||
          next.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames))
    )
      return void 0;
    return () => {
      Object.assign(voice, next);
    };
  }
  executeCommand(command) {
    switch (command.name) {
      case 'sfx': {
        const [handle] = expectArguments(command, 1);
        this.trigger(readAssetName(handle, 'Sound'), void 0, 1, command.sourceSpan);
        return;
      }
      case 'music': {
        const [handle] = expectArguments(command, 1);
        const name = readAssetName(handle, 'Music');
        this.playMusic(name);
        return;
      }
      case 'music_stop':
        expectArguments(command, 0);
        this.tracker = null;
        return;
      default:
        throw new TypeError(`unknown audio command '${command.name}'`);
    }
  }
  trigger(
    soundName,
    noteOverride,
    volumeScale = 1,
    sourceSpan = {
      start: 0,
      end: 0,
    },
  ) {
    const sound = this.assets.get(soundName);
    if (sound?.kind !== 'sound') throw new TypeError(`missing Sound asset '${soundName}'`);
    if (this.nextSequence === Number.MAX_SAFE_INTEGER)
      throw new RuntimeFault('M019012', 'voice-allocation counter is exhausted', sourceSpan);
    const voice =
      this.voices.find((candidate) => !candidate.active) ??
      this.voices.reduce((oldest, candidate) =>
        candidate.sequence < oldest.sequence ? candidate : oldest,
      );
    Object.assign(voice, {
      active: true,
      sound: soundName,
      note: noteOverride ?? sound.note,
      volumeScale,
      ageFrames: 0,
      phase: 0,
      noiseState: nonZeroNoiseSeed(this.frame, voice.slot, this.nextSequence),
      sequence: this.nextSequence,
    });
    this.nextSequence += 1;
  }
  advanceTracker() {
    const tracker = this.tracker;
    if (tracker === null) return;
    const music = this.assets.get(tracker.music);
    if (music?.kind !== 'music') {
      this.tracker = null;
      return;
    }
    if (tracker.frameInRow === 0) {
      const patternName = music.order[tracker.orderIndex];
      (patternName === void 0 ? void 0 : music.patterns[patternName])?.rows[tracker.row]?.forEach(
        (cell) => {
          if (cell !== null) this.trigger(cell.sound, cell.note, cell.volume ?? 1);
        },
      );
    }
    tracker.frameInRow += 1;
    if (tracker.frameInRow < music.framesPerRow) return;
    tracker.frameInRow = 0;
    tracker.row += 1;
    const patternName = music.order[tracker.orderIndex];
    const pattern = patternName === void 0 ? void 0 : music.patterns[patternName];
    if (pattern !== void 0 && tracker.row < pattern.rows.length) return;
    tracker.row = 0;
    tracker.orderIndex += 1;
    if (tracker.orderIndex >= music.order.length) {
      if (music.loop) tracker.orderIndex = 0;
      else this.tracker = null;
    }
  }
  mixVoice(voice, left, right) {
    const sound = this.assets.get(voice.sound);
    if (sound?.kind !== 'sound') {
      voice.active = false;
      return;
    }
    const frameEnvelope = envelopeAt(sound, voice.ageFrames);
    const pan = Math.max(-1, Math.min(1, sound.pan));
    const leftGain = (1 - pan) / 2;
    const rightGain = (1 + pan) / 2;
    const phaseStep =
      noteFrequency(
        voice.note +
          sound.pitch.slideSemitonesPerFrame * voice.ageFrames +
          triangleLfo(voice.ageFrames, sound.pitch.vibratoPeriodFrames) *
            sound.pitch.vibratoDepthSemitones,
      ) / HARDWARE.audioSampleRate;
    for (let index = 0; index < left.length; index += 1) {
      const sample =
        oscillatorValue(sound, voice) * frameEnvelope * sound.volume * voice.volumeScale * 0.24;
      left[index] = clampSample((left[index] ?? 0) + sample * leftGain);
      right[index] = clampSample((right[index] ?? 0) + sample * rightGain);
      voice.phase = (voice.phase + phaseStep) % 1;
    }
  }
};
function validTrackerPosition(music, orderIndex, row, frameInRow) {
  if (
    !audioCounter(orderIndex) ||
    !audioCounter(row) ||
    !audioCounter(frameInRow) ||
    frameInRow >= music.framesPerRow
  )
    return false;
  const name = music.order[orderIndex];
  return name !== void 0 && music.patterns[name]?.rows[row] !== void 0;
}
function emptyVoice(slot) {
  return {
    active: false,
    slot,
    sound: '',
    note: 0,
    volumeScale: 1,
    ageFrames: 0,
    phase: 0,
    noiseState: 1,
    sequence: 0,
  };
}
function integerByte(value, offset) {
  return Math.floor(value / 2 ** (offset * 8)) & 255;
}
function floatByte(value, offset) {
  const view = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(8));
  view.setFloat64(0, value, true);
  return view.getUint8(offset);
}
function validateAudioAsset(asset) {
  if (!audioRecord(asset) || typeof asset.name !== 'string' || asset.name.length === 0)
    throw new TypeError('audio assets require a nonempty name');
  if (asset.kind === 'sound') {
    if (
      typeof asset.waveform !== 'string' ||
      !['pulse', 'triangle', 'saw', 'noise', 'wavetable'].includes(asset.waveform) ||
      !audioInteger(asset.note, 0, 127) ||
      !audioInteger(asset.durationFrames, 1, 3600) ||
      !audioNumber(asset.volume, 0, 1) ||
      !audioNumber(asset.pan, -1, 1) ||
      !validEnvelope(asset.envelope) ||
      !validPitch(asset.pitch, asset.durationFrames + asset.envelope.releaseFrames)
    )
      throw new RangeError(`sound '${asset.name}' is outside MOD-01 limits`);
    if (
      asset.waveform === 'pulse' &&
      (!audioNumber(asset.duty, 0, 1) || asset.duty === 0 || asset.duty === 1)
    )
      throw new RangeError(`pulse sound '${asset.name}' requires a duty between zero and one`);
    if (
      asset.waveform === 'wavetable' &&
      (!denseAudioArray(asset.wavetable) ||
        asset.wavetable.length < 4 ||
        asset.wavetable.length > 32 ||
        asset.wavetable.some((sample) => !audioNumber(sample, -1, 1)))
    )
      throw new RangeError(`wavetable sound '${asset.name}' requires 4-32 normalized entries`);
    return;
  }
  if (
    asset.kind !== 'music' ||
    typeof asset.loop !== 'boolean' ||
    !audioInteger(asset.framesPerRow, 1, 240) ||
    !denseAudioArray(asset.order) ||
    asset.order.length === 0 ||
    !audioRecord(asset.patterns) ||
    asset.order.some((name) => typeof name !== 'string' || !Object.hasOwn(asset.patterns, name))
  )
    throw new RangeError(`music '${asset.name}' has an invalid order list or tempo`);
  for (const pattern of Object.values(asset.patterns)) {
    if (
      !audioRecord(pattern) ||
      !denseAudioArray(pattern.rows) ||
      pattern.rows.length === 0 ||
      pattern.rows.length > 256
    )
      throw new RangeError(`music '${asset.name}' has an invalid pattern length`);
    for (const row of pattern.rows) {
      if (!denseAudioArray(row) || row.length !== HARDWARE.trackerChannels)
        throw new RangeError(`music '${asset.name}' patterns must have eight channels`);
      for (const cell of row)
        if (
          cell !== null &&
          (!audioRecord(cell) ||
            !audioInteger(cell.note, 0, 127) ||
            typeof cell.sound !== 'string' ||
            cell.sound.length === 0 ||
            (cell.volume !== void 0 && !audioNumber(cell.volume, 0, 1)))
        )
          throw new RangeError(`music '${asset.name}' contains an invalid note`);
    }
  }
}
function denseAudioArray(value) {
  return (
    Array.isArray(value) &&
    Object.keys(value).length === value.length &&
    Array.from(value.keys()).every((index) => Object.hasOwn(value, index))
  );
}
function audioInteger(value, minimum, maximum) {
  return audioNumber(value, minimum, maximum) && Number.isSafeInteger(value);
}
function validEnvelope(envelope) {
  return (
    audioRecord(envelope) &&
    [envelope.attackFrames, envelope.decayFrames, envelope.releaseFrames].every((value) =>
      audioInteger(value, 0, 3600),
    ) &&
    audioNumber(envelope.sustainLevel, 0, 1)
  );
}
function validPitch(pitch, lifetime) {
  if (
    !audioRecord(pitch) ||
    typeof pitch.slideSemitonesPerFrame !== 'number' ||
    !Number.isFinite(pitch.slideSemitonesPerFrame) ||
    typeof pitch.vibratoDepthSemitones !== 'number' ||
    !Number.isFinite(pitch.vibratoDepthSemitones) ||
    !audioInteger(pitch.vibratoPeriodFrames, 0, 3600)
  )
    return false;
  const excursion =
    Math.abs(pitch.slideSemitonesPerFrame) * lifetime + Math.abs(pitch.vibratoDepthSemitones);
  return Number.isFinite(excursion) && Number.isFinite(noteFrequency(127 + excursion));
}
function oscillatorValue(sound, voice) {
  switch (sound.waveform) {
    case 'pulse':
      return voice.phase < (sound.duty ?? 0.5) ? 1 : -1;
    case 'triangle':
      return 1 - 4 * Math.abs(voice.phase - 0.5);
    case 'saw':
      return voice.phase * 2 - 1;
    case 'noise':
      voice.noiseState = xorshift(voice.noiseState);
      return (voice.noiseState & 65535) / 32767 - 1;
    case 'wavetable': {
      const table = sound.wavetable ?? [0];
      return table[Math.floor(voice.phase * table.length) % table.length] ?? 0;
    }
  }
}
function envelopeAt(sound, age) {
  const envelope = sound.envelope;
  if (envelope.attackFrames > 0 && age < envelope.attackFrames) return age / envelope.attackFrames;
  const decayAge = age - envelope.attackFrames;
  if (envelope.decayFrames > 0 && decayAge < envelope.decayFrames)
    return 1 - (1 - envelope.sustainLevel) * (decayAge / envelope.decayFrames);
  if (age < sound.durationFrames) return envelope.sustainLevel;
  if (envelope.releaseFrames === 0) return 0;
  return (
    envelope.sustainLevel * Math.max(0, 1 - (age - sound.durationFrames) / envelope.releaseFrames)
  );
}
function triangleLfo(frame, period) {
  if (period === 0) return 0;
  const phase = (frame % period) / period;
  return 1 - 4 * Math.abs(phase - 0.5);
}
function noteFrequency(note) {
  return 440 * 2 ** ((note - 69) / 12);
}
function xorshift(state) {
  let value = state >>> 0;
  value ^= value << 13;
  value ^= value >>> 17;
  value ^= value << 5;
  return value >>> 0;
}
function nonZeroNoiseSeed(frame, slot, sequence) {
  return ((frame + 1) * 2654435761 + (slot + 1) * 2246822507 + sequence) >>> 0 || 1;
}
function clampSample(sample) {
  return Math.max(-1, Math.min(1, sample));
}
function expectArguments(command, count) {
  if (command.arguments.length !== count)
    throw new TypeError(
      `${command.name} expected ${String(count)} arguments, received ${String(command.arguments.length)}`,
    );
  return [...command.arguments];
}
function readAssetName(value, kind) {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('name' in value) ||
    typeof value.name !== 'string' ||
    !('kind' in value) ||
    value.kind !== kind
  )
    throw new TypeError(`expected a ${kind} asset handle`);
  return value.name;
}
//#endregion
//#region src/asset-codec.ts
/** Bounded, data-only source bank sent to the console core for authoritative decoding. */
function isRuntimeAssetSource(value) {
  if (
    !isRecord$4(value) ||
    !isRecord$4(value.declarations) ||
    !isRecord$4(value.files) ||
    Array.isArray(value.declarations) ||
    Array.isArray(value.files) ||
    Object.keys(value).some((key) => !['declarations', 'files', 'displayPath'].includes(key)) ||
    Object.keys(value.declarations).length > 4096 ||
    Object.keys(value.files).length > 4096 ||
    (value.displayPath !== void 0 &&
      value.displayPath !== null &&
      !canonicalAssetPath(value.displayPath))
  )
    return false;
  let bytes = typeof value.displayPath === 'string' ? value.displayPath.length : 0;
  for (const [path, data] of Object.entries(value.files)) {
    if (!canonicalAssetPath(path) || !(data instanceof Uint8Array)) return false;
    bytes += path.length + data.byteLength;
    if (bytes > 16777216) return false;
  }
  for (const [name, declaration] of Object.entries(value.declarations)) {
    if (
      !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name) ||
      !isRecord$4(declaration) ||
      Object.keys(declaration).length !== 2 ||
      !canonicalAssetPath(declaration.path) ||
      typeof declaration.kind !== 'string' ||
      !['sprite', 'animation', 'tile_set', 'map', 'font', 'sound', 'music'].includes(
        declaration.kind,
      )
    )
      return false;
    bytes += name.length + declaration.path.length + declaration.kind.length;
    if (bytes > 16777216) return false;
  }
  return true;
}
function canonicalAssetPath(value) {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    value.length <= 1024 &&
    value
      .split('/')
      .every((part) => /^[A-Za-z0-9_.-]+$/.test(part) && part !== '.' && part !== '..')
  );
}
/** Decodes documented JSON asset files into validated hardware stores and query views. */
function decodeRuntimeAssets(declarations, files, displayPath) {
  const visual = [];
  const audio = [];
  for (const [name, declaration] of Object.entries(declarations).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    const bytes = files[declaration.path];
    if (bytes === void 0) throw new TypeError(`asset '${name}' is missing '${declaration.path}'`);
    const value = JSON.parse(new TextDecoder().decode(bytes));
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
    }
  }
  const visualStore = new VisualAssetStore(visual);
  new AudioAssetStore(audio);
  const tileSets = new Map(
    visual.filter((asset) => asset.kind === 'tile_set').map((asset) => [asset.name, asset]),
  );
  const maps = visual
    .filter((asset) => asset.kind === 'map')
    .map((asset) => ({
      name: asset.name,
      layers: asset.layers.map((layer) => {
        const tileSet = tileSets.get(layer.tileSet);
        if (tileSet === void 0)
          throw new TypeError(`map '${asset.name}' references missing tile set '${layer.tileSet}'`);
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
  if (visualBytes > HARDWARE.visualCapacityBytes)
    throw new RangeError('visual assets exceed the 1 MiB shared capacity');
  return {
    visual,
    audio,
    maps,
    ...(display === void 0 ? {} : { display }),
    visualBytes,
  };
}
function decodeSprite(name, value, animation) {
  if (
    !isRecord$4(value) ||
    value.revision !== 2 ||
    value.kind !== 'sprite' ||
    !boundedInteger(value.width, 1, HARDWARE.spriteMaximumAxis) ||
    !boundedInteger(value.height, 1, HARDWARE.spriteMaximumAxis) ||
    !Array.isArray(value.frames) ||
    value.frames.length === 0 ||
    value.frames.length > 256
  )
    throw new TypeError(`sprite asset '${name}' is invalid`);
  const pixelCount = value.width * value.height;
  const frames = value.frames.map((frame) => {
    if (!isNumberArray(frame, pixelCount, 0, 255))
      throw new TypeError(`sprite asset '${name}' has invalid indexed pixels`);
    return {
      kind: 'sprite',
      name,
      width: value.width,
      height: value.height,
      pixels: Uint8Array.from(frame),
    };
  });
  const first = frames[0];
  if (first === void 0) throw new TypeError(`sprite asset '${name}' requires a frame`);
  return animation
    ? {
        kind: 'animation',
        name,
        frames,
      }
    : {
        ...first,
        name,
      };
}
function decodeTileSet(name, value) {
  if (
    !isRecord$4(value) ||
    value.revision !== 2 ||
    value.kind !== 'tile_set' ||
    !Array.isArray(value.tiles) ||
    value.tiles.length === 0 ||
    value.tiles.length > 4096 ||
    !isNumberArray(value.flags, value.tiles.length, 0, 255)
  )
    throw new TypeError(`tile-set asset '${name}' is invalid`);
  return {
    kind: 'tile_set',
    name,
    tiles: value.tiles.map((pixels, index) => {
      if (!isNumberArray(pixels, 64, 0, 255))
        throw new TypeError(`tile ${String(index)} in '${name}' has invalid indexed pixels`);
      return {
        kind: 'sprite',
        name: `${name}:${String(index)}`,
        width: 8,
        height: 8,
        pixels: Uint8Array.from(pixels),
      };
    }),
    flags: Uint8Array.from(value.flags),
  };
}
function decodeMap(name, value) {
  if (
    !isRecord$4(value) ||
    value.revision !== 1 ||
    value.kind !== 'map' ||
    !Array.isArray(value.layers) ||
    value.layers.length === 0 ||
    value.layers.length > 8
  )
    throw new TypeError(`map asset '${name}' is invalid`);
  return {
    kind: 'map',
    name,
    layers: value.layers.map((layer) => {
      if (
        !isRecord$4(layer) ||
        !boundedInteger(layer.width, 1, 256) ||
        !boundedInteger(layer.height, 1, 256) ||
        typeof layer.tileSet !== 'string' ||
        !isNumberArray(layer.cells, layer.width * layer.height, 0, 65535)
      )
        throw new TypeError(`map asset '${name}' has an invalid layer`);
      return {
        width: layer.width,
        height: layer.height,
        cells: Uint16Array.from(layer.cells),
        tileSet: layer.tileSet,
      };
    }),
  };
}
function decodeFont(name, value) {
  if (
    !isRecord$4(value) ||
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
  )
    throw new TypeError(`font asset '${name}' is invalid`);
  const glyphs = /* @__PURE__ */ new Map();
  let previous = -1;
  for (const glyph of value.glyphs) {
    if (
      !isRecord$4(glyph) ||
      !boundedInteger(glyph.code, 0, 255) ||
      glyph.code <= previous ||
      !isNumberArray(glyph.pixels, value.glyphWidth * value.glyphHeight, 0, 1)
    )
      throw new TypeError(`font asset '${name}' has an invalid glyph map`);
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
function decodeSound(name, value) {
  if (!isRecord$4(value) || value.revision !== 1 || value.kind !== 'sound')
    throw new TypeError(`sound asset '${name}' is invalid`);
  const sound = {
    ...value,
    name,
  };
  new AudioAssetStore([sound]);
  return sound;
}
function decodeMusic(name, value) {
  if (!isRecord$4(value) || value.revision !== 1 || value.kind !== 'music')
    throw new TypeError(`music asset '${name}' is invalid`);
  return {
    ...value,
    name,
  };
}
function decodeDisplay(path, files) {
  if (path === void 0 || path === null) return void 0;
  const bytes = files[path];
  if (bytes === void 0) throw new TypeError(`display configuration is missing '${path}'`);
  const value = JSON.parse(new TextDecoder().decode(bytes));
  if (
    !isRecord$4(value) ||
    value.revision !== 2 ||
    value.kind !== 'display' ||
    !isNumberArray(value.remap, HARDWARE.rasterPaletteSize, 0, 255) ||
    !Array.isArray(value.raster) ||
    value.raster.length > HARDWARE.height
  )
    throw new TypeError('display configuration is invalid');
  let previousLine = -1;
  const raster = value.raster.map((state) => {
    if (
      !isRecord$4(state) ||
      !boundedInteger(state.line, 0, HARDWARE.height - 1) ||
      state.line <= previousLine ||
      !boundedInteger(state.scrollX, -32768, 32767) ||
      !boundedInteger(state.scrollY, -32768, 32767) ||
      !isNumberArray(state.remap, HARDWARE.rasterPaletteSize, 0, 255)
    )
      throw new TypeError('display configuration has invalid raster state');
    previousLine = state.line;
    return {
      line: state.line,
      scrollX: state.scrollX,
      scrollY: state.scrollY,
      remap: Uint8Array.from(state.remap),
    };
  });
  return {
    remap: Uint8Array.from(value.remap),
    raster,
  };
}
function displayBytes(display) {
  return display === void 0
    ? 0
    : HARDWARE.rasterPaletteSize + display.raster.length * (HARDWARE.rasterPaletteSize + 6);
}
function isNumberArray(value, length, minimum, maximum) {
  return (
    Array.isArray(value) &&
    value.length === length &&
    value.every((entry) => Number.isSafeInteger(entry) && entry >= minimum && entry <= maximum)
  );
}
function boundedInteger(value, minimum, maximum) {
  return (
    Number.isSafeInteger(value) && typeof value === 'number' && value >= minimum && value <= maximum
  );
}
function isRecord$4(value) {
  return typeof value === 'object' && value !== null;
}
//#endregion
//#region src/map-query.ts
/** Read-only map view exposed to cartridge query calls. */
var MapQueryStore = class {
  maps = /* @__PURE__ */ new Map();
  constructor(assets = []) {
    for (const asset of assets) {
      if (asset.name.length === 0 || this.maps.has(asset.name) || asset.layers.length === 0)
        throw new TypeError('map query catalog contains an invalid or duplicate map');
      for (const layer of asset.layers)
        if (
          !Number.isSafeInteger(layer.width) ||
          !Number.isSafeInteger(layer.height) ||
          layer.width <= 0 ||
          layer.height <= 0 ||
          layer.cells.length !== layer.width * layer.height ||
          layer.cells.some((tile) => tile >= layer.tileFlags.length)
        )
          throw new TypeError(`map query data for '${asset.name}' is incoherent`);
      this.maps.set(asset.name, asset);
    }
  }
  cell(name, layerIndex, x, y) {
    const layer = this.maps.get(name)?.layers[layerIndex];
    if (layer === void 0 || x < 0 || y < 0 || x >= layer.width || y >= layer.height) return -1;
    return layer.cells[y * layer.width + x] ?? -1;
  }
  flag(name, layerIndex, x, y, flagIndex) {
    if (flagIndex < 0 || flagIndex > 7) return false;
    const layer = this.maps.get(name)?.layers[layerIndex];
    const tile = this.cell(name, layerIndex, x, y);
    return layer !== void 0 && tile >= 0 && ((layer.tileFlags[tile] ?? 0) & (1 << flagIndex)) !== 0;
  }
};
function isMapQueryCatalog(value) {
  return Array.isArray(value) && value.every(isMapQueryAsset);
}
function isMapQueryAsset(value) {
  return (
    isRecord$3(value) &&
    typeof value.name === 'string' &&
    value.name.length > 0 &&
    Array.isArray(value.layers) &&
    value.layers.length > 0 &&
    value.layers.every(isMapQueryLayer)
  );
}
function isMapQueryLayer(value) {
  if (
    !isRecord$3(value) ||
    !Number.isSafeInteger(value.width) ||
    typeof value.width !== 'number' ||
    value.width <= 0 ||
    !Number.isSafeInteger(value.height) ||
    typeof value.height !== 'number' ||
    value.height <= 0 ||
    !(value.cells instanceof Uint16Array) ||
    value.cells.length !== value.width * value.height ||
    !(value.tileFlags instanceof Uint8Array)
  )
    return false;
  const tileFlags = value.tileFlags;
  return value.cells.every((tile) => tile < tileFlags.length);
}
function isRecord$3(value) {
  return typeof value === 'object' && value !== null;
}
//#endregion
//#region src/save.ts
/** One byte image, with a commit latch delivered to the trusted host after a successful frame. */
var SaveMemory = class SaveMemory {
  bytes = new Uint8Array(HARDWARE.saveCapacityBytes);
  committed = new Uint8Array(HARDWARE.saveCapacityBytes);
  values;
  pendingCommit = false;
  commits = 0;
  dirtyBytes = 0;
  writes = /* @__PURE__ */ new Map();
  constructor(initial = {}) {
    if (!isSaveImage(initial)) throw new TypeError('invalid MOD-01 save image');
    this.bytes.set(initial instanceof Uint8Array ? initial : encodeValues(initial));
    this.committed.set(this.bytes);
  }
  get(key, fallback) {
    validateKey(key);
    validateInteger(fallback);
    const values = this.readValues();
    return Object.hasOwn(values, key) ? (values[key] ?? fallback) : fallback;
  }
  set(key, value) {
    validateKey(key);
    validateInteger(value);
    this.checkCommitCounter();
    const next = {
      ...this.readValues(),
      [key]: value,
    };
    const bytes = encodeValues(next);
    this.bytes.fill(0);
    this.bytes.set(bytes);
    this.values = next;
    this.latch();
    for (const [pendingKey, pendingValue] of this.writes)
      if (!Object.hasOwn(next, pendingKey) || next[pendingKey] !== pendingValue)
        this.writes.delete(pendingKey);
    this.writes.set(key, value);
  }
  snapshot() {
    return sortedValues(this.readValues());
  }
  deviceSnapshot() {
    return {
      revision: 1,
      bytes: this.bytes.slice(),
      committed: this.committed.slice(),
      pendingCommit: this.pendingCommit,
      commits: this.commits,
    };
  }
  restoreDevice(value, pendingWrites = []) {
    if (!isSaveSnapshot(value) || !isPendingDeviceWrites(pendingWrites, value))
      throw new TypeError('invalid MOD-01 save snapshot');
    this.bytes.set(value.bytes);
    this.committed.set(value.committed);
    this.values = void 0;
    this.pendingCommit = value.pendingCommit;
    this.commits = value.commits;
    this.dirtyBytes = this.bytes.reduce(
      (count, byte, index) => count + Number(byte !== this.committed[index]),
      0,
    );
    this.writes.clear();
    for (const write of pendingWrites) this.writes.set(write.key, write.value);
  }
  commit() {
    this.checkCommitCounter();
    this.latch();
    this.writes.clear();
  }
  takeCommit() {
    if (!this.pendingCommit) return void 0;
    this.pendingCommit = false;
    this.writes.clear();
    return this.committed.slice();
  }
  restore(value, pendingWrites = []) {
    if (!isSaveValues(value) || !isPendingSaveWrites(pendingWrites, value))
      throw new TypeError('invalid MOD-01 save snapshot');
    const initial = new SaveMemory(value).deviceSnapshot();
    this.restoreDevice(
      {
        ...initial,
        pendingCommit: pendingWrites.length > 0,
      },
      pendingWrites,
    );
  }
  pendingWrites() {
    return [...this.writes]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, value]) => ({
        key,
        value,
      }));
  }
  takeWrites() {
    const writes = this.pendingWrites();
    this.writes.clear();
    return writes;
  }
  memoryRegions(charge) {
    return [
      {
        name: 'save working bytes',
        address: MEMORY.save,
        length: this.bytes.length,
        writable: true,
        readByte: (offset) => this.bytes[offset] ?? 0,
        prepareWrite: (offset, bytes) => () => {
          for (let index = 0; index < bytes.length; index += 1) {
            const cursor = offset + index;
            this.dirtyBytes +=
              Number(bytes[index] !== this.committed[cursor]) -
              Number(this.bytes[cursor] !== this.committed[cursor]);
          }
          this.bytes.set(bytes, offset);
          this.values = void 0;
        },
      },
      {
        name: 'save committed latch',
        address: MEMORY.saveCommitted,
        bytes: this.committed,
        writable: false,
      },
      {
        name: 'save commit command',
        address: MEMORY.saveControl,
        length: 1,
        writable: true,
        readByte: () => 0,
        prepareWrite: (_offset, bytes, span, debugEdit) => {
          if (bytes[0] === 0) return () => void 0;
          if (bytes[0] !== 1) return void 0;
          try {
            this.checkCommitCounter();
          } catch (error) {
            throw new RuntimeFault(
              'M019012',
              error instanceof Error ? error.message : 'save counter exhausted',
              span,
            );
          }
          if (debugEdit !== true) charge(HARDWARE.saveCapacityBytes, span);
          return () => {
            this.latch();
            this.writes.clear();
          };
        },
      },
      {
        name: 'save status',
        address: MEMORY.saveControl + 1,
        length: 31,
        writable: false,
        readByte: (offset) => {
          const status = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(32));
          status.setUint8(1, (this.dirtyBytes > 0 ? 1 : 0) | (this.pendingCommit ? 2 : 0));
          status.setUint32(4, HARDWARE.saveCapacityBytes, true);
          status.setBigUint64(8, BigInt(this.commits), true);
          status.setUint32(16, this.dirtyBytes, true);
          return status.getUint8(offset + 1);
        },
      },
    ];
  }
  readValues() {
    this.values ??= decodeSaveValues(this.bytes);
    return this.values;
  }
  checkCommitCounter() {
    if (this.commits === Number.MAX_SAFE_INTEGER)
      throw new RangeError('save commit counter exhausted');
  }
  latch() {
    this.committed.set(this.bytes);
    this.pendingCommit = true;
    this.dirtyBytes = 0;
    this.commits += 1;
  }
};
function isSaveImage(value) {
  return value instanceof Uint8Array
    ? value.length <= HARDWARE.saveCapacityBytes
    : isSaveValues(value);
}
function isSaveSnapshot(value) {
  return (
    isRecord$2(value) &&
    Object.keys(value).length === 5 &&
    value.revision === 1 &&
    value.bytes instanceof Uint8Array &&
    value.bytes.length === HARDWARE.saveCapacityBytes &&
    value.committed instanceof Uint8Array &&
    value.committed.length === HARDWARE.saveCapacityBytes &&
    typeof value.pendingCommit === 'boolean' &&
    typeof value.commits === 'number' &&
    Number.isSafeInteger(value.commits) &&
    value.commits >= 0
  );
}
function isPendingDeviceWrites(value, snapshot) {
  if (!Array.isArray(value)) return false;
  if (value.length === 0) return Object.keys(value).length === 0;
  if (!snapshot.pendingCommit) return false;
  try {
    return isPendingSaveWrites(value, decodeSaveValues(snapshot.committed));
  } catch {
    return false;
  }
}
function decodeSaveValues(bytes) {
  if (bytes.length > HARDWARE.saveCapacityBytes)
    throw new TypeError('save image exceeds the 8 KiB capacity');
  const zero = bytes.indexOf(0);
  const end = zero === -1 ? bytes.length : zero;
  if (
    end > HARDWARE.saveCapacityBytes ||
    (zero !== -1 && bytes.subarray(zero).some((byte) => byte !== 0))
  )
    throw new TypeError('save bytes are not an integer-save image');
  if (end === 0) return {};
  let values;
  try {
    values = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, end)));
  } catch {
    throw new TypeError('save bytes are not an integer-save image');
  }
  if (!isSaveValues(values)) throw new TypeError('save bytes are not an integer-save image');
  return values;
}
function isPendingSaveWrites(value, values) {
  if (
    !Array.isArray(value) ||
    value.length > Object.keys(values).length ||
    Object.keys(value).length !== value.length
  )
    return false;
  const keys = /* @__PURE__ */ new Set();
  for (const write of value) {
    if (
      !isRecord$2(write) ||
      Object.keys(write).length !== 2 ||
      typeof write.key !== 'string' ||
      !Object.hasOwn(values, write.key) ||
      write.value !== values[write.key] ||
      keys.has(write.key)
    )
      return false;
    keys.add(write.key);
  }
  return true;
}
function isSaveValues(value) {
  if (!isRecord$2(value) || Array.isArray(value)) return false;
  try {
    for (const [key, entry] of Object.entries(value)) {
      validateKey(key);
      validateInteger(entry);
    }
    return encodedBytes(value) <= HARDWARE.saveCapacityBytes;
  } catch {
    return false;
  }
}
function sortedValues(values) {
  return Object.fromEntries(
    Object.entries(values).sort(([left], [right]) => left.localeCompare(right)),
  );
}
function encodedBytes(values) {
  return new TextEncoder().encode(JSON.stringify(sortedValues(values))).byteLength;
}
function encodeValues(values) {
  if (Object.keys(values).length === 0) return /* @__PURE__ */ new Uint8Array();
  const bytes = new TextEncoder().encode(
    JSON.stringify(
      Object.fromEntries(
        Object.entries(values).sort(([left], [right]) =>
          left < right ? -1 : left > right ? 1 : 0,
        ),
      ),
    ),
  );
  if (bytes.length > HARDWARE.saveCapacityBytes)
    throw new RangeError('cartridge save exceeds the 8 KiB capacity');
  return bytes;
}
function validateKey(key) {
  if (
    !/^[A-Za-z_][A-Za-z0-9_.-]{0,63}$/.test(key) ||
    ['__proto__', 'constructor', 'prototype'].includes(key)
  )
    throw new TypeError('save keys must be 1-64 canonical ASCII characters');
}
function validateInteger(value) {
  if (typeof value !== 'number' || !Number.isSafeInteger(value))
    throw new TypeError('save values must be safe integers');
}
function isRecord$2(value) {
  return typeof value === 'object' && value !== null;
}
//#endregion
//#region src/protocol.ts
function isConsoleConfiguration(value) {
  return (
    isRecord$1(value) &&
    hasExactKeys(value, [
      'seed',
      'workUnitsPerFrame',
      'updateRate',
      ...(value.hardwareRevision === void 0 ? [] : ['hardwareRevision']),
      ...(value.maps === void 0 ? [] : ['maps']),
      ...(value.save === void 0 ? [] : ['save']),
      ...(value.debug === void 0 ? [] : ['debug']),
      ...(value.assets === void 0 ? [] : ['assets']),
      ...(value.rom === void 0 ? [] : ['rom']),
    ]) &&
    Number.isSafeInteger(value.seed) &&
    isNonNegativeInteger(value.workUnitsPerFrame) &&
    value.workUnitsPerFrame > 0 &&
    value.workUnitsPerFrame <= HARDWARE.workUnitsPerFrame &&
    (value.updateRate === 30 || value.updateRate === 60) &&
    (value.hardwareRevision === void 0 || value.hardwareRevision === 2) &&
    (value.maps === void 0 || isMapQueryCatalog(value.maps)) &&
    (value.save === void 0 || isSaveImage(value.save)) &&
    (value.debug === void 0 || typeof value.debug === 'boolean') &&
    (value.assets === void 0 || isRuntimeAssetSource(value.assets)) &&
    (value.rom === void 0 ||
      (value.rom instanceof Uint8Array &&
        value.rom.length > 0 &&
        value.rom.length <= HARDWARE.cartridgeCapacityBytes))
  );
}
function isRecord$1(value) {
  return typeof value === 'object' && value !== null;
}
function isNonNegativeInteger(value) {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
function hasExactKeys(value, expected) {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length && expected.every((key) => actual.includes(key));
}
function isCartridgeAlias(value) {
  return typeof value === 'string' && /^[a-z][a-z0-9_-]{0,31}$/.test(value);
}
function isTransferParameter(value) {
  return (
    typeof value === 'string' &&
    value.length <= 1024 &&
    value.isWellFormed() &&
    new TextEncoder().encode(value).length <= 1024
  );
}
function request(value) {
  if (!record(value) || !isCartridgeAlias(value.alias)) return false;
  return value.kind === 'load'
    ? exact(value, ['kind', 'alias', 'parameter']) && isTransferParameter(value.parameter)
    : value.kind === 'reload' && exact(value, ['kind', 'alias']);
}
function isCartridgeControlSnapshot(value) {
  return (
    record(value) &&
    exact(value, ['revision', 'alias', 'parameter', 'pending']) &&
    value.revision === 1 &&
    isCartridgeAlias(value.alias) &&
    isTransferParameter(value.parameter) &&
    (value.pending === null || request(value.pending))
  );
}
function record(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function exact(value, keys) {
  return (
    Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key))
  );
}
//#endregion
//#region src/console-runtime.ts
function isConsoleRuntimeSnapshot(value) {
  return (
    isRecord(value) &&
    value.revision === 7 &&
    Object.keys(value).length === 8 &&
    isMachineSnapshot(value.machine) &&
    isSaveSnapshot(value.save) &&
    isGraphicsSnapshot(value.graphics) &&
    isSynthSnapshot(value.audio) &&
    isPendingDeviceWrites(value.pendingSaveWrites, value.save) &&
    isMemorySnapshot(value.memory) &&
    graphicsMatchesMemory(value.graphics, value.memory) &&
    (value.cartridgeControl === null || isCartridgeControlSnapshot(value.cartridgeControl))
  );
}
/** One production dispatcher for native Studio, standalone players and headless hosts. */
function createConsoleRuntime(factory, configuration, services = {}) {
  if (!isConsoleConfiguration(configuration))
    throw new RuntimeFault('M019100', 'invalid console configuration', {
      start: 0,
      end: 0,
    });
  const cartridgeControl = services.cartridge;
  const deferBoot = services.deferBoot === true;
  const source = configuration.assets;
  const assets =
    source === void 0
      ? decodeRuntimeAssets({}, {})
      : decodeRuntimeAssets(source.declarations, source.files, source.displayPath);
  const visualStore = new VisualAssetStore(assets.visual, assets.display);
  const graphics = new IndexedGraphics(visualStore);
  const audioStore = new AudioAssetStore(assets.audio);
  const synthesizer = new Synthesizer(audioStore);
  let rendering = false;
  let machine = void 0;
  let drawCommands = [];
  let audioCommands = [];
  let frameOutput;
  let debugFrameActive = false;
  const mapQueries = new MapQueryStore(source === void 0 ? (configuration.maps ?? []) : []);
  const saveMemory = new SaveMemory(configuration.save ?? {});
  const cartridgeRom = configuration.rom?.slice() ?? /* @__PURE__ */ new Uint8Array();
  const cartridgeInfo = createCartridgeInfo(cartridgeRom, configuration.hardwareRevision ?? 2);
  const debugEnabled = configuration.debug ?? false;
  let debugTrace = [];
  const debugCallStack = [];
  let debugTraceTruncated = false;
  const ram = new Uint8Array(MEMORY.ramBytes);
  const bus = new MemoryBus(
    [
      {
        name: 'ram',
        address: MEMORY.ram,
        bytes: ram,
        writable: true,
      },
      ...graphics.memoryRegions(),
      ...visualStore.memoryRegions(),
      ...synthesizer.memoryRegions(),
      ...saveMemory.memoryRegions((units, span) => {
        requireMachine().work(units, span);
      }),
      {
        name: 'cartridge status',
        address: MEMORY.cartridgeInfo,
        bytes: cartridgeInfo,
        writable: false,
      },
      ...(cartridgeRom.length === 0
        ? []
        : [
            {
              name: 'canonical cartridge ROM',
              address: MEMORY.cartridgeRom,
              bytes: cartridgeRom,
              writable: false,
            },
          ]),
      {
        name: 'controllers and pointer',
        address: MEMORY.input,
        length: MEMORY.inputBytes,
        writable: false,
        readByte: (offset) => requireMachine().readInputByte(offset),
      },
      {
        name: 'scheduler, RNG, work and fault status',
        address: MEMORY.system,
        length: MEMORY.systemBytes,
        writable: false,
        readByte: (offset) => requireMachine().readSystemByte(offset),
      },
      {
        name: 'master palette RGBA',
        address: MEMORY.palette,
        bytes: Uint8Array.from(MASTER_PALETTE_RGBA),
        writable: false,
      },
    ],
    (units, span) => {
      requireMachine().work(units, span);
    },
  );
  machine = new DeterministicMachine(factory, configuration, {
    completeFrame: () => {
      frameOutput = {
        indexedPixels: graphics.finishFrame().indexedPixels,
        audio: synthesizer.finishFrame(),
        audioState: synthesizer.snapshot(),
      };
    },
    call: handleConsoleCall,
    ...(debugEnabled
      ? {
          probe: (id, sourceSpan, locals) => {
            if (debugTrace.length >= HARDWARE.drawCommandsPerFrame) {
              debugTraceTruncated = true;
              return;
            }
            debugTrace.push({
              id,
              sourceSpan,
              locals: structuredClone(locals),
              callStack: structuredClone(debugCallStack),
            });
          },
          enter: (name, sourceSpan) => {
            debugCallStack.push({
              name,
              sourceSpan,
            });
          },
          leave: () => {
            debugCallStack.pop();
          },
        }
      : {}),
  });
  graphics.beginFrame();
  rendering = true;
  try {
    if (!debugEnabled && !deferBoot) machine.boot();
  } finally {
    rendering = false;
  }
  graphics.finishFrame();
  let boundarySnapshot = captureSnapshot();
  return {
    runFrame(input) {
      if (!isInputFrame(input))
        throw new RuntimeFault('M019008', 'invalid controller input frame', {
          start: 0,
          end: 0,
        });
      const active = requireMachine();
      active.assertRunnable();
      drawCommands = [];
      audioCommands = [];
      frameOutput = void 0;
      debugTrace = [];
      debugTraceTruncated = false;
      graphics.beginFrame();
      rendering = true;
      let report;
      try {
        report = active.runFrame(input);
      } finally {
        rendering = false;
      }
      const output = completedOutput();
      const saveWrites = saveMemory.takeWrites();
      const saveCommit = saveMemory.takeCommit();
      boundarySnapshot = captureSnapshot();
      return {
        ...report,
        drawCommands,
        audioCommands,
        saveWrites,
        ...(saveCommit === void 0 ? {} : { saveCommit }),
        output,
        ...(debugEnabled
          ? {
              debug: {
                trace: debugTrace,
                truncated: debugTraceTruncated,
                inspection: structuredClone(active.inspect()),
              },
            }
          : {}),
      };
    },
    stepDebug(input) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'statement stepping requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!isInputFrame(input))
        throw new RuntimeFault('M019008', 'invalid controller input frame', {
          start: 0,
          end: 0,
        });
      if (!debugFrameActive) {
        drawCommands = [];
        audioCommands = [];
        frameOutput = void 0;
        debugTrace = [];
        debugTraceTruncated = false;
        graphics.beginFrame();
        debugFrameActive = true;
      }
      rendering = true;
      let step;
      try {
        step = requireMachine().stepDebug(input);
      } finally {
        rendering = false;
      }
      if (step.event !== void 0) {
        const inspection = structuredClone(requireMachine().inspect());
        return {
          event: {
            ...step.event,
            callStack: structuredClone(debugCallStack),
          },
          inspection,
        };
      }
      if (step.booted === true) {
        debugFrameActive = false;
        graphics.finishFrame();
        boundarySnapshot = captureSnapshot();
        return { booted: true };
      }
      const report = step.report;
      if (report === void 0) throw new TypeError('debug step produced no event or frame');
      debugFrameActive = false;
      const output = completedOutput();
      const saveWrites = saveMemory.takeWrites();
      const saveCommit = saveMemory.takeCommit();
      const inspection = structuredClone(requireMachine().inspect());
      boundarySnapshot = captureSnapshot();
      return {
        frame: {
          ...report,
          drawCommands,
          audioCommands,
          saveWrites,
          ...(saveCommit === void 0 ? {} : { saveCommit }),
          output,
          debug: {
            trace: debugTrace,
            truncated: debugTraceTruncated,
            inspection,
          },
        },
      };
    },
    snapshot: captureSnapshot,
    restore(value) {
      if (!isConsoleRuntimeSnapshot(value))
        throw new RuntimeFault('M019103', 'invalid console snapshot', {
          start: 0,
          end: 0,
        });
      const snapshot = value;
      if ((cartridgeControl === void 0) !== (snapshot.cartridgeControl === null))
        throw new RuntimeFault(
          'M019103',
          'snapshot cartridge environment does not match this console',
          {
            start: 0,
            end: 0,
          },
        );
      const before = debugFrameActive ? boundarySnapshot : captureSnapshot();
      try {
        if (snapshot.cartridgeControl !== null)
          cartridgeControl?.restore(snapshot.cartridgeControl);
        requireMachine().restore(snapshot.machine);
        debugFrameActive = false;
        saveMemory.restoreDevice(snapshot.save, snapshot.pendingSaveWrites);
        graphics.restore(snapshot.graphics);
        synthesizer.restore(snapshot.audio);
        bus.restore(snapshot.memory);
        boundarySnapshot = captureSnapshot();
      } catch (error) {
        if (before.cartridgeControl !== null) cartridgeControl?.restore(before.cartridgeControl);
        requireMachine().restore(before.machine);
        saveMemory.restoreDevice(before.save, before.pendingSaveWrites);
        graphics.restore(before.graphics);
        synthesizer.restore(before.audio);
        bus.restore(before.memory);
        throw new RuntimeFault(
          'M019103',
          error instanceof Error ? error.message : 'invalid device snapshot',
          {
            start: 0,
            end: 0,
          },
        );
      }
    },
    inspectMemory(address, length) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'memory inspection requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!Number.isSafeInteger(length) || length < 1 || length > 256)
        throw new RuntimeFault('M019020', 'debug memory reads require 1-256 bytes', {
          start: 0,
          end: 0,
        });
      return {
        address,
        bytes: bus.inspect(address, length),
        regions: bus.describe(),
      };
    },
    editMemory(address, bytes) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'memory editing requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!(bytes instanceof Uint8Array) || bytes.length < 1 || bytes.length > 256)
        throw new RuntimeFault('M019020', 'debug memory edits require 1-256 bytes', {
          start: 0,
          end: 0,
        });
      bus.edit(address, bytes);
    },
    memoryRegions() {
      return bus.describe();
    },
    inspectMenu() {
      return requireMachine().inspectMenu();
    },
  };
  function captureSnapshot() {
    return {
      revision: 7,
      machine: requireMachine().snapshot(),
      save: saveMemory.deviceSnapshot(),
      graphics: graphics.snapshot(),
      audio: synthesizer.snapshot(),
      pendingSaveWrites: saveMemory.pendingWrites(),
      memory: bus.snapshot(),
      cartridgeControl: cartridgeControl?.snapshot() ?? null,
    };
  }
  function completedOutput() {
    if (frameOutput === void 0)
      throw new RuntimeFault('M019102', 'frame completed without device output', {
        start: 0,
        end: 0,
      });
    return frameOutput;
  }
  function handleConsoleCall(name, arguments_, sourceSpan, context) {
    if (name === 'load_cart' || name === 'cart_parameter' || name === 'reload_assets') {
      if (cartridgeControl === void 0)
        throw new RuntimeFault(
          'M019017',
          'cartridge changes require an admitted game scheduler',
          sourceSpan,
        );
      return cartridgeControl.call(name, arguments_, sourceSpan, context, (units, span) => {
        requireMachine().work(units, span);
      });
    }
    if (name === 'visual_id' || name === 'audio_id') {
      if (arguments_.length !== 1 || typeof arguments_[0] !== 'string')
        throw new RuntimeFault('M019009', `${name} expects one Text name`, sourceSpan);
      requireMachine().work(1 + arguments_[0].length, sourceSpan);
      return (name === 'visual_id' ? visualStore : audioStore).id(arguments_[0]);
    }
    if (MEMORY_CALLS.has(name)) {
      if (arguments_.length !== MEMORY_CALLS.get(name))
        throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
      const values = arguments_.map((value) => readInteger(value, sourceSpan));
      const address = values[0] ?? 0;
      const second = values[1] ?? 0;
      const third = values[2] ?? 0;
      const raster = context.phase === 'raster';
      switch (name) {
        case 'mem_read':
          return bus.read(address, 1, sourceSpan);
        case 'mem_read16':
          return bus.read(address, 2, sourceSpan);
        case 'mem_write':
          bus.write(address, second, 1, sourceSpan, raster);
          return;
        case 'mem_write16':
          bus.write(address, second, 2, sourceSpan, raster);
          return;
        case 'mem_copy':
          bus.copy(address, second, third, sourceSpan, raster);
          return;
        case 'mem_fill':
          bus.fill(address, second, third, sourceSpan, raster);
          return;
      }
    }
    requireMachine().work(consoleWorkCost(name, arguments_), sourceSpan);
    if (context.phase === 'raster' && name !== 'pal' && name !== 'raster_scroll')
      throw new RuntimeFault(
        'M019011',
        `console API call '${name}' is not valid in the raster callback`,
        sourceSpan,
      );
    if (name === 'raster_scroll' && context.phase !== 'raster')
      throw new RuntimeFault(
        'M019011',
        'raster_scroll is only valid in the raster callback',
        sourceSpan,
      );
    if (name === 'save_commit') {
      if (arguments_.length !== 0)
        throw new RuntimeFault('M019009', 'save_commit expects no arguments', sourceSpan);
      requireMachine().work(HARDWARE.saveCapacityBytes, sourceSpan);
      try {
        saveMemory.commit();
      } catch (error) {
        throw new RuntimeFault(
          'M019012',
          error instanceof Error ? error.message : 'invalid save commit',
          sourceSpan,
        );
      }
      return;
    }
    if (name === 'map_cell' || name === 'map_flag') {
      const handle = arguments_[0];
      if (!isAssetHandle(handle, 'Map'))
        throw new RuntimeFault('M019009', 'expected a Map asset handle', sourceSpan);
      const integers = arguments_.slice(1).map((value) => readInteger(value, sourceSpan));
      if (name === 'map_cell' && integers.length === 3) {
        if (source !== void 0)
          return (
            visualStore.mapCell(
              handle.name,
              integers[0] ?? 0,
              integers[1] ?? 0,
              integers[2] ?? 0,
            ) ?? -1
          );
        return mapQueries.cell(handle.name, integers[0] ?? 0, integers[1] ?? 0, integers[2] ?? 0);
      }
      if (name === 'map_flag' && integers.length === 4) {
        if (source !== void 0)
          return visualStore.mapFlag(
            handle.name,
            integers[0] ?? 0,
            integers[1] ?? 0,
            integers[2] ?? 0,
            integers[3] ?? 0,
          );
        return mapQueries.flag(
          handle.name,
          integers[0] ?? 0,
          integers[1] ?? 0,
          integers[2] ?? 0,
          integers[3] ?? 0,
        );
      }
      throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
    }
    if (name === 'save_get_int' || name === 'save_set_int') {
      const key = arguments_[0];
      if (typeof key !== 'string')
        throw new RuntimeFault('M019009', 'save key must be Text', sourceSpan);
      try {
        if (name === 'save_get_int' && arguments_.length === 2)
          return saveMemory.get(key, readInteger(arguments_[1], sourceSpan));
        if (name === 'save_set_int' && arguments_.length === 2) {
          saveMemory.set(key, readInteger(arguments_[1], sourceSpan));
          return;
        }
      } catch (error) {
        throw new RuntimeFault(
          'M019012',
          error instanceof Error ? error.message : 'invalid cartridge save operation',
          sourceSpan,
        );
      }
      throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
    }
    const command = {
      name,
      arguments: structuredClone(arguments_),
      sourceSpan,
      ...(context.rasterLine === void 0 ? {} : { rasterLine: context.rasterLine }),
    };
    if (DRAW_CALLS.has(name)) {
      if (drawCommands.length >= HARDWARE.drawCommandsPerFrame)
        throw new RuntimeFault('M019010', 'draw-command ceiling exceeded', sourceSpan);
      drawCommands.push(command);
      if (rendering)
        try {
          graphics.executeCommand(command);
        } catch (error) {
          if (error instanceof TypeError || error instanceof RangeError)
            throw new RuntimeFault('M019009', error.message, sourceSpan);
          throw error;
        }
      return;
    }
    if (AUDIO_CALLS.has(name)) {
      audioCommands.push(command);
      if (rendering) synthesizer.executeCommand(command);
      return;
    }
    throw new RuntimeFault('M019004', `console API call '${name}' is unavailable`, sourceSpan);
  }
  function requireMachine() {
    if (machine === void 0)
      throw new RuntimeFault('M019102', 'no cartridge is loaded', {
        start: 0,
        end: 0,
      });
    return machine;
  }
}
function createCartridgeInfo(rom, hardwareRevision) {
  const bytes = /* @__PURE__ */ new Uint8Array(64);
  const view = new DataView(bytes.buffer);
  const validHeader =
    rom.length >= 12 &&
    [77, 79, 68, 45, 48, 49, 26].every((byte, index) => rom[index] === byte) &&
    rom[7] === 2;
  view.setUint16(0, hardwareRevision, true);
  view.setUint16(2, validHeader ? (rom[7] ?? 0) : 0, true);
  view.setUint32(4, rom.length, true);
  view.setUint32(8, HARDWARE.cartridgeCapacityBytes, true);
  view.setUint32(12, MEMORY.cartridgeRom, true);
  view.setUint32(16, Number(rom.length > 0) | (Number(validHeader) << 1), true);
  view.setUint32(
    20,
    validHeader ? new DataView(rom.buffer, rom.byteOffset).getUint32(8, true) : 0,
    true,
  );
  return bytes;
}
var DRAW_CALLS = /* @__PURE__ */ new Set([
  'clear',
  'pixel',
  'line',
  'rect',
  'rect_fill',
  'circle',
  'circle_fill',
  'triangle',
  'camera',
  'clip',
  'clip_reset',
  'sprite',
  'sprite_xform',
  'sprite_region',
  'texture_line',
  'animation',
  'map',
  'pal',
  'pal_reset',
  'palt',
  'palt_reset',
  'fill_pattern',
  'fill_pattern_reset',
  'raster_scroll',
  'print',
  'font_print',
]);
var AUDIO_CALLS = /* @__PURE__ */ new Set(['sfx', 'music', 'music_stop']);
function consoleWorkCost(name, arguments_) {
  const integer = (index) => {
    const value = arguments_[index];
    return typeof value === 'number' && Number.isSafeInteger(value) ? value : 0;
  };
  switch (name) {
    case 'clear':
      return Math.ceil((HARDWARE.width * HARDWARE.height) / 32);
    case 'pixel':
      return 1;
    case 'line':
      return Math.max(Math.abs(integer(2) - integer(0)), Math.abs(integer(3) - integer(1))) + 1;
    case 'rect':
      return Math.max(1, 2 * Math.abs(integer(2)) + 2 * Math.abs(integer(3)));
    case 'rect_fill':
      return Math.max(1, Math.ceil((Math.abs(integer(2)) * Math.abs(integer(3))) / 4));
    case 'circle':
      return Math.max(1, Math.abs(integer(2)) * 8);
    case 'circle_fill':
      return Math.max(1, Math.ceil((Math.abs(integer(2)) ** 2 * 3) / 4));
    case 'triangle': {
      const area = Math.abs(
        (integer(2) - integer(0)) * (integer(5) - integer(1)) -
          (integer(4) - integer(0)) * (integer(3) - integer(1)),
      );
      return Math.max(1, Math.ceil(area / 8));
    }
    case 'sprite':
    case 'animation':
      return 32;
    case 'sprite_xform':
      return Math.max(32, 32 * Math.abs(integer(3)) ** 2);
    case 'sprite_region':
      if ([3, 4, 7, 8].some((index) => integer(index) <= 0)) return 1;
      return Math.min(
        Number.MAX_SAFE_INTEGER,
        Math.max(1, Math.ceil((integer(7) * integer(8)) / 4)),
      );
    case 'texture_line': {
      const samples =
        Math.max(Math.abs(integer(3) - integer(1)), Math.abs(integer(4) - integer(2))) + 1;
      return Math.min(Number.MAX_SAFE_INTEGER, 2 + Math.ceil(samples / 2));
    }
    case 'map':
      return 128;
    case 'print':
      return Math.max(1, (typeof arguments_[0] === 'string' ? arguments_[0].length : 0) * 6);
    case 'font_print':
      return Math.max(1, (typeof arguments_[1] === 'string' ? arguments_[1].length : 0) * 8);
    case 'sfx':
    case 'music':
    case 'music_stop':
      return 8;
    default:
      return 1;
  }
}
function isRecord(value) {
  return typeof value === 'object' && value !== null;
}
function isAssetHandle(value, kind) {
  return (
    isRecord(value) &&
    typeof value.name === 'string' &&
    value.name.length > 0 &&
    value.kind === kind
  );
}
function readInteger(value, sourceSpan) {
  if (typeof value !== 'number' || !Number.isSafeInteger(value))
    throw new RuntimeFault('M019009', 'console arguments must be safe integers', sourceSpan);
  return value;
}
function graphicsMatchesMemory(graphics, memory) {
  return [
    [MEMORY.front, graphics.front],
    [MEMORY.display, graphics.resolved],
  ].every(([address, pixels]) => {
    if (!(pixels instanceof Uint8Array)) return false;
    const bytes = memory.regions.find((region) => region.address === address)?.bytes;
    return (
      bytes !== void 0 &&
      bytes.length === pixels.length &&
      bytes.every((value, index) => value === pixels[index])
    );
  });
}
var MEMORY_CALLS = /* @__PURE__ */ new Map([
  ['mem_read', 1],
  ['mem_read16', 1],
  ['mem_write', 2],
  ['mem_write16', 2],
  ['mem_copy', 3],
  ['mem_fill', 3],
]);
//#endregion
//#region src/wav.ts
var span = {
  start: 0,
  end: 0,
};
/** Encodes production-synth frames as deterministic stereo 16-bit PCM WAV. */
function encodeAudioFramesWav(frames) {
  const dataBytes = frames.reduce((total, frame) => total + frame.left.length, 0) * 4;
  const output = new Uint8Array(44 + dataBytes);
  const view = new DataView(output.buffer);
  writeAscii(output, 0, 'RIFF');
  view.setUint32(4, 36 + dataBytes, true);
  writeAscii(output, 8, 'WAVEfmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 2, true);
  view.setUint32(24, HARDWARE.audioSampleRate, true);
  view.setUint32(28, HARDWARE.audioSampleRate * 4, true);
  view.setUint16(32, 4, true);
  view.setUint16(34, 16, true);
  writeAscii(output, 36, 'data');
  view.setUint32(40, dataBytes, true);
  let offset = 44;
  for (const frame of frames) {
    if (frame.left.length !== frame.right.length)
      throw new TypeError('audio frame channel lengths differ');
    for (let sample = 0; sample < frame.left.length; sample += 1) {
      view.setInt16(offset, pcm16(frame.left[sample] ?? 0), true);
      view.setInt16(offset + 2, pcm16(frame.right[sample] ?? 0), true);
      offset += 4;
    }
  }
  return output;
}
function renderSoundWav(sound) {
  const synth = new Synthesizer(new AudioAssetStore([sound]));
  const frames = sound.durationFrames + sound.envelope.releaseFrames;
  return encodeAudioFramesWav(
    Array.from({ length: frames }, (_, frame) =>
      synth.executeFrame(frame === 0 ? [audioCommand('sfx', sound.name, 'Sound')] : []),
    ),
  );
}
function renderMusicWav(music, sounds) {
  const frames =
    music.framesPerRow *
    music.order.reduce((total, name) => total + (music.patterns[name]?.rows.length ?? 0), 0);
  if (frames < 1 || frames > HARDWARE.frameRate * 60 * 10)
    throw new RangeError('music WAV duration must be between one frame and ten minutes');
  const synth = new Synthesizer(new AudioAssetStore([...sounds, music]));
  return encodeAudioFramesWav(
    Array.from({ length: frames }, (_, frame) =>
      synth.executeFrame(frame === 0 ? [audioCommand('music', music.name, 'Music')] : []),
    ),
  );
}
function audioCommand(name, asset, kind) {
  return {
    name,
    arguments: [
      {
        name: asset,
        kind,
      },
    ],
    sourceSpan: span,
  };
}
function pcm16(value) {
  if (!Number.isFinite(value)) throw new TypeError('audio sample is not finite');
  const clamped = Math.max(-1, Math.min(1, value));
  return clamped < 0 ? Math.round(clamped * 32768) : Math.round(clamped * 32767);
}
function writeAscii(target, offset, text) {
  for (let index = 0; index < text.length; index += 1)
    target[offset + index] = text.charCodeAt(index);
}
//#endregion
//#region src/debugger.ts
/** Evaluates a non-mutating MODL-like watch subset without dynamic JavaScript execution. */
function evaluateWatch(source, environment) {
  const parser = new WatchParser(tokenizeWatch(source), environment);
  const value = parser.expression();
  parser.finish();
  return value;
}
var WatchParser = class {
  tokens;
  environment;
  cursor = 0;
  constructor(tokens, environment) {
    this.tokens = tokens;
    this.environment = environment;
  }
  expression(minimumPrecedence = 0) {
    let left = this.prefix();
    for (;;) {
      const operator = this.peek().text;
      const precedence = BINARY_PRECEDENCE[operator];
      if (precedence === void 0 || precedence < minimumPrecedence) break;
      this.cursor += 1;
      const right = this.expression(precedence + 1);
      left = binaryWatch(operator, left, right);
    }
    return left;
  }
  finish() {
    if (this.peek().kind !== 'eof')
      throw new SyntaxError(`unexpected '${this.peek().text}' in watch expression`);
  }
  prefix() {
    const token = this.take();
    let value;
    if (token.kind === 'number') value = Number(token.text);
    else if (token.kind === 'text') value = JSON.parse(token.text);
    else if (token.kind === 'name') {
      if (token.text === 'true' || token.text === 'false') value = token.text === 'true';
      else if (token.text === 'none') value = null;
      else if (token.text === 'not') value = !truthy(this.expression(7));
      else if (Object.hasOwn(this.environment, token.text)) value = this.environment[token.text];
      else throw new ReferenceError(`unknown watch name '${token.text}'`);
    } else if (token.text === '-' || token.text === '+') {
      const number = watchNumber(this.expression(7));
      value = token.text === '-' ? -number : number;
    } else if (token.text === '(') {
      value = this.expression();
      this.expect(')');
    } else throw new SyntaxError(`expected a watch value, received '${token.text}'`);
    while (this.peek().text === '.' || this.peek().text === '[')
      if (this.take().text === '.') {
        const property = this.take();
        if (property.kind !== 'name') throw new SyntaxError('expected a field name after dot');
        value = readWatchProperty(value, property.text);
      } else {
        const key = this.expression();
        this.expect(']');
        value = readWatchProperty(value, key);
      }
    return value;
  }
  expect(text) {
    if (this.take().text !== text) throw new SyntaxError(`expected '${text}' in watch expression`);
  }
  peek() {
    return (
      this.tokens[this.cursor] ?? {
        kind: 'eof',
        text: '',
      }
    );
  }
  take() {
    const token = this.peek();
    this.cursor += 1;
    return token;
  }
};
var BINARY_PRECEDENCE = {
  or: 1,
  and: 2,
  '==': 3,
  '!=': 3,
  '<': 4,
  '<=': 4,
  '>': 4,
  '>=': 4,
  '+': 5,
  '-': 5,
  '*': 6,
  '/': 6,
  '%': 6,
};
function tokenizeWatch(source) {
  const tokens = [];
  let cursor = 0;
  while (cursor < source.length) {
    const character = source[cursor] ?? '';
    if (/\s/u.test(character)) {
      cursor += 1;
      continue;
    }
    const rest = source.slice(cursor);
    const number = /^(?:\d+(?:\.\d+)?|\.\d+)/u.exec(rest)?.[0];
    if (number !== void 0) {
      tokens.push({
        kind: 'number',
        text: number,
      });
      cursor += number.length;
      continue;
    }
    const name = /^[A-Za-z_][A-Za-z0-9_]*/u.exec(rest)?.[0];
    if (name !== void 0) {
      tokens.push({
        kind: 'name',
        text: name,
      });
      cursor += name.length;
      continue;
    }
    if (character === '"') {
      let end = cursor + 1;
      while (end < source.length)
        if (source[end] === '\\') end += 2;
        else if (source[end] === '"') break;
        else end += 1;
      if (source[end] !== '"') throw new SyntaxError('unterminated watch text');
      tokens.push({
        kind: 'text',
        text: source.slice(cursor, end + 1),
      });
      cursor = end + 1;
      continue;
    }
    const pair = rest.slice(0, 2);
    const operator = ['==', '!=', '<=', '>='].includes(pair)
      ? pair
      : '+-*/%<>().[]'.includes(character)
        ? character
        : void 0;
    if (operator === void 0) throw new SyntaxError(`unsupported watch token '${character}'`);
    tokens.push({
      kind: 'operator',
      text: operator,
    });
    cursor += operator.length;
  }
  tokens.push({
    kind: 'eof',
    text: '',
  });
  return tokens;
}
function binaryWatch(operator, left, right) {
  switch (operator) {
    case 'or':
      return truthy(left) || truthy(right);
    case 'and':
      return truthy(left) && truthy(right);
    case '==':
      return Object.is(left, right);
    case '!=':
      return !Object.is(left, right);
    case '<':
      return watchNumber(left) < watchNumber(right);
    case '<=':
      return watchNumber(left) <= watchNumber(right);
    case '>':
      return watchNumber(left) > watchNumber(right);
    case '>=':
      return watchNumber(left) >= watchNumber(right);
    case '+':
      return typeof left === 'string' && typeof right === 'string'
        ? left + right
        : watchNumber(left) + watchNumber(right);
    case '-':
      return watchNumber(left) - watchNumber(right);
    case '*':
      return watchNumber(left) * watchNumber(right);
    case '/':
      return watchNumber(left) / watchNumber(right);
    case '%':
      return watchNumber(left) % watchNumber(right);
    default:
      throw new SyntaxError(`unsupported watch operator '${operator}'`);
  }
}
function readWatchProperty(value, key) {
  if (typeof key !== 'string' && typeof key !== 'number')
    throw new TypeError('watch index must be a number or field name');
  if (key === '__proto__' || key === 'prototype' || key === 'constructor')
    throw new TypeError('watch access to host prototype fields is unavailable');
  if ((typeof value !== 'object' && typeof value !== 'string') || value === null)
    throw new TypeError('watch field access requires a record, collection, or text');
  if (typeof value === 'string') {
    if (key === 'length') return value.length;
    if (typeof key === 'number' && Number.isSafeInteger(key)) return value[key];
    throw new TypeError(`watch field '${String(key)}' is unavailable`);
  }
  if (!Object.hasOwn(value, key))
    throw new TypeError(`watch field '${String(key)}' is unavailable`);
  return value[key];
}
function watchNumber(value) {
  if (typeof value !== 'number' || !Number.isFinite(value))
    throw new TypeError('watch arithmetic requires finite numbers');
  return value;
}
function truthy(value) {
  if (typeof value !== 'boolean') throw new TypeError('watch logic requires Bool values');
  return value;
}
//#endregion
//#region src/native-host.ts
var runtime;
var audioPreview;
var scratchFresh = false;
var cartridgeId;
var symbolNames = {};
var input = createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});
function bytes(encoded) {
  if (typeof encoded !== 'string' || encoded.length > 25165824)
    throw new TypeError('invalid native host byte payload');
  const decoded = Uint8Array.from(Buffer.from(encoded, 'base64'));
  if (decoded.length > 16777216) throw new RangeError('native host byte payload exceeds 16 MiB');
  return decoded;
}
function encoded(value) {
  return Buffer.from(value.buffer, value.byteOffset, value.byteLength).toString('base64');
}
function respond(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}
function namedValues(value) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
  return Object.fromEntries(
    Object.entries(value).map(([key, entry]) => [symbolNames[key] ?? key, entry]),
  );
}
function evaluateExpressions(expressions, state, locals, booleanOnly = false) {
  if (expressions === void 0) return [];
  if (expressions.length > 32) throw new RangeError('too many native debugger expressions');
  const environment = {
    ...namedValues(state),
    ...namedValues(locals),
  };
  return expressions.map((expression) => {
    if (expression.length > 256) return { error: 'expression exceeds 256 characters' };
    if (expression.length === 0) return { value: true };
    try {
      const value = evaluateWatch(expression, environment);
      return booleanOnly && typeof value !== 'boolean'
        ? { error: 'breakpoint condition must produce Bool' }
        : { value };
    } catch (error) {
      return { error: error instanceof Error ? error.message : 'watch evaluation failed' };
    }
  });
}
async function handle(command) {
  switch (command.type) {
    case 'audio-preview-load':
      runtime = void 0;
      audioPreview = void 0;
      scratchFresh = false;
      cartridgeId = void 0;
      symbolNames = {};
      if (
        !Array.isArray(command.assets) ||
        command.assets.length > 4096 ||
        Buffer.byteLength(JSON.stringify(command.assets), 'utf8') > 16777216
      )
        throw new RangeError('audio preview requires at most 4096 assets and 16 MiB');
      audioPreview = new Synthesizer(new AudioAssetStore(command.assets));
      return { type: 'audio-preview-loaded' };
    case 'audio-preview-play':
      if (audioPreview === void 0) throw new Error('no audio preview is loaded');
      audioPreview.playMusic(command.music, command.orderIndex, command.row);
      return {
        type: 'audio-preview-playing',
        position: audioPreview.inspectTracker(),
      };
    case 'audio-preview-audition':
      if (audioPreview === void 0) throw new Error('no audio preview is loaded');
      audioPreview.audition(command.sound, command.note, command.volume);
      return { type: 'audio-preview-auditioned' };
    case 'audio-preview-frame': {
      if (audioPreview === void 0) throw new Error('no audio preview is loaded');
      const frame = audioPreview.snapshot().frame;
      const position = audioPreview.inspectTracker();
      const result = audioPreview.finishFrame();
      return {
        type: 'audio-preview-frame',
        frame,
        position,
        nextPosition: result.tracker,
        left: encoded(result.left),
        right: encoded(result.right),
        activeVoices: result.activeVoices,
        voices: audioPreview.inspectVoices(),
        peakLeft: result.left.reduce((peak, sample) => Math.max(peak, Math.abs(sample)), 0),
        peakRight: result.right.reduce((peak, sample) => Math.max(peak, Math.abs(sample)), 0),
      };
    }
    case 'render-wav': {
      const wav =
        command.kind === 'sound'
          ? renderSoundWav(command.asset)
          : renderMusicWav(command.asset, command.sounds);
      if (wav.byteLength > 8388608) throw new RangeError('native WAV preview exceeds 8 MiB');
      return {
        type: 'wav',
        bytes: encoded(wav),
      };
    }
    case 'load': {
      if (typeof command.javascript !== 'string' || command.javascript.length > 2097152)
        throw new TypeError('invalid compiled cartridge');
      runtime = void 0;
      audioPreview = void 0;
      scratchFresh = false;
      const module = await import(
        `data:text/javascript;base64,${Buffer.from(command.javascript).toString('base64')}`
      );
      if (typeof module !== 'object' || module === null || !('default' in module))
        throw new TypeError('compiled cartridge has no factory');
      const factory = module.default;
      if (typeof factory !== 'function')
        throw new TypeError('compiled cartridge factory is invalid');
      const files = Object.fromEntries(
        Object.entries(command.files).map(([path, value]) => [path, bytes(value)]),
      );
      const manifestAssets = command.manifest.assets;
      runtime = createConsoleRuntime(factory, {
        seed: command.seed === void 0 ? DEFAULT_SEED : command.seed,
        workUnitsPerFrame: HARDWARE.workUnitsPerFrame,
        updateRate: command.manifest.updateRate,
        hardwareRevision: command.manifest.hardwareRevision ?? 2,
        debug: command.debug === true,
        assets: {
          declarations: manifestAssets,
          files,
          displayPath: command.manifest.display,
        },
        rom: bytes(command.rom),
        save: bytes(command.save),
      });
      cartridgeId = command.manifest.id;
      symbolNames = command.symbolNames ?? {};
      scratchFresh = command.debug === true;
      return {
        type: 'loaded',
        cartridgeId,
        menuItems: runtime.inspectMenu(),
      };
    }
    case 'frame': {
      if (runtime === void 0) throw new Error('no cartridge is loaded');
      scratchFresh = false;
      const result = runtime.runFrame(command.input);
      return {
        type: 'frame',
        frame: result.frame,
        workUnits: result.workUnits,
        pixels: encoded(result.output.indexedPixels),
        left: encoded(result.output.audio.left),
        right: encoded(result.output.audio.right),
        saveCommit: result.saveCommit === void 0 ? null : encoded(result.saveCommit),
        debug: result.debug ?? null,
        menuItems: runtime.inspectMenu(),
      };
    }
    case 'scratch': {
      if (runtime === void 0) throw new Error('no scratchpad program is loaded');
      if (!scratchFresh) throw new TypeError('scratchpad requires a freshly loaded debug program');
      scratchFresh = false;
      let locals = {};
      let state = {};
      let booted = false;
      for (let step = 0; step < 65536; step++) {
        const result = runtime.stepDebug(command.input);
        if ('event' in result) {
          locals = namedValues(result.event.locals);
          state = namedValues(result.inspection.state);
        } else if ('booted' in result) {
          booted = true;
          break;
        } else throw new TypeError('scratchpad must start from a fresh debug program');
      }
      if (!booted)
        throw new RuntimeFault('M019105', 'scratchpad exceeds 65,536 statement boundaries', {
          start: 0,
          end: 0,
        });
      const work = runtime.inspectMemory(MEMORY.system + 32, 8).bytes;
      const bootWorkUnits = Number(
        new DataView(work.buffer, work.byteOffset, 8).getBigUint64(0, true),
      );
      const frame = runtime.runFrame(command.input);
      return {
        type: 'scratch',
        locals,
        state: frame.debug === void 0 ? state : namedValues(frame.debug.inspection.state),
        bootWorkUnits,
        workUnits: frame.workUnits,
        pixels: encoded(frame.output.indexedPixels),
        left: encoded(frame.output.audio.left),
        right: encoded(frame.output.audio.right),
      };
    }
    case 'step': {
      if (runtime === void 0) throw new Error('no cartridge is loaded');
      scratchFresh = false;
      const result = runtime.stepDebug(command.input);
      if ('frame' in result)
        return {
          type: 'step',
          frame: {
            frame: result.frame.frame,
            workUnits: result.frame.workUnits,
            pixels: encoded(result.frame.output.indexedPixels),
            left: encoded(result.frame.output.audio.left),
            right: encoded(result.frame.output.audio.right),
            saveCommit:
              result.frame.saveCommit === void 0 ? null : encoded(result.frame.saveCommit),
            menuItems: runtime.inspectMenu(),
          },
        };
      if ('booted' in result)
        return {
          type: 'step',
          ...result,
          menuItems: runtime.inspectMenu(),
        };
      return {
        type: 'step',
        ...result,
        menuItems: runtime.inspectMenu(),
        watches: evaluateExpressions(command.watches, result.inspection.state, result.event.locals),
        conditions: evaluateExpressions(
          command.conditions,
          result.inspection.state,
          result.event.locals,
          true,
        ),
      };
    }
    case 'snapshot':
      if (runtime === void 0) throw new Error('no cartridge is loaded');
      {
        const snapshot = runtime.snapshot();
        return {
          type: 'snapshot',
          snapshot: serialize(snapshot).toString('base64'),
          frame: snapshot.machine.frame,
          booted: snapshot.machine.execution.booted,
        };
      }
    case 'restore':
      if (runtime === void 0) throw new Error('no cartridge is loaded');
      scratchFresh = false;
      runtime.restore(deserialize(Buffer.from(command.snapshot, 'base64')));
      {
        const snapshot = runtime.snapshot();
        return {
          type: 'restored',
          frame: snapshot.machine.frame,
          booted: snapshot.machine.execution.booted,
          menuItems: runtime.inspectMenu(),
        };
      }
    case 'memory': {
      if (runtime === void 0) throw new Error('no cartridge is loaded');
      const result = runtime.inspectMemory(command.address, command.length);
      return {
        type: 'memory',
        address: result.address,
        bytes: encoded(result.bytes),
        regions: result.regions,
      };
    }
    case 'memory-edit':
      if (runtime === void 0) throw new Error('no cartridge is loaded');
      runtime.editMemory(command.address, bytes(command.bytes));
      return { type: 'memory-edited' };
    case 'stop':
      runtime = void 0;
      audioPreview = void 0;
      scratchFresh = false;
      cartridgeId = void 0;
      return { type: 'stopped' };
  }
}
var chain = Promise.resolve();
input.on('line', (line) => {
  chain = chain.then(async () => {
    try {
      respond(await handle(JSON.parse(line)));
    } catch (error) {
      respond({
        type: 'error',
        code: error instanceof RuntimeFault ? error.code : 'M019100',
        message: error instanceof Error ? error.message : 'native host failure',
        ...(error instanceof RuntimeFault ? { sourceSpan: error.sourceSpan } : {}),
      });
    }
  });
});
//#endregion
