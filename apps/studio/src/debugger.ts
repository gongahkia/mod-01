import {
  BrowserInput,
  consoleReplayObservable,
  decodeRuntimeAssets,
  evaluateWatch,
  HARDWARE,
  isConsoleRuntimeSnapshot,
  ReplayJournal,
  SandboxSession,
  WebAudioSink,
  WebGlIndexedRenderer,
  type DebugTraceEvent,
  type ControllerProfile,
  type GraphicsSnapshot,
  type InputFrame,
  type MemoryRegionDescriptor,
  type SaveImage,
  type SynthSnapshot,
  type RuntimeAssetSource,
} from '@mod01/runtime';

import { BrowserCompiler, type CompilationResult } from './compiler';
import InlineSandboxWorker from '../../../packages/runtime/src/sandbox-worker?worker&inline';
import { enhancePixelSelects } from './pixel-select';

export interface DebugProject {
  readonly id: string;
  readonly title: string;
  readonly manifest: string;
  readonly files: Readonly<Record<string, Uint8Array>>;
  readonly workcart?: string;
}

export interface ActiveDebugger {
  readonly stop: () => void;
}

interface DebuggerSnapshot {
  readonly revision: 1;
  readonly worker: unknown;
  readonly graphics: GraphicsSnapshot;
  readonly audio: SynthSnapshot;
}

type FrameResponse = Awaited<ReturnType<SandboxSession['frame']>>;
type DebugStepResponse = Awaited<ReturnType<SandboxSession['debugStep']>>;
type DebugFrameInspection = NonNullable<DebugStepResponse['inspection']>;
type DebugTab = 'SOURCE' | 'STATE' | 'TASKS' | 'PROFILE' | 'MEMORY' | 'AUDIO';

export interface SourceBreakpoint {
  readonly source: string;
  readonly line: number;
  readonly condition: string;
  readonly anchor: string;
}

const SNAPSHOT_INTERVAL = 30;
const WORK_LIMIT = HARDWARE.workUnitsPerFrame;
const decoder = new TextDecoder();

export async function openDebugger(
  root: HTMLElement,
  project: DebugProject,
  compiler: BrowserCompiler,
  save: SaveImage,
  back: () => void,
  manual: () => void,
  controllerProfile: ControllerProfile,
  audioVolume: number,
): Promise<ActiveDebugger> {
  const controller = await DebuggerController.create(
    root,
    project,
    compiler,
    save,
    back,
    manual,
    controllerProfile,
    audioVolume,
  );
  try {
    await controller.start();
  } catch (error) {
    controller.stop();
    throw error;
  }
  return {
    stop: () => {
      controller.stop();
    },
  };
}

class DebuggerController {
  private readonly journal = new ReplayJournal(3_600);
  private readonly breakpoints: Map<string, SourceBreakpoint>;
  private readonly watches: string[] = [];
  private readonly profile = new Map<string, { start: number; end: number; units: number }>();
  private readonly source: string;
  private readonly sourcePath: string;
  private readonly sources: ReadonlyMap<string, string>;
  private readonly symbolNames: ReadonlyMap<number, string>;
  private readonly sandbox: SandboxSession;
  private readonly input: BrowserInput;
  private lastPixels: Uint8Array = new Uint8Array(HARDWARE.width * HARDWARE.height);
  private readonly renderer: WebGlIndexedRenderer;
  private lastAudio: SynthSnapshot | undefined;
  private readonly packedBytes: number;
  private readonly visualBytes: number;
  private readonly visualAssets: readonly { readonly name: string; readonly kind: string }[];
  private readonly displayRasterRows: number;
  private audioSink: WebAudioSink | undefined;
  private lastFrame: FrameResponse | undefined;
  private pausedEvent: DebugTraceEvent | undefined;
  private pausedInspection: DebugFrameInspection | undefined;
  private activeInput: InputFrame | undefined;
  private selectedTrace = -1;
  private currentFrame = 0;
  private tab: DebugTab = 'SOURCE';
  private memoryAddress = 0;
  private memoryLength = 64;
  private memoryRadix: 10 | 16 = 16;
  private memoryBytes = new Uint8Array();
  private previousMemoryBytes = new Uint8Array();
  private lastMemoryAddress = -1;
  private memoryRegions: readonly MemoryRegionDescriptor[] = [];
  private readonly memoryWatchpoints = new Map<number, number>();
  private running = false;
  private busy = false;
  private stopped = false;
  private message = 'PAUSED AT FRAME 0';

  public static async create(
    root: HTMLElement,
    project: DebugProject,
    compiler: BrowserCompiler,
    save: SaveImage,
    back: () => void,
    manual: () => void,
    controllerProfile: ControllerProfile,
    audioVolume: number,
  ): Promise<DebuggerController> {
    const compilation =
      project.workcart === undefined
        ? await compiler.compileProject(project.manifest, project.files, true)
        : await compiler.compileWorkcart(project.workcart, true);
    const diagnostic = compilation.analysis.diagnostics[0];
    const generated = compilation.generated;
    if (diagnostic !== undefined || generated === undefined) {
      throw new Error(
        diagnostic === undefined
          ? 'debug compiler produced no program'
          : `${diagnostic.code} ${diagnostic.message}`,
      );
    }
    const manifest = await compiler.parseManifest(project.manifest);
    const assets = decodeRuntimeAssets(manifest.assets, project.files, manifest.display);
    const rom =
      project.workcart === undefined
        ? await compiler.packProject(project.manifest, project.files)
        : await compiler.packWorkcart(project.workcart);
    const debugSource = debugSources(compilation, project);
    return new DebuggerController(
      root,
      project,
      save,
      back,
      manual,
      compilation,
      generated.javascript,
      manifest.update_rate,
      manifest.hardware_revision ?? 1,
      assets,
      { declarations: manifest.assets, files: project.files, displayPath: manifest.display },
      rom,
      debugSource.entry,
      debugSource.entryPath,
      debugSource.files,
      controllerProfile,
      audioVolume,
    );
  }

