import { deepStrictEqual } from 'node:assert/strict';
import { describe, expect, it } from 'vitest';
import { IndexedGraphics, VisualAssetStore, type IndexedSprite } from './graphics';
import { consoleWorkCost, createConsoleRuntime } from './console-runtime';
import { emptyInputFrame } from './input';
import type { ConsoleCommand } from './protocol';

const span = { start: 4, end: 10 };
const sprite: IndexedSprite = {
  kind: 'sprite',
  name: 'texture',
  width: 4,
  height: 2,
  pixels: Uint8Array.of(0, 1, 2, 3, 4, 5, 6, 7),
};
const handle = { kind: 'Sprite', name: 'texture' };
const command = (name: string, args: readonly unknown[]): ConsoleCommand => ({
  name,
  arguments: args,
  sourceSpan: span,
});
const graphics = () => new IndexedGraphics(new VisualAssetStore([sprite]));

describe('bounded affine texture lines', () => {
  it('covers the ordinary inclusive Bresenham pixels in every octant and reversed direction', () => {
    for (const dx of [-6, -3, 0, 3, 6])
      for (const dy of [-6, -3, 0, 3, 6]) {
        const textured = graphics().executeFrame([
          command('clear', [32]),
          command('texture_line', [handle, 10, 10, 10 + dx, 10 + dy, 1, 0, 0, 0, false]),
        ]);
        const solid = graphics().executeFrame([
          command('clear', [32]),
          command('line', [10, 10, 10 + dx, 10 + dy, 1]),
        ]);
        deepStrictEqual(textured.indexedPixels, solid.indexedPixels);
      }
  });

  it('samples negative/fractional coordinates, wrapping or skipping before transparency and remapping', () => {
    for (const repeat of [true, false])
      for (const reverse of [true, false]) {
        const frame = graphics().executeFrame([
          command('clear', [32]),
          command('palt', [0, false]),
          command('palt', [2, true]),
          command('pal', [1, 255]),
          command('fill_pattern', [65535, 7, true]),
          command('texture_line', [
            handle,
            reverse ? 7 : 0,
            0,
            reverse ? 0 : 7,
            0,
            -0.5,
            0,
            0.5,
            0,
            repeat,
          ]),
        ]);
        const expected = [repeat ? 3 : 32, 0, 0, 255, 255, 32, 32, 3];
        expect([...frame.indexedPixels.slice(0, 8)]).toEqual(
          reverse ? expected.reverse() : expected,
        );
      }
    const vertical = graphics().executeFrame([
      command('texture_line', [handle, 0, 0, 0, 3, 1, -1, 0, 1, true]),
    ]);
    expect([0, 1, 2, 3].map((y) => vertical.indexedPixels[y * 240])).toEqual([5, 1, 5, 1]);
    const diagonal = graphics().executeFrame([
      command('clear', [32]),
      command('palt', [0, false]),
      command('texture_line', [handle, 0, 0, 4, 2, 0, 0, 1, 0, true]),
    ]);
    expect(
      [
        [0, 0],
        [1, 1],
        [2, 1],
        [3, 2],
        [4, 2],
      ].map(([x, y]) => diagonal.indexedPixels[(y ?? 0) * 240 + (x ?? 0)]),
    ).toEqual([0, 1, 2, 3, 0]);
  });

  it('quantizes starts and deltas to 16 fractional bits before accumulation and does not restart sampling at clips', () => {
    const fixed = graphics().executeFrame([
      command('palt', [0, false]),
      command('texture_line', [handle, 0, 0, 4, 0, 0, 0, 1 / 3, 0, false]),
    ]);
    expect([...fixed.indexedPixels.slice(0, 5)]).toEqual([0, 0, 0, 0, 1]);
    const clipped = graphics().executeFrame([
      command('clear', [32]),
      command('clip', [1, 0, 2, 1]),
      command('texture_line', [handle, -2, 0, 5, 0, 0, 0, 1, 0, true]),
    ]);
    expect([...clipped.indexedPixels.slice(0, 5)]).toEqual([32, 3, 32, 32, 32]);
    const extreme = graphics().executeFrame([
      command('camera', [Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER]),
      command('texture_line', [
        handle,
        Number.MAX_SAFE_INTEGER - 3,
        Number.MAX_SAFE_INTEGER,
        Number.MAX_SAFE_INTEGER,
        Number.MAX_SAFE_INTEGER,
        0,
        0,
        1,
        0,
        false,
      ]),
    ]);
    expect(extreme.indexedPixels[0]).toBe(3);
  });

  it('bounds sampling, rejects nonfinite and out-of-range texture fields atomically and charges the full line', () => {
    expect(consoleWorkCost('texture_line', [handle, 0, 0, 239, 0, 0, 0, 1, 0, true])).toBe(122);
    expect(consoleWorkCost('texture_line', [handle, 0, 0, 0, 0, 0, 0, 1, 0, true])).toBe(3);
    for (const [index, value] of [
      [3, 4096],
      [5, Infinity],
      [5, NaN],
      [6, -32769],
      [7, 32769],
      [8, -Infinity],
      [9, 1],
    ] as const) {
      const args: unknown[] = [handle, 0, 0, 3, 0, 0, 0, 1, 0, false];
      args[index] = value;
      const g = graphics();
      g.executeFrame([command('clear', [7])]);
      expect(() => {
        g.executeFrame([command('texture_line', args)]);
      }).toThrow();
      expect(g.snapshot().front.every((pixel) => pixel === 7)).toBe(true);
    }
    const endpoint = graphics().executeFrame([
      command('texture_line', [handle, -4095, 0, 0, 0, 1, 0, 32768, 0, true]),
    ]);
    expect(endpoint.indexedPixels[0]).toBe(1);
    const runtime = createConsoleRuntime(
      (api) => ({
        start() {},
        update() {},
        raster() {},
        draw() {
          api.call('texture_line', [handle, 0, 0, 239, 0, 0, 0, 1, 0, true], span);
        },
        snapshot: () => ({ state: {}, tasks: [], nextTaskId: 1 }),
        restore() {},
        inspect: () => ({ state: {}, tasks: [], callStack: [] }),
      }),
      { seed: 1, updateRate: 60, workUnitsPerFrame: 100 },
    );
    expect(() => {
      runtime.runFrame(emptyInputFrame());
    }).toThrow(expect.objectContaining({ code: 'M019001', sourceSpan: span }));
    expect(runtime.snapshot().graphics.front.every((pixel) => pixel === 0)).toBe(true);
  });
});
