import {
  BUTTONS,
  BrowserInput,
  HARDWARE,
  IndexedDbStorage,
  MASTER_PALETTE_RGBA,
  REPLAY_MAX_BYTES,
  SandboxSession,
  StudioRepository,
  WebAudioSink,
  WebGlIndexedRenderer,
  decodeCartridgePng,
  decodeCartridgeFragment,
  decodeReplayTrace,
  defaultControllerProfile,
  emptyInputFrame,
  encodeCartridgePng,
  encodeCartridgeFragment,
  encodeIndexedGif,
  encodeReplayTrace,
  encodeRgbaPng,
  encodeSingleFileZip,
  replayInputFrames,
  remapControllerKey,
  type Button,
  type DecodedPng,
  type InputFrame,
  type ReplayTrace,
  type StoredProject,
  type StudioSettings,
} from '@mod01/runtime';

import {
  BrowserCompiler,
  type CompilationResult,
  type CompilerDiagnostic,
  type ProjectManifest,
} from './compiler';
import InlineSandboxWorker from '../../../packages/runtime/src/sandbox-worker?worker&inline';
import kenneyActionCursor from './assets/kenney-cursor-pixel-pack/tile_0134.png?inline';
import kenneyCanvasCursor from './assets/kenney-cursor-pixel-pack/tile_0048.png?inline';
import kenneyDefaultCursor from './assets/kenney-cursor-pixel-pack/tile_0028.png?inline';
import kenneyTextCursor from './assets/kenney-cursor-pixel-pack/tile_0120.png?inline';
import kenneyWaitCursor from './assets/kenney-cursor-pixel-pack/tile_0012.png?inline';
import mod01Logo from './assets/mod01-logo.png?inline';
import { deriveCodeStateGraph, renderCodeStateGraph } from './compiler-graph';
import { openDebugger, type ActiveDebugger } from './debugger';
import { enhancePixelSelects } from './pixel-select';
import {
  openAssetWorkbench,
  openCreationTool,
  type CreationTool,
  type WorkbenchRecipe,
} from './tools';

interface WorkingProject {
  id: string;
  title: string;
  manifest: string;
  revision: number;
  files: Record<string, Uint8Array>;
  /** Canonical authored source for V2 projects; manifest/files are editor-facing views. */
  workcart?: string;
}

interface ActivePlayer {
  readonly sandbox: SandboxSession;
  readonly input: BrowserInput;
  readonly stop: () => void;
}

interface FolderBinding {
  readonly handle: FileSystemDirectoryHandle;
  modified: Map<string, number>;
  workcartPath?: string;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const PROJECT_ID = /^[a-z0-9][a-z0-9.-]{2,63}$/;
const TERMINAL_LINE_DELAY_MS = 52;
const BUNDLED_CARTRIDGES = [
  'cinder-circuit',
  'ashvault',
  'raster-rush',
  'mod01-service',
  'signal-4k',
  'pocket-relay',
  'hardware-gauntlet',
  'modl-tutorial',
] as const;
const STARTUP_CATALOG = [
  'CATALOG // RUN <ID> TO PLAY',
  'CINDER CIRCUIT / RUN cinder-circuit',
  'ASHVAULT / RUN ashvault',
  'RASTER RUSH 99 / RUN raster-rush',
  'MOD-01 SERVICE / RUN mod01-service',
  'SIGNAL 4K / RUN signal-4k',
  'POCKET RELAY / RUN pocket-relay',
  'HARDWARE GAUNTLET / RUN hardware-gauntlet',
  'FIRST SIGNAL / RUN modl-tutorial',
] as const;

/** Diegetic boot monitor, command shell, source editor, and cartridge player foundation. */
export class StudioApp {
  private readonly root: HTMLElement;
  private readonly repository: StudioRepository;
  private readonly compiler = new BrowserCompiler();
  private activeProject: WorkingProject | undefined;
  private readonly terminalLines: string[] = [];
  private visibleTerminalLines = 0;
  private terminalRevealTimer: number | undefined;
  private readonly terminalIdleResolvers: Array<() => void> = [];
  private readonly history: string[] = [];
  private historyCursor = 0;
  private player: ActivePlayer | undefined;
  private activeDebugger: ActiveDebugger | undefined;
  private readonly capturedFrames = new Map<string, Uint8Array>();
  private readonly folderBindings = new Map<string, FolderBinding>();
  private pendingRemovalId: string | undefined;

  public constructor(root: HTMLElement, databaseName = 'mod01-studio') {
    this.root = root;
    this.repository = new StudioRepository(new IndexedDbStorage(databaseName));
  }

  public async boot(): Promise<void> {
    delete document.documentElement.dataset.studioReady;
    document.documentElement.style.setProperty(
      '--mod01-cursor-default',
      `url("${kenneyDefaultCursor}") 0 0`,
    );
    document.documentElement.style.setProperty(
      '--mod01-cursor-action',
      `url("${kenneyActionCursor}") 1 1`,
    );
    document.documentElement.style.setProperty(
      '--mod01-cursor-text',
      `url("${kenneyTextCursor}") 8 8`,
    );
    document.documentElement.style.setProperty(
      '--mod01-cursor-wait',
      `url("${kenneyWaitCursor}") 8 8`,
    );
    document.documentElement.style.setProperty(
      '--mod01-cursor-canvas',
      `url("${kenneyCanvasCursor}") 1 1`,
    );
    const startup = this.playStartupSequence();
    this.stopPlayer();
    this.stopDebugger();
    this.applySettings(await this.repository.settings());
    this.stopTerminalReveal();
    this.terminalLines.length = 0;
    this.visibleTerminalLines = 0;
    this.appendLines(['MODL/1 READY', ...STARTUP_CATALOG, "TYPE 'HELP' FOR COMMANDS"]);
    const installed = await this.installBundledCartridges();
    if (installed > 0) {
      this.appendLines([`${String(installed)} BUILT-IN CARTRIDGES INSTALLED`]);
    }
    await this.importUrlFragment();
    const projects = await this.repository.listProjects();
    if (projects.length > 0 && this.activeProject === undefined) {
      this.activeProject = fromStored(projects[0] as StoredProject);
      this.appendLines([
        `AUTOLOAD ${this.activeProject.id} R${String(this.activeProject.revision)}`,
      ]);
    }
    await startup;
    this.renderShell();
    await this.waitForTerminalIdle();
    document.documentElement.dataset.studioReady = 'true';
  }

  /** A short, skippable power-on sequence; all checks name local browser subsystems. */
  private playStartupSequence(): Promise<void> {
    this.root.innerHTML = `
      <section class="display boot-screen" data-view="boot" aria-label="MOD-01 startup sequence">
        <div class="boot-power" aria-hidden="true"></div>
        <img class="boot-emblem" src="${mod01Logo}" alt="" />
        <div class="boot-terminal" role="status" aria-live="polite" aria-label="MOD-01 startup checks">
          <p class="boot-rom">MOD-01 SYSTEM ROM 1.0</p>
        </div>
        <button class="boot-skip" type="button">PRESS ENTER TO SKIP</button>
      </section>
    `;
    const terminal = requireElement(this.root, '.boot-terminal');
    const skip = requireElement(this.root, '.boot-skip') as HTMLButtonElement;
    const checks = [
      'POWER RELAY // WHIRR',
      'VISUAL SRAM 128K ....... OK',
      'SOUND ASIC / 8 VOICES ... OK',
      'LOCAL STORE ............ OK',
      'CARTRIDGE BUS .......... SCANNED',
      'WORKER SANDBOX ......... ARMED',
      'MODL/1 MONITOR ......... READY',
    ];

    return new Promise((resolve) => {
      let finished = false;
      const timers: number[] = [];
      const finish = (): void => {
        if (finished) return;
        finished = true;
        for (const timer of timers) window.clearTimeout(timer);
        window.removeEventListener('keydown', onKeyDown);
        skip.removeEventListener('click', finish);
        this.root.querySelector<HTMLElement>('.boot-screen')?.classList.add('boot-complete');
        resolve();
      };
      const onKeyDown = (event: KeyboardEvent): void => {
        if (event.key !== 'Enter' && event.key !== ' ' && event.key !== 'Escape') return;
        event.preventDefault();
        finish();
      };

      skip.addEventListener('click', finish);
      window.addEventListener('keydown', onKeyDown);
      checks.forEach((check, index) => {
        timers.push(
          window.setTimeout(
            () => {
              if (finished) return;
              const line = document.createElement('p');
              line.className = 'boot-check';
              line.textContent = check;
              terminal.append(line);
            },
            100 + index * 125,
          ),
        );
      });
      timers.push(window.setTimeout(finish, 1_250));
    });
  }

  private async installBundledCartridges(): Promise<number> {
    let installed = 0;
    for (const id of BUNDLED_CARTRIDGES) {
      if (await this.repository.isProjectRemoved(id)) continue;
      if ((await this.repository.loadProject(id)) !== undefined) {
        await this.repository.setShelfOrigin(id, 'bundled');
        continue;
      }
      const response = await fetch(new URL(`./cartridges/${id}.m01c`, document.baseURI));
      if (!response.ok) {
        throw new Error(`BUILT-IN CARTRIDGE ${id} COULD NOT BE READ`);
      }
      const unpacked = await this.compiler.unpackCartridge(
        new Uint8Array(await response.arrayBuffer()),
      );
      const manifest = await this.compiler.parseManifest(unpacked.manifest);
      await this.repository.saveProject({
        id: manifest.id,
        title: manifest.title,
        manifest: unpacked.manifest,
        files: Object.fromEntries(
          Object.entries(unpacked.files).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
        ),
      });
      await this.repository.setShelfOrigin(id, 'bundled');
      installed += 1;
    }
    return installed;
  }

  private renderShell(): void {
    this.root.innerHTML = `
      <section class="display shell" data-view="shell" aria-label="MOD-01 monitor shell">
        <header class="system-bar"><span class="system-product-brand"><img class="shell-logo" src="${mod01Logo}" alt="" />MOD-01</span></header>
        <div class="terminal" role="log" aria-live="polite" aria-relevant="additions text"></div>
        <form class="command-line">
          <label for="command">&gt;</label>
          <input id="command" name="command" autocomplete="off" autocapitalize="off" spellcheck="false" aria-label="MOD-01 command" />
        </form>
      </section>
    `;
    this.refreshTerminal();
    const form = requireElement(this.root, '.command-line') as HTMLFormElement;
    const input = requireElement(this.root, '#command') as HTMLInputElement;
    form.addEventListener('submit', (event) => {
      event.preventDefault();
      const command = input.value.trim();
      input.value = '';
      if (command.length === 0) {
        return;
      }
      this.history.push(command);
      this.historyCursor = this.history.length;
      this.appendLines([`> ${command}`]);
      input.disabled = true;
      void this.execute(command).finally(() => {
        if (this.root.contains(input)) {
          input.disabled = false;
          input.focus();
        }
      });
    });
    input.addEventListener('keydown', (event) => {
      if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        event.preventDefault();
        this.historyCursor = Math.max(
          0,
          Math.min(this.history.length, this.historyCursor + (event.key === 'ArrowUp' ? -1 : 1)),
        );
        input.value = this.history[this.historyCursor] ?? '';
      }
    });
    input.focus();
  }

