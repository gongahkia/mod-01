import {
  AudioAssetStore,
  BITMAP_FONT,
  HARDWARE,
  MASTER_PALETTE_RGBA,
  Synthesizer,
  WebAudioSink,
  convertRgbaToIndexed,
  decodeRuntimeAssets,
  decodePngRgba,
  encodeAssetFile,
  encodeMusicAssetFile,
  encodeSoundAssetFile,
  encodeRgbaPng,
  glyphRows,
  renderMusicWav,
  renderSoundWav,
  type MusicAsset,
  type SoundAsset,
  type TrackerCell,
} from '@mod01/runtime';

import type { ProjectManifest } from './compiler';
import { enhancePixelSelects, refreshPixelSelect } from './pixel-select';

export type CreationTool = 'sprite' | 'map' | 'palette' | 'font' | 'sfx' | 'music' | 'project';

export interface ToolProject {
  readonly id: string;
  revision: number;
  title: string;
  manifest: string;
  readonly files: Record<string, Uint8Array>;
}

export interface ToolCallbacks {
  readonly back: () => void;
  readonly save: () => Promise<void>;
  readonly parseManifest: () => Promise<ProjectManifest>;
  readonly capacity?: () => Promise<number>;
  readonly recipes?: () => Promise<readonly WorkbenchRecipe[]>;
  readonly generateRecipe?: (recipeId: string) => Promise<void>;
  readonly deleteAsset?: (assetName: string) => Promise<void>;
}

export interface WorkbenchRecipe {
  readonly id: string;
  readonly algorithm: string;
  readonly output: string;
}

const textDecoder = new TextDecoder();

export async function openCreationTool(
  root: HTMLElement,
  tool: CreationTool,
  project: ToolProject,
  callbacks: ToolCallbacks,
): Promise<void> {
  switch (tool) {
    case 'sprite':
      openSpriteEditor(root, project, callbacks);
      break;
    case 'map':
      await openMapEditor(root, project, callbacks);
      break;
    case 'palette':
      openPaletteEditor(root, project, callbacks);
      break;
    case 'font':
      openFontEditor(root, project, callbacks);
      break;
    case 'sfx':
      openSoundEditor(root, project, callbacks);
      break;
    case 'music':
      openMusicEditor(root, project, callbacks);
      break;
    case 'project':
      await openProjectSettings(root, project, callbacks);
      break;
  }
}

interface AssetHistory {
  readonly past: Uint8Array[];
  readonly future: Uint8Array[];
}

type AssetTool = Exclude<CreationTool, 'project'>;

/** Opens the console-native asset tree, preview, history, and capacity workbench. */
export async function openAssetWorkbench(
  root: HTMLElement,
  project: ToolProject,
  callbacks: ToolCallbacks,
  histories = new Map<string, AssetHistory>(),
): Promise<void> {
  let selectedAsset: string | undefined;
  const render = async (): Promise<void> => {
    const manifest = await callbacks.parseManifest();
    const assets = Object.entries(manifest.assets).sort(([left], [right]) =>
      left.localeCompare(right),
    );
    selectedAsset ??= assets[0]?.[0];
    if (selectedAsset !== undefined && manifest.assets[selectedAsset] === undefined)
      selectedAsset = assets[0]?.[0];
    root.innerHTML = toolFrame(
      'ASSET WORKBENCH',
      `<div class="asset-workbench">
        <nav class="workbench-tree" aria-label="Asset tree"></nav>
        <section class="workbench-preview">
          <p class="workbench-selection">SELECT AN ASSET</p>
          <canvas class="workbench-atlas" width="152" height="48" aria-label="Tile atlas preview"></canvas>
          <canvas class="workbench-map" width="152" height="40" aria-label="Map preview"></canvas>
          <div class="workbench-actions">
            <button type="button" data-workbench="open">OPEN</button>
            <button type="button" data-workbench="undo">UNDO</button>
            <button type="button" data-workbench="redo">REDO</button>
            <button type="button" data-workbench="delete">DEL</button>
            <button type="button" data-workbench="export">OUT</button>
            <label class="file-button">IN<input data-workbench-import type="file" accept="application/json,.m01g,.m01m,.m01f,.m01s,.m01t"></label>
          </div>
          <div class="workbench-recipes"></div>
          <p class="workbench-capacity">PACK CHECKING...</p>
        </section>
      </div>`,
    );
    bindCommon(root, callbacks);
    const tree = requireElement(root, '.workbench-tree');
    appendWorkbenchTree(tree, project, assets, selectedAsset, (name) => {
      selectedAsset = name;
      void render().catch((error: unknown) => {
        setToolStatus(root, errorMessage(error), true);
      });
    });
    const selection = requireElement(root, '.workbench-selection');
    const selected = selectedAsset === undefined ? undefined : manifest.assets[selectedAsset];
    selection.textContent =
      selectedAsset === undefined || selected === undefined
        ? 'NO DECLARED ASSETS'
        : `${selectedAsset.toUpperCase()} / ${selected.kind.toUpperCase()} / ${selected.path}`;
    drawWorkbenchAtlas(
      requireElement(root, '.workbench-atlas') as HTMLCanvasElement,
      firstTileSetPayload(manifest, project.files),
    );
    drawWorkbenchMap(
      requireElement(root, '.workbench-map') as HTMLCanvasElement,
      firstMapPayload(manifest, project.files),
      firstTileSetPayload(manifest, project.files),
    );
    const history = selected === undefined ? undefined : histories.get(selected.path);
    setWorkbenchButtons(root, selected === undefined, history);
    bindWorkbenchActions(root, project, callbacks, manifest, selectedAsset, histories, render);
    const recipes = await callbacks.recipes?.();
    renderWorkbenchRecipes(root, recipes ?? [], async (recipe) => {
      if (callbacks.generateRecipe === undefined) return;
      const before = assetSnapshots(manifest, project.files);
      await callbacks.generateRecipe(recipe.id);
      recordAssetHistory(before, manifest, project.files, histories);
      await render();
    });
    const capacity = requireElement(root, '.workbench-capacity');
    if (callbacks.capacity === undefined) {
      capacity.textContent = 'PACK METER UNAVAILABLE';
    } else {
      void callbacks
        .capacity()
        .then((bytes) => {
          if (root.contains(capacity)) {
            capacity.textContent = `PACK ${String(bytes)}/${String(HARDWARE.cartridgeCapacityBytes)}B ${String(Math.round((bytes / HARDWARE.cartridgeCapacityBytes) * 100))}%`;
          }
        })
        .catch((error: unknown) => {
          if (root.contains(capacity)) capacity.textContent = `PACK ${errorMessage(error)}`;
        });
    }
  };
  await render();
}

function appendWorkbenchTree(
  tree: Element,
  project: ToolProject,
  assets: readonly [string, ProjectManifest['assets'][string]][],
  selected: string | undefined,
  select: (name: string) => void,
): void {
  const modules = Object.keys(project.files)
    .filter((path) => path.endsWith('.modl'))
    .sort();
  const moduleHeading = document.createElement('p');
  moduleHeading.textContent = 'MODULES';
  tree.append(moduleHeading);
  for (const path of modules) {
    const row = document.createElement('p');
    row.className = 'workbench-module';
    row.textContent = path;
    tree.append(row);
  }
  const assetHeading = document.createElement('p');
  assetHeading.textContent = 'ASSETS';
  tree.append(assetHeading);
  for (const [name, asset] of assets) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = name === selected ? 'selected' : '';
    button.textContent = `${name} [${asset.kind}]`;
    button.title = asset.path;
    button.addEventListener('click', () => {
      select(name);
    });
    tree.append(button);
  }
}

function setWorkbenchButtons(
  root: HTMLElement,
  disabled: boolean,
  history: AssetHistory | undefined,
): void {
  for (const action of ['open', 'delete', 'export'] as const) {
    const button = root.querySelector<HTMLButtonElement>(`[data-workbench="${action}"]`);
    if (button !== null) button.disabled = disabled;
  }
  const undo = root.querySelector<HTMLButtonElement>('[data-workbench="undo"]');
  if (undo !== null) undo.disabled = history === undefined || history.past.length === 0;
  const redo = root.querySelector<HTMLButtonElement>('[data-workbench="redo"]');
  if (redo !== null) redo.disabled = history === undefined || history.future.length === 0;
  const input = root.querySelector<HTMLInputElement>('[data-workbench-import]');
  if (input !== null) input.disabled = disabled;
}

function bindWorkbenchActions(
  root: HTMLElement,
  project: ToolProject,
  callbacks: ToolCallbacks,
  manifest: ProjectManifest,
  selectedName: string | undefined,
  histories: Map<string, AssetHistory>,
  render: () => Promise<void>,
): void {
  const selected = selectedName === undefined ? undefined : manifest.assets[selectedName];
  const recordAndSave = async (before: Map<string, Uint8Array>): Promise<void> => {
    recordAssetHistory(before, manifest, project.files, histories);
    await callbacks.save();
  };
  root.querySelector('[data-workbench="open"]')?.addEventListener('click', () => {
    if (selected === undefined) return;
    const tool = workbenchTool(selected.kind);
    if (tool === undefined) {
      setToolStatus(root, `${selected.kind.toUpperCase()} HAS NO EDITOR`, true);
      return;
    }
    let previous = assetSnapshots(manifest, project.files);
    void openCreationTool(root, tool, project, {
      ...callbacks,
      back: () => {
        void openAssetWorkbench(root, project, callbacks, histories);
      },
      save: async () => {
        recordAssetHistory(previous, manifest, project.files, histories);
        previous = assetSnapshots(manifest, project.files);
        await callbacks.save();
      },
    }).catch((error: unknown) => {
      setToolStatus(root, errorMessage(error), true);
    });
  });
  root.querySelector('[data-workbench="undo"]')?.addEventListener('click', () => {
    if (selected === undefined) return;
    void applyAssetHistory(selected.path, project, callbacks, histories, 'undo')
      .then(render)
      .catch((error: unknown) => {
        setToolStatus(root, errorMessage(error), true);
      });
  });
  root.querySelector('[data-workbench="redo"]')?.addEventListener('click', () => {
    if (selected === undefined) return;
    void applyAssetHistory(selected.path, project, callbacks, histories, 'redo')
      .then(render)
      .catch((error: unknown) => {
        setToolStatus(root, errorMessage(error), true);
      });
  });
  root
    .querySelector<HTMLButtonElement>('[data-workbench="delete"]')
    ?.addEventListener('click', (event) => {
      if (selectedName === undefined || callbacks.deleteAsset === undefined) return;
      const button = event.currentTarget as HTMLButtonElement;
      if (button.dataset.confirm !== 'true') {
        button.dataset.confirm = 'true';
        button.textContent = 'CONFIRM DEL';
        setToolStatus(root, `CONFIRM DELETE ${selectedName.toUpperCase()}`, true);
        return;
      }
      void callbacks
        .deleteAsset(selectedName)
        .then(() => {
          if (selected !== undefined) histories.delete(selected.path);
          return render();
        })
        .catch((error: unknown) => {
          setToolStatus(root, errorMessage(error), true);
        });
    });
  root.querySelector('[data-workbench="export"]')?.addEventListener('click', () => {
    if (selected === undefined) return;
    const bytes = project.files[selected.path];
    if (bytes === undefined) return;
    downloadToolBytes(selected.path.split('/').at(-1) ?? selected.path, bytes, 'application/json');
  });
  root
    .querySelector<HTMLInputElement>('[data-workbench-import]')
    ?.addEventListener('change', (event) => {
      const file = (event.currentTarget as HTMLInputElement).files?.[0];
      if (file === undefined || selected === undefined) return;
      void file
        .arrayBuffer()
        .then(async (buffer) => {
          if (buffer.byteLength > HARDWARE.visualCapacityBytes) {
            throw new RangeError('ASSET IMPORT EXCEEDS VISUAL CAPACITY');
          }
          const bytes = new Uint8Array(buffer);
          const value = JSON.parse(textDecoder.decode(bytes)) as { kind?: unknown };
          if (value.kind !== selected.kind) throw new TypeError('ASSET KIND DOES NOT MATCH TARGET');
          const before = assetSnapshots(manifest, project.files);
          project.files[selected.path] = bytes;
          await recordAndSave(before);
          await render();
        })
        .catch((error: unknown) => {
          setToolStatus(root, errorMessage(error), true);
        });
    });
}

function workbenchTool(kind: ProjectManifest['assets'][string]['kind']): AssetTool | undefined {
  switch (kind) {
    case 'sprite':
    case 'animation':
    case 'tile_set':
      return 'sprite';
    case 'map':
      return 'map';
    case 'font':
      return 'font';
    case 'sound':
      return 'sfx';
    case 'music':
      return 'music';
  }
}

function assetSnapshots(
  manifest: ProjectManifest,
  files: Readonly<Record<string, Uint8Array>>,
): Map<string, Uint8Array> {
  return new Map(
    Object.values(manifest.assets).flatMap((asset) => {
      const bytes = files[asset.path];
      return bytes === undefined ? [] : [[asset.path, bytes.slice()] as const];
    }),
  );
}

function recordAssetHistory(
  before: ReadonlyMap<string, Uint8Array>,
  manifest: ProjectManifest,
  files: Readonly<Record<string, Uint8Array>>,
  histories: Map<string, AssetHistory>,
): void {
  for (const asset of Object.values(manifest.assets)) {
    const previous = before.get(asset.path);
    const current = files[asset.path];
    if (previous === undefined || current === undefined || equalBytes(previous, current)) continue;
    const history = histories.get(asset.path) ?? { past: [], future: [] };
    history.past.push(previous.slice());
    if (history.past.length > 64) history.past.shift();
    history.future.length = 0;
    histories.set(asset.path, history);
  }
}

