import {
  WorkBudget,
  isWorkBudgetSnapshot,
  type WorkAttribution,
  type WorkBudgetSnapshot,
} from './budget';
import { RuntimeFault } from './errors';
import { orderedDither } from './graphics';
import {
  emptyInputFrame,
  inputRegisterByte,
  isButton,
  isInputFrame,
  type InputFrame,
} from './input';
import type { ConsoleConfiguration, SourceSpan } from './protocol';
import { DeterministicRng } from './rng';
import { isMathCall, mathCall } from './math';
import { isTextCall, textCall } from './text';
import {
  isExecutionSnapshot,
  isFaultSpan,
  systemRegisterByte,
  type ExecutionPhase,
  type ExecutionSnapshot,
  type MachineFault,
} from './system';
export type { ExecutionPhase } from './system';

export interface CartridgeSnapshot {
  readonly state: unknown;
  readonly tasks: unknown;
  readonly nextTaskId: number;
}

export interface CartridgeInspection {
  readonly state: unknown;
  readonly tasks: unknown;
  readonly callStack: unknown;
}

export interface DebugProbeEvent {
  readonly id: number;
  readonly sourceSpan: SourceSpan;
  readonly locals: unknown;
}

export interface DebugStep {
  readonly done: boolean;
  readonly event?: DebugProbeEvent;
}

// Release cartridges return void; debug cartridges return one resumable step.
// eslint-disable-next-line @typescript-eslint/no-invalid-void-type
type CartridgeCallbackResult = void | DebugStep;

export interface GeneratedCartridge {
  start(): CartridgeCallbackResult;
  update(): CartridgeCallbackResult;
  draw(): CartridgeCallbackResult;
  raster(line: number): CartridgeCallbackResult;
  snapshot(): CartridgeSnapshot;
  restore(snapshot: CartridgeSnapshot): void;
  inspect(): CartridgeInspection;
}

export interface CartridgeApi {
  work(units: number, sourceSpan: SourceSpan): void;
  call(name: string, arguments_: readonly unknown[], sourceSpan: SourceSpan): unknown;
  fault(code: string, message: string, sourceSpan: SourceSpan): never;
  probe?(id: number, sourceSpan: SourceSpan, locals: unknown): void;
  enter?(name: string, sourceSpan: SourceSpan): void;
  leave?(): void;
}

export type CartridgeFactory = (api: CartridgeApi) => GeneratedCartridge;

export interface ExecutionContext {
  readonly frame: number;
  readonly phase: ExecutionPhase;
  readonly rasterLine?: number;
}

export interface RuntimeHooks {
  readonly completeFrame?: () => void;
  readonly call?: (
    name: string,
    arguments_: readonly unknown[],
    sourceSpan: SourceSpan,
    context: ExecutionContext,
  ) => unknown;
  readonly probe?: (id: number, sourceSpan: SourceSpan, locals: unknown) => void;
  readonly enter?: (name: string, sourceSpan: SourceSpan) => void;
  readonly leave?: () => void;
}

export interface FrameReport {
  readonly frame: number;
  readonly workUnits: number;
  readonly attribution: readonly WorkAttribution[];
}

export interface MachineDebugStep {
  readonly event?: DebugProbeEvent;
  readonly report?: FrameReport;
  readonly booted?: true;
}

export interface LegacyMachineSnapshot {
  readonly revision: 1;
  readonly frame: number;
  readonly rngState: number;
  readonly cartridge: CartridgeSnapshot;
  readonly input: InputFrame;
  readonly previousInput: InputFrame;
}

export interface MachineSnapshot extends Omit<LegacyMachineSnapshot, 'revision'> {
  readonly revision: 2;
  readonly updateRate: 30 | 60;
  readonly budget: WorkBudgetSnapshot;
  readonly execution: ExecutionSnapshot;
}

