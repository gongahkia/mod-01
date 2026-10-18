import { isRuntimeAssetSource, type RuntimeAssetSource } from './asset-codec';
import { isAudioFrame, isSynthSnapshot, type AudioFrame, type SynthSnapshot } from './audio';
import { HARDWARE } from './hardware';
import { isMapQueryCatalog, type MapQueryAsset } from './map-query';
import { isSaveImage, type SaveImage, type SaveWrite } from './save';

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

export interface ConsoleConfiguration {
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

export interface ConsoleFrame {
  readonly frame: number;
  readonly workUnits: number;
  readonly attribution: readonly { readonly sourceSpan: SourceSpan; readonly units: number }[];
  readonly drawCommands: readonly ConsoleCommand[];
  readonly audioCommands: readonly ConsoleCommand[];
  readonly saveWrites: readonly SaveWrite[];
  readonly saveCommit?: Uint8Array;
  readonly output: ConsoleOutput;
  readonly debug?: DebugFrame;
}

export interface ConsoleCommand {
  readonly name: string;
  readonly arguments: readonly unknown[];
  readonly sourceSpan: SourceSpan;
  readonly rasterLine?: number;
}

export function isConsoleConfiguration(value: unknown): value is ConsoleConfiguration {
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length && expected.every((key) => actual.includes(key));
}
