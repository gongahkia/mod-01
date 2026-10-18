import { HARDWARE } from './hardware';
import { BITMAP_FONT, glyphRows } from './font';
import type { ConsoleCommand } from './protocol';
import { MEMORY, type ByteMemoryRegion } from './bus';
import { VisualAssetStore } from './visual-store';
export { VisualAssetStore, visualAssetBytes } from './visual-store';

export interface IndexedSprite {
  readonly kind: 'sprite';
  readonly name: string;
  readonly width: number;
  readonly height: number;
  readonly pixels: Uint8Array;
}

export interface IndexedAnimation {
  readonly kind: 'animation';
  readonly name: string;
  readonly frames: readonly IndexedSprite[];
}

export interface IndexedTileSet {
  readonly kind: 'tile_set';
  readonly name: string;
  readonly tiles: readonly IndexedSprite[];
  readonly flags: Uint8Array;
}

export interface IndexedMapLayer {
  readonly width: number;
  readonly height: number;
  readonly cells: Uint16Array;
  readonly tileSet: string;
}

export interface IndexedMap {
  readonly kind: 'map';
  readonly name: string;
  readonly layers: readonly IndexedMapLayer[];
}

export interface IndexedFont {
  readonly kind: 'font';
  readonly name: string;
  readonly glyphWidth: number;
  readonly glyphHeight: number;
  readonly baseline: number;
  readonly advanceX: number;
  readonly advanceY: number;
  readonly missingGlyph: number;
  readonly glyphs: ReadonlyMap<number, Uint8Array>;
}

export type VisualAsset =
  IndexedSprite | IndexedAnimation | IndexedTileSet | IndexedMap | IndexedFont;

export interface DisplayRasterState {
  readonly line: number;
  readonly scrollX: number;
  readonly scrollY: number;
  readonly remap: Uint8Array;
}

export interface DisplayConfiguration {
  readonly remap: Uint8Array;
  readonly raster: readonly DisplayRasterState[];
}

interface DrawState {
  cameraX: number;
  cameraY: number;
  clipX: number;
  clipY: number;
  clipWidth: number;
  clipHeight: number;
  readonly remap: Uint8Array;
}

export interface GraphicsFrame {
  readonly indexedPixels: Uint8Array;
  readonly commands: number;
}

export interface GraphicsSnapshot {
  readonly revision: 1;
  readonly front: Uint8Array;
  readonly resolved: Uint8Array;
}

export function isGraphicsSnapshot(value: unknown): value is GraphicsSnapshot {
  const pixels = HARDWARE.width * HARDWARE.height;
  return (
    isRecord(value) &&
    value.revision === 1 &&
    value.front instanceof Uint8Array &&
    value.front.length === pixels &&
    value.front.every((color) => color < HARDWARE.paletteSize) &&
    value.resolved instanceof Uint8Array &&
    value.resolved.length === pixels &&
    value.resolved.every((color) => color < HARDWARE.paletteSize)
  );
}

/** Deterministic indexed immediate-mode rasterizer with double-buffered storage. */
export class IndexedGraphics {
  private readonly front = new Uint8Array(HARDWARE.width * HARDWARE.height);
  private readonly back = new Uint8Array(HARDWARE.width * HARDWARE.height);
  private readonly resolved = new Uint8Array(HARDWARE.width * HARDWARE.height);
  private readonly assets: VisualAssetStore;
  private readonly display: DisplayConfiguration;
  private readonly drawRegisters = new Uint8Array(80);
  private readonly transparency = Uint8Array.of(HARDWARE.transparentColor);
  private readonly state = new MemoryDrawState(this.drawRegisters);
  private readonly rasterBytes = new Uint8Array(HARDWARE.height * MEMORY.rasterStride);
  private readonly rasterView = new DataView(this.rasterBytes.buffer);
  private readonly rasterRemaps = Array.from({ length: HARDWARE.height }, (_, line) =>
    this.rasterBytes.subarray(line * MEMORY.rasterStride + 8, (line + 1) * MEMORY.rasterStride),
  );
  private readonly rasterLive = new Uint8Array(48);
  private readonly rasterLiveView = new DataView(this.rasterLive.buffer);
  private readonly displayRemap = this.rasterLive.subarray(16);
  private commandCount = 0;
  private activeFrame = false;

