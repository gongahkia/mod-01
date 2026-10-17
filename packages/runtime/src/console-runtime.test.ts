import { deepStrictEqual } from 'node:assert/strict';
import { describe, expect, it } from 'vitest';
import { createConsoleRuntime } from './console-runtime';
import { MEMORY } from './bus';
import { emptyInputFrame } from './input';
import type { CartridgeFactory, MachineSnapshot } from './machine';
import { SaveMemory, decodeSaveValues } from './save';

const legacyMachine = (snapshot: MachineSnapshot) => ({
  revision: 1,
  frame: snapshot.frame,
  rngState: snapshot.rngState,
  cartridge: snapshot.cartridge,
  input: snapshot.input,
  previousInput: snapshot.previousInput,
});

const span = { start: 10, end: 20 };
const configuration = { seed: 99, workUnitsPerFrame: 50_000, updateRate: 60 as const };
const factory: CartridgeFactory = (api) => ({
  start() {},
  update() {
    const value = api.call('save_get_int', ['counter', 0], span) as number;
    api.call('save_set_int', ['counter', value + 1], span);
  },
  draw() {
    api.enter?.('draw', span);
    api.probe?.(1, span, { value: 7 });
    api.call('pixel', [0, 0, 7], span);
    api.leave?.();
  },
  raster() {},
  snapshot: () => ({ state: {}, tasks: [], nextTaskId: 1 }),
  restore() {},
  inspect: () => ({ state: {}, tasks: [], callStack: [] }),
});