  private constructor(
    private readonly root: HTMLElement,
    private readonly project: DebugProject,
    save: SaveImage,
    private readonly back: () => void,
    private readonly manual: () => void,
    compilation: CompilationResult,
    javascript: string,
    updateRate: 30 | 60,
    hardwareRevision: 1 | 2,
    assets: ReturnType<typeof decodeRuntimeAssets>,
    assetSource: RuntimeAssetSource,
    rom: Uint8Array,
    source: string,
    sourcePath: string,
    sources: ReadonlyMap<string, string>,
    controllerProfile: ControllerProfile,
    private readonly audioVolume: number,
  ) {
    this.source = source;
    this.sourcePath = sourcePath;
    this.sources = sources;
    this.breakpoints = loadBreakpoints(project.id, sources);
    this.symbolNames = new Map(
      compilation.analysis.symbols.map((symbol) => [symbol.id, symbol.name]),
    );
    this.packedBytes = rom.byteLength;
    this.visualBytes = assets.visualBytes;
    this.visualAssets = assets.visual.map((asset) => ({ name: asset.name, kind: asset.kind }));
    this.displayRasterRows = assets.display?.raster.length ?? 0;
    this.renderShell();
    const canvas = requireElement(this.root, '.debug-screen') as HTMLCanvasElement;
    this.sandbox = new SandboxSession(
      new InlineSandboxWorker({
        name: `mod01-debug-${project.id}`,
      }),
      1_000,
    );
    this.input = new BrowserInput(canvas, undefined, controllerProfile);
    this.renderer = new WebGlIndexedRenderer(canvas);
    this.initialization = this.sandbox.load(javascript, {
      seed: 0x4d30_3031,
      workUnitsPerFrame: WORK_LIMIT,
      updateRate,
      hardwareRevision,
      assets: assetSource,
      save,
      rom,
      debug: true,
    });
  }

  private readonly initialization: Promise<void>;

  public async start(): Promise<void> {
    await this.initialization;
    await this.refreshMemory(false);
    this.journal.recordSnapshot(0, await this.captureSnapshot());
    this.bindControls();
    this.setControlsEnabled(true);
    requireElement(this.root, '.debugger').setAttribute('aria-busy', 'false');
    this.renderer.render(this.lastPixels);
    this.render();
    (requireElement(this.root, '.debugger') as HTMLElement).focus();
  }

  public stop(): void {
    if (this.stopped) return;
    this.stopped = true;
    this.running = false;
    globalThis.removeEventListener('keydown', this.handleGlobalKey, true);
    this.input.destroy();
    this.sandbox.dispose();
    this.renderer.destroy();
    if (this.audioSink !== undefined) void this.audioSink.close();
  }

  private renderShell(): void {
    this.root.innerHTML = `
      <section class="display debugger" data-view="debugger" aria-label="MODL source debugger" aria-busy="true" tabindex="-1">
        <header class="system-bar"><span>DEBUG / ${escapeHtml(this.project.id)}</span><span>STATEMENT/1</span></header>
        <main class="debug-stage">
          <div class="debug-left">
            <canvas class="debug-screen" width="240" height="144" aria-label="Debug framebuffer"></canvas>
            <label class="timeline-label">TIMELINE <input class="debug-timeline" type="range" min="0" max="0" value="0"></label>
            <p class="debug-location">NO TRACE</p>
          </div>
          <div class="debug-right">
            <nav class="debug-tabs" aria-label="Debugger inspectors"></nav>
            <pre class="debug-output" tabindex="0"></pre>
          </div>
        </main>
        <div class="debug-actions">
          <button type="button" data-debug="run">RUN</button><button type="button" data-debug="frame">FRAME</button><button type="button" data-debug="in">IN</button><button type="button" data-debug="over">OVER</button><button type="button" data-debug="out">OUT</button><button type="button" data-debug="rewind">-1F</button><button type="button" data-debug="restart">RST</button><button type="button" data-debug="sound">SND</button>
        </div>
        <div class="debug-entry">
          <div class="source-debug-entry">
            <input class="break-line" type="number" min="1" max="${String(this.source.split('\n').length)}" value="1" aria-label="Breakpoint line"><input class="break-condition" type="text" placeholder="CONDITION" aria-label="Breakpoint condition"><button type="button" data-debug="break">BRK</button>
            <input class="watch-expression" type="text" placeholder="WATCH" aria-label="Watch expression"><button type="button" data-debug="watch">ADD</button>
          </div>
          <div class="memory-debug-entry" hidden>
            <input class="memory-address" type="text" value="000000" aria-label="Memory address"><input class="memory-length" type="number" min="1" max="64" value="64" aria-label="Memory length"><select class="memory-radix" aria-label="Memory number format"><option value="16">HEX</option><option value="10">DEC</option></select><button type="button" data-debug="memory-read">GET</button><input class="memory-value" type="text" value="00" aria-label="Memory byte value"><button type="button" data-debug="memory-write">SET</button><button type="button" data-debug="memory-watch">WP</button><button type="button" data-debug="memory-manual">MAN</button>
          </div>
        </div>
        <p class="debug-status" role="status" aria-live="polite">INITIALIZING HARDWARE</p>
        <button class="debug-back" type="button" data-debug="back">ESC BACK</button>
      </section>
    `;
    enhancePixelSelects(this.root);
    this.setControlsEnabled(false);
  }

  private setControlsEnabled(enabled: boolean): void {
    this.root
      .querySelectorAll<HTMLButtonElement | HTMLInputElement>('button, input')
      .forEach((control) => {
        control.disabled = !enabled;
      });
  }