  public constructor(assets = new VisualAssetStore(), display?: DisplayConfiguration) {
    this.assets = assets;
    this.display =
      display === undefined
        ? (assets.display ?? copyDisplayConfiguration())
        : copyDisplayConfiguration(display);
    this.state.clipWidth = HARDWARE.width;
    this.state.clipHeight = HARDWARE.height;
    this.state.remap.set(this.display.remap);
    this.displayRemap.set(identityRemap());
  }

  public executeFrame(commands: readonly ConsoleCommand[]): GraphicsFrame {
    if (commands.length > HARDWARE.drawCommandsPerFrame)
      throw new RangeError('draw-command ceiling exceeded');
    this.beginFrame();
    for (const command of commands) this.executeCommand(command);
    return this.finishFrame();
  }

  public beginFrame(): void {
    this.back.set(this.front);
    this.drawRegisters.fill(0);
    this.state.clipWidth = HARDWARE.width;
    this.state.clipHeight = HARDWARE.height;
    this.state.remap.set(this.display.remap);
    this.rasterBytes.fill(0);
    this.rasterLive.fill(0);
    this.displayRemap.set(identityRemap());
    this.commandCount = 0;
    this.activeFrame = true;
    for (const raster of this.display.raster) {
      this.rasterRemaps[raster.line]?.set(raster.remap);
      this.rasterView.setInt16(raster.line * MEMORY.rasterStride + 4, raster.scrollX, true);
      this.rasterView.setInt16(raster.line * MEMORY.rasterStride + 6, raster.scrollY, true);
      this.rasterBytes[raster.line * MEMORY.rasterStride] = 1;
    }
  }

  public executeCommand(command: ConsoleCommand): void {
    if (!this.activeFrame) throw new Error('graphics frame has not begun');
    if (this.commandCount >= HARDWARE.drawCommandsPerFrame)
      throw new RangeError('draw-command ceiling exceeded');
    this.commandCount += 1;
    if (command.rasterLine === undefined) {
      const writesState = ['camera', 'clip', 'clip_reset', 'pal', 'pal_reset'].includes(
        command.name,
      );
      // commands are atomic; latch their read-only registers outside the pixel loops.
      this.executeDraw(command, writesState ? this.state : this.state.capture());
      return;
    }
    const line = command.rasterLine;
    if (line < 0 || line >= HARDWARE.height) {
      throw new RangeError('raster command has an invalid scanline');
    }
    if (command.name === 'pal') {
      const [from, to] = expectIntegers(command, 2);
      this.displayRemap[expectRemappableColor(from)] = expectColor(to);
    } else if (command.name === 'raster_scroll') {
      const [x, y] = expectIntegers(command, 2);
      this.rasterLiveView.setFloat64(0, x, true);
      this.rasterLiveView.setFloat64(8, y, true);
    } else {
      throw new TypeError(`'${command.name}' is not valid during raster display`);
    }
    this.rasterView.setInt16(
      line * MEMORY.rasterStride + 4,
      clampInt16(this.rasterLiveView.getFloat64(0, true)),
      true,
    );
    this.rasterView.setInt16(
      line * MEMORY.rasterStride + 6,
      clampInt16(this.rasterLiveView.getFloat64(8, true)),
      true,
    );
    this.rasterRemaps[line]?.set(this.displayRemap);
    this.rasterBytes[line * MEMORY.rasterStride] = 1;
  }