describe('production console dispatcher', () => {
  it('exposes cloned ROM/status bytes and permits debugger-only transactional RAM edits', () => {
    let observed = -1;
    const rom = Uint8Array.of(77, 79, 68, 45, 48, 49, 26, 1, 3, 0, 0, 0, 77);
    const runtime = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        update() {
          observed = api.call('mem_read', [MEMORY.cartridgeRom + 12], span) as number;
        },
      }),
      { ...configuration, debug: true, rom },
    );
    rom[12] = 0;
    const status = runtime.inspectMemory(MEMORY.cartridgeInfo, 64);
    expect(new DataView(status.bytes.buffer).getUint16(0, true)).toBe(1);
    expect(new DataView(status.bytes.buffer).getUint16(2, true)).toBe(1);
    expect(new DataView(status.bytes.buffer).getUint32(4, true)).toBe(13);
    expect(new DataView(status.bytes.buffer).getUint32(20, true)).toBe(3);
    expect(status.regions).toContainEqual({
      name: 'canonical cartridge ROM',
      address: MEMORY.cartridgeRom,
      length: 13,
      writable: false,
    });
    runtime.editMemory(MEMORY.ram, Uint8Array.of(9, 8));
    expect(runtime.inspectMemory(MEMORY.ram, 2).bytes).toEqual(Uint8Array.of(9, 8));
    expect(() => {
      runtime.editMemory(MEMORY.cartridgeRom, Uint8Array.of(0));
    }).toThrow(expect.objectContaining({ code: 'M019021' }));
    runtime.runFrame(emptyInputFrame());
    expect(observed).toBe(77);

    const release = createConsoleRuntime(factory, { ...configuration, rom });
    expect(() => {
      release.inspectMemory(0, 1);
    }).toThrow(expect.objectContaining({ code: 'M019104' }));
    expect(() => {
      release.editMemory(0, Uint8Array.of(1));
    }).toThrow(expect.objectContaining({ code: 'M019104' }));
  });

  it('latches output-stage audio exhaustion before advancing the completed frame counter', () => {
    const runtime = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        update() {},
        draw() {
          api.call('music', [{ kind: 'Music', name: 'song' }], span);
        },
      }),
      {
        ...configuration,
        assets: {
          declarations: {
            tone: { kind: 'sound', path: 'tone.m01s' },
            song: { kind: 'music', path: 'song.m01t' },
          },
          files: {
            'tone.m01s': new TextEncoder().encode(
              JSON.stringify({
                revision: 1,
                kind: 'sound',
                waveform: 'triangle',
                note: 60,
                durationFrames: 4,
                volume: 0.5,
                pan: 0,
                envelope: { attackFrames: 0, decayFrames: 0, sustainLevel: 1, releaseFrames: 0 },
                pitch: {
                  slideSemitonesPerFrame: 0,
                  vibratoDepthSemitones: 0,
                  vibratoPeriodFrames: 0,
                },
              }),
            ),
            'song.m01t': new TextEncoder().encode(
              JSON.stringify({
                revision: 1,
                kind: 'music',
                framesPerRow: 1,
                order: ['p'],
                loop: true,
                patterns: {
                  p: {
                    rows: [[{ note: 60, sound: 'tone' }, null, null, null, null, null, null, null]],
                  },
                },
              }),
            ),
          },
        },
      },
    );
    const healthy = runtime.snapshot();
    runtime.restore({
      ...healthy,
      audio: { ...healthy.audio, nextSequence: Number.MAX_SAFE_INTEGER },
    });
    expect(() => runtime.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019012' }),
    );
    const faulted = runtime.snapshot();
    expect(faulted.machine.frame).toBe(0);
    expect(faulted.audio.frame).toBe(0);
    expect(faulted.audio.nextSequence).toBe(Number.MAX_SAFE_INTEGER);
    expect(faulted.machine.execution).toMatchObject({
      phase: 'output',
      updates: 1,
      rasterLine: null,
      fault: { code: 9012, sourceSpan: { start: 0, end: 0 } },
    });
    expect(() => runtime.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019014' }),
    );
    deepStrictEqual(runtime.snapshot(), faulted);
    runtime.restore(healthy);
    expect(runtime.runFrame(emptyInputFrame()).frame).toBe(0);
    expect(runtime.snapshot().machine.frame).toBe(1);
    runtime.restore(faulted);
    deepStrictEqual(runtime.snapshot(), faulted);
  });

  it('rejects a raised hardware work ceiling before constructing cartridge state', () => {
    let constructed = false;
    expect(() =>
      createConsoleRuntime(
        (api) => {
          constructed = true;
          return factory(api);
        },
        { ...configuration, workUnitsPerFrame: 50_001 },
      ),
    ).toThrow(expect.objectContaining({ code: 'M019100' }));
    expect(constructed).toBe(false);
  });

  it('rejects malformed input before resetting any device state', () => {
    const runtime = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        draw() {
          api.call('pal', [2, 7], span);
        },
      }),
      configuration,
    );
    runtime.runFrame(emptyInputFrame());
    const before = runtime.snapshot();
    const input = emptyInputFrame();
    expect(() => {
      runtime.runFrame({ ...input, pointer: { ...input.pointer, x: 240 } });
    }).toThrow(expect.objectContaining({ code: 'M019008' }));
    deepStrictEqual(runtime.snapshot(), before);
  });

  it('restores all device state and pending boot saves transactionally', () => {
    const runtime = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        start() {
          api.call('save_set_int', ['boots', 1], span);
        },
      }),
      configuration,
    );
    const before = runtime.snapshot();
    const invalid = {
      ...before,
      machine: {
        ...before.machine,
        frame: 99,
        execution: { ...before.machine.execution, updates: 99 },
      },
      save: { ...before.save, bytes: new SaveMemory({ counter: 99 }).deviceSnapshot().bytes },
      audio: {
        ...before.audio,
        voices: before.audio.voices.map((voice, index) =>
          index === 0 ? { ...voice, active: true, sound: 'missing' } : voice,
        ),
      },
    };
    expect(() => {
      runtime.restore(invalid);
    }).toThrow(/missing sound/);
    deepStrictEqual(runtime.snapshot(), before);
    const first = runtime.runFrame(emptyInputFrame());
    expect(first.saveWrites).toContainEqual({ key: 'boots', value: 1 });
    runtime.restore(before);
    deepStrictEqual(runtime.runFrame(emptyInputFrame()), first);
    const invalidPixels = { ...before, graphics: { ...before.graphics, front: new Uint8Array(1) } };
    const saved = runtime.snapshot();
    expect(() => {
      runtime.restore(invalidPixels);
    }).toThrow(/invalid worker snapshot/);
    deepStrictEqual(runtime.snapshot(), saved);
    expect(() => {
      runtime.restore({
        ...saved,
        memory: {
          ...saved.memory,
          regions: saved.memory.regions.filter((region) => region.address !== MEMORY.ram),
        },
      });
    }).toThrow(/memory snapshot/);
    deepStrictEqual(runtime.snapshot(), saved);
  });

  it('migrates revision-2 frame snapshots with zero-initialized work RAM', () => {
    const runtime = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        update() {
          api.call('mem_write', [0, 7], span);
        },
      }),
      configuration,
    );
    const initial = runtime.snapshot();
    runtime.runFrame(emptyInputFrame());
    expect(runtime.snapshot().memory.regions[0]?.bytes[0]).toBe(7);
    runtime.restore({
      revision: 2,
      machine: legacyMachine(initial.machine),
      save: decodeSaveValues(initial.save.bytes),
      graphics: initial.graphics,
      audio: initial.audio,
      pendingSaveWrites: initial.pendingSaveWrites,
    });
    deepStrictEqual(runtime.snapshot(), initial);
  });

  it('migrates revision-3 bus snapshots from source visuals while retaining RAM and rejecting malformed images', () => {
    const runtime = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        update() {
          api.call('mem_write', [0, 7], span);
          api.call('mem_write', [MEMORY.visual, 23], span);
        },
      }),
      {
        ...configuration,
        assets: {
          declarations: { dot: { kind: 'sprite', path: 'dot.m01g' } },
          files: {
            'dot.m01g': new TextEncoder().encode(
              JSON.stringify({ revision: 1, kind: 'sprite', width: 1, height: 1, frames: [[7]] }),
            ),
          },
        },
      },
    );
    runtime.runFrame(emptyInputFrame());
    const current = runtime.snapshot();
    expect(current.revision).toBe(6);
    const legacy = {
      ...current,
      revision: 3,
      save: decodeSaveValues(current.save.bytes),
      machine: legacyMachine(current.machine),
      memory: {
        ...current.memory,
        regions: current.memory.regions.filter((region) => region.address !== MEMORY.visual),
      },
    };
    runtime.restore(legacy);
    const migrated = runtime.snapshot();
    expect(
      migrated.memory.regions.find((region) => region.address === MEMORY.visual)?.bytes[0],
    ).toBe(7);
    expect(migrated.memory.regions[0]?.bytes[0]).toBe(7);
    runtime.restore(current);
    deepStrictEqual(runtime.snapshot(), current);
    const malformed = structuredClone(current);
    const visual = malformed.memory.regions.find((region) => region.address === MEMORY.visual);
    if (visual === undefined) throw new Error('missing visual region');
    Object.defineProperty(visual, 'bytes', { value: new Uint8Array(3) });
    expect(() => {
      runtime.restore(malformed);
    }).toThrow(/snapshot/);
    deepStrictEqual(runtime.snapshot(), current);
    expect(() => {
      runtime.restore({ ...current, revision: 3, machine: legacyMachine(current.machine) });
    }).toThrow(/snapshot/);
    deepStrictEqual(runtime.snapshot(), current);
  });

  it('migrates revision-4 device images without inventing prior work or fault state', () => {
    const runtime = createConsoleRuntime(factory, configuration);
    runtime.runFrame(emptyInputFrame());
    const current = runtime.snapshot();
    runtime.runFrame(emptyInputFrame());
    runtime.restore({
      ...current,
      revision: 4,
      machine: legacyMachine(current.machine),
      save: decodeSaveValues(current.save.bytes),
    });
    deepStrictEqual(runtime.snapshot(), {
      ...current,
      save: { ...current.save, commits: 0 },
      machine: {
        ...current.machine,
        budget: { ...current.machine.budget, used: 0, attribution: [] },
      },
    });
    for (const revision of [1, 2, 3, 4, 5, 7])
      expect(() => {
        runtime.restore({ ...current, revision });
      }).toThrow(/snapshot/);
  });

  it('latches system faults before a retry can reset devices and restores both healthy and faulted checkpoints', () => {
    const runtime = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        draw() {
          api.call('pal', [2, 7], span);
          api.call('mem_write', [MEMORY.system, 0], span);
        },
      }),
      configuration,
    );
    const healthy = runtime.snapshot();
    expect(() => runtime.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019021' }),
    );
    const faulted = runtime.snapshot();
    expect(faulted.machine.execution).toEqual({
      booted: true,
      updates: 1,
      phase: 'draw',
      rasterLine: null,
      fault: { code: 9021, sourceSpan: span },
    });
    for (let attempt = 0; attempt < 2; attempt += 1) {
      expect(() => runtime.runFrame(emptyInputFrame())).toThrow(
        expect.objectContaining({ code: 'M019014' }),
      );
      deepStrictEqual(runtime.snapshot(), faulted);
    }
    runtime.restore(healthy);
    deepStrictEqual(runtime.snapshot(), healthy);
    runtime.restore(faulted);
    deepStrictEqual(runtime.snapshot(), faulted);
    expect(() => runtime.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019014' }),
    );
  });

  it('charges system reads before encoding authoritative work usage and rejects inconsistent accounting', () => {
    const observed: unknown[] = [];
    const runtime = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        update() {},
        draw() {
          api.work(7, span);
          observed.push(api.call('mem_read16', [MEMORY.system + 32], span));
          observed.push(api.call('mem_read16', [MEMORY.system + 40], span));
          observed.push(api.call('mem_read', [MEMORY.system + 29], span));
        },
      }),
      configuration,
    );
    expect(runtime.runFrame(emptyInputFrame()).workUnits).toBe(12);
    expect(observed).toEqual([9, 50_000, 3]);
    const before = runtime.snapshot();
    expect(before.memory.regions.some((region) => region.address === MEMORY.system)).toBe(false);
    expect(() => {
      runtime.restore({
        ...before,
        machine: { ...before.machine, budget: { ...before.machine.budget, used: 11 } },
      });
    }).toThrow(/snapshot/);
    deepStrictEqual(runtime.snapshot(), before);
  });

  it('isolates instances, flushes saves once and resets per-frame debug/command buffers', () => {
    const first = createConsoleRuntime(factory, { ...configuration, debug: true });
    const second = createConsoleRuntime(factory, configuration);
    const initial = first.snapshot();
    const frame = first.runFrame(emptyInputFrame());
    expect(frame.saveWrites).toEqual([{ key: 'counter', value: 1 }]);
    expect(frame.drawCommands).toHaveLength(1);
    expect(frame.workUnits).toBe(3);
    expect(frame.debug?.trace).toEqual([
      {
        id: 1,
        sourceSpan: span,
        locals: { value: 7 },
        callStack: [{ name: 'draw', sourceSpan: span }],
      },
    ]);
    expect(first.runFrame(emptyInputFrame()).saveWrites).toEqual([{ key: 'counter', value: 2 }]);
    expect(second.runFrame(emptyInputFrame())).not.toHaveProperty('debug');
    expect(decodeSaveValues(second.snapshot().save.bytes)).toEqual({ counter: 1 });
    first.restore(initial);
    deepStrictEqual(first.runFrame(emptyInputFrame()), frame);
  });

  it('preserves source-mapped raster phase and draw/work ceiling faults', () => {
    const illegalRaster = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        raster(line) {
          if (line === 0) api.call('pixel', [0, 0, 7], span);
        },
      }),
      configuration,
    );
    expect(() => illegalRaster.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019011', sourceSpan: span }),
    );
    const commands = createConsoleRuntime(
      (api) => ({
        ...factory(api),
        draw() {
          for (let index = 0; index < 4097; index += 1) api.call('pixel', [0, 0, 7], span);
        },
      }),
      configuration,
    );
    expect(() => commands.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019010', sourceSpan: span }),
    );
    const limited = createConsoleRuntime(factory, { ...configuration, workUnitsPerFrame: 2 });
    expect(() => limited.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019001', sourceSpan: span }),
    );
  });

  it('reports source-mapped bus faults without changing memory for invalid operations', () => {
    const operations: readonly [string, number[], string][] = [
      ['mem_read', [-1], 'M019020'],
      ['mem_read16', [MEMORY.size - 1], 'M019020'],
      ['mem_write', [MEMORY.palette, 0], 'M019021'],
      ['mem_write', [MEMORY.transparency, 1], 'M019021'],
      ['mem_write', [MEMORY.display, 0], 'M019021'],
      ['mem_write', [MEMORY.rasterLive, 0], 'M019021'],
      ['mem_write', [MEMORY.back, 256], 'M019022'],
      ['mem_write16', [0, 65536], 'M019022'],
      ['mem_fill', [MEMORY.back, 1, 50_000], 'M019001'],
    ];
    for (const [name, args, code] of operations) {
      const runtime = createConsoleRuntime(
        (api) => ({
          ...factory(api),
          update() {},
          draw() {
            api.call(name, args, span);
          },
        }),
        configuration,
      );
      const before = runtime.snapshot().memory;
      expect(() => runtime.runFrame(emptyInputFrame())).toThrow(
        expect.objectContaining({ code, sourceSpan: span }),
      );
      deepStrictEqual(runtime.snapshot().memory, before);
    }
  });
});
