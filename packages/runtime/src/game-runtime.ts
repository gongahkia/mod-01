import { MEMORY } from './bus';
import {
  isConsoleRuntimeSnapshot,
  type ConsoleDebugPause,
  type ConsoleRuntimeSnapshot,
} from './console-runtime';
import { isCartridgeAlias, isTransferParameter } from './cartridge-control';
import { RuntimeFault } from './errors';
import { GameCatalog, type GameDefinition } from './game-definition';
import { HARDWARE } from './hardware';
import { InputBarrier, isInputBarrierSnapshot, type InputBarrierSnapshot } from './input-barrier';
import { isInputFrame, type InputFrame } from './input';
import type { ConsoleFrame } from './protocol';

export interface GameLocation {
  readonly cartridgeAlias: string;
  readonly cartridgeId: string;
}
export interface GameFrame extends ConsoleFrame, GameLocation {
  /** Console time restarts on load; the public frame belongs to the whole game. */
  readonly cartridgeFrame: number;
}
export type GameDebugResult =
  | (ConsoleDebugPause & GameLocation)
  | ({ readonly booted: true } & GameLocation)
  | { readonly frame: GameFrame };

export interface GameSnapshot {
  readonly revision: 1;
  readonly identity: string;
  readonly seed: number;
  readonly debug: boolean;
  readonly frame: number;
  readonly alias: string;
  readonly parameter: string;
  readonly bankAlias: string;
  readonly saves: Readonly<Record<string, Uint8Array>>;
  readonly inputBarrier: InputBarrierSnapshot;
  readonly console: ConsoleRuntimeSnapshot;
}

/** One pre-admitted game, one global input/replay timeline, no host I/O at a transition. */
export class GameRuntime {
  private readonly catalog: GameCatalog;
  private binding: ReturnType<GameCatalog['prepare']>;
  private alias = 'main';
  private parameter: string;
  private bankAlias = 'main';
  private frame = 0;
  private saves: Map<string, Uint8Array>;
  private barrier = new InputBarrier();
  private input: { readonly raw: InputFrame; readonly effective: InputFrame } | undefined;

  public constructor(definition: GameDefinition) {
    this.catalog = new GameCatalog(definition);
    this.parameter = this.catalog.parameter;
    this.saves = new Map([...this.catalog.initialSaves].map(([id, bytes]) => [id, bytes.slice()]));
    this.binding = this.catalog.prepare(
      this.alias,
      this.parameter,
      this.bankAlias,
      this.saved(this.alias),
      false,
    );
  }

  public runFrame(input: InputFrame): GameFrame {
    this.validateInput(input);
    this.assertExecution(false);
    const effective = this.frameInput(input);
    return this.complete(this.binding.runtime.runFrame(effective));
  }

  public stepDebug(input: InputFrame): GameDebugResult {
    this.validateInput(input);
    if (!this.catalog.debug)
      throw new RuntimeFault('M019104', 'statement stepping requires a debug cartridge', {
        start: 0,
        end: 0,
      });
    this.assertExecution(true);
    // Stepping start is not a logical input frame and cannot release a quarantine.
    const effective = this.binding.runtime.inspectExecution().booted
      ? this.frameInput(input)
      : input;
    const result = this.binding.runtime.stepDebug(effective);
    if ('frame' in result) return { frame: this.complete(result.frame) };
    return { ...result, ...this.location() };
  }

  public inspect() {
    const execution = this.binding.runtime.inspectExecution();
    return {
      ...execution,
      ...this.location(),
      frame: this.frame,
      cartridgeFrame: execution.frame,
      parameter: this.parameter,
      bankAlias: this.bankAlias,
      pending: this.binding.control.pendingRequest(),
    };
  }

  public snapshot(): GameSnapshot {
    return {
      revision: 1,
      identity: this.catalog.identity,
      seed: this.catalog.seed,
      debug: this.catalog.debug,
      frame: this.frame,
      alias: this.alias,
      parameter: this.parameter,
      bankAlias: this.bankAlias,
      saves: Object.fromEntries([...this.saves].map(([id, bytes]) => [id, bytes.slice()])),
      inputBarrier: this.barrier.snapshot(),
      console: this.binding.runtime.snapshot(),
    };
  }

