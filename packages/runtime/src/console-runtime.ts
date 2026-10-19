import { RuntimeFault } from './errors';
import {
  DeterministicMachine,
  isMachineSnapshot,
  type CartridgeFactory,
  type ExecutionContext,
  type MachineSnapshot,
} from './machine';
import { decodeRuntimeAssets } from './asset-codec';
import { AudioAssetStore, Synthesizer, isSynthSnapshot, type SynthSnapshot } from './audio';
import {
  IndexedGraphics,
  VisualAssetStore,
  isGraphicsSnapshot,
  type GraphicsSnapshot,
} from './graphics';
import { HARDWARE, MASTER_PALETTE_RGBA } from './hardware';
import { MEMORY, MemoryBus, isMemorySnapshot, type MemorySnapshot } from './bus';
import type { MemoryRegionDescriptor } from './bus';
import { MapQueryStore } from './map-query';
import {
  SaveMemory,
  isSaveSnapshot,
  isPendingDeviceWrites,
  type SaveSnapshot,
  type SaveWrite,
} from './save';
import { isInputFrame, type InputFrame } from './input';
import type { MenuItem } from './menu';
import { isConsoleConfiguration } from './protocol';
import type { ExecutionSnapshot } from './system';
import {
  isCartridgeControlSnapshot,
  type CartridgeControl,
  type CartridgeControlSnapshot,
} from './cartridge-control';
import type {
  ConsoleCommand,
  DebugStackFrame,
  DebugTraceEvent,
  ConsoleConfiguration,
  SourceSpan,
  ConsoleFrame,
} from './protocol';

export type { ConsoleFrame } from './protocol';

export interface ConsoleDebugPause {
  readonly event: DebugTraceEvent;
  readonly inspection: {
    readonly state: unknown;
    readonly tasks: unknown;
    readonly callStack: unknown;
  };
}

export type ConsoleDebugResult =
  ConsoleDebugPause | { readonly booted: true } | { readonly frame: ConsoleFrame };

export interface ConsoleRuntime {
  runFrame(input: InputFrame): ConsoleFrame;
  stepDebug(input: InputFrame): ConsoleDebugResult;
  snapshot(): ConsoleRuntimeSnapshot;
  restore(snapshot: unknown): void;
  inspectMemory(
    address: number,
    length: number,
  ): {
    readonly address: number;
    readonly bytes: Uint8Array;
    readonly regions: readonly MemoryRegionDescriptor[];
  };
  editMemory(address: number, bytes: Uint8Array): void;
  memoryRegions(): readonly MemoryRegionDescriptor[];
  inspectMenu(): readonly MenuItem[];
  inspectExecution(): ExecutionSnapshot & { readonly frame: number };
}

export interface ConsoleRuntimeSnapshot {
  readonly revision: 7;
  readonly machine: MachineSnapshot;
  readonly save: SaveSnapshot;
  readonly graphics: GraphicsSnapshot;
  readonly audio: SynthSnapshot;
  readonly pendingSaveWrites: readonly SaveWrite[];
  readonly memory: MemorySnapshot;
  readonly cartridgeControl: CartridgeControlSnapshot | null;
}

/** Trusted scheduler services, never data supplied by cartridge code or JSON configuration. */
export interface ConsoleRuntimeServices {
  readonly cartridge?: CartridgeControl;
  readonly deferBoot?: boolean;
}

export function isConsoleRuntimeSnapshot(value: unknown): value is ConsoleRuntimeSnapshot {
  return (
    isRecord(value) &&
    value.revision === 7 &&
    Object.keys(value).length === 8 &&
    isMachineSnapshot(value.machine) &&
    isSaveSnapshot(value.save) &&
    isGraphicsSnapshot(value.graphics) &&
    isSynthSnapshot(value.audio) &&
    isPendingDeviceWrites(value.pendingSaveWrites, value.save) &&
    isMemorySnapshot(value.memory) &&
    graphicsMatchesMemory(value.graphics, value.memory) &&
    (value.cartridgeControl === null || isCartridgeControlSnapshot(value.cartridgeControl))
  );
}

