import { createHash } from 'node:crypto';

import { RuntimeFault } from './errors';
import { canonicalBytes, pcmBytes, saveHashes, sha256, stateBytes } from './execution-hashes';
import { decodeRuntimeBytes, loadRuntimeGame, type RuntimeGamePayload } from './game-loader';
import { HARDWARE } from './hardware';
import { emptyInputFrame } from './input';
import {
  isReplayTrace,
  replayInputFrames,
  type ReplayTrace,
  type ReplayTraceFrame,
} from './replay';

export type HeadlessTraceFrame = ReplayTraceFrame;
export type HeadlessTrace = ReplayTrace;

export interface HeadlessRequest {
  readonly revision: 2;
  readonly game: RuntimeGamePayload;
  readonly seed: number;
  readonly frames: number;
  readonly trace: HeadlessTrace;
  readonly parameter: string;
  readonly saves: Readonly<Record<string, string>>;
}

export interface HeadlessFrameResult {
  readonly frame: number;
  readonly cartridgeFrame: number;
  readonly cartridgeAlias: string;
  readonly cartridgeId: string;
  readonly workUnits: number;
  readonly framebufferSha256: string;
  readonly stateSha256: string;
  readonly snapshotSha256: string;
  readonly audioCommandSha256: string;
  readonly pcmSha256: string;
  readonly saveSha256: string;
}

export interface HeadlessResult {
  readonly revision: 2;
  readonly cartridge: { readonly id: string; readonly bytes: number; readonly sha256: string };
  readonly game: {
    readonly identity: string;
    readonly members: Readonly<
      Record<string, { readonly id: string; readonly bytes: number; readonly sha256: string }>
    >;
  };
  readonly configuration: {
    readonly seed: number;
    readonly requestedFrames: number;
    readonly updateRate: 30 | 60;
    readonly inputTraceSha256: string;
    readonly initialSaveSha256: string;
    readonly initialSaves: Readonly<Record<string, string>>;
    readonly parameter: string;
    readonly debug: boolean;
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
    readonly finalSnapshotSha256: string;
    readonly audioCommandsSha256: string;
    readonly pcmSha256: string;
    readonly finalSaveSha256: string;
    readonly finalSaves: Readonly<Record<string, string>>;
  };
  readonly fault?: {
    readonly frame: number;
    readonly cartridgeFrame: number;
    readonly cartridgeAlias: string;
    readonly cartridgeId: string;
    readonly code: string;
    readonly message: string;
    readonly sourceSpan: { readonly start: number; readonly end: number };
  };
}

/** Runs validated compiler output through the same production core used by native Studio/games. */
export async function runHeadless(value: unknown): Promise<HeadlessResult> {
  if (!isHeadlessRequest(value)) throw new TypeError('invalid MOD-01 headless request');
  const request = value;
  const { runtime, metadata } = await loadRuntimeGame(request.game, {
    seed: request.seed,
    parameter: request.parameter,
    saves: Object.fromEntries(
      Object.entries(request.saves).map(([id, image]) => [
        id,
        decodeRuntimeBytes(image, HARDWARE.saveCapacityBytes),
      ]),
    ),
  });
  const initial = runtime.snapshot();
  const members = Object.fromEntries(
    [...metadata].map(([alias, member]) => {
      const rom = decodeRuntimeBytes(member.rom, HARDWARE.cartridgeCapacityBytes);
      return [alias, { id: member.manifest.id, bytes: rom.length, sha256: sha256(rom) }];
    }),
  );
  const main = metadata.get('main'),
    entry = members.main;
  if (main === undefined || entry === undefined) throw new TypeError('missing admitted game entry');
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
        cartridgeFrame: result.cartridgeFrame,
        cartridgeAlias: result.cartridgeAlias,
        cartridgeId: result.cartridgeId,
        workUnits: result.workUnits,
        framebufferSha256: sha256(result.output.indexedPixels),
        stateSha256: sha256(stateBytes(snapshot)),
        snapshotSha256: sha256(canonicalBytes(snapshot)),
        audioCommandSha256: sha256(commands),
        pcmSha256: sha256(framePcm),
        saveSha256: sha256(canonicalBytes(snapshot.saves)),
      });
    } catch (error: unknown) {
      if (!(error instanceof RuntimeFault)) throw error;
      const status = runtime.inspect();
      fault = {
        frame,
        cartridgeFrame: status.cartridgeFrame,
        cartridgeAlias: status.cartridgeAlias,
        cartridgeId: status.cartridgeId,
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
    revision: 2,
    cartridge: entry,
    game: { identity: initial.identity, members },
    configuration: {
      seed: request.seed,
      requestedFrames: request.frames,
      updateRate: main.manifest.updateRate,
      inputTraceSha256: sha256(canonicalBytes(request.trace)),
      initialSaveSha256: sha256(canonicalBytes(initial.saves)),
      initialSaves: saveHashes(initial),
      parameter: request.parameter,
      debug: initial.debug,
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
      finalStateSha256: sha256(stateBytes(finalSnapshot)),
      finalSnapshotSha256: sha256(canonicalBytes(finalSnapshot)),
      audioCommandsSha256: commandHash.digest('hex'),
      pcmSha256: pcmHash.digest('hex'),
      finalSaveSha256: sha256(canonicalBytes(finalSnapshot.saves)),
      finalSaves: saveHashes(finalSnapshot),
    },
    ...(fault === undefined ? {} : { fault }),
  };
}

function isHeadlessRequest(value: unknown): value is HeadlessRequest {
  if (!isRecord(value) || !hasExactKeys(value, HEADLESS_KEYS) || value.revision !== 2) return false;
  if (
    !isRecord(value.game) ||
    typeof value.seed !== 'number' ||
    !Number.isSafeInteger(value.seed) ||
    value.seed < 0 ||
    value.seed > 0xffff_ffff ||
    typeof value.frames !== 'number' ||
    !Number.isSafeInteger(value.frames) ||
    value.frames < 0 ||
    value.frames > 36_000 ||
    !isReplayTrace(value.trace, value.frames) ||
    typeof value.parameter !== 'string' ||
    !isRecord(value.saves) ||
    Object.keys(value.saves).length > 32 ||
    Object.entries(value.saves).some(
      ([id, image]) =>
        !/^[a-z0-9][a-z0-9.-]{2,63}$/.test(id) ||
        typeof image !== 'string' ||
        image.length > Math.ceil(HARDWARE.saveCapacityBytes / 3) * 4,
    )
  )
    return false;
  return true;
}

const HEADLESS_KEYS = [
  'revision',
  'game',
  'seed',
  'frames',
  'trace',
  'parameter',
  'saves',
] as const;

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return (
    actual.length === keys.length && [...keys].sort().every((key, index) => actual[index] === key)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
