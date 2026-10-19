import { describe, expect, it } from 'vitest';
import { deepStrictEqual } from 'node:assert/strict';
import { IndexedGraphics, VisualAssetStore, type IndexedSprite } from './graphics';
import { consoleWorkCost, createConsoleRuntime } from './console-runtime';
import { emptyInputFrame } from './input';
import type { ConsoleCommand } from './protocol';

const span = { start: 2, end: 8 };
const sprite: IndexedSprite = {
  kind: 'sprite',
  name: 'texture',
  width: 4,
  height: 3,
  pixels: Uint8Array.from({ length: 12 }, (_, index) => index),
};
const handle = { kind: 'Sprite', name: 'texture' };
const command = (name: string, args: readonly unknown[]): ConsoleCommand => ({
  name,
  arguments: args,
  sourceSpan: span,
});
const graphics = () => new IndexedGraphics(new VisualAssetStore([sprite]));

describe('source/destination region blits', () => {
  it('matches independent nearest sampling for clipped source rectangles, stretching, shrinking and both flips', () => {
    for (const flipX of [false, true])
      for (const flipY of [false, true]) {
        for (let sample = 0; sample < 45; sample++) {
          const sx = (sample % 5) - 2,
            sy = (sample % 3) - 1;
          const sw = (sample % 4) + 1,
            sh = (sample % 3) + 1;
          const dw = (sample % 7) + 1,
            dh = (sample % 5) + 1;
          const dx = (sample % 6) - 2,
            dy = (sample % 4) - 1;
          const frame = graphics().executeFrame([
            command('clear', [32]),
            command('palt', [0, false]),
            command('camera', [1, 1]),
            command('clip', [1, 1, 5, 5]),
            command('sprite_region', [handle, sx, sy, sw, sh, dx, dy, dw, dh, flipX, flipY]),
          ]);
          const expected = new Uint8Array(240 * 144).fill(32);
          for (let y = 0; y < dh; y++)
            for (let x = 0; x < dw; x++) {
              const screenX = dx + x - 1,
                screenY = dy + y - 1;
              if (screenX < 1 || screenX >= 6 || screenY < 1 || screenY >= 6) continue;
              const u =
                sx + (flipX ? sw - 1 - Math.trunc((x * sw) / dw) : Math.trunc((x * sw) / dw));
              const v =
                sy + (flipY ? sh - 1 - Math.trunc((y * sh) / dh) : Math.trunc((y * sh) / dh));
              if (u >= 0 && u < 4 && v >= 0 && v < 3)
                expected[screenY * 240 + screenX] = sprite.pixels[v * 4 + u] ?? 0;
            }
          deepStrictEqual(frame.indexedPixels, expected);
        }
      }
  });

  it('shares transparency/remaps, ignores fill patterns and retains exact camera-relative pixels near the Int boundary', () => {
    const frame = graphics().executeFrame([
      command('clear', [32]),
      command('palt', [0, false]),
      command('palt', [1, true]),
      command('pal', [2, 255]),
      command('fill_pattern', [65535, 7, true]),
      command('camera', [Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER]),
      command('sprite_region', [
        handle,
        0,
        0,
        4,
        1,
        Number.MAX_SAFE_INTEGER,
        Number.MAX_SAFE_INTEGER,
        4,
        1,
        false,
        false,
      ]),
    ]);
    expect([...frame.indexedPixels.slice(0, 4)]).toEqual([0, 32, 255, 3]);
    const outside = graphics().executeFrame([
      command('clear', [7]),
      command('palt', [0, false]),
      command('sprite_region', [handle, -1, 0, 4, 1, 0, 0, 4, 1, false, false]),
    ]);
    expect([...outside.indexedPixels.slice(0, 4)]).toEqual([7, 0, 1, 2]);
  });

  it('bounds geometry, treats nonpositive extents as empty, and charges destination area before drawing', () => {
    expect(consoleWorkCost('sprite_region', [handle, 0, 0, 4, 3, 0, 0, 12, 8, false, false])).toBe(
      24,
    );
    for (const index of [3, 4, 7, 8])
      for (const value of [0, -1]) {
        const args: unknown[] = [handle, 0, 0, 4, 3, 0, 0, 12, 8, false, false];
        args[index] = value;
        expect(
          graphics()
            .executeFrame([command('sprite_region', args)])
            .indexedPixels.every((pixel) => pixel === 0),
        ).toBe(true);
        expect(consoleWorkCost('sprite_region', args)).toBe(1);
      }
    for (const index of [3, 4, 7, 8, 9]) {
      const args: unknown[] = [handle, 0, 0, 4, 3, 0, 0, 12, 8, false, false];
      args[index] = index < 5 ? 129 : index < 9 ? 2049 : 1;
      expect(() => {
        graphics().executeFrame([command('sprite_region', args)]);
      }).toThrow();
    }
    const runtime = createConsoleRuntime(
      (api) => ({
        start() {},
        update() {},
        raster() {},
        draw() {
          api.call('sprite_region', [handle, 0, 0, 4, 3, 0, 0, 2048, 2048, false, false], span);
        },
        snapshot: () => ({ state: {}, tasks: [], nextTaskId: 1 }),
        restore() {},
        inspect: () => ({ state: {}, tasks: [], callStack: [] }),
      }),
      { seed: 1, updateRate: 60, workUnitsPerFrame: 50_000 },
    );
    expect(() => {
      runtime.runFrame(emptyInputFrame());
    }).toThrow(expect.objectContaining({ code: 'M019001', sourceSpan: span }));
    expect(runtime.snapshot().graphics.front.every((value) => value === 0)).toBe(true);
  });
});
