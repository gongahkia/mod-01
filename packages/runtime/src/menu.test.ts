import { describe, expect, it } from 'vitest';
import { CartridgeMenu, isMenuSnapshot } from './menu';
import { emptyInputFrame, isInputFrame } from './input';
import { DeterministicMachine, isMachineSnapshot, type CartridgeFactory } from './machine';
import { decodeReplayTrace, encodeReplayTrace, replayInputFrames } from './replay';

const span = { start: 4, end: 9 };
const configuration = { seed: 7, updateRate: 30 as const, workUnitsPerFrame: 1000 };
const empty: CartridgeFactory = () => ({
  start() {},
  update() {},
  draw() {},
  raster() {},
  snapshot: () => ({ state: {}, tasks: [], nextTaskId: 1 }),
  restore() {},
  inspect: () => ({ state: {}, tasks: [], callStack: [] }),
});

describe('bounded cartridge menu state and recorded activation', () => {
  it('validates and charges registration before mutation, with ordered slots and explicit removal', () => {
    const menu = new CartridgeMenu();
    const costs: number[] = [];
    const charge = (cost: number) => {
      costs.push(cost);
    };
    menu.set(5, 'LAST', span, charge);
    menu.set(1, 'FIRST', span, charge);
    expect(menu.items()).toEqual([
      { slot: 1, label: 'FIRST' },
      { slot: 5, label: 'LAST' },
    ]);
    expect(costs).toEqual([5, 6]);
    const snapshot = menu.snapshot();
    for (const [slot, label] of [
      [0, 'BAD'],
      [6, 'BAD'],
      [1.5, 'BAD'],
      [true, 'BAD'],
      [1, 'x'.repeat(25)],
      [1, 'BAD\n'],
      [1, 'é'],
      [1, null],
    ]) {
      expect(() => {
        menu.set(slot, label, span, charge);
      }).toThrow(expect.objectContaining({ code: 'M019016', sourceSpan: span }));
      expect(menu.snapshot()).toEqual(snapshot);
    }
    expect(() => {
      menu.set(1, 'REPLACEMENT', span, () => {
        throw new Error('budget');
      });
    }).toThrow('budget');
    expect(menu.snapshot()).toEqual(snapshot);
    menu.set(1, '', span, charge);
    expect(menu.items()).toEqual([{ slot: 5, label: 'LAST' }]);
    menu.restore(snapshot);
    expect(menu.items()[0]?.label).toBe('FIRST');
    expect(isMenuSnapshot(snapshot)).toBe(true);
    for (const value of [
      [],
      Array(5),
      ['', null, null, null, null],
      ['x\n', null, null, null, null],
      Array(6).fill(null),
    ])
      expect(isMenuSnapshot(value)).toBe(false);
  });

  it('dispatches exactly once before the normal update, including odd frames at 30 Hz', () => {
    const calls: string[] = [];
    const machine = new DeterministicMachine(
      (api) => ({
        ...empty(api),
        start() {
          api.call('menu_item', [3, 'THIRD'], span);
        },
        menu(action) {
          calls.push(`menu:${String(action)}`);
          expect(machine.readSystemByte(29)).toBe(6);
        },
        update() {
          calls.push('update');
        },
        draw() {
          calls.push('draw');
        },
      }),
      configuration,
    );
    machine.runFrame(emptyInputFrame());
    const checkpoint = machine.snapshot();
    machine.runFrame({ ...emptyInputFrame(), menuAction: 3 });
    machine.runFrame(emptyInputFrame());
    expect(calls).toEqual(['update', 'draw', 'menu:3', 'draw', 'update', 'draw']);
    expect(machine.snapshot().execution.updates).toBe(2);
    expect(isMachineSnapshot(machine.snapshot())).toBe(true);
    const menu = machine.inspectMenu();
    Object.assign(menu[0] ?? {}, { label: 'HOST EDIT' });
    expect(machine.inspectMenu()[0]?.label).toBe('THIRD');
    const bad = structuredClone(checkpoint);
    (bad.menu as (string | null)[])[0] = 'bad\n';
    expect(() => {
      machine.restore(bad);
    }).toThrow(/snapshot/);
    machine.restore(checkpoint);
    expect(machine.snapshot()).toEqual(checkpoint);
  });

  it('rejects unregistered and raster actions with a restorable fault boundary', () => {
    const machine = new DeterministicMachine(empty, configuration);
    machine.runFrame(emptyInputFrame());
    const healthy = machine.snapshot();
    expect(() => machine.runFrame({ ...emptyInputFrame(), menuAction: 1 })).toThrow(
      expect.objectContaining({ code: 'M019016' }),
    );
    expect(isMachineSnapshot(machine.snapshot())).toBe(true);
    machine.restore(healthy);
    expect(machine.frame).toBe(1);
    const raster = new DeterministicMachine(
      (api) => ({
        ...empty(api),
        raster() {
          api.call('menu_item', [1, 'NO'], span);
        },
      }),
      configuration,
    );
    expect(() => raster.runFrame(emptyInputFrame())).toThrow(
      expect.objectContaining({ code: 'M019016' }),
    );
    expect(raster.inspectMenu()).toEqual([]);
  });

  it('strictly round trips action events without merging them into adjacent neutral frames', () => {
    const neutral = emptyInputFrame();
    const action = { ...neutral, menuAction: 5 };
    const frames = [
      { frame: 0, input: neutral },
      { frame: 1, input: action },
      { frame: 2, input: action },
      { frame: 3, input: neutral },
    ];
    const trace = decodeReplayTrace(encodeReplayTrace(frames));
    expect(trace.frames).toHaveLength(3);
    expect(trace.frames[1]).toMatchObject({ frame: 1, duration: 2, menuAction: 5 });
    expect([...replayInputFrames(trace)]).toEqual(frames.map(({ frame, input }) => [frame, input]));
    for (const value of [null, 0, 6, 1.5, '1', undefined]) {
      expect(isInputFrame({ ...neutral, menuAction: value })).toBe(false);
      expect(() =>
        decodeReplayTrace(
          new TextEncoder().encode(
            JSON.stringify({
              revision: 1,
              frames: [
                { frame: 0, controllers: [], menuAction: value === undefined ? null : value },
              ],
            }),
          ),
        ),
      ).toThrow(/schema/);
    }
    expect(isInputFrame(neutral)).toBe(true);
    expect(isInputFrame(action)).toBe(true);
  });
});