async function applyAssetHistory(
  path: string,
  project: ToolProject,
  callbacks: ToolCallbacks,
  histories: Map<string, AssetHistory>,
  action: 'undo' | 'redo',
): Promise<void> {
  const history = histories.get(path);
  const current = project.files[path];
  if (history === undefined || current === undefined) return;
  const source = action === 'undo' ? history.past : history.future;
  const target = source.pop();
  if (target === undefined) return;
  const destination = action === 'undo' ? history.future : history.past;
  destination.push(current.slice());
  project.files[path] = target;
  await callbacks.save();
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  return left.byteLength === right.byteLength && left.every((byte, index) => byte === right[index]);
}

function firstTileSetPayload(
  manifest: ProjectManifest,
  files: Readonly<Record<string, Uint8Array>>,
): readonly (readonly number[])[] | undefined {
  const asset = Object.values(manifest.assets).find((candidate) => candidate.kind === 'tile_set');
  if (asset === undefined) return undefined;
  const parsed = readJson<{ tiles?: unknown } | undefined>(files[asset.path], undefined);
  return Array.isArray(parsed?.tiles)
    ? (parsed.tiles as readonly (readonly number[])[])
    : undefined;
}

function firstMapPayload(
  manifest: ProjectManifest,
  files: Readonly<Record<string, Uint8Array>>,
):
  | { readonly width: number; readonly height: number; readonly cells: readonly number[] }
  | undefined {
  const asset = Object.values(manifest.assets).find((candidate) => candidate.kind === 'map');
  if (asset === undefined) return undefined;
  const parsed = readJson<{ layers?: unknown } | undefined>(files[asset.path], undefined);
  const layers: unknown = parsed?.layers;
  const layer: unknown = Array.isArray(layers) ? (layers as unknown[])[0] : undefined;
  if (
    typeof layer !== 'object' ||
    layer === null ||
    !Number.isInteger((layer as { width?: unknown }).width) ||
    !Number.isInteger((layer as { height?: unknown }).height) ||
    !Array.isArray((layer as { cells?: unknown }).cells)
  )
    return undefined;
  const map = layer as { width: number; height: number; cells: readonly number[] };
  return map.cells.length === map.width * map.height ? map : undefined;
}

function drawWorkbenchAtlas(
  canvas: HTMLCanvasElement,
  tiles: readonly (readonly number[])[] | undefined,
): void {
  const context = canvas.getContext('2d');
  if (context === null) return;
  context.fillStyle = '#0b1120';
  context.fillRect(0, 0, canvas.width, canvas.height);
  for (const [index, tile] of (tiles ?? []).slice(0, 19).entries()) {
    if (tile.length !== 64) continue;
    const left = (index % 19) * 8;
    for (let pixel = 0; pixel < tile.length; pixel += 1) {
      context.fillStyle = paletteCss(tile[pixel] ?? 0);
      context.fillRect(left + (pixel % 8), Math.floor(pixel / 8), 1, 1);
    }
  }
}

function drawWorkbenchMap(
  canvas: HTMLCanvasElement,
  map:
    | { readonly width: number; readonly height: number; readonly cells: readonly number[] }
    | undefined,
  tiles: readonly (readonly number[])[] | undefined,
): void {
  const context = canvas.getContext('2d');
  if (context === null) return;
  context.fillStyle = '#0b1120';
  context.fillRect(0, 0, canvas.width, canvas.height);
  if (map === undefined) return;
  const scale = Math.max(
    1,
    Math.floor(Math.min(canvas.width / map.width, canvas.height / map.height)),
  );
  for (let y = 0; y < map.height; y += 1) {
    for (let x = 0; x < map.width; x += 1) {
      const tile = tiles?.[map.cells[y * map.width + x] ?? 0] ?? [];
      const color = tile.find((value) => value !== 0) ?? 0;
      context.fillStyle = paletteCss(color);
      context.fillRect(x * scale, y * scale, scale, scale);
    }
  }
}

function renderWorkbenchRecipes(
  root: HTMLElement,
  recipes: readonly WorkbenchRecipe[],
  generate: (recipe: WorkbenchRecipe) => Promise<void>,
): void {
  const target = requireElement(root, '.workbench-recipes');
  target.replaceChildren();
  if (recipes.length === 0) {
    target.textContent = 'NO STORED RECIPES';
    return;
  }
  for (const recipe of recipes) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = `GEN ${recipe.id}`;
    button.title = `${recipe.algorithm} -> ${recipe.output}`;
    button.addEventListener('click', () => {
      void generate(recipe).catch((error: unknown) => {
        setToolStatus(root, errorMessage(error), true);
      });
    });
    target.append(button);
  }
}

interface MutableFontDocument {
  revision: 1;
  kind: 'font';
  glyphWidth: number;
  glyphHeight: number;
  baseline: number;
  advanceX: number;
  advanceY: number;
  missingGlyph: number;
  glyphs: { code: number; pixels: number[] }[];
}

function openFontEditor(root: HTMLElement, project: ToolProject, callbacks: ToolCallbacks): void {
  const path = 'assets/typeface.m01f';
  const document = readJson<MutableFontDocument>(project.files[path], defaultFont());
  root.innerHTML = toolFrame(
    'BITMAP FONT',
    `<div class="font-layout">
      <canvas class="font-canvas" width="64" height="64" aria-label="Font glyph pixels"></canvas>
      <div class="font-controls">
        <p>GLYPH <span class="glyph-readout"></span></p>
        <div><button data-font="prev">&lt;G</button><button data-font="next">G&gt;</button><input class="glyph-code" aria-label="Glyph code" type="number" min="0" max="255"><button data-font="add">+</button><button data-font="delete">-</button></div>
        <div><button data-font="undo">UNDO</button><button data-font="redo">REDO</button><button data-font="select">SELECT</button></div>
        <div><button data-font="flip-h">FLIP H</button><button data-font="flip-v">FLIP V</button><button data-font="rotate">ROTATE</button></div>
        <div><button data-font-export>M01F OUT</button><label class="file-button">M01F IN<input class="font-file-input" type="file" accept=".m01f,application/json"></label></div>
        <div class="font-metrics"><label>W <input data-font-metric="glyphWidth" type="number" min="1" max="16"></label><label>H <input data-font-metric="glyphHeight" type="number" min="1" max="16"></label><label>BASE <input data-font-metric="baseline" type="number" min="0" max="15"></label><label>AX <input data-font-metric="advanceX" type="number" min="1" max="32"></label><label>AY <input data-font-metric="advanceY" type="number" min="1" max="32"></label><label>MISS <input data-font-metric="missingGlyph" type="number" min="0" max="255"></label></div>
        <p class="capacity"></p>
      </div>
      <label class="font-preview-label">TEXT <input class="font-preview-text" aria-label="Font preview text" maxlength="64" value="MOD-01 ?"></label>
      <canvas class="font-preview" width="128" height="24" aria-label="Font preview"></canvas>
    </div>`,
  );
  bindCommon(root, callbacks);
  const canvas = requireElement(root, '.font-canvas') as HTMLCanvasElement;
  const context = requireContext(canvas);
  const preview = requireElement(root, '.font-preview') as HTMLCanvasElement;
  const previewContext = requireContext(preview);
  const codeInput = requireElement(root, '.glyph-code') as HTMLInputElement;
  let glyphIndex = Math.max(
    0,
    document.glyphs.findIndex((glyph) => glyph.code === 65),
  );
  let painting = false;
  let selecting = false;
  let anchor: [number, number] | undefined;
  let selection: [number, number, number, number] | undefined;
  const undo: string[] = [];
  const redo: string[] = [];
  const snapshot = (): string => JSON.stringify(document);
  const remember = (): void => {
    undo.push(snapshot());
    if (undo.length > 32) undo.shift();
    redo.length = 0;
  };
  const restore = (encoded: string): void => {
    Object.assign(document, JSON.parse(encoded) as MutableFontDocument);
    glyphIndex = Math.min(glyphIndex, document.glyphs.length - 1);
    selection = undefined;
    syncControls();
    draw();
  };
  const fontBytes = (): number =>
    8 + document.glyphs.length * (2 + document.glyphWidth * document.glyphHeight);
  const syncControls = (): void => {
    for (const metric of [
      'glyphWidth',
      'glyphHeight',
      'baseline',
      'advanceX',
      'advanceY',
      'missingGlyph',
    ] as const) {
      (requireElement(root, `[data-font-metric="${metric}"]`) as HTMLInputElement).value = String(
        document[metric],
      );
    }
    codeInput.value = String(document.glyphs[glyphIndex]?.code ?? document.missingGlyph);
  };
  const draw = (): void => {
    const glyph = document.glyphs[glyphIndex];
    if (glyph === undefined) return;
    context.clearRect(0, 0, canvas.width, canvas.height);
    const scale = Math.max(
      1,
      Math.floor(
        Math.min(canvas.width / document.glyphWidth, canvas.height / document.glyphHeight),
      ),
    );
    const offsetX = Math.floor((canvas.width - document.glyphWidth * scale) / 2);
    const offsetY = Math.floor((canvas.height - document.glyphHeight * scale) / 2);
    drawMask(
      context,
      glyph.pixels,
      document.glyphWidth,
      document.glyphHeight,
      scale,
      offsetX,
      offsetY,
      23,
    );
    context.strokeStyle = paletteCss(10);
    context.beginPath();
    context.moveTo(offsetX, offsetY + (document.baseline + 1) * scale - 0.5);
    context.lineTo(
      offsetX + document.glyphWidth * scale,
      offsetY + (document.baseline + 1) * scale - 0.5,
    );
    context.stroke();
    if (selection !== undefined) {
      const [left, top, right, bottom] = selection;
      context.strokeStyle = paletteCss(24);
      context.strokeRect(
        offsetX + left * scale + 0.5,
        offsetY + top * scale + 0.5,
        (right - left + 1) * scale - 1,
        (bottom - top + 1) * scale - 1,
      );
    }
    setText(
      root,
      '.glyph-readout',
      `${String(glyphIndex + 1)}/${String(document.glyphs.length)} $${glyph.code.toString(16).toUpperCase().padStart(2, '0')}`,
    );
    setText(root, '.capacity', `${String(fontBytes())} / ${String(HARDWARE.visualCapacityBytes)}B`);
    codeInput.value = String(glyph.code);
    drawFontPreview(
      previewContext,
      document,
      (requireElement(root, '.font-preview-text') as HTMLInputElement).value,
    );
  };
  const point = (event: PointerEvent): [number, number] | undefined => {
    const bounds = canvas.getBoundingClientRect();
    const scale = Math.max(
      1,
      Math.floor(
        Math.min(canvas.width / document.glyphWidth, canvas.height / document.glyphHeight),
      ),
    );
    const offsetX = Math.floor((canvas.width - document.glyphWidth * scale) / 2);
    const offsetY = Math.floor((canvas.height - document.glyphHeight * scale) / 2);
    const x = Math.floor(((event.clientX - bounds.left) * canvas.width) / bounds.width - offsetX);
    const y = Math.floor(((event.clientY - bounds.top) * canvas.height) / bounds.height - offsetY);
    const column = Math.floor(x / scale);
    const row = Math.floor(y / scale);
    return column >= 0 && row >= 0 && column < document.glyphWidth && row < document.glyphHeight
      ? [column, row]
      : undefined;
  };
  const paint = (event: PointerEvent): void => {
    const selected = point(event);
    const glyph = document.glyphs[glyphIndex];
    if (selected === undefined || glyph === undefined) return;
    glyph.pixels[selected[1] * document.glyphWidth + selected[0]] = event.buttons === 2 ? 0 : 1;
    draw();
  };
  canvas.addEventListener('contextmenu', (event) => {
    event.preventDefault();
  });
  canvas.addEventListener('pointerdown', (event) => {
    const selected = point(event);
    if (selected === undefined) return;
    if (selecting) {
      if (anchor === undefined) anchor = selected;
      else {
        selection = [
          Math.min(anchor[0], selected[0]),
          Math.min(anchor[1], selected[1]),
          Math.max(anchor[0], selected[0]),
          Math.max(anchor[1], selected[1]),
        ];
        anchor = undefined;
        selecting = false;
      }
      draw();
      return;
    }
    remember();
    painting = true;
    canvas.setPointerCapture(event.pointerId);
    paint(event);
  });
  canvas.addEventListener('pointermove', (event) => {
    if (painting) paint(event);
  });
  canvas.addEventListener('pointerup', () => {
    painting = false;
  });
  root.querySelectorAll<HTMLElement>('[data-font]').forEach((button) => {
    button.addEventListener('click', () => {
      const action = button.dataset.font;
      if (action === 'undo' || action === 'redo') {
        const from = action === 'undo' ? undo : redo;
        const to = action === 'undo' ? redo : undo;
        const state = from.pop();
        if (state !== undefined) {
          to.push(snapshot());
          restore(state);
        }
        return;
      }
      if (action === 'prev' || action === 'next') {
        glyphIndex =
          (glyphIndex + document.glyphs.length + (action === 'prev' ? -1 : 1)) %
          document.glyphs.length;
        selection = undefined;
      } else if (action === 'add') {
        const code = clamp(Number(codeInput.value), 0, 255);
        const existing = document.glyphs.findIndex((glyph) => glyph.code === code);
        if (existing >= 0) glyphIndex = existing;
        else if (document.glyphs.length < 256) {
          remember();
          document.glyphs.push({
            code,
            pixels: Array.from({ length: document.glyphWidth * document.glyphHeight }, () => 0),
          });
          document.glyphs.sort((left, right) => left.code - right.code);
          glyphIndex = document.glyphs.findIndex((glyph) => glyph.code === code);
        }
      } else if (action === 'delete') {
        const glyph = document.glyphs[glyphIndex];
        if (
          document.glyphs.length > 1 &&
          glyph !== undefined &&
          glyph.code !== document.missingGlyph
        ) {
          remember();
          document.glyphs.splice(glyphIndex, 1);
          glyphIndex = Math.min(glyphIndex, document.glyphs.length - 1);
        }
      } else if (action === 'select') {
        selecting = true;
        anchor = undefined;
      } else if (action === 'flip-h' || action === 'flip-v' || action === 'rotate') {
        remember();
        const glyph = document.glyphs[glyphIndex];
        if (glyph !== undefined)
          transformMask(glyph.pixels, document.glyphWidth, document.glyphHeight, action, selection);
      }
      syncControls();
      draw();
    });
  });
  root.querySelectorAll<HTMLInputElement>('[data-font-metric]').forEach((input) => {
    input.addEventListener('change', () => {
      const width = clamp(
        Number((requireElement(root, '[data-font-metric="glyphWidth"]') as HTMLInputElement).value),
        1,
        16,
      );
      const height = clamp(
        Number(
          (requireElement(root, '[data-font-metric="glyphHeight"]') as HTMLInputElement).value,
        ),
        1,
        16,
      );
      const proposed = 8 + document.glyphs.length * (2 + width * height);
      if (proposed > HARDWARE.visualCapacityBytes) {
        syncControls();
        setToolStatus(root, 'FONT EXCEEDS VISUAL CAPACITY', true);
        return;
      }
      remember();
      if (width !== document.glyphWidth || height !== document.glyphHeight)
        resizeFont(document, width, height);
      document.baseline = clamp(
        Number((requireElement(root, '[data-font-metric="baseline"]') as HTMLInputElement).value),
        0,
        document.glyphHeight - 1,
      );
      document.advanceX = clamp(
        Number((requireElement(root, '[data-font-metric="advanceX"]') as HTMLInputElement).value),
        1,
        32,
      );
      document.advanceY = clamp(
        Number((requireElement(root, '[data-font-metric="advanceY"]') as HTMLInputElement).value),
        1,
        32,
      );
      const missing = clamp(
        Number(
          (requireElement(root, '[data-font-metric="missingGlyph"]') as HTMLInputElement).value,
        ),
        0,
        255,
      );
      if (document.glyphs.some((glyph) => glyph.code === missing)) document.missingGlyph = missing;
      else setToolStatus(root, 'MISSING GLYPH CODE MUST EXIST', true);
      selection = undefined;
      syncControls();
      draw();
    });
  });
  requireElement(root, '.font-preview-text').addEventListener('input', draw);
  root.querySelector('[data-font-export]')?.addEventListener('click', () => {
    downloadToolBytes('typeface.m01f', encodeAssetFile(document), 'application/json');
  });
  (requireElement(root, '.font-file-input') as HTMLInputElement).addEventListener(
    'change',
    (event) => {
      const file = (event.currentTarget as HTMLInputElement).files?.[0];
      if (file === undefined) return;
      if (file.size > HARDWARE.visualCapacityBytes) {
        setToolStatus(root, 'FONT FILE EXCEEDS VISUAL CAPACITY', true);
        return;
      }
      void file
        .arrayBuffer()
        .then((buffer) => {
          const bytes = new Uint8Array(buffer);
          const bundle = decodeRuntimeAssets(
            { imported: { kind: 'font', path: 'imported.m01f' } },
            { 'imported.m01f': bytes },
          );
          const font = bundle.visual[0];
          if (font?.kind !== 'font') throw new TypeError('font file did not decode as a font');
          remember();
          Object.assign(document, {
            revision: 1,
            kind: 'font',
            glyphWidth: font.glyphWidth,
            glyphHeight: font.glyphHeight,
            baseline: font.baseline,
            advanceX: font.advanceX,
            advanceY: font.advanceY,
            missingGlyph: font.missingGlyph,
            glyphs: [...font.glyphs].map(([code, pixels]) => ({ code, pixels: [...pixels] })),
          } satisfies MutableFontDocument);
          glyphIndex = 0;
          selection = undefined;
          syncControls();
          draw();
          setToolStatus(root, `FONT IMPORTED ${String(document.glyphs.length)} GLYPHS`, false);
        })
        .catch((error: unknown) => {
          setToolStatus(root, errorMessage(error), true);
        });
    },
  );
  bindSave(root, async () => {
    document.glyphs.sort((left, right) => left.code - right.code);
    project.files[path] = encodeAssetFile(document);
    project.manifest = upsertAsset(project.manifest, 'typeface', 'font', path);
    await callbacks.save();
  });
  syncControls();
  draw();
}