  private async execute(commandLine: string): Promise<void> {
    const [command = '', ...arguments_] = splitCommand(commandLine);
    const normalizedCommand = command.toLowerCase();
    if (normalizedCommand !== 'remove') this.pendingRemovalId = undefined;
    try {
      switch (normalizedCommand) {
        case 'dir':
          await this.directory();
          break;
        case 'new':
          await this.newProject(arguments_);
          break;
        case 'load':
          await this.loadProject(arguments_[0]);
          break;
        case 'import':
          this.openImporter();
          return;
        case 'save':
          if (arguments_[0] === undefined) await this.saveProject();
          else await this.openSaveManager(await this.cartridgeForCommand(arguments_[0], 'SAVE'));
          return;
        case 'recover':
          await this.recoverProject(arguments_[0]);
          break;
        case 'edit':
          this.openEditor(arguments_[0]);
          return;
        case 'sprite':
        case 'map':
        case 'palette':
        case 'font':
        case 'sfx':
        case 'music':
        case 'project':
          await this.openTool(command as CreationTool);
          return;
        case 'assets':
        case 'workbench':
          await this.openWorkbench();
          return;
        case 'manual':
        case 'man':
          this.openManual(arguments_.join(' ') || 'START');
          return;
        case 'explore':
          await this.openExplorer();
          return;
        case 'run':
          await this.activateCartridgeForCommand(arguments_[0], 'RUN');
          await this.runProject();
          return;
        case 'debug':
          await this.activateCartridgeForCommand(arguments_[0], 'DEBUG');
          await this.debugProject();
          return;
        case 'pack':
        case 'out':
          await this.activateCartridgeForCommand(arguments_[0], 'PACK');
          await this.packProject();
          break;
        case 'cart':
          await this.activateCartridgeForCommand(arguments_[0], 'CART');
          await this.exportCartridgePng();
          break;
        case 'share':
          await this.openShare();
          return;
        case 'info':
          await this.info();
          break;
        case 'settings':
        case 'controls':
          await this.openSettings();
          return;
        case 'folder':
          await this.openProjectFolder();
          break;
        case 'inspect':
        case 'source':
          await this.activateCartridgeForCommand(arguments_[0], 'INSPECT');
          await this.openInspector();
          return;
        case 'copy': {
          const project = await this.cartridgeForCommand(arguments_[0], 'COPY');
          const copy = await this.duplicateShelfProject(project);
          this.activeProject = copy;
          this.appendLines([`DUPLICATED ${project.id} AS ${copy.id}`]);
          break;
        }
        case 'name': {
          const project = await this.cartridgeForCommand(arguments_[0], 'NAME');
          const title = arguments_.slice(1).join(' ');
          if (title.length === 0) throw new Error('NAME REQUIRES A TITLE');
          await this.renameCartridge(project, title);
          break;
        }
        case 'star': {
          const project = await this.cartridgeForCommand(arguments_[0], 'STAR');
          const state = await this.repository.shelfState(project.id);
          await this.repository.saveShelfState(project.id, { ...state, favorite: !state.favorite });
          this.appendLines([`${state.favorite ? 'UNSTARRED' : 'STARRED'} ${project.id}`]);
          break;
        }
        case 'remove':
          await this.removeCartridgeFromCommand(arguments_[0]);
          break;
        case 'shell':
          this.renderShell();
          return;
        case 'clear':
        case 'cls':
          this.clearTerminal();
          return;
        case 'help':
          if (arguments_.length > 0) {
            this.openManual(arguments_.join(' '));
            return;
          }
          this.appendLines([
            'DIR NEW LOAD SAVE RECOVER IMPORT',
            'EDIT RUN [ID] DEBUG [ID] PACK [ID] OUT [ID] CART [ID] EXPORT SHARE',
            'INSPECT [ID] SOURCE [ID] INFO',
            'COPY <ID> NAME <ID> [TITLE] STAR <ID> SAVE <ID> REMOVE <ID> SHELL',
            'ASSETS WORKBENCH / PROJECT SPRITE MAP PALETTE FONT SFX MUSIC',
            'MANUAL MAN HELP CLEAR CLS REBOOT SETTINGS CONTROLS FOLDER',
            'NEW <ID> [TITLE] / LOAD <ID> / RUN <ID>',
          ]);
          break;
        case 'reboot':
          await this.boot();
          return;
        case 'export':
          if (arguments_[0]?.toLowerCase() === 'zip') await this.exportZip();
          else await this.exportHtml();
          break;
        case '':
          break;
        default:
          this.appendLines([`?UNKNOWN COMMAND: ${command.toUpperCase()}`]);
      }
    } catch (error: unknown) {
      this.appendLines([`!${errorMessage(error)}`]);
    }
    this.refreshTerminal();
  }

  private async directory(): Promise<void> {
    const projects = await this.repository.listProjects();
    if (projects.length === 0) {
      this.appendLines(['NO CARTRIDGES']);
      return;
    }
    this.appendLines(
      projects.map(
        (project) =>
          `${project.id === this.activeProject?.id ? '*' : ' '} ${project.id} R${String(project.revision)} ${project.title}`,
      ),
    );
  }

  private async newProject(arguments_: readonly string[]): Promise<void> {
    const [id, ...titleParts] = arguments_;
    if (id === undefined || !PROJECT_ID.test(id)) {
      throw new Error('NEW REQUIRES A 3-64 CHARACTER LOWERCASE ID');
    }
    if ((await this.repository.loadProject(id)) !== undefined) {
      throw new Error(`CARTRIDGE '${id}' ALREADY EXISTS`);
    }
    const title = (titleParts.join(' ') || id.replaceAll(/[.-]+/g, ' ')).toUpperCase().slice(0, 64);
    const manifest = `format = 1\nlanguage = "MODL/1"\nid = "${id}"\ntitle = ${JSON.stringify(title)}\nauthor = "@gongahkia"\nversion = "0.1.0"\nentry = "src/main.modl"\nupdate_rate = 60\n\n[assets]\n`;
    const files = {
      'src/main.modl': encoder.encode(
        '// Made by @gongahkia\n\nstate player_x: Int = 112\n\non update:\n  if btn(pad1, left):\n    player_x -= 1\n  if btn(pad1, right):\n    player_x += 1\n\non draw:\n  clear(1)\n  rect_fill(player_x, 64, 16, 16, 23)\n  print("MODL/1", 98, 88, 7)\n',
      ),
    };
    const project = await this.repository.saveProject({
      id,
      title,
      manifest,
      files,
      workcart: await this.compiler.encodeWorkcart(manifest, files),
    });
    this.activeProject = fromStored(project);
    await this.repository.setShelfOrigin(id, 'created');
    this.appendLines([`CREATED ${id}`, "EDIT 'src/main.modl' OR RUN"]);
  }

  private async loadProject(id: string | undefined): Promise<void> {
    if (id === undefined) {
      throw new Error('LOAD REQUIRES A CARTRIDGE ID');
    }
    const project = await this.repository.loadProject(id);
    if (project === undefined) {
      throw new Error(`CARTRIDGE '${id}' NOT FOUND`);
    }
    this.activeProject = fromStored(project);
    this.appendLines([`LOADED ${id} R${String(project.revision)}`]);
  }

  private async cartridgeForCommand(
    id: string | undefined,
    operation: string,
  ): Promise<WorkingProject> {
    const cartridgeId = id ?? this.activeProject?.id;
    if (cartridgeId === undefined) throw new Error(`${operation} REQUIRES A CARTRIDGE ID`);
    const project = await this.repository.loadProject(cartridgeId);
    if (project === undefined) throw new Error(`CARTRIDGE '${cartridgeId}' NOT FOUND`);
    return fromStored(project);
  }

  private async activateCartridgeForCommand(
    id: string | undefined,
    operation: string,
  ): Promise<WorkingProject> {
    const project = await this.cartridgeForCommand(id, operation);
    this.activeProject = project;
    return project;
  }

  private async removeCartridgeFromCommand(id: string | undefined): Promise<void> {
    if (id === undefined) throw new Error('REMOVE REQUIRES A CARTRIDGE ID');
    if (await this.repository.isProjectRemoved(id)) {
      const restored = await this.repository.restoreRemovedProject(id);
      this.activeProject = fromStored(restored);
      this.pendingRemovalId = undefined;
      this.appendLines([`RESTORED ${id} FROM CART BAY BIN`]);
      return;
    }
    await this.cartridgeForCommand(id, 'REMOVE');
    if (this.pendingRemovalId !== id) {
      this.pendingRemovalId = id;
      this.appendLines([`CONFIRM REMOVE ${id.toUpperCase()} / TYPE 'REMOVE ${id}' AGAIN`]);
      return;
    }
    await this.repository.removeProjectRecoverably(id);
    if (this.activeProject?.id === id) this.activeProject = undefined;
    this.pendingRemovalId = undefined;
    this.appendLines([`REMOVED ${id} TO CART BAY BIN`]);
  }

  private openImporter(): void {
    this.root.innerHTML = `
      <section class="display cartridge-import" data-view="import" aria-label="MOD-01 cartridge import">
        <header class="system-bar"><span>CARTRIDGE IMPORT</span><span>M01C/1</span></header>
        <main>
          <p>SELECT A SOURCE-INSPECTABLE .M01C OR .M01C.PNG.</p>
          <p>AN EXISTING ID IS REPLACED WITH A RECOVERY SNAPSHOT.</p>
          <label class="import-pick">OPEN <input type="file" accept=".m01c,.m01c.png,application/x-mod01-cartridge,image/png"></label>
        </main>
        <p class="import-status" role="status" aria-live="polite">WAITING FOR CARTRIDGE</p>
        <footer class="tool-bar"><button type="button" data-back>ESC BACK</button></footer>
      </section>
    `;
    const section = requireElement(this.root, '[data-view="import"]');
    const input = requireElement(this.root, 'input[type="file"]') as HTMLInputElement;
    const back = (): void => {
      this.renderShell();
    };
    this.root.querySelector('[data-back]')?.addEventListener('click', back);
    section.addEventListener('keydown', (event) => {
      if ((event as KeyboardEvent).key === 'Escape') back();
    });
    input.addEventListener('change', () => {
      const file = input.files?.[0];
      if (file === undefined) return;
      void this.importCartridge(file).catch((error: unknown) => {
        const status = this.root.querySelector<HTMLElement>('.import-status');
        if (status !== null) {
          status.textContent = errorMessage(error);
          status.classList.add('error');
        }
      });
    });
    input.focus();
  }

  private async importCartridge(file: File): Promise<void> {
    if (file.size > 8 * 1024 * 1024)
      throw new RangeError('CARTRIDGE FILE EXCEEDS ITS FORMAT CAPACITY');
    const imported = new Uint8Array(await file.arrayBuffer());
    const isPng =
      imported.length >= 8 &&
      [137, 80, 78, 71, 13, 10, 26, 10].every((byte, index) => imported[index] === byte);
    if (!isPng && file.size > HARDWARE.cartridgeCapacityBytes)
      throw new RangeError('CARTRIDGE FILE EXCEEDS ITS FORMAT CAPACITY');
    const cartridge = isPng ? decodeCartridgePng(imported).cartridge : imported;
    const unpacked = await this.compiler.unpackCartridge(cartridge);
    const manifest = await this.compiler.parseManifest(unpacked.manifest);
    const workcart = await this.compiler.unpackWorkcart(cartridge).catch(() => undefined);
    const project = await this.repository.saveProject({
      id: manifest.id,
      title: manifest.title,
      manifest: unpacked.manifest,
      files: Object.fromEntries(
        Object.entries(unpacked.files).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
      ),
      ...(workcart === undefined ? {} : { workcart }),
    });
    this.activeProject = fromStored(project);
    await this.repository.setShelfOrigin(manifest.id, 'imported');
    this.appendLines([
      `IMPORTED ${manifest.id} R${String(project.revision)}`,
      `${String(Object.keys(project.files).length)} SOURCE/ASSET FILES`,
    ]);
    this.renderShell();
  }

  private async importUrlFragment(): Promise<void> {
    try {
      const cartridge = decodeCartridgeFragment(globalThis.location.hash);
      if (cartridge === undefined) return;
      const unpacked = await this.compiler.unpackCartridge(cartridge);
      const manifest = await this.compiler.parseManifest(unpacked.manifest);
      const workcart = await this.compiler.unpackWorkcart(cartridge).catch(() => undefined);
      const project = await this.repository.saveProject({
        id: manifest.id,
        title: manifest.title,
        manifest: unpacked.manifest,
        files: Object.fromEntries(
          Object.entries(unpacked.files).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
        ),
        ...(workcart === undefined ? {} : { workcart }),
      });
      this.activeProject = fromStored(project);
      await this.repository.setShelfOrigin(manifest.id, 'fragment');
      this.appendLines([`FRAGMENT IMPORT ${manifest.id} / ${String(cartridge.byteLength)} BYTES`]);
      history.replaceState(null, '', `${location.pathname}${location.search}`);
    } catch (error: unknown) {
      this.appendLines([`!FRAGMENT ${errorMessage(error)}`]);
    }
  }

  private async openProjectFolder(): Promise<void> {
    if (window.showDirectoryPicker === undefined)
      throw new Error('FOLDER API UNAVAILABLE / USE IMPORT + DOWNLOAD');
    const handle = await window.showDirectoryPicker({ mode: 'readwrite' });
    const loaded = await this.readProjectFolder(handle);
    const stored = await this.repository.saveProject(loaded.project);
    this.activeProject = fromStored(stored);
    this.folderBindings.set(stored.id, {
      handle,
      modified: loaded.modified,
      ...(loaded.workcartPath === undefined ? {} : { workcartPath: loaded.workcartPath }),
    });
    await this.repository.setShelfOrigin(stored.id, 'imported');
    this.appendLines([
      `FOLDER OPEN ${stored.id} / R${String(stored.revision)}`,
      'F6 PULLS EXTERNAL CHANGES / F7 EXPLICITLY OVERWRITES A CONFLICT',
    ]);
  }

