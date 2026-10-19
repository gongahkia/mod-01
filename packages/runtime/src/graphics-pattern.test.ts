import { describe, expect, it } from 'vitest';
import { IndexedGraphics, VisualAssetStore } from './graphics';
import { MEMORY, MemoryBus } from './bus';
import { consoleWorkCost, createConsoleRuntime } from './console-runtime';
import { emptyInputFrame } from './input';
import type { ConsoleCommand } from './protocol';

const span = { start: 2, end: 8 };
const command = (name: string, args: readonly unknown[]): ConsoleCommand => ({
  name,
  arguments: args,
  sourceSpan: span,
});

describe('primitive pattern state', () => {
  it('uses screen-anchored low-bit-first 4x4 masks, clipping and both palette remaps', () => {
    const frame = new IndexedGraphics().executeFrame([
      command('clear', [1]),
      command('camera', [3, 2]),
      command('clip', [1, 1, 3, 3]),
      command('pal', [9, 23]),
      command('pal', [11, 12]),
      command('fill_pattern', [0xa5a5, 9, false]),
      command('rect_fill', [3, 2, 4, 4, 11]),
    ]);
    for (let y = 0; y < 4; y++)
      for (let x = 0; x < 4; x++)
        expect(frame.indexedPixels[y * 240 + x]).toBe(
          x === 0 || y === 0 ? 1 : (0xa5a5 & (1 << (y * 4 + x))) === 0 ? 12 : 23,
        );
  });

  it('applies the second colour to every primitive, including outlines, but not clear/text/assets', () => {
    const graphics = new IndexedGraphics(
      new VisualAssetStore([
        { kind: 'sprite', name: 'dot', width: 1, height: 1, pixels: Uint8Array.of(3) },
      ]),
    );
    const shapes = [
      command('pixel', [0, 0, 3]),
      command('line', [1, 1, 4, 4, 3]),
      command('rect', [8, 1, 3, 3, 3]),
      command('rect_fill', [12, 1, 3, 3, 3]),
      command('circle', [18, 3, 2, 3]),
      command('circle_fill', [24, 3, 2, 3]),
      command('triangle', [28, 1, 31, 1, 28, 4, 3]),
    ];
    const coloured = graphics.executeFrame([
      command('clear', [1]),
      command('fill_pattern', [65535, 255, false]),
      ...shapes,
    ]);
    expect(coloured.indexedPixels).toContain(255);
    expect(coloured.indexedPixels.every((value) => value === 1 || value === 255)).toBe(true);
    const holes = graphics.executeFrame([
      command('fill_pattern', [65535, 255, true]),
      command('clear', [7]),
      ...shapes,
      command('print', ['A', 40, 0, 3]),
      command('sprite', [{ name: 'dot', kind: 'Sprite' }, 50, 0]),
    ]);
    expect(holes.indexedPixels[0]).toBe(7);
    expect(holes.indexedPixels[240 + 8]).toBe(7);
    expect(holes.indexedPixels[50]).toBe(3);
    expect(holes.indexedPixels[240 + 40]).toBe(3);
    expect(holes.indexedPixels).not.toContain(255);
  });

  it('shares validated pattern registers with the bus, snapshots, reset and one-unit work costs', () => {
    const graphics = new IndexedGraphics();
    const bus = new MemoryBus(graphics.memoryRegions(), () => {});
    graphics.beginFrame();
    graphics.executeCommand(command('fill_pattern', [0x8001, 255, true]));
    expect(bus.inspect(MEMORY.pattern, 8)).toEqual(Uint8Array.of(1, 128, 255, 1, 1, 0, 0, 0));
    const saved = bus.snapshot();
    for (const [offset, value] of [
      [3, 2],
      [4, 2],
      [5, 1],
    ]) {
      expect(() => {
        bus.write(MEMORY.pattern + (offset ?? 0), value ?? 0, 1, span);
      }).toThrow();
      expect(bus.snapshot()).toEqual(saved);
    }
    graphics.executeCommand(command('pal_reset', []));
    graphics.executeCommand(command('palt_reset', []));
    expect(bus.snapshot()).toEqual(saved);
    graphics.executeCommand(command('fill_pattern_reset', []));
    expect(bus.inspect(MEMORY.pattern, 8)).toEqual(new Uint8Array(8));
    bus.restore(saved);
    bus.write(MEMORY.pattern + 4, 0, 1, span);
    graphics.executeCommand(command('pixel', [0, 0, 3]));
    expect(graphics.finishFrame().indexedPixels[0]).toBe(255);
    graphics.beginFrame();
    expect(bus.inspect(MEMORY.pattern, 8)).toEqual(new Uint8Array(8));
    expect(consoleWorkCost('fill_pattern', [65535, 255, true])).toBe(1);
    expect(consoleWorkCost('fill_pattern_reset', [])).toBe(1);
    expect(() => {
      graphics.executeCommand({ ...command('fill_pattern_reset', []), rasterLine: 0 });
    }).toThrow();
  });

  it('rejects invalid masks before register mutation with an attributed runtime fault', () => {
    for (const mask of [-1, 65536, 0.5]) {
      const runtime = createConsoleRuntime(
        (api) => ({
          start() {},
          update() {},
          raster() {},
          draw() {
            api.call('fill_pattern', [mask, 255, false], span);
          },
          snapshot: () => ({ state: {}, tasks: [], nextTaskId: 1 }),
          restore() {},
          inspect: () => ({ state: {}, tasks: [], callStack: [] }),
        }),
        { seed: 1, updateRate: 60, workUnitsPerFrame: 50_000 },
      );
      expect(() => {
        runtime.runFrame(emptyInputFrame());
      }).toThrow(expect.objectContaining({ code: 'M019009', sourceSpan: span }));
      expect(
        runtime.snapshot().memory.regions.find((region) => region.address === MEMORY.pattern)
          ?.bytes,
      ).toEqual(new Uint8Array(8));
    }
  });
});