interface SpriteDocument {
  revision: 1;
  kind: 'sprite';
  width: number;
  height: number;
  frames: number[][];
}

function spriteToTileSet(sprite: SpriteDocument): TileSetDocument {
  const tiles: number[][] = [];
  for (const frame of sprite.frames) {
    for (let top = 0; top < sprite.height; top += 8) {
      for (let left = 0; left < sprite.width; left += 8) {
        tiles.push(
          Array.from({ length: 64 }, (_pixel, index) => {
            const x = left + (index % 8);
            const y = top + Math.floor(index / 8);
            return x < sprite.width && y < sprite.height ? (frame[y * sprite.width + x] ?? 0) : 0;
          }),
        );
      }
    }
  }
  const atlas = tiles.length > 0 ? tiles : defaultTiles();
  return {
    revision: 1,
    kind: 'tile_set',
    tiles: atlas,
    flags: atlas.map(() => 0),
  };
}

function openSpriteEditor(root: HTMLElement, project: ToolProject, callbacks: ToolCallbacks): void {
  const path = 'assets/hero.m01g';
  const document = readJson<SpriteDocument>(project.files[path], {
    revision: 1,
    kind: 'sprite',
    width: 16,
    height: 16,
    frames: [Array.from({ length: 256 }, () => 0)],
  });
  root.innerHTML = toolFrame(
    'SPRITE / ANIMATION',
    `<div class="sprite-layout">
      <canvas class="pixel-canvas" width="64" height="64" aria-label="Indexed sprite pixels"></canvas>
      <div class="tool-controls">
        <p>HERO <span class="frame-readout"></span></p>
        <div><button data-act="prev">&lt;F</button><button data-act="next">F&gt;</button><button data-act="add">+F</button></div>
        <div><button data-act="undo">UNDO</button><button data-act="redo">REDO</button></div>
        <div><button data-act="flip-h">FLIP H</button><button data-act="flip-v">FLIP V</button></div>
        <div><button data-act="rotate">ROTATE</button><button data-act="select">SELECT</button></div>
        <div><select class="png-mode" aria-label="PNG conversion mode"><option value="nearest">NEAR</option><option value="ordered">DITHER</option></select><label>A&lt;= <input class="png-alpha" aria-label="PNG alpha threshold" type="number" min="0" max="255" value="127"></label><button data-png-export>PNG OUT</button><label class="file-button">PNG IN<input class="sprite-png-input" type="file" accept="image/png,.png"></label></div>
        <label>W <input data-size="width" type="number" min="1" max="64" value="${String(document.width)}"></label>
        <label>H <input data-size="height" type="number" min="1" max="64" value="${String(document.height)}"></label>
        <p class="capacity"></p>
      </div>
      <div class="palette-grid" aria-label="Fixed master palette"></div>
    </div>`,
  );
  bindCommon(root, callbacks);
  const canvas = requireElement(root, '.pixel-canvas') as HTMLCanvasElement;
  const context = requireContext(canvas);
  let frameIndex = 0;
  let color = 23;
  let painting = false;
  let selecting = false;
  let anchor: [number, number] | undefined;
  let selection: [number, number, number, number] | undefined;
  let importedPng: Awaited<ReturnType<typeof decodePngRgba>> | undefined;
  const undo: string[] = [];
  const redo: string[] = [];

  const snapshot = (): string => JSON.stringify(document);
  const remember = (): void => {
    undo.push(snapshot());
    if (undo.length > 32) {
      undo.shift();
    }
    redo.length = 0;
  };
  const restore = (encoded: string): void => {
    Object.assign(document, JSON.parse(encoded) as SpriteDocument);
    frameIndex = Math.min(frameIndex, document.frames.length - 1);
    draw();
  };
  const draw = (): void => {
    context.clearRect(0, 0, 64, 64);
    context.imageSmoothingEnabled = false;
    const scale = Math.max(1, Math.floor(Math.min(64 / document.width, 64 / document.height)));
    const offsetX = Math.floor((64 - document.width * scale) / 2);
    const offsetY = Math.floor((64 - document.height * scale) / 2);
    const previous =
      document.frames[(frameIndex + document.frames.length - 1) % document.frames.length];
    if (document.frames.length > 1 && previous !== undefined) {
      context.globalAlpha = 0.2;
      drawPixels(context, previous, document.width, document.height, scale, offsetX, offsetY);
      context.globalAlpha = 1;
    }
    drawPixels(
      context,
      document.frames[frameIndex] ?? [],
      document.width,
      document.height,
      scale,
      offsetX,
      offsetY,
    );
    context.strokeStyle = paletteCss(24);
    context.lineWidth = 1;
    if (selection !== undefined) {
      const [left, top, right, bottom] = selection;
      context.strokeRect(
        offsetX + left * scale + 0.5,
        offsetY + top * scale + 0.5,
        (right - left + 1) * scale - 1,
        (bottom - top + 1) * scale - 1,
      );
    }
    setText(root, '.frame-readout', `${String(frameIndex + 1)}/${String(document.frames.length)}`);
    setText(
      root,
      '.capacity',
      `${String(document.width * document.height * document.frames.length)} / ${String(HARDWARE.visualCapacityBytes)}B`,
    );
  };
  const point = (event: PointerEvent): [number, number] | undefined => {
    const bounds = canvas.getBoundingClientRect();
    const scale = Math.max(1, Math.floor(Math.min(64 / document.width, 64 / document.height)));
    const offsetX = Math.floor((64 - document.width * scale) / 2);
    const offsetY = Math.floor((64 - document.height * scale) / 2);
    const x = Math.floor(((event.clientX - bounds.left) * 64) / bounds.width - offsetX) / scale;
    const y = Math.floor(((event.clientY - bounds.top) * 64) / bounds.height - offsetY) / scale;
    const pixelX = Math.floor(x);
    const pixelY = Math.floor(y);
    return pixelX >= 0 && pixelY >= 0 && pixelX < document.width && pixelY < document.height
      ? [pixelX, pixelY]
      : undefined;
  };
  const paint = (event: PointerEvent): void => {
    const selected = point(event);
    const frame = document.frames[frameIndex];
    if (selected === undefined || frame === undefined) {
      return;
    }
    const [x, y] = selected;
    frame[y * document.width + x] = event.buttons === 2 ? 0 : color;
    draw();
  };
  canvas.addEventListener('contextmenu', (event) => {
    event.preventDefault();
  });
  canvas.addEventListener('pointerdown', (event) => {
    const selected = point(event);
    if (selected === undefined) {
      return;
    }
    if (selecting) {
      if (anchor === undefined) {
        anchor = selected;
      } else {
        selection = [
          Math.min(anchor[0], selected[0]),
          Math.min(anchor[1], selected[1]),
          Math.max(anchor[0], selected[0]),
          Math.max(anchor[1], selected[1]),
        ];
        anchor = undefined;
        selecting = false;
      }
      draw();
      return;
    }
    remember();
    painting = true;
    canvas.setPointerCapture(event.pointerId);
    paint(event);
  });
  canvas.addEventListener('pointermove', (event) => {
    if (painting) {
      paint(event);
    }
  });
  canvas.addEventListener('pointerup', () => {
    painting = false;
  });
  createPalette(root, (index) => {
    color = index;
  });
  const applyPng = (): void => {
    if (importedPng === undefined) return;
    const mode = (requireElement(root, '.png-mode') as HTMLSelectElement).value as
      'nearest' | 'ordered';
    const alpha = clamp(
      Number((requireElement(root, '.png-alpha') as HTMLInputElement).value),
      0,
      255,
    );
    const frame = convertRgbaToIndexed(importedPng, mode, alpha, 0);
    resizeSprite(document, importedPng.width, importedPng.height);
    document.frames[frameIndex] = [...frame];
    selection = undefined;
    draw();
  };
  (requireElement(root, '.sprite-png-input') as HTMLInputElement).addEventListener(
    'change',
    (event) => {
      const file = (event.currentTarget as HTMLInputElement).files?.[0];
      if (file === undefined) return;
      void file
        .arrayBuffer()
        .then((bytes) => decodePngRgba(new Uint8Array(bytes)))
        .then((image) => {
          if (image.width > 64 || image.height > 64)
            throw new RangeError('sprite PNG must be at most 64x64');
          remember();
          importedPng = image;
          applyPng();
          setToolStatus(root, `PNG ${String(image.width)}X${String(image.height)} PREVIEW`, false);
        })
        .catch((error: unknown) => {
          setToolStatus(root, errorMessage(error), true);
        });
    },
  );
  for (const selector of ['.png-mode', '.png-alpha'])
    requireElement(root, selector).addEventListener('change', applyPng);
  root.querySelector('[data-png-export]')?.addEventListener('click', () => {
    const width = document.width * document.frames.length;
    const indexed = new Uint8Array(width * document.height);
    for (const [index, frame] of document.frames.entries()) {
      for (let y = 0; y < document.height; y += 1)
        indexed.set(
          frame.slice(y * document.width, (y + 1) * document.width),
          y * width + index * document.width,
        );
    }
    downloadToolBytes(
      'hero.png',
      encodeRgbaPng(width, document.height, indexedRgba(indexed, true)),
      'image/png',
    );
  });
  root.querySelectorAll<HTMLElement>('[data-act]').forEach((button) => {
    button.addEventListener('click', () => {
      const action = button.dataset.act;
      if (action === 'undo') {
        const state = undo.pop();
        if (state !== undefined) {
          redo.push(snapshot());
          restore(state);
        }
        return;
      }
      if (action === 'redo') {
        const state = redo.pop();
        if (state !== undefined) {
          undo.push(snapshot());
          restore(state);
        }
        return;
      }
      if (action === 'prev' || action === 'next') {
        const direction = action === 'prev' ? -1 : 1;
        frameIndex = (frameIndex + document.frames.length + direction) % document.frames.length;
      } else if (action === 'add') {
        remember();
        document.frames.splice(frameIndex + 1, 0, [
          ...(document.frames[frameIndex] ??
            Array.from({ length: document.width * document.height }, () => 0)),
        ]);
        frameIndex += 1;
      } else if (action === 'select') {
        selecting = true;
        anchor = undefined;
      } else if (action === 'flip-h' || action === 'flip-v' || action === 'rotate') {
        remember();
        transformSprite(document, frameIndex, action, selection);
      }
      draw();
    });
  });
  root.querySelectorAll<HTMLInputElement>('[data-size]').forEach((input) => {
    input.addEventListener('change', () => {
      remember();
      resizeSprite(
        document,
        clamp(
          Number((requireElement(root, '[data-size="width"]') as HTMLInputElement).value),
          1,
          64,
        ),
        clamp(
          Number((requireElement(root, '[data-size="height"]') as HTMLInputElement).value),
          1,
          64,
        ),
      );
      selection = undefined;
      draw();
    });
  });
  bindSave(root, async () => {
    project.files[path] = encodeAssetFile(document);
    project.manifest = upsertAsset(
      project.manifest,
      'hero',
      document.frames.length > 1 ? 'animation' : 'sprite',
      path,
    );
    await callbacks.save();
  });
  draw();
}