  public finishFrame(): GraphicsFrame {
    if (!this.activeFrame) throw new Error('graphics frame has not begun');
    let previousRemap = identityRemap();
    let previousScrollX = 0;
    let previousScrollY = 0;
    for (let y = 0; y < HARDWARE.height; y += 1) {
      const lineRemap = this.rasterRemaps[y];
      if (lineRemap !== undefined && this.rasterBytes[y * MEMORY.rasterStride] === 1) {
        previousRemap = lineRemap;
        previousScrollX = this.rasterView.getInt16(y * MEMORY.rasterStride + 4, true);
        previousScrollY = this.rasterView.getInt16(y * MEMORY.rasterStride + 6, true);
      }
      for (let x = 0; x < HARDWARE.width; x += 1) {
        const sourceX = wrap(x + previousScrollX, HARDWARE.width);
        const sourceY = wrap(y + previousScrollY, HARDWARE.height);
        const color = this.back[sourceY * HARDWARE.width + sourceX] ?? 0;
        this.resolved[y * HARDWARE.width + x] = previousRemap[color] ?? color;
      }
    }

    // fixed buffer identity is required by the byte-addressed hardware interface.
    this.front.set(this.back);
    this.activeFrame = false;
    return { indexedPixels: this.resolved.slice(), commands: this.commandCount };
  }

  public snapshot(): GraphicsSnapshot {
    return { revision: 1, front: this.front.slice(), resolved: this.resolved.slice() };
  }

  public memoryRegions(): readonly ByteMemoryRegion[] {
    const validate = (_offset: number, bytes: Uint8Array): boolean =>
      bytes.every((color) => color < HARDWARE.paletteSize);
    return [
      { name: 'front', address: MEMORY.front, bytes: this.front, writable: true, validate },
      { name: 'back', address: MEMORY.back, bytes: this.back, writable: true, validate },
      {
        name: 'display',
        address: MEMORY.display,
        bytes: this.resolved,
        writable: false,
        retained: true,
        validate,
      },
      {
        name: 'draw state',
        address: MEMORY.draw,
        bytes: this.drawRegisters,
        writable: true,
        validate: (offset, bytes) => {
          const candidate = this.drawRegisters.slice();
          candidate.set(bytes, offset);
          const view = new DataView(candidate.buffer);
          for (let field = 0; field < 48; field += 8)
            if (!Number.isSafeInteger(view.getFloat64(field, true))) return false;
          return (
            view.getFloat64(32, true) >= 0 &&
            view.getFloat64(40, true) >= 0 &&
            candidate.subarray(48).every((color) => color < HARDWARE.paletteSize)
          );
        },
      },
      {
        name: 'transparency index',
        address: MEMORY.transparency,
        bytes: this.transparency,
        writable: false,
      },
      {
        name: 'raster callback state',
        address: MEMORY.rasterLive,
        bytes: this.rasterLive,
        writable: false,
        retained: true,
        validate: (_offset, bytes) => {
          const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
          return (
            bytes.length === 48 &&
            Number.isSafeInteger(view.getFloat64(0, true)) &&
            Number.isSafeInteger(view.getFloat64(8, true)) &&
            bytes.subarray(16).every((color) => color < HARDWARE.paletteSize)
          );
        },
      },
      {
        name: 'raster table',
        address: MEMORY.raster,
        bytes: this.rasterBytes,
        writable: true,
        rasterWritable: true,
        validate: (offset, bytes) =>
          bytes.every((value, index) => {
            const field = (offset + index) % MEMORY.rasterStride;
            return field === 0
              ? value <= 1
              : field < 4
                ? value === 0
                : field < 8 || value < HARDWARE.paletteSize;
          }),
      },
    ];
  }

  public restore(snapshot: unknown): void {
    if (!isGraphicsSnapshot(snapshot)) {
      throw new TypeError('invalid indexed graphics snapshot');
    }
    this.front.set(snapshot.front);
    this.back.set(snapshot.front);
    this.resolved.set(snapshot.resolved);
    this.activeFrame = false;
  }

