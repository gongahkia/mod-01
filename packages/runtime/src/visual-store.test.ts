import { deepStrictEqual } from 'node:assert/strict';
import { describe, expect, it } from 'vitest';
import { MEMORY, MemoryBus } from './bus';
import { HARDWARE } from './hardware';
import { IndexedGraphics, type VisualAsset } from './graphics';
import { VisualAssetStore } from './visual-store';

const span = { start: 12, end: 18 };
const assets: readonly VisualAsset[] = [
  {
    kind: 'tile_set',
    name: 'zterrain',
    flags: Uint8Array.of(2, 4),
    tiles: [6, 9].map((color) => ({
      kind: 'sprite',
      name: 'tile',
      width: 8,
      height: 8,
      pixels: new Uint8Array(64).fill(color),
    })),
  },
  {
    kind: 'map',
    name: 'level',
    layers: [{ width: 2, height: 1, cells: Uint16Array.of(0, 1), tileSet: 'zterrain' }],
  },
  { kind: 'sprite', name: 'hero', width: 1, height: 1, pixels: Uint8Array.of(7) },
];
const font: VisualAsset = {
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
const command = (name: string, arguments_: unknown[]) => ({
  name,
  arguments: arguments_,
  sourceSpan: span,
});
const word32 = (bus: MemoryBus, address: number) =>
  bus.read(address, 2, span) + bus.read(address + 2, 2, span) * 65536;

describe('packed visual hardware storage', () => {
  it('allocates deterministically without alignment padding and publishes exact descriptors', () => {
    const store = new VisualAssetStore(assets);
    const bus = new MemoryBus(store.memoryRegions(), () => {});
    expect(store.usedBytes).toBe(135);
    expect(['hero', 'level', 'zterrain', 'absent'].map((name) => store.id(name))).toEqual([
      0, 1, 2, -1,
    ]);
    expect(
      Array.from({ length: 6 }, (_, field) => word32(bus, MEMORY.visualInfo + field * 4)),
    ).toEqual([3, 135, MEMORY.assets, 5, MEMORY.allocations, 0]);
    expect(
      Array.from({ length: 4 }, (_, field) => word32(bus, MEMORY.assets + 32 + field * 4)),
    ).toEqual([4, 1, MEMORY.allocations + 24, 4]);
    expect(
      Array.from({ length: 6 }, (_, field) => word32(bus, MEMORY.allocations + 24 + field * 4)),
    ).toEqual([2, MEMORY.visual + 1, 4, 2, 1, 3]);
    expect(bus.read(MEMORY.visual + 1, 2, span)).toBe(0);
    expect(bus.read(MEMORY.visual + 3, 2, span)).toBe(1);
    const reversed = new MemoryBus(
      new VisualAssetStore([...assets].reverse()).memoryRegions(),
      () => {},
    );
    deepStrictEqual(bus.snapshot(), reversed.snapshot());
    for (const address of [
      MEMORY.visualInfo,
      MEMORY.assets,
      MEMORY.assets + 16,
      MEMORY.allocations,
    ])
      expect(() => {
        bus.write(address, 0, 1, span);
      }).toThrow(expect.objectContaining({ code: 'M019021' }));
    expect(bus.read(MEMORY.assets + 3 * 32, 2, span)).toBe(0);
  });

  it('shares sprite pixels, map cells and flags with drawing and query APIs', () => {
    const store = new VisualAssetStore(assets);
    const graphics = new IndexedGraphics(store);
    const bus = new MemoryBus([...store.memoryRegions(), ...graphics.memoryRegions()], () => {});
    bus.write(MEMORY.visual, 23, 1, span);
    bus.write(MEMORY.visual + 1, 1, 2, span);
    bus.write(MEMORY.visual + 69, 11, 1, span);
    bus.write(MEMORY.visual + 134, 8, 1, span);
    expect(store.mapCell('level', 0, 0, 0)).toBe(1);
    expect(store.mapFlag('level', 0, 0, 0, 3)).toBe(true);
    const frame = graphics.executeFrame([
      command('sprite', [{ kind: 'Sprite', name: 'hero' }, 0, 0]),
      command('map', [{ kind: 'Map', name: 'level' }, 1, 0]),
    ]);
    expect([...frame.indexedPixels.slice(0, 3)]).toEqual([23, 11, 9]);
    expect(store.get('hero')?.kind === 'sprite' && store.get('hero')).toMatchObject({
      pixels: Uint8Array.of(23),
    });
    const original = assets[2];
    expect(original?.kind === 'sprite' && original.pixels[0]).toBe(7);
    const saved = bus.snapshot();
    bus.write(MEMORY.visual + 1, 0, 2, span);
    expect(store.mapCell('level', 0, 0, 0)).toBe(0);
    bus.restore(saved);
    expect(store.mapCell('level', 0, 0, 0)).toBe(1);
    expect(
      graphics.executeFrame([command('map', [{ kind: 'Map', name: 'level' }, 0, 0])])
        .indexedPixels[0],
    ).toBe(11);
  });

  it('rejects invalid pixels and partial map words before any mutation, including full restores', () => {
    const store = new VisualAssetStore(assets);
    const bus = new MemoryBus(store.memoryRegions(), () => {});
    const saved = bus.snapshot();
    for (const [address, value] of [
      [MEMORY.visual + 1, 2],
      [MEMORY.visual + 2, 1],
    ])
      expect(() => {
        bus.write(address ?? 0, value ?? 0, 1, span);
      }).toThrow(expect.objectContaining({ code: 'M019022' }));
    expect(() => {
      bus.fill(MEMORY.visual, 2, 5, span);
    }).toThrow(expect.objectContaining({ code: 'M019022' }));
    deepStrictEqual(bus.snapshot(), saved);
    const invalid = structuredClone(saved);
    const bytes = invalid.regions[0]?.bytes;
    if (bytes === undefined) throw new Error('missing visual memory');
    bytes[2] = 1;
    expect(() => {
      bus.restore(invalid);
    }).toThrow(/snapshot/);
    deepStrictEqual(bus.snapshot(), saved);
    bus.write(MEMORY.visual + 128 * 1024 - 1, 255, 1, span);
    expect(bus.read(MEMORY.visual + 128 * 1024 - 1, 1, span)).toBe(255);
    expect(() => {
      bus.write(MEMORY.visual, 1, 1, span, true);
    }).toThrow(expect.objectContaining({ code: 'M019011' }));
  });

  it('keeps display defaults in the same charged image with next-frame visibility', () => {
    const identity = Uint8Array.from({ length: 32 }, (_, index) => index);
    const store = new VisualAssetStore([], {
      remap: identity,
      raster: [{ line: 0, scrollX: 0, scrollY: 0, remap: identity }],
    });
    const bus = new MemoryBus(store.memoryRegions(), () => {});
    const graphics = new IndexedGraphics(store);
    expect(store.usedBytes).toBe(70);
    expect(word32(bus, MEMORY.visualInfo + 20)).toBe(MEMORY.visual);
    graphics.beginFrame();
    bus.write(MEMORY.visual + 7, 9, 1, span);
    bus.write(MEMORY.visual + 32 + 6 + 9, 11, 1, span);
    graphics.executeCommand(command('pixel', [0, 0, 7]));
    expect(graphics.finishFrame().indexedPixels[0]).toBe(7);
    expect(graphics.executeFrame([command('pixel', [0, 0, 7])]).indexedPixels[0]).toBe(11);
    bus.write(MEMORY.visual + 34, 65535, 2, span);
    expect(store.display?.raster[0]?.scrollX).toBe(-1);
    expect(() => {
      bus.write(MEMORY.visual + 32, 1, 1, span);
    }).toThrow(expect.objectContaining({ code: 'M019022' }));
    bus.write(MEMORY.visual + 38, 32, 1, span);
    expect(bus.read(MEMORY.visual + 38, 1, span)).toBe(32);
  });

  it('accounts for exact capacity and bounds descriptors independently of host alignment', () => {
    const full = Array.from({ length: 256 }, (_, index): VisualAsset => ({
      kind: 'sprite',
      name: `s${String(index)}`,
      width: 64,
      height: 64,
      pixels: new Uint8Array(4096),
    }));
    expect(new VisualAssetStore(full).usedBytes).toBe(HARDWARE.visualCapacityBytes);
    expect(() => new VisualAssetStore(full, { remap: new Uint8Array(32), raster: [] })).toThrow(
      /1 MiB/,
    );
    expect(
      () => new VisualAssetStore(Array.from({ length: 4097 }, () => assets[2] as VisualAsset)),
    ).toThrow(/too many/);
    expect(MEMORY.allocations + 4096 * MEMORY.allocationStride).toBeLessThanOrEqual(
      MEMORY.cartridgeRomExtension,
    );
  });

  it('maps larger revision-2 visual banks without moving legacy addresses', () => {
    const sprites = Array.from({ length: 9 }, (_, index): VisualAsset => ({
      kind: 'sprite',
      name: `large${String(index)}`,
      width: 128,
      height: 128,
      pixels: new Uint8Array(128 * 128).fill(index + 32),
    }));
    const store = new VisualAssetStore(sprites);
    const bus = new MemoryBus(store.memoryRegions(), () => {});
    expect(store.usedBytes).toBe(9 * 128 * 128);
    expect(word32(bus, MEMORY.allocations + 8 * MEMORY.allocationStride + 4)).toBe(
      MEMORY.visualExtended + 8 * 128 * 128,
    );
    expect(bus.read(MEMORY.visual, 1, span)).toBe(32);
    expect(bus.read(MEMORY.visualExtended + 8 * 128 * 128, 1, span)).toBe(40);
    bus.write(MEMORY.visualExtended + 8 * 128 * 128, 255, 1, span);
    const last = store.get('large8');
    if (last?.kind !== 'sprite') throw new Error('missing large sprite');
    expect(last.pixels[0]).toBe(255);
  });

  it('packs font metadata immutably while glyph bitmap bytes alias the visual bus', () => {
    const store = new VisualAssetStore([font]);
    const bus = new MemoryBus(store.memoryRegions(), () => {});
    expect(store.usedBytes).toBe(20);
    expect(word32(bus, MEMORY.assets)).toBe(5);
    expect(word32(bus, MEMORY.allocations)).toBe(6);
    expect(
      Array.from({ length: 8 }, (_, index) => bus.read(MEMORY.visual + index, 1, span)),
    ).toEqual([2, 2, 1, 3, 4, 63, 2, 0]);
    const stored = store.get('tiny');
    if (stored?.kind !== 'font') throw new Error('missing stored font');
    expect(stored.glyphs.get(63)).toEqual(Uint8Array.of(1, 1, 0, 1));
    bus.write(MEMORY.visual + 10, 0, 1, span);
    expect(stored.glyphs.get(63)?.[0]).toBe(0);
    const saved = bus.snapshot();
    for (const [address, value] of [
      [MEMORY.visual, 3],
      [MEMORY.visual + 8, 64],
      [MEMORY.visual + 10, 2],
    ]) {
      expect(() => {
        bus.write(address ?? 0, value ?? 0, 1, span);
      }).toThrow(expect.objectContaining({ code: 'M019022' }));
      deepStrictEqual(bus.snapshot(), saved);
    }
  });
});