interface MapDocument {
  revision: 1;
  kind: 'map';
  layers: { width: number; height: number; cells: number[]; tileSet: string }[];
}

interface TileSetDocument {
  revision: 1;
  kind: 'tile_set';
  tiles: number[][];
  flags: number[];
}

async function openMapEditor(
  root: HTMLElement,
  project: ToolProject,
  callbacks: ToolCallbacks,
): Promise<void> {
  const manifest = await callbacks.parseManifest();
  const mapEntry = Object.entries(manifest.assets)
    .filter(([, asset]) => asset.kind === 'map')
    .sort(([left], [right]) => left.localeCompare(right))[0];
  const mapName = mapEntry?.[0] ?? 'room';
  const path = mapEntry?.[1].path ?? 'assets/room.m01m';
  const declaredTileSets = Object.entries(manifest.assets)
    .filter(([, asset]) => asset.kind === 'tile_set')
    .sort(([left], [right]) => left.localeCompare(right));
  const declaredSprites = Object.entries(manifest.assets)
    .filter(([, asset]) => asset.kind === 'sprite' || asset.kind === 'animation')
    .sort(([left], [right]) => left.localeCompare(right));
  if (declaredTileSets.length === 0)
    declaredTileSets.push(['tiles', { kind: 'tile_set', path: 'assets/tiles.m01g' }]);
  const tileSets = new Map<string, { path: string; document: TileSetDocument }>();
  for (const [name, asset] of declaredTileSets) {
    const loaded = readJson<TileSetDocument>(project.files[asset.path], {
      revision: 1,
      kind: 'tile_set',
      tiles: defaultTiles(),
      flags: [0, 1, 2, 3],
    });
    loaded.flags = loaded.tiles.map((_tile, index) => loaded.flags[index] ?? 0);
    tileSets.set(name, {
      path: asset.path,
      document: loaded,
    });
  }
  const firstTileSet = tileSets.keys().next().value;
  if (firstTileSet === undefined) throw new Error('map editor requires a tile set');
  const document = readJson<MapDocument>(project.files[path], {
    revision: 1,
    kind: 'map',
    layers: [
      {
        width: 30,
        height: 18,
        cells: Array.from({ length: 540 }, () => 0),
        tileSet: firstTileSet,
      },
    ],
  });
  for (const layer of document.layers) {
    if (!tileSets.has(layer.tileSet)) {
      tileSets.set(layer.tileSet, {
        path: `assets/${layer.tileSet}.m01g`,
        document: {
          revision: 1,
          kind: 'tile_set',
          tiles: defaultTiles(),
          flags: [0, 1, 2, 3],
        },
      });
    }
  }
  const editedBytes = (): number =>
    document.layers.reduce((total, layer) => total + layer.cells.length * 2, 0) +
    [...tileSets.values()].reduce(
      (total, tileSet) =>
        total + tileSet.document.tiles.length * 64 + tileSet.document.flags.length,
      0,
    );
  let otherVisualBytes = 0;
  try {
    otherVisualBytes = Math.max(
      0,
      decodeRuntimeAssets(manifest.assets, project.files, manifest.display).visualBytes -
        editedBytes(),
    );
  } catch {
    // A fresh/incomplete project is still editable; save will create the required assets.
  }
  root.innerHTML = toolFrame(
    'LAYERED TILE MAP',
    `<div class="map-layout">
      <canvas class="map-canvas" width="180" height="108" aria-label="Tile map"></canvas>
      <aside class="map-controls">
        <p>${mapName.toUpperCase()} <span class="layer-readout"></span> <span class="layer-mode"></span></p>
        <label>ATLAS <select class="tileset-select"></select></label>
        <div><select class="tile-png-mode" aria-label="Tile PNG conversion mode"><option value="nearest">NEAR</option><option value="ordered">DITHER</option></select><button data-map="png-out">PNG OUT</button><label class="file-button">PNG IN<input class="tile-png-input" type="file" accept="image/png,.png"></label></div>
        <div class="tile-picks"></div>
        <div><button data-map="prev">&lt;L</button><button data-map="next">L&gt;</button><button data-map="add">+L</button><button data-map="remove">-L</button><button data-map="visible">EYE</button></div>
        <div><button data-map="up">UP</button><button data-map="down">DOWN</button><button data-map="select">SEL</button><button data-map="copy">COPY</button><button data-map="stamp">STAMP</button><button data-map="move">MOVE</button><button data-map="fill">FILL</button></div>
        <div><button data-map="undo">UNDO</button><button data-map="redo">REDO</button></div>
        <div><label>W <input class="map-width" type="number" min="1" max="256"></label><label>H <input class="map-height" type="number" min="1" max="256"></label><button data-map="resize">SIZE</button></div>
        <label>FLAG <select class="flag-select"><option value="0">0</option><option value="1">1</option><option value="2">2</option><option value="3">3</option></select></label>
        <button data-map="toggle-flag">TOGGLE</button>
        <p class="capacity"></p>
      </aside>
    </div>`,
  );
  bindCommon(root, callbacks);
  const canvas = requireElement(root, '.map-canvas') as HTMLCanvasElement;
  const context = requireContext(canvas);
  const tileSetSelect = requireElement(root, '.tileset-select') as HTMLSelectElement;
  for (const name of tileSets.keys()) {
    const nativeOption = root.ownerDocument.createElement('option');
    nativeOption.value = name;
    nativeOption.textContent = name.toUpperCase();
    tileSetSelect.append(nativeOption);
  }
  for (const [name] of declaredSprites) {
    const nativeOption = root.ownerDocument.createElement('option');
    nativeOption.value = `sprite:${name}`;
    nativeOption.textContent = `${name.toUpperCase()} SPRITE`;
    tileSetSelect.append(nativeOption);
  }
  let layer = 0;
  let selectedTile = 0;
  let painting = false;
  let mode: 'paint' | 'select' | 'stamp' | 'move' | 'fill' = 'paint';
  let selection: [number, number, number, number] | undefined;
  let anchor: [number, number] | undefined;
  let clipboard: { width: number; height: number; cells: number[] } | undefined;
  const visible = new Set(document.layers.map((_entry, index) => index));
  const undo: string[] = [];
  const redo: string[] = [];
  const snapshot = (): string =>
    JSON.stringify({
      document,
      tileSets: Object.fromEntries(
        [...tileSets].map(([name, entry]) => [
          name,
          { path: entry.path, document: entry.document },
        ]),
      ),
      visible: [...visible],
    });
  const restore = (value: string): void => {
    const state = JSON.parse(value) as {
      document: MapDocument;
      tileSets: Record<string, { path: string; document: TileSetDocument }>;
      visible: number[];
    };
    Object.assign(document, state.document);
    tileSets.clear();
    for (const [name, entry] of Object.entries(state.tileSets)) tileSets.set(name, entry);
    visible.clear();
    for (const index of state.visible) visible.add(index);
    layer = Math.min(layer, document.layers.length - 1);
    selection = undefined;
    refreshTilePicks();
    draw();
  };
  const remember = (): void => {
    undo.push(snapshot());
    if (undo.length > 32) undo.shift();
    redo.length = 0;
  };
  const draw = (): void => {
    const current = document.layers[layer];
    if (current === undefined) return;
    context.fillStyle = paletteCss(1);
    context.fillRect(0, 0, canvas.width, canvas.height);
    const scale = mapScale(canvas, current);
    for (const [layerIndex, mapLayer] of document.layers.entries()) {
      if (!visible.has(layerIndex)) continue;
      const tileSet = tileSets.get(mapLayer.tileSet)?.document;
      if (tileSet === undefined) continue;
      for (let y = 0; y < mapLayer.height; y += 1) {
        for (let x = 0; x < mapLayer.width; x += 1) {
          const tile = mapLayer.cells[y * mapLayer.width + x] ?? 0;
          context.fillStyle = paletteCss(tileColor(tileSet.tiles[tile]));
          context.fillRect(x * scale, y * scale, scale, scale);
        }
      }
    }
    if (selection !== undefined) {
      const [left, top, right, bottom] = selection;
      context.strokeStyle = paletteCss(24);
      context.strokeRect(
        left * scale + 0.5,
        top * scale + 0.5,
        (right - left + 1) * scale - 1,
        (bottom - top + 1) * scale - 1,
      );
    }
    tileSetSelect.value = current.tileSet;
    refreshPixelSelect(tileSetSelect);
    (requireElement(root, '.map-width') as HTMLInputElement).value = String(current.width);
    (requireElement(root, '.map-height') as HTMLInputElement).value = String(current.height);
    setText(root, '.layer-readout', `${String(layer + 1)}/${String(document.layers.length)}`);
    setText(root, '.layer-mode', `${visible.has(layer) ? 'ON' : 'OFF'} ${mode.toUpperCase()}`);
    setText(
      root,
      '.capacity',
      `${String(otherVisualBytes + editedBytes())}/${String(HARDWARE.visualCapacityBytes)}B`,
    );
  };
  const point = (event: PointerEvent): [number, number] | undefined => {
    const current = document.layers[layer];
    if (current === undefined) return undefined;
    const bounds = canvas.getBoundingClientRect();
    const scale = mapScale(canvas, current);
    const x = Math.floor(((event.clientX - bounds.left) * canvas.width) / bounds.width / scale);
    const y = Math.floor(((event.clientY - bounds.top) * canvas.height) / bounds.height / scale);
    return x >= 0 && y >= 0 && x < current.width && y < current.height ? [x, y] : undefined;
  };
  const applyAt = (selected: [number, number]): void => {
    const current = document.layers[layer];
    if (current === undefined) return;
    const [x, y] = selected;
    if (mode === 'select') {
      if (anchor === undefined) anchor = selected;
      else {
        selection = [
          Math.min(anchor[0], x),
          Math.min(anchor[1], y),
          Math.max(anchor[0], x),
          Math.max(anchor[1], y),
        ];
        anchor = undefined;
        mode = 'paint';
      }
    } else if (mode === 'fill') {
      floodMap(current, x, y, selectedTile);
      mode = 'paint';
    } else if ((mode === 'stamp' || mode === 'move') && clipboard !== undefined) {
      if (mode === 'move' && selection !== undefined) clearMapRegion(current, selection);
      stampMap(current, clipboard, x, y);
      selection = [x, y, x + clipboard.width - 1, y + clipboard.height - 1];
      mode = 'paint';
    } else current.cells[y * current.width + x] = selectedTile;
    draw();
  };
  const paint = (event: PointerEvent): void => {
    const selected = point(event);
    if (selected !== undefined) applyAt(selected);
  };
  canvas.addEventListener('pointerdown', (event) => {
    if (point(event) === undefined) return;
    remember();
    painting = mode === 'paint';
    canvas.setPointerCapture(event.pointerId);
    paint(event);
  });
  canvas.addEventListener('pointermove', (event) => {
    if (painting) paint(event);
  });
  canvas.addEventListener('pointerup', () => {
    painting = false;
  });
  function refreshTilePicks(): void {
    const picks = requireElement(root, '.tile-picks');
    picks.replaceChildren();
    const current = document.layers[layer];
    const tiles = current === undefined ? undefined : tileSets.get(current.tileSet)?.document.tiles;
    if (tiles === undefined) return;
    selectedTile = Math.min(selectedTile, tiles.length - 1);
    for (let index = 0; index < tiles.length; index += 1) {
      const button = documentElement('button', String(index));
      button.style.background = paletteCss(tileColor(tiles[index]));
      button.addEventListener('click', () => {
        selectedTile = index;
      });
      picks.append(button);
    }
  }
  tileSetSelect.addEventListener('change', () => {
    const current = document.layers[layer];
    let selectedName = tileSetSelect.value;
    if (selectedName.startsWith('sprite:')) {
      const spriteName = selectedName.slice('sprite:'.length);
      const spriteAsset = manifest.assets[spriteName];
      if (spriteAsset === undefined) return;
      const source = readJson<SpriteDocument>(project.files[spriteAsset.path], {
        revision: 1,
        kind: 'sprite',
        width: 8,
        height: 8,
        frames: [Array.from({ length: 64 }, () => 0)],
      });
      const baseName = `${spriteName}_tiles`;
      selectedName = baseName;
      if (!tileSets.has(selectedName)) {
        tileSets.set(selectedName, {
          path: `assets/${selectedName}.m01g`,
          document: spriteToTileSet(source),
        });
        const nativeOption = root.ownerDocument.createElement('option');
        nativeOption.value = selectedName;
        nativeOption.textContent = `${spriteName.toUpperCase()} TILES`;
        tileSetSelect.append(nativeOption);
      }
    }
    const selected = tileSets.get(selectedName)?.document;
    if (current === undefined || selected === undefined) return;
    remember();
    current.tileSet = selectedName;
    current.cells = current.cells.map((cell) => (cell < selected.tiles.length ? cell : 0));
    selectedTile = 0;
    refreshTilePicks();
    draw();
  });
  (requireElement(root, '.tile-png-input') as HTMLInputElement).addEventListener(
    'change',
    (event) => {
      const file = (event.currentTarget as HTMLInputElement).files?.[0];
      if (file === undefined) return;
      void file
        .arrayBuffer()
        .then((bytes) => decodePngRgba(new Uint8Array(bytes)))
        .then((image) => {
          if (image.width % 8 !== 0 || image.height % 8 !== 0)
            throw new RangeError('tile PNG dimensions must be multiples of 8');
          const count = (image.width / 8) * (image.height / 8);
          if (count < 1 || count > 4096) throw new RangeError('tile PNG must contain 1-4096 tiles');
          const current = document.layers[layer];
          const tileSet =
            current === undefined ? undefined : tileSets.get(current.tileSet)?.document;
          if (current === undefined || tileSet === undefined)
            throw new Error('selected atlas is missing');
          const proposed =
            otherVisualBytes + editedBytes() - tileSet.tiles.length * 65 + count * 65;
          if (proposed > HARDWARE.visualCapacityBytes)
            throw new RangeError('tile PNG exceeds visual capacity');
          const mode = (requireElement(root, '.tile-png-mode') as HTMLSelectElement).value as
            'nearest' | 'ordered';
          const indexed = convertRgbaToIndexed(image, mode, 127, 0);
          remember();
          tileSet.tiles = [];
          for (let tileY = 0; tileY < image.height / 8; tileY += 1) {
            for (let tileX = 0; tileX < image.width / 8; tileX += 1) {
              const tile = Array.from({ length: 64 }, (_, pixel) => {
                const x = tileX * 8 + (pixel % 8);
                const y = tileY * 8 + Math.floor(pixel / 8);
                return indexed[y * image.width + x] ?? 0;
              });
              tileSet.tiles.push(tile);
            }
          }
          tileSet.flags = tileSet.tiles.map((_tile, index) => tileSet.flags[index] ?? 0);
          for (const mapLayer of document.layers) {
            if (mapLayer.tileSet === current.tileSet)
              mapLayer.cells = mapLayer.cells.map((cell) => (cell < count ? cell : 0));
          }
          selectedTile = 0;
          refreshTilePicks();
          draw();
          setToolStatus(root, `ATLAS IMPORTED ${String(count)} TILES`, false);
        })
        .catch((error: unknown) => {
          setToolStatus(root, errorMessage(error), true);
        });
    },
  );
  root.querySelectorAll<HTMLElement>('[data-map]').forEach((button) => {
    button.addEventListener('click', () => {
      const action = button.dataset.map;
      if (action === 'undo' || action === 'redo') {
        const from = action === 'undo' ? undo : redo;
        const to = action === 'undo' ? redo : undo;
        const state = from.pop();
        if (state !== undefined) {
          to.push(snapshot());
          restore(state);
        }
        return;
      }
      if (action === 'png-out') {
        const current = document.layers[layer];
        const tiles =
          current === undefined ? undefined : tileSets.get(current.tileSet)?.document.tiles;
        if (current === undefined || tiles === undefined) return;
        const columns = Math.min(16, tiles.length);
        const rows = Math.ceil(tiles.length / columns);
        const indexed = new Uint8Array(columns * 8 * rows * 8);
        for (const [index, tile] of tiles.entries()) {
          const tileX = (index % columns) * 8;
          const tileY = Math.floor(index / columns) * 8;
          for (let y = 0; y < 8; y += 1)
            indexed.set(tile.slice(y * 8, (y + 1) * 8), (tileY + y) * columns * 8 + tileX);
        }
        downloadToolBytes(
          `${current.tileSet}.png`,
          encodeRgbaPng(columns * 8, rows * 8, indexedRgba(indexed, true)),
          'image/png',
        );
        return;
      }
      if (action === 'prev' || action === 'next') {
        layer =
          (layer + document.layers.length + (action === 'prev' ? -1 : 1)) % document.layers.length;
        selection = undefined;
        refreshTilePicks();
      } else if (action === 'add' && document.layers.length < 8) {
        const base = document.layers[0];
        if (base !== undefined) {
          if (
            otherVisualBytes + editedBytes() + base.cells.length * 2 >
            HARDWARE.visualCapacityBytes
          ) {
            setToolStatus(root, 'LAYER EXCEEDS VISUAL CAPACITY', true);
            return;
          }
          remember();
          document.layers.push({
            width: base.width,
            height: base.height,
            cells: Array.from({ length: base.cells.length }, () => 0),
            tileSet: tileSetSelect.value,
          });
          layer = document.layers.length - 1;
          visible.add(layer);
          refreshTilePicks();
        }
      } else if (action === 'remove' && document.layers.length > 1) {
        remember();
        document.layers.splice(layer, 1);
        layer = Math.min(layer, document.layers.length - 1);
        visible.clear();
        for (let index = 0; index < document.layers.length; index += 1) visible.add(index);
        refreshTilePicks();
      } else if (action === 'visible') {
        if (visible.has(layer)) visible.delete(layer);
        else visible.add(layer);
      } else if (
        (action === 'up' && layer > 0) ||
        (action === 'down' && layer + 1 < document.layers.length)
      ) {
        remember();
        const target = layer + (action === 'up' ? -1 : 1);
        const current = document.layers[layer];
        const other = document.layers[target];
        if (current !== undefined && other !== undefined) {
          document.layers[layer] = other;
          document.layers[target] = current;
          layer = target;
        }
      } else if (action === 'select') {
        mode = 'select';
        anchor = undefined;
      } else if (action === 'copy' && selection !== undefined) {
        const current = document.layers[layer];
        if (current !== undefined) clipboard = copyMapRegion(current, selection);
      } else if (action === 'stamp' || action === 'move') {
        if (selection !== undefined && clipboard === undefined) {
          const current = document.layers[layer];
          if (current !== undefined) clipboard = copyMapRegion(current, selection);
        }
        if (clipboard !== undefined) mode = action;
      } else if (action === 'fill') {
        mode = 'fill';
      } else if (action === 'resize') {
        const current = document.layers[layer];
        if (current !== undefined) {
          const width = clamp(
            Number((requireElement(root, '.map-width') as HTMLInputElement).value),
            1,
            256,
          );
          const height = clamp(
            Number((requireElement(root, '.map-height') as HTMLInputElement).value),
            1,
            256,
          );
          const proposed =
            otherVisualBytes + editedBytes() - current.cells.length * 2 + width * height * 2;
          if (proposed > HARDWARE.visualCapacityBytes) {
            setToolStatus(root, 'RESIZE EXCEEDS VISUAL CAPACITY', true);
            draw();
            return;
          }
          remember();
          resizeMapLayer(current, width, height);
          selection = undefined;
        }
      } else if (action === 'toggle-flag') {
        remember();
        const flag = Number((requireElement(root, '.flag-select') as HTMLSelectElement).value);
        const current = document.layers[layer];
        const tiles = current === undefined ? undefined : tileSets.get(current.tileSet)?.document;
        if (tiles !== undefined)
          tiles.flags[selectedTile] = (tiles.flags[selectedTile] ?? 0) ^ (1 << flag);
      }
      draw();
    });
  });
  bindSave(root, async () => {
    project.files[path] = encodeAssetFile(document);
    project.manifest = upsertAsset(project.manifest, mapName, 'map', path);
    for (const [name, tileSet] of tileSets) {
      project.files[tileSet.path] = encodeAssetFile(tileSet.document);
      project.manifest = upsertAsset(project.manifest, name, 'tile_set', tileSet.path);
    }
    await callbacks.save();
  });
  refreshTilePicks();
  draw();
}

