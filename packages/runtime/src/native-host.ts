import { createInterface } from 'node:readline';
import { deserialize, serialize } from 'node:v8';

import {
  decodeRuntimeBytes,
  loadRuntimeGame,
  type RuntimeGameMember,
  type RuntimeGamePayload,
} from './game-loader';
import type { GameFrame, GameRuntime } from './game-runtime';
import { DEFAULT_SEED, HARDWARE } from './hardware';
import type { InputFrame } from './input';
import { RuntimeFault } from './errors';
import { renderMusicWav, renderSoundWav } from './wav';
import {
  AudioAssetStore,
  Synthesizer,
  type AudioAsset,
  type MusicAsset,
  type SoundAsset,
} from './audio';
import { evaluateWatch } from './debugger';
import { MEMORY } from './bus';

type Command =
  | {
      type: 'load';
      game: RuntimeGamePayload;
      parameter: string;
      saves: Record<string, string>;
      seed?: number;
    }
  | { type: 'frame'; input: InputFrame }
  | { type: 'scratch'; input: InputFrame }
  | { type: 'step'; input: InputFrame; watches?: string[]; conditions?: string[] }
  | { type: 'snapshot' }
  | { type: 'restore'; snapshot: string }
  | { type: 'memory'; address: number; length: number }
  | { type: 'memory-edit'; address: number; bytes: string }
  | { type: 'audio-preview-load'; assets: AudioAsset[] }
  | { type: 'audio-preview-play'; music: string; orderIndex: number; row: number }
  | { type: 'audio-preview-audition'; sound: string; note: number; volume?: number }
  | { type: 'audio-preview-frame' }
  | {
      type: 'render-wav';
      kind: 'sound' | 'music';
      asset: SoundAsset | MusicAsset;
      sounds: SoundAsset[];
    }
  | { type: 'stop' };

let runtime: GameRuntime | undefined;
let metadata: ReadonlyMap<string, RuntimeGameMember> = new Map();
let audioPreview: Synthesizer | undefined;
let scratchFresh = false;
const input = createInterface({ input: process.stdin, crlfDelay: Infinity });

function bytes(encoded: string): Uint8Array {
  if (typeof encoded !== 'string' || encoded.length > 24 * 1024 * 1024)
    throw new TypeError('invalid native host byte payload');
  const decoded = Uint8Array.from(Buffer.from(encoded, 'base64'));
  if (decoded.length > 16 * 1024 * 1024)
    throw new RangeError('native host byte payload exceeds 16 MiB');
  return decoded;
}

function encoded(value: Uint8Array | Float32Array): string {
  return Buffer.from(value.buffer, value.byteOffset, value.byteLength).toString('base64');
}

