import { describe, expect, it } from 'vitest';
import { mathCall } from './math';
const span = { start: 7, end: 12 };
const call = (name: string, ...args: unknown[]): number => mathCall(name, args, span, () => {});

describe('deterministic numeric helpers', () => {
  it('has conventional finite arithmetic and rounding semantics', () => {
    expect(call('abs', -17)).toBe(17);
    expect(call('min', -4, 8)).toBe(-4);
    expect(call('max', -4, 8)).toBe(8);
    expect(call('clamp', 15, -1, 10)).toBe(10);
    expect(call('floor', -1.2)).toBe(-2);
    expect(call('ceil', -1.2)).toBe(-1);
    expect(call('round', -1.5)).toBe(-1);
    expect(call('trunc', -1.8)).toBe(-1);
    expect(Object.is(call('round', -0.4), -0)).toBe(false);
  });
  it('matches independent math references over quadrants and extreme magnitudes', () => {
    for (let i = -1024; i <= 1024; i++) {
      const x = i / 10;
      expect(Math.abs(call('sin', x) - Math.sin(x))).toBeLessThan(2e-14);
      expect(Math.abs(call('cos', x) - Math.cos(x))).toBeLessThan(2e-14);
      expect(Math.abs(call('atan2', i, 257) - Math.atan2(i, 257))).toBeLessThan(1e-14);
      expect(Math.abs(call('atan2', i, -257) - Math.atan2(i, -257))).toBeLessThan(1e-14);
    }
    for (const x of [0, Number.MIN_VALUE, 1e-300, 0.5, 2, 4, 1e300, Number.MAX_VALUE]) {
      const expected = Math.sqrt(x);
      expect(Math.abs(call('sqrt', x) - expected)).toBeLessThanOrEqual(expected * 3e-16);
    }
    expect(call('sin', 0)).toBe(0);
    expect(call('cos', 0)).toBe(1.0000000000000002);
    expect(call('atan2', 0, 0)).toBe(0);
    for (const x of [-1_000_000, -999_999.9, 999_999.9, 1_000_000]) {
      expect(Math.abs(call('sin', x) - Math.sin(x))).toBeLessThan(1e-10);
      expect(Math.abs(call('cos', x) - Math.cos(x))).toBeLessThan(1e-10);
    }
  });
  it('charges before execution and rejects domain, range and malformed arguments', () => {
    const costs: number[] = [];
    mathCall('sqrt', [4], span, (units) => costs.push(units));
    mathCall('sin', [1], span, (units) => costs.push(units));
    expect(costs).toEqual([640, 32]);
    for (const [name, args] of [
      ['sqrt', [-1]],
      ['clamp', [1, 2, 0]],
      ['sin', [1_000_001]],
      ['floor', [Number.MAX_VALUE]],
      ['abs', [Infinity]],
      ['max', [1]],
    ] as const)
      expect(() => mathCall(name, args, span, () => {})).toThrow();
  });
});
