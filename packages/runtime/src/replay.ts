import { HARDWARE } from './hardware';
import { isMenuAction } from './menu';
import {
  BUTTONS,
  emptyInputFrame,
  isButton,
  type Button,
  type InputFrame,
  type PointerState,
} from './input';

const encoder = new TextEncoder();
// Preserve a BOM so JSON.parse rejects it, matching Rust's strict JSON reader.
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

export const REPLAY_MAX_FRAMES = 36_000;
export const REPLAY_MAX_BYTES = 8 * 1024 * 1024;

export interface ReplayTraceFrame {
  readonly frame: number;
  readonly duration?: number;
  readonly controllers: readonly {
    readonly port: 1 | 2 | 3 | 4;
    readonly buttons: readonly Button[];
  }[];
  readonly pointer?: PointerState;
  readonly menuAction?: number;
}

export interface ReplayTrace {
  readonly revision: 1;
  readonly frames: readonly ReplayTraceFrame[];
}

/** Encodes the public, source-readable revision-1 input trace used by Studio and the CLI. */
export function encodeReplayTrace(
  frames: readonly { frame: number; input: InputFrame }[],
): Uint8Array {
  if (frames.length > REPLAY_MAX_FRAMES) throw new RangeError('replay exceeds its frame limit');
  const encodedFrames: Array<{
    frame: number;
    duration?: number;
    controllers: ReplayTraceFrame['controllers'];
    pointer?: PointerState;
    menuAction?: number;
  }> = [];
  let previousSignature: string | undefined;
  for (const { frame, input } of frames) {
    const controllers = input.controllers.flatMap((controller, port) => {
      const buttons = BUTTONS.filter((button) => controller.buttons[button]);
      return buttons.length === 0 ? [] : [{ port: (port + 1) as 1 | 2 | 3 | 4, buttons }];
    });
    const pointer = input.pointer;
    const signature = JSON.stringify([controllers, pointer, input.menuAction]);
    const previous = encodedFrames.at(-1);
    const previousEnd = previous === undefined ? -1 : previous.frame + (previous.duration ?? 1);
    if (previous !== undefined && previousEnd === frame && signature === previousSignature) {
      previous.duration = (previous.duration ?? 1) + 1;
    } else {
      encodedFrames.push({
        frame,
        controllers,
        pointer,
        ...(input.menuAction === undefined ? {} : { menuAction: input.menuAction }),
      });
      previousSignature = signature;
    }
  }
  const trace: ReplayTrace = {
    revision: 1,
    frames: encodedFrames,
  };
  if (!isReplayTrace(trace, REPLAY_MAX_FRAMES)) throw new TypeError('replay input is invalid');
  const bytes = encoder.encode(`${JSON.stringify(trace)}\n`);
  if (bytes.length > REPLAY_MAX_BYTES) throw new RangeError('replay exceeds its byte limit');
  return bytes;
}

/** Strictly decodes a bounded trace before it can influence a cartridge frame. */
export function decodeReplayTrace(bytes: Uint8Array): ReplayTrace {
  if (bytes.length === 0 || bytes.length > REPLAY_MAX_BYTES)
    throw new RangeError('replay byte length is invalid');
  let value: unknown;
  try {
    value = JSON.parse(decoder.decode(bytes)) as unknown;
  } catch {
    throw new TypeError('replay JSON is invalid');
  }
  if (isReplayTrace(value, REPLAY_MAX_FRAMES)) return structuredClone(value);
  throw new TypeError('replay schema is invalid');
}

export function replayInputFrames(trace: ReplayTrace): ReadonlyMap<number, InputFrame> {
  if (!isReplayTrace(trace, REPLAY_MAX_FRAMES)) throw new TypeError('replay schema is invalid');
  const inputs = new Map<number, InputFrame>();
  for (const item of trace.frames) {
    for (let offset = 0; offset < (item.duration ?? 1); offset += 1)
      inputs.set(item.frame + offset, replayFrameInput(item));
  }
  return inputs;
}

export function isReplayTrace(
  value: unknown,
  frameLimit = REPLAY_MAX_FRAMES,
): value is ReplayTrace {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['revision', 'frames']) ||
    value.revision !== 1 ||
    !Array.isArray(value.frames) ||
    value.frames.length > frameLimit
  )
    return false;
  let previousEnd = 0;
  for (const item of value.frames) {
    if (
      !isRecord(item) ||
      !hasOnlyKeys(item, ['frame', 'duration', 'controllers', 'pointer', 'menuAction']) ||
      (Object.hasOwn(item, 'menuAction') && !isMenuAction(item.menuAction)) ||
      !('controllers' in item) ||
      typeof item.frame !== 'number' ||
      !Number.isSafeInteger(item.frame) ||
      item.frame < previousEnd ||
      item.frame >= frameLimit ||
      (item.duration !== undefined &&
        (typeof item.duration !== 'number' ||
          !Number.isSafeInteger(item.duration) ||
          item.duration < 1 ||
          item.duration > frameLimit - item.frame)) ||
      !isReplayControllers(item.controllers) ||
      (item.pointer !== undefined && !isReplayPointer(item.pointer))
    )
      return false;
    previousEnd = item.frame + (item.duration ?? 1);
  }
  return true;
}

function replayFrameInput(frame: ReplayTraceFrame): InputFrame {
  const input = emptyInputFrame();
  for (const controller of frame.controllers) {
    const target = input.controllers[controller.port - 1];
    if (target === undefined) continue;
    for (const button of controller.buttons)
      (target.buttons as Record<Button, boolean>)[button] = true;
  }
  return {
    ...input,
    ...(frame.pointer === undefined
      ? {}
      : {
          pointer: {
            ...frame.pointer,
            x: frame.pointer.x === 0 ? 0 : frame.pointer.x,
            y: frame.pointer.y === 0 ? 0 : frame.pointer.y,
          },
        }),
    ...(frame.menuAction === undefined ? {} : { menuAction: frame.menuAction }),
  };
}

function isReplayPointer(value: unknown): value is PointerState {
  return (
    isRecord(value) &&
    hasExactKeys(value, ['x', 'y', 'primary', 'secondary', 'inside']) &&
    typeof value.x === 'number' &&
    Number.isSafeInteger(value.x) &&
    value.x >= 0 &&
    value.x < HARDWARE.width &&
    typeof value.y === 'number' &&
    Number.isSafeInteger(value.y) &&
    value.y >= 0 &&
    value.y < HARDWARE.height &&
    typeof value.primary === 'boolean' &&
    typeof value.secondary === 'boolean' &&
    typeof value.inside === 'boolean'
  );
}

function isReplayControllers(value: unknown): value is ReplayTraceFrame['controllers'] {
  if (!Array.isArray(value) || value.length > 4) return false;
  const ports = new Set<number>();
  for (const controller of value) {
    if (
      !isRecord(controller) ||
      !hasExactKeys(controller, ['port', 'buttons']) ||
      typeof controller.port !== 'number' ||
      ![1, 2, 3, 4].includes(controller.port) ||
      ports.has(controller.port) ||
      !Array.isArray(controller.buttons) ||
      controller.buttons.length > BUTTONS.length ||
      new Set(controller.buttons).size !== controller.buttons.length ||
      !controller.buttons.every(isButton)
    )
      return false;
    ports.add(controller.port);
  }
  return true;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length && expected.every((key) => actual.includes(key));
}

function hasOnlyKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  return Object.keys(value).every((key) => expected.includes(key));
}