  public restore(value: unknown): void {
    try {
      if (
        !record(value) ||
        !exact(value, [
          'revision',
          'identity',
          'seed',
          'debug',
          'frame',
          'alias',
          'parameter',
          'bankAlias',
          'saves',
          'inputBarrier',
          'console',
        ]) ||
        value.revision !== 1 ||
        value.identity !== this.catalog.identity ||
        value.seed !== this.catalog.seed ||
        value.debug !== this.catalog.debug ||
        typeof value.frame !== 'number' ||
        !Number.isSafeInteger(value.frame) ||
        value.frame < 0 ||
        !isCartridgeAlias(value.alias) ||
        !isTransferParameter(value.parameter) ||
        !isCartridgeAlias(value.bankAlias) ||
        !this.catalog.compatible(value.alias, value.bankAlias) ||
        !isConsoleRuntimeSnapshot(value.console) ||
        value.console.machine.frame > value.frame ||
        value.console.cartridgeControl === null ||
        value.console.cartridgeControl.alias !== value.alias ||
        value.console.cartridgeControl.parameter !== value.parameter ||
        !isInputBarrierSnapshot(value.inputBarrier) ||
        !record(value.saves) ||
        !exact(value.saves, this.catalog.ids())
      )
        throw new TypeError('invalid game checkpoint context');
      const saves = new Map<string, Uint8Array>();
      for (const [id, bytes] of Object.entries(value.saves)) {
        if (!(bytes instanceof Uint8Array) || bytes.length !== HARDWARE.saveCapacityBytes)
          throw new TypeError('invalid committed game save image');
        saves.set(id, bytes.slice());
      }
      // Construct and restore a candidate without running start. No live VM/device changes
      // until every source, memory, input-owner and save-context check succeeds.
      const binding = this.catalog.prepare(
        value.alias,
        value.parameter,
        value.bankAlias,
        value.console.save.committed,
        true,
      );
      binding.runtime.restore(value.console);
      const barrier = new InputBarrier();
      barrier.restore(value.inputBarrier);
      this.binding = binding;
      this.barrier = barrier;
      this.saves = saves;
      this.frame = value.frame;
      this.alias = value.alias;
      this.parameter = value.parameter;
      this.bankAlias = value.bankAlias;
      this.input = undefined;
    } catch (error) {
      throw new RuntimeFault(
        'M019103',
        error instanceof Error ? error.message : 'invalid game checkpoint',
        { start: 0, end: 0 },
      );
    }
  }

  public inspectMemory(address: number, length: number) {
    return this.binding.runtime.inspectMemory(address, length);
  }
  public editMemory(address: number, bytes: Uint8Array): void {
    this.binding.runtime.editMemory(address, bytes);
  }
  public memoryRegions() {
    return this.binding.runtime.memoryRegions();
  }
  public inspectMenu() {
    return this.binding.runtime.inspectMenu();
  }

  private location(): GameLocation {
    return { cartridgeAlias: this.alias, cartridgeId: this.catalog.member(this.alias).id };
  }
  private saved(alias: string): Uint8Array {
    const bytes = this.saves.get(this.catalog.member(alias).id);
    if (bytes === undefined) throw new TypeError('missing admitted save identity');
    return bytes;
  }
  private validateInput(input: unknown): asserts input is InputFrame {
    if (!isInputFrame(input))
      throw new RuntimeFault('M019008', 'invalid controller input frame', { start: 0, end: 0 });
    if (this.frame === Number.MAX_SAFE_INTEGER)
      throw new RuntimeFault('M019012', 'game display-frame counter is exhausted', {
        start: 0,
        end: 0,
      });
  }
  private frameInput(raw: InputFrame): InputFrame {
    this.input ??= { raw: structuredClone(raw), effective: this.barrier.apply(raw) };
    return this.input.effective;
  }
  private assertExecution(allowSuspended: boolean): void {
    const status = this.binding.runtime.inspectExecution();
    if (status.fault !== null)
      throw new RuntimeFault(
        'M019014',
        'cartridge faulted; restart or restore a healthy checkpoint',
        status.fault.sourceSpan,
      );
    if (!allowSuspended && status.phase !== 'idle')
      throw new RuntimeFault('M019014', 'cartridge is already executing', { start: 0, end: 0 });
  }

  private complete(output: ConsoleFrame): GameFrame {
    const frame = {
      ...output,
      ...this.location(),
      cartridgeFrame: output.frame,
      frame: this.frame,
    };
    const request = this.binding.control.pendingRequest();
    if (output.saveCommit !== undefined)
      this.saves.set(frame.cartridgeId, output.saveCommit.slice());
    if (request?.kind === 'load') {
      const next = this.catalog.prepare(
        request.alias,
        request.parameter,
        request.alias,
        this.saved(request.alias),
        true,
      );
      if (this.input === undefined) throw new TypeError('missing completed game input');
      this.barrier.quarantine(this.input.raw);
      this.binding = next;
      this.alias = request.alias;
      this.parameter = request.parameter;
      this.bankAlias = request.alias;
    } else if (request?.kind === 'reload') {
      const before = this.binding.runtime.snapshot();
      const next = this.catalog.prepare(
        this.alias,
        this.parameter,
        request.alias,
        this.saved(this.alias),
        true,
      );
      const fresh = next.runtime.snapshot();
      const retained = new Set<number>([
        MEMORY.ram,
        MEMORY.front,
        MEMORY.back,
        MEMORY.display,
        MEMORY.draw,
        MEMORY.transparency,
        MEMORY.pattern,
        MEMORY.rasterLive,
        MEMORY.raster,
      ]);
      const oldRegions = new Map(before.memory.regions.map((region) => [region.address, region]));
      next.runtime.restore({
        ...before,
        audio: fresh.audio,
        cartridgeControl: { ...before.cartridgeControl, pending: null },
        memory: {
          ...fresh.memory,
          regions: fresh.memory.regions.map((region) =>
            retained.has(region.address) ? (oldRegions.get(region.address) ?? region) : region,
          ),
        },
      });
      this.binding = next;
      this.bankAlias = request.alias;
    }
    this.frame++;
    this.input = undefined;
    return frame;
  }
}

export function createGameRuntime(definition: GameDefinition): GameRuntime {
  return new GameRuntime(definition);
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function exact(value: object, keys: readonly string[]): boolean {
  return (
    Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key))
  );
}
