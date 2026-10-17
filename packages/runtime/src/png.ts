import { glyphRows } from './font';
import { HARDWARE, MASTER_PALETTE_RGBA } from './hardware';

const SIGNATURE = Uint8Array.of(137, 80, 78, 71, 13, 10, 26, 10);
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });
const MAX_PNG_BYTES = 8 * 1024 * 1024;
const MAX_PIXELS = 4096 * 4096;

export interface DecodedPng {
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8Array;
}

export interface CartridgePngMetadata {
  readonly title: string;
  readonly author: string;
  readonly year: number;
  readonly players: number;
  readonly controls: string;
}

export interface DecodedCartridgePng {
  readonly cartridge: Uint8Array;
  readonly metadata: CartridgePngMetadata;
}

interface PngChunk {
  readonly type: string;
  readonly data: Uint8Array;
}

/** Dependency-free deterministic PNG encoder using filter 0 and stored DEFLATE blocks. */
export function encodeRgbaPng(
  width: number,
  height: number,
  rgba: Uint8Array,
  ancillary: readonly PngChunk[] = [],
): Uint8Array {
  validateDimensions(width, height);
  if (rgba.length !== width * height * 4) throw new RangeError('PNG RGBA byte length is invalid');
  const header = new Uint8Array(13);
  const headerView = new DataView(header.buffer);
  headerView.setUint32(0, width);
  headerView.setUint32(4, height);
  header.set([8, 6, 0, 0, 0], 8);
  const scanlines = new Uint8Array(height * (1 + width * 4));
  for (let y = 0; y < height; y += 1)
    scanlines.set(rgba.subarray(y * width * 4, (y + 1) * width * 4), y * (1 + width * 4) + 1);
  const chunks = [
    pngChunk('IHDR', header),
    ...ancillary.map((chunk) => {
      if (!/^[a-z][A-Za-z]{2}[a-z]$/.test(chunk.type))
        throw new TypeError('PNG ancillary chunk type is invalid');
      return pngChunk(chunk.type, chunk.data);
    }),
    pngChunk('IDAT', zlibStored(scanlines)),
    pngChunk('IEND', new Uint8Array()),
  ];
  return concatenate([SIGNATURE, ...chunks]);
}

/** Parses bounded, non-interlaced 8-bit RGB/RGBA/indexed PNG input into canonical RGBA. */
export async function decodePngRgba(bytes: Uint8Array): Promise<DecodedPng> {
  const parsed = parsePng(bytes);
  const header = parsed.chunks.find((chunk) => chunk.type === 'IHDR')?.data;
  if (header === undefined || header.length !== 13) throw new TypeError('PNG IHDR is missing');
  const view = new DataView(header.buffer, header.byteOffset, header.byteLength);
  const width = view.getUint32(0);
  const height = view.getUint32(4);
  validateDimensions(width, height);
  const bitDepth = header[8];
  const colorType = header[9];
  if (bitDepth !== 8 || ![2, 3, 6].includes(colorType ?? -1) || header[12] !== 0)
    throw new TypeError('PNG must be non-interlaced 8-bit RGB, RGBA, or indexed color');
  const bytesPerPixel = colorType === 6 ? 4 : colorType === 2 ? 3 : 1;
  const expected = height * (1 + width * bytesPerPixel);
  const compressed = concatenate(
    parsed.chunks.filter((chunk) => chunk.type === 'IDAT').map((chunk) => chunk.data),
  );
  if (compressed.length === 0) throw new TypeError('PNG IDAT is missing');
  const scanlines = await inflateBounded(compressed, expected);
  if (scanlines.length !== expected) throw new TypeError('PNG decompressed size is invalid');
  const unpacked = unfilter(scanlines, width, height, bytesPerPixel);
  const palette = parsed.chunks.find((chunk) => chunk.type === 'PLTE')?.data;
  const transparency = parsed.chunks.find((chunk) => chunk.type === 'tRNS')?.data;
  if (
    colorType === 3 &&
    (palette === undefined || palette.length === 0 || palette.length % 3 !== 0)
  )
    throw new TypeError('indexed PNG palette is invalid');
  const rgba = new Uint8Array(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const target = pixel * 4;
    if (colorType === 6) rgba.set(unpacked.subarray(pixel * 4, pixel * 4 + 4), target);
    else if (colorType === 2) {
      rgba.set(unpacked.subarray(pixel * 3, pixel * 3 + 3), target);
      rgba[target + 3] = 255;
    } else {
      const index = unpacked[pixel] ?? 0;
      if (palette === undefined || index * 3 + 2 >= palette.length)
        throw new TypeError('indexed PNG pixel exceeds its palette');
      rgba.set(palette.subarray(index * 3, index * 3 + 3), target);
      rgba[target + 3] = transparency?.[index] ?? 255;
    }
  }
  return { width, height, rgba };
}