interface PaletteDocument {
  revision: 1;
  kind: 'display';
  remap: number[];
  raster: { line: number; scrollX: number; scrollY: number; remap: number[] }[];
}

interface CartridgeIdentityDocument {
  revision: 1;
  year: number;
  players: number;
  controls: string;
}

function openPaletteEditor(
  root: HTMLElement,
  project: ToolProject,
  callbacks: ToolCallbacks,
): void {
  const path = 'assets/display.m01p';
  const document = readJson<PaletteDocument>(project.files[path], {
    revision: 1,
    kind: 'display',
    remap: Array.from({ length: 32 }, (_, index) => index),
    raster: [
      { line: 96, scrollX: 0, scrollY: 0, remap: Array.from({ length: 32 }, (_, index) => index) },
    ],
  });
  root.innerHTML = toolFrame(
    'PALETTE / RASTER',
    `<div class="palette-tool">
      <canvas class="raster-preview" width="240" height="144" aria-label="Raster timeline preview"></canvas>
      <p>MASTER PALETTE / LOGICAL <input class="logical-index-input" aria-label="Logical palette index" type="number" min="0" max="31" value="0"> -&gt; <span class="physical-index">0</span></p>
      <div class="master-palette"></div>
      <label>RASTER LINE <input class="raster-line" type="range" min="0" max="143" value="${String(document.raster[0]?.line ?? 96)}"></label>
      <div class="raster-fields"><label><input class="raster-enabled" type="checkbox"> KEYFRAME</label><label><input class="edit-default" type="checkbox"> DEFAULT</label><label>X <input class="scroll-x" type="number" min="-32768" max="32767"></label><label>Y <input class="scroll-y" type="number" min="-32768" max="32767"></label></div>
      <div class="raster-range"><label>FROM <input class="range-start" type="number" min="0" max="143" value="0"></label><label>TO <input class="range-end" type="number" min="0" max="143" value="143"></label><button data-raster="copy">COPY</button><button data-raster="paste">PASTE</button><button data-raster="fill">FILL</button><button data-raster="interpolate">TWEEN</button><button data-raster="undo">UNDO</button><button data-raster="redo">REDO</button></div>
      <p class="raster-readout"></p>
    </div>`,
  );
  bindCommon(root, callbacks);
  const preview = requireElement(root, '.raster-preview') as HTMLCanvasElement;
  const previewContext = requireContext(preview);
  const lineInput = requireElement(root, '.raster-line') as HTMLInputElement;
  const enabledInput = requireElement(root, '.raster-enabled') as HTMLInputElement;
  const defaultInput = requireElement(root, '.edit-default') as HTMLInputElement;
  const scrollXInput = requireElement(root, '.scroll-x') as HTMLInputElement;
  const scrollYInput = requireElement(root, '.scroll-y') as HTMLInputElement;
  const logicalInput = requireElement(root, '.logical-index-input') as HTMLInputElement;
  const undo: string[] = [];
  const redo: string[] = [];
  let clipboard: PaletteDocument['raster'][number] | undefined;
  let syncing = false;
  const snapshot = (): string => JSON.stringify(document);
  const remember = (): void => {
    undo.push(snapshot());
    if (undo.length > 32) undo.shift();
    redo.length = 0;
  };
  const restore = (encoded: string): void => {
    Object.assign(document, JSON.parse(encoded) as PaletteDocument);
    syncControls();
    drawRasterPreview(previewContext, document);
  };
  const selectedLine = (): number => clamp(Number(lineInput.value), 0, 143);
  const rowAt = (line: number): PaletteDocument['raster'][number] | undefined =>
    document.raster.find((row) => row.line === line);
  const effectiveAt = (line: number): PaletteDocument['raster'][number] => {
    let state: PaletteDocument['raster'][number] = {
      line,
      scrollX: 0,
      scrollY: 0,
      remap: [...document.remap],
    };
    for (const row of document.raster) {
      if (row.line > line) break;
      state = { line, scrollX: row.scrollX, scrollY: row.scrollY, remap: [...row.remap] };
    }
    return state;
  };
  const ensureRow = (line: number): PaletteDocument['raster'][number] => {
    const existing = rowAt(line);
    if (existing !== undefined) return existing;
    const row = effectiveAt(line);
    document.raster.push(row);
    document.raster.sort((left, right) => left.line - right.line);
    return row;
  };
  const currentState = (): PaletteDocument['raster'][number] => {
    if (defaultInput.checked)
      return { line: selectedLine(), scrollX: 0, scrollY: 0, remap: document.remap };
    return rowAt(selectedLine()) ?? effectiveAt(selectedLine());
  };
  const syncControls = (): void => {
    syncing = true;
    const line = selectedLine();
    const row = rowAt(line);
    const state = defaultInput.checked ? currentState() : (row ?? effectiveAt(line));
    enabledInput.checked = row !== undefined;
    enabledInput.disabled = defaultInput.checked;
    scrollXInput.value = String(state.scrollX);
    scrollYInput.value = String(state.scrollY);
    scrollXInput.disabled = defaultInput.checked;
    scrollYInput.disabled = defaultInput.checked;
    const logical = clamp(Number(logicalInput.value), 0, 31);
    logicalInput.value = String(logical);
    setText(root, '.physical-index', String(state.remap[logical] ?? logical));
    setText(
      root,
      '.raster-readout',
      `LINE ${String(line).padStart(3, '0')} ${row === undefined ? 'PASS' : 'KEY'} ${String(document.raster.length)}/144 / ${String(32 + document.raster.length * 38)}B`,
    );
    syncing = false;
  };
  const changed = (): void => {
    syncControls();
    drawRasterPreview(previewContext, document);
  };
  const palette = requireElement(root, '.master-palette');
  for (let index = 0; index < 32; index += 1) {
    const button = documentElement('button', String(index));
    button.style.background = paletteCss(index);
    button.title = `master color ${String(index)}`;
    button.addEventListener('click', () => {
      remember();
      const logical = clamp(Number(logicalInput.value), 0, 31);
      const state = defaultInput.checked ? currentState() : ensureRow(selectedLine());
      state.remap[logical] = index;
      changed();
    });
    palette.append(button);
  }
  lineInput.addEventListener('input', syncControls);
  logicalInput.addEventListener('change', syncControls);
  defaultInput.addEventListener('change', syncControls);
  enabledInput.addEventListener('change', () => {
    if (syncing) return;
    remember();
    const line = selectedLine();
    if (enabledInput.checked) ensureRow(line);
    else document.raster = document.raster.filter((row) => row.line !== line);
    changed();
  });
  for (const input of [scrollXInput, scrollYInput]) {
    input.addEventListener('change', () => {
      if (syncing || defaultInput.checked) return;
      remember();
      const row = ensureRow(selectedLine());
      row.scrollX = clamp(Number(scrollXInput.value), -32768, 32767);
      row.scrollY = clamp(Number(scrollYInput.value), -32768, 32767);
      changed();
    });
  }
  root.querySelectorAll<HTMLElement>('[data-raster]').forEach((button) => {
    button.addEventListener('click', () => {
      const action = button.dataset.raster;
      if (action === 'undo' || action === 'redo') {
        const from = action === 'undo' ? undo : redo;
        const to = action === 'undo' ? redo : undo;
        const state = from.pop();
        if (state !== undefined) {
          to.push(snapshot());
          restore(state);
        }
        return;
      }
      if (action === 'copy') {
        clipboard = structuredClone(currentState());
        setToolStatus(root, `COPIED LINE ${String(selectedLine())}`, false);
        return;
      }
      const start = clamp(
        Number((requireElement(root, '.range-start') as HTMLInputElement).value),
        0,
        143,
      );
      const end = clamp(
        Number((requireElement(root, '.range-end') as HTMLInputElement).value),
        start,
        143,
      );
      remember();
      if (action === 'paste' && clipboard !== undefined) {
        const row = ensureRow(selectedLine());
        row.scrollX = clipboard.scrollX;
        row.scrollY = clipboard.scrollY;
        row.remap = [...clipboard.remap];
      } else if (action === 'fill') {
        const source = structuredClone(currentState());
        for (let line = start; line <= end; line += 1) {
          const row = ensureRow(line);
          row.scrollX = source.scrollX;
          row.scrollY = source.scrollY;
          row.remap = [...source.remap];
        }
      } else if (action === 'interpolate') {
        const first = effectiveAt(start);
        const last = effectiveAt(end);
        const distance = Math.max(1, end - start);
        for (let line = start; line <= end; line += 1) {
          const amount = (line - start) / distance;
          const row = ensureRow(line);
          row.scrollX = Math.round(first.scrollX + (last.scrollX - first.scrollX) * amount);
          row.scrollY = Math.round(first.scrollY + (last.scrollY - first.scrollY) * amount);
          row.remap = amount < 0.5 ? [...first.remap] : [...last.remap];
        }
      }
      changed();
    });
  });
  bindSave(root, async () => {
    document.raster.sort((left, right) => left.line - right.line);
    project.files[path] = encodeAssetFile(document);
    project.manifest = upsertTopLevel(project.manifest, 'display', path);
    await callbacks.save();
  });
  syncControls();
  drawRasterPreview(previewContext, document);
}

