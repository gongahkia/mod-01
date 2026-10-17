import { isInputFrame, type InputFrame } from './input';
import { isRuntimeAssetSource, type RuntimeAssetSource } from './asset-codec';
import { isAudioFrame, isSynthSnapshot, type AudioFrame, type SynthSnapshot } from './audio';
import { HARDWARE } from './hardware';
import { isMapQueryCatalog, type MapQueryAsset } from './map-query';
import { isSaveImage, isSaveValues, type SaveImage, type SaveWrite } from './save';
import type { MemoryRegionDescriptor } from './bus';

export interface SourceSpan {
  readonly source?: string;
  readonly start: number;
  readonly end: number;
}

export interface DebugStackFrame {
  readonly name: string;
  readonly sourceSpan: SourceSpan;
}

export interface DebugTraceEvent {
  readonly id: number;
  readonly sourceSpan: SourceSpan;
  readonly locals: unknown;
  readonly callStack: readonly DebugStackFrame[];
}

export interface DebugFrame {
  readonly trace: readonly DebugTraceEvent[];
  readonly truncated: boolean;
  readonly inspection: {
    readonly state: unknown;
    readonly tasks: unknown;
    readonly callStack: unknown;
  };
}

export interface SandboxConfiguration {
  readonly seed: number;
  readonly workUnitsPerFrame: number;
  readonly updateRate: 30 | 60;
  readonly hardwareRevision?: 1 | 2;
  readonly maps?: readonly MapQueryAsset[];
  readonly save?: SaveImage;
  readonly debug?: boolean;
  readonly assets?: RuntimeAssetSource;
  /** Host-validated canonical `.m01c` bytes exposed read-only on the hardware bus. */
  readonly rom?: Uint8Array;
}

export interface ConsoleOutput {
  readonly indexedPixels: Uint8Array;
  readonly audio: AudioFrame;
  readonly audioState: SynthSnapshot;
}

export type HostRequest =
  | {
      readonly id: number;
      readonly type: 'load';
      readonly moduleUrl: string;
      readonly configuration: SandboxConfiguration;
    }
  | { readonly id: number; readonly type: 'frame'; readonly input: InputFrame }
  | { readonly id: number; readonly type: 'debug-step'; readonly input: InputFrame }
  | { readonly id: number; readonly type: 'audit' }
  | { readonly id: number; readonly type: 'snapshot' }
  | { readonly id: number; readonly type: 'restore'; readonly snapshot: unknown }
  | {
      readonly id: number;
      readonly type: 'memory';
      readonly address: number;
      readonly length: number;
    }
  | {
      readonly id: number;
      readonly type: 'memory-edit';
      readonly address: number;
      readonly bytes: Uint8Array;
    };

export type WorkerResponse =
  | { readonly id: number; readonly type: 'loaded' }
  | {
      readonly id: number;
      readonly type: 'debug-step';
      readonly event?: DebugTraceEvent;
      readonly inspection?: DebugFrame['inspection'];
      readonly booted?: true;
      readonly frame?: Omit<Extract<WorkerResponse, { type: 'frame' }>, 'id' | 'type'>;
    }
  | {
      readonly id: number;
      readonly type: 'frame';
      readonly frame: number;
      readonly workUnits: number;
      readonly attribution: readonly {
        readonly sourceSpan: SourceSpan;
        readonly units: number;
      }[];
      readonly drawCommands: readonly ConsoleCommand[];
      readonly audioCommands: readonly ConsoleCommand[];
      readonly saveWrites: readonly SaveWrite[];
      readonly saveCommit?: Uint8Array;
      readonly output: ConsoleOutput;
      readonly debug?: DebugFrame;
    }
  | { readonly id: number; readonly type: 'snapshot'; readonly snapshot: unknown }
  | { readonly id: number; readonly type: 'restored' }
  | {
      readonly id: number;
      readonly type: 'memory';
      readonly address: number;
      readonly bytes: Uint8Array;
      readonly regions: readonly MemoryRegionDescriptor[];
    }
  | { readonly id: number; readonly type: 'memory-edited' }
  | {
      readonly id: number;
      readonly type: 'audit';
      readonly exposedCapabilities: readonly string[];
      readonly mathRandomAvailable: boolean;
    }
  | {
      readonly id: number;
      readonly type: 'error';
      readonly code: string;
      readonly message: string;
      readonly sourceSpan?: SourceSpan;
    };