export function convertRgbaToIndexed(
  image: DecodedPng,
  mode: 'nearest' | 'ordered',
  alphaThreshold = 127,
  transparentIndex = 0,
): Uint8Array {
  if (!Number.isSafeInteger(alphaThreshold) || alphaThreshold < 0 || alphaThreshold > 255)
    throw new RangeError('PNG alpha threshold must be 0-255');
  if (
    !Number.isSafeInteger(transparentIndex) ||
    transparentIndex < 0 ||
    transparentIndex >= HARDWARE.paletteSize
  )
    throw new RangeError('PNG transparent index must be 0-255');
  const output = new Uint8Array(image.width * image.height);
  const bayer = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5] as const;
  for (let pixel = 0; pixel < output.length; pixel += 1) {
    const offset = pixel * 4;
    if ((image.rgba[offset + 3] ?? 0) <= alphaThreshold) {
      output[pixel] = transparentIndex;
      continue;
    }
    const distances = Array.from({ length: HARDWARE.paletteSize }, (_, color) => ({
      color,
      distance: colorDistance(image.rgba, offset, color),
    })).sort((left, right) => left.distance - right.distance || left.color - right.color);
    const first = distances[0];
    const second = distances[1];
    if (first === undefined || second === undefined || mode === 'nearest' || first.distance === 0) {
      output[pixel] = first?.color ?? 0;
      continue;
    }
    const total = first.distance + second.distance;
    const secondShare = total === 0 ? 0 : Math.round((first.distance / total) * 16);
    const x = pixel % image.width;
    const y = Math.floor(pixel / image.width);
    output[pixel] = (bayer[(y % 4) * 4 + (x % 4)] ?? 0) < secondShare ? second.color : first.color;
  }
  return output;
}

/** Creates the MOD-01's own rectangular 1999 cartridge-object design and embeds canonical M01C. */
export function encodeCartridgePng(
  cartridge: Uint8Array,
  metadata: CartridgePngMetadata,
  frame?: DecodedPng,
): Uint8Array {
  if (cartridge.length === 0 || cartridge.length > HARDWARE.cartridgeCapacityBytes)
    throw new RangeError('embedded cartridge must fit the 256 KiB cartridge capacity');
  validateCartridgeMetadata(metadata);
  const width = 320;
  const height = 240;
  const rgba = new Uint8Array(width * height * 4);
  fillRgba(rgba, 0, 0, width, height, 2, width);
  fillRgba(rgba, 12, 8, 296, 224, 4, width);
  fillRgba(rgba, 20, 18, 280, 186, 1, width);
  fillRgba(rgba, 28, 26, 264, 162, 0, width);
  if (frame !== undefined) blitNearest(rgba, width, frame, 28, 26, 264, 162);
  fillRgba(rgba, 20, 194, 280, 30, 7, width);
  drawLabelText(rgba, width, metadata.title.toUpperCase(), 28, 199, 23, 42);
  drawLabelText(
    rgba,
    width,
    `${metadata.author.toUpperCase()} / ${String(metadata.year)}`,
    28,
    210,
    1,
    42,
  );
  drawLabelText(
    rgba,
    width,
    `${String(metadata.players)}P ${metadata.controls.toUpperCase()}`,
    190,
    210,
    1,
    18,
  );
  const metadataBytes = encoder.encode(JSON.stringify(metadata));
  if (metadataBytes.length > 2048) throw new RangeError('cartridge PNG metadata is too large');
  return encodeRgbaPng(width, height, rgba, [
    { type: 'moCm', data: metadataBytes },
    { type: 'moCa', data: cartridge },
  ]);
}