/** One production dispatcher for native Studio, standalone players and headless hosts. */
export function createConsoleRuntime(
  factory: CartridgeFactory,
  configuration: ConsoleConfiguration,
  services: ConsoleRuntimeServices = {},
): ConsoleRuntime {
  if (!isConsoleConfiguration(configuration))
    throw new RuntimeFault('M019100', 'invalid console configuration', { start: 0, end: 0 });
  const cartridgeControl = services.cartridge;
  const deferBoot = services.deferBoot === true;
  const source = configuration.assets;
  const assets =
    source === undefined
      ? decodeRuntimeAssets({}, {})
      : decodeRuntimeAssets(source.declarations, source.files, source.displayPath);
  const visualStore = new VisualAssetStore(assets.visual, assets.display);
  const graphics = new IndexedGraphics(visualStore);
  const audioStore = new AudioAssetStore(assets.audio);
  const synthesizer = new Synthesizer(audioStore);
  let rendering = false;
  let machine: DeterministicMachine | undefined = undefined;
  let drawCommands: ConsoleCommand[] = [];
  let audioCommands: ConsoleCommand[] = [];
  let frameOutput: ConsoleFrame['output'] | undefined;
  let debugFrameActive = false;
  const mapQueries = new MapQueryStore(source === undefined ? (configuration.maps ?? []) : []);
  const saveMemory = new SaveMemory(configuration.save ?? {});
  const cartridgeRom = configuration.rom?.slice() ?? new Uint8Array();
  const cartridgeInfo = createCartridgeInfo(cartridgeRom, configuration.hardwareRevision ?? 2);
  const debugEnabled = configuration.debug ?? false;
  let debugTrace: DebugTraceEvent[] = [];
  const debugCallStack: DebugStackFrame[] = [];
  let debugTraceTruncated = false;
  const ram = new Uint8Array(MEMORY.ramBytes);
  const bus = new MemoryBus(
    [
      { name: 'ram', address: MEMORY.ram, bytes: ram, writable: true },
      ...graphics.memoryRegions(),
      ...visualStore.memoryRegions(),
      ...synthesizer.memoryRegions(),
      ...saveMemory.memoryRegions((units, span) => {
        requireMachine().work(units, span);
      }),
      {
        name: 'cartridge status',
        address: MEMORY.cartridgeInfo,
        bytes: cartridgeInfo,
        writable: false,
      },
      ...(cartridgeRom.length === 0
        ? []
        : [
            {
              name: 'canonical cartridge ROM',
              address: MEMORY.cartridgeRom,
              bytes: cartridgeRom,
              writable: false,
            } as const,
          ]),
      {
        name: 'controllers and pointer',
        address: MEMORY.input,
        length: MEMORY.inputBytes,
        writable: false,
        readByte: (offset) => requireMachine().readInputByte(offset),
      },
      {
        name: 'scheduler, RNG, work and fault status',
        address: MEMORY.system,
        length: MEMORY.systemBytes,
        writable: false,
        readByte: (offset) => requireMachine().readSystemByte(offset),
      },
      {
        name: 'master palette RGBA',
        address: MEMORY.palette,
        bytes: Uint8Array.from(MASTER_PALETTE_RGBA),
        writable: false,
      },
    ],
    (units, span) => {
      requireMachine().work(units, span);
    },
  );

  machine = new DeterministicMachine(factory, configuration, {
    completeFrame: () => {
      frameOutput = {
        indexedPixels: graphics.finishFrame().indexedPixels,
        audio: synthesizer.finishFrame(),
        audioState: synthesizer.snapshot(),
      };
    },
    call: handleConsoleCall,
    ...(debugEnabled
      ? {
          probe: (id: number, sourceSpan: SourceSpan, locals: unknown) => {
            if (debugTrace.length >= HARDWARE.drawCommandsPerFrame) {
              debugTraceTruncated = true;
              return;
            }
            debugTrace.push({
              id,
              sourceSpan,
              locals: structuredClone(locals),
              callStack: structuredClone(debugCallStack),
            });
          },
          enter: (name: string, sourceSpan: SourceSpan) => {
            debugCallStack.push({ name, sourceSpan });
          },
          leave: () => {
            debugCallStack.pop();
          },
        }
      : {}),
  });
  graphics.beginFrame();
  rendering = true;
  try {
    if (!debugEnabled && !deferBoot) machine.boot();
  } finally {
    rendering = false;
  }
  graphics.finishFrame();
  let boundarySnapshot = captureSnapshot();

  return {
    runFrame(input) {
      if (!isInputFrame(input))
        throw new RuntimeFault('M019008', 'invalid controller input frame', { start: 0, end: 0 });
      const active = requireMachine();
      active.assertRunnable();
      drawCommands = [];
      audioCommands = [];
      frameOutput = undefined;
      debugTrace = [];
      debugTraceTruncated = false;
      graphics.beginFrame();
      rendering = true;
      let report;
      try {
        report = active.runFrame(input);
      } finally {
        rendering = false;
      }
      const output = completedOutput();
      const saveWrites = saveMemory.takeWrites();
      const saveCommit = saveMemory.takeCommit();
      boundarySnapshot = captureSnapshot();
      return {
        ...report,
        drawCommands,
        audioCommands,
        saveWrites,
        ...(saveCommit === undefined ? {} : { saveCommit }),
        output,
        ...(debugEnabled
          ? {
              debug: {
                trace: debugTrace,
                truncated: debugTraceTruncated,
                inspection: structuredClone(active.inspect()),
              },
            }
          : {}),
      };
    },
    stepDebug(input) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'statement stepping requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!isInputFrame(input))
        throw new RuntimeFault('M019008', 'invalid controller input frame', { start: 0, end: 0 });
      if (!debugFrameActive) {
        drawCommands = [];
        audioCommands = [];
        frameOutput = undefined;
        debugTrace = [];
        debugTraceTruncated = false;
        graphics.beginFrame();
        debugFrameActive = true;
      }
      rendering = true;
      let step;
      try {
        step = requireMachine().stepDebug(input);
      } finally {
        rendering = false;
      }
      if (step.event !== undefined) {
        const inspection = structuredClone(requireMachine().inspect());
        const event: DebugTraceEvent = {
          ...step.event,
          callStack: structuredClone(debugCallStack),
        };
        return { event, inspection };
      }
      if (step.booted === true) {
        debugFrameActive = false;
        graphics.finishFrame();
        boundarySnapshot = captureSnapshot();
        return { booted: true };
      }
      const report = step.report;
      if (report === undefined) throw new TypeError('debug step produced no event or frame');
      debugFrameActive = false;
      const output = completedOutput();
      const saveWrites = saveMemory.takeWrites();
      const saveCommit = saveMemory.takeCommit();
      const inspection = structuredClone(requireMachine().inspect());
      boundarySnapshot = captureSnapshot();
      return {
        frame: {
          ...report,
          drawCommands,
          audioCommands,
          saveWrites,
          ...(saveCommit === undefined ? {} : { saveCommit }),
          output,
          debug: {
            trace: debugTrace,
            truncated: debugTraceTruncated,
            inspection,
          },
        },
      };
    },
    snapshot: captureSnapshot,
    restore(value) {
      if (!isConsoleRuntimeSnapshot(value))
        throw new RuntimeFault('M019103', 'invalid console snapshot', { start: 0, end: 0 });
      const snapshot = value;
      if ((cartridgeControl === undefined) !== (snapshot.cartridgeControl === null))
        throw new RuntimeFault(
          'M019103',
          'snapshot cartridge environment does not match this console',
          { start: 0, end: 0 },
        );
      const before = debugFrameActive ? boundarySnapshot : captureSnapshot();
      try {
        if (snapshot.cartridgeControl !== null)
          cartridgeControl?.restore(snapshot.cartridgeControl);
        requireMachine().restore(snapshot.machine);
        debugFrameActive = false;
        saveMemory.restoreDevice(snapshot.save, snapshot.pendingSaveWrites);
        graphics.restore(snapshot.graphics);
        synthesizer.restore(snapshot.audio);
        bus.restore(snapshot.memory);
        boundarySnapshot = captureSnapshot();
      } catch (error) {
        if (before.cartridgeControl !== null) cartridgeControl?.restore(before.cartridgeControl);
        requireMachine().restore(before.machine);
        saveMemory.restoreDevice(before.save, before.pendingSaveWrites);
        graphics.restore(before.graphics);
        synthesizer.restore(before.audio);
        bus.restore(before.memory);
        throw new RuntimeFault(
          'M019103',
          error instanceof Error ? error.message : 'invalid device snapshot',
          { start: 0, end: 0 },
        );
      }
    },
    inspectMemory(address, length) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'memory inspection requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!Number.isSafeInteger(length) || length < 1 || length > 256)
        throw new RuntimeFault('M019020', 'debug memory reads require 1-256 bytes', {
          start: 0,
          end: 0,
        });
      return { address, bytes: bus.inspect(address, length), regions: bus.describe() };
    },
    editMemory(address, bytes) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'memory editing requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!(bytes instanceof Uint8Array) || bytes.length < 1 || bytes.length > 256)
        throw new RuntimeFault('M019020', 'debug memory edits require 1-256 bytes', {
          start: 0,
          end: 0,
        });
      bus.edit(address, bytes);
    },
    memoryRegions() {
      return bus.describe();
    },
    inspectMenu() {
      return requireMachine().inspectMenu();
    },
    inspectExecution() {
      const active = requireMachine();
      return { frame: active.frame, ...active.inspectExecution() };
    },
  };

  function captureSnapshot(): ConsoleRuntimeSnapshot {
    return {
      revision: 7,
      machine: requireMachine().snapshot(),
      save: saveMemory.deviceSnapshot(),
      graphics: graphics.snapshot(),
      audio: synthesizer.snapshot(),
      pendingSaveWrites: saveMemory.pendingWrites(),
      memory: bus.snapshot(),
      cartridgeControl: cartridgeControl?.snapshot() ?? null,
    };
  }

  function completedOutput(): ConsoleFrame['output'] {
    if (frameOutput === undefined)
      throw new RuntimeFault('M019102', 'frame completed without device output', {
        start: 0,
        end: 0,
      });
    return frameOutput;
  }

  function handleConsoleCall(
    name: string,
    arguments_: readonly unknown[],
    sourceSpan: SourceSpan,
    context: ExecutionContext,
  ): unknown {
    if (name === 'load_cart' || name === 'cart_parameter' || name === 'reload_assets') {
      if (cartridgeControl === undefined)
        throw new RuntimeFault(
          'M019017',
          'cartridge changes require an admitted game scheduler',
          sourceSpan,
        );
      return cartridgeControl.call(name, arguments_, sourceSpan, context, (units, span) => {
        requireMachine().work(units, span);
      });
    }
    if (name === 'visual_id' || name === 'audio_id') {
      if (arguments_.length !== 1 || typeof arguments_[0] !== 'string')
        throw new RuntimeFault('M019009', `${name} expects one Text name`, sourceSpan);
      requireMachine().work(1 + arguments_[0].length, sourceSpan);
      return (name === 'visual_id' ? visualStore : audioStore).id(arguments_[0]);
    }
    if (MEMORY_CALLS.has(name)) {
      if (arguments_.length !== MEMORY_CALLS.get(name))
        throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
      const values = arguments_.map((value) => readInteger(value, sourceSpan));
      const address = values[0] ?? 0;
      const second = values[1] ?? 0;
      const third = values[2] ?? 0;
      const raster = context.phase === 'raster';
      switch (name) {
        case 'mem_read':
          return bus.read(address, 1, sourceSpan);
        case 'mem_read16':
          return bus.read(address, 2, sourceSpan);
        case 'mem_write': {
          bus.write(address, second, 1, sourceSpan, raster);
          return;
        }
        case 'mem_write16': {
          bus.write(address, second, 2, sourceSpan, raster);
          return;
        }
        case 'mem_copy': {
          bus.copy(address, second, third, sourceSpan, raster);
          return;
        }
        case 'mem_fill': {
          bus.fill(address, second, third, sourceSpan, raster);
          return;
        }
      }
    }
    requireMachine().work(consoleWorkCost(name, arguments_), sourceSpan);
    if (context.phase === 'raster' && name !== 'pal' && name !== 'raster_scroll') {
      throw new RuntimeFault(
        'M019011',
        `console API call '${name}' is not valid in the raster callback`,
        sourceSpan,
      );
    }
    if (name === 'raster_scroll' && context.phase !== 'raster') {
      throw new RuntimeFault(
        'M019011',
        'raster_scroll is only valid in the raster callback',
        sourceSpan,
      );
    }
    if (name === 'save_commit') {
      if (arguments_.length !== 0)
        throw new RuntimeFault('M019009', 'save_commit expects no arguments', sourceSpan);
      requireMachine().work(HARDWARE.saveCapacityBytes, sourceSpan);
      try {
        saveMemory.commit();
      } catch (error) {
        throw new RuntimeFault(
          'M019012',
          error instanceof Error ? error.message : 'invalid save commit',
          sourceSpan,
        );
      }
      return;
    }
    if (name === 'map_cell' || name === 'map_flag') {
      const handle = arguments_[0];
      if (!isAssetHandle(handle, 'Map')) {
        throw new RuntimeFault('M019009', 'expected a Map asset handle', sourceSpan);
      }
      const integers = arguments_.slice(1).map((value) => readInteger(value, sourceSpan));
      if (name === 'map_cell' && integers.length === 3) {
        if (source !== undefined)
          return (
            visualStore.mapCell(
              handle.name,
              integers[0] ?? 0,
              integers[1] ?? 0,
              integers[2] ?? 0,
            ) ?? -1
          );
        return mapQueries.cell(handle.name, integers[0] ?? 0, integers[1] ?? 0, integers[2] ?? 0);
      }
      if (name === 'map_flag' && integers.length === 4) {
        if (source !== undefined)
          return visualStore.mapFlag(
            handle.name,
            integers[0] ?? 0,
            integers[1] ?? 0,
            integers[2] ?? 0,
            integers[3] ?? 0,
          );
        return mapQueries.flag(
          handle.name,
          integers[0] ?? 0,
          integers[1] ?? 0,
          integers[2] ?? 0,
          integers[3] ?? 0,
        );
      }
      throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
    }
    if (name === 'save_get_int' || name === 'save_set_int') {
      const key = arguments_[0];
      if (typeof key !== 'string') {
        throw new RuntimeFault('M019009', 'save key must be Text', sourceSpan);
      }
      try {
        if (name === 'save_get_int' && arguments_.length === 2) {
          return saveMemory.get(key, readInteger(arguments_[1], sourceSpan));
        }
        if (name === 'save_set_int' && arguments_.length === 2) {
          saveMemory.set(key, readInteger(arguments_[1], sourceSpan));
          return undefined;
        }
      } catch (error: unknown) {
        throw new RuntimeFault(
          'M019012',
          error instanceof Error ? error.message : 'invalid cartridge save operation',
          sourceSpan,
        );
      }
      throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
    }
    const command = {
      name,
      arguments: structuredClone(arguments_),
      sourceSpan,
      ...(context.rasterLine === undefined ? {} : { rasterLine: context.rasterLine }),
    };
    if (DRAW_CALLS.has(name)) {
      if (drawCommands.length >= HARDWARE.drawCommandsPerFrame) {
        throw new RuntimeFault('M019010', 'draw-command ceiling exceeded', sourceSpan);
      }
      drawCommands.push(command);
      if (rendering) {
        try {
          graphics.executeCommand(command);
        } catch (error) {
          if (error instanceof TypeError || error instanceof RangeError)
            throw new RuntimeFault('M019009', error.message, sourceSpan);
          throw error;
        }
      }
      return undefined;
    }
    if (AUDIO_CALLS.has(name)) {
      audioCommands.push(command);
      if (rendering) synthesizer.executeCommand(command);
      return undefined;
    }
    throw new RuntimeFault('M019004', `console API call '${name}' is unavailable`, sourceSpan);
  }

  function requireMachine(): DeterministicMachine {
    if (machine === undefined) {
      throw new RuntimeFault('M019102', 'no cartridge is loaded', { start: 0, end: 0 });
    }
    return machine;
  }
}