/** Deterministic callback scheduler and the only API surface visible to generated cartridge code. */
export class DeterministicMachine implements CartridgeApi {
  private readonly budget: WorkBudget;
  private readonly rng: DeterministicRng;
  private readonly updateRate: 30 | 60;
  private readonly cartridge: GeneratedCartridge;
  private readonly hooks: RuntimeHooks;
  private readonly debugEnabled: boolean;
  private currentFrame = 0;
  private booted = false;
  private input: InputFrame = emptyInputFrame();
  private previousInput: InputFrame = emptyInputFrame();
  private phase: ExecutionPhase = 'idle';
  private rasterLine: number | undefined;
  private completedUpdates = 0;
  private lastFault: MachineFault | null = null;
  private debugInput: InputFrame | undefined;

  public constructor(
    factory: CartridgeFactory,
    configuration: ConsoleConfiguration,
    hooks: RuntimeHooks = {},
  ) {
    if (![30, 60].includes(configuration.updateRate))
      throw new RangeError('update rate must be 30 or 60 Hz');
    this.budget = new WorkBudget(configuration.workUnitsPerFrame);
    this.rng = new DeterministicRng(configuration.seed);
    this.updateRate = configuration.updateRate;
    this.debugEnabled = configuration.debug === true;
    this.hooks = hooks;
    this.cartridge = factory(this);
  }

  public get frame(): number {
    return this.currentFrame;
  }

  public get cartridgeTimeSeconds(): number {
    return this.currentFrame / 60;
  }