  private bindControls(): void {
    const tabs = requireElement(this.root, '.debug-tabs');
    for (const tab of ['SOURCE', 'STATE', 'TASKS', 'PROFILE', 'MEMORY', 'AUDIO'] as const) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = tab.slice(0, 4);
      button.addEventListener('click', () => {
        this.tab = tab;
        this.render();
        if (tab === 'MEMORY') void this.refreshMemory(true);
      });
      tabs.append(button);
    }
    this.root.querySelectorAll<HTMLButtonElement>('[data-debug]').forEach((button) => {
      button.addEventListener('click', () => {
        void this.action(button.dataset.debug ?? '');
      });
    });
    const timeline = requireElement(this.root, '.debug-timeline') as HTMLInputElement;
    timeline.addEventListener('change', () => {
      void this.rewind(Number(timeline.value));
    });
    globalThis.addEventListener('keydown', this.handleGlobalKey, true);
  }

  private readonly handleGlobalKey = (event: KeyboardEvent): void => {
    if (event.key === 'Escape' && event.shiftKey) {
      event.preventDefault();
      this.stop();
      this.back();
    }
  };

  private async action(action: string): Promise<void> {
    if (this.busy || this.stopped) return;
    try {
      switch (action) {
        case 'run':
          this.running = !this.running;
          if (!this.running) this.closeAudioQueue();
          this.message = this.running ? 'RUNNING' : `PAUSED AT FRAME ${String(this.currentFrame)}`;
          this.render();
          if (this.running) this.schedule();
          break;
        case 'frame':
          await this.advance();
          break;
        case 'in':
          await this.stepStatement('in');
          break;
        case 'over':
          await this.stepStatement('over');
          break;
        case 'out':
          await this.stepStatement('out');
          break;
        case 'rewind':
          await this.rewind(Math.max(this.journal.oldestFrame, this.currentFrame - 1));
          break;
        case 'restart':
          await this.rewind(0);
          break;
        case 'break':
          this.toggleBreakpoint();
          break;
        case 'watch':
          this.addWatch();
          break;
        case 'sound':
          await this.enableSound();
          break;
        case 'memory-read':
          await this.refreshMemory(true);
          this.message = `MEMORY ${hexAddress(this.memoryAddress)} READ`;
          this.render();
          break;
        case 'memory-write':
          await this.editMemory();
          break;
        case 'memory-watch':
          await this.toggleMemoryWatchpoint();
          break;
        case 'memory-manual':
          this.stop();
          this.manual();
          break;
        case 'back':
          this.stop();
          this.back();
          break;
      }
    } catch (error: unknown) {
      this.running = false;
      this.closeAudioQueue();
      this.message = errorMessage(error);
      this.render();
    }
  }

  private schedule(): void {
    requestAnimationFrame(() => {
      void this.tick();
    });
  }

  private async tick(): Promise<void> {
    if (!this.running || this.busy || this.stopped) return;
    await this.advance();
    this.schedule();
  }

  private async advance(): Promise<void> {
    this.busy = true;
    try {
      if (this.currentFrame < this.journal.cursor) this.journal.truncate(this.currentFrame);
      for (;;) {
        const input = (this.activeInput ??= this.input.poll());
        const response = await this.sandbox.debugStep(input);
        if (response.booted === true) {
          this.activeInput = undefined;
          this.pausedEvent = undefined;
          this.pausedInspection = undefined;
          continue;
        }
        if (response.frame !== undefined) {
          await this.acceptDebugFrame(response.frame, input, true);
          if (!this.running) this.message = `PAUSED AT FRAME ${String(this.currentFrame)}`;
          break;
        }
        if (response.event === undefined || response.inspection === undefined)
          throw new Error('debug worker omitted a statement event');
        this.acceptDebugPause(response.event, response.inspection);
        const memoryHit = await this.changedMemoryWatchpoint();
        const breakpoint = this.breakpointFor(response.event);
        if (memoryHit !== undefined) {
          this.running = false;
          this.closeAudioQueue();
          this.message = `WATCH ${hexAddress(memoryHit.address)} ${byteHex(memoryHit.before)}>${byteHex(memoryHit.after)}`;
          break;
        }
        if (breakpoint !== undefined) {
          this.running = false;
          this.closeAudioQueue();
          this.message = `BREAK ${shortSource(breakpoint.source)}:${String(breakpoint.line)} / FRAME ${String(this.currentFrame)}`;
          break;
        }
      }
      if (this.tab === 'MEMORY') await this.refreshMemory(true);
      this.render();
    } finally {
      this.busy = false;
    }
  }

  private acceptDebugPause(event: DebugTraceEvent, inspection: DebugFrameInspection): void {
    this.pausedEvent = event;
    this.pausedInspection = inspection;
    this.selectedTrace = -1;
  }

  private async acceptDebugFrame(
    frame: NonNullable<DebugStepResponse['frame']>,
    input: InputFrame,
    audible: boolean,
  ): Promise<void> {
    const response: FrameResponse = { id: 0, type: 'frame', ...frame };
    if (response.frame !== this.currentFrame)
      throw new Error(
        `replay cursor expected frame ${String(this.currentFrame)}, received ${String(response.frame)}`,
      );
    this.journal.recordFrame(response.frame, input, consoleReplayObservable(response));
    this.currentFrame = response.frame + 1;
    this.activeInput = undefined;
    this.pausedEvent = undefined;
    this.pausedInspection = undefined;
    this.lastPixels = response.output.indexedPixels;
    this.lastAudio = response.output.audioState;
    this.renderer.render(this.lastPixels);
    if (audible) this.audioSink?.enqueue(response.output.audio);
    this.lastFrame = response;
    this.collectProfile(response);
    if (this.currentFrame % SNAPSHOT_INTERVAL === 0)
      this.journal.recordSnapshot(this.currentFrame, await this.captureSnapshot());
  }

  private async executeFrame(input: InputFrame, audible: boolean): Promise<FrameResponse> {
    const response = await this.sandbox.frame(input);
    if (response.debug === undefined) throw new Error('debug worker omitted trace data');
    this.lastPixels = response.output.indexedPixels;
    this.lastAudio = response.output.audioState;
    this.renderer.render(this.lastPixels);
    if (audible) this.audioSink?.enqueue(response.output.audio);
    this.lastFrame = response;
    this.activeInput = undefined;
    this.pausedEvent = undefined;
    this.pausedInspection = undefined;
    return response;
  }

  private async captureSnapshot(): Promise<DebuggerSnapshot> {
    const worker = await this.sandbox.snapshot();
    if (!isConsoleRuntimeSnapshot(worker)) throw new TypeError('worker omitted device snapshot');
    const graphics = worker.graphics;
    const audio = worker.audio;
    this.lastPixels = graphics.resolved;
    this.lastAudio = audio;
    return {
      revision: 1,
      worker,
      graphics,
      audio,
    };
  }

  private async rewind(target: number): Promise<void> {
    this.running = false;
    this.closeAudioQueue();
    this.busy = true;
    this.message = `REPLAYING TO ${String(target)}`;
    this.render();
    try {
      this.activeInput = undefined;
      this.pausedEvent = undefined;
      this.pausedInspection = undefined;
      const result = await this.journal.replay(target, {
        restore: async (value) => {
          const snapshot = readDebuggerSnapshot(value);
          await this.sandbox.restore(snapshot.worker);
          this.lastPixels = snapshot.graphics.resolved;
          this.lastAudio = snapshot.audio;
          this.renderer.render(snapshot.graphics.resolved);
          this.lastFrame = undefined;
        },
        frame: async (input) => consoleReplayObservable(await this.executeFrame(input, false)),
      });
      this.currentFrame = result.frame;
      this.selectedTrace = -1;
      await this.refreshWatchpoints();
      this.message =
        result.divergence === undefined
          ? `REWOUND TO FRAME ${String(result.frame)}`
          : `DIVERGENCE F${String(result.divergence.frame)} ${result.divergence.expected}/${result.divergence.actual}`;
    } finally {
      this.busy = false;
      this.render();
    }
  }

  private async stepStatement(kind: 'in' | 'over' | 'out'): Promise<void> {
    this.busy = true;
    const origin = this.pausedEvent;
    const depth = origin?.callStack.length;
    try {
      for (;;) {
        const input = (this.activeInput ??= this.input.poll());
        const response = await this.sandbox.debugStep(input);
        if (response.booted === true) {
          this.activeInput = undefined;
          this.pausedEvent = undefined;
          this.pausedInspection = undefined;
          this.message = 'BOOT COMPLETE';
          break;
        }
        if (response.frame !== undefined) {
          await this.acceptDebugFrame(response.frame, input, false);
          this.message = `FRAME ${String(this.currentFrame - 1)} COMPLETE`;
          break;
        }
        if (response.event === undefined || response.inspection === undefined)
          throw new Error('debug worker omitted a statement event');
        const event = response.event;
        this.acceptDebugPause(event, response.inspection);
        if (
          kind === 'in' ||
          depth === undefined ||
          (kind === 'over' && event.callStack.length <= depth) ||
          (kind === 'out' && event.callStack.length < depth) ||
          this.breakpointFor(event) !== undefined
        ) {
          const location = this.eventLocation(event);
          this.message = `PAUSED ${shortSource(location.source)}:${String(location.line)} / DEPTH ${String(event.callStack.length)}`;
          break;
        }
      }
      if (this.tab === 'MEMORY') await this.refreshMemory(true);
    } finally {
      this.busy = false;
      this.render();
    }
  }

  private breakpointFor(event: DebugTraceEvent): SourceBreakpoint | undefined {
    const location = this.eventLocation(event);
    const breakpoint = this.breakpoints.get(breakpointKey(location.source, location.line));
    if (breakpoint === undefined) return undefined;
    return breakpoint.condition.length === 0 ||
      evaluateBreakpoint(breakpoint.condition, this.environment(event))
      ? breakpoint
      : undefined;
  }

  private toggleBreakpoint(): void {
    const line = Number((requireElement(this.root, '.break-line') as HTMLInputElement).value);
    const event = this.selectedEvent();
    const source = event?.sourceSpan.source ?? this.sourcePath;
    const sourceText = this.sources.get(source) ?? this.source;
    if (!Number.isSafeInteger(line) || line < 1 || line > sourceText.split('\n').length) {
      throw new RangeError('breakpoint line is outside the current source module');
    }
    const condition = (
      requireElement(this.root, '.break-condition') as HTMLInputElement
    ).value.trim();
    const key = breakpointKey(source, line);
    if (this.breakpoints.has(key) && condition.length === 0) {
      this.breakpoints.delete(key);
      this.message = `REMOVED BREAKPOINT ${shortSource(source)}:${String(line)}`;
    } else {
      if (condition.length > 0) evaluateWatch(condition, this.environment(this.selectedEvent()));
      this.breakpoints.set(key, {
        source,
        line,
        condition,
        anchor: sourceText.split('\n')[line - 1]?.trim() ?? '',
      });
      this.message = `BREAKPOINT ${shortSource(source)}:${String(line)}${condition.length > 0 ? ' IF ' + condition : ''}`;
    }
    saveBreakpoints(this.project.id, this.breakpoints);
    this.tab = 'SOURCE';
    this.render();
  }

  private addWatch(): void {
    const input = requireElement(this.root, '.watch-expression') as HTMLInputElement;
    const expression = input.value.trim();
    if (expression.length === 0) return;
    evaluateWatch(expression, this.environment(this.selectedEvent()));
    if (!this.watches.includes(expression)) this.watches.push(expression);
    if (this.watches.length > 4) this.watches.shift();
    input.value = '';
    this.tab = 'STATE';
    this.message = `WATCHING ${expression}`;
    this.render();
  }

  private async enableSound(): Promise<void> {
    this.audioSink ??= new WebAudioSink(undefined, this.audioVolume);
    await this.audioSink.resume();
    const button = this.root.querySelector<HTMLButtonElement>('[data-debug="sound"]');
    if (button !== null) {
      button.textContent = 'ON';
      button.disabled = true;
    }
  }

  private closeAudioQueue(): void {
    const current = this.audioSink;
    this.audioSink = undefined;
    if (current !== undefined) void current.close();
    const button = this.root.querySelector<HTMLButtonElement>('[data-debug="sound"]');
    if (button !== null) {
      button.textContent = 'SOUND';
      button.disabled = false;
    }
  }

  private async refreshMemory(updateAddress: boolean): Promise<void> {
    const addressInput = this.root.querySelector<HTMLInputElement>('.memory-address');
    const lengthInput = this.root.querySelector<HTMLInputElement>('.memory-length');
    const radixInput = this.root.querySelector<HTMLSelectElement>('.memory-radix');
    if (updateAddress && addressInput !== null && lengthInput !== null) {
      this.memoryAddress = parseDebuggerAddress(addressInput.value);
      this.memoryLength = parseDebuggerInteger(lengthInput.value, 'memory length', 1, 64);
      this.memoryLength = Math.min(this.memoryLength, 0x400000 - this.memoryAddress);
      this.memoryRadix = radixInput?.value === '10' ? 10 : 16;
    }
    const response = await this.sandbox.inspectMemory(this.memoryAddress, this.memoryLength);
    this.previousMemoryBytes =
      response.address === this.lastMemoryAddress &&
      this.memoryBytes.length === response.bytes.length
        ? this.memoryBytes
        : new Uint8Array();
    this.memoryBytes = new Uint8Array(response.bytes);
    this.lastMemoryAddress = response.address;
    this.memoryRegions = response.regions;
    if (addressInput !== null) addressInput.value = hexAddress(this.memoryAddress);
    if (lengthInput !== null) lengthInput.value = String(this.memoryLength);
    this.render();
  }

  private async editMemory(): Promise<void> {
    this.running = false;
    this.closeAudioQueue();
    const address = parseDebuggerAddress(
      (requireElement(this.root, '.memory-address') as HTMLInputElement).value,
    );
    const radix =
      (requireElement(this.root, '.memory-radix') as HTMLSelectElement).value === '10' ? 10 : 16;
    const value = parseDebuggerInteger(
      (requireElement(this.root, '.memory-value') as HTMLInputElement).value,
      'memory byte',
      0,
      255,
      radix,
    );
    await this.sandbox.editMemory(address, Uint8Array.of(value));
    this.memoryAddress = address;
    await this.refreshMemory(false);
    this.message = `SET ${hexAddress(address)}=${byteHex(value)} / PAUSED`;
    this.render();
  }

  private async toggleMemoryWatchpoint(): Promise<void> {
    const address = parseDebuggerAddress(
      (requireElement(this.root, '.memory-address') as HTMLInputElement).value,
    );
    if (this.memoryWatchpoints.has(address)) {
      this.memoryWatchpoints.delete(address);
      this.message = `REMOVED WATCH ${hexAddress(address)}`;
    } else {
      if (this.memoryWatchpoints.size >= 8)
        throw new RangeError('at most eight memory watchpoints');
      const response = await this.sandbox.inspectMemory(address, 1);
      this.memoryWatchpoints.set(address, response.bytes[0] ?? 0);
      this.message = `WATCHING ${hexAddress(address)}`;
    }
    this.render();
  }

  private async changedMemoryWatchpoint(): Promise<
    { readonly address: number; readonly before: number; readonly after: number } | undefined
  > {
    for (const [address, before] of this.memoryWatchpoints) {
      const response = await this.sandbox.inspectMemory(address, 1);
      const after = response.bytes[0] ?? 0;
      this.memoryWatchpoints.set(address, after);
      if (after !== before) return { address, before, after };
    }
    return undefined;
  }

  private async refreshWatchpoints(): Promise<void> {
    for (const address of this.memoryWatchpoints.keys()) {
      const response = await this.sandbox.inspectMemory(address, 1);
      this.memoryWatchpoints.set(address, response.bytes[0] ?? 0);
    }
  }

  private collectProfile(response: FrameResponse): void {
    for (const attribution of response.attribution) {
      const key = `${String(attribution.sourceSpan.start)}:${String(attribution.sourceSpan.end)}`;
      const previous = this.profile.get(key);
      this.profile.set(key, {
        start: attribution.sourceSpan.start,
        end: attribution.sourceSpan.end,
        units: (previous?.units ?? 0) + attribution.units,
      });
    }
  }

  private selectedEvent(): DebugTraceEvent | undefined {
    return this.pausedEvent ?? this.lastFrame?.debug?.trace[this.selectedTrace];
  }

  private eventLocation(event: DebugTraceEvent): {
    readonly source: string;
    readonly line: number;
  } {
    const source = event.sourceSpan.source ?? this.sourcePath;
    return {
      source,
      line: lineForOffset(this.sources.get(source) ?? this.source, event.sourceSpan.start),
    };
  }

  private environment(event: DebugTraceEvent | undefined): Readonly<Record<string, unknown>> {
    const state = this.pausedInspection?.state ?? this.lastFrame?.debug?.inspection.state;
    return {
      ...namedValues(state, this.symbolNames),
      ...namedValues(event?.locals, this.symbolNames),
    };
  }

  private render(): void {
    const status = this.root.querySelector<HTMLElement>('.debug-status');
    if (status === null) return;
    status.textContent = this.message;
    const run = this.root.querySelector<HTMLButtonElement>('[data-debug="run"]');
    if (run !== null) run.textContent = this.running ? 'PAUSE' : 'RUN';
    const timeline = requireElement(this.root, '.debug-timeline') as HTMLInputElement;
    timeline.min = String(this.journal.oldestFrame);
    timeline.max = String(this.journal.cursor);
    timeline.value = String(this.currentFrame);
    const event = this.selectedEvent();
    const eventLocation = event === undefined ? undefined : this.eventLocation(event);
    const location = requireElement(this.root, '.debug-location');
    location.textContent =
      eventLocation === undefined
        ? `F${String(this.currentFrame).padStart(4, '0')} / NO TRACE SELECTED`
        : `F${String(this.pausedEvent === undefined ? this.currentFrame - 1 : this.currentFrame).padStart(4, '0')} ${shortSource(eventLocation.source)}:L${String(eventLocation.line)} P${String(event?.id ?? '?')}`;
    const breakpointLine = requireElement(this.root, '.break-line') as HTMLInputElement;
    const activeSource = eventLocation?.source ?? this.sourcePath;
    breakpointLine.max = String((this.sources.get(activeSource) ?? this.source).split('\n').length);
    const output = requireElement(this.root, '.debug-output');
    output.textContent = this.tabOutput(this.tab, event);
    this.root.querySelectorAll<HTMLButtonElement>('.debug-tabs button').forEach((button) => {
      button.classList.toggle('active', button.textContent === this.tab.slice(0, 4));
    });
    const memoryMode = this.tab === 'MEMORY';
    const sourceEntry = this.root.querySelector<HTMLElement>('.source-debug-entry');
    const memoryEntry = this.root.querySelector<HTMLElement>('.memory-debug-entry');
    if (sourceEntry !== null) sourceEntry.hidden = memoryMode;
    if (memoryEntry !== null) memoryEntry.hidden = !memoryMode;
  }

  private tabOutput(tab: DebugTab, event: DebugTraceEvent | undefined): string {
    switch (tab) {
      case 'SOURCE': {
        const sourcePath = event?.sourceSpan.source ?? this.sourcePath;
        const source = this.sources.get(sourcePath) ?? this.source;
        return sourceOutput(
          sourcePath,
          source,
          event,
          this.breakpoints,
          this.lastFrame?.debug?.truncated ?? false,
        );
      }
      case 'STATE': {
        const environment = this.environment(event);
        const values = Object.entries(environment).map(
          ([name, value]) => `${name} = ${shortValue(value)}`,
        );
        const watches = this.watches.map((watch) => {
          try {
            return `? ${watch} = ${shortValue(evaluateWatch(watch, environment))}`;
          } catch (error: unknown) {
            return `? ${watch} ! ${errorMessage(error)}`;
          }
        });
        return [...values, ...watches].join('\n') || 'NO STATE AT CURRENT TRACE';
      }
      case 'TASKS': {
        const tasks = this.pausedInspection?.tasks ?? this.lastFrame?.debug?.inspection.tasks;
        const stack = event?.callStack ?? [];
        return `CALL STACK\n${
          stack
            .map((frame) => {
              const source = frame.sourceSpan.source ?? this.sourcePath;
              const line = lineForOffset(
                this.sources.get(source) ?? this.source,
                frame.sourceSpan.start,
              );
              return `${frame.name} ${shortSource(source)}:${String(line)}`;
            })
            .join('\n') || '(FRAME BOUNDARY)'
        }\n\nTASKS\n${namedTaskValue(tasks, this.symbolNames)}`;
      }
      case 'PROFILE':
        return (
          [...this.profile.values()]
            .sort((left, right) => right.units - left.units || left.start - right.start)
            .slice(0, 12)
            .map(
              (item) =>
                `L${String(lineForOffset(this.source, item.start)).padStart(3, '0')} ${String(item.units).padStart(7, ' ')} WU`,
            )
            .join('\n') || 'ADVANCE A FRAME TO PROFILE'
        );
      case 'MEMORY': {
        return memoryOutput(
          this.memoryAddress,
          this.memoryBytes,
          this.previousMemoryBytes,
          this.memoryRadix,
          this.memoryRegions,
          this.memoryWatchpoints,
          {
            packedBytes: this.packedBytes,
            visualBytes: this.visualBytes,
            rasterRows: this.displayRasterRows,
            visualAssets: this.visualAssets,
          },
        );
      }
      case 'AUDIO': {
        const snapshot = this.lastAudio;
        if (snapshot === undefined) return 'NO AUDIO SNAPSHOT';
        const voices = snapshot.voices
          .map(
            (voice, index) =>
              `${String(index)} ${voice.active ? voice.sound + ' N' + String(voice.note) : '--'}`,
          )
          .join('\n');
        const tracker = snapshot.tracker;
        return `FRAME ${String(snapshot.frame)}\nTRACK ${tracker === null ? '--' : `${tracker.music} ${String(tracker.orderIndex)}:${String(tracker.row)}`}\n\n${voices}`;
      }
    }
  }
}