function createCartridgeInfo(rom: Uint8Array, hardwareRevision: 2): Uint8Array {
  const bytes = new Uint8Array(64);
  const view = new DataView(bytes.buffer);
  const validHeader =
    rom.length >= 12 &&
    [77, 79, 68, 45, 48, 49, 26].every((byte, index) => rom[index] === byte) &&
    rom[7] === 2;
  view.setUint16(0, hardwareRevision, true);
  view.setUint16(2, validHeader ? (rom[7] ?? 0) : 0, true);
  view.setUint32(4, rom.length, true);
  view.setUint32(8, HARDWARE.cartridgeCapacityBytes, true);
  view.setUint32(12, MEMORY.cartridgeRom, true);
  view.setUint32(16, Number(rom.length > 0) | (Number(validHeader) << 1), true);
  view.setUint32(
    20,
    validHeader ? new DataView(rom.buffer, rom.byteOffset).getUint32(8, true) : 0,
    true,
  );
  return bytes;
}

const DRAW_CALLS = new Set([
  'clear',
  'pixel',
  'line',
  'rect',
  'rect_fill',
  'circle',
  'circle_fill',
  'triangle',
  'camera',
  'clip',
  'clip_reset',
  'sprite',
  'sprite_xform',
  'sprite_region',
  'texture_line',
  'animation',
  'map',
  'pal',
  'pal_reset',
  'palt',
  'palt_reset',
  'fill_pattern',
  'fill_pattern_reset',
  'raster_scroll',
  'print',
  'font_print',
]);