  private async readProjectFolder(handle: FileSystemDirectoryHandle): Promise<{
    project: WorkingProject;
    modified: Map<string, number>;
    workcartPath?: string;
  }> {
    const modified = new Map<string, number>();
    const manifestFile = await readFolderFile(handle, 'cart.toml', true);
    if (manifestFile === undefined) {
      const workcart = await readFolderWorkcart(handle);
      if (workcart === undefined) throw new Error('FOLDER HAS NO CART.TOML OR .M01W');
      const source = decoder.decode(workcart.file.bytes);
      const view = await this.compiler.workcartProjectView(source);
      const parsed = await this.compiler.parseManifest(view.manifest);
      modified.set(workcart.path, workcart.file.lastModified);
      return {
        project: {
          id: parsed.id,
          title: parsed.title,
          manifest: view.manifest,
          revision: (await this.repository.loadProject(parsed.id))?.revision ?? 0,
          files: Object.fromEntries(
            Object.entries(view.files).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
          ),
          workcart: source,
        },
        modified,
        workcartPath: workcart.path,
      };
    }
    modified.set('cart.toml', manifestFile.lastModified);
    const manifest = decoder.decode(manifestFile.bytes);
    const parsed = await this.compiler.parseManifest(manifest);
    const files: Record<string, Uint8Array> = {};
    await collectFolderSources(handle, 'src', files, modified, 0);
    const paths = new Set<string>([
      parsed.entry,
      ...Object.values(parsed.assets).map((asset) => asset.path),
      ...[parsed.label, parsed.thumbnail, parsed.display].filter(
        (path): path is string => path !== null,
      ),
    ]);
    for (const path of paths) {
      if (files[path] !== undefined) continue;
      const file = await readFolderFile(handle, path);
      if (file === undefined) throw new Error(`FOLDER FILE ${path} IS MISSING`);
      files[path] = file.bytes;
      modified.set(path, file.lastModified);
    }
    const identity = await readFolderFile(handle, 'presentation/cartridge.json', true);
    if (identity !== undefined) {
      files['presentation/cartridge.json'] = identity.bytes;
      modified.set('presentation/cartridge.json', identity.lastModified);
    }
    const total = Object.values(files).reduce((sum, bytes) => sum + bytes.byteLength, 0);
    if (Object.keys(files).length > 4096 || total > 2 * 1024 * 1024)
      throw new RangeError('FOLDER PROJECT EXCEEDS FILE OR BYTE LIMIT');
    return {
      project: {
        id: parsed.id,
        title: parsed.title,
        manifest,
        revision: (await this.repository.loadProject(parsed.id))?.revision ?? 0,
        files,
      },
      modified,
    };
  }

  private async syncProjectFolder(project: WorkingProject, overwrite: boolean): Promise<void> {
    const binding = this.folderBindings.get(project.id);
    if (binding === undefined) return;
    const outputs = new Map<string, Uint8Array>();
    if (binding.workcartPath === undefined) {
      outputs.set('cart.toml', encoder.encode(project.manifest));
      for (const [path, bytes] of Object.entries(project.files)) outputs.set(path, bytes);
    } else {
      if (project.workcart === undefined) throw new Error('M01W FOLDER PROJECT LOST ITS SOURCE');
      outputs.set(binding.workcartPath, encoder.encode(project.workcart));
    }
    if (!overwrite) {
      for (const path of outputs.keys()) {
        const current = await readFolderFile(binding.handle, path, true);
        const expected = binding.modified.get(path);
        if (
          (current === undefined && expected !== undefined) ||
          (current !== undefined && current.lastModified !== expected)
        )
          throw new Error(`FOLDER CONFLICT ${path} / F6 PULL OR F7 PUSH`);
      }
    }
    const modified = new Map(binding.modified);
    for (const [path, bytes] of outputs) {
      const file = await writeFolderFile(binding.handle, path, bytes);
      modified.set(path, file.lastModified);
    }
    binding.modified = modified;
  }

  private async synchronizeWorkcart(project: WorkingProject): Promise<void> {
    if (project.workcart === undefined) return;
    project.workcart = await this.compiler.rewriteWorkcart(
      project.workcart,
      project.manifest,
      project.files,
    );
  }

  private async compileAuthoredProject(
    project: WorkingProject,
    debug: boolean,
  ): Promise<CompilationResult> {
    await this.synchronizeWorkcart(project);
    return project.workcart === undefined
      ? this.compiler.compileProject(project.manifest, project.files, debug)
      : this.compiler.compileWorkcart(project.workcart, debug);
  }

  private async packAuthoredProject(project: WorkingProject): Promise<Uint8Array> {
    await this.synchronizeWorkcart(project);
    return project.workcart === undefined
      ? this.compiler.packProject(project.manifest, project.files)
      : this.compiler.packWorkcart(project.workcart);
  }

  private async exportAuthoredProject(project: WorkingProject): Promise<string> {
    await this.synchronizeWorkcart(project);
    return project.workcart === undefined
      ? this.compiler.exportHtml(project.manifest, project.files)
      : this.compiler.exportHtmlWorkcart(project.workcart);
  }

  private async saveProject(): Promise<void> {
    const project = this.requireProject();
    await this.synchronizeWorkcart(project);
    await this.syncProjectFolder(project, false);
    const stored = await this.repository.saveProject(project);
    Object.assign(project, fromStored(stored));
    this.activeProject = project;
    this.appendLines([`SAVED ${project.id} R${String(stored.revision)}`]);
  }

  private async recoverProject(revisionText: string | undefined): Promise<void> {
    const project = this.requireProject();
    const revisions = await this.repository.recoverySnapshots(project.id);
    if (revisions.length === 0) {
      this.appendLines(['NO RECOVERY REVISIONS']);
      return;
    }
    if (revisionText === undefined) {
      this.appendLines(
        revisions.map((revision) => `R${String(revision.revision)} ${revision.title}`),
      );
      return;
    }
    const requested = Number(revisionText.replace(/^r/i, ''));
    const recovered = revisions.find((revision) => revision.revision === requested);
    if (recovered === undefined) {
      throw new Error(`RECOVERY R${String(requested)} NOT FOUND`);
    }
    this.activeProject = fromStored(recovered);
    this.appendLines([`RECOVERED R${String(recovered.revision)} IN MEMORY`, 'USE SAVE TO COMMIT']);
  }

  private openEditor(path?: string): void {
    const project = this.requireProject();
    const selectedPath = path ?? manifestEntry(project.manifest);
    const bytes = project.files[selectedPath];
    if (bytes === undefined || !selectedPath.endsWith('.modl')) {
      throw new Error(`SOURCE '${selectedPath}' NOT FOUND`);
    }
    this.root.innerHTML = `
      <section class="display editor" data-view="editor" aria-label="MODL source editor">
        <header class="system-bar"><span>CODE</span><span class="editor-file"></span></header>
        <pre class="source-highlight" aria-hidden="true"></pre>
        <textarea class="source-input" spellcheck="false" aria-label="MODL source"></textarea>
        <div class="diagnostic-strip" role="status" aria-live="polite">CHECKING...</div>
        <footer class="tool-bar">
          <button type="button" data-action="back">ESC BACK</button>
          <button type="button" data-action="help">F1 HELP</button>
          <button type="button" data-action="format">F2 FORMAT</button>
          <button type="button" data-action="save">F3 SAVE</button>
          <button type="button" data-action="symbol">F4 SYMBOL</button>
          <button type="button" data-action="run">F5 RUN</button>
          <button type="button" data-action="reload">F6 RELOAD</button>
          <button type="button" data-action="overwrite">F7 PUSH</button>
        </footer>
      </section>
    `;
    requireElement(this.root, '.editor-file').textContent = selectedPath;
    const textarea = requireElement(this.root, '.source-input') as HTMLTextAreaElement;
    const highlight = requireElement(this.root, '.source-highlight') as HTMLElement;
    textarea.value = decoder.decode(bytes);
    renderHighlight(highlight, textarea.value);
    let analysisTimer: ReturnType<typeof setTimeout> | undefined;
    let autosaveTimer: ReturnType<typeof setTimeout> | undefined;
    let saveQueue = Promise.resolve();
    let dirty = false;
    const updateWorkingCopy = (): void => {
      project.files[selectedPath] = encoder.encode(textarea.value);
      this.activeProject = project;
    };
    const persistWorkingCopy = (): Promise<void> => {
      updateWorkingCopy();
      if (!dirty) {
        return saveQueue.catch(() => undefined);
      }
      dirty = false;
      saveQueue = saveQueue
        .catch(() => undefined)
        .then(async () => {
          await this.synchronizeWorkcart(project);
          await this.syncProjectFolder(project, false);
          const stored = await this.repository.loadProject(project.id);
          if (stored !== undefined && stored.revision !== project.revision) {
            dirty = true;
            throw new Error(`R${String(stored.revision)} CHANGED EXTERNALLY / F6 RELOAD`);
          }
          const saved = await this.repository.saveProject(project);
          Object.assign(project, fromStored(saved));
          this.activeProject = project;
          if (this.root.contains(textarea)) {
            this.setDiagnostic(`AUTOSAVED R${String(saved.revision)}`, false);
          }
        });
      return saveQueue;
    };
    const scheduleAutosave = (): void => {
      if (autosaveTimer !== undefined) {
        clearTimeout(autosaveTimer);
      }
      autosaveTimer = setTimeout(() => {
        void persistWorkingCopy().catch((error: unknown) => {
          if (this.root.contains(textarea)) {
            this.setDiagnostic(errorMessage(error), true);
          }
        });
      }, 750);
    };
    const analyze = (): void => {
      updateWorkingCopy();
      void this.showDiagnostics(selectedPath, textarea.value);
    };
    textarea.addEventListener('input', () => {
      dirty = true;
      renderHighlight(highlight, textarea.value);
      if (analysisTimer !== undefined) {
        clearTimeout(analysisTimer);
      }
      analysisTimer = setTimeout(analyze, 120);
      scheduleAutosave();
    });
    textarea.addEventListener('scroll', () => {
      highlight.scrollTop = textarea.scrollTop;
      highlight.scrollLeft = textarea.scrollLeft;
    });
    textarea.addEventListener('focus', () => {
      void this.repository.loadProject(project.id).then((stored) => {
        if (stored !== undefined && stored.revision > project.revision) {
          this.setDiagnostic(`R${String(stored.revision)} AVAILABLE / F6 RELOAD`, true);
        }
      });
    });
    const editor = requireElement(this.root, '.editor');
    editor.addEventListener(
      'click',
      (event) => {
        const action = (event.target as HTMLElement).closest<HTMLElement>('[data-action]')?.dataset
          .action;
        if (action === undefined) {
          return;
        }
        updateWorkingCopy();
        if (action === 'back') {
          if (autosaveTimer !== undefined) {
            clearTimeout(autosaveTimer);
          }
          void persistWorkingCopy()
            .catch((error: unknown) => {
              this.appendLines([`!${errorMessage(error)}`]);
            })
            .finally(() => {
              this.renderShell();
            });
        } else if (action === 'format') {
          void this.compiler
            .format(selectedPath, textarea.value)
            .then((formatted) => {
              textarea.value = formatted;
              dirty = true;
              analyze();
              scheduleAutosave();
            })
            .catch((error: unknown) => {
              this.setDiagnostic(errorMessage(error), true);
            });
        } else if (action === 'save') {
          if (autosaveTimer !== undefined) {
            clearTimeout(autosaveTimer);
          }
          void persistWorkingCopy()
            .then(() => {
              this.setDiagnostic('SAVED', false);
            })
            .catch((error: unknown) => {
              this.setDiagnostic(errorMessage(error), true);
            });
        } else if (action === 'run') {
          void this.runProject().catch((error: unknown) => {
            this.setDiagnostic(errorMessage(error), true);
          });
        } else if (action === 'symbol') {
          gotoDefinition(textarea);
        } else if (action === 'reload') {
          if (autosaveTimer !== undefined) {
            clearTimeout(autosaveTimer);
          }
          void this.reloadEditorProject(project, selectedPath, textarea, highlight).then(() => {
            dirty = false;
          });
        } else if (action === 'help') {
          this.openManual(manualTitleForSymbol(symbolAtCursor(textarea)));
        } else if (action === 'overwrite') {
          updateWorkingCopy();
          void this.synchronizeWorkcart(project)
            .then(() => this.syncProjectFolder(project, true))
            .then(() => this.repository.saveProject(project))
            .then((stored) => {
              Object.assign(project, fromStored(stored));
              dirty = false;
              this.setDiagnostic(`FOLDER PUSHED / R${String(stored.revision)}`, false);
            })
            .catch((error: unknown) => {
              this.setDiagnostic(errorMessage(error), true);
            });
        }
      },
      { once: false },
    );
    textarea.addEventListener('keydown', (event) => {
      const action =
        event.key === 'Escape'
          ? 'back'
          : event.key === 'F1'
            ? 'help'
            : event.key === 'F2'
              ? 'format'
              : event.key === 'F3'
                ? 'save'
                : event.key === 'F5'
                  ? 'run'
                  : event.key === 'F4'
                    ? 'symbol'
                    : event.key === 'F6'
                      ? 'reload'
                      : event.key === 'F7'
                        ? 'overwrite'
                        : undefined;
      if (event.ctrlKey && event.code === 'Space') {
        event.preventDefault();
        completeAtCursor(textarea);
        dirty = true;
        renderHighlight(highlight, textarea.value);
        analyze();
        scheduleAutosave();
        return;
      }
      if (action !== undefined) {
        event.preventDefault();
        (requireElement(this.root, `[data-action="${action}"]`) as HTMLButtonElement).click();
      }
    });
    void this.showDiagnostics(selectedPath, textarea.value);
    textarea.focus();
  }

