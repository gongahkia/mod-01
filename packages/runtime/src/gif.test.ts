import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';

import { encodeIndexedGif } from './gif';

describe('deterministic indexed GIF capture', () => {
  it('encodes byte-identical animated 30 fps frames with bounded full images', () => {
    const frames = [Uint8Array.of(0, 1, 2, 3), Uint8Array.of(3, 2, 1, 0)];
    const first = encodeIndexedGif(frames, 2, 2);
    expect(encodeIndexedGif(frames, 2, 2)).toEqual(first);
    expect(new TextDecoder().decode(first.subarray(0, 6))).toBe('GIF89a');
    expect(first.at(-1)).toBe(0x3b);
    expect(createHash('sha256').update(first).digest('hex')).toBe(
      'be91301d85c0c8ba5c9f20ac0d37042224946f0e265bfd13eab744dd7871ef28',
    );
  });

  it('rejects oversize, malformed, and over-duration captures', () => {
    expect(() => encodeIndexedGif([], 2, 2)).toThrow(/limit/);
    expect(() => encodeIndexedGif([Uint8Array.of(0, 1)], 1, 1)).toThrow(/canonical/);
    expect(() =>
      encodeIndexedGif(
        Array.from({ length: 151 }, () => Uint8Array.of(0)),
        1,
        1,
      ),
    ).toThrow(/limit/);
  });

  it('encodes high palette indices with a 256-color table', () => {
    const gif = encodeIndexedGif([Uint8Array.of(255)], 1, 1);
    expect(gif[10]).toBe(0xf7);
    expect(gif[13 + 255 * 3]).toBeGreaterThan(0);
  });
});