const AUDIO_CALLS = new Set(['sfx', 'music', 'music_stop']);

export function consoleWorkCost(name: string, arguments_: readonly unknown[]): number {
  const integer = (index: number): number => {
    const value = arguments_[index];
    return typeof value === 'number' && Number.isSafeInteger(value) ? value : 0;
  };
  switch (name) {
    case 'clear':
      return Math.ceil((HARDWARE.width * HARDWARE.height) / 32);
    case 'pixel':
      return 1;
    case 'line':
      return Math.max(Math.abs(integer(2) - integer(0)), Math.abs(integer(3) - integer(1))) + 1;
    case 'rect':
      return Math.max(1, 2 * Math.abs(integer(2)) + 2 * Math.abs(integer(3)));
    case 'rect_fill':
      return Math.max(1, Math.ceil((Math.abs(integer(2)) * Math.abs(integer(3))) / 4));
    case 'circle':
      return Math.max(1, Math.abs(integer(2)) * 8);
    case 'circle_fill':
      return Math.max(1, Math.ceil((Math.abs(integer(2)) ** 2 * 3) / 4));
    case 'triangle': {
      const area = Math.abs(
        (integer(2) - integer(0)) * (integer(5) - integer(1)) -
          (integer(4) - integer(0)) * (integer(3) - integer(1)),
      );
      return Math.max(1, Math.ceil(area / 8));
    }
    case 'sprite':
    case 'animation':
      return 32;
    case 'sprite_xform':
      return Math.max(32, 32 * Math.abs(integer(3)) ** 2);
    case 'sprite_region': {
      if ([3, 4, 7, 8].some((index) => integer(index) <= 0)) return 1;
      return Math.min(
        Number.MAX_SAFE_INTEGER,
        Math.max(1, Math.ceil((integer(7) * integer(8)) / 4)),
      );
    }
    case 'texture_line': {
      const samples =
        Math.max(Math.abs(integer(3) - integer(1)), Math.abs(integer(4) - integer(2))) + 1;
      return Math.min(Number.MAX_SAFE_INTEGER, 2 + Math.ceil(samples / 2));
    }
    case 'map':
      return 128;
    case 'print':
      return Math.max(1, (typeof arguments_[0] === 'string' ? arguments_[0].length : 0) * 6);
    case 'font_print':
      return Math.max(1, (typeof arguments_[1] === 'string' ? arguments_[1].length : 0) * 8);
    case 'sfx':
    case 'music':
    case 'music_stop':
      return 8;
    default:
      return 1;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isAssetHandle(value: unknown, kind: string): value is { name: string; kind: string } {
  return (
    isRecord(value) &&
    typeof value.name === 'string' &&
    value.name.length > 0 &&
    value.kind === kind
  );
}

function readInteger(value: unknown, sourceSpan: SourceSpan): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) {
    throw new RuntimeFault('M019009', 'console arguments must be safe integers', sourceSpan);
  }
  return value;
}

function graphicsMatchesMemory(graphics: GraphicsSnapshot, memory: MemorySnapshot): boolean {
  return [
    [MEMORY.front, graphics.front],
    [MEMORY.display, graphics.resolved],
  ].every(([address, pixels]) => {
    if (!(pixels instanceof Uint8Array)) return false;
    const bytes = memory.regions.find((region) => region.address === address)?.bytes;
    return (
      bytes !== undefined &&
      bytes.length === pixels.length &&
      bytes.every((value, index) => value === pixels[index])
    );
  });
}

const MEMORY_CALLS = new Map([
  ['mem_read', 1],
  ['mem_read16', 1],
  ['mem_write', 2],
  ['mem_write16', 2],
  ['mem_copy', 3],
  ['mem_fill', 3],
]);