  private async reloadEditorProject(
    project: WorkingProject,
    path: string,
    textarea: HTMLTextAreaElement,
    highlight: HTMLElement,
  ): Promise<void> {
    const binding = this.folderBindings.get(project.id);
    if (binding !== undefined) {
      const external = await this.readProjectFolder(binding.handle);
      Object.assign(project, external.project);
      this.activeProject = project;
      binding.modified = external.modified;
      if (external.workcartPath === undefined) delete binding.workcartPath;
      else binding.workcartPath = external.workcartPath;
      const source = project.files[path];
      if (source === undefined) throw new Error('SOURCE REMOVED FROM FOLDER');
      textarea.value = decoder.decode(source);
      renderHighlight(highlight, textarea.value);
      await this.showDiagnostics(path, textarea.value);
      return;
    }
    const stored = await this.repository.loadProject(project.id);
    const source = stored?.files[path];
    if (stored === undefined || source === undefined) {
      this.setDiagnostic('NO EXTERNAL REVISION AVAILABLE', true);
      return;
    }
    Object.assign(project, fromStored(stored));
    this.activeProject = project;
    textarea.value = decoder.decode(source);
    renderHighlight(highlight, textarea.value);
    await this.showDiagnostics(path, textarea.value);
  }

  private async openTool(tool: CreationTool): Promise<void> {
    const project = this.requireProject();
    await openCreationTool(this.root, tool, project, {
      back: () => {
        this.renderShell();
      },
      save: async () => {
        const stored = await this.repository.loadProject(project.id);
        if (stored !== undefined && stored.revision !== project.revision)
          throw new Error(`R${String(stored.revision)} CHANGED EXTERNALLY / REOPEN TOOL`);
        await this.saveProject();
      },
      parseManifest: async () => this.compiler.parseManifest(project.manifest),
    });
  }

  private async openWorkbench(): Promise<void> {
    const project = this.requireProject();
    await openAssetWorkbench(this.root, project, {
      back: () => {
        this.renderShell();
      },
      save: async () => {
        const stored = await this.repository.loadProject(project.id);
        if (stored !== undefined && stored.revision !== project.revision)
          throw new Error(`R${String(stored.revision)} CHANGED EXTERNALLY / REOPEN ASSETS`);
        await this.saveProject();
      },
      parseManifest: async () => this.compiler.parseManifest(project.manifest),
      capacity: async () => (await this.packAuthoredProject(project)).byteLength,
      recipes: async () => {
        if (project.workcart === undefined) return [];
        return workcartRecipes((await this.compiler.parseWorkcart(project.workcart)).recipes);
      },
      generateRecipe: async (recipeId) => {
        if (project.workcart === undefined) throw new Error('GENERATORS REQUIRE AN M01W PROJECT');
        project.workcart = await this.compiler.materializeWorkcartRecipe(
          project.workcart,
          recipeId,
        );
        const view = await this.compiler.workcartProjectView(project.workcart);
        project.manifest = view.manifest;
        project.files = Object.fromEntries(
          Object.entries(view.files).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
        );
        await this.saveProject();
      },
      deleteAsset: async (assetName) => {
        if (project.workcart === undefined) throw new Error('DELETE REQUIRES AN M01W PROJECT');
        const stored = await this.repository.loadProject(project.id);
        if (stored !== undefined && stored.revision !== project.revision)
          throw new Error(`R${String(stored.revision)} CHANGED EXTERNALLY / REOPEN ASSETS`);
        project.workcart = await this.compiler.deleteWorkcartAsset(project.workcart, assetName);
        const view = await this.compiler.workcartProjectView(project.workcart);
        project.manifest = view.manifest;
        project.files = Object.fromEntries(
          Object.entries(view.files).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
        );
        await this.saveProject();
      },
    });
  }

  private openManual(initialQuery = 'START'): void {
    const topics = manualTopics();
    this.root.innerHTML = `
      <section class="display manual" data-view="manual" aria-label="MOD-01 manual browser">
        <header class="system-bar"><span>MODL/1 MANUAL</span><span>ROM 1.0</span></header>
        <input class="manual-search" type="search" aria-label="Search manual" placeholder="SEARCH">
        <nav class="manual-topics" aria-label="Manual topics"></nav>
        <article class="manual-page" tabindex="0"></article>
        <footer class="tool-bar"><button type="button" data-back>ESC BACK</button></footer>
      </section>
    `;
    const search = requireElement(this.root, '.manual-search') as HTMLInputElement;
    const navigation = requireElement(this.root, '.manual-topics');
    const page = requireElement(this.root, '.manual-page');
    const show = (topic: (typeof topics)[number]): void => {
      page.replaceChildren();
      const heading = document.createElement('h1');
      heading.textContent = topic.title;
      const body = document.createElement('p');
      body.textContent = topic.body;
      page.append(heading, body);
    };
    const renderTopics = (): void => {
      const query = search.value.toLowerCase();
      const visible = topics.filter((topic) =>
        `${topic.title} ${topic.body}`.toLowerCase().includes(query),
      );
      navigation.replaceChildren(
        ...visible.map((topic) => {
          const button = document.createElement('button');
          button.type = 'button';
          button.textContent = topic.title;
          button.addEventListener('click', () => {
            show(topic);
          });
          return button;
        }),
      );
      const first = visible[0];
      if (first !== undefined) show(first);
    };
    search.addEventListener('input', renderTopics);
    const back = (): void => {
      this.renderShell();
    };
    this.root.querySelector('[data-back]')?.addEventListener('click', back);
    this.root.querySelector('[data-view="manual"]')?.addEventListener('keydown', (event) => {
      if ((event as KeyboardEvent).key === 'Escape') back();
    });
    renderTopics();
    const initial = topics.find((topic) => topic.title === initialQuery.toUpperCase());
    if (initial !== undefined) show(initial);
    else {
      search.value = initialQuery;
      renderTopics();
    }
    search.focus();
  }