export interface ConsoleCommand {
  readonly name: string;
  readonly arguments: readonly unknown[];
  readonly sourceSpan: SourceSpan;
  readonly rasterLine?: number;
}

export function isHostRequest(value: unknown): value is HostRequest {
  if (!isRecord(value) || !isNonNegativeInteger(value.id) || typeof value.type !== 'string') {
    return false;
  }
  switch (value.type) {
    case 'load':
      return (
        hasExactKeys(value, ['id', 'type', 'moduleUrl', 'configuration']) &&
        typeof value.moduleUrl === 'string' &&
        value.moduleUrl.startsWith('blob:') &&
        isSandboxConfiguration(value.configuration)
      );
    case 'frame':
      return hasExactKeys(value, ['id', 'type', 'input']) && isInputFrame(value.input);
    case 'debug-step':
      return hasExactKeys(value, ['id', 'type', 'input']) && isInputFrame(value.input);
    case 'snapshot':
    case 'audit':
      return hasExactKeys(value, ['id', 'type']);
    case 'restore':
      return hasExactKeys(value, ['id', 'type', 'snapshot']);
    case 'memory':
      return (
        hasExactKeys(value, ['id', 'type', 'address', 'length']) &&
        isMemoryRange(value.address, value.length)
      );
    case 'memory-edit':
      return (
        hasExactKeys(value, ['id', 'type', 'address', 'bytes']) &&
        value.bytes instanceof Uint8Array &&
        value.bytes.length > 0 &&
        value.bytes.length <= 256 &&
        isMemoryRange(value.address, value.bytes.length)
      );
    default:
      return false;
  }
}

export function isWorkerResponse(value: unknown): value is WorkerResponse {
  if (!isRecord(value) || !isNonNegativeInteger(value.id) || typeof value.type !== 'string') {
    return false;
  }
  switch (value.type) {
    case 'loaded':
    case 'restored':
    case 'memory-edited':
      return hasExactKeys(value, ['id', 'type']);
    case 'debug-step':
      return isDebugStepResponse(value);
    case 'snapshot':
      return hasExactKeys(value, ['id', 'type', 'snapshot']);
    case 'memory':
      return (
        hasExactKeys(value, ['id', 'type', 'address', 'bytes', 'regions']) &&
        value.bytes instanceof Uint8Array &&
        value.bytes.length > 0 &&
        value.bytes.length <= 256 &&
        isMemoryRange(value.address, value.bytes.length) &&
        Array.isArray(value.regions) &&
        value.regions.length <= 4096 &&
        value.regions.every(isMemoryRegionDescriptor)
      );
    case 'frame':
      return (
        hasExactKeys(value, [
          'id',
          'type',
          'frame',
          'workUnits',
          'attribution',
          'drawCommands',
          'audioCommands',
          'saveWrites',
          'output',
          ...(value.saveCommit === undefined ? [] : ['saveCommit']),
          ...(value.debug === undefined ? [] : ['debug']),
        ]) &&
        isNonNegativeInteger(value.frame) &&
        isNonNegativeInteger(value.workUnits) &&
        Array.isArray(value.attribution) &&
        value.attribution.every(isAttribution) &&
        Array.isArray(value.drawCommands) &&
        value.drawCommands.every(isConsoleCommand) &&
        Array.isArray(value.audioCommands) &&
        value.audioCommands.every(isConsoleCommand) &&
        Array.isArray(value.saveWrites) &&
        value.saveWrites.every(isSaveWrite) &&
        (value.saveCommit === undefined ||
          (value.saveCommit instanceof Uint8Array &&
            value.saveCommit.length === HARDWARE.saveCapacityBytes)) &&
        isConsoleOutput(value.output) &&
        (value.debug === undefined || isDebugFrame(value.debug))
      );
    case 'audit':
      return (
        hasExactKeys(value, ['id', 'type', 'exposedCapabilities', 'mathRandomAvailable']) &&
        Array.isArray(value.exposedCapabilities) &&
        value.exposedCapabilities.every((capability) => typeof capability === 'string') &&
        typeof value.mathRandomAvailable === 'boolean'
      );
    case 'error':
      return (
        hasExactKeys(
          value,
          value.sourceSpan === undefined
            ? ['id', 'type', 'code', 'message']
            : ['id', 'type', 'code', 'message', 'sourceSpan'],
        ) &&
        typeof value.code === 'string' &&
        typeof value.message === 'string' &&
        (value.sourceSpan === undefined || isSourceSpan(value.sourceSpan))
      );
    default:
      return false;
  }
}