  private executeDraw(command: ConsoleCommand, state: DrawState): void {
    switch (command.name) {
      case 'clear': {
        const [color] = expectIntegers(command, 1);
        this.back.fill(state.remap[expectColor(color)] ?? color);
        return;
      }
      case 'pixel': {
        const [x, y, color] = expectIntegers(command, 3);
        this.plot(x, y, color, state);
        return;
      }
      case 'line': {
        const [x0, y0, x1, y1, color] = expectIntegers(command, 5);
        this.line(x0, y0, x1, y1, color, state);
        return;
      }
      case 'rect':
      case 'rect_fill': {
        const [x, y, width, height, color] = expectIntegers(command, 5);
        this.rectangle(x, y, width, height, color, command.name === 'rect_fill', state);
        return;
      }
      case 'circle':
      case 'circle_fill': {
        const [x, y, radius, color] = expectIntegers(command, 4);
        this.circle(x, y, radius, color, command.name === 'circle_fill', state);
        return;
      }
      case 'triangle': {
        const [x0, y0, x1, y1, x2, y2, color] = expectIntegers(command, 7);
        this.triangle(x0, y0, x1, y1, x2, y2, color, state);
        return;
      }
      case 'camera':
        [state.cameraX, state.cameraY] = expectIntegers(command, 2);
        return;
      case 'clip': {
        const [x, y, width, height] = expectIntegers(command, 4);
        state.clipX = x;
        state.clipY = y;
        state.clipWidth = Math.max(0, width);
        state.clipHeight = Math.max(0, height);
        return;
      }
      case 'clip_reset':
        expectIntegers(command, 0);
        state.clipX = 0;
        state.clipY = 0;
        state.clipWidth = HARDWARE.width;
        state.clipHeight = HARDWARE.height;
        return;
      case 'pal': {
        const [from, to] = expectIntegers(command, 2);
        state.remap[expectRemappableColor(from)] = expectColor(to);
        return;
      }
      case 'pal_reset':
        expectIntegers(command, 0);
        state.remap.set(identityRemap());
        return;
      case 'sprite': {
        const [handle, x, y] = command.arguments;
        this.drawSprite(readAssetName(handle, 'Sprite'), expectInteger(x), expectInteger(y), state);
        return;
      }
      case 'animation': {
        const [handle, frame, x, y] = command.arguments;
        this.drawAnimation(
          readAssetName(handle, 'Animation'),
          expectInteger(frame),
          expectInteger(x),
          expectInteger(y),
          state,
        );
        return;
      }
      case 'sprite_xform': {
        const [handle, x, y, scale, quarterTurns, flipX, flipY] = command.arguments;
        this.drawTransformedSprite(
          readAssetName(handle, 'Sprite'),
          expectInteger(x),
          expectInteger(y),
          expectInteger(scale),
          expectInteger(quarterTurns),
          expectBoolean(flipX),
          expectBoolean(flipY),
          state,
        );
        return;
      }
      case 'map': {
        const [handle, x, y] = command.arguments;
        this.drawMap(readAssetName(handle, 'Map'), expectInteger(x), expectInteger(y), state);
        return;
      }
      case 'raster_scroll':
        throw new TypeError('raster_scroll is only valid in the raster callback');
      case 'print': {
        const [text, x, y, color] = command.arguments;
        this.print(
          expectText(text),
          expectInteger(x),
          expectInteger(y),
          expectInteger(color),
          state,
        );
        return;
      }
      case 'font_print': {
        const [handle, text, x, y, color] = command.arguments;
        this.printFont(
          readAssetName(handle, 'Font'),
          expectText(text),
          expectInteger(x),
          expectInteger(y),
          expectInteger(color),
          state,
        );
        return;
      }
      default:
        throw new TypeError(`unknown graphics command '${command.name}'`);
    }
  }

  private plot(x: number, y: number, color: number, state: DrawState): void {
    const screenX = x - state.cameraX;
    const screenY = y - state.cameraY;
    if (
      screenX < 0 ||
      screenX >= HARDWARE.width ||
      screenY < 0 ||
      screenY >= HARDWARE.height ||
      screenX < state.clipX ||
      screenX >= state.clipX + state.clipWidth ||
      screenY < state.clipY ||
      screenY >= state.clipY + state.clipHeight
    ) {
      return;
    }
    this.back[screenY * HARDWARE.width + screenX] = state.remap[expectColor(color)] ?? color;
  }

