import palette from './palette.json' with { type: 'json' };

/** Default seed shared by native play and CLI headless runs; explicit seeds remain supported. */
export const DEFAULT_SEED = 604_772_761;

export const HARDWARE = Object.freeze({
  width: 240,
  height: 144,
  frameRate: 60,
  paletteSize: 256,
  rasterPaletteSize: 32,
  transparentColor: 0,
  visualCapacityBytes: 1024 * 1024,
  visualAllocations: 8192,
  saveCapacityBytes: 8 * 1024,
  cartridgeCapacityBytes: 1024 * 1024,
  drawCommandsPerFrame: 4096,
  workUnitsPerFrame: 50_000,
  audioVoices: 8,
  audioSampleRate: 48_000,
  trackerChannels: 8,
  controllerPorts: 4,
  spriteMaximumAxis: 128,
  tileSize: 8,
} as const);

export type HardwareProfile = typeof HARDWARE;

/** Shared immutable 256-colour data, also embedded by modl-core for native tools. */
export const MASTER_PALETTE = Object.freeze(
  palette.map((rgb) => `#${rgb.map((value) => value.toString(16).padStart(2, '0')).join('')}`),
);

export const MASTER_PALETTE_RGBA = Object.freeze(
  MASTER_PALETTE.flatMap((hex) => [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
    255,
  ]),
);