function respond(value: unknown): void {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function namedValues(value: unknown, alias: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
  const symbolNames = metadata.get(alias)?.symbolNames ?? {};
  return Object.fromEntries(
    Object.entries(value).map(([key, entry]) => [symbolNames[key] ?? key, entry]),
  );
}

function evaluateExpressions(
  expressions: readonly string[] | undefined,
  state: unknown,
  locals: unknown,
  alias: string,
  booleanOnly = false,
): { value?: unknown; error?: string }[] {
  if (expressions === undefined) return [];
  if (expressions.length > 32) throw new RangeError('too many native debugger expressions');
  const environment = { ...namedValues(state, alias), ...namedValues(locals, alias) };
  return expressions.map((expression) => {
    if (expression.length > 256) return { error: 'expression exceeds 256 characters' };
    if (expression.length === 0) return { value: true };
    try {
      const value = evaluateWatch(expression, environment);
      return booleanOnly && typeof value !== 'boolean'
        ? { error: 'breakpoint condition must produce Bool' }
        : { value };
    } catch (error) {
      return { error: error instanceof Error ? error.message : 'watch evaluation failed' };
    }
  });
}

function frameResponse(result: GameFrame) {
  return {
    frame: result.frame,
    cartridgeAlias: result.cartridgeAlias,
    cartridgeId: result.cartridgeId,
    cartridgeFrame: result.cartridgeFrame,
    workUnits: result.workUnits,
    pixels: encoded(result.output.indexedPixels),
    left: encoded(result.output.audio.left),
    right: encoded(result.output.audio.right),
    saveCommit: result.saveCommit === undefined ? null : encoded(result.saveCommit),
    debug: result.debug ?? null,
    menuItems: runtime?.inspectMenu(),
    active: runtime?.inspect(),
  };
}

async function handle(command: Command): Promise<unknown> {
  switch (command.type) {
    case 'audio-preview-load': {
      runtime = undefined;
      audioPreview = undefined;
      scratchFresh = false;
      metadata = new Map();
      if (
        !Array.isArray(command.assets) ||
        command.assets.length > 4096 ||
        Buffer.byteLength(JSON.stringify(command.assets), 'utf8') > 16 * 1024 * 1024
      )
        throw new RangeError('audio preview requires at most 4096 assets and 16 MiB');
      audioPreview = new Synthesizer(new AudioAssetStore(command.assets));
      return { type: 'audio-preview-loaded' };
    }
    case 'audio-preview-play':
      if (audioPreview === undefined) throw new Error('no audio preview is loaded');
      audioPreview.playMusic(command.music, command.orderIndex, command.row);
      return { type: 'audio-preview-playing', position: audioPreview.inspectTracker() };
    case 'audio-preview-audition':
      if (audioPreview === undefined) throw new Error('no audio preview is loaded');
      audioPreview.audition(command.sound, command.note, command.volume);
      return { type: 'audio-preview-auditioned' };
    case 'audio-preview-frame': {
      if (audioPreview === undefined) throw new Error('no audio preview is loaded');
      const frame = audioPreview.snapshot().frame;
      const position = audioPreview.inspectTracker();
      const result = audioPreview.finishFrame();
      return {
        type: 'audio-preview-frame',
        frame,
        position,
        nextPosition: result.tracker,
        left: encoded(result.left),
        right: encoded(result.right),
        activeVoices: result.activeVoices,
        voices: audioPreview.inspectVoices(),
        peakLeft: result.left.reduce((peak, sample) => Math.max(peak, Math.abs(sample)), 0),
        peakRight: result.right.reduce((peak, sample) => Math.max(peak, Math.abs(sample)), 0),
      };
    }
    case 'render-wav': {
      const wav =
        command.kind === 'sound'
          ? renderSoundWav(command.asset as SoundAsset)
          : renderMusicWav(command.asset as MusicAsset, command.sounds);
      if (wav.byteLength > 8 * 1024 * 1024)
        throw new RangeError('native WAV preview exceeds 8 MiB');
      return { type: 'wav', bytes: encoded(wav) };
    }
    case 'load': {
      const saves: unknown = command.saves;
      runtime = undefined;
      metadata = new Map();
      audioPreview = undefined;
      scratchFresh = false;
      if (
        Object.keys(command).some(
          (key) => !['type', 'game', 'parameter', 'saves', 'seed'].includes(key),
        ) ||
        typeof command.parameter !== 'string' ||
        typeof saves !== 'object' ||
        saves === null ||
        Array.isArray(saves) ||
        Object.keys(saves).length > 32
      )
        throw new TypeError('invalid current native game load command');
      const loaded = await loadRuntimeGame(command.game, {
        seed: command.seed === undefined ? DEFAULT_SEED : command.seed,
        parameter: command.parameter,
        saves: Object.fromEntries(
          Object.entries(command.saves).map(([id, value]) => [
            id,
            decodeRuntimeBytes(value, HARDWARE.saveCapacityBytes),
          ]),
        ),
      });
      runtime = loaded.runtime;
      metadata = loaded.metadata;
      scratchFresh = command.game.debug;
      return {
        type: 'loaded',
        identity: command.game.identity,
        ...runtime.inspect(),
        active: runtime.inspect(),
        menuItems: runtime.inspectMenu(),
      };
    }
    case 'frame': {
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      scratchFresh = false;
      const result = runtime.runFrame(command.input);
      return { type: 'frame', ...frameResponse(result) };
    }
    case 'scratch': {
      if (runtime === undefined) throw new Error('no scratchpad program is loaded');
      if (!scratchFresh) throw new TypeError('scratchpad requires a freshly loaded debug program');
      scratchFresh = false;
      let locals: Record<string, unknown> = {};
      let state: Record<string, unknown> = {};
      let booted = false;
      for (let step = 0; step < 65_536; step++) {
        const result = runtime.stepDebug(command.input);
        if ('event' in result) {
          locals = namedValues(result.event.locals, result.cartridgeAlias);
          state = namedValues(result.inspection.state, result.cartridgeAlias);
        } else if ('booted' in result) {
          booted = true;
          break;
        } else throw new TypeError('scratchpad must start from a fresh debug program');
      }
      if (!booted)
        throw new RuntimeFault('M019105', 'scratchpad exceeds 65,536 statement boundaries', {
          start: 0,
          end: 0,
        });
      const work = runtime.inspectMemory(MEMORY.system + 0x20, 8).bytes;
      const bootWorkUnits = Number(
        new DataView(work.buffer, work.byteOffset, 8).getBigUint64(0, true),
      );
      const frame = runtime.runFrame(command.input);
      return {
        type: 'scratch',
        locals,
        state:
          frame.debug === undefined
            ? state
            : namedValues(frame.debug.inspection.state, frame.cartridgeAlias),
        bootWorkUnits,
        workUnits: frame.workUnits,
        pixels: encoded(frame.output.indexedPixels),
        left: encoded(frame.output.audio.left),
        right: encoded(frame.output.audio.right),
      };
    }
    case 'step': {
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      scratchFresh = false;
      const result = runtime.stepDebug(command.input);
      if ('frame' in result) {
        return {
          type: 'step',
          frame: frameResponse(result.frame),
        };
      }
      if ('booted' in result)
        return {
          type: 'step',
          ...result,
          active: runtime.inspect(),
          menuItems: runtime.inspectMenu(),
        };
      return {
        type: 'step',
        ...result,
        active: runtime.inspect(),
        menuItems: runtime.inspectMenu(),
        watches: evaluateExpressions(
          command.watches,
          result.inspection.state,
          result.event.locals,
          result.cartridgeAlias,
        ),
        conditions: evaluateExpressions(
          command.conditions,
          result.inspection.state,
          result.event.locals,
          result.cartridgeAlias,
          true,
        ),
      };
    }
    case 'snapshot':
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      {
        const snapshot = runtime.snapshot();
        return {
          type: 'snapshot',
          snapshot: serialize(snapshot).toString('base64'),
          frame: snapshot.frame,
          booted: snapshot.console.machine.execution.booted,
          active: runtime.inspect(),
        };
      }
    case 'restore':
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      scratchFresh = false;
      runtime.restore(deserialize(Buffer.from(command.snapshot, 'base64')));
      {
        const snapshot = runtime.snapshot();
        return {
          type: 'restored',
          frame: snapshot.frame,
          booted: snapshot.console.machine.execution.booted,
          active: runtime.inspect(),
          menuItems: runtime.inspectMenu(),
        };
      }
    case 'memory': {
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      const result = runtime.inspectMemory(command.address, command.length);
      return {
        type: 'memory',
        address: result.address,
        bytes: encoded(result.bytes),
        regions: result.regions,
      };
    }
    case 'memory-edit':
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      runtime.editMemory(command.address, bytes(command.bytes));
      return { type: 'memory-edited' };
    case 'stop':
      runtime = undefined;
      audioPreview = undefined;
      scratchFresh = false;
      metadata = new Map();
      return { type: 'stopped' };
  }
}

let chain = Promise.resolve();
input.on('line', (line) => {
  chain = chain.then(async () => {
    try {
      const command = JSON.parse(line) as Command;
      respond(await handle(command));
    } catch (error: unknown) {
      respond({
        type: 'error',
        code: error instanceof RuntimeFault ? error.code : 'M019100',
        message: error instanceof Error ? error.message : 'native host failure',
        ...(runtime === undefined ? {} : { active: runtime.inspect() }),
        ...(error instanceof RuntimeFault ? { sourceSpan: error.sourceSpan } : {}),
      });
    }
  });
});