export function decodeCartridgePng(bytes: Uint8Array): DecodedCartridgePng {
  const parsed = parsePng(bytes);
  const cartridges = parsed.chunks.filter((chunk) => chunk.type === 'moCa');
  const metadataChunks = parsed.chunks.filter((chunk) => chunk.type === 'moCm');
  if (cartridges.length !== 1 || metadataChunks.length !== 1)
    throw new TypeError('cartridge PNG must contain one payload and one metadata chunk');
  const cartridge = cartridges[0]?.data;
  const metadataBytes = metadataChunks[0]?.data;
  if (
    cartridge === undefined ||
    cartridge.length === 0 ||
    cartridge.length > HARDWARE.cartridgeCapacityBytes ||
    metadataBytes === undefined ||
    metadataBytes.length > 2048
  )
    throw new RangeError('cartridge PNG payload is invalid');
  let metadata: unknown;
  try {
    metadata = JSON.parse(decoder.decode(metadataBytes));
  } catch {
    throw new TypeError('cartridge PNG metadata is invalid');
  }
  validateCartridgeMetadata(metadata);
  return { cartridge: cartridge.slice(), metadata };
}

function validateCartridgeMetadata(value: unknown): asserts value is CartridgePngMetadata {
  if (
    typeof value !== 'object' ||
    value === null ||
    Object.keys(value).length !== 5 ||
    !('title' in value) ||
    typeof value.title !== 'string' ||
    value.title.length < 1 ||
    value.title.length > 64 ||
    !('author' in value) ||
    typeof value.author !== 'string' ||
    value.author.length < 1 ||
    value.author.length > 64 ||
    !('year' in value) ||
    !Number.isSafeInteger(value.year) ||
    (value.year as number) < 1970 ||
    (value.year as number) > 9999 ||
    !('players' in value) ||
    !Number.isSafeInteger(value.players) ||
    (value.players as number) < 1 ||
    (value.players as number) > 4 ||
    !('controls' in value) ||
    typeof value.controls !== 'string' ||
    value.controls.length > 64
  )
    throw new TypeError('cartridge PNG metadata is invalid');
}

function parsePng(bytes: Uint8Array): { readonly chunks: readonly PngChunk[] } {
  if (bytes.length < SIGNATURE.length || bytes.length > MAX_PNG_BYTES)
    throw new RangeError('PNG file size is invalid');
  if (!SIGNATURE.every((byte, index) => bytes[index] === byte))
    throw new TypeError('PNG signature is invalid');
  const chunks: PngChunk[] = [];
  let offset = SIGNATURE.length;
  let ended = false;
  while (offset < bytes.length) {
    if (offset + 12 > bytes.length) throw new TypeError('PNG chunk is truncated');
    const view = new DataView(bytes.buffer, bytes.byteOffset + offset, bytes.length - offset);
    const length = view.getUint32(0);
    if (length > MAX_PNG_BYTES || offset + 12 + length > bytes.length)
      throw new RangeError('PNG chunk length is invalid');
    const typeBytes = bytes.subarray(offset + 4, offset + 8);
    const type = String.fromCharCode(...typeBytes);
    if (!/^[A-Za-z]{4}$/.test(type)) throw new TypeError('PNG chunk type is invalid');
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    const expectedCrc = view.getUint32(8 + length);
    if (crc32(concatenate([typeBytes, data])) !== expectedCrc)
      throw new TypeError('PNG chunk CRC mismatch');
    if (ended) throw new TypeError('PNG contains bytes after IEND');
    chunks.push({ type, data });
    offset += 12 + length;
    if (type === 'IEND') ended = true;
  }
  if (!ended || chunks[0]?.type !== 'IHDR') throw new TypeError('PNG chunk order is invalid');
  const critical = chunks.filter((chunk) => /^[A-Z]/.test(chunk.type));
  if (critical.some((chunk) => !['IHDR', 'PLTE', 'IDAT', 'IEND'].includes(chunk.type)))
    throw new TypeError('PNG contains an unsupported critical chunk');
  return { chunks };
}