function memoryOutput(
  address: number,
  bytes: Uint8Array,
  previous: Uint8Array,
  radix: 10 | 16,
  regions: readonly MemoryRegionDescriptor[],
  watchpoints: ReadonlyMap<number, number>,
  metrics: {
    readonly packedBytes: number;
    readonly visualBytes: number;
    readonly rasterRows: number;
    readonly visualAssets: readonly { readonly name: string; readonly kind: string }[];
  },
): string {
  const region = regions.find(
    (entry) => address >= entry.address && address < entry.address + entry.length,
  );
  const lines = Array.from({ length: Math.ceil(bytes.length / 8) }, (_, row) => {
    const start = row * 8;
    const cells = [...bytes.slice(start, start + 8)].map((byte, column) => {
      const changed = previous[start + column] !== undefined && previous[start + column] !== byte;
      const value = radix === 16 ? byteHex(byte) : String(byte).padStart(3, '0');
      return `${changed ? '*' : ' '}${value}`;
    });
    return `${hexAddress(address + start)} ${cells.join(' ')}`;
  });
  const watched = [...watchpoints.keys()].sort((left, right) => left - right);
  const labels = regions
    .filter(
      (entry) => entry.address < address + bytes.length && entry.address + entry.length > address,
    )
    .map(
      (entry) =>
        `${hexAddress(entry.address)} ${entry.writable ? 'RW' : 'R '} ${entry.name.toUpperCase()}`,
    );
  return `${region?.name.toUpperCase() ?? 'RESERVED'} ${region?.writable ? 'RW' : 'R'} / ${radix === 16 ? 'HEX' : 'DEC'}
${lines.join('\n') || 'NO BYTES'}

${labels.join('\n') || 'UNMAPPED / READS ZERO'}
WP ${watched.map(hexAddress).join(' ') || '(NONE)'}
CART ${String(metrics.packedBytes)}/${String(HARDWARE.cartridgeCapacityBytes)} VIS ${String(metrics.visualBytes)}/${String(HARDWARE.visualCapacityBytes)} R${String(metrics.rasterRows)} A${String(metrics.visualAssets.length)}`;
}

