import { RuntimeFault } from './errors';
import type { SourceSpan } from './protocol';

export const TEXT_LIMIT = 4096;
export const SPLIT_LIMIT = 256;
const ARITIES: Readonly<Record<string, number>> = {
  format_int: 3,
  format_num: 3,
  parse_int: 2,
  parse_num: 1,
  text_length: 1,
  text_slice: 3,
  text_code: 2,
  text_char: 1,
  text_split: 2,
  text_pad: 3,
  list_length: 1,
  text_at: 2,
};
export function isTextCall(name: string): boolean {
  return Object.hasOwn(ARITIES, name);
}

/** Bounded, locale-free conversions. Options use the compiler's ordinary value representation. */
export function textCall(
  name: string,
  args: readonly unknown[],
  span: SourceSpan,
  work: (units: number, span: SourceSpan) => void,
): unknown {
  const fail = (message: string): never => {
    throw new RuntimeFault('M019031', message, span);
  };
  if (!isTextCall(name) || args.length !== ARITIES[name]) fail(`${name}: invalid argument count`);
  let units = 4;
  for (const value of args) {
    if (typeof value === 'string') {
      if (value.length > TEXT_LIMIT) fail('text exceeds 4096 UTF-16 code units');
      units += value.length;
    }
  }
  work(units, span);
  const text = (index: number): string => {
    const value = args[index];
    return typeof value === 'string' && value.isWellFormed()
      ? value
      : fail('expected well-formed Text');
  };
  const integer = (index: number): number => {
    const value = args[index];
    return typeof value === 'number' && Number.isSafeInteger(value)
      ? value
      : fail('expected safe Int');
  };
  const bounded = (index: number, maximum: number): number => {
    const value = integer(index);
    return value >= 0 && value <= maximum
      ? value
      : fail(`argument must be between 0 and ${String(maximum)}`);
  };
  const some = (value: unknown): unknown => ({ __some: true, value });
  const base = (index: number): number => {
    const value = integer(index);
    return [2, 8, 10, 16].includes(value) ? value : fail('base must be 2, 8, 10 or 16');
  };
  switch (name) {
    case 'format_int': {
      const value = integer(0),
        radix = base(1),
        width = bounded(2, 64);
      const digits = Math.abs(value).toString(radix);
      const sign = value < 0 ? '-' : '';
      return sign + digits.padStart(Math.max(0, width - sign.length), '0');
    }
    case 'format_num': {
      const value = args[0];
      if (typeof value !== 'number' || !Number.isFinite(value) || Math.abs(value) >= 1e21)
        return fail('format_num requires a finite magnitude below 1e21');
      const decimals = bounded(1, 12),
        width = bounded(2, 64);
      return (value === 0 ? 0 : value).toFixed(decimals).padStart(width, ' ');
    }
    case 'parse_int': {
      let input = text(0);
      const radix = base(1);
      if (input.length === 0 || input.length > 256) return null;
      let sign = 1;
      if (input[0] === '-' || input[0] === '+') {
        sign = input[0] === '-' ? -1 : 1;
        input = input.slice(1);
      }
      if (input.length === 0) return null;
      let result = 0;
      for (const character of input.toLowerCase()) {
        const digit = '0123456789abcdef'.indexOf(character);
        if (digit < 0 || digit >= radix) return null;
        result = result * radix + digit;
        if (!Number.isSafeInteger(result)) return null;
      }
      return some(result === 0 ? 0 : sign * result);
    }
    case 'parse_num': {
      const input = text(0);
      if (
        input.length > 256 ||
        input.trim() !== input ||
        !/^[+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?$/.test(input)
      )
        return null;
      const value = Number(input);
      return Number.isFinite(value) ? some(value === 0 ? 0 : value) : null;
    }
    case 'text_length':
      return Array.from(text(0)).length;
    case 'text_slice': {
      const chars = Array.from(text(0));
      const start = bounded(1, chars.length),
        end = bounded(2, chars.length);
      if (end < start) return fail('text slice end precedes start');
      return chars.slice(start, end).join('');
    }
    case 'text_at':
    case 'text_code': {
      const chars = Array.from(text(0)),
        index = integer(1);
      const value = chars[index]?.codePointAt(0);
      if (name === 'text_code') return value === undefined ? null : some(value);
      return value === undefined ? fail('text index is outside its length') : value;
    }
    case 'text_char': {
      const value = integer(0);
      return value < 0 || value > 0x10ffff || (value >= 0xd800 && value <= 0xdfff)
        ? null
        : some(String.fromCodePoint(value));
    }
    case 'text_split': {
      const input = text(0),
        separator = text(1);
      if (separator.length === 0) return fail('split separator must not be empty');
      const parts = input.split(separator, SPLIT_LIMIT + 1);
      if (parts.length > SPLIT_LIMIT) return fail('split exceeds 256 parts');
      return parts;
    }
    case 'text_pad': {
      const input = text(0),
        width = bounded(1, TEXT_LIMIT),
        padding = text(2);
      if (Array.from(padding).length !== 1) return fail('padding must be one Unicode scalar');
      const resultLength =
        input.length + Math.max(0, width - Array.from(input).length) * padding.length;
      if (resultLength > TEXT_LIMIT) return fail('padded text exceeds 4096 UTF-16 code units');
      work(resultLength, span);
      return padding.repeat(Math.max(0, width - Array.from(input).length)) + input;
    }
    case 'list_length': {
      const value = args[0];
      return Array.isArray(value) && value.length <= 65535
        ? value.length
        : fail('expected bounded list or array');
    }
    default:
      return fail('unknown text helper');
  }
}