function pngChunk(type: string, data: Uint8Array): Uint8Array {
  const typeBytes = encoder.encode(type);
  const output = new Uint8Array(12 + data.length);
  const view = new DataView(output.buffer);
  view.setUint32(0, data.length);
  output.set(typeBytes, 4);
  output.set(data, 8);
  view.setUint32(8 + data.length, crc32(concatenate([typeBytes, data])));
  return output;
}

function zlibStored(bytes: Uint8Array): Uint8Array {
  const blocks = Math.max(1, Math.ceil(bytes.length / 65_535));
  const output = new Uint8Array(2 + bytes.length + blocks * 5 + 4);
  output.set([0x78, 0x01]);
  let source = 0;
  let target = 2;
  for (let block = 0; block < blocks; block += 1) {
    const length = Math.min(65_535, bytes.length - source);
    output[target] = block === blocks - 1 ? 1 : 0;
    output[target + 1] = length & 0xff;
    output[target + 2] = length >>> 8;
    const complement = 0xffff ^ length;
    output[target + 3] = complement & 0xff;
    output[target + 4] = complement >>> 8;
    output.set(bytes.subarray(source, source + length), target + 5);
    source += length;
    target += 5 + length;
  }
  new DataView(output.buffer).setUint32(output.length - 4, adler32(bytes));
  return output;
}

async function inflateBounded(compressed: Uint8Array, expected: number): Promise<Uint8Array> {
  const stream = new Blob([new Uint8Array(compressed).buffer])
    .stream()
    .pipeThrough(new DecompressionStream('deflate'));
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  for (;;) {
    const result = await reader.read();
    if (result.done) break;
    length += result.value.length;
    if (length > expected) {
      await reader.cancel();
      throw new RangeError('PNG decompressed data exceeds its declared dimensions');
    }
    chunks.push(result.value);
  }
  return concatenate(chunks);
}

function unfilter(bytes: Uint8Array, width: number, height: number, bpp: number): Uint8Array {
  const stride = width * bpp;
  const output = new Uint8Array(height * stride);
  for (let y = 0; y < height; y += 1) {
    const source = y * (stride + 1);
    const filter = bytes[source];
    if (filter === undefined || filter > 4) throw new TypeError('PNG scanline filter is invalid');
    for (let x = 0; x < stride; x += 1) {
      const raw = bytes[source + 1 + x] ?? 0;
      const left = x >= bpp ? (output[y * stride + x - bpp] ?? 0) : 0;
      const above = y > 0 ? (output[(y - 1) * stride + x] ?? 0) : 0;
      const upperLeft = y > 0 && x >= bpp ? (output[(y - 1) * stride + x - bpp] ?? 0) : 0;
      const predictor =
        filter === 0
          ? 0
          : filter === 1
            ? left
            : filter === 2
              ? above
              : filter === 3
                ? Math.floor((left + above) / 2)
                : paeth(left, above, upperLeft);
      output[y * stride + x] = (raw + predictor) & 0xff;
    }
  }
  return output;
}