  private async openExplorer(): Promise<void> {
    const project = this.requireProject();
    const [compilation, manifest, packed] = await Promise.all([
      this.compileAuthoredProject(project, false),
      this.compiler.parseManifest(project.manifest),
      this.packAuthoredProject(project),
    ]);
    const cartridge = await this.compiler.decodeCartridge(packed);
    const report = projectSizeReport(packed, cartridge.entries, manifest, compilation);
    const graph = deriveCodeStateGraph(compilation);
    const panes: Readonly<Record<string, unknown>> = {
      GRAPH: graph,
      TOKENS: compilation.analysis.tokens,
      AST: compilation.analysis.module,
      TYPED: compilation.analysis.symbols,
      IR: compilation.analysis.ir,
      JS: compilation.generated?.javascript ?? 'NO GENERATED PROGRAM',
      MAP: compilation.generated?.source_map_json ?? 'NO SOURCE MAP',
      DIAG: compilation.analysis.diagnostics,
      SIZE: {
        ...report,
        probes: compilation.generated?.probe_count ?? 0,
        workModel: compilation.generated?.work_model ?? {},
      },
    };
    this.root.innerHTML = `
      <section class="display explorer" data-view="explorer" aria-label="MODL compiler explorer">
        <header class="system-bar"><span>COMPILER EXPLORER</span><span>RELEASE</span></header>
        <nav class="explorer-tabs" aria-label="Compiler stages"></nav>
        <pre class="explorer-output" tabindex="0"></pre>
        <footer class="tool-bar"><button type="button" data-back>ESC BACK</button></footer>
      </section>
    `;
    const tabs = requireElement(this.root, '.explorer-tabs');
    const output = requireElement(this.root, '.explorer-output') as HTMLElement;
    const show = (name: string): void => {
      const value = panes[name];
      if (name === 'GRAPH') {
        renderCodeStateGraph(output, graph);
        return;
      }
      const source = typeof value === 'string' ? value : JSON.stringify(value, undefined, 2);
      if (name === 'JS') renderHighlight(output, source);
      else renderJsonHighlight(output, source);
    };
    for (const name of Object.keys(panes)) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = name;
      button.addEventListener('click', () => {
        show(name);
      });
      tabs.append(button);
    }
    const back = (): void => {
      this.renderShell();
    };
    this.root.querySelector('[data-back]')?.addEventListener('click', back);
    this.root.querySelector('[data-view="explorer"]')?.addEventListener('keydown', (event) => {
      if ((event as KeyboardEvent).key === 'Escape') back();
    });
    show('TOKENS');
    output.focus();
  }

  private async showDiagnostics(path: string, source: string): Promise<void> {
    try {
      const analysis = await this.compiler.analyze(path, source);
      const diagnostic = analysis.diagnostics[0];
      this.setDiagnostic(
        diagnostic === undefined ? 'OK / 0 ERRORS' : `${diagnostic.code} ${diagnostic.message}`,
        diagnostic !== undefined,
      );
    } catch (error: unknown) {
      this.setDiagnostic(errorMessage(error), true);
    }
  }

  private setDiagnostic(message: string, error: boolean): void {
    const strip = this.root.querySelector<HTMLElement>('.diagnostic-strip');
    if (strip !== null) {
      strip.textContent = message;
      strip.classList.toggle('error', error);
    }
  }

  private async runProject(replay?: ReplayTrace): Promise<void> {
    this.stopDebugger();
    const project = this.requireProject();
    const compilation = await this.compileAuthoredProject(project, false);
    const diagnostic = compilation.analysis.diagnostics[0];
    if (diagnostic !== undefined || compilation.generated === undefined) {
      this.reportCompilerDiagnostic(diagnostic);
      return;
    }
    const parsedManifest = await this.compiler.parseManifest(project.manifest);
    const rom = await this.packAuthoredProject(project);
    const saveAccess = this.repository.cartridgeSave(project.id);
    const save = await saveAccess.read();
    const settings = await this.repository.settings();
    this.root.innerHTML = `
      <section class="display player" data-view="player"${replay === undefined ? '' : ' data-replay="true"'} aria-label="Running MOD-01 cartridge">
        <canvas class="player-screen" width="240" height="144" tabindex="0" aria-label="Cartridge display"></canvas>
        <div class="capture-player"><button class="capture-scale" type="button" aria-label="Capture scale">SCALE 1X</button><button class="capture-shot" type="button">PNG</button><button class="capture-gif" type="button">GIF 5S</button><button class="capture-replay" type="button">M01REC OUT</button><label class="file-button">M01REC IN<input class="replay-input" type="file" accept=".m01rec,application/json"></label></div>
        <div class="player-controls"><button class="stop-player" type="button" aria-label="Stop cartridge (Shift+Escape)">STOP</button><button class="enable-player-audio" type="button" aria-label="Enable sound">SOUND</button></div>
        <p class="player-budget">${String(rom.byteLength)}B/${sizeClass(rom.byteLength)} D0000 V0</p>
        <p class="player-status" role="status"></p>
      </section>
    `;
    const canvas = requireElement(this.root, '.player-screen') as HTMLCanvasElement;
    const status = requireElement(this.root, '.player-status') as HTMLElement;
    const budget = requireElement(this.root, '.player-budget') as HTMLElement;
    const worker = new InlineSandboxWorker({
      name: `mod01-${project.id}`,
    });
    const sandbox = new SandboxSession(worker, 1_000);
    const input = new BrowserInput(canvas, undefined, settings.controllerProfile);
    const renderer = new WebGlIndexedRenderer(canvas);
    let audioSink: WebAudioSink | undefined;
    let captureScale = 1;
    const capturedFrames: Uint8Array[] = [];
    const capturedInputs: { frame: number; input: InputFrame }[] = [];
    const importedInputs = replay === undefined ? undefined : replayInputFrames(replay);
    let frameCursor = 0;
    try {
      await sandbox.load(compilation.generated.javascript, {
        seed: 0x4d30_3031,
        workUnitsPerFrame: HARDWARE.workUnitsPerFrame,
        updateRate: manifestUpdateRate(project.manifest),
        hardwareRevision: parsedManifest.hardware_revision ?? 1,
        assets: {
          declarations: parsedManifest.assets,
          files: project.files,
          displayPath: parsedManifest.display,
        },
        save,
        rom,
      });
      await this.repository.markPlayed(project.id);
    } catch (error) {
      input.destroy();
      sandbox.dispose();
      this.renderShell();
      throw error;
    }
    let stopped = false;
    const stopKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape' && event.shiftKey) {
        event.preventDefault();
        stop();
      }
    };
    const stop = (): void => {
      if (stopped) {
        return;
      }
      stopped = true;
      input.destroy();
      sandbox.dispose();
      if (audioSink !== undefined) {
        void audioSink.close();
      }
      globalThis.removeEventListener('keydown', stopKey, true);
      this.player = undefined;
      this.appendLines([`STOPPED ${project.id}`]);
      this.renderShell();
    };
    this.player = { sandbox, input, stop };
    (requireElement(this.root, '.stop-player') as HTMLButtonElement).addEventListener(
      'click',
      stop,
    );
    (requireElement(this.root, '.enable-player-audio') as HTMLButtonElement).addEventListener(
      'click',
      (event) => {
        const button = event.currentTarget as HTMLButtonElement;
        audioSink = new WebAudioSink(undefined, settings.audioVolume);
        void audioSink.resume().then(() => {
          button.textContent = 'SOUND ON';
          button.disabled = true;
        });
      },
      { once: true },
    );
    (requireElement(this.root, '.capture-shot') as HTMLButtonElement).addEventListener(
      'click',
      () => {
        const last = capturedFrames.at(-1);
        if (last === undefined) return;
        const image = scaleRgba(indexedFrame(last), captureScale);
        downloadBytes(
          `${project.id}-${String(capturedInputs.at(-1)?.frame ?? 0).padStart(5, '0')}@${String(captureScale)}x.png`,
          encodeRgbaPng(image.width, image.height, image.rgba),
          'image/png',
        );
      },
    );
    (requireElement(this.root, '.capture-scale') as HTMLButtonElement).addEventListener(
      'click',
      (event) => {
        captureScale = captureScale === 4 ? 1 : captureScale + 1;
        (event.currentTarget as HTMLButtonElement).textContent = `SCALE ${String(captureScale)}X`;
      },
    );
    (requireElement(this.root, '.capture-gif') as HTMLButtonElement).addEventListener(
      'click',
      () => {
        status.textContent = 'GIF ENCODING 30FPS';
        const sampled = capturedFrames.filter(
          (_frame, index) => index % 2 === capturedFrames.length % 2,
        );
        if (sampled.length === 0) return;
        downloadBytes(`${project.id}.gif`, encodeIndexedGif(sampled.slice(-150)), 'image/gif');
        status.textContent = `GIF SAVED ${String(Math.min(150, sampled.length))}F`;
      },
    );
    (requireElement(this.root, '.capture-replay') as HTMLButtonElement).addEventListener(
      'click',
      () => {
        downloadBytes(
          `${project.id}.m01rec`,
          encodeReplayTrace(capturedInputs),
          'application/json',
        );
      },
    );
    (requireElement(this.root, '.replay-input') as HTMLInputElement).addEventListener(
      'change',
      (event) => {
        const file = (event.currentTarget as HTMLInputElement).files?.[0];
        if (file === undefined) return;
        void (async () => {
          if (file.size > REPLAY_MAX_BYTES) throw new RangeError('replay byte length is invalid');
          const trace = decodeReplayTrace(new Uint8Array(await file.arrayBuffer()));
          stop();
          await this.runProject(trace);
          const replayStatus = this.root.querySelector<HTMLElement>('.player-status');
          if (replayStatus !== null)
            replayStatus.textContent = `M01REC ${String(trace.frames.length)}F`;
        })().catch((error: unknown) => {
          status.textContent = errorMessage(error);
          status.classList.add('error');
        });
      },
    );
    globalThis.addEventListener('keydown', stopKey, true);
    const frame = async (): Promise<void> => {
      if (stopped) {
        return;
      }
      try {
        const inputFrame =
          importedInputs?.get(frameCursor) ??
          (importedInputs === undefined ? input.poll() : emptyInputFrame());
        const result = await sandbox.frame(inputFrame);
        this.capturedFrames.set(project.id, result.output.indexedPixels.slice());
        capturedFrames.push(result.output.indexedPixels.slice());
        capturedInputs.push({ frame: result.frame, input: inputFrame });
        frameCursor += 1;
        if (capturedFrames.length > 300) capturedFrames.shift();
        if (capturedInputs.length > 36_000) capturedInputs.shift();
        renderer.render(result.output.indexedPixels);
        audioSink?.enqueue(result.output.audio);
        if (result.saveCommit !== undefined) await saveAccess.write(result.saveCommit);
        status.textContent = `F${String(result.frame).padStart(5, '0')} W${String(result.workUnits).padStart(5, '0')}`;
        budget.textContent = `${String(rom.byteLength)}B/${sizeClass(rom.byteLength)} D${String(result.drawCommands.length).padStart(4, '0')} V${String(result.output.audio.activeVoices)}`;
        requestAnimationFrame(() => void frame());
      } catch (error: unknown) {
        status.textContent = errorMessage(error);
        status.classList.add('error');
      }
    };
    canvas.focus();
    requestAnimationFrame(() => void frame());
  }

  private async debugProject(): Promise<void> {
    this.stopPlayer();
    const project = this.requireProject();
    await this.synchronizeWorkcart(project);
    const save = await this.repository.cartridgeSave(project.id).read();
    const settings = await this.repository.settings();
    try {
      this.activeDebugger = await openDebugger(
        this.root,
        project,
        this.compiler,
        save,
        () => {
          this.activeDebugger = undefined;
          this.appendLines([`DEBUG STOPPED ${project.id}`]);
          this.renderShell();
        },
        () => {
          this.activeDebugger = undefined;
          this.openManual('HARDWARE');
        },
        settings.controllerProfile,
        settings.audioVolume,
      );
    } catch (error: unknown) {
      this.renderShell();
      throw error;
    }
  }

  private async packProject(): Promise<void> {
    const project = this.requireProject();
    const bytes = await this.packAuthoredProject(project);
    const buffer = new ArrayBuffer(bytes.byteLength);
    new Uint8Array(buffer).set(bytes);
    const url = URL.createObjectURL(new Blob([buffer], { type: 'application/x-mod01-cartridge' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = `${project.id}.m01c`;
    link.click();
    URL.revokeObjectURL(url);
    this.appendLines([`PACKED ${project.id}.m01c ${String(bytes.byteLength)} BYTES`]);
  }

  private async exportCartridgePng(): Promise<void> {
    const project = this.requireProject();
    const manifest = await this.compiler.parseManifest(project.manifest);
    const cartridge = await this.packAuthoredProject(project);
    const identity = decodeIdentity(project.files['presentation/cartridge.json']);
    const frame = this.capturedFrames.get(project.id);
    const png = encodeCartridgePng(
      cartridge,
      {
        title: manifest.title,
        author: manifest.author,
        year: identity.year,
        players: identity.players,
        controls: identity.controls,
      },
      frame === undefined ? undefined : indexedFrame(frame),
    );
    downloadBytes(`${project.id}.m01c.png`, png, 'image/png');
    this.appendLines([
      `CART IMAGE ${project.id}.m01c.png ${String(png.byteLength)} BYTES`,
      frame === undefined
        ? 'LABEL USED FACTORY SCREEN / RUN TO CAPTURE FRAME'
        : 'LABEL CAPTURED LAST RUN FRAME',
    ]);
  }

  private async exportHtml(): Promise<void> {
    const project = this.requireProject();
    const html = await this.exportAuthoredProject(project);
    const url = URL.createObjectURL(new Blob([html], { type: 'text/html;charset=utf-8' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = `${project.id}.html`;
    link.click();
    URL.revokeObjectURL(url);
    this.appendLines([
      `EXPORTED ${project.id}.html ${String(encoder.encode(html).byteLength)} BYTES`,
    ]);
  }

  private async exportZip(): Promise<void> {
    const project = this.requireProject();
    const html = await this.exportAuthoredProject(project);
    const zip = encodeSingleFileZip('index.html', encoder.encode(html));
    downloadBytes(`${project.id}-itch.zip`, zip, 'application/zip');
    this.appendLines([
      `EXPORTED ${project.id}-itch.zip ${String(zip.byteLength)} BYTES / INDEX.HTML`,
    ]);
  }

  private async openSaveManager(project: WorkingProject): Promise<void> {
    const access = this.repository.cartridgeSave(project.id);
    const [bytes, schema] = await Promise.all([access.read(), access.schemaVersion()]);
    this.root.innerHTML = `
      <section class="display save-manager" data-view="save-manager" aria-label="Cartridge save manager">
        <header class="system-bar"><span>SAVE SERVICE</span><span>${escapeHtml(project.id.toUpperCase())}</span></header>
        <main class="save-manager-body">
          <h1>LOCAL SAVE</h1>
          <p>SCHEMA ${String(schema)} / ${String(bytes.byteLength)} OF 8192 BYTES</p>
          <p>IDENTITY: ${escapeHtml(project.id)}</p>
          <p>CHECKSUM VERIFIED ON READ AND IMPORT. OLD RAW SAVES MIGRATE WITH A RETAINED BACKUP.</p>
          <p class="save-manager-status" role="status">READY</p>
        </main>
        <footer class="tool-bar"><button data-save="out">OUT</button><label class="file-button">IN<input data-save-input type="file" accept=".m01save,application/json"></label><button data-save="reset">RESET</button><button data-save="delete">DELETE</button><button data-save="back">BACK</button></footer>
      </section>
    `;
    const status = requireElement(this.root, '.save-manager-status');
    let confirm: 'reset' | 'delete' | undefined;
    const act = async (action: string): Promise<void> => {
      if (action === 'back') {
        this.renderShell();
      } else if (action === 'out') {
        const exported = await access.export();
        downloadBytes(`${project.id}.m01save`, exported, 'application/json');
        status.textContent = `EXPORTED ${String(exported.byteLength)} BYTES`;
      } else if (action === 'reset' || action === 'delete') {
        if (confirm !== action) {
          confirm = action;
          status.textContent = `CONFIRM ${action.toUpperCase()} / RECOVERY COPY RETAINED`;
          return;
        }
        if (action === 'reset') await access.reset(schema);
        else await access.clear();
        status.textContent = action === 'reset' ? 'RESET TO EMPTY SAVE' : 'SAVE DELETED';
        confirm = undefined;
      }
    };
    for (const button of this.root.querySelectorAll<HTMLButtonElement>('[data-save]'))
      button.addEventListener('click', () => {
        void act(button.dataset.save ?? '').catch((error: unknown) => {
          status.textContent = errorMessage(error);
        });
      });
    this.root
      .querySelector<HTMLInputElement>('[data-save-input]')
      ?.addEventListener('change', (event) => {
        const file = (event.currentTarget as HTMLInputElement).files?.[0];
        if (file === undefined) return;
        void file
          .arrayBuffer()
          .then((value) => access.import(new Uint8Array(value)))
          .then(() => {
            status.textContent = 'IMPORTED / CHECKSUM VERIFIED';
          })
          .catch((error: unknown) => {
            status.textContent = errorMessage(error);
          });
      });
  }

  private async duplicateShelfProject(project: WorkingProject): Promise<WorkingProject> {
    let suffix = 1;
    let id = `${project.id}.copy`;
    while (
      (await this.repository.loadProject(id)) !== undefined ||
      (await this.repository.isProjectRemoved(id))
    ) {
      suffix += 1;
      id = `${project.id.slice(0, Math.max(3, 58 - String(suffix).length))}.copy${String(suffix)}`;
    }
    const title = `${project.title} COPY`.slice(0, 64);
    const manifest = replaceManifestIdentity(project.manifest, id, title);
    const workcart =
      project.workcart === undefined
        ? undefined
        : await this.compiler.rewriteWorkcart(project.workcart, manifest, project.files);
    const stored = await this.repository.saveProject({
      id,
      title,
      manifest,
      files: project.files,
      ...(workcart === undefined ? {} : { workcart }),
    });
    await this.repository.setShelfOrigin(id, 'duplicate');
    return fromStored(stored);
  }

  private async renameCartridge(project: WorkingProject, requestedTitle: string): Promise<void> {
    const title = requestedTitle.trim().slice(0, 64);
    if (title.length === 0 || !/^[\x20-\x7e]+$/.test(title))
      throw new TypeError('TITLE MUST BE 1-64 ASCII CHARACTERS');
    const manifest = replaceManifestIdentity(project.manifest, project.id, title);
    const workcart =
      project.workcart === undefined
        ? undefined
        : await this.compiler.rewriteWorkcart(project.workcart, manifest, project.files);
    const stored = await this.repository.saveProject({
      ...project,
      title,
      manifest,
      ...(workcart === undefined ? {} : { workcart }),
    });
    this.activeProject = fromStored(stored);
    this.appendLines([`RENAMED ${project.id} / SAVE ID UNCHANGED`]);
  }

  private async openShare(): Promise<void> {
    const project = this.requireProject();
    const cartridge = await this.packAuthoredProject(project);
    const fragment = encodeCartridgeFragment(cartridge);
    const url = new URL(location.href);
    url.hash = fragment;
    this.root.innerHTML = `
      <section class="display share-view" data-view="share" aria-label="Tiny cartridge fragment share">
        <header class="system-bar"><span>TINY CART LINK</span><span>${String(cartridge.byteLength)}B</span></header>
        <main><p>FRAGMENT ONLY / NO UPLOAD</p><p>${String(fragment.length)} / 8192 CHARACTERS</p><textarea class="share-url" readonly aria-label="Cartridge share URL"></textarea><button class="share-copy" type="button">COPY LINK</button><p class="share-status" role="status">READY</p></main>
        <footer class="tool-bar"><button type="button" data-back>ESC BACK</button></footer>
      </section>
    `;
    const text = requireElement(this.root, '.share-url') as HTMLTextAreaElement;
    text.value = url.href;
    const back = (): void => {
      this.renderShell();
    };
    this.root.querySelector('[data-back]')?.addEventListener('click', back);
    this.root.querySelector('.share-copy')?.addEventListener('click', () => {
      text.select();
      void navigator.clipboard
        .writeText(text.value)
        .then(() => {
          const status = this.root.querySelector<HTMLElement>('.share-status');
          if (status !== null) status.textContent = 'COPIED';
        })
        .catch(() => {
          const status = this.root.querySelector<HTMLElement>('.share-status');
          if (status !== null) status.textContent = 'SELECTED / COPY MANUALLY';
        });
    });
    this.root.querySelector('[data-view="share"]')?.addEventListener('keydown', (event) => {
      if ((event as KeyboardEvent).key === 'Escape') back();
    });
    text.focus();
  }

  private async info(): Promise<void> {
    const identity = await this.compiler.identity();
    const project = this.activeProject;
    const detail =
      project === undefined
        ? undefined
        : await this.packAuthoredProject(project).then(async (packed) => {
            const [manifest, decoded, compilation] = await Promise.all([
              this.compiler.parseManifest(project.manifest),
              this.compiler.decodeCartridge(packed),
              this.compileAuthoredProject(project, false),
            ]);
            return projectSizeReport(packed, decoded.entries, manifest, compilation);
          });
    this.appendLines([
      `${identity.language} COMPILER ${identity.compiler}`,
      project === undefined
        ? 'NO CARTRIDGE LOADED'
        : `${project.id} / ${project.title} / R${String(project.revision)}`,
      '240X144 / 32 COLOR / 60HZ',
      ...(detail === undefined
        ? []
        : [
            `${detail.sizeClass} ${String(detail.canonicalCartridgeBytes)}B / SRC ${String(detail.sourceBytes)} / GEN ${String(detail.generatedReleaseBytes)}`,
            `VIS ${String(detail.visualBytes)} MAP ${String(detail.mapBytes)} FONT ${String(detail.fontBytes)} AUDIO ${String(detail.audioBytes)}`,
            `META ${String(detail.metadataBytes)} OVER ${String(detail.containerOverheadBytes)} GAIN ${String(detail.compressionGainBytes)}`,
            `SAVE 8192 / ${detail.warning}`,
          ]),
    ]);
  }

  private async openSettings(): Promise<void> {
    const settings = await this.repository.settings();
    let profile = structuredClone(settings.controllerProfile);
    const buttonOptions = BUTTONS.map(
      (button) => `<option value="${button}">${button.toUpperCase()}</option>`,
    ).join('');
    const gamepadOptions = (assigned: number | null): string =>
      [null, 0, 1, 2, 3]
        .map(
          (index) =>
            `<option value="${index === null ? '-1' : String(index)}"${assigned === index ? ' selected' : ''}>${index === null ? 'NONE' : `PAD ${String(index)}`}</option>`,
        )
        .join('');
    this.root.innerHTML = `
      <section class="display settings" data-view="settings" aria-label="Controller and accessibility settings">
        <header class="system-bar"><span>SETTINGS / CONTROLS</span><span>LOCAL PROFILE</span></header>
        <main class="settings-body">
          <label><input data-setting="reducedMotion" type="checkbox"${settings.reducedMotion ? ' checked' : ''}> REDUCED FLASHING</label>
          <label><input data-setting="mutedStartup" type="checkbox"${settings.mutedStartup ? ' checked' : ''}> MUTED STARTUP</label>
          <label><input data-setting="highContrast" type="checkbox"${settings.highContrast ? ' checked' : ''}> HIGH CONTRAST UI</label>
          <label><input data-setting="largeHelp" type="checkbox"${settings.largeHelp ? ' checked' : ''}> LARGE HELP TEXT</label>
          <label>VOLUME <input data-setting="audioVolume" type="range" min="0" max="1" step="0.05" value="${String(settings.audioVolume)}"></label>
          <div class="remap-row"><label>PORT <select data-remap-port>${[0, 1, 2, 3].map((port) => `<option value="${String(port)}">${String(port + 1)}</option>`).join('')}</select></label><label>BUTTON <select data-remap-button>${buttonOptions}</select></label><button data-remap-key type="button">PRESS KEY</button></div>
          <div class="gamepad-row">${profile.gamepads.map((gamepad, port) => `<label>P${String(port + 1)} <select data-gamepad="${String(port)}">${gamepadOptions(gamepad)}</select></label>`).join('')}</div>
          <p class="settings-status" role="status">KEYBOARD + STANDARD GAMEPAD / FOUR PORTS</p>
        </main>
        <footer class="tool-bar"><button data-settings="defaults">DEFAULT</button><button data-settings="save">SAVE</button><button data-settings="back">BACK</button></footer>
      </section>
    `;
    enhancePixelSelects(this.root);
    const status = requireElement(this.root, '.settings-status');
    const save = async (): Promise<void> => {
      const gamepads = [0, 1, 2, 3].map((port) => {
        const value = Number(
          this.root.querySelector<HTMLSelectElement>(`[data-gamepad="${String(port)}"]`)?.value,
        );
        return value < 0 ? null : value;
      }) as [number | null, number | null, number | null, number | null];
      const active = gamepads.filter((value): value is number => value !== null);
      if (new Set(active).size !== active.length)
        throw new Error('GAMEPAD CONFLICT / ONE PORT EACH');
      profile = { ...profile, gamepads };
      const checked = (name: string): boolean =>
        this.root.querySelector<HTMLInputElement>(`[data-setting="${name}"]`)?.checked ?? false;
      const next: StudioSettings = {
        ...settings,
        revision: 2,
        reducedMotion: checked('reducedMotion'),
        mutedStartup: checked('mutedStartup'),
        highContrast: checked('highContrast'),
        largeHelp: checked('largeHelp'),
        audioVolume: Number(
          this.root.querySelector<HTMLInputElement>('[data-setting="audioVolume"]')?.value ?? '0.8',
        ),
        controllerProfile: profile,
      };
      await this.repository.saveSettings(next);
      this.applySettings(next);
      status.textContent = 'PROFILE SAVED / RESTART RUN TO APPLY INPUT';
    };
    this.root.querySelector('[data-remap-key]')?.addEventListener('click', () => {
      status.textContent = 'PRESS A KEY / EXISTING CONFLICT WILL MOVE';
      globalThis.addEventListener(
        'keydown',
        (event) => {
          event.preventDefault();
          event.stopPropagation();
          const port = Number(
            this.root.querySelector<HTMLSelectElement>('[data-remap-port]')?.value ?? '0',
          );
          const button = (this.root.querySelector<HTMLSelectElement>('[data-remap-button]')
            ?.value ?? 'a') as Button;
          const previous = profile.keyboard[event.code];
          profile = remapControllerKey(profile, event.code, port, button);
          status.textContent = `${event.code.toUpperCase()} -> P${String(port + 1)} ${button.toUpperCase()}${previous === undefined ? '' : ' / PRIOR CONFLICT REASSIGNED'}`;
        },
        { capture: true, once: true },
      );
    });
    for (const button of this.root.querySelectorAll<HTMLButtonElement>('[data-settings]'))
      button.addEventListener('click', () => {
        const action = button.dataset.settings;
        if (action === 'back') this.renderShell();
        else if (action === 'defaults') {
          profile = defaultControllerProfile();
          status.textContent = 'DEFAULT PROFILE LOADED / SAVE TO APPLY';
        } else if (action === 'save') {
          void save().catch((error: unknown) => {
            status.textContent = errorMessage(error);
          });
        }
      });
  }

  private applySettings(settings: StudioSettings): void {
    document.documentElement.dataset.reducedMotion = String(settings.reducedMotion);
    document.documentElement.dataset.highContrast = String(settings.highContrast);
    document.documentElement.dataset.largeHelp = String(settings.largeHelp);
    document.documentElement.dataset.mutedStartup = String(settings.mutedStartup);
  }

  private async openInspector(): Promise<void> {
    const project = this.requireProject();
    const cartridge = await this.compiler.decodeCartridge(await this.packAuthoredProject(project));
    const sources = Object.entries(cartridge.entries)
      .filter(([path]) => path.startsWith('source/'))
      .sort(([left], [right]) => left.localeCompare(right));
    this.root.innerHTML = `
      <section class="display cartridge-inspector" data-view="inspector" aria-label="Packed cartridge inspector">
        <header class="system-bar"><span>M01C INSPECTOR</span><span>SOURCE VISIBLE</span></header>
        <nav class="inspector-files" aria-label="Cartridge contents"></nav>
        <pre class="inspector-output" tabindex="0"></pre>
        <footer class="tool-bar"><button type="button" data-back>ESC BACK</button></footer>
      </section>
    `;
    const navigation = requireElement(this.root, '.inspector-files');
    const output = requireElement(this.root, '.inspector-output') as HTMLElement;
    const panes = new Map<string, string>([
      ['MANIFEST', JSON.stringify(cartridge.manifest, undefined, 2)],
      ...sources.map(
        ([path, bytes]) =>
          [path.slice('source/'.length), decoder.decode(Uint8Array.from(bytes))] as [
            string,
            string,
          ],
      ),
    ]);
    const show = (name: string, contents: string): void => {
      if (name === 'MANIFEST') renderJsonHighlight(output, contents);
      else renderHighlight(output, contents);
    };
    for (const [name, contents] of panes) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = name;
      button.addEventListener('click', () => {
        show(name, contents);
      });
      navigation.append(button);
    }
    show('MANIFEST', panes.get('MANIFEST') ?? '');
    const back = (): void => {
      this.renderShell();
    };
    this.root.querySelector('[data-back]')?.addEventListener('click', back);
    this.root.querySelector('[data-view="inspector"]')?.addEventListener('keydown', (event) => {
      if ((event as KeyboardEvent).key === 'Escape') back();
    });
    output.focus();
  }

  private reportCompilerDiagnostic(diagnostic: CompilerDiagnostic | undefined): void {
    const message =
      diagnostic === undefined
        ? 'COMPILER PRODUCED NO PROGRAM'
        : `${diagnostic.code} ${diagnostic.message}`;
    if (this.root.querySelector('.editor') !== null) {
      this.setDiagnostic(message, true);
    } else {
      this.appendLines([`!${message}`]);
      this.refreshTerminal();
    }
  }

  private requireProject(): WorkingProject {
    if (this.activeProject === undefined) {
      throw new Error('NO CARTRIDGE LOADED');
    }
    return this.activeProject;
  }

  private stopPlayer(): void {
    this.player?.stop();
    this.player = undefined;
  }

  private stopDebugger(): void {
    this.activeDebugger?.stop();
    this.activeDebugger = undefined;
  }

  private appendLines(lines: readonly string[]): void {
    this.terminalLines.push(...lines);
    if (this.terminalLines.length > 80) {
      const removed = this.terminalLines.length - 80;
      this.terminalLines.splice(0, removed);
      this.visibleTerminalLines = Math.max(0, this.visibleTerminalLines - removed);
    }
    this.refreshTerminal();
  }

  private clearTerminal(): void {
    this.stopTerminalReveal();
    this.terminalLines.length = 0;
    this.visibleTerminalLines = 0;
    this.refreshTerminal();
  }

  private refreshTerminal(): void {
    const terminal = this.root.querySelector<HTMLElement>('.terminal');
    if (terminal === null) {
      return;
    }
    terminal.replaceChildren(
      ...this.terminalLines.slice(0, this.visibleTerminalLines).map((line) => {
        const paragraph = document.createElement('p');
        if (line.startsWith('> ')) {
          paragraph.className = 'terminal-command';
          const prompt = document.createElement('span');
          prompt.className = 'terminal-prompt';
          prompt.textContent = '>';
          const command = document.createElement('span');
          command.className = 'terminal-input';
          command.textContent = line.slice(1);
          paragraph.append(prompt, command);
        } else {
          paragraph.className =
            line.startsWith('!') || line.startsWith('?') ? 'terminal-error' : 'terminal-output';
          paragraph.textContent = line || '\u00a0';
        }
        return paragraph;
      }),
    );
    terminal.scrollTop = terminal.scrollHeight;
    this.scheduleTerminalReveal();
  }

  private scheduleTerminalReveal(): void {
    if (this.terminalRevealTimer !== undefined) return;
    if (this.visibleTerminalLines >= this.terminalLines.length) {
      this.resolveTerminalIdle();
      return;
    }
    if (this.root.querySelector('.terminal') === null) return;
    this.terminalRevealTimer = window.setTimeout(() => {
      this.terminalRevealTimer = undefined;
      if (this.root.querySelector('.terminal') === null) return;
      this.visibleTerminalLines += 1;
      this.refreshTerminal();
    }, TERMINAL_LINE_DELAY_MS);
  }

  private stopTerminalReveal(): void {
    if (this.terminalRevealTimer !== undefined) window.clearTimeout(this.terminalRevealTimer);
    this.terminalRevealTimer = undefined;
  }

  private waitForTerminalIdle(): Promise<void> {
    if (this.visibleTerminalLines >= this.terminalLines.length) return Promise.resolve();
    return new Promise((resolve) => this.terminalIdleResolvers.push(resolve));
  }

  private resolveTerminalIdle(): void {
    while (this.terminalIdleResolvers.length > 0) this.terminalIdleResolvers.shift()?.();
  }
}

async function readFolderFile(
  root: FileSystemDirectoryHandle,
  path: string,
  optional = false,
): Promise<{ readonly bytes: Uint8Array; readonly lastModified: number } | undefined> {
  try {
    const parts = safeFolderPath(path);
    const name = parts.pop();
    if (name === undefined) throw new TypeError('FOLDER PATH IS EMPTY');
    let directory = root;
    for (const part of parts) directory = await directory.getDirectoryHandle(part);
    const file = await (await directory.getFileHandle(name)).getFile();
    if (file.size > 2 * 1024 * 1024) throw new RangeError(`FOLDER FILE ${path} EXCEEDS 2 MIB`);
    return { bytes: new Uint8Array(await file.arrayBuffer()), lastModified: file.lastModified };
  } catch (error: unknown) {
    if (optional && error instanceof DOMException && error.name === 'NotFoundError')
      return undefined;
    throw error;
  }
}

async function readFolderWorkcart(root: FileSystemDirectoryHandle): Promise<
  | {
      readonly path: string;
      readonly file: { readonly bytes: Uint8Array; readonly lastModified: number };
    }
  | undefined
> {
  const names: string[] = [];
  for await (const entry of root.values()) {
    if (entry.kind === 'file' && entry.name.toLowerCase().endsWith('.m01w')) names.push(entry.name);
  }
  names.sort();
  if (names.length > 1) throw new Error('FOLDER HAS MULTIPLE .M01W WORK-CARTS');
  const path = names[0];
  if (path === undefined) return undefined;
  const file = await readFolderFile(root, path);
  if (file === undefined) throw new Error(`WORK-CART ${path} DISAPPEARED`);
  return { path, file };
}

async function writeFolderFile(
  root: FileSystemDirectoryHandle,
  path: string,
  bytes: Uint8Array,
): Promise<File> {
  const parts = safeFolderPath(path);
  const name = parts.pop();
  if (name === undefined) throw new TypeError('FOLDER PATH IS EMPTY');
  let directory = root;
  for (const part of parts) directory = await directory.getDirectoryHandle(part, { create: true });
  const handle = await directory.getFileHandle(name, { create: true });
  const writable = await handle.createWritable();
  await writable.write(bytes.slice().buffer);
  await writable.close();
  return handle.getFile();
}

async function collectFolderSources(
  root: FileSystemDirectoryHandle,
  path: string,
  files: Record<string, Uint8Array>,
  modified: Map<string, number>,
  depth: number,
): Promise<void> {
  if (depth > 8) throw new RangeError('FOLDER SOURCE DEPTH EXCEEDS 8');
  const parts = safeFolderPath(path);
  let directory = root;
  try {
    for (const part of parts) directory = await directory.getDirectoryHandle(part);
  } catch (error: unknown) {
    if (error instanceof DOMException && error.name === 'NotFoundError') return;
    throw error;
  }
  for await (const entry of directory.values()) {
    const child = `${path}/${entry.name}`;
    if (entry.kind === 'directory') {
      await collectFolderSources(root, child, files, modified, depth + 1);
    } else if (entry.name.endsWith('.modl')) {
      const file = await readFolderFile(root, child);
      if (file !== undefined) {
        files[child] = file.bytes;
        modified.set(child, file.lastModified);
      }
    }
    if (Object.keys(files).length > 4096) throw new RangeError('FOLDER FILE COUNT EXCEEDS 4096');
  }
}

function safeFolderPath(path: string): string[] {
  const parts = path.split('/');
  if (
    path.length === 0 ||
    path.length > 1024 ||
    path.startsWith('/') ||
    path.includes('\\') ||
    parts.some((part) => !/^[A-Za-z0-9._-]+$/.test(part) || part === '.' || part === '..')
  )
    throw new TypeError(`UNSAFE FOLDER PATH ${path}`);
  return parts;
}

function fromStored(project: StoredProject): WorkingProject {
  return {
    id: project.id,
    title: project.title,
    manifest: project.manifest,
    revision: project.revision,
    files: Object.fromEntries(
      Object.entries(project.files).map(([path, bytes]) => [path, bytes.slice()]),
    ),
    ...(project.workcart === undefined ? {} : { workcart: project.workcart }),
  };
}

function manifestEntry(manifest: string): string {
  return /^entry\s*=\s*"([A-Za-z0-9_./-]+)"\s*$/m.exec(manifest)?.[1] ?? 'src/main.modl';
}

function manifestUpdateRate(manifest: string): 30 | 60 {
  return /^update_rate\s*=\s*30\s*$/m.test(manifest) ? 30 : 60;
}

function workcartRecipes(values: readonly unknown[]): WorkbenchRecipe[] {
  return values.flatMap((value) => {
    if (typeof value !== 'object' || value === null) return [];
    const recipe = value as Record<string, unknown>;
    if (
      typeof recipe.id !== 'string' ||
      typeof recipe.algorithm !== 'string' ||
      typeof recipe.output !== 'string'
    )
      return [];
    return [{ id: recipe.id, algorithm: recipe.algorithm, output: recipe.output }];
  });
}

function splitCommand(source: string): string[] {
  const result: string[] = [];
  for (const match of source.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)) {
    result.push(match[1] ?? match[2] ?? match[3] ?? '');
  }
  return result;
}

const COMPLETIONS = [
  'import',
  'pub',
  'private',
  'state',
  'let',
  'var',
  'fn',
  'task',
  'on',
  'if',
  'else',
  'for',
  'while',
  'match',
  'return',
  'wait',
  'start',
  'clear',
  'pixel',
  'line',
  'rect',
  'rect_fill',
  'circle',
  'circle_fill',
  'triangle',
  'sprite',
  'animation',
  'map',
  'print',
  'font_print',
  'btn',
  'btnp',
  'rng_int',
  'rng_num',
  'sfx',
  'music',
  'save_get_int',
  'save_set_int',
  'save_commit',
] as const;

function completeAtCursor(textarea: HTMLTextAreaElement): void {
  const cursor = textarea.selectionStart;
  const prefix = /[A-Za-z_][A-Za-z0-9_]*$/.exec(textarea.value.slice(0, cursor))?.[0] ?? '';
  const completion = COMPLETIONS.find((candidate) => candidate.startsWith(prefix));
  if (completion === undefined) {
    return;
  }
  const start = cursor - prefix.length;
  textarea.setRangeText(completion, start, cursor, 'end');
}

function gotoDefinition(textarea: HTMLTextAreaElement): void {
  const cursor = textarea.selectionStart;
  const left = textarea.value.slice(0, cursor).search(/[A-Za-z_][A-Za-z0-9_]*$/);
  const tail = /^[A-Za-z0-9_]*/.exec(textarea.value.slice(cursor))?.[0] ?? '';
  if (left < 0) return;
  const symbol = `${textarea.value.slice(0, cursor).slice(left)}${tail}`;
  const matcher = new RegExp(`^(?:state|const|fn|task|record|enum|let|var)\\s+${symbol}\\b`, 'm');
  const definition = matcher.exec(textarea.value);
  if (definition === null) return;
  const start = definition.index + definition[0].lastIndexOf(symbol);
  textarea.focus();
  textarea.setSelectionRange(start, start + symbol.length);
  const line = textarea.value.slice(0, start).split('\n').length - 1;
  textarea.scrollTop = Math.max(0, line * 7 - 28);
}

function symbolAtCursor(textarea: HTMLTextAreaElement): string {
  const cursor = textarea.selectionStart;
  const before = /[A-Za-z_][A-Za-z0-9_]*$/.exec(textarea.value.slice(0, cursor))?.[0] ?? '';
  const after = /^[A-Za-z0-9_]*/.exec(textarea.value.slice(cursor))?.[0] ?? '';
  return `${before}${after}`;
}

function manualTitleForSymbol(symbol: string): string {
  if (
    ['mem_read', 'mem_read16', 'mem_write', 'mem_write16', 'mem_copy', 'mem_fill'].includes(symbol)
  )
    return 'MEMORY';
  if (['btn', 'btnp', 'pad1', 'pad2', 'pad3', 'pad4'].includes(symbol)) return 'INPUT';
  if (['sfx', 'music', 'music_stop'].includes(symbol)) return 'AUDIO';
  if (['task', 'start', 'wait'].includes(symbol)) return 'TASKS';
  if (['save_get_int', 'save_set_int', 'save_commit'].includes(symbol)) return 'ARTIFACTS';
  if (
    [
      'clear',
      'pixel',
      'line',
      'rect',
      'rect_fill',
      'circle',
      'circle_fill',
      'sprite',
      'map',
      'print',
      'font_print',
    ].includes(symbol)
  )
    return 'DRAWING';
  return symbol.length === 0 ? 'START' : symbol;
}

function renderHighlight(target: HTMLElement, source: string): void {
  const pattern =
    /\/\/.*$|"(?:\\.|[^"\\])*"|#[A-Za-z_][A-Za-z0-9_]*|\b(?:and|as|assert|break|case|const|continue|draw|elif|else|enum|false|fn|for|if|import|in|let|match|none|not|on|or|private|pub|raster|record|return|start|state|task|true|update|var|wait|while)\b|\b\d+(?:\.\d+)?(?:f|s)?\b/gm;
  target.replaceChildren();
  let cursor = 0;
  for (const match of source.matchAll(pattern)) {
    const index = match.index;
    if (index > cursor) target.append(document.createTextNode(source.slice(cursor, index)));
    const token = document.createElement('span');
    const text = match[0];
    token.className = text.startsWith('//')
      ? 'syntax-comment'
      : text.startsWith('"')
        ? 'syntax-text'
        : text.startsWith('#')
          ? 'syntax-asset'
          : /^\d/.test(text)
            ? 'syntax-number'
            : 'syntax-keyword';
    token.textContent = text;
    target.append(token);
    cursor = index + text.length;
  }
  target.append(document.createTextNode(source.slice(cursor)));
}

function renderJsonHighlight(target: HTMLElement, source: string): void {
  const pattern =
    /"(?:\\.|[^"\\])*"(?=\s*:)|"(?:\\.|[^"\\])*"|\b(?:true|false|null)\b|-?\b\d+(?:\.\d+)?(?:e[+-]?\d+)?\b/gi;
  target.replaceChildren();
  let cursor = 0;
  for (const match of source.matchAll(pattern)) {
    const index = match.index;
    if (index > cursor) target.append(document.createTextNode(source.slice(cursor, index)));
    const token = document.createElement('span');
    const text = match[0];
    token.className = text.startsWith('"')
      ? source.slice(index + text.length).match(/^\s*:/) === null
        ? 'syntax-text'
        : 'syntax-json-key'
      : /^-?\d/.test(text)
        ? 'syntax-number'
        : 'syntax-literal';
    token.textContent = text;
    target.append(token);
    cursor = index + text.length;
  }
  target.append(document.createTextNode(source.slice(cursor)));
}

function manualTopics(): readonly { readonly title: string; readonly body: string }[] {
  return [
    {
      title: 'START',
      body: 'Create with NEW id, open EDIT, then RUN. Save with F3 or SAVE. PACK downloads a deterministic source-inspectable cartridge. LOAD modl-tutorial runs the five-to-ten-minute FIRST SIGNAL lesson.',
    },
    {
      title: 'MODL',
      body: 'MODL/1 is ASCII-only, statically typed, indentation-based, and deterministic. Mutable top-level values use state with an explicit type. Arrays and Lists have fixed checked capacities; use none/some/is_some/unwrap_or with Option values.',
    },
    {
      title: 'CALLBACKS',
      body: 'on start runs once. on update runs at 30 or 60 Hz. on draw renders every frame. on raster(line: Int) may change palette and scroll state.',
    },
    {
      title: 'TASKS',
      body: 'Declare task name(...): and launch it with start name(...). wait 2f suspends for frames; wait 0.5s converts to cartridge time.',
    },
    {
      title: 'DRAWING',
      body: 'Use clear, pixel, line, rect/rect_fill, circle/circle_fill, triangle, sprite/sprite_xform, animation, map/map_cell/map_flag, print/font_print, camera, clip, pal, dither, and raster_scroll. FONT edits custom glyphs; print keeps the fixed system font. Colors are fixed indices 0-31.',
    },
    {
      title: 'INPUT',
      body: 'btn and btnp accept pad1 through pad4 and button values up/down/left/right/a/b/x/y/l/r/start_button/menu. pointer_x/y, pointer_inside, and pointer_primary/secondary expose recorded mouse, pen, and touch input.',
    },
    {
      title: 'AUDIO',
      body: 'SFX are eight-voice oscillator patches. MUSIC opens the eight-channel pattern tracker. Imported PCM and arbitrary samples are unavailable.',
    },
    {
      title: 'DEBUG',
      body: 'DEBUG opens frame pause, source breakpoints, trace stepping, watches, state/task/profile/hardware inspectors, and deterministic rewind. Debug save writes are not persisted.',
    },
    {
      title: 'HARDWARE',
      body: 'Hardware Revision 1 uses a 22-bit byte bus. In DEBUG choose MEMO, enter a hexadecimal address and 1-64 byte length, then GET. HEX/DEC changes display and SET edits one writable byte while paused. WP adds up to eight change watchpoints. RAM begins 000000, framebuffers 010000, visual store 030000, registers 050000, save 058000, cartridge ROM 060000. Reserved bytes read zero and reject writes.',
    },
    {
      title: 'MEMORY',
      body: 'mem_read/mem_write access bytes; mem_read16/mem_write16 use little-endian words. mem_copy is overlap-safe and mem_fill is transactional. Fault M019005 marks bounds/permission errors. The debugger MEMO page links the same named regions.',
    },
    {
      title: 'DIAGNOSTICS',
      body: 'M011xxx lexes, M012xxx parses, M013xxx resolves/types, M014xxx validates cartridges, and M019xxx reports deterministic runtime/hardware faults. A diagnostic includes its source range; F1 on a symbol opens the matching API page.',
    },
    {
      title: 'TUTORIAL',
      body: 'LOAD modl-tutorial then RUN. A advances through pixel, drawing, input, animation, synth, save and pack; B moves back. Inspect its ordinary src/main.modl at any time.',
    },
    {
      title: 'LIMITS',
      body: '240x144, 32 colors, 128 KiB visual assets, 8 KiB save, 256 KiB packed cartridge, 50,000 work units, 4096 draw commands, 8 synth voices, 4 local ports.',
    },
    {
      title: 'ARTIFACTS',
      body: 'PACK downloads raw source-visible .m01c. CART downloads the MOD-01 cartridge-object .m01c.png with the same complete bytes and project identity metadata; run first to use the last game frame as its label. EXPORT writes the single-file offline HTML player.',
    },
  ];
}

function decodeIdentity(bytes: Uint8Array | undefined): {
  readonly year: number;
  readonly players: number;
  readonly controls: string;
} {
  if (bytes !== undefined) {
    try {
      const value: unknown = JSON.parse(decoder.decode(bytes));
      if (
        typeof value === 'object' &&
        value !== null &&
        'revision' in value &&
        value.revision === 1 &&
        'year' in value &&
        typeof value.year === 'number' &&
        Number.isSafeInteger(value.year) &&
        value.year >= 1970 &&
        value.year <= 9999 &&
        'players' in value &&
        typeof value.players === 'number' &&
        Number.isSafeInteger(value.players) &&
        value.players >= 1 &&
        value.players <= 4 &&
        'controls' in value &&
        typeof value.controls === 'string' &&
        value.controls.length <= 64
      )
        return { year: value.year, players: value.players, controls: value.controls };
    } catch {
      // A malformed optional identity file falls back without blocking raw project access.
    }
  }
  return { year: 1999, players: 1, controls: 'PAD' };
}

function indexedFrame(indexed: Uint8Array): DecodedPng {
  if (indexed.length !== HARDWARE.width * HARDWARE.height)
    throw new RangeError('captured framebuffer dimensions are invalid');
  const rgba = new Uint8Array(indexed.length * 4);
  for (let pixel = 0; pixel < indexed.length; pixel += 1) {
    const color = (indexed[pixel] ?? 0) * 4;
    rgba[pixel * 4] = MASTER_PALETTE_RGBA[color] ?? 0;
    rgba[pixel * 4 + 1] = MASTER_PALETTE_RGBA[color + 1] ?? 0;
    rgba[pixel * 4 + 2] = MASTER_PALETTE_RGBA[color + 2] ?? 0;
    rgba[pixel * 4 + 3] = 255;
  }
  return { width: HARDWARE.width, height: HARDWARE.height, rgba };
}

function scaleRgba(image: DecodedPng, scale: number): DecodedPng {
  if (!Number.isSafeInteger(scale) || scale < 1 || scale > 4)
    throw new RangeError('capture scale must be 1-4');
  const width = image.width * scale;
  const height = image.height * scale;
  const rgba = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const source = (Math.floor(y / scale) * image.width + Math.floor(x / scale)) * 4;
      rgba.set(image.rgba.subarray(source, source + 4), (y * width + x) * 4);
    }
  }
  return { width, height, rgba };
}