function parseDebuggerInteger(
  source: string,
  label: string,
  minimum: number,
  maximum: number,
  forcedRadix?: 10 | 16,
): number {
  const trimmed = source.trim();
  const radix = forcedRadix ?? (/^0x/iu.test(trimmed) || /[a-f]/iu.test(trimmed) ? 16 : 10);
  const digits = trimmed.replace(/^0x/iu, '');
  const value = Number.parseInt(digits, radix);
  if (
    !/^[0-9a-f]+$/iu.test(digits) ||
    !Number.isSafeInteger(value) ||
    value < minimum ||
    value > maximum
  )
    throw new RangeError(`${label} must be ${String(minimum)}-${String(maximum)}`);
  return value;
}

function parseDebuggerAddress(source: string): number {
  return parseDebuggerInteger(source, 'memory address', 0, 0x3fffff, 16);
}

function hexAddress(value: number): string {
  return value.toString(16).toUpperCase().padStart(6, '0');
}

function byteHex(value: number): string {
  return value.toString(16).toUpperCase().padStart(2, '0');
}

function debugSources(
  compilation: CompilationResult,
  project: DebugProject,
): {
  readonly entry: string;
  readonly entryPath: string;
  readonly files: ReadonlyMap<string, string>;
} {
  const sourceMap: unknown = JSON.parse(compilation.generated?.source_map_json ?? '{}');
  const mapped = new Map<string, string>();
  if (
    typeof sourceMap === 'object' &&
    sourceMap !== null &&
    Array.isArray((sourceMap as { sources?: unknown }).sources) &&
    Array.isArray((sourceMap as { sourcesContent?: unknown }).sourcesContent) &&
    (sourceMap as { sources: unknown[] }).sources.length ===
      (sourceMap as { sourcesContent: unknown[] }).sourcesContent.length
  ) {
    const sources = (sourceMap as { sources: unknown[] }).sources;
    const contents = (sourceMap as { sourcesContent: unknown[] }).sourcesContent;
    for (const [index, source] of sources.entries()) {
      const content = contents[index];
      if (typeof source === 'string' && typeof content === 'string') mapped.set(source, content);
    }
  }
  for (const [path, bytes] of Object.entries(project.files)) {
    if (path.endsWith('.modl') && !mapped.has(path)) mapped.set(path, decoder.decode(bytes));
  }
  const entryPath =
    /^entry\s*=\s*"([A-Za-z0-9_./-]+)"\s*$/m.exec(project.manifest)?.[1] ??
    mapped.keys().next().value ??
    'src/main.modl';
  return { entry: mapped.get(entryPath) ?? '', entryPath, files: mapped };
}