function paeth(left: number, above: number, upperLeft: number): number {
  const estimate = left + above - upperLeft;
  const leftDistance = Math.abs(estimate - left);
  const aboveDistance = Math.abs(estimate - above);
  const upperLeftDistance = Math.abs(estimate - upperLeft);
  return leftDistance <= aboveDistance && leftDistance <= upperLeftDistance
    ? left
    : aboveDistance <= upperLeftDistance
      ? above
      : upperLeft;
}

function validateDimensions(width: number, height: number): void {
  if (
    !Number.isSafeInteger(width) ||
    !Number.isSafeInteger(height) ||
    width < 1 ||
    height < 1 ||
    width > 4096 ||
    height > 4096 ||
    width * height > MAX_PIXELS
  )
    throw new RangeError('PNG dimensions are invalid');
}

function colorDistance(rgba: Uint8Array, offset: number, color: number): number {
  const paletteOffset = color * 4;
  const red = (rgba[offset] ?? 0) - (MASTER_PALETTE_RGBA[paletteOffset] ?? 0);
  const green = (rgba[offset + 1] ?? 0) - (MASTER_PALETTE_RGBA[paletteOffset + 1] ?? 0);
  const blue = (rgba[offset + 2] ?? 0) - (MASTER_PALETTE_RGBA[paletteOffset + 2] ?? 0);
  return red * red + green * green + blue * blue;
}

function fillRgba(
  rgba: Uint8Array,
  x: number,
  y: number,
  width: number,
  height: number,
  color: number,
  stride: number,
): void {
  const palette = color * 4;
  for (let row = y; row < y + height; row += 1) {
    for (let column = x; column < x + width; column += 1) {
      const offset = (row * stride + column) * 4;
      rgba[offset] = MASTER_PALETTE_RGBA[palette] ?? 0;
      rgba[offset + 1] = MASTER_PALETTE_RGBA[palette + 1] ?? 0;
      rgba[offset + 2] = MASTER_PALETTE_RGBA[palette + 2] ?? 0;
      rgba[offset + 3] = MASTER_PALETTE_RGBA[palette + 3] ?? 255;
    }
  }
}

function blitNearest(
  target: Uint8Array,
  targetWidth: number,
  image: DecodedPng,
  left: number,
  top: number,
  width: number,
  height: number,
): void {
  for (let y = 0; y < height; y += 1) {
    const sourceY = Math.floor((y * image.height) / height);
    for (let x = 0; x < width; x += 1) {
      const sourceX = Math.floor((x * image.width) / width);
      const source = (sourceY * image.width + sourceX) * 4;
      target.set(image.rgba.subarray(source, source + 4), ((top + y) * targetWidth + left + x) * 4);
    }
  }
}

function drawLabelText(
  rgba: Uint8Array,
  stride: number,
  text: string,
  left: number,
  top: number,
  color: number,
  maximum: number,
): void {
  for (const [index, character] of Array.from(text.slice(0, maximum)).entries()) {
    const rows = glyphRows(character);
    for (const [y, bits] of rows.entries()) {
      for (let x = 0; x < 5; x += 1) {
        if ((bits & (1 << (4 - x))) !== 0)
          fillRgba(rgba, left + index * 6 + x, top + y, 1, 1, color, stride);
      }
    }
  }
}

function concatenate(chunks: readonly Uint8Array[]): Uint8Array {
  const output = new Uint8Array(chunks.reduce((total, chunk) => total + chunk.length, 0));
  let offset = 0;
  for (const chunk of chunks) {
    output.set(chunk, offset);
    offset += chunk.length;
  }
  return output;
}

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function adler32(bytes: Uint8Array): number {
  let first = 1;
  let second = 0;
  for (const byte of bytes) {
    first = (first + byte) % 65_521;
    second = (second + first) % 65_521;
  }
  return ((second << 16) | first) >>> 0;
}