  private line(
    startX: number,
    startY: number,
    endX: number,
    endY: number,
    color: number,
    state: DrawState,
  ): void {
    let x = startX;
    let y = startY;
    const deltaX = Math.abs(endX - startX);
    const stepX = startX < endX ? 1 : -1;
    const deltaY = -Math.abs(endY - startY);
    const stepY = startY < endY ? 1 : -1;
    let error = deltaX + deltaY;
    for (;;) {
      this.plot(x, y, color, state);
      if (x === endX && y === endY) {
        return;
      }
      const doubled = error * 2;
      if (doubled >= deltaY) {
        error += deltaY;
        x += stepX;
      }
      if (doubled <= deltaX) {
        error += deltaX;
        y += stepY;
      }
    }
  }

  private rectangle(
    x: number,
    y: number,
    width: number,
    height: number,
    color: number,
    filled: boolean,
    state: DrawState,
  ): void {
    if (width <= 0 || height <= 0) {
      return;
    }
    if (!filled) {
      this.line(x, y, x + width - 1, y, color, state);
      this.line(x, y + height - 1, x + width - 1, y + height - 1, color, state);
      this.line(x, y, x, y + height - 1, color, state);
      this.line(x + width - 1, y, x + width - 1, y + height - 1, color, state);
      return;
    }
    for (let row = 0; row < height; row += 1) {
      this.line(x, y + row, x + width - 1, y + row, color, state);
    }
  }

  private circle(
    centerX: number,
    centerY: number,
    radius: number,
    color: number,
    filled: boolean,
    state: DrawState,
  ): void {
    if (radius < 0) {
      return;
    }
    let x = radius;
    let y = 0;
    let error = 1 - radius;
    while (x >= y) {
      if (filled) {
        this.line(centerX - x, centerY + y, centerX + x, centerY + y, color, state);
        this.line(centerX - x, centerY - y, centerX + x, centerY - y, color, state);
        this.line(centerX - y, centerY + x, centerX + y, centerY + x, color, state);
        this.line(centerX - y, centerY - x, centerX + y, centerY - x, color, state);
      } else {
        for (const [plotX, plotY] of [
          [centerX + x, centerY + y],
          [centerX + y, centerY + x],
          [centerX - y, centerY + x],
          [centerX - x, centerY + y],
          [centerX - x, centerY - y],
          [centerX - y, centerY - x],
          [centerX + y, centerY - x],
          [centerX + x, centerY - y],
        ] as const) {
          this.plot(plotX, plotY, color, state);
        }
      }
      y += 1;
      if (error < 0) {
        error += 2 * y + 1;
      } else {
        x -= 1;
        error += 2 * (y - x) + 1;
      }
    }
  }

  private triangle(
    x0: number,
    y0: number,
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    color: number,
    state: DrawState,
  ): void {
    const minimumX = Math.min(x0, x1, x2);
    const maximumX = Math.max(x0, x1, x2);
    const minimumY = Math.min(y0, y1, y2);
    const maximumY = Math.max(y0, y1, y2);
    const area = edge(x0, y0, x1, y1, x2, y2);
    if (area === 0) {
      this.line(x0, y0, x1, y1, color, state);
      this.line(x1, y1, x2, y2, color, state);
      return;
    }
    for (let y = minimumY; y <= maximumY; y += 1) {
      for (let x = minimumX; x <= maximumX; x += 1) {
        const first = edge(x1, y1, x2, y2, x, y);
        const second = edge(x2, y2, x0, y0, x, y);
        const third = edge(x0, y0, x1, y1, x, y);
        if (
          (area > 0 && first >= 0 && second >= 0 && third >= 0) ||
          (area < 0 && first <= 0 && second <= 0 && third <= 0)
        ) {
          this.plot(x, y, color, state);
        }
      }
    }
  }