function sourceOutput(
  sourcePath: string,
  source: string,
  event: DebugTraceEvent | undefined,
  breakpoints: ReadonlyMap<string, SourceBreakpoint>,
  truncated: boolean,
): string {
  const lines = source.split('\n');
  const line = event === undefined ? 1 : lineForOffset(source, event.sourceSpan.start);
  const start = Math.max(1, line - 3);
  const end = Math.min(lines.length, line + 4);
  const excerpt: string[] = [];
  for (let current = start; current <= end; current += 1) {
    excerpt.push(
      `${current === line ? '>' : ' '} ${breakpoints.has(breakpointKey(sourcePath, current)) ? '*' : ' '} ${String(current).padStart(3, '0')} ${lines[current - 1] ?? ''}`,
    );
  }
  const listed = [...breakpoints.values()]
    .sort((left, right) => left.source.localeCompare(right.source) || left.line - right.line)
    .map(
      (breakpoint) =>
        `${shortSource(breakpoint.source)}:${String(breakpoint.line)}${breakpoint.condition ? ' IF ' + breakpoint.condition : ''}`,
    );
  return `${truncated ? 'TRACE TRUNCATED AT 4096 EVENTS\n' : ''}MODULE ${sourcePath}\n${excerpt.join('\n')}\n\nBREAKPOINTS\n${listed.join('\n') || '(NONE)'}`;
}

