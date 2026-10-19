import { describe, expect, it } from 'vitest';
import { DeterministicMachine, isMachineSnapshot, type CartridgeFactory } from './machine';
import { emptyInputFrame } from './input';
import { EXECUTION_PHASES, type ExecutionPhase } from './system';

const span = { start: 12, end: 21 };
const configuration = { seed: 1, updateRate: 60 as const, workUnitsPerFrame: 100 };
const empty: CartridgeFactory = () => ({
  start() {},
  update() {},
  draw() {},
  raster() {},
  snapshot: () => ({ state: {}, tasks: [], nextTaskId: 1 }),
  restore() {},
  inspect: () => ({ state: {}, tasks: [], callStack: [] }),
});
const registers = (machine: DeterministicMachine): DataView =>
  new DataView(
    Uint8Array.from({ length: 64 }, (_, offset) => machine.readSystemByte(offset)).buffer,
  );

describe('authoritative system registers and execution snapshots', () => {
  for (const updateRate of [30, 60] as const) {
    it(`encodes actual scheduler phases, counters, RNG, time and work at ${String(updateRate)} Hz`, () => {
      const phases: [number, number, number, number, number][] = [];
      const record = () => {
        const view = registers(machine);
        phases.push([
          Number(view.getBigUint64(0, true)),
          Number(view.getBigUint64(8, true)),
          view.getUint8(29),
          view.getUint16(30, true),
          view.getUint8(48),
        ]);
        expect(view.getFloat64(16, true)).toBe(machine.cartridgeTimeSeconds);
        expect(view.getUint8(28)).toBe(updateRate);
        expect(view.getBigUint64(40, true)).toBe(100n);
      };
      const machine = new DeterministicMachine(
        (api) => ({
          ...empty(api),
          start: record,
          update: record,
          draw: record,
          raster(line) {
            if (line === 143) record();
          },
        }),
        { ...configuration, updateRate },
      );
      const unbooted = machine.snapshot();
      expect(isMachineSnapshot(unbooted)).toBe(true);
      expect(registers(machine).getUint8(48)).toBe(0);
      machine.boot();
      for (let frame = 0; frame < 3; frame += 1) machine.runFrame(emptyInputFrame());
      const expected = [[0, 0, 1, 65535, 2]];
      let updates = 0;
      for (let frame = 0; frame < 3; frame += 1) {
        if (updateRate === 60 || frame % 2 === 0) {
          expected.push([frame, updates, 2, 65535, 3]);
          updates += 1;
        }
        expected.push([frame, updates, 3, 65535, 3], [frame, updates, 4, 143, 3]);
      }
      expect(phases).toEqual(expected);
      const view = registers(machine);
      expect(view.getBigUint64(0, true)).toBe(3n);
      expect(view.getBigUint64(8, true)).toBe(BigInt(updates));
      expect(view.getUint8(29)).toBe(0);
      expect(view.getUint16(30, true)).toBe(65535);
      expect(view.getUint8(48)).toBe(1);
      expect(Array.from({ length: 15 }, (_, i) => view.getUint8(49 + i))).toEqual(
        Array(15).fill(0),
      );
      expect(view.getUint32(24, true)).toBe(1);
      machine.call('rng_int', [0, 9], span);
      expect(registers(machine).getUint32(24, true)).toBe(machine.snapshot().rngState);
      expect(registers(machine).getUint32(24, true)).not.toBe(1);
      machine.work(17, span);
      expect(registers(machine).getBigUint64(32, true)).toBe(17n);
      const current = machine.snapshot();
      machine.restore(unbooted);
      expect(machine.snapshot()).toEqual(unbooted);
      machine.runFrame(emptyInputFrame());
      expect(phases.at(-4)).toEqual([0, 0, 1, 65535, 2]);
      machine.restore(current);
      expect(machine.snapshot()).toEqual(current);
    });
  }

  for (const phase of ['start', 'update', 'draw', 'raster'] as const) {
    it(`retains the ${phase} fault boundary and source span without retrying execution`, () => {
      let fail = true;
      let calls = 0;
      const machine = new DeterministicMachine((api) => {
        const run = (active: ExecutionPhase, line?: number) => {
          if (active !== phase || (active === 'raster' && line !== 7)) return;
          calls += 1;
          if (fail) api.work(101, span);
        };
        return {
          ...empty(api),
          start: () => {
            run('start');
          },
          update: () => {
            run('update');
          },
          draw: () => {
            run('draw');
          },
          raster: (line) => {
            run('raster', line);
          },
        };
      }, configuration);
      const healthy = machine.snapshot();
      expect(() => machine.runFrame(emptyInputFrame())).toThrow(
        expect.objectContaining({ code: 'M019001' }),
      );
      const faulted = machine.snapshot();
      expect(isMachineSnapshot(faulted)).toBe(true);
      expect(faulted.execution).toEqual({
        booted: phase !== 'start',
        updates: phase === 'draw' || phase === 'raster' ? 1 : 0,
        phase,
        rasterLine: phase === 'raster' ? 7 : null,
        fault: { code: 9001, sourceSpan: span },
      });
      const view = registers(machine);
      expect(view.getUint8(29)).toBe(EXECUTION_PHASES.indexOf(phase));
      expect(view.getUint8(48)).toBe(phase === 'start' ? 4 : 5);
      expect(view.getUint16(52, true)).toBe(9001);
      expect(view.getUint32(56, true)).toBe(12);
      expect(view.getUint32(60, true)).toBe(21);
      expect(view.getBigUint64(32, true)).toBe(101n);
      expect(() => machine.runFrame(emptyInputFrame())).toThrow(
        expect.objectContaining({ code: 'M019014' }),
      );
      expect(calls).toBe(1);
      expect(machine.snapshot()).toEqual(faulted);
      machine.restore(healthy);
      fail = false;
      machine.runFrame(emptyInputFrame());
      expect(machine.frame).toBe(1);
      machine.restore(faulted);
      expect(machine.snapshot()).toEqual(faulted);
      expect(() => {
        machine.boot();
      }).toThrow(expect.objectContaining({ code: 'M019014' }));
    });
  }

  it('rejects inconsistent snapshots and configuration mismatches without mutating state', () => {
    const machine = new DeterministicMachine(empty, configuration);
    machine.runFrame(emptyInputFrame());
    const before = machine.snapshot();
    for (const execution of [
      { ...before.execution, updates: 0 },
      { ...before.execution, booted: false },
      { ...before.execution, phase: 'draw' },
      { ...before.execution, rasterLine: 0 },
      { ...before.execution, fault: { code: 9001, sourceSpan: { start: 5, end: 4 } } },
      { ...before.execution, fault: { code: 9001, sourceSpan: { start: 0, end: 2 ** 32 } } },
    ]) {
      expect(() => {
        machine.restore({ ...before, execution });
      }).toThrow(/snapshot/);
      expect(machine.snapshot()).toEqual(before);
    }
    for (const altered of [
      { ...before, updateRate: 30 },
      { ...before, budget: { ...before.budget, limit: 99 } },
      { ...before, rngState: 0 },
    ]) {
      expect(() => {
        machine.restore(altered);
      }).toThrow(/snapshot/);
      expect(machine.snapshot()).toEqual(before);
    }
  });

  it('faults at the exact integer counter boundary instead of rounding or wrapping', () => {
    const machine = new DeterministicMachine(empty, configuration);
    machine.boot();
    const initial = machine.snapshot();
    machine.restore({
      ...initial,
      frame: Number.MAX_SAFE_INTEGER,
      execution: { ...initial.execution, updates: Number.MAX_SAFE_INTEGER },
    });
    expect(registers(machine).getBigUint64(0, true)).toBe(BigInt(Number.MAX_SAFE_INTEGER));
    expect(() => machine.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019012' }),
    );
    expect(machine.frame).toBe(Number.MAX_SAFE_INTEGER);
    expect(isMachineSnapshot(machine.snapshot())).toBe(true);
  });

  it('rejects frame-only snapshots instead of inventing scheduler and RNG defaults', () => {
    const machine = new DeterministicMachine(empty, { ...configuration, updateRate: 30 });
    const initial = machine.snapshot();
    const legacy = {
      revision: 1,
      frame: 3,
      rngState: 0,
      cartridge: initial.cartridge,
      input: initial.input,
      previousInput: initial.previousInput,
    };
    expect(isMachineSnapshot(legacy)).toBe(false);
    expect(() => {
      machine.restore(legacy);
    }).toThrow(/snapshot/);
    expect(machine.snapshot()).toEqual(initial);
    expect(() => {
      machine.restore({ ...initial, rngState: 0 });
    }).toThrow(/snapshot/);
    expect(machine.snapshot()).toEqual(initial);
    machine.runFrame(emptyInputFrame());
    expect(machine.frame).toBe(1);
    expect(machine.snapshot().execution.updates).toBe(1);
  });

  it('rolls back a cartridge restore failure and rejects snapshots taken during active callbacks', () => {
    let state = 0;
    const machine = new DeterministicMachine(
      (api) => ({
        ...empty(api),
        update() {
          expect(() => machine.snapshot()).toThrow(/completed frame or fault boundary/);
          state += 1;
        },
        snapshot: () => ({ state, tasks: [], nextTaskId: 1 }),
        restore(snapshot) {
          state = snapshot.state as number;
          if (state === 99) throw new TypeError('rejected cartridge state');
        },
      }),
      configuration,
    );
    machine.runFrame(emptyInputFrame());
    const before = machine.snapshot();
    expect(() => {
      machine.restore({ ...before, cartridge: { ...before.cartridge, state: 99 } });
    }).toThrow(/rejected/);
    expect(machine.snapshot()).toEqual(before);
    expect(state).toBe(1);
  });
});
