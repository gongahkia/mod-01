import { describe, expect, it, vi } from 'vitest';

import { HARDWARE } from './hardware';
import { MEMORY, MemoryBus } from './bus';
import { WorkBudget } from './budget';
import { isSaveValues, isPendingSaveWrites, SaveMemory, decodeSaveValues } from './save';

const span = { start: 17, end: 24 };
function device(initial: Uint8Array = new Uint8Array()) {
  const save = new SaveMemory(initial);
  const budget = new WorkBudget(HARDWARE.workUnitsPerFrame);
  const charge = (units: number) => {
    budget.charge(units, span);
  };
  return { save, budget, bus: new MemoryBus(save.memoryRegions(charge), charge) };
}

describe('worker save memory', () => {
  it('orders exposed values and pending writes by ASCII without consulting host collation', () => {
    const names = ['a_', 'z', '_a', 'a0', 'a', 'A'];
    const expected = ['A', '_a', 'a', 'a0', 'a_', 'z'];
    const collation = vi.spyOn(String.prototype, 'localeCompare').mockImplementation(() => {
      throw new Error('locale collation must not affect save state');
    });
    try {
      const images: Uint8Array[] = [];
      for (const order of [names, [...names].reverse()]) {
        const save = new SaveMemory();
        for (const name of order) save.set(name, 7);
        expect(Object.keys(save.snapshot())).toEqual(expected);
        expect(save.pendingWrites().map((write) => write.key)).toEqual(expected);
        const snapshot = save.deviceSnapshot();
        const pending = save.pendingWrites();
        save.restoreDevice(snapshot, pending);
        expect(save.pendingWrites()).toEqual(pending);
        images.push(save.takeCommit() ?? new Uint8Array());
      }
      expect(images[0]).toEqual(images[1]);
      expect(decodeSaveValues(images[0] ?? new Uint8Array())).toEqual({
        A: 7,
        _a: 7,
        a: 7,
        a0: 7,
        a_: 7,
        z: 7,
      });
    } finally {
      collation.mockRestore();
    }
  });

  it('maps zero-reset working bytes, a read-only commit latch, and live status without retained mirrors', () => {
    const { save, bus } = device();
    expect(save.deviceSnapshot().bytes.every((byte) => byte === 0)).toBe(true);
    expect(bus.snapshot().regions).toEqual([]);
    expect(bus.read(MEMORY.saveControl + 4, 2, span)).toBe(8192);
    bus.write(MEMORY.save + 3, 0x1234, 2, span);
    expect(bus.read(MEMORY.save + 3, 2, span)).toBe(0x1234);
    expect(bus.read(MEMORY.saveCommitted + 3, 2, span)).toBe(0);
    expect(bus.read(MEMORY.saveControl + 1, 1, span)).toBe(1);
    expect(bus.read(MEMORY.saveControl + 16, 2, span)).toBe(2);
    expect(save.takeCommit()).toBeUndefined();
    bus.write(MEMORY.saveControl, 1, 1, span);
    expect(bus.read(MEMORY.saveCommitted + 3, 2, span)).toBe(0x1234);
    expect(bus.read(MEMORY.saveControl + 1, 1, span)).toBe(2);
    expect(bus.read(MEMORY.saveControl + 8, 2, span)).toBe(1);
    bus.write(MEMORY.save + 3, 0, 1, span);
    expect(bus.read(MEMORY.saveControl + 1, 1, span)).toBe(3);
    const committed = save.takeCommit();
    expect(committed?.slice(3, 5)).toEqual(Uint8Array.of(0x34, 0x12));
    expect(bus.read(MEMORY.saveControl + 1, 1, span)).toBe(1);
    committed?.fill(0);
    expect(bus.read(MEMORY.saveCommitted + 3, 2, span)).toBe(0x1234);
    expect(save.takeCommit()).toBeUndefined();
  });

  it('shares JSON integer values with raw bytes and latches high-level writes at call time', () => {
    const bytes = new TextEncoder().encode('{"score":7}');
    const { save, bus } = device(bytes);
    bytes.fill(0);
    expect(save.get('score', 0)).toBe(7);
    bus.write(MEMORY.save + 9, 56, 1, span);
    expect(save.get('score', 0)).toBe(8);
    save.set('level', 2);
    const checkpoint = save.deviceSnapshot();
    bus.fill(MEMORY.save, 0, HARDWARE.saveCapacityBytes, span);
    expect(save.get('score', 99)).toBe(99);
    expect(decodeSaveValues(save.takeCommit() ?? new Uint8Array())).toEqual({ score: 8, level: 2 });
    save.restoreDevice(checkpoint, [{ key: 'level', value: 2 }]);
    expect(save.get('score', 0)).toBe(8);
    bus.fill(MEMORY.save, 0, HARDWARE.saveCapacityBytes, span);
    save.set('new_key', 3);
    expect(save.pendingWrites()).toEqual([{ key: 'new_key', value: 3 }]);
    save.restoreDevice(save.deviceSnapshot(), save.pendingWrites());
    expect(save.snapshot()).toEqual({ new_key: 3 });
  });

  it('keeps arbitrary binary images and rejects integer access without destructive fallback', () => {
    const { save, bus } = device(Uint8Array.of(255, 0, 123));
    const before = save.deviceSnapshot();
    expect(() => save.get('score', 0)).toThrow(/integer-save image/);
    expect(() => {
      save.set('score', 9);
    }).toThrow(/integer-save image/);
    expect(save.deviceSnapshot()).toEqual(before);
    save.commit();
    expect(save.takeCommit()?.slice(0, 3)).toEqual(Uint8Array.of(255, 0, 123));
    bus.fill(MEMORY.save, 0, HARDWARE.saveCapacityBytes, span);
    save.set('score', 9);
    expect(save.get('score', 0)).toBe(9);
  });

  it('validates complete snapshots and pending writes against the latch before mutating', () => {
    const { save, bus } = device();
    save.set('score', 7);
    bus.write(MEMORY.save, 255, 1, span);
    const before = save.deviceSnapshot();
    const pending = save.pendingWrites();
    save.restoreDevice(before, pending);
    for (const bad of [
      { ...before, bytes: new Uint8Array(8191) },
      { ...before, committed: [] },
      { ...before, commits: Number.MAX_SAFE_INTEGER + 1 },
      { ...before, pendingCommit: false },
    ]) {
      expect(() => {
        save.restoreDevice(bad, pending);
      }).toThrow(/snapshot/);
      expect(save.deviceSnapshot()).toEqual(before);
    }
    expect(() => {
      save.restoreDevice(before, Array(1));
    }).toThrow(/snapshot/);
    const disguised = Object.assign(Array(1), { extra: { key: 'score', value: 7 } });
    expect(() => {
      save.restoreDevice(before, disguised);
    }).toThrow(/snapshot/);
    before.bytes.fill(0);
    expect(bus.read(MEMORY.save, 1, span)).toBe(255);
  });

  it('preflights command permissions, raster writes, counters and work without partial commits', () => {
    const { save, bus, budget } = device();
    const before = save.deviceSnapshot();
    for (const [address, value, width, raster, code] of [
      [MEMORY.saveControl, 1, 2, false, 'M019021'],
      [MEMORY.saveControl, 2, 1, false, 'M019022'],
      [MEMORY.saveCommitted, 1, 1, false, 'M019021'],
      [MEMORY.save, 1, 1, true, 'M019011'],
      [MEMORY.saveControl, 1, 1, true, 'M019011'],
    ] as const) {
      expect(() => {
        bus.write(address, value, width, span, raster);
      }).toThrow(expect.objectContaining({ code, sourceSpan: span }));
      expect(save.deviceSnapshot()).toEqual(before);
    }
    budget.beginFrame();
    bus.write(MEMORY.saveControl, 0, 1, span);
    expect(budget.used).toBe(1);
    bus.write(MEMORY.saveControl, 1, 1, span);
    expect(budget.used).toBe(8194);
    expect(save.deviceSnapshot().commits).toBe(1);
    const overflow = { ...save.deviceSnapshot(), commits: Number.MAX_SAFE_INTEGER };
    save.restoreDevice(overflow);
    expect(() => {
      bus.write(MEMORY.saveControl, 1, 1, span);
    }).toThrow(expect.objectContaining({ code: 'M019012' }));
    expect(() => {
      save.set('score', 1);
    }).toThrow(/counter exhausted/);
    expect(save.deviceSnapshot()).toEqual(overflow);
    save.restoreDevice(before);
    budget.beginFrame();
    budget.charge(HARDWARE.workUnitsPerFrame - 8192, span);
    expect(() => {
      bus.write(MEMORY.saveControl, 1, 1, span);
    }).toThrow(expect.objectContaining({ code: 'M019001' }));
    expect(save.deviceSnapshot()).toEqual(before);
  });

  it('applies debugger save edits without consuming cartridge work', () => {
    const { save, bus, budget } = device();
    budget.beginFrame();
    bus.edit(MEMORY.save + 7, Uint8Array.of(42));
    bus.edit(MEMORY.saveControl, Uint8Array.of(1));
    expect(budget.used).toBe(0);
    expect(save.deviceSnapshot().commits).toBe(1);
    expect(save.takeCommit()?.[7]).toBe(42);
  });

  it('uses fallback values for absent keys that match Object prototype names', () => {
    const save = new SaveMemory();
    for (const key of ['toString', 'valueOf', 'hasOwnProperty', '__defineGetter__']) {
      expect(save.get(key, 7)).toBe(7);
      save.set(key, 9);
      expect(save.get(key, 7)).toBe(9);
    }
  });

  it('rejects sparse pending-write arrays before restore mutation', () => {
    const save = new SaveMemory({ score: 7 });
    const before = save.snapshot();
    expect(isPendingSaveWrites(Array(1), before)).toBe(false);
    expect(() => {
      save.restore({ score: 99 }, Array(1));
    }).toThrow(/snapshot/);
    expect(save.snapshot()).toEqual(before);
  });

  it('reads fallbacks, emits sorted writes, and restores snapshots', () => {
    const save = new SaveMemory({ score: 2 });
    expect(save.get('score', 0)).toBe(2);
    expect(save.get('missing', 7)).toBe(7);
    save.set('z_value', 4);
    save.set('a_value', 3);
    expect(save.takeWrites()).toEqual([
      { key: 'a_value', value: 3 },
      { key: 'z_value', value: 4 },
    ]);
    const snapshot = save.snapshot();
    save.set('score', 9);
    save.restore(snapshot);
    expect(save.get('score', 0)).toBe(2);
    expect(save.takeWrites()).toEqual([]);
  });

  it('rejects unsafe keys, non-integers, and save-capacity overflow', () => {
    const save = new SaveMemory();
    expect(() => {
      save.set('__proto__', 1);
    }).toThrow(/canonical ASCII/);
    expect(() => {
      save.set('score', 0.5);
    }).toThrow(/safe integers/);
    const oversized = Object.fromEntries(
      Array.from({ length: 512 }, (_, index) => [
        `value_${String(index)}`,
        Number.MAX_SAFE_INTEGER,
      ]),
    );
    expect(new TextEncoder().encode(JSON.stringify(oversized)).byteLength).toBeGreaterThan(
      HARDWARE.saveCapacityBytes,
    );
    expect(isSaveValues(oversized)).toBe(false);
  });

  it('validates pending writes and preserves them across snapshots without flushing early', () => {
    const save = new SaveMemory();
    save.set('score', 7);
    expect(save.pendingWrites()).toEqual([{ key: 'score', value: 7 }]);
    const snapshot = save.snapshot();
    expect(() => {
      save.restore({ score: 7 }, [{ key: 'score', value: 8 }]);
    }).toThrow(/snapshot/);
    expect(() => {
      save.restore({ score: 7 }, [
        { key: 'score', value: 7 },
        { key: 'score', value: 7 },
      ]);
    }).toThrow(/snapshot/);
    expect(save.snapshot()).toEqual(snapshot);
    expect(save.takeWrites()).toEqual([{ key: 'score', value: 7 }]);
  });
});