function openSoundEditor(root: HTMLElement, project: ToolProject, callbacks: ToolCallbacks): void {
  const path = 'assets/blip.m01s';
  const document = readJson<Record<string, unknown>>(project.files[path], defaultSound());
  root.innerHTML = toolFrame('SOUND EFFECT / 8V SYNTH', soundControls(document));
  bindCommon(root, callbacks);
  (requireElement(root, '[name="wave"]') as HTMLSelectElement).value =
    typeof document.waveform === 'string' ? document.waveform : 'pulse';
  const read = (): SoundAsset => soundFromControls(root);
  const drawScope = (): void => {
    drawWavScope(root, renderSoundWav(read()));
  };
  root.querySelector('[data-preview]')?.addEventListener('click', () => {
    void previewSound(read()).catch((error: unknown) => {
      setToolStatus(root, errorMessage(error), true);
    });
  });
  root.querySelector('[data-wav]')?.addEventListener('click', () => {
    const wav = renderSoundWav(read());
    drawWavScope(root, wav);
    downloadToolBytes('blip.wav', wav, 'audio/wav');
  });
  root.querySelector('.sound-controls')?.addEventListener('input', drawScope);
  bindSave(root, async () => {
    project.files[path] = encodeSoundAssetFile(read());
    project.manifest = upsertAsset(project.manifest, 'blip', 'sound', path);
    await callbacks.save();
  });
  drawScope();
}

function openMusicEditor(root: HTMLElement, project: ToolProject, callbacks: ToolCallbacks): void {
  const path = 'assets/theme.m01t';
  const existing = readJson<Record<string, unknown> | undefined>(project.files[path], undefined);
  let song = musicDocument(existing);
  root.innerHTML = toolFrame(
    '8-CHANNEL TRACKER',
    `<div class="tracker">
      <div class="tracker-head"><label>F/ROW <input class="tempo" type="number" min="1" max="60" value="${String(numberValue(existing?.framesPerRow, 6))}"></label><label><input class="loop" type="checkbox"${existing?.loop === false ? '' : ' checked'}> LOOP</label><button data-preview>PLAY</button><button data-music-wav>WAV</button><label>PAT <select class="pattern-select"></select></label><button data-add-pattern title="Add pattern">+PAT</button></div>
      <div class="tracker-order"><label>ORDER <input value=""></label><button data-track-undo>UNDO</button><button data-track-redo>REDO</button></div>
      <div class="tracker-grid" role="grid"></div>
    </div>`,
  );
  bindCommon(root, callbacks);
  const grid = requireElement(root, '.tracker-grid');
  const patternSelect = requireElement(root, '.pattern-select') as HTMLSelectElement;
  const orderInput = requireElement(root, '.tracker-order input') as HTMLInputElement;
  orderInput.value = song.order.join(' ');
  interface HistoryEntry {
    readonly song: EditableMusicDocument;
    readonly order: string;
    readonly selected: string;
  }
  const undo: HistoryEntry[] = [];
  const redo: HistoryEntry[] = [];
  const capture = (order = orderInput.value): HistoryEntry => ({
    song: structuredClone(song),
    order,
    selected: patternSelect.value,
  });
  const remember = (entry = capture()): void => {
    undo.push(entry);
    if (undo.length > 32) undo.shift();
    redo.length = 0;
  };
  const refreshPatternSelect = (selected: string): void => {
    patternSelect.replaceChildren();
    for (const name of Object.keys(song.patterns)) {
      const option = document.createElement('option');
      option.textContent = name;
      option.value = name;
      patternSelect.append(option);
    }
    patternSelect.value = selected;
  };
  const renderPattern = (): void => {
    const name = patternSelect.value;
    const rows = song.patterns[name];
    if (rows === undefined) throw new Error(`pattern '${name}' is missing`);
    grid.replaceChildren();
    grid.setAttribute('aria-label', `Pattern ${name}`);
    for (let row = 0; row < 16; row += 1) {
      const label = documentElement('span', row.toString(16).toUpperCase().padStart(2, '0'));
      grid.append(label);
      for (let channel = 0; channel < 8; channel += 1) {
        const button = documentElement('button', noteLabel(rows[row]?.[channel]?.note ?? null));
        button.setAttribute('role', 'gridcell');
        button.addEventListener('click', () => {
          remember();
          const current = rows[row]?.[channel] ?? null;
          const notes = [null, 48, 55, 60, 64, 67, 72] as const;
          const note = current?.note ?? null;
          const next = notes[(notes.indexOf(note as (typeof notes)[number]) + 1) % notes.length];
          const target = rows[row];
          if (target !== undefined) {
            target[channel] =
              next === null || next === undefined
                ? null
                : { ...current, note: next, sound: current?.sound ?? 'blip' };
          }
          button.textContent = noteLabel(next ?? null);
        });
        grid.append(button);
      }
    }
  };
  refreshPatternSelect(Object.keys(song.patterns)[0] ?? '00');
  renderPattern();
  const restore = (entry: HistoryEntry): void => {
    song = structuredClone(entry.song);
    orderInput.value = entry.order;
    const selected =
      song.patterns[entry.selected] === undefined
        ? (Object.keys(song.patterns)[0] ?? '00')
        : entry.selected;
    refreshPatternSelect(selected);
    renderPattern();
  };
  const stepHistory = (from: HistoryEntry[], to: HistoryEntry[]): void => {
    const entry = from.pop();
    if (entry === undefined) return;
    to.push(capture());
    restore(entry);
  };
  patternSelect.addEventListener('change', renderPattern);
  let orderBefore = orderInput.value;
  orderInput.addEventListener('focus', () => {
    orderBefore = orderInput.value;
  });
  orderInput.addEventListener('change', () => {
    if (orderInput.value !== orderBefore) remember(capture(orderBefore));
  });
  root.querySelector('[data-track-undo]')?.addEventListener('click', () => {
    stepHistory(undo, redo);
  });
  root.querySelector('[data-track-redo]')?.addEventListener('click', () => {
    stepHistory(redo, undo);
  });
  (requireElement(root, '.asset-tool') as HTMLElement).addEventListener('keydown', (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') {
      event.preventDefault();
      stepHistory(event.shiftKey ? redo : undo, event.shiftKey ? undo : redo);
    }
  });
  root.querySelector('[data-add-pattern]')?.addEventListener('click', () => {
    const name = nextPatternName(song.patterns);
    if (name === undefined) {
      setToolStatus(root, 'PATTERN LIMIT REACHED', true);
      return;
    }
    remember();
    song.patterns[name] = emptyMusicRows();
    orderInput.value = `${orderInput.value.trim()} ${name}`.trim();
    refreshPatternSelect(name);
    renderPattern();
    setToolStatus(root, `PATTERN ${name} ADDED`, false);
  });
  const read = (): MusicAsset => musicFromDocument(root, song);
  root.querySelector('[data-preview]')?.addEventListener('click', () => {
    void callbacks
      .parseManifest()
      .then((manifest) => previewMusic(read(), musicSounds(project, manifest)))
      .catch((error: unknown) => {
        setToolStatus(root, errorMessage(error), true);
      });
  });
  root.querySelector('[data-music-wav]')?.addEventListener('click', () => {
    void callbacks
      .parseManifest()
      .then((manifest) => renderMusicWav(read(), musicSounds(project, manifest)))
      .then((wav) => {
        downloadToolBytes('theme.wav', wav, 'audio/wav');
      })
      .catch((error: unknown) => {
        setToolStatus(root, errorMessage(error), true);
      });
  });
  bindSave(root, async () => {
    const music = read();
    const usesBlip = Object.values(music.patterns).some((pattern) =>
      pattern.rows.some((row) => row.some((cell) => cell?.sound === 'blip')),
    );
    if (usesBlip && project.files['assets/blip.m01s'] === undefined) {
      project.files['assets/blip.m01s'] = encodeAssetFile(defaultSound());
      project.manifest = upsertAsset(project.manifest, 'blip', 'sound', 'assets/blip.m01s');
    }
    project.files[path] = encodeMusicAssetFile(music);
    project.manifest = upsertAsset(project.manifest, 'theme', 'music', path);
    await callbacks.save();
  });
}

async function openProjectSettings(
  root: HTMLElement,
  project: ToolProject,
  callbacks: ToolCallbacks,
): Promise<void> {
  const manifest = await callbacks.parseManifest();
  const identity = readJson<CartridgeIdentityDocument>(
    project.files['presentation/cartridge.json'],
    { revision: 1, year: 1999, players: 1, controls: 'PAD' },
  );
  root.innerHTML = toolFrame(
    'CARTRIDGE SETTINGS',
    `<form class="project-settings">
      <label>ID <input value="" disabled></label>
      <label>TITLE <input name="title" maxlength="64"></label>
      <label>AUTHOR <input name="author" maxlength="64"></label>
      <label>VERSION <input name="version" maxlength="32"></label>
      <label>UPDATE <select name="update"><option value="60">60 HZ</option><option value="30">30 HZ</option></select></label>
      <label>YEAR <input name="year" type="number" min="1970" max="9999"></label>
      <label>PLAYERS <input name="players" type="number" min="1" max="4"></label>
      <label>CONTROLS <input name="controls" maxlength="64"></label>
      <div><label class="file-button">LABEL IN<input class="label-file-input" type="file" accept="image/png,.png"></label><button type="button" data-label-export>LABEL OUT</button></div>
      <p>SOURCE AND CARTRIDGE OWNERSHIP REMAIN WITH THE AUTHOR.</p>
    </form>`,
  );
  bindCommon(root, callbacks);
  const form = requireElement(root, '.project-settings') as HTMLFormElement;
  const controls = form.elements as typeof form.elements & {
    title: HTMLInputElement;
    author: HTMLInputElement;
    version: HTMLInputElement;
    update: HTMLSelectElement;
    year: HTMLInputElement;
    players: HTMLInputElement;
    controls: HTMLInputElement;
  };
  (form.querySelector('input[disabled]') as HTMLInputElement).value = manifest.id;
  controls.title.value = manifest.title;
  controls.author.value = manifest.author;
  controls.version.value = manifest.version;
  controls.update.value = String(manifest.update_rate);
  controls.year.value = String(identity.year);
  controls.players.value = String(identity.players);
  controls.controls.value = identity.controls;
  (requireElement(root, '.label-file-input') as HTMLInputElement).addEventListener(
    'change',
    (event) => {
      const file = (event.currentTarget as HTMLInputElement).files?.[0];
      if (file === undefined) return;
      void file
        .arrayBuffer()
        .then((buffer) => {
          const bytes = new Uint8Array(buffer);
          return decodePngRgba(bytes).then((image) => ({ bytes, image }));
        })
        .then(({ bytes, image }) => {
          if (image.width !== HARDWARE.width || image.height !== HARDWARE.height)
            throw new RangeError('label PNG must be exactly 240x144');
          project.files['presentation/label.png'] = bytes;
          project.manifest = upsertTopLevel(project.manifest, 'label', 'presentation/label.png');
          setToolStatus(root, 'LABEL PNG READY / SAVE', false);
        })
        .catch((error: unknown) => {
          setToolStatus(root, errorMessage(error), true);
        });
    },
  );
  root.querySelector('[data-label-export]')?.addEventListener('click', () => {
    const labelPath = manifest.label;
    const bytes = labelPath === null ? undefined : project.files[labelPath];
    if (bytes === undefined) {
      setToolStatus(root, 'NO LABEL FILE', true);
      return;
    }
    downloadToolBytes(labelPath?.split('/').at(-1) ?? 'label', bytes, 'application/octet-stream');
  });
  bindSave(root, async () => {
    project.title = controls.title.value;
    project.manifest = upsertTopLevel(project.manifest, 'title', controls.title.value);
    project.manifest = upsertTopLevel(project.manifest, 'author', controls.author.value);
    project.manifest = upsertTopLevel(project.manifest, 'version', controls.version.value);
    project.manifest = upsertTopLevel(
      project.manifest,
      'update_rate',
      Number(controls.update.value),
    );
    project.files['presentation/cartridge.json'] = encodeAssetFile({
      revision: 1,
      year: clamp(Number(controls.year.value), 1970, 9999),
      players: clamp(Number(controls.players.value), 1, 4),
      controls: controls.controls.value,
    });
    await callbacks.save();
  });
}