function isMemoryRange(address: unknown, length: unknown): boolean {
  return (
    typeof address === 'number' &&
    Number.isSafeInteger(address) &&
    address >= 0 &&
    typeof length === 'number' &&
    Number.isSafeInteger(length) &&
    length > 0 &&
    length <= 256 &&
    address <= 0x400000 - length
  );
}

function isMemoryRegionDescriptor(value: unknown): value is MemoryRegionDescriptor {
  return (
    isRecord(value) &&
    hasExactKeys(value, ['name', 'address', 'length', 'writable']) &&
    typeof value.name === 'string' &&
    value.name.length > 0 &&
    value.name.length <= 128 &&
    typeof value.writable === 'boolean' &&
    typeof value.address === 'number' &&
    Number.isSafeInteger(value.address) &&
    value.address >= 0 &&
    typeof value.length === 'number' &&
    Number.isSafeInteger(value.length) &&
    value.length > 0 &&
    value.address <= 0x400000 - value.length
  );
}

export function isSandboxConfiguration(value: unknown): value is SandboxConfiguration {
  return (
    isRecord(value) &&
    hasExactKeys(value, [
      'seed',
      'workUnitsPerFrame',
      'updateRate',
      ...(value.hardwareRevision === undefined ? [] : ['hardwareRevision']),
      ...(value.maps === undefined ? [] : ['maps']),
      ...(value.save === undefined ? [] : ['save']),
      ...(value.debug === undefined ? [] : ['debug']),
      ...(value.assets === undefined ? [] : ['assets']),
      ...(value.rom === undefined ? [] : ['rom']),
    ]) &&
    Number.isSafeInteger(value.seed) &&
    isNonNegativeInteger(value.workUnitsPerFrame) &&
    value.workUnitsPerFrame > 0 &&
    value.workUnitsPerFrame <= HARDWARE.workUnitsPerFrame &&
    (value.updateRate === 30 || value.updateRate === 60) &&
    (value.hardwareRevision === undefined ||
      value.hardwareRevision === 1 ||
      value.hardwareRevision === 2) &&
    (value.maps === undefined || isMapQueryCatalog(value.maps)) &&
    (value.save === undefined || isSaveImage(value.save)) &&
    (value.debug === undefined || typeof value.debug === 'boolean') &&
    (value.assets === undefined || isRuntimeAssetSource(value.assets)) &&
    (value.rom === undefined ||
      (value.rom instanceof Uint8Array &&
        value.rom.length > 0 &&
        value.rom.length <= HARDWARE.cartridgeCapacityBytes))
  );
}

