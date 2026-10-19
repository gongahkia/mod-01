import { deepStrictEqual } from 'node:assert/strict';
import { describe, expect, it, vi } from 'vitest';

import {
  IndexedGraphics,
  VisualAssetStore,
  orderedDither,
  type IndexedMap,
  type IndexedFont,
  type IndexedSprite,
  type IndexedTileSet,
} from './graphics';
import { HARDWARE, MASTER_PALETTE } from './hardware';
import { MEMORY, MemoryBus } from './bus';
import type { ConsoleCommand } from './protocol';

const sourceSpan = { start: 0, end: 1 };

function command(
  name: string,
  arguments_: readonly unknown[],
  rasterLine?: number,
): ConsoleCommand {
  return {
    name,
    arguments: arguments_,
    sourceSpan,
    ...(rasterLine === undefined ? {} : { rasterLine }),
  };
}

describe('indexed graphics hardware', () => {
  it('masks any of the 256 original blit colours before remapping and resets every frame', () => {
    const sprite: IndexedSprite = {
      kind: 'sprite',
      name: 'colors',
      width: 128,
      height: 2,
      pixels: Uint8Array.from({ length: 256 }, (_, index) => index),
    };
    const graphics = new IndexedGraphics(
      new VisualAssetStore([sprite, { kind: 'animation', name: 'motion', frames: [sprite] }]),
    );
    const handle = { name: 'colors', kind: 'Sprite' };
    graphics.beginFrame();
    graphics.executeCommand(command('clear', [32]));
    graphics.executeCommand(command('palt', [0, false]));
    for (let color = 1; color < 256; color += 2)
      graphics.executeCommand(command('palt', [color, true]));
    graphics.executeCommand(command('pal', [2, 7]));
    graphics.executeCommand(command('pal', [3, 8]));
    graphics.executeCommand(command('palt', [7, true]));
    graphics.executeCommand(command('sprite', [handle, 0, 0]));
    graphics.executeCommand(command('animation', [{ name: 'motion', kind: 'Animation' }, 0, 0, 4]));
    graphics.executeCommand(command('sprite_xform', [handle, 0, 8, 1, 0, false, false]));
    graphics.executeCommand(command('pixel', [200, 0, 7]));
    const frame = graphics.finishFrame();
    for (let offsetY of [0, 4, 8])
      for (let color = 0; color < 256; color++)
        expect(frame.indexedPixels[(offsetY + Math.floor(color / 128)) * 240 + (color % 128)]).toBe(
          color % 2 === 1 ? 32 : color === 2 ? 7 : color,
        );
    expect(frame.indexedPixels[200]).toBe(7); // solid primitives ignore the blit mask
    const reset = graphics.executeFrame([
      command('clear', [32]),
      command('sprite', [handle, 0, 0]),
    ]);
    expect(reset.indexedPixels[0]).toBe(32);
    expect(reset.indexedPixels[1]).toBe(1);
    expect(reset.indexedPixels[2]).toBe(2);
  });

  it('shares the complete mask with writable memory, preserves it in bus snapshots and resets explicitly', () => {
    const sprite: IndexedSprite = {
      kind: 'sprite',
      name: 'dot',
      width: 1,
      height: 1,
      pixels: Uint8Array.of(255),
    };
    const graphics = new IndexedGraphics(new VisualAssetStore([sprite]));
    const bus = new MemoryBus(graphics.memoryRegions(), () => {});
    graphics.beginFrame();
    expect(bus.read(MEMORY.transparency, 1, sourceSpan)).toBe(1);
    bus.write(MEMORY.transparency + 31, 128, 1, sourceSpan);
    const saved = bus.snapshot();
    graphics.executeCommand(command('palt_reset', []));
    expect(bus.read(MEMORY.transparency + 31, 1, sourceSpan)).toBe(0);
    bus.restore(saved);
    graphics.executeCommand(command('clear', [7]));
    graphics.executeCommand(command('sprite', [{ name: 'dot', kind: 'Sprite' }, 0, 0]));
    expect(graphics.finishFrame().indexedPixels[0]).toBe(7);
    graphics.beginFrame();
    expect(bus.read(MEMORY.transparency + 31, 1, sourceSpan)).toBe(0);
    for (const args of [
      [256, true],
      [-1, true],
      [1, 1],
      [1, true, false],
    ])
      expect(() => {
        graphics.executeCommand(command('palt', args));
      }).toThrow();
    expect(bus.read(MEMORY.transparency, 1, sourceSpan)).toBe(1);
    expect(() => {
      bus.write(MEMORY.transparency, 0, 1, sourceSpan, true);
    }).toThrow();
  });

  it('executes ordered commands before publishing scanout and resets frame-local state', () => {
    const graphics = new IndexedGraphics();
    const calls = [
      command('clear', [1]),
      command('pal', [2, 11]),
      command('pixel', [0, 0, 2]),
      command('pal', [11, 23], 0),
    ];
    expect(() => {
      graphics.executeCommand(command('clear', [1]));
    }).toThrow(/not begun/);
    graphics.beginFrame();
    for (const call of calls) graphics.executeCommand(call);
    expect(graphics.snapshot().front[0]).toBe(0);
    const result = graphics.finishFrame();
    deepStrictEqual(result, new IndexedGraphics().executeFrame(calls));
    expect(result.indexedPixels[0]).toBe(23);
    expect(graphics.snapshot().front[0]).toBe(11);
    expect(() => graphics.finishFrame()).toThrow(/not begun/);
    graphics.beginFrame();
    graphics.executeCommand(command('pixel', [0, 0, 2]));
    expect(graphics.finishFrame().indexedPixels[0]).toBe(2);
    const snapshot = graphics.snapshot();
    graphics.beginFrame();
    graphics.executeCommand(command('clear', [7]));
    graphics.restore(snapshot);
    expect(() => graphics.finishFrame()).toThrow(/not begun/);
    deepStrictEqual(graphics.snapshot(), snapshot);
  });

  it('uses one immutable, unique 256-colour master palette', () => {
    expect(MASTER_PALETTE).toHaveLength(HARDWARE.paletteSize);
    expect(new Set(MASTER_PALETTE)).toHaveLength(HARDWARE.paletteSize);
    expect(Object.isFrozen(MASTER_PALETTE)).toBe(true);
    expect(MASTER_PALETTE[25]).toBe('#345581');
    expect(MASTER_PALETTE[32]).toBe('#000000');
    expect(MASTER_PALETTE[247]).toBe('#ffffff');
    expect(MASTER_PALETTE[255]).toBe('#e5e5e5');
  });

  it('rasterizes primitives with camera, clipping, and logical palette remapping', () => {
    const graphics = new IndexedGraphics();
    const frame = graphics.executeFrame([
      command('clear', [1]),
      command('pal', [2, 11]),
      command('camera', [4, 3]),
      command('clip', [2, 2, 8, 8]),
      command('pixel', [6, 5, 2]),
      command('line', [6, 6, 9, 6, 3]),
      command('rect_fill', [6, 7, 2, 2, 4]),
      command('circle_fill', [10, 9, 1, 5]),
      command('triangle', [6, 10, 8, 10, 6, 12, 6]),
    ]);
    expect(frame.commands).toBe(9);
    expect(frame.indexedPixels[2 * HARDWARE.width + 2]).toBe(11);
    expect(frame.indexedPixels[3 * HARDWARE.width + 4]).toBe(3);
    expect(frame.indexedPixels[4 * HARDWARE.width + 2]).toBe(4);
    expect(frame.indexedPixels[6 * HARDWARE.width + 6]).toBe(5);
    expect(frame.indexedPixels[7 * HARDWARE.width + 2]).toBe(6);
    expect(frame.indexedPixels[0]).toBe(1);
  });

  it('applies scanline scroll and palette state only during display resolution', () => {
    const graphics = new IndexedGraphics();
    const frame = graphics.executeFrame([
      command('clear', [1]),
      command('pixel', [1, 0, 3]),
      command('raster_scroll', [1, 0], 0),
      command('pal', [3, 12], 0),
    ]);
    expect(frame.indexedPixels[0]).toBe(12);
    expect(frame.indexedPixels[HARDWARE.width]).toBe(1);

    const persistent = graphics.executeFrame([]);
    expect(persistent.indexedPixels[1]).toBe(3);
  });

  it('restores persistent and resolved framebuffer state', () => {
    const graphics = new IndexedGraphics();
    graphics.executeFrame([command('clear', [3]), command('pal', [3, 12], 0)]);
    const snapshot = graphics.snapshot();
    graphics.executeFrame([command('clear', [7])]);
    graphics.restore(snapshot);

    deepStrictEqual(graphics.snapshot(), snapshot);
    expect(graphics.executeFrame([]).indexedPixels[0]).toBe(3);
  });

  it('draws transparent sprites, transforms, tile maps, and bounded tile flags', () => {
    const sprite: IndexedSprite = {
      kind: 'sprite',
      name: 'hero',
      width: 2,
      height: 2,
      pixels: Uint8Array.of(2, 3, 0, 4),
    };
    const tile: IndexedSprite = {
      kind: 'sprite',
      name: 'ground-tile',
      width: 8,
      height: 8,
      pixels: new Uint8Array(64).fill(6),
    };
    const tileSet: IndexedTileSet = {
      kind: 'tile_set',
      name: 'ground',
      tiles: [tile],
      flags: Uint8Array.of(0b10),
    };
    const map: IndexedMap = {
      kind: 'map',
      name: 'level',
      layers: [{ width: 1, height: 1, cells: Uint16Array.of(0), tileSet: 'ground' }],
    };
    const assets = new VisualAssetStore([sprite, tileSet, map]);
    const graphics = new IndexedGraphics(assets);
    const frame = graphics.executeFrame([
      command('clear', [1]),
      command('sprite', [{ name: 'hero', kind: 'Sprite' }, 1, 1]),
      command('sprite_xform', [{ name: 'hero', kind: 'Sprite' }, 4, 1, 2, 1, false, false]),
      command('map', [{ name: 'level', kind: 'Map' }, 10, 1]),
    ]);
    expect(frame.indexedPixels[1 * HARDWARE.width + 1]).toBe(2);
    expect(frame.indexedPixels[2 * HARDWARE.width + 1]).toBe(1);
    expect(frame.indexedPixels[3 * HARDWARE.width + 4]).toBe(4);
    expect(frame.indexedPixels[1 * HARDWARE.width + 10]).toBe(6);
    expect(assets.mapCell('level', 0, 0, 0)).toBe(0);
    expect(assets.mapCell('level', 0, 1, 0)).toBeUndefined();
    expect(assets.mapFlag('level', 0, 0, 0, 1)).toBe(true);
    expect(
      graphics.executeFrame([
        command('clear', [1]),
        command('palt', [6, true]),
        command('map', [{ name: 'level', kind: 'Map' }, 0, 0]),
      ]).indexedPixels[0],
    ).toBe(1);
  });

  it('reads only camera-visible map cells while preserving visible tile output', () => {
    const tile: IndexedSprite = {
      kind: 'sprite',
      name: 'ground-tile',
      width: 8,
      height: 8,
      pixels: new Uint8Array(64).fill(6),
    };
    const tileSet: IndexedTileSet = {
      kind: 'tile_set',
      name: 'ground',
      tiles: [tile],
      flags: Uint8Array.of(0),
    };
    const map: IndexedMap = {
      kind: 'map',
      name: 'wide-level',
      layers: [{ width: 40, height: 1, cells: new Uint16Array(40), tileSet: 'ground' }],
    };
    const store = new VisualAssetStore([tileSet, map]);
    const stored = store.get('wide-level');
    if (stored?.kind !== 'map' || stored.layers[0] === undefined) throw new Error('missing map');
    const cells = stored.layers[0].cells;
    const readCell = cells.getUint16.bind(cells);
    const reads = vi.spyOn(cells, 'getUint16').mockImplementation((offset, littleEndian) => {
      if (offset < 60) throw new Error('renderer read a fully off-camera map cell');
      return readCell(offset, littleEndian);
    });
    const graphics = new IndexedGraphics(store);

    const frame = graphics.executeFrame([
      command('clear', [1]),
      command('camera', [240, 0]),
      command('map', [{ name: 'wide-level', kind: 'Map' }, 0, 0]),
    ]);

    expect(frame.indexedPixels[0]).toBe(6);
    expect(frame.indexedPixels[79]).toBe(6);
    expect(frame.indexedPixels[80]).toBe(1);
    expect(reads).toHaveBeenCalledTimes(10);
    reads.mockRestore();
  });

  it('enforces asset, sprite, palette, transform, and command limits', () => {
    const maximum = new Uint8Array(HARDWARE.spriteMaximumAxis ** 2);
    const assets = Array.from({ length: 65 }, (_, index): IndexedSprite => ({
      kind: 'sprite',
      name: `sprite-${String(index)}`,
      width: HARDWARE.spriteMaximumAxis,
      height: HARDWARE.spriteMaximumAxis,
      pixels: maximum,
    }));
    expect(() => new VisualAssetStore(assets)).toThrow(/1 MiB/);
    expect(() => new IndexedGraphics().executeFrame([command('clear', [256])])).toThrow(
      /0 and 255/,
    );
    expect(() =>
      new IndexedGraphics(
        new VisualAssetStore([
          { kind: 'sprite', name: 'dot', width: 1, height: 1, pixels: Uint8Array.of(1) },
        ]),
      ).executeFrame([
        command('sprite_xform', [{ name: 'dot', kind: 'Sprite' }, 0, 0, 17, 0, false, false]),
      ]),
    ).toThrow(/between 1 and 16/);
    expect(() =>
      new IndexedGraphics().executeFrame(
        Array.from({ length: HARDWARE.drawCommandsPerFrame + 1 }, () =>
          command('pixel', [0, 0, 1]),
        ),
      ),
    ).toThrow(/ceiling/);
  });

  it('provides deterministic ordered dithering', () => {
    const pattern = Array.from({ length: 16 }, (_, index) =>
      orderedDither(index % 4, Math.floor(index / 4), 1, 2, 8),
    );
    expect(pattern.filter((color) => color === 2)).toHaveLength(8);
    expect(pattern).toEqual([2, 1, 2, 1, 1, 2, 1, 2, 2, 1, 2, 1, 1, 2, 1, 2]);
  });

  it('renders the built-in ASCII bitmap font into indexed pixels', () => {
    const frame = new IndexedGraphics().executeFrame([
      command('clear', [0]),
      command('print', ['A!', 0, 0, 7]),
    ]);
    expect(frame.indexedPixels[1]).toBe(7);
    expect(frame.indexedPixels[2]).toBe(7);
    expect(frame.indexedPixels[3]).toBe(7);
    expect(frame.indexedPixels[6 + 2]).toBe(7);
    expect(frame.indexedPixels[6 * HARDWARE.width + 6 + 2]).toBe(7);
  });

  it('renders custom bitmap fonts with newlines and fallback glyphs', () => {
    const font: IndexedFont = {
      kind: 'font',
      name: 'tiny',
      glyphWidth: 2,
      glyphHeight: 2,
      baseline: 1,
      advanceX: 3,
      advanceY: 4,
      missingGlyph: 63,
      glyphs: new Map([
        [63, Uint8Array.of(1, 1, 0, 1)],
        [65, Uint8Array.of(1, 0, 1, 1)],
      ]),
    };
    const frame = new IndexedGraphics(new VisualAssetStore([font])).executeFrame([
      command('clear', [0]),
      command('font_print', [{ kind: 'Font', name: 'tiny' }, 'AZ\nA', 1, 2, 9]),
    ]);
    expect(frame.indexedPixels[2 * HARDWARE.width + 1]).toBe(9);
    expect(frame.indexedPixels[3 * HARDWARE.width + 2]).toBe(9);
    expect(frame.indexedPixels[2 * HARDWARE.width + 4]).toBe(9);
    expect(frame.indexedPixels[3 * HARDWARE.width + 5]).toBe(9);
    expect(frame.indexedPixels[6 * HARDWARE.width + 1]).toBe(9);
    expect(frame.indexedPixels[6 * HARDWARE.width + 2]).toBe(0);
  });
});