function sizeClass(bytes: number): '4K' | '16K' | '64K' | '256K' {
  if (bytes <= 4_096) return '4K';
  if (bytes <= 16_384) return '16K';
  if (bytes <= 65_536) return '64K';
  return '256K';
}

interface ProjectSizeReport {
  readonly sizeClass: '4K' | '16K' | '64K' | '256K';
  readonly canonicalCartridgeBytes: number;
  readonly sourceBytes: number;
  readonly generatedReleaseBytes: number;
  readonly visualBytes: number;
  readonly mapBytes: number;
  readonly fontBytes: number;
  readonly audioBytes: number;
  readonly metadataBytes: number;
  readonly containerOverheadBytes: number;
  readonly compressionGainBytes: number;
  readonly saveAllocationBytes: 8192;
  readonly largestEntries: readonly { readonly path: string; readonly bytes: number }[];
  readonly largestSymbols: readonly {
    readonly name: string;
    readonly kind: string;
    readonly definitionBytes: number;
  }[];
  readonly warning: string;
}

function projectSizeReport(
  packed: Uint8Array,
  entries: Readonly<Record<string, readonly number[]>>,
  manifest: ProjectManifest,
  compilation: CompilationResult,
): ProjectSizeReport {
  const encoded = archiveEncodedBytes(packed);
  let sourceBytes = 0;
  let visualBytes = 0;
  let mapBytes = 0;
  let fontBytes = 0;
  let audioBytes = 0;
  let metadataBytes = 0;
  for (const [path, bytes] of Object.entries(entries)) {
    if (path.startsWith('source/')) sourceBytes += bytes.length;
    if (path === 'manifest.json' || path.startsWith('presentation/')) metadataBytes += bytes.length;
  }
  for (const asset of Object.values(manifest.assets)) {
    const archivePath = `assets/${asset.path}`;
    const bytes = entries[archivePath]?.length ?? 0;
    if (asset.kind === 'map') {
      mapBytes += bytes;
      visualBytes += bytes;
    } else if (asset.kind === 'font') {
      fontBytes += bytes;
      visualBytes += bytes;
    } else if (asset.kind === 'sound' || asset.kind === 'music') {
      audioBytes += bytes;
    } else {
      visualBytes += bytes;
    }
  }
  const rawArchiveBytes = Object.values(entries).reduce((total, bytes) => total + bytes.length, 0);
  const generatedReleaseBytes = compilation.generated?.generated_bytes ?? 0;
  const work = Object.values(compilation.generated?.work_model ?? {}).reduce(
    (total, value) => total + value,
    0,
  );
  const warning =
    packed.length > 196_608
      ? 'PACK >75%; INSPECT LARGEST ENTRY'
      : visualBytes > 98_304
        ? 'VISUAL >75%; TRIM LARGEST ASSET'
        : work > 40_000
          ? 'STATIC WORK MODEL HIGH; PROFILE RUN'
          : 'BUDGETS WITHIN FIXED LIMITS';
  return {
    sizeClass: sizeClass(packed.length),
    canonicalCartridgeBytes: packed.length,
    sourceBytes,
    generatedReleaseBytes,
    visualBytes,
    mapBytes,
    fontBytes,
    audioBytes,
    metadataBytes,
    containerOverheadBytes: packed.length - encoded,
    compressionGainBytes: rawArchiveBytes - encoded,
    saveAllocationBytes: 8192,
    largestEntries: Object.entries(entries)
      .map(([path, bytes]) => ({ path, bytes: bytes.length }))
      .sort((left, right) => right.bytes - left.bytes || left.path.localeCompare(right.path))
      .slice(0, 8),
    largestSymbols: compilation.analysis.symbols
      .filter((symbol) => symbol.defined_at !== undefined)
      .map((symbol) => ({
        name: symbol.name,
        kind: symbol.kind,
        definitionBytes: (symbol.defined_at?.end ?? 0) - (symbol.defined_at?.start ?? 0),
      }))
      .sort(
        (left, right) =>
          right.definitionBytes - left.definitionBytes || left.name.localeCompare(right.name),
      )
      .slice(0, 8),
    warning,
  };
}