export function isConsoleOutput(value: unknown): value is ConsoleOutput {
  return (
    isRecord(value) &&
    hasExactKeys(value, ['indexedPixels', 'audio', 'audioState']) &&
    value.indexedPixels instanceof Uint8Array &&
    value.indexedPixels.length === HARDWARE.width * HARDWARE.height &&
    value.indexedPixels.every((color) => color < HARDWARE.paletteSize) &&
    isAudioFrame(value.audio) &&
    isSynthSnapshot(value.audioState)
  );
}

function isDebugFrame(value: unknown): value is DebugFrame {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ['trace', 'truncated', 'inspection']) ||
    !Array.isArray(value.trace) ||
    typeof value.truncated !== 'boolean' ||
    !isRecord(value.inspection) ||
    !hasExactKeys(value.inspection, ['state', 'tasks', 'callStack'])
  ) {
    return false;
  }
  return value.trace.every(isDebugTraceEvent);
}

function isDebugStepResponse(value: Record<string, unknown>): boolean {
  if (value.booted === true) return hasExactKeys(value, ['id', 'type', 'booted']);
  if (value.frame !== undefined) {
    return (
      hasExactKeys(value, ['id', 'type', 'frame']) &&
      isRecord(value.frame) &&
      isWorkerResponse({ id: value.id, type: 'frame', ...value.frame })
    );
  }
  return (
    hasExactKeys(value, ['id', 'type', 'event', 'inspection']) &&
    isDebugTraceEvent(value.event) &&
    isRecord(value.inspection) &&
    hasExactKeys(value.inspection, ['state', 'tasks', 'callStack'])
  );
}

function isDebugTraceEvent(value: unknown): value is DebugTraceEvent {
  return (
    isRecord(value) &&
    hasExactKeys(value, ['id', 'sourceSpan', 'locals', 'callStack']) &&
    isNonNegativeInteger(value.id) &&
    isSourceSpan(value.sourceSpan) &&
    Array.isArray(value.callStack) &&
    value.callStack.every(
      (frame) =>
        isRecord(frame) &&
        hasExactKeys(frame, ['name', 'sourceSpan']) &&
        typeof frame.name === 'string' &&
        isSourceSpan(frame.sourceSpan),
    )
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

function isSourceSpan(value: unknown): value is SourceSpan {
  return (
    isRecord(value) &&
    hasExactKeys(
      value,
      value.source === undefined ? ['start', 'end'] : ['source', 'start', 'end'],
    ) &&
    (value.source === undefined || isSourceName(value.source)) &&
    isNonNegativeInteger(value.start) &&
    isNonNegativeInteger(value.end) &&
    value.start <= value.end
  );
}

function isSourceName(value: unknown): value is string {
  if (typeof value !== 'string' || value.length === 0 || value.length > 512) return false;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code <= 31 || code === 127) return false;
  }
  return true;
}

function isAttribution(value: unknown): boolean {
  return (
    isRecord(value) &&
    hasExactKeys(value, ['sourceSpan', 'units']) &&
    isSourceSpan(value.sourceSpan) &&
    isNonNegativeInteger(value.units)
  );
}

function isConsoleCommand(value: unknown): value is ConsoleCommand {
  return (
    isRecord(value) &&
    hasExactKeys(
      value,
      value.rasterLine === undefined
        ? ['name', 'arguments', 'sourceSpan']
        : ['name', 'arguments', 'sourceSpan', 'rasterLine'],
    ) &&
    typeof value.name === 'string' &&
    Array.isArray(value.arguments) &&
    isSourceSpan(value.sourceSpan) &&
    (value.rasterLine === undefined ||
      (isNonNegativeInteger(value.rasterLine) && value.rasterLine < HARDWARE.height))
  );
}

function isSaveWrite(value: unknown): value is SaveWrite {
  return (
    isRecord(value) &&
    hasExactKeys(value, ['key', 'value']) &&
    typeof value.key === 'string' &&
    isSaveValues({ [value.key]: value.value })
  );
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length && expected.every((key) => actual.includes(key));
}
