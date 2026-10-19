import { createHash } from 'node:crypto';

import { decodeRuntimeAssets, type ProjectAssetDeclaration } from './asset-codec';
import { createConsoleRuntime, type ConsoleFrame } from './console-runtime';
import { RuntimeFault } from './errors';
import { HARDWARE } from './hardware';
import { emptyInputFrame } from './input';
import type { CartridgeFactory } from './machine';
import {
  isReplayTrace,
  replayInputFrames,
  type ReplayTrace,
  type ReplayTraceFrame,
} from './replay';

export type HeadlessTraceFrame = ReplayTraceFrame;
export type HeadlessTrace = ReplayTrace;

export interface HeadlessRequest {
  readonly revision: 1;
  readonly javascript: string;
  readonly manifest: {
    readonly id: string;
    readonly updateRate: 30 | 60;
    readonly display: string | null;
    readonly assets: Readonly<Record<string, ProjectAssetDeclaration>>;
  };
  readonly entries: Readonly<Record<string, readonly number[]>>;
  readonly rom: readonly number[];
  readonly seed: number;
  readonly frames: number;
  readonly trace: HeadlessTrace;
  readonly save: readonly number[];
}

export interface HeadlessFrameResult {
  readonly frame: number;
  readonly workUnits: number;
  readonly framebufferSha256: string;
  readonly stateSha256: string;
  readonly audioCommandSha256: string;
  readonly pcmSha256: string;
  readonly saveSha256: string;
}

export interface HeadlessResult {
  readonly revision: 1;
  readonly cartridge: { readonly id: string; readonly bytes: number; readonly sha256: string };
  readonly configuration: {
    readonly seed: number;
    readonly requestedFrames: number;
    readonly updateRate: 30 | 60;
    readonly inputTraceSha256: string;
    readonly initialSaveSha256: string;
  };
  readonly frames: readonly HeadlessFrameResult[];
  readonly summary: {
    readonly completedFrames: number;
    readonly workPeak: number;
    readonly commandPeak: number;
    readonly drawCommandPeak: number;
    readonly audioCommandPeak: number;
    readonly voicePeak: number;
    readonly busMappedBytes: number;
    readonly finalFramebufferSha256: string;
    readonly finalStateSha256: string;
    readonly audioCommandsSha256: string;
    readonly pcmSha256: string;
    readonly finalSaveSha256: string;
  };
  readonly fault?: {
    readonly frame: number;
    readonly code: string;
    readonly message: string;
    readonly sourceSpan: { readonly start: number; readonly end: number };
  };
}

/** Runs validated compiler output through the same production core used by native Studio/games. */
export async function runHeadless(value: unknown): Promise<HeadlessResult> {
  if (!isHeadlessRequest(value)) throw new TypeError('invalid MOD-01 headless request');
  const request = value;
  const loaded: unknown = await import(
    `data:text/javascript;base64,${Buffer.from(request.javascript).toString('base64')}`
  );
  const factory = readFactory(loaded);
  const files = Object.fromEntries(
    Object.entries(request.entries).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
  );
  // Decode before constructing the runtime so malformed/unbounded asset data cannot partially boot.
  decodeRuntimeAssets(request.manifest.assets, files, request.manifest.display);
  const runtime = createConsoleRuntime(factory, {
    seed: request.seed,
    workUnitsPerFrame: HARDWARE.workUnitsPerFrame,
    updateRate: request.manifest.updateRate,
    assets: {
      declarations: request.manifest.assets,
      files,
      displayPath: request.manifest.display,
    },
    save: Uint8Array.from(request.save),
    rom: Uint8Array.from(request.rom),
  });
  const inputs = replayInputFrames(request.trace);
  const frameResults: HeadlessFrameResult[] = [];
  const commandHash = createHash('sha256');
  const pcmHash = createHash('sha256');
  let workPeak = 0;
  let commandPeak = 0;
  let drawCommandPeak = 0;
  let audioCommandPeak = 0;
  let voicePeak = 0;
  let fault: HeadlessResult['fault'];

  for (let frame = 0; frame < request.frames; frame += 1) {
    try {
      const result = runtime.runFrame(inputs.get(frame) ?? emptyInputFrame());
      const snapshot = runtime.snapshot();
      const framePcm = pcmBytes(result);
      const commands = canonicalBytes(result.audioCommands);
      commandHash.update(commands);
      pcmHash.update(framePcm);
      workPeak = Math.max(workPeak, result.workUnits);
      drawCommandPeak = Math.max(drawCommandPeak, result.drawCommands.length);
      audioCommandPeak = Math.max(audioCommandPeak, result.audioCommands.length);
      commandPeak = Math.max(commandPeak, result.drawCommands.length + result.audioCommands.length);
      voicePeak = Math.max(voicePeak, result.output.audio.activeVoices);
      frameResults.push({
        frame: result.frame,
        workUnits: result.workUnits,
        framebufferSha256: sha256(result.output.indexedPixels),
        stateSha256: sha256(canonicalBytes(snapshot)),
        audioCommandSha256: sha256(commands),
        pcmSha256: sha256(framePcm),
        saveSha256: sha256(snapshot.save.committed),
      });
    } catch (error: unknown) {
      if (!(error instanceof RuntimeFault)) throw error;
      fault = {
        frame,
        code: error.code,
        message: error.message,
        sourceSpan: error.sourceSpan,
      };
      break;
    }
  }

  const finalSnapshot = runtime.snapshot();
  const final = frameResults.at(-1);
  return {
    revision: 1,
    cartridge: {
      id: request.manifest.id,
      bytes: request.rom.length,
      sha256: sha256(Uint8Array.from(request.rom)),
    },
    configuration: {
      seed: request.seed,
      requestedFrames: request.frames,
      updateRate: request.manifest.updateRate,
      inputTraceSha256: sha256(canonicalBytes(request.trace)),
      initialSaveSha256: sha256(Uint8Array.from(request.save)),
    },
    frames: frameResults,
    summary: {
      completedFrames: frameResults.length,
      workPeak,
      commandPeak,
      drawCommandPeak,
      audioCommandPeak,
      voicePeak,
      busMappedBytes: runtime.memoryRegions().reduce((total, region) => total + region.length, 0),
      finalFramebufferSha256: final?.framebufferSha256 ?? sha256(new Uint8Array()),
      finalStateSha256: sha256(canonicalBytes(finalSnapshot)),
      audioCommandsSha256: commandHash.digest('hex'),
      pcmSha256: pcmHash.digest('hex'),
      finalSaveSha256: sha256(finalSnapshot.save.committed),
    },
    ...(fault === undefined ? {} : { fault }),
  };
}