  private drawSprite(name: string, x: number, y: number, state: DrawState): void {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'sprite') {
      throw new TypeError(`missing Sprite asset '${name}'`);
    }
    this.blit(asset, x, y, 1, 0, false, false, state);
  }

  private drawAnimation(name: string, frame: number, x: number, y: number, state: DrawState): void {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'animation' || asset.frames.length === 0) {
      throw new TypeError(`missing Animation asset '${name}'`);
    }
    const selected = asset.frames[wrap(frame, asset.frames.length)];
    if (selected !== undefined) {
      this.blit(selected, x, y, 1, 0, false, false, state);
    }
  }

  private drawTransformedSprite(
    name: string,
    x: number,
    y: number,
    scale: number,
    quarterTurns: number,
    flipX: boolean,
    flipY: boolean,
    state: DrawState,
  ): void {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'sprite') {
      throw new TypeError(`missing Sprite asset '${name}'`);
    }
    if (scale < 1 || scale > 16) {
      throw new RangeError('sprite scale must be between 1 and 16');
    }
    this.blit(asset, x, y, scale, wrap(quarterTurns, 4), flipX, flipY, state);
  }

  private blit(
    sprite: IndexedSprite,
    x: number,
    y: number,
    scale: number,
    quarterTurns: number,
    flipX: boolean,
    flipY: boolean,
    state: DrawState,
  ): void {
    const outputWidth = (quarterTurns % 2 === 0 ? sprite.width : sprite.height) * scale;
    const outputHeight = (quarterTurns % 2 === 0 ? sprite.height : sprite.width) * scale;
    for (let outputY = 0; outputY < outputHeight; outputY += 1) {
      for (let outputX = 0; outputX < outputWidth; outputX += 1) {
        let sourceX = Math.floor(outputX / scale);
        let sourceY = Math.floor(outputY / scale);
        [sourceX, sourceY] = unrotate(sourceX, sourceY, sprite.width, sprite.height, quarterTurns);
        if (flipX) {
          sourceX = sprite.width - 1 - sourceX;
        }
        if (flipY) {
          sourceY = sprite.height - 1 - sourceY;
        }
        const color = sprite.pixels[sourceY * sprite.width + sourceX] ?? HARDWARE.transparentColor;
        if (color !== this.transparency[0]) {
          this.plot(x + outputX, y + outputY, color, state);
        }
      }
    }
  }

  private drawMap(name: string, x: number, y: number, state: DrawState): void {
    const map = this.assets.get(name);
    if (map?.kind !== 'map') {
      throw new TypeError(`missing Map asset '${name}'`);
    }
    for (const layer of map.layers) {
      const tileSet = this.assets.get(layer.tileSet);
      if (tileSet?.kind !== 'tile_set') {
        throw new TypeError(`missing TileSet asset '${layer.tileSet}'`);
      }
      const clipRight = Math.min(HARDWARE.width, state.clipX + state.clipWidth);
      const clipBottom = Math.min(HARDWARE.height, state.clipY + state.clipHeight);
      const firstColumn = Math.max(
        0,
        Math.floor((state.clipX + state.cameraX - x) / HARDWARE.tileSize),
      );
      const lastColumn = Math.min(
        layer.width,
        Math.ceil((clipRight + state.cameraX - x) / HARDWARE.tileSize),
      );
      const firstRow = Math.max(
        0,
        Math.floor((state.clipY + state.cameraY - y) / HARDWARE.tileSize),
      );
      const lastRow = Math.min(
        layer.height,
        Math.ceil((clipBottom + state.cameraY - y) / HARDWARE.tileSize),
      );
      for (let row = firstRow; row < lastRow; row += 1) {
        for (let column = firstColumn; column < lastColumn; column += 1) {
          const tile = tileSet.tiles[layer.cells.getUint16((row * layer.width + column) * 2, true)];
          if (tile !== undefined) {
            this.blit(
              tile,
              x + column * HARDWARE.tileSize,
              y + row * HARDWARE.tileSize,
              1,
              0,
              false,
              false,
              state,
            );
          }
        }
      }
    }
  }

  private print(text: string, x: number, y: number, color: number, state: DrawState): void {
    let cursorX = x;
    let cursorY = y;
    for (const character of text) {
      if (character === '\n') {
        cursorX = x;
        cursorY += BITMAP_FONT.advanceY;
        continue;
      }
      const rows = glyphRows(character);
      rows.forEach((bits, row) => {
        for (let column = 0; column < BITMAP_FONT.glyphWidth; column += 1) {
          if ((bits & (1 << (BITMAP_FONT.glyphWidth - 1 - column))) !== 0) {
            this.plot(cursorX + column, cursorY + row, color, state);
          }
        }
      });
      cursorX += BITMAP_FONT.advanceX;
    }
  }

  private printFont(
    name: string,
    text: string,
    x: number,
    y: number,
    color: number,
    state: DrawState,
  ): void {
    const font = this.assets.get(name);
    if (font?.kind !== 'font') throw new TypeError(`missing Font asset '${name}'`);
    let cursorX = x;
    let cursorY = y;
    for (const character of text) {
      if (character === '\n') {
        cursorX = x;
        cursorY += font.advanceY;
        continue;
      }
      const code = character.codePointAt(0) ?? font.missingGlyph;
      const glyph = font.glyphs.get(code) ?? font.glyphs.get(font.missingGlyph);
      if (glyph !== undefined) {
        for (let row = 0; row < font.glyphHeight; row += 1) {
          for (let column = 0; column < font.glyphWidth; column += 1) {
            if (glyph[row * font.glyphWidth + column] === 1)
              this.plot(cursorX + column, cursorY + row, color, state);
          }
        }
      }
      cursorX += font.advanceX;
    }
  }
}