function breakpointKey(source: string, line: number): string {
  return `${source}\u0000${String(line)}`;
}

function shortSource(source: string): string {
  return source.split('/').at(-1) ?? source;
}

const BREAKPOINT_LIMIT = 128;

function breakpointStorageKey(projectId: string): string {
  return `mod01:v1:breakpoints:${projectId}`;
}

function loadBreakpoints(
  projectId: string,
  sources: ReadonlyMap<string, string>,
): Map<string, SourceBreakpoint> {
  try {
    const stored = globalThis.localStorage.getItem(breakpointStorageKey(projectId));
    if (stored === null || stored.length > 65_536) return new Map();
    return new Map(
      remapBreakpoints(JSON.parse(stored), sources).map((breakpoint) => [
        breakpointKey(breakpoint.source, breakpoint.line),
        breakpoint,
      ]),
    );
  } catch {
    return new Map();
  }
}

function saveBreakpoints(
  projectId: string,
  breakpoints: ReadonlyMap<string, SourceBreakpoint>,
): void {
  try {
    const values = [...breakpoints.values()]
      .sort((left, right) => left.source.localeCompare(right.source) || left.line - right.line)
      .slice(0, BREAKPOINT_LIMIT);
    globalThis.localStorage.setItem(
      breakpointStorageKey(projectId),
      JSON.stringify({ revision: 1, breakpoints: values }),
    );
  } catch {
    // Debugging remains usable when storage is unavailable or full.
  }
}

