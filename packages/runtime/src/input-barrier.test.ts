import { describe, expect, it } from 'vitest';
import { InputBarrier, isInputBarrierSnapshot } from './input-barrier';
import { BUTTONS, emptyInputFrame } from './input';

describe('cartridge input ownership', () => {
  it('blocks every held button on all four ports until release, never hover or explicit menu events', () => {
    const barrier = new InputBarrier();
    const held = emptyInputFrame();
    for (const controller of held.controllers)
      Object.assign(
        controller.buttons,
        Object.fromEntries(BUTTONS.map((button) => [button, true])),
      );
    Object.assign(held.pointer, { x: 10, y: 20, inside: true, primary: true, secondary: true });
    barrier.quarantine(held);
    expect(barrier.snapshot().controllers).toEqual([4095, 4095, 4095, 4095]);
    const first = barrier.apply({ ...held, menuAction: 3 });
    expect(first.controllers).toEqual(emptyInputFrame().controllers);
    expect(first.pointer).toEqual({ x: 10, y: 20, inside: true, primary: false, secondary: false });
    expect(first.menuAction).toBe(3);
    expect(barrier.apply(held).controllers).toEqual(first.controllers);
    barrier.apply(emptyInputFrame());
    expect(barrier.apply(held)).toEqual(held);
  });

  it('lets unrelated fresh buttons through and does not release other held controls', () => {
    const barrier = new InputBarrier();
    const held = emptyInputFrame();
    Object.assign(held.controllers[0].buttons, { a: true });
    Object.assign(held.controllers[3].buttons, { start: true });
    barrier.quarantine(held);
    const mixed = structuredClone(held);
    Object.assign(mixed.controllers[0].buttons, { b: true });
    Object.assign(mixed.controllers[2].buttons, { x: true });
    const effective = barrier.apply(mixed);
    expect(effective.controllers[0].buttons.a).toBe(false);
    expect(effective.controllers[0].buttons.b).toBe(true);
    expect(effective.controllers[2].buttons.x).toBe(true);
    expect(effective.controllers[3].buttons.start).toBe(false);
    Object.assign(mixed.controllers[0].buttons, { a: false });
    barrier.apply(mixed);
    Object.assign(mixed.controllers[0].buttons, { a: true });
    const next = barrier.apply(mixed);
    expect(next.controllers[0].buttons.a).toBe(true);
    expect(next.controllers[3].buttons.start).toBe(false);
  });

  it('restores cloned owner masks and rejects malformed snapshots without changing them', () => {
    const barrier = new InputBarrier();
    const held = emptyInputFrame();
    Object.assign(held.controllers[1].buttons, { a: true });
    barrier.quarantine(held);
    const saved = barrier.snapshot();
    barrier.apply(emptyInputFrame());
    barrier.restore(saved);
    Object.assign(saved.controllers, { 1: 0 });
    expect(barrier.apply(held).controllers[1].buttons.a).toBe(false);
    const before = barrier.snapshot();
    for (const invalid of [
      null,
      {},
      { ...before, revision: 2 },
      { ...before, extra: true },
      { ...before, controllers: Array(4) },
      { ...before, controllers: [0, 0, 0, 4096] },
      { ...before, controllers: [0, 0, -1, 0] },
      { ...before, controllers: [0, 0, 0] },
      { ...before, pointer: 4 },
      { ...before, pointer: 0.5 },
      Object.assign(Object.create({ revision: 1 }) as object, {
        controllers: [0, 0, 0, 0],
        pointer: 0,
        extra: true,
      }),
    ]) {
      expect(isInputBarrierSnapshot(invalid)).toBe(false);
      expect(() => {
        barrier.restore(invalid);
      }).toThrow(/snapshot/);
      expect(barrier.snapshot()).toEqual(before);
    }
    expect(isInputBarrierSnapshot(before)).toBe(true);
  });

  it('validates raw frames before consuming releases and returns independent frame records', () => {
    const barrier = new InputBarrier();
    const held = emptyInputFrame();
    Object.assign(held.controllers[0].buttons, { a: true });
    barrier.quarantine(held);
    const before = barrier.snapshot();
    for (const bad of [null, {}, { ...held, menuAction: null }, { ...held, controllers: [] }]) {
      expect(() => {
        barrier.apply(bad as typeof held);
      }).toThrow(/input/);
      expect(() => {
        barrier.quarantine(bad as typeof held);
      }).toThrow(/input/);
      expect(barrier.snapshot()).toEqual(before);
    }
    const returned = barrier.apply(held);
    Object.assign(returned.controllers[0].buttons, { a: true });
    expect(barrier.apply(held).controllers[0].buttons.a).toBe(false);
    expect(held.controllers[0].buttons.a).toBe(true);
  });
});
