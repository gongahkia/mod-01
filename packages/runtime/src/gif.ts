import { HARDWARE, MASTER_PALETTE_RGBA } from './hardware';

const encoder = new TextEncoder();

/** Deterministic full-frame 30 fps GIF for at most five seconds of indexed console output. */
export function encodeIndexedGif(
  frames: readonly Uint8Array[],
  width: number = HARDWARE.width,
  height: number = HARDWARE.height,
): Uint8Array {
  if (
    !Number.isSafeInteger(width) ||
    !Number.isSafeInteger(height) ||
    width < 1 ||
    height < 1 ||
    width > HARDWARE.width ||
    height > HARDWARE.height ||
    frames.length < 1 ||
    frames.length > 150
  )
    throw new RangeError('GIF dimensions or 5-second frame limit are invalid');
  for (const frame of frames) {
    if (frame.length !== width * height || frame.some((color) => color >= HARDWARE.paletteSize))
      throw new TypeError('GIF frame is not canonical indexed MOD-01 output');
  }
  const paletteSize = frames.some((frame) => frame.some((color) => color >= 32)) ? 256 : 32;
  const codeSize = paletteSize === 256 ? 8 : 5;
  const chunks: Uint8Array[] = [];
  chunks.push(encoder.encode('GIF89a'));
  chunks.push(
    Uint8Array.of(
      width & 0xff,
      width >>> 8,
      height & 0xff,
      height >>> 8,
      paletteSize === 256 ? 0xf7 : 0xf4,
      0,
      0,
    ),
  );
  const palette = new Uint8Array(paletteSize * 3);
  for (let color = 0; color < paletteSize; color += 1) {
    palette[color * 3] = MASTER_PALETTE_RGBA[color * 4] ?? 0;
    palette[color * 3 + 1] = MASTER_PALETTE_RGBA[color * 4 + 1] ?? 0;
    palette[color * 3 + 2] = MASTER_PALETTE_RGBA[color * 4 + 2] ?? 0;
  }
  chunks.push(palette);
  chunks.push(Uint8Array.of(0x21, 0xff, 0x0b), encoder.encode('NETSCAPE2.0'));
  chunks.push(Uint8Array.of(3, 1, 0, 0, 0));
  for (const [index, frame] of frames.entries()) {
    const delay = index % 3 === 2 ? 4 : 3;
    chunks.push(Uint8Array.of(0x21, 0xf9, 4, 4, delay, 0, 0, 0));
    chunks.push(
      Uint8Array.of(
        0x2c,
        0,
        0,
        0,
        0,
        width & 0xff,
        width >>> 8,
        height & 0xff,
        height >>> 8,
        0,
        codeSize,
      ),
    );
    const compressed = literalLzw(frame, codeSize);
    for (let offset = 0; offset < compressed.length; offset += 255) {
      const block = compressed.subarray(offset, offset + 255);
      chunks.push(Uint8Array.of(block.length), block);
    }
    chunks.push(Uint8Array.of(0));
  }
  chunks.push(Uint8Array.of(0x3b));
  return concatenate(chunks);
}

function literalLzw(pixels: Uint8Array, minimumCodeSize: number): Uint8Array {
  const clear = 1 << minimumCodeSize;
  const end = clear + 1;
  const bits = minimumCodeSize + 1;
  const codes: number[] = [];
  for (let offset = 0; offset < pixels.length; offset += 30) {
    codes.push(clear);
    for (const pixel of pixels.subarray(offset, offset + 30)) codes.push(pixel);
  }
  codes.push(end);
  const output = new Uint8Array(Math.ceil((codes.length * bits) / 8));
  let bit = 0;
  for (const code of codes) {
    for (let index = 0; index < bits; index += 1) {
      if ((code & (1 << index)) !== 0) {
        const byte = Math.floor(bit / 8);
        output[byte] = (output[byte] ?? 0) | (1 << (bit % 8));
      }
      bit += 1;
    }
  }
  return output;
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