/** Deterministic four-by-four Bayer choice between two palette indices. */
export function orderedDither(
  x: number,
  y: number,
  first: number,
  second: number,
  level: number,
): number {
  const matrix = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5] as const;
  const threshold = matrix[wrap(y, 4) * 4 + wrap(x, 4)] ?? 0;
  return threshold < Math.max(0, Math.min(16, level)) ? expectColor(second) : expectColor(first);
}

class MemoryDrawState implements DrawState {
  private readonly view: DataView;
  public readonly remap: Uint8Array;

  public constructor(bytes: Uint8Array) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.remap = bytes.subarray(48);
  }

  public capture(): DrawState {
    return {
      cameraX: this.cameraX,
      cameraY: this.cameraY,
      clipX: this.clipX,
      clipY: this.clipY,
      clipWidth: this.clipWidth,
      clipHeight: this.clipHeight,
      remap: this.remap,
    };
  }

  public get cameraX(): number {
    return this.view.getFloat64(0, true);
  }
  public set cameraX(value: number) {
    this.view.setFloat64(0, value, true);
  }
  public get cameraY(): number {
    return this.view.getFloat64(8, true);
  }
  public set cameraY(value: number) {
    this.view.setFloat64(8, value, true);
  }
  public get clipX(): number {
    return this.view.getFloat64(16, true);
  }
  public set clipX(value: number) {
    this.view.setFloat64(16, value, true);
  }
  public get clipY(): number {
    return this.view.getFloat64(24, true);
  }
  public set clipY(value: number) {
    this.view.setFloat64(24, value, true);
  }
  public get clipWidth(): number {
    return this.view.getFloat64(32, true);
  }
  public set clipWidth(value: number) {
    this.view.setFloat64(32, value, true);
  }
  public get clipHeight(): number {
    return this.view.getFloat64(40, true);
  }
  public set clipHeight(value: number) {
    this.view.setFloat64(40, value, true);
  }
}