function bindCommon(root: HTMLElement, callbacks: ToolCallbacks): void {
  root.querySelector('[data-common="back"]')?.addEventListener('click', callbacks.back);
  const panel = requireElement(root, '.asset-tool') as HTMLElement;
  enhancePixelSelects(panel);
  panel.tabIndex = -1;
  panel.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      callbacks.back();
    } else if (event.key === 'F3') {
      event.preventDefault();
      root.querySelector<HTMLButtonElement>('[data-common="save"]')?.click();
    }
  });
  panel.focus();
}

function bindSave(root: HTMLElement, save: () => Promise<void>): void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let saves = Promise.resolve();
  const run = (): void => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    saves = saves
      .then(save)
      .then(() => {
        setToolStatus(root, 'SAVED', false);
      })
      .catch((error: unknown) => {
        setToolStatus(root, errorMessage(error), true);
      });
  };
  root.querySelector('[data-common="save"]')?.addEventListener('click', run);
  const dirty = (): void => {
    setToolStatus(root, 'DIRTY / AUTOSAVE', false);
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(run, 750);
  };
  const panel = requireElement(root, '.asset-tool');
  panel.addEventListener('input', dirty);
  panel.addEventListener('change', dirty);
  panel.addEventListener('pointerup', (event) => {
    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.closest('[data-common]') !== null || target.closest('[data-preview]') !== null)
    )
      return;
    dirty();
  });
}

function toolFrame(title: string, body: string): string {
  return `<section class="display asset-tool" data-view="tool" aria-label="${title}">
    <header class="system-bar"><span>${title}</span><span>MODL/1</span></header>
    <main class="tool-stage">${body}</main>
    <p class="tool-status" role="status" aria-live="polite">READY</p>
    <footer class="tool-bar"><button type="button" data-common="back">ESC BACK</button><button type="button" data-common="save">F3 SAVE</button></footer>
  </section>`;
}

function createPalette(root: ParentNode, select: (index: number) => void): void {
  const palette = requireElement(root, '.palette-grid');
  for (let index = 0; index < 32; index += 1) {
    const button = documentElement('button', String(index));
    button.style.background = paletteCss(index);
    button.title = `color ${String(index)}`;
    button.addEventListener('click', () => {
      select(index);
    });
    palette.append(button);
  }
}

function drawPixels(
  context: CanvasRenderingContext2D,
  pixels: readonly number[],
  width: number,
  height: number,
  scale: number,
  offsetX: number,
  offsetY: number,
): void {
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const color = pixels[y * width + x] ?? 0;
      if (color !== 0) {
        context.fillStyle = paletteCss(color);
        context.fillRect(offsetX + x * scale, offsetY + y * scale, scale, scale);
      }
    }
  }
}

function drawMask(
  context: CanvasRenderingContext2D,
  pixels: readonly number[],
  width: number,
  height: number,
  scale: number,
  offsetX: number,
  offsetY: number,
  color: number,
): void {
  context.fillStyle = paletteCss(color);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      if (pixels[y * width + x] === 1)
        context.fillRect(offsetX + x * scale, offsetY + y * scale, scale, scale);
    }
  }
}

function defaultFont(): MutableFontDocument {
  return {
    revision: 1,
    kind: 'font',
    glyphWidth: BITMAP_FONT.glyphWidth,
    glyphHeight: BITMAP_FONT.glyphHeight,
    baseline: BITMAP_FONT.glyphHeight - 1,
    advanceX: BITMAP_FONT.advanceX,
    advanceY: BITMAP_FONT.advanceY,
    missingGlyph: 63,
    glyphs: Array.from({ length: 95 }, (_, index) => {
      const code = index + 32;
      const rows = glyphRows(String.fromCodePoint(code));
      return {
        code,
        pixels: rows.flatMap((row) =>
          Array.from({ length: BITMAP_FONT.glyphWidth }, (_, x) =>
            (row & (1 << (BITMAP_FONT.glyphWidth - x - 1))) === 0 ? 0 : 1,
          ),
        ),
      };
    }),
  };
}

function resizeFont(document: MutableFontDocument, width: number, height: number): void {
  for (const glyph of document.glyphs) {
    const resized = Array.from({ length: width * height }, () => 0);
    for (let y = 0; y < Math.min(height, document.glyphHeight); y += 1) {
      for (let x = 0; x < Math.min(width, document.glyphWidth); x += 1) {
        resized[y * width + x] = glyph.pixels[y * document.glyphWidth + x] ?? 0;
      }
    }
    glyph.pixels = resized;
  }
  document.glyphWidth = width;
  document.glyphHeight = height;
  document.baseline = Math.min(document.baseline, height - 1);
}

function transformMask(
  pixels: number[],
  glyphWidth: number,
  glyphHeight: number,
  action: string,
  selection: [number, number, number, number] | undefined,
): void {
  const [left, top, right, bottom] = selection ?? [0, 0, glyphWidth - 1, glyphHeight - 1];
  const width = right - left + 1;
  const height = bottom - top + 1;
  const source = [...pixels];
  for (let y = top; y <= bottom; y += 1) {
    for (let x = left; x <= right; x += 1) {
      let sourceX = action === 'flip-h' ? right - (x - left) : x;
      let sourceY = action === 'flip-v' ? bottom - (y - top) : y;
      if (action === 'rotate' && width === height) {
        sourceX = left + (y - top);
        sourceY = bottom - (x - left);
      }
      pixels[y * glyphWidth + x] = source[sourceY * glyphWidth + sourceX] ?? 0;
    }
  }
}

function drawFontPreview(
  context: CanvasRenderingContext2D,
  document: MutableFontDocument,
  text: string,
): void {
  context.fillStyle = paletteCss(1);
  context.fillRect(0, 0, context.canvas.width, context.canvas.height);
  let x = 2;
  let y = 2;
  for (const character of text) {
    if (character === '\n') {
      x = 2;
      y += document.advanceY;
      continue;
    }
    const code = character.codePointAt(0) ?? document.missingGlyph;
    const glyph =
      document.glyphs.find((candidate) => candidate.code === code) ??
      document.glyphs.find((candidate) => candidate.code === document.missingGlyph);
    if (glyph !== undefined)
      drawMask(context, glyph.pixels, document.glyphWidth, document.glyphHeight, 1, x, y, 23);
    x += document.advanceX;
  }
}

function drawRasterPreview(context: CanvasRenderingContext2D, document: PaletteDocument): void {
  const rows = new Map(document.raster.map((row) => [row.line, row]));
  let scrollX = 0;
  let scrollY = 0;
  let remap = document.remap;
  for (let y = 0; y < HARDWARE.height; y += 1) {
    const row = rows.get(y);
    if (row !== undefined) {
      scrollX = row.scrollX;
      scrollY = row.scrollY;
      remap = row.remap;
    }
    for (let x = 0; x < HARDWARE.width; x += 1) {
      const pattern = Math.floor((x + scrollX) / 12) + Math.floor((y + scrollY) / 12);
      const logical =
        ((pattern % HARDWARE.paletteSize) + HARDWARE.paletteSize) % HARDWARE.paletteSize;
      context.fillStyle = paletteCss(remap[logical] ?? logical);
      context.fillRect(x, y, 1, 1);
    }
  }
}

function transformSprite(
  document: SpriteDocument,
  frameIndex: number,
  action: string,
  selection: [number, number, number, number] | undefined,
): void {
  const frame = document.frames[frameIndex];
  if (frame === undefined) return;
  const [left, top, right, bottom] = selection ?? [0, 0, document.width - 1, document.height - 1];
  const source = [...frame];
  const width = right - left + 1;
  const height = bottom - top + 1;
  for (let y = top; y <= bottom; y += 1) {
    for (let x = left; x <= right; x += 1) {
      let sourceX = action === 'flip-h' ? right - (x - left) : x;
      let sourceY = action === 'flip-v' ? bottom - (y - top) : y;
      if (action === 'rotate' && width === height) {
        sourceX = left + (y - top);
        sourceY = bottom - (x - left);
      }
      frame[y * document.width + x] = source[sourceY * document.width + sourceX] ?? 0;
    }
  }
}

function resizeSprite(document: SpriteDocument, width: number, height: number): void {
  document.frames = document.frames.map((frame) => {
    const resized = Array.from({ length: width * height }, () => 0);
    for (let y = 0; y < Math.min(height, document.height); y += 1) {
      for (let x = 0; x < Math.min(width, document.width); x += 1) {
        resized[y * width + x] = frame[y * document.width + x] ?? 0;
      }
    }
    return resized;
  });
  document.width = width;
  document.height = height;
}

function defaultTiles(): number[][] {
  return [1, 23, 10, 15].map((color, tile) =>
    Array.from({ length: 64 }, (_, index) =>
      tile === 0 || (index + Math.floor(index / 8)) % (tile + 2) === 0 ? color : 1,
    ),
  );
}

function mapScale(canvas: HTMLCanvasElement, layer: MapDocument['layers'][number]): number {
  return Math.max(
    1,
    Math.floor(Math.min(canvas.width / layer.width, canvas.height / layer.height)),
  );
}

function tileColor(tile: readonly number[] | undefined): number {
  if (tile === undefined) return 0;
  const counts = new Uint16Array(HARDWARE.paletteSize);
  for (const color of tile) {
    if (color > 0 && color < HARDWARE.paletteSize) counts[color] = (counts[color] ?? 0) + 1;
  }
  let selected = 0;
  for (let color = 1; color < counts.length; color += 1) {
    if ((counts[color] ?? 0) > (counts[selected] ?? 0)) selected = color;
  }
  return selected;
}

function copyMapRegion(
  layer: MapDocument['layers'][number],
  selection: [number, number, number, number],
): { width: number; height: number; cells: number[] } {
  const [left, top, right, bottom] = selection;
  const width = right - left + 1;
  const height = bottom - top + 1;
  return {
    width,
    height,
    cells: Array.from({ length: width * height }, (_, index) => {
      const x = left + (index % width);
      const y = top + Math.floor(index / width);
      return layer.cells[y * layer.width + x] ?? 0;
    }),
  };
}

function clearMapRegion(
  layer: MapDocument['layers'][number],
  selection: [number, number, number, number],
): void {
  const [left, top, right, bottom] = selection;
  for (let y = top; y <= bottom; y += 1) {
    for (let x = left; x <= right; x += 1) layer.cells[y * layer.width + x] = 0;
  }
}

function stampMap(
  layer: MapDocument['layers'][number],
  clipboard: { width: number; height: number; cells: number[] },
  left: number,
  top: number,
): void {
  for (let y = 0; y < clipboard.height; y += 1) {
    for (let x = 0; x < clipboard.width; x += 1) {
      const targetX = left + x;
      const targetY = top + y;
      if (targetX < layer.width && targetY < layer.height)
        layer.cells[targetY * layer.width + targetX] =
          clipboard.cells[y * clipboard.width + x] ?? 0;
    }
  }
}

function floodMap(
  layer: MapDocument['layers'][number],
  startX: number,
  startY: number,
  replacement: number,
): void {
  const target = layer.cells[startY * layer.width + startX];
  if (target === undefined || target === replacement) return;
  const queue: [number, number][] = [[startX, startY]];
  layer.cells[startY * layer.width + startX] = replacement;
  for (let cursor = 0; cursor < queue.length; cursor += 1) {
    const point = queue[cursor];
    if (point === undefined) continue;
    const [x, y] = point;
    const neighbors: readonly { x: number; y: number }[] = [
      { x: x - 1, y },
      { x: x + 1, y },
      { x, y: y - 1 },
      { x, y: y + 1 },
    ];
    for (const neighbor of neighbors) {
      const nextX = neighbor.x;
      const nextY = neighbor.y;
      if (
        nextX >= 0 &&
        nextY >= 0 &&
        nextX < layer.width &&
        nextY < layer.height &&
        layer.cells[nextY * layer.width + nextX] === target
      ) {
        layer.cells[nextY * layer.width + nextX] = replacement;
        queue.push([nextX, nextY]);
      }
    }
  }
}