function isHeadlessRequest(value: unknown): value is HeadlessRequest {
  if (!isRecord(value) || !hasExactKeys(value, HEADLESS_KEYS) || value.revision !== 1) return false;
  if (
    typeof value.javascript !== 'string' ||
    value.javascript.length === 0 ||
    value.javascript.length > 2 * 1024 * 1024 ||
    !isManifest(value.manifest) ||
    !isByteRecord(value.entries, 4096, 16 * 1024 * 1024) ||
    !isByteArray(value.rom, HARDWARE.cartridgeCapacityBytes) ||
    typeof value.seed !== 'number' ||
    !Number.isSafeInteger(value.seed) ||
    value.seed < 0 ||
    value.seed > 0xffff_ffff ||
    typeof value.frames !== 'number' ||
    !Number.isSafeInteger(value.frames) ||
    value.frames < 0 ||
    value.frames > 36_000 ||
    !isReplayTrace(value.trace, value.frames) ||
    !isByteArray(value.save, HARDWARE.saveCapacityBytes)
  )
    return false;
  return true;
}

const HEADLESS_KEYS = [
  'revision',
  'javascript',
  'manifest',
  'entries',
  'rom',
  'seed',
  'frames',
  'trace',
  'save',
] as const;

function isManifest(value: unknown): value is HeadlessRequest['manifest'] {
  return (
    isRecord(value) &&
    hasExactKeys(value, ['id', 'updateRate', 'display', 'assets']) &&
    typeof value.id === 'string' &&
    /^[a-z0-9][a-z0-9.-]{2,63}$/.test(value.id) &&
    (value.updateRate === 30 || value.updateRate === 60) &&
    (value.display === null || typeof value.display === 'string') &&
    isAssetDeclarations(value.assets)
  );
}

function isAssetDeclarations(
  value: unknown,
): value is Readonly<Record<string, ProjectAssetDeclaration>> {
  if (!isRecord(value) || Object.keys(value).length > 4096) return false;
  return Object.values(value).every(
    (asset) =>
      isRecord(asset) &&
      hasExactKeys(asset, ['kind', 'path']) &&
      typeof asset.kind === 'string' &&
      ['sprite', 'animation', 'tile_set', 'map', 'font', 'sound', 'music'].includes(asset.kind) &&
      typeof asset.path === 'string' &&
      asset.path.length > 0 &&
      asset.path.length <= 1024,
  );
}

function isByteRecord(
  value: unknown,
  entryLimit: number,
  byteLimit: number,
): value is Readonly<Record<string, readonly number[]>> {
  if (!isRecord(value) || Object.keys(value).length > entryLimit) return false;
  let total = 0;
  for (const [path, bytes] of Object.entries(value)) {
    if (path.length === 0 || path.length > 1024 || !isByteArray(bytes, byteLimit - total))
      return false;
    total += bytes.length;
  }
  return true;
}

function isByteArray(value: unknown, limit: number): value is readonly number[] {
  return (
    Array.isArray(value) &&
    value.length <= limit &&
    Object.keys(value).length === value.length &&
    value.every((byte) => Number.isSafeInteger(byte) && byte >= 0 && byte <= 255)
  );
}

function readFactory(value: unknown): CartridgeFactory {
  if (!isRecord(value) || typeof value.default !== 'function') {
    throw new RuntimeFault('M019101', 'compiled module does not export a cartridge factory', {
      start: 0,
      end: 0,
    });
  }
  return value.default as CartridgeFactory;
}

function pcmBytes(frame: ConsoleFrame): Uint8Array {
  const samples = frame.output.audio.left.length;
  const bytes = new Uint8Array(samples * 8);
  const view = new DataView(bytes.buffer);
  for (let index = 0; index < samples; index += 1) {
    view.setFloat32(index * 8, frame.output.audio.left[index] ?? 0, true);
    view.setFloat32(index * 8 + 4, frame.output.audio.right[index] ?? 0, true);
  }
  return bytes;
}

function canonicalBytes(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(canonicalValue(value)));
}

function canonicalValue(value: unknown): unknown {
  if (value instanceof Uint8Array || value instanceof Float32Array) return [...value];
  if (Array.isArray(value)) return value.map(canonicalValue);
  if (isRecord(value))
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, item]) => [key, canonicalValue(item)]),
    );
  return value;
}

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return (
    actual.length === keys.length && [...keys].sort().every((key, index) => actual[index] === key)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