export function remapBreakpoints(
  value: unknown,
  sources: ReadonlyMap<string, string>,
): readonly SourceBreakpoint[] {
  if (!isRecord(value) || value.revision !== 1 || !Array.isArray(value.breakpoints)) return [];
  const remapped = new Map<string, SourceBreakpoint>();
  for (const candidate of value.breakpoints.slice(0, BREAKPOINT_LIMIT)) {
    if (
      !isRecord(candidate) ||
      typeof candidate.source !== 'string' ||
      !Number.isSafeInteger(candidate.line) ||
      (candidate.line as number) < 1 ||
      typeof candidate.condition !== 'string' ||
      candidate.condition.length > 256 ||
      /[\r\n]/u.test(candidate.condition) ||
      typeof candidate.anchor !== 'string' ||
      candidate.anchor.length > 512
    ) {
      continue;
    }
    const source = sources.get(candidate.source);
    if (source === undefined) continue;
    const lines = source.split('\n');
    const previousLine = candidate.line as number;
    let line = Math.min(previousLine, lines.length);
    if (candidate.anchor.length > 0 && lines[line - 1]?.trim() !== candidate.anchor) {
      const matches = lines
        .map((text, index) => ({ text: text.trim(), line: index + 1 }))
        .filter((entry) => entry.text === candidate.anchor)
        .sort(
          (left, right) =>
            Math.abs(left.line - previousLine) - Math.abs(right.line - previousLine) ||
            left.line - right.line,
        );
      if (matches[0] === undefined) continue;
      line = matches[0].line;
    }
    const breakpoint = {
      source: candidate.source,
      line,
      condition: candidate.condition,
      anchor: candidate.anchor,
    };
    remapped.set(breakpointKey(breakpoint.source, breakpoint.line), breakpoint);
  }
  return [...remapped.values()];
}

function namedValues(
  value: unknown,
  names: ReadonlyMap<number, string>,
): Readonly<Record<string, unknown>> {
  if (!isRecord(value)) return {};
  return Object.fromEntries(
    Object.entries(value).map(([key, entry]) => {
      const id = /^s(\d+)$/u.exec(key)?.[1];
      const name = id === undefined ? key : (names.get(Number(id)) ?? key);
      return [name, entry];
    }),
  );
}

function namedTaskValue(value: unknown, names: ReadonlyMap<number, string>): string {
  if (!Array.isArray(value) || value.length === 0) return '(NONE)';
  return value
    .map((task, index) => {
      if (!isRecord(task)) return `${String(index)} ?`;
      const kind =
        typeof task.kind === 'number' ? (names.get(task.kind) ?? String(task.kind)) : '?';
      return `${debugScalar(task.id, index)} ${kind} PC${debugScalar(task.pc)} WAIT${debugScalar(task.wait)}\n  ${shortValue(namedValues(task.locals, names))}`;
    })
    .join('\n');
}

function evaluateBreakpoint(
  expression: string,
  environment: Readonly<Record<string, unknown>>,
): boolean {
  const value = evaluateWatch(expression, environment);
  if (typeof value !== 'boolean') throw new TypeError('breakpoint condition must produce Bool');
  return value;
}

function lineForOffset(source: string, offset: number): number {
  let line = 1;
  for (let index = 0; index < Math.min(offset, source.length); index += 1) {
    if (source[index] === '\n') line += 1;
  }
  return line;
}

function readDebuggerSnapshot(value: unknown): DebuggerSnapshot {
  if (
    !isRecord(value) ||
    value.revision !== 1 ||
    !('worker' in value) ||
    !('graphics' in value) ||
    !('audio' in value)
  ) {
    throw new TypeError('invalid debugger snapshot');
  }
  return value as unknown as DebuggerSnapshot;
}

function shortValue(value: unknown): string {
  if (value === undefined) return 'undefined';
  const text = JSON.stringify(value);
  return text.length > 160 ? `${text.slice(0, 157)}...` : text;
}

function debugScalar(value: unknown, fallback: string | number = '?'): string {
  return typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean'
    ? String(value)
    : String(fallback);
}

function requireElement(root: ParentNode, selector: string): Element {
  const element = root.querySelector(selector);
  if (element === null) throw new Error(`debugger is missing '${selector}'`);
  return element;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message.toUpperCase() : 'UNKNOWN DEBUGGER ERROR';
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}
