import { RuntimeFault } from './errors';
import type { SourceSpan } from './protocol';
const CALLS = new Set([
  'bit_and',
  'bit_or',
  'bit_xor',
  'bit_not',
  'bit_shl',
  'bit_shr',
  'bit_ushr',
]);
export function isBitCall(name: string): boolean {
  return CALLS.has(name);
}

/** Explicit 32-bit two's-complement words; large safe Ints never silently lose high bits. */
export function bitCall(name: string, args: readonly unknown[], span: SourceSpan): number {
  const fail = (): never => {
    throw new RuntimeFault(
      'M019032',
      'bit operations require 32-bit words and shift counts 0-31',
      span,
    );
  };
  if (!isBitCall(name) || args.length !== (name === 'bit_not' ? 1 : 2)) return fail();
  const word = (value: unknown): number =>
    typeof value === 'number' &&
    Number.isInteger(value) &&
    value >= -0x80000000 &&
    value <= 0xffffffff
      ? value
      : fail();
  const a = word(args[0]);
  if (name === 'bit_not') return ~a;
  const b = word(args[1]);
  if (name.startsWith('bit_s') || name === 'bit_ushr') {
    if (b < 0 || b > 31) return fail();
  }
  switch (name) {
    case 'bit_and':
      return a & b;
    case 'bit_or':
      return a | b;
    case 'bit_xor':
      return a ^ b;
    case 'bit_shl':
      return a << b;
    case 'bit_shr':
      return a >> b;
    case 'bit_ushr':
      return a >>> b;
    default:
      return fail();
  }
}
