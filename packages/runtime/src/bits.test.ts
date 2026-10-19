import { describe, expect, it } from 'vitest';
import { bitCall } from './bits';
const span = { start: 4, end: 9 };
const call = (name: string, ...args: number[]): number => bitCall(name, args, span);
describe('explicit 32-bit word operations', () => {
  it('wraps signed results, preserves unsigned shift results and sign extension', () => {
    expect(call('bit_and', 0xffffffff, 255)).toBe(255);
    expect(call('bit_or', 0x80000000, 1)).toBe(-2147483647);
    expect(call('bit_xor', 0xffffffff, 255)).toBe(-256);
    expect(call('bit_not', 0xffffffff)).toBe(0);
    expect(call('bit_shl', 1, 31)).toBe(-2147483648);
    expect(call('bit_shr', -2147483648, 31)).toBe(-1);
    expect(call('bit_ushr', -1, 0)).toBe(4294967295);
  });
  it('does not silently truncate high bits or mask invalid shift counts', () => {
    for (const value of [1.5, NaN, Infinity, 0x100000000, -2147483649])
      expect(() => call('bit_and', value, 0)).toThrow(
        expect.objectContaining({ code: 'M019032', sourceSpan: span }),
      );
    for (const shift of [-1, 32, 33, 1.5])
      for (const name of ['bit_shl', 'bit_shr', 'bit_ushr'])
        expect(() => call(name, 1, shift)).toThrow();
    expect(() => call('bit_not', 1, 2)).toThrow();
    expect(() => call('unknown', 0)).toThrow();
  });
});
