import { deepStrictEqual } from 'node:assert/strict';
import { describe, expect, it } from 'vitest';
import { WorkBudget } from './budget';
import { MEMORY, MemoryBus, isMemorySnapshot, type WritableMemoryRegion } from './bus';
import { IndexedGraphics } from './graphics';
import { HARDWARE } from './hardware';

const span = { start: 23, end: 31 };

describe('hardware byte bus', () => {
  it('provides uncharged bounded debugger reads, region labels, and transactional edits', () => {
    const ram = Uint8Array.of(1, 2, 3, 4);
    const rom = Uint8Array.of(5, 6);
    const costs: number[] = [];
    const bus = new MemoryBus(
      [
        { name: 'work RAM', address: 0, bytes: ram, writable: true },
        { name: 'ROM', address: 8, bytes: rom, writable: false },
      ],
      (units) => costs.push(units),
    );
    expect(bus.inspect(0, 10)).toEqual(Uint8Array.of(1, 2, 3, 4, 0, 0, 0, 0, 5, 6));
    expect(bus.describe()).toEqual([
      { name: 'work RAM', address: 0, length: 4, writable: true },
      { name: 'ROM', address: 8, length: 2, writable: false },
    ]);
    expect(costs).toEqual([]);
    bus.edit(1, Uint8Array.of(7, 8));
    expect(ram).toEqual(Uint8Array.of(1, 7, 8, 4));
    const before = ram.slice();
    expect(() => {
      bus.edit(3, Uint8Array.of(9, 9));
    }).toThrow(expect.objectContaining({ code: 'M019021' }));
    expect(ram).toEqual(before);
    expect(() => {
      bus.inspect(MEMORY.size, 1);
    }).toThrow(expect.objectContaining({ code: 'M019020' }));
  });

  it('prepares writable device operations without shadow state or partial cross-region commits', () => {
    const owners = [0x0403, 0x0605];
    let commits = 0;
    const devices = owners.map((_, index): WritableMemoryRegion => ({
      name: `device ${String(index)}`,
      address: 2 + index * 2,
      length: 2,
      writable: true,
      readByte: (offset) => ((owners[index] ?? 0) >>> (offset * 8)) & 255,
      prepareWrite(offset, bytes) {
        const staged = new DataView(new ArrayBuffer(2));
        staged.setUint16(0, owners[index] ?? 0, true);
        new Uint8Array(staged.buffer).set(bytes, offset);
        const value = staged.getUint16(0, true);
        if (value > 0x7fff) return undefined;
        return () => {
          owners[index] = value;
          commits += 1;
        };
      },
    }));
    const ram = Uint8Array.of(1, 2);
    const costs: number[] = [];
    const bus = new MemoryBus(
      [{ name: 'RAM', address: 0, bytes: ram, writable: true }, ...devices],
      (units) => {
        costs.push(units);
      },
    );
    bus.copy(1, 0, 5, span);
    expect([...ram]).toEqual([1, 1]);
    expect(owners).toEqual([0x0302, 0x0504]);
    expect(commits).toBe(2);
    expect(costs).toEqual([11]);
    const saved = bus.snapshot();
    expect(saved.regions).toHaveLength(1);
    expect(() => {
      bus.fill(0, 7, 7, span);
    }).toThrow(expect.objectContaining({ code: 'M019021' }));
    expect(() => {
      bus.fill(0, 255, 6, span);
    }).toThrow(expect.objectContaining({ code: 'M019022' }));
    expect(() => {
      bus.write(2, 9, 1, span, true);
    }).toThrow(expect.objectContaining({ code: 'M019011' }));
    expect(commits).toBe(2);
    expect(owners).toEqual([0x0302, 0x0504]);
    deepStrictEqual(bus.snapshot(), saved);
    owners[0] = 0x1234;
    expect(bus.read(2, 2, span)).toBe(0x1234);
    bus.write(2, 0x5678, 2, span);
    expect(owners[0]).toBe(0x5678);
    bus.restore(saved);
    expect(owners[0]).toBe(0x5678);
    expect(commits).toBe(3);
  });

  it('reads live read-only device registers without storing a shadow image', () => {
    let value = 0x1234;
    const ram = new Uint8Array(2);
    const bus = new MemoryBus(
      [
        { name: 'ram', address: 0, bytes: ram, writable: true },
        {
          name: 'device',
          address: 2,
          length: 2,
          writable: false,
          readByte: (offset) => (value >>> (offset * 8)) & 255,
        },
      ],
      () => {},
    );
    expect(bus.read(2, 2, span)).toBe(0x1234);
    value = 0xabcd;
    expect(bus.read(2, 2, span)).toBe(0xabcd);
    bus.copy(0, 2, 2, span);
    expect([...ram]).toEqual([0xcd, 0xab]);
    const saved = bus.snapshot();
    expect(saved.regions).toHaveLength(1);
    expect(() => {
      bus.fill(0, 0, 4, span);
    }).toThrow(expect.objectContaining({ code: 'M019021' }));
    deepStrictEqual(bus.snapshot(), saved);
    for (const length of [0, -1, 0.5, Infinity, MEMORY.size + 1])
      expect(
        () =>
          new MemoryBus(
            [{ name: 'invalid', address: 0, length, writable: false, readByte: () => 0 }],
            () => {},
          ),
      ).toThrow(/region/);
  });

  it('shares camera, clip and palette registers with high-level drawing and rejects invalid encodings', () => {
    const graphics = new IndexedGraphics();
    const ram = new Uint8Array(8);
    const bus = new MemoryBus(
      [{ name: 'ram', address: 0, bytes: ram, writable: true }, ...graphics.memoryRegions()],
      () => {},
    );
    const command = (name: string, args: number[]) => ({ name, arguments: args, sourceSpan: span });
    expect(bus.read(MEMORY.draw + 48 + 7, 1, span)).toBe(7);
    expect(bus.read(MEMORY.transparency, 1, span)).toBe(1);
    graphics.beginFrame();
    graphics.executeCommand(command('camera', [2, 3]));
    expect(bus.read(MEMORY.draw + 6, 2, span)).toBe(0x4000);
    new DataView(ram.buffer).setFloat64(0, 5, true);
    bus.copy(MEMORY.draw, 0, 8, span);
    bus.write(MEMORY.draw + 48 + 2, 11, 1, span);
    graphics.executeCommand(command('pixel', [5, 3, 2]));
    expect(bus.read(MEMORY.back, 1, span)).toBe(11);
    graphics.executeCommand(command('clip', [1, 0, 1, 1]));
    graphics.executeCommand(command('pixel', [5, 3, 7]));
    graphics.executeCommand(command('pixel', [6, 3, 7]));
    expect(bus.read(MEMORY.back, 2, span)).toBe(0x070b);
    const before = bus.snapshot();
    for (const value of [NaN, Infinity, 0.5, Number.MAX_SAFE_INTEGER + 1]) {
      new DataView(ram.buffer).setFloat64(0, value, true);
      expect(() => {
        bus.copy(MEMORY.draw, 0, 8, span);
      }).toThrow(expect.objectContaining({ code: 'M019022' }));
    }
    new DataView(ram.buffer).setFloat64(0, -1, true);
    expect(() => {
      bus.copy(MEMORY.draw + 32, 0, 8, span);
    }).toThrow(expect.objectContaining({ code: 'M019022' }));
    deepStrictEqual(bus.snapshot().regions.slice(1), before.regions.slice(1));
    graphics.executeCommand(command('clip_reset', []));
    graphics.executeCommand(command('pal_reset', []));
    graphics.executeCommand(command('pixel', [5, 3, 2]));
    expect(bus.read(MEMORY.back, 1, span)).toBe(2);
    graphics.finishFrame();
    graphics.beginFrame();
    expect(bus.read(MEMORY.draw, 2, span)).toBe(0);
    expect(bus.read(MEMORY.draw + 6, 2, span)).toBe(0);
    expect(bus.read(MEMORY.draw + 48 + 2, 1, span)).toBe(2);
  });

  it('maps all 144 raster rows with explicit enable, signed scroll, remaps and frame reset', () => {
    const graphics = new IndexedGraphics();
    const bus = new MemoryBus(graphics.memoryRegions(), () => {});
    graphics.beginFrame();
    graphics.executeCommand({ name: 'clear', arguments: [3], sourceSpan: span });
    graphics.executeCommand({ name: 'pixel', arguments: [239, 0, 4], sourceSpan: span });
    for (let line = 0; line < HARDWARE.height; line += 1) {
      const row = MEMORY.raster + line * MEMORY.rasterStride;
      expect(bus.read(row, 1, span)).toBe(0);
      bus.write(row, 1, 1, span, true);
      bus.write(row + 8 + 3, line % 32, 1, span, true);
    }
    graphics.executeCommand({ name: 'pal', arguments: [3, 23], sourceSpan: span, rasterLine: 1 });
    expect(bus.read(MEMORY.raster + MEMORY.rasterStride + 11, 1, span)).toBe(23);
    expect(bus.read(MEMORY.rasterLive + 16 + 3, 1, span)).toBe(23);
    bus.write(MEMORY.raster + MEMORY.rasterStride + 11, 7, 1, span, true);
    bus.write(MEMORY.raster + 4, 65535, 2, span, true);
    bus.write(MEMORY.raster + 8 + 4, 9, 1, span, true);
    const before = bus.snapshot();
    for (const [offset, value] of [
      [0, 2],
      [1, 1],
    ]) {
      expect(() => {
        bus.write(MEMORY.raster + (offset ?? 0), value ?? 0, 1, span, true);
      }).toThrow(expect.objectContaining({ code: 'M019022' }));
    }
    deepStrictEqual(bus.snapshot(), before);
    const frame = graphics.finishFrame();
    for (let line = 0; line < HARDWARE.height; line += 1)
      expect(frame.indexedPixels[line * HARDWARE.width]).toBe(
        line === 0 ? 9 : line === 1 ? 7 : line % 32,
      );
    graphics.beginFrame();
    expect(bus.read(MEMORY.raster, 1, span)).toBe(0);
    expect(graphics.finishFrame().indexedPixels[0]).toBe(3);
    bus.restore(before);
    deepStrictEqual(bus.snapshot(), before);
  });

  it('aliases storage, supports unaligned little-endian words and charges exact work', () => {
    const bytes = new Uint8Array(16);
    const budget = new WorkBudget(100);
    const bus = new MemoryBus(
      [{ name: 'ram', address: 0, bytes, writable: true }],
      (units, source) => {
        budget.charge(units, source);
      },
    );
    expect(bus.read(0, 2, span)).toBe(0);
    bus.write(1, 0x1234, 2, span);
    expect([...bytes.slice(0, 4)]).toEqual([0, 0x34, 0x12, 0]);
    expect(bus.read(1, 1, span)).toBe(0x34);
    expect(bus.read(1, 2, span)).toBe(0x1234);
    bytes[2] = 0xab;
    expect(bus.read(1, 2, span)).toBe(0xab34);
    expect(budget.used).toBe(9);
    expect(budget.attribution()).toEqual([{ sourceSpan: span, units: 9 }]);
  });

  it('copies overlaps in both directions and defines empty ranges without address wrapping', () => {
    const bytes = Uint8Array.of(1, 2, 3, 4, 5, 6);
    const costs: number[] = [];
    const bus = new MemoryBus([{ name: 'ram', address: 0, bytes, writable: true }], (units) =>
      costs.push(units),
    );
    bus.copy(1, 0, 5, span);
    expect([...bytes]).toEqual([1, 1, 2, 3, 4, 5]);
    bus.copy(0, 1, 5, span);
    expect([...bytes]).toEqual([1, 2, 3, 4, 5, 5]);
    bus.fill(2, 9, 2, span);
    expect([...bytes]).toEqual([1, 2, 9, 9, 5, 5]);
    bus.copy(MEMORY.size, MEMORY.size, 0, span);
    bus.fill(MEMORY.size, 255, 0, span);
    expect(costs).toEqual([11, 11, 3, 1, 1]);
    expect(bus.read(MEMORY.size - 1, 1, span)).toBe(0);
    for (const address of [-1, 0.5, NaN, Infinity, MEMORY.size, Number.MAX_SAFE_INTEGER])
      expect(() => bus.read(address, 1, span)).toThrow(
        expect.objectContaining({ code: 'M019020', sourceSpan: span }),
      );
    expect(() => bus.read(MEMORY.size - 1, 2, span)).toThrow(
      expect.objectContaining({ code: 'M019020' }),
    );
    for (const length of [-1, 0.5, Infinity, MEMORY.size + 1])
      expect(() => {
        bus.copy(0, 0, length, span);
      }).toThrow(expect.objectContaining({ code: 'M019020' }));
  });

  it('validates the entire operation before cross-region, permission or value mutations', () => {
    const first = Uint8Array.of(3, 4);
    const second = Uint8Array.of(5, 6);
    const readOnly = Uint8Array.of(7, 8);
    const bus = new MemoryBus(
      [
        { name: 'first', address: 0, bytes: first, writable: true },
        {
          name: 'second',
          address: 2,
          bytes: second,
          writable: true,
          validate: (_offset, bytes) => bytes.every((byte) => byte < 32),
        },
        { name: 'rom', address: 4, bytes: readOnly, writable: false },
      ],
      () => {},
    );
    const before = bus.snapshot();
    expect(() => {
      bus.fill(0, 1, 5, span);
    }).toThrow(expect.objectContaining({ code: 'M019021' }));
    expect(() => {
      bus.fill(0, 32, 3, span);
    }).toThrow(expect.objectContaining({ code: 'M019022' }));
    expect(() => {
      bus.write(1, 0x2001, 2, span);
    }).toThrow(expect.objectContaining({ code: 'M019022' }));
    expect(() => {
      bus.fill(0, 1, 3, span, true);
    }).toThrow(expect.objectContaining({ code: 'M019011' }));
    for (const value of [-1, 0.5, 256, NaN])
      expect(() => {
        bus.write(0, value, 1, span);
      }).toThrow(expect.objectContaining({ code: 'M019022' }));
    deepStrictEqual(bus.snapshot(), before);
    expect([...readOnly]).toEqual([7, 8]);
    expect(bus.read(6, 1, span)).toBe(0);
    expect(() => {
      bus.write(6, 0, 1, span);
    }).toThrow(expect.objectContaining({ code: 'M019021' }));
    bus.write(1, 0x0201, 2, span);
    expect([...first, ...second]).toEqual([3, 1, 2, 6]);
  });

  it('charges bulk work before allocating or mutating and restores snapshots transactionally', () => {
    const bytes = new Uint8Array(16);
    const budget = new WorkBudget(4);
    const bus = new MemoryBus(
      [{ name: 'ram', address: 0, bytes, writable: true }],
      (units, source) => {
        budget.charge(units, source);
      },
    );
    const before = bus.snapshot();
    expect(() => {
      bus.fill(0, 1, MEMORY.size, span);
    }).toThrow(expect.objectContaining({ code: 'M019001', sourceSpan: span }));
    deepStrictEqual(bus.snapshot(), before);
    bytes[0] = 2;
    bus.restore(before);
    expect(bytes[0]).toBe(0);
    for (const snapshot of [
      null,
      { revision: 1, regions: [] },
      { revision: 1, regions: [{ address: 0, bytes: new Uint8Array(15) }] },
    ])
      expect(() => {
        bus.restore(snapshot);
      }).toThrow(/snapshot/);
    deepStrictEqual(bus.snapshot(), before);
    expect(
      isMemorySnapshot({
        revision: 1,
        regions: [{ address: MEMORY.size, bytes: new Uint8Array(1) }],
      }),
    ).toBe(false);
    expect(
      isMemorySnapshot({
        revision: 1,
        regions: [
          { address: 1, bytes },
          { address: 0, bytes },
        ],
      }),
    ).toBe(false);
  });

  it('uses the production front/back/scanout storage without mirrors or remapping raw writes', () => {
    const graphics = new IndexedGraphics();
    const regions = graphics.memoryRegions();
    const bus = new MemoryBus(regions, () => {});
    const command = (name: string, args: number[], rasterLine?: number) => ({
      name,
      arguments: args,
      sourceSpan: span,
      ...(rasterLine === undefined ? {} : { rasterLine }),
    });
    graphics.beginFrame();
    graphics.executeCommand(command('pal', [3, 11]));
    graphics.executeCommand(command('pixel', [0, 0, 3]));
    expect(bus.read(MEMORY.back, 1, span)).toBe(11);
    expect(bus.read(MEMORY.front, 1, span)).toBe(0);
    bus.write(MEMORY.back + 1, 3, 1, span);
    graphics.executeCommand(command('pal', [3, 23], 0));
    const output = graphics.finishFrame();
    expect([...output.indexedPixels.slice(0, 2)]).toEqual([11, 23]);
    expect(bus.read(MEMORY.front + 1, 1, span)).toBe(3);
    expect(bus.read(MEMORY.display + 1, 1, span)).toBe(23);
    const before = bus.snapshot();
    for (const operation of [
      () => {
        bus.write(MEMORY.display, 0, 1, span);
      },
      () => {
        bus.fill(MEMORY.back + HARDWARE.width * HARDWARE.height - 1, 1, 2, span);
      },
    ])
      expect(operation).toThrow();
    deepStrictEqual(bus.snapshot(), before);
    graphics.beginFrame();
    expect(bus.read(MEMORY.back, 1, span)).toBe(11);
    expect(bus.read(MEMORY.back + 1, 1, span)).toBe(3);
    graphics.finishFrame();
    for (let index = 0; index < regions.length; index += 1)
      expect(graphics.memoryRegions()[index]?.bytes).toBe(regions[index]?.bytes);
    bus.restore(before);
    expect(bus.read(MEMORY.display + 1, 1, span)).toBe(23);
  });
});