function copyDisplayConfiguration(display?: DisplayConfiguration): DisplayConfiguration {
  if (display === undefined) {
    return { remap: identityRemap(), raster: [] };
  }
  if (!validRemap(display.remap)) {
    throw new TypeError('display configuration has an invalid base remap');
  }
  let previousLine = -1;
  const raster = display.raster.map((state) => {
    if (
      !Number.isInteger(state.line) ||
      state.line <= previousLine ||
      state.line >= HARDWARE.height ||
      !Number.isSafeInteger(state.scrollX) ||
      !Number.isSafeInteger(state.scrollY) ||
      state.scrollX < -32_768 ||
      state.scrollX > 32_767 ||
      state.scrollY < -32_768 ||
      state.scrollY > 32_767 ||
      !validRemap(state.remap)
    ) {
      throw new TypeError('display configuration has invalid raster state');
    }
    previousLine = state.line;
    return { ...state, remap: state.remap.slice() };
  });
  return { remap: display.remap.slice(), raster };
}

function validRemap(remap: Uint8Array): boolean {
  return (
    remap.length === HARDWARE.rasterPaletteSize &&
    remap.every((color) => color < HARDWARE.paletteSize)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function identityRemap(): Uint8Array {
  return Uint8Array.from({ length: HARDWARE.rasterPaletteSize }, (_, index) => index);
}

function expectIntegers(command: ConsoleCommand, count: 0): [];
function expectIntegers(command: ConsoleCommand, count: 1): [number];
function expectIntegers(command: ConsoleCommand, count: 2): [number, number];
function expectIntegers(command: ConsoleCommand, count: 3): [number, number, number];
function expectIntegers(command: ConsoleCommand, count: 4): [number, number, number, number];
function expectIntegers(
  command: ConsoleCommand,
  count: 5,
): [number, number, number, number, number];
function expectIntegers(
  command: ConsoleCommand,
  count: 7,
): [number, number, number, number, number, number, number];
function expectIntegers(command: ConsoleCommand, count: number): number[] {
  if (command.arguments.length !== count) {
    throw new TypeError(
      `${command.name} expected ${String(count)} arguments, received ${String(command.arguments.length)}`,
    );
  }
  return command.arguments.map(expectInteger);
}

function expectInteger(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) {
    throw new TypeError('graphics arguments must be safe integers');
  }
  return value;
}

function expectBoolean(value: unknown): boolean {
  if (typeof value !== 'boolean') {
    throw new TypeError('graphics argument must be Bool');
  }
  return value;
}

function expectText(value: unknown): string {
  if (typeof value !== 'string') {
    throw new TypeError('graphics argument must be Text');
  }
  return value;
}

function expectColor(value: number): number {
  if (value < 0 || value >= HARDWARE.paletteSize) {
    throw new RangeError('palette index must be between 0 and 255');
  }
  return value;
}

function expectRemappableColor(value: number): number {
  if (value < 0 || value >= HARDWARE.rasterPaletteSize) {
    throw new RangeError('palette remap source must be between 0 and 31');
  }
  return value;
}

function readAssetName(value: unknown, expectedKind: string): string {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('name' in value) ||
    typeof value.name !== 'string' ||
    !('kind' in value) ||
    value.kind !== expectedKind
  ) {
    throw new TypeError(`expected a ${expectedKind} asset handle`);
  }
  return value.name;
}

function edge(
  firstX: number,
  firstY: number,
  secondX: number,
  secondY: number,
  pointX: number,
  pointY: number,
): number {
  return (pointX - firstX) * (secondY - firstY) - (pointY - firstY) * (secondX - firstX);
}

function unrotate(
  x: number,
  y: number,
  width: number,
  height: number,
  quarterTurns: number,
): [number, number] {
  switch (quarterTurns) {
    case 1:
      return [y, height - 1 - x];
    case 2:
      return [width - 1 - x, height - 1 - y];
    case 3:
      return [width - 1 - y, x];
    default:
      return [x, y];
  }
}

function wrap(value: number, modulus: number): number {
  return ((value % modulus) + modulus) % modulus;
}

function clampInt16(value: number): number {
  return Math.max(-32_768, Math.min(32_767, value));
}
