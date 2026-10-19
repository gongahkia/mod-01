import { HARDWARE } from './hardware';
import type { SourceSpan } from './protocol';
import type { WorkBudgetSnapshot } from './budget';

export const EXECUTION_PHASES = [
  'idle',
  'start',
  'update',
  'draw',
  'raster',
  'output',
  'menu',
] as const;
export type ExecutionPhase = (typeof EXECUTION_PHASES)[number];

export interface MachineFault {
  readonly code: number;
  readonly sourceSpan: SourceSpan;
}

export interface ExecutionSnapshot {
  readonly booted: boolean;
  readonly updates: number;
  readonly phase: ExecutionPhase;
  readonly rasterLine: number | null;
  readonly fault: MachineFault | null;
}

export function isExecutionSnapshot(
  value: unknown,
  frame: number,
  rate: 30 | 60,
  budget: WorkBudgetSnapshot,
): value is ExecutionSnapshot {
  if (
    !isRecord(value) ||
    Object.keys(value).length !== 5 ||
    typeof value.booted !== 'boolean' ||
    typeof value.updates !== 'number' ||
    !Number.isSafeInteger(value.updates) ||
    value.updates < 0 ||
    typeof value.phase !== 'string' ||
    !(EXECUTION_PHASES as readonly string[]).includes(value.phase) ||
    (value.phase === 'raster'
      ? typeof value.rasterLine !== 'number' ||
        !Number.isInteger(value.rasterLine) ||
        value.rasterLine < 0 ||
        value.rasterLine >= HARDWARE.height
      : value.rasterLine !== null) ||
    (value.fault !== null && !isMachineFault(value.fault))
  )
    return false;
  const base = rate === 60 ? frame : Math.ceil(frame / 2);
  const updated =
    (value.phase === 'draw' || value.phase === 'raster' || value.phase === 'output') &&
    (rate === 60 || frame % 2 === 0);
  if (
    value.updates !== base + Number(updated) ||
    (value.phase !== 'idle' && frame === Number.MAX_SAFE_INTEGER) ||
    (!value.booted && (frame !== 0 || (value.phase !== 'idle' && value.phase !== 'start'))) ||
    (value.phase === 'start' && value.booted) ||
    (value.phase === 'update' && rate === 30 && frame % 2 !== 0)
  )
    return false;
  return value.fault !== null || (value.phase === 'idle' && budget.used <= budget.limit);
}

export function isMachineFault(value: unknown): value is MachineFault {
  return (
    isRecord(value) &&
    Object.keys(value).length === 2 &&
    typeof value.code === 'number' &&
    Number.isInteger(value.code) &&
    value.code >= 9000 &&
    value.code <= 9999 &&
    isFaultSpan(value.sourceSpan)
  );
}

export function isFaultSpan(value: unknown): value is SourceSpan {
  return (
    isRecord(value) &&
    Object.keys(value).length === 2 &&
    typeof value.start === 'number' &&
    Number.isInteger(value.start) &&
    value.start >= 0 &&
    typeof value.end === 'number' &&
    Number.isInteger(value.end) &&
    value.end >= value.start &&
    value.end <= 0xffffffff
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export interface SystemRegisters {
  readonly frame: number;
  readonly updates: number;
  readonly seconds: number;
  readonly rngState: number;
  readonly updateRate: 30 | 60;
  readonly phase: ExecutionPhase;
  readonly rasterLine: number | undefined;
  readonly booted: boolean;
  readonly fault: MachineFault | null;
  readonly used: number;
  readonly limit: number;
}

/** Wire encoding of current device-owned state, not a retained register image. */
export function systemRegisterByte(state: SystemRegisters, offset: number): number {
  if (!Number.isInteger(offset) || offset < 0 || offset >= 64) return 0;
  const byte = (value: number, index: number): number => Math.floor(value / 2 ** (index * 8)) & 255;
  if (offset < 8) return byte(state.frame, offset);
  if (offset < 16) return byte(state.updates, offset - 8);
  if (offset < 24) {
    const encoded = new DataView(new ArrayBuffer(8));
    encoded.setFloat64(0, state.seconds, true);
    return encoded.getUint8(offset - 16);
  }
  if (offset < 28) return byte(state.rngState, offset - 24);
  if (offset === 28) return state.updateRate;
  if (offset === 29) return EXECUTION_PHASES.indexOf(state.phase);
  if (offset < 32) return byte(state.rasterLine ?? 65535, offset - 30);
  if (offset < 40) return byte(state.used, offset - 32);
  if (offset < 48) return byte(state.limit, offset - 40);
  if (offset === 48)
    return (
      Number(state.booted) |
      (Number(state.phase !== 'idle' && state.fault === null) << 1) |
      (Number(state.fault !== null) << 2)
    );
  if (offset < 52) return 0;
  if (offset < 54) return byte(state.fault?.code ?? 0, offset - 52);
  if (offset < 56) return 0;
  if (offset < 60) return byte(state.fault?.sourceSpan.start ?? 0, offset - 56);
  return byte(state.fault?.sourceSpan.end ?? 0, offset - 60);
}