  public boot(): void {
    this.assertRunnable();
    if (this.booted) {
      return;
    }
    this.budget.beginFrame();
    this.phase = 'start';
    this.rasterLine = undefined;
    try {
      this.completeDebugCallback(() => this.cartridge.start());
      this.booted = true;
      this.phase = 'idle';
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }

  public runFrame(input: InputFrame): FrameReport {
    if (!isInputFrame(input))
      throw new RuntimeFault('M019008', 'invalid controller input frame', { start: 0, end: 0 });
    this.assertRunnable();
    if (this.debugEnabled) {
      for (;;) {
        const step = this.stepDebug(input);
        if (step.report !== undefined) return step.report;
      }
    }
    if (!this.booted) {
      this.boot();
    }
    this.previousInput = this.input;
    this.input = structuredClone(input);
    this.budget.beginFrame();
    try {
      if (this.updateRate === 60 || this.currentFrame % 2 === 0) {
        this.phase = 'update';
        this.completeDebugCallback(() => this.cartridge.update());
        this.completedUpdates += 1;
      }
      this.phase = 'draw';
      this.completeDebugCallback(() => this.cartridge.draw());
      for (let line = 0; line < 144; line += 1) {
        this.phase = 'raster';
        this.rasterLine = line;
        this.completeDebugCallback(() => this.cartridge.raster(line));
      }
      this.rasterLine = undefined;
      this.phase = 'output';
      this.hooks.completeFrame?.();
      const report: FrameReport = {
        frame: this.currentFrame,
        workUnits: this.budget.used,
        attribution: this.budget.attribution(),
      };
      this.currentFrame += 1;
      this.phase = 'idle';
      return report;
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }

  /** Advances a debug build to exactly one statement boundary or one completed frame. */
  public stepDebug(input: InputFrame): MachineDebugStep {
    if (!this.debugEnabled)
      throw new RuntimeFault('M019015', 'statement stepping requires a debug cartridge', {
        start: 0,
        end: 0,
      });
    if (!isInputFrame(input))
      throw new RuntimeFault('M019008', 'invalid controller input frame', { start: 0, end: 0 });
    if (this.lastFault !== null) this.assertRunnable();
    try {
      if (this.phase === 'idle') {
        this.debugInput = structuredClone(input);
        this.budget.beginFrame();
        if (!this.booted) {
          this.phase = 'start';
        } else {
          this.beginDebugFrame();
        }
      }
      for (;;) {
        const step = this.stepDebugPhase();
        if (step.event !== undefined) {
          this.probe(step.event.id, step.event.sourceSpan, step.event.locals);
          return { event: structuredClone(step.event) };
        }
        if (!step.done) throw new TypeError('invalid generated debug step');
        const boundary = this.advanceDebugPhase();
        if (boundary === 'booted') return { booted: true };
        if (boundary !== undefined) return { report: boundary };
      }
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }

  private beginDebugFrame(): void {
    const input = this.debugInput ?? emptyInputFrame();
    this.previousInput = this.input;
    this.input = structuredClone(input);
    this.phase = this.updateRate === 60 || this.currentFrame % 2 === 0 ? 'update' : 'draw';
  }

  private stepDebugPhase(): DebugStep {
    switch (this.phase) {
      case 'start':
        return this.cartridge.start() ?? { done: true };
      case 'update':
        return this.cartridge.update() ?? { done: true };
      case 'draw':
        return this.cartridge.draw() ?? { done: true };
      case 'raster':
        return this.cartridge.raster(this.rasterLine ?? 0) ?? { done: true };
      case 'idle':
      case 'output':
        throw new TypeError(`cannot step machine phase ${this.phase}`);
    }
  }

  private advanceDebugPhase(): FrameReport | 'booted' | undefined {
    switch (this.phase) {
      case 'start':
        this.booted = true;
        this.phase = 'idle';
        this.debugInput = undefined;
        return 'booted';
      case 'update':
        this.completedUpdates += 1;
        this.phase = 'draw';
        return undefined;
      case 'draw':
        this.phase = 'raster';
        this.rasterLine = 0;
        return undefined;
      case 'raster': {
        if ((this.rasterLine ?? 0) < 143) {
          this.rasterLine = (this.rasterLine ?? 0) + 1;
          return undefined;
        }
        this.rasterLine = undefined;
        this.phase = 'output';
        this.hooks.completeFrame?.();
        const report: FrameReport = {
          frame: this.currentFrame,
          workUnits: this.budget.used,
          attribution: this.budget.attribution(),
        };
        this.currentFrame += 1;
        this.phase = 'idle';
        this.debugInput = undefined;
        return report;
      }
      case 'idle':
      case 'output':
        throw new TypeError(`cannot advance machine phase ${this.phase}`);
    }
  }

  private completeDebugCallback(callback: () => CartridgeCallbackResult): void {
    for (;;) {
      const step = callback();
      if (step === undefined || step.done) return;
      if (step.event !== undefined)
        this.probe(step.event.id, step.event.sourceSpan, step.event.locals);
    }
  }

  public snapshot(): MachineSnapshot {
    if (this.phase !== 'idle' && this.lastFault === null)
      throw new TypeError('machine snapshots require a completed frame or fault boundary');
    return structuredClone({
      revision: 2,
      frame: this.currentFrame,
      rngState: this.rng.state,
      cartridge: this.cartridge.snapshot(),
      input: this.input,
      previousInput: this.previousInput,
      updateRate: this.updateRate,
      budget: this.budget.snapshot(),
      execution: {
        booted: this.booted,
        updates: this.completedUpdates,
        phase: this.phase,
        rasterLine: this.rasterLine ?? null,
        fault: this.lastFault,
      },
    });
  }

  public restore(value: unknown): void {
    if (!isMachineSnapshot(value)) {
      throw new TypeError('invalid MOD-01 machine snapshot');
    }
    const snapshot = value;
    if (
      snapshot.revision === 2 &&
      (snapshot.updateRate !== this.updateRate || snapshot.budget.limit !== this.budget.limit)
    )
      throw new TypeError('machine snapshot does not match the execution configuration');
    const previous = structuredClone(this.cartridge.snapshot());
    try {
      this.cartridge.restore(structuredClone(snapshot.cartridge));
    } catch (error) {
      this.cartridge.restore(previous);
      throw error;
    }
    if (snapshot.revision === 2) {
      this.budget.restore(snapshot.budget);
      this.booted = snapshot.execution.booted;
      this.completedUpdates = snapshot.execution.updates;
      this.phase = snapshot.execution.phase;
      this.rasterLine = snapshot.execution.rasterLine ?? undefined;
      this.lastFault = structuredClone(snapshot.execution.fault);
    } else {
      this.budget.beginFrame();
      this.booted = this.booted || snapshot.frame > 0;
      this.completedUpdates =
        this.updateRate === 60 ? snapshot.frame : Math.ceil(snapshot.frame / 2);
      this.phase = 'idle';
      this.rasterLine = undefined;
      this.lastFault = null;
    }
    this.currentFrame = snapshot.frame;
    this.rng.restore(snapshot.rngState);
    this.input = structuredClone(snapshot.input);
    this.previousInput = structuredClone(snapshot.previousInput);
    this.debugInput = undefined;
  }

  public inspect(): CartridgeInspection {
    return this.cartridge.inspect();
  }

  public readInputByte(offset: number): number {
    return inputRegisterByte(this.input, this.previousInput, offset);
  }

  public readSystemByte(offset: number): number {
    return systemRegisterByte(
      {
        frame: this.currentFrame,
        updates: this.completedUpdates,
        seconds: this.cartridgeTimeSeconds,
        rngState: this.rng.state,
        updateRate: this.updateRate,
        phase: this.phase,
        rasterLine: this.rasterLine,
        booted: this.booted,
        fault: this.lastFault,
        used: this.budget.used,
        limit: this.budget.limit,
      },
      offset,
    );
  }

  public assertRunnable(): void {
    if (this.lastFault !== null)
      throw new RuntimeFault(
        'M019014',
        'cartridge faulted; restart or restore a healthy checkpoint',
        this.lastFault.sourceSpan,
      );
    if (this.phase !== 'idle')
      throw new RuntimeFault('M019014', 'cartridge is already executing', { start: 0, end: 0 });
    if (this.currentFrame === Number.MAX_SAFE_INTEGER)
      this.fault('M019012', 'display-frame counter is exhausted', { start: 0, end: 0 });
  }

  public work(units: number, sourceSpan: SourceSpan): void {
    try {
      this.budget.charge(units, sourceSpan);
    } catch (error) {
      this.rememberFault(error, sourceSpan);
      throw error;
    }
  }

  public call(name: string, arguments_: readonly unknown[], sourceSpan: SourceSpan): unknown {
    if (isTextCall(name))
      return textCall(name, arguments_, sourceSpan, (units, span) => {
        this.work(units, span);
      });
    if (isMathCall(name))
      return mathCall(name, arguments_, sourceSpan, (units, span) => {
        this.work(units, span);
      });
    switch (name) {
      case 'rng_num':
        expectArguments(name, arguments_, 0, sourceSpan);
        return this.rng.nextNum();
      case 'rng_int': {
        expectArguments(name, arguments_, 2, sourceSpan);
        const minimum = expectInteger(arguments_[0], sourceSpan);
        const maximum = expectInteger(arguments_[1], sourceSpan);
        try {
          return this.rng.nextInt(minimum, maximum);
        } catch (error: unknown) {
          return this.fault(
            'M019007',
            error instanceof Error ? error.message : 'invalid RNG bounds',
            sourceSpan,
          );
        }
      }
      case 'Vec2':
        expectArguments(name, arguments_, 2, sourceSpan);
        return {
          x: expectNumber(arguments_[0], sourceSpan),
          y: expectNumber(arguments_[1], sourceSpan),
        };
      case 'Rect':
        expectArguments(name, arguments_, 4, sourceSpan);
        return {
          x: expectNumber(arguments_[0], sourceSpan),
          y: expectNumber(arguments_[1], sourceSpan),
          w: expectNumber(arguments_[2], sourceSpan),
          h: expectNumber(arguments_[3], sourceSpan),
        };
      case 'dither': {
        expectArguments(name, arguments_, 5, sourceSpan);
        return orderedDither(
          expectInteger(arguments_[0], sourceSpan),
          expectInteger(arguments_[1], sourceSpan),
          expectInteger(arguments_[2], sourceSpan),
          expectInteger(arguments_[3], sourceSpan),
          expectInteger(arguments_[4], sourceSpan),
        );
      }
      case 'btn':
      case 'btnp': {
        expectArguments(name, arguments_, 2, sourceSpan);
        const port = expectInteger(arguments_[0], sourceSpan);
        const button = arguments_[1];
        if (port < 0 || port >= 4 || !isButton(button)) {
          return this.fault('M019008', 'invalid controller port or button', sourceSpan);
        }
        const pressed = this.input.controllers[port]?.buttons[button] ?? false;
        if (name === 'btn') {
          return pressed;
        }
        return pressed && !(this.previousInput.controllers[port]?.buttons[button] ?? false);
      }
      case 'pointer_x':
        expectArguments(name, arguments_, 0, sourceSpan);
        return this.input.pointer.x;
      case 'pointer_y':
        expectArguments(name, arguments_, 0, sourceSpan);
        return this.input.pointer.y;
      case 'pointer_inside':
        expectArguments(name, arguments_, 0, sourceSpan);
        return this.input.pointer.inside;
      case 'pointer_primary':
      case 'pointer_secondary': {
        expectArguments(name, arguments_, 0, sourceSpan);
        const button = name === 'pointer_primary' ? 'primary' : 'secondary';
        return this.input.pointer[button] && !this.previousInput.pointer[button];
      }
      default: {
        const context: ExecutionContext = {
          frame: this.currentFrame,
          phase: this.phase,
          ...(this.rasterLine === undefined ? {} : { rasterLine: this.rasterLine }),
        };
        const result = this.hooks.call?.(name, arguments_, sourceSpan, context);
        if (this.hooks.call === undefined) {
          return this.fault('M019004', `console API call '${name}' is unavailable`, sourceSpan);
        }
        return result;
      }
    }
  }

  public fault(code: string, message: string, sourceSpan: SourceSpan): never {
    const error = new RuntimeFault(code, message, sourceSpan);
    this.rememberFault(error);
    throw error;
  }

  private rememberFault(error: unknown, fallback: SourceSpan = { start: 0, end: 0 }): void {
    const span = error instanceof RuntimeFault ? error.sourceSpan : fallback;
    this.lastFault = {
      code:
        error instanceof RuntimeFault && /^M019\d{3}$/.test(error.code)
          ? Number(error.code.slice(3))
          : 9199,
      sourceSpan: isFaultSpan(span) ? { ...span } : { start: 0, end: 0 },
    };
  }

  public probe(id: number, sourceSpan: SourceSpan, locals: unknown): void {
    this.hooks.probe?.(id, sourceSpan, locals);
  }

  public enter(name: string, sourceSpan: SourceSpan): void {
    this.hooks.enter?.(name, sourceSpan);
  }

  public leave(): void {
    this.hooks.leave?.();
  }
}

export function isMachineSnapshot(
  value: unknown,
): value is MachineSnapshot | LegacyMachineSnapshot {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  const base =
    Number.isSafeInteger(candidate.frame) &&
    typeof candidate.frame === 'number' &&
    candidate.frame >= 0 &&
    Number.isSafeInteger(candidate.rngState) &&
    isInputFrame(candidate.input) &&
    isInputFrame(candidate.previousInput) &&
    typeof candidate.cartridge === 'object' &&
    candidate.cartridge !== null;
  if (!base) return false;
  if (candidate.revision === 1) return Object.keys(candidate).length === 6;
  return (
    candidate.revision === 2 &&
    Object.keys(candidate).length === 9 &&
    typeof candidate.rngState === 'number' &&
    candidate.rngState > 0 &&
    candidate.rngState <= 0xffffffff &&
    (candidate.updateRate === 30 || candidate.updateRate === 60) &&
    isWorkBudgetSnapshot(candidate.budget) &&
    isExecutionSnapshot(
      candidate.execution,
      candidate.frame as number,
      candidate.updateRate,
      candidate.budget,
    )
  );
}

function expectArguments(
  name: string,
  arguments_: readonly unknown[],
  count: number,
  sourceSpan: SourceSpan,
): void {
  if (arguments_.length !== count) {
    throw new RuntimeFault(
      'M019009',
      `${name} expected ${String(count)} arguments, received ${String(arguments_.length)}`,
      sourceSpan,
    );
  }
}

function expectNumber(value: unknown, sourceSpan: SourceSpan): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new RuntimeFault('M019009', 'expected a finite number', sourceSpan);
  }
  return value;
}

function expectInteger(value: unknown, sourceSpan: SourceSpan): number {
  const number = expectNumber(value, sourceSpan);
  if (!Number.isSafeInteger(number)) {
    throw new RuntimeFault('M019009', 'expected a safe integer', sourceSpan);
  }
  return number;
}