function archiveEncodedBytes(bytes: Uint8Array): number {
  if (bytes.length < 12) throw new TypeError('M01C HEADER IS TRUNCATED');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const count = view.getUint32(8, true);
  let cursor = 12;
  let encoded = 0;
  for (let index = 0; index < count; index += 1) {
    if (cursor + 42 > bytes.length) throw new TypeError('M01C ENTRY HEADER IS TRUNCATED');
    const pathLength = view.getUint16(cursor, true);
    const encodedLength = view.getUint32(cursor + 6, true);
    cursor += 42;
    const end = cursor + pathLength + encodedLength;
    if (!Number.isSafeInteger(end) || end > bytes.length)
      throw new TypeError('M01C ENTRY IS TRUNCATED');
    encoded += encodedLength;
    cursor = end;
  }
  if (cursor !== bytes.length) throw new TypeError('M01C HAS TRAILING DATA');
  return encoded;
}

function replaceManifestIdentity(manifest: string, id: string, title: string): string {
  if (!PROJECT_ID.test(id)) throw new TypeError('duplicate cartridge id is invalid');
  const idMatches = manifest.match(/^id\s*=.*$/gm);
  const titleMatches = manifest.match(/^title\s*=.*$/gm);
  if (idMatches?.length !== 1 || titleMatches?.length !== 1)
    throw new TypeError('manifest identity is ambiguous');
  return manifest
    .replace(/^id\s*=.*$/m, `id = ${JSON.stringify(id)}`)
    .replace(/^title\s*=.*$/m, `title = ${JSON.stringify(title)}`);
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function downloadBytes(name: string, bytes: Uint8Array, type: string): void {
  const buffer = new Uint8Array(bytes).buffer;
  const url = URL.createObjectURL(new Blob([buffer], { type }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message.toUpperCase() : 'UNKNOWN SYSTEM ERROR';
}

function requireElement(root: ParentNode, selector: string): Element {
  const element = root.querySelector(selector);
  if (element === null) {
    throw new Error(`studio element '${selector}' is missing`);
  }
  return element;
}
