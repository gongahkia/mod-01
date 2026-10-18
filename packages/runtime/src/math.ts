import { RuntimeFault } from './errors';
import type { SourceSpan } from './protocol';

const PI = 3.141592653589793;
const HALF_PI = PI / 2;
const TAU = PI * 2;
const MATH_COSTS: Readonly<Record<string, readonly [number, number]>> = {
  abs: [1, 4],
  min: [2, 4],
  max: [2, 4],
  clamp: [3, 4],
  floor: [1, 4],
  ceil: [1, 4],
  round: [1, 4],
  trunc: [1, 4],
  sqrt: [1, 640],
  sin: [1, 32],
  cos: [1, 32],
  atan2: [2, 96],
};

export function isMathCall(name: string): boolean {
  return Object.hasOwn(MATH_COSTS, name);
}

/** Arithmetic-only transcendental algorithms: no host libm, clock or locale. */
export function mathCall(
  name: string,
  args: readonly unknown[],
  span: SourceSpan,
  work: (units: number, span: SourceSpan) => void,
): number {
  const spec = MATH_COSTS[name];
  if (
    spec === undefined ||
    args.length !== spec[0] ||
    !args.every((value) => typeof value === 'number' && Number.isFinite(value))
  )
    throw new RuntimeFault('M019030', `${name} requires finite numeric arguments`, span);
  work(spec[1], span);
  const [a = 0, b = 0, c = 0] = args as readonly number[];
  let result: number;
  switch (name) {
    case 'abs':
      result = Math.abs(a);
      break;
    case 'min':
      result = Math.min(a, b);
      break;
    case 'max':
      result = Math.max(a, b);
      break;
    case 'clamp':
      if (b > c) throw new RuntimeFault('M019030', 'clamp minimum exceeds maximum', span);
      result = Math.max(b, Math.min(a, c));
      break;
    case 'floor':
      result = Math.floor(a);
      break;
    case 'ceil':
      result = Math.ceil(a);
      break;
    case 'round':
      result = Math.round(a);
      break;
    case 'trunc':
      result = Math.trunc(a);
      break;
    case 'sqrt':
      if (a < 0) throw new RuntimeFault('M019030', 'sqrt requires a nonnegative value', span);
      result = squareRoot(a);
      break;
    case 'sin':
    case 'cos':
      if (Math.abs(a) > 1_000_000)
        throw new RuntimeFault('M019030', 'trig angle must be within +/-1000000 radians', span);
      result = sine(name === 'cos' ? a + HALF_PI : a);
      break;
    case 'atan2':
      result = angle(a, b);
      break;
    default:
      throw new RuntimeFault('M019030', 'unknown numeric helper', span);
  }
  if (
    !Number.isFinite(result) ||
    (['floor', 'ceil', 'round', 'trunc'].includes(name) && !Number.isSafeInteger(result))
  )
    throw new RuntimeFault('M019030', 'numeric result is outside its finite/safe range', span);
  return result === 0 ? 0 : result;
}

function squareRoot(value: number): number {
  if (value === 0) return 0;
  let normalized = value;
  let scale = 1;
  while (normalized >= 4) {
    normalized /= 4;
    scale *= 2;
  }
  while (normalized < 1) {
    normalized *= 4;
    scale /= 2;
  }
  let root = (normalized + 1) / 2;
  for (let iteration = 0; iteration < 8; iteration++) root = (root + normalized / root) / 2;
  return root * scale;
}

function sine(angle: number): number {
  let x = angle % TAU;
  if (x > PI) x -= TAU;
  if (x < -PI) x += TAU;
  if (x > HALF_PI) x = PI - x;
  if (x < -HALF_PI) x = -PI - x;
  let term = x;
  let sum = term;
  for (let n = 1; n <= 10; n++) {
    term *= (-x * x) / (2 * n * (2 * n + 1));
    sum += term;
  }
  return sum;
}

function arctangent(value: number): number {
  const reduced = value > 0.41421356237309503;
  const x = reduced ? (value - 1) / (value + 1) : value;
  let term = x;
  let sum = term;
  for (let n = 1; n <= 24; n++) {
    term *= -x * x;
    sum += term / (2 * n + 1);
  }
  return (reduced ? PI / 4 : 0) + sum;
}

function angle(y: number, x: number): number {
  if (x === 0 && y === 0) return 0;
  const ax = Math.abs(x),
    ay = Math.abs(y);
  let result = ay > ax ? HALF_PI - arctangent(ax / ay) : arctangent(ay / ax);
  if (x < 0) result = PI - result;
  return y < 0 ? -result : result;
}
