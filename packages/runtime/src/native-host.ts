import { createInterface } from 'node:readline';
import { deserialize, serialize } from 'node:v8';

import { createConsoleRuntime, type ConsoleRuntime } from './console-runtime';
import { HARDWARE } from './hardware';
import type { InputFrame } from './input';
import type { CartridgeFactory } from './machine';
import { RuntimeFault } from './errors';
import { renderMusicWav, renderSoundWav } from './wav';
import type { MusicAsset, SoundAsset } from './audio';
import { evaluateWatch } from './debugger';

type Command =
  | {
      type: 'load';
      javascript: string;
      manifest: {
        id: string;
        hardwareRevision?: 2;
        updateRate: 30 | 60;
        display: string | null;
        assets: Record<string, { kind: string; path: string }>;
      };
      files: Record<string, string>;
      rom: string;
      save: string;
      debug?: boolean;
      symbolNames?: Record<string, string>;
    }
  | { type: 'frame'; input: InputFrame }
  | { type: 'step'; input: InputFrame; watches?: string[]; conditions?: string[] }
  | { type: 'snapshot' }
  | { type: 'restore'; snapshot: string }
  | { type: 'memory'; address: number; length: number }
  | { type: 'memory-edit'; address: number; bytes: string }
  | {
      type: 'render-wav';
      kind: 'sound' | 'music';
      asset: SoundAsset | MusicAsset;
      sounds: SoundAsset[];
    }
  | { type: 'stop' };

let runtime: ConsoleRuntime | undefined;
let cartridgeId: string | undefined;
let symbolNames: Readonly<Record<string, string>> = {};
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

function namedValues(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
  return Object.fromEntries(
    Object.entries(value).map(([key, entry]) => [symbolNames[key] ?? key, entry]),
  );
}

function evaluateExpressions(
  expressions: readonly string[] | undefined,
  state: unknown,
  locals: unknown,
  booleanOnly = false,
): { value?: unknown; error?: string }[] {
  if (expressions === undefined) return [];
  if (expressions.length > 32) throw new RangeError('too many native debugger expressions');
  const environment = { ...namedValues(state), ...namedValues(locals) };
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

async function handle(command: Command): Promise<unknown> {
  switch (command.type) {
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
      if (typeof command.javascript !== 'string' || command.javascript.length > 2 * 1024 * 1024)
        throw new TypeError('invalid compiled cartridge');
      runtime = undefined;
      const module: unknown = await import(
        `data:text/javascript;base64,${Buffer.from(command.javascript).toString('base64')}`
      );
      if (typeof module !== 'object' || module === null || !('default' in module))
        throw new TypeError('compiled cartridge has no factory');
      const factory = module.default;
      if (typeof factory !== 'function')
        throw new TypeError('compiled cartridge factory is invalid');
      const files = Object.fromEntries(
        Object.entries(command.files).map(([path, value]) => [path, bytes(value)]),
      );
      const manifestAssets = command.manifest.assets as Record<
        string,
        {
          kind: 'sprite' | 'animation' | 'tile_set' | 'map' | 'font' | 'sound' | 'music';
          path: string;
        }
      >;
      runtime = createConsoleRuntime(factory as CartridgeFactory, {
        seed: 0,
        workUnitsPerFrame: HARDWARE.workUnitsPerFrame,
        updateRate: command.manifest.updateRate,
        hardwareRevision: command.manifest.hardwareRevision ?? 2,
        debug: command.debug === true,
        assets: { declarations: manifestAssets, files, displayPath: command.manifest.display },
        rom: bytes(command.rom),
        save: bytes(command.save),
      });
      cartridgeId = command.manifest.id;
      symbolNames = command.symbolNames ?? {};
      return { type: 'loaded', cartridgeId };
    }
    case 'frame': {
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      const result = runtime.runFrame(command.input);
      return {
        type: 'frame',
        frame: result.frame,
        workUnits: result.workUnits,
        pixels: encoded(result.output.indexedPixels),
        left: encoded(result.output.audio.left),
        right: encoded(result.output.audio.right),
        saveCommit: result.saveCommit === undefined ? null : encoded(result.saveCommit),
        debug: result.debug ?? null,
      };
    }
    case 'step': {
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      const result = runtime.stepDebug(command.input);
      if ('frame' in result) {
        return {
          type: 'step',
          frame: {
            frame: result.frame.frame,
            workUnits: result.frame.workUnits,
            pixels: encoded(result.frame.output.indexedPixels),
            left: encoded(result.frame.output.audio.left),
            right: encoded(result.frame.output.audio.right),
            saveCommit:
              result.frame.saveCommit === undefined ? null : encoded(result.frame.saveCommit),
          },
        };
      }
      if ('booted' in result) return { type: 'step', ...result };
      return {
        type: 'step',
        ...result,
        watches: evaluateExpressions(command.watches, result.inspection.state, result.event.locals),
        conditions: evaluateExpressions(
          command.conditions,
          result.inspection.state,
          result.event.locals,
          true,
        ),
      };
    }
    case 'snapshot':
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      return { type: 'snapshot', snapshot: serialize(runtime.snapshot()).toString('base64') };
    case 'restore':
      if (runtime === undefined) throw new Error('no cartridge is loaded');
      runtime.restore(deserialize(Buffer.from(command.snapshot, 'base64')));
      return { type: 'restored' };
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
      cartridgeId = undefined;
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
      });
    }
  });
});
