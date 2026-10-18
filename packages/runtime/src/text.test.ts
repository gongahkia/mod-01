import { describe, expect, it } from 'vitest';
import { textCall, TEXT_LIMIT } from './text';
const span = { start: 5, end: 9 };
const call = (name: string, ...args: unknown[]): unknown => textCall(name, args, span, () => {});
const some = (value: unknown): unknown => ({ __some: true, value });

describe('bounded text and conversion helpers', () => {
  it('formats locale-free HUD numbers, signs, bases and padding', () => {
    expect(call('format_int', -15, 16, 5)).toBe('-000f');
    expect(call('format_int', 7, 2, 5)).toBe('00111');
    expect(call('format_int', Number.MAX_SAFE_INTEGER, 16, 0)).toBe('1fffffffffffff');
    expect(call('format_num', 12.5, 2, 8)).toBe('   12.50');
    expect(call('format_num', -0, 0, 0)).toBe('0');
    expect(call('text_pad', 'hi', 4, '0')).toBe('00hi');
  });
  it('parses complete strings into typed options, never numeric prefixes', () => {
    expect(call('parse_int', '-7F', 16)).toEqual(some(-127));
    expect(call('parse_int', '9007199254740991', 10)).toEqual(some(Number.MAX_SAFE_INTEGER));
    expect(call('parse_num', '-.25e+2')).toEqual(some(-25));
    for (const input of ['', '+', ' 12', '12 ', '12tail', '0xff', '1.0', '9007199254740992'])
      expect(call('parse_int', input, 10)).toBeNull();
    for (const input of ['', '.', '1e', 'Infinity', 'NaN', '0xff', '1e999', ' 2', '2\n'])
      expect(call('parse_num', input)).toBeNull();
  });
  it('uses Unicode scalar indexes and preserves trailing split fields', () => {
    expect(call('text_length', 'a🙂b')).toBe(3);
    expect(call('text_slice', 'a🙂b', 1, 2)).toBe('🙂');
    expect(call('text_code', 'a🙂b', 1)).toEqual(some(128578));
    expect(call('text_at', 'a🙂b', 1)).toBe(128578);
    expect(call('text_char', 128578)).toEqual(some('🙂'));
    for (const code of [-1, 0xd800, 0xdfff, 0x110000]) expect(call('text_char', code)).toBeNull();
    expect(call('text_code', '', 0)).toBeNull();
    expect(call('text_split', 'a::b::', '::')).toEqual(['a', 'b', '']);
    expect(call('list_length', ['a', 'b'])).toBe(2);
  });
  it('rejects invalid types, ranges and capacity before unbounded allocation', () => {
    for (const [name, args] of [
      ['text_length', ['x'.repeat(TEXT_LIMIT + 1)]],
      ['text_at', ['hi', 2]],
      ['text_slice', ['hi', 2, 1]],
      ['text_slice', ['hi', -1, 2]],
      ['text_split', ['a', '']],
      ['text_split', [','.repeat(256), ',']],
      ['text_pad', ['hi', 4096, '🙂']],
      ['text_pad', ['hi', -1, ' ']],
      ['text_pad', ['hi', 4, 'ab']],
      ['format_int', [1, 3, 0]],
      ['format_int', [1.5, 10, 0]],
      ['format_num', [1e21, 2, 0]],
      ['format_num', [1, 13, 0]],
      ['format_num', [1, 2, 65]],
      ['list_length', ['not a list']],
      ['text_length', [7]],
      ['text_length', ['\ud800']],
    ] as const)
      expect(() => call(name, ...args)).toThrow();
    expect(call('text_length', 'x'.repeat(TEXT_LIMIT))).toBe(TEXT_LIMIT);
    expect((call('text_split', ','.repeat(255), ',') as unknown[]).length).toBe(256);
  });
  it('charges proportional input and output work', () => {
    const costs: number[] = [];
    textCall('text_pad', ['hi', 4, '0'], span, (units) => {
      costs.push(units);
    });
    expect(costs).toEqual([7, 4]);
    expect(() =>
      textCall('text_split', ['a,b', ','], span, () => {
        throw new Error('budget');
      }),
    ).toThrow('budget');
  });
});