function resizeMapLayer(layer: MapDocument['layers'][number], width: number, height: number): void {
  const cells = Array.from({ length: width * height }, () => 0);
  for (let y = 0; y < Math.min(height, layer.height); y += 1) {
    for (let x = 0; x < Math.min(width, layer.width); x += 1)
      cells[y * width + x] = layer.cells[y * layer.width + x] ?? 0;
  }
  layer.width = width;
  layer.height = height;
  layer.cells = cells;
}

function defaultSound(): Record<string, unknown> {
  return {
    revision: 1,
    kind: 'sound',
    waveform: 'pulse',
    note: 60,
    durationFrames: 18,
    volume: 0.6,
    pan: 0,
    duty: 0.5,
    envelope: { attackFrames: 1, decayFrames: 3, sustainLevel: 0.65, releaseFrames: 4 },
    pitch: { slideSemitonesPerFrame: 0, vibratoDepthSemitones: 0, vibratoPeriodFrames: 0 },
  };
}

function soundControls(value: Record<string, unknown>): string {
  const envelope = isRecord(value.envelope) ? value.envelope : {};
  const pitch = isRecord(value.pitch) ? value.pitch : {};
  return `<div class="sound-controls">
    <label>WAVE <select name="wave"><option>pulse</option><option>triangle</option><option>saw</option><option>noise</option><option>wavetable</option></select></label>
    ${rangeControl('NOTE', 'note', numberValue(value.note, 60), 24, 96, 1)}
    ${rangeControl('LENGTH', 'duration', numberValue(value.durationFrames, 18), 1, 120, 1)}
    ${rangeControl('VOLUME', 'volume', numberValue(value.volume, 0.6), 0, 1, 0.05)}
    ${rangeControl('PAN', 'pan', numberValue(value.pan, 0), -1, 1, 0.1)}
    ${rangeControl('ATTACK', 'attack', numberValue(envelope.attackFrames, 1), 0, 30, 1)}
    ${rangeControl('DECAY', 'decay', numberValue(envelope.decayFrames, 3), 0, 30, 1)}
    ${rangeControl('SUSTAIN', 'sustain', numberValue(envelope.sustainLevel, 0.65), 0, 1, 0.05)}
    ${rangeControl('RELEASE', 'release', numberValue(envelope.releaseFrames, 4), 0, 30, 1)}
    ${rangeControl('SLIDE', 'slide', numberValue(pitch.slideSemitonesPerFrame, 0), -1, 1, 0.05)}
    ${rangeControl('VIBRATO', 'vibrato', numberValue(pitch.vibratoDepthSemitones, 0), 0, 4, 0.1)}
    <div><button type="button" data-preview>PREVIEW</button><button type="button" data-wav>WAV</button><span class="voice-readout">8V / PRODUCTION PCM</span></div>
    <canvas class="sound-scope" width="120" height="24" aria-label="Sound oscilloscope"></canvas>
  </div>`;
}

function drawWavScope(root: ParentNode, wav: Uint8Array): void {
  const canvas = requireElement(root, '.sound-scope') as HTMLCanvasElement;
  const context = requireContext(canvas);
  const view = new DataView(wav.buffer, wav.byteOffset, wav.byteLength);
  const samples = Math.floor((wav.length - 44) / 4);
  context.fillStyle = paletteCss(1);
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.strokeStyle = paletteCss(23);
  context.beginPath();
  for (let x = 0; x < canvas.width; x += 1) {
    const sample = Math.min(samples - 1, Math.floor((x * samples) / canvas.width));
    const value = sample < 0 ? 0 : view.getInt16(44 + sample * 4, true) / 32_768;
    const y = Math.round(canvas.height / 2 - value * (canvas.height / 2 - 1));
    if (x === 0) context.moveTo(x, y);
    else context.lineTo(x, y);
  }
  context.stroke();
}

function rangeControl(
  label: string,
  name: string,
  value: number,
  minimum: number,
  maximum: number,
  step: number,
): string {
  return `<label>${label} <input name="${name}" type="range" min="${String(minimum)}" max="${String(maximum)}" step="${String(step)}" value="${String(value)}"></label>`;
}

function soundFromControls(root: ParentNode): SoundAsset {
  const value = (name: string): number =>
    Number((requireElement(root, `[name="${name}"]`) as HTMLInputElement).value);
  const wave = (requireElement(root, '[name="wave"]') as HTMLSelectElement)
    .value as SoundAsset['waveform'];
  return {
    kind: 'sound',
    name: 'blip',
    waveform: wave,
    note: value('note'),
    durationFrames: value('duration'),
    volume: value('volume'),
    pan: value('pan'),
    ...(wave === 'pulse' ? { duty: 0.5 } : {}),
    ...(wave === 'wavetable' ? { wavetable: [0, 0.7, 1, 0.7, 0, -0.7, -1, -0.7] } : {}),
    envelope: {
      attackFrames: value('attack'),
      decayFrames: value('decay'),
      sustainLevel: value('sustain'),
      releaseFrames: value('release'),
    },
    pitch: {
      slideSemitonesPerFrame: value('slide'),
      vibratoDepthSemitones: value('vibrato'),
      vibratoPeriodFrames: value('vibrato') === 0 ? 0 : 8,
    },
  };
}

async function previewSound(sound: SoundAsset): Promise<void> {
  const synth = new Synthesizer(new AudioAssetStore([sound]));
  const sink = new WebAudioSink();
  await sink.resume();
  for (let frame = 0; frame < sound.durationFrames + sound.envelope.releaseFrames; frame += 1) {
    sink.enqueue(
      synth.executeFrame(
        frame === 0
          ? [
              {
                name: 'sfx',
                arguments: [{ name: sound.name, kind: 'Sound' }],
                sourceSpan: { start: 0, end: 0 },
              },
            ]
          : [],
      ),
    );
  }
  setTimeout(() => void sink.close(), 2_000);
}

type EditableMusicRows = (TrackerCell | null)[][];

interface EditableMusicDocument {
  readonly order: string[];
  readonly patterns: Record<string, EditableMusicRows>;
}

function emptyMusicRows(): EditableMusicRows {
  return Array.from({ length: 16 }, () => Array.from<TrackerCell | null>({ length: 8 }).fill(null));
}

function musicDocument(value: Record<string, unknown> | undefined): EditableMusicDocument {
  const patterns: Record<string, EditableMusicRows> = {};
  if (value !== undefined && isRecord(value.patterns)) {
    for (const [name, rawPattern] of Object.entries(value.patterns)) {
      if (!isRecord(rawPattern) || !Array.isArray(rawPattern.rows)) continue;
      const rows = emptyMusicRows();
      for (let row = 0; row < rows.length; row += 1) {
        const target = rows[row];
        if (target === undefined) continue;
        for (let channel = 0; channel < 8; channel += 1) {
          const cell: unknown = (rawPattern.rows as unknown[][])[row]?.[channel];
          if (isRecord(cell) && typeof cell.note === 'number' && typeof cell.sound === 'string') {
            target[channel] = {
              note: cell.note,
              sound: cell.sound,
              ...(typeof cell.volume === 'number' ? { volume: cell.volume } : {}),
            };
          }
        }
      }
      patterns[name] = rows;
    }
  }
  if (Object.keys(patterns).length === 0) patterns['00'] = emptyMusicRows();
  const order = Array.isArray(value?.order)
    ? value.order.filter(
        (name): name is string => typeof name === 'string' && patterns[name] !== undefined,
      )
    : [];
  return { order: order.length === 0 ? [Object.keys(patterns)[0] ?? '00'] : order, patterns };
}

function nextPatternName(
  patterns: Readonly<Record<string, EditableMusicRows>>,
): string | undefined {
  for (let index = 0; index <= 0xff; index += 1) {
    const name = index.toString(16).toUpperCase().padStart(2, '0');
    if (patterns[name] === undefined) return name;
  }
  return undefined;
}

function musicFromDocument(root: ParentNode, document: EditableMusicDocument): MusicAsset {
  const order = (requireElement(root, '.tracker-order input') as HTMLInputElement).value
    .split(/[\s,]+/u)
    .filter((name) => name.length > 0);
  if (order.length === 0) throw new Error('order list cannot be empty');
  const missing = order.find((name) => document.patterns[name] === undefined);
  if (missing !== undefined) throw new Error(`order references missing pattern '${missing}'`);
  return {
    kind: 'music',
    name: 'theme',
    framesPerRow: Number((requireElement(root, '.tempo') as HTMLInputElement).value),
    order,
    patterns: Object.fromEntries(
      Object.entries(document.patterns).map(([name, rows]) => [name, { rows }]),
    ),
    loop: (requireElement(root, '.loop') as HTMLInputElement).checked,
  };
}

function musicSounds(project: ToolProject, manifest: ProjectManifest): SoundAsset[] {
  return Object.entries(manifest.assets).flatMap(([name, asset]) => {
    if (asset.kind !== 'sound') return [];
    const value = readJson<Record<string, unknown> | undefined>(
      project.files[asset.path],
      undefined,
    );
    return value === undefined ? [] : [{ ...value, name } as unknown as SoundAsset];
  });
}

async function previewMusic(music: MusicAsset, sounds: readonly SoundAsset[]): Promise<void> {
  const assets = [...sounds];
  if (!assets.some((sound) => sound.name === 'blip')) {
    assets.push({ ...defaultSound(), name: 'blip' } as unknown as SoundAsset);
  }
  const synth = new Synthesizer(new AudioAssetStore([...assets, music]));
  const sink = new WebAudioSink();
  await sink.resume();
  const frames =
    music.framesPerRow *
    music.order.reduce((rows, name) => rows + (music.patterns[name]?.rows.length ?? 0), 0);
  for (let frame = 0; frame < frames; frame += 1) {
    sink.enqueue(
      synth.executeFrame(
        frame === 0
          ? [
              {
                name: 'music',
                arguments: [{ name: music.name, kind: 'Music' }],
                sourceSpan: { start: 0, end: 0 },
              },
            ]
          : [],
      ),
    );
  }
  setTimeout(() => void sink.close(), 4_000);
}

function noteLabel(note: number | null): string {
  return note === null ? '--' : note.toString(16).toUpperCase().padStart(2, '0');
}

function upsertAsset(manifest: string, name: string, kind: string, path: string): string {
  const section = `[assets.${name}]\nkind = "${kind}"\npath = "${path}"\n`;
  const matcher = new RegExp(`\\[assets\\.${name}\\]\\n[\\s\\S]*?(?=\\n\\[|$)`);
  if (matcher.test(manifest)) {
    return manifest.replace(matcher, section.trimEnd());
  }
  return `${manifest.trimEnd()}\n\n${section}`;
}

function upsertTopLevel(manifest: string, key: string, value: string | number): string {
  const encoded = typeof value === 'number' ? String(value) : JSON.stringify(value);
  const line = `${key} = ${encoded}`;
  const matcher = new RegExp(`^${key}\\s*=.*$`, 'm');
  if (matcher.test(manifest)) return manifest.replace(matcher, line);
  const section = manifest.indexOf('\n[');
  return section < 0
    ? `${manifest.trimEnd()}\n${line}\n`
    : `${manifest.slice(0, section)}\n${line}${manifest.slice(section)}`;
}

function paletteCss(index: number): string {
  const offset = clamp(index, 0, HARDWARE.paletteSize - 1) * 4;
  const red = MASTER_PALETTE_RGBA[offset] ?? 0;
  const green = MASTER_PALETTE_RGBA[offset + 1] ?? 0;
  const blue = MASTER_PALETTE_RGBA[offset + 2] ?? 0;
  return `rgb(${String(red)} ${String(green)} ${String(blue)})`;
}

function indexedRgba(indexed: Uint8Array, transparentZero: boolean): Uint8Array {
  const rgba = new Uint8Array(indexed.length * 4);
  for (let pixel = 0; pixel < indexed.length; pixel += 1) {
    const color = clamp(indexed[pixel] ?? 0, 0, HARDWARE.paletteSize - 1);
    const palette = color * 4;
    rgba[pixel * 4] = MASTER_PALETTE_RGBA[palette] ?? 0;
    rgba[pixel * 4 + 1] = MASTER_PALETTE_RGBA[palette + 1] ?? 0;
    rgba[pixel * 4 + 2] = MASTER_PALETTE_RGBA[palette + 2] ?? 0;
    rgba[pixel * 4 + 3] = transparentZero && color === 0 ? 0 : 255;
  }
  return rgba;
}

function downloadToolBytes(name: string, bytes: Uint8Array, type: string): void {
  const url = URL.createObjectURL(new Blob([new Uint8Array(bytes).buffer], { type }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}

function readJson<Value>(bytes: Uint8Array | undefined, fallback: Value): Value {
  if (bytes === undefined) return fallback;
  try {
    return JSON.parse(textDecoder.decode(bytes)) as Value;
  } catch {
    return fallback;
  }
}

function documentElement(tag: 'button' | 'span', text: string): HTMLElement {
  const element = document.createElement(tag);
  if (tag === 'button') element.setAttribute('type', 'button');
  element.textContent = text;
  return element;
}

function requireContext(canvas: HTMLCanvasElement): CanvasRenderingContext2D {
  const context = canvas.getContext('2d');
  if (context === null) throw new Error('2D canvas is unavailable');
  return context;
}

function setText(root: ParentNode, selector: string, value: string): void {
  requireElement(root, selector).textContent = value;
}

function setToolStatus(root: ParentNode, message: string, error: boolean): void {
  const status = requireElement(root, '.tool-status');
  status.textContent = message;
  status.classList.toggle('error', error);
}

function requireElement(root: ParentNode, selector: string): Element {
  const element = root.querySelector(selector);
  if (element === null) throw new Error(`tool element '${selector}' is missing`);
  return element;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, Math.round(value)));
}

function numberValue(value: unknown, fallback: number): number {
  return typeof value === 'number' ? value : fallback;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message.toUpperCase() : 'TOOL ERROR';
}
