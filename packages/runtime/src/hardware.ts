export const HARDWARE = Object.freeze({
  width: 240,
  height: 144,
  frameRate: 60,
  paletteSize: 256,
  rasterPaletteSize: 32,
  transparentColor: 0,
  visualCapacityBytes: 1024 * 1024,
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

/** Original MOD-01 RGB master palette. Logical index 0 is also the sprite transparency key. */
const ORIGINAL_PALETTE = [
  '#17141f',
  '#292532',
  '#403946',
  '#5d5054',
  '#806a63',
  '#aa8b74',
  '#d5b992',
  '#f4e5bd',
  '#5b2938',
  '#8b3c47',
  '#bf5558',
  '#ed7b69',
  '#5a3928',
  '#89572e',
  '#c18436',
  '#e7bd50',
  '#263c32',
  '#345f46',
  '#4b8b58',
  '#7fbd68',
  '#203b47',
  '#2e6571',
  '#43969a',
  '#75cbc0',
  '#243451',
  '#345581',
  '#4b7db3',
  '#73a9d1',
  '#3e3154',
  '#654777',
  '#936397',
  '#c38aae',
] as const;

/** Hardware Revision 2 keeps the original 32 colours, then adds a 216-colour cube and eight greys. */
export const MASTER_PALETTE = Object.freeze([
  ...ORIGINAL_PALETTE,
  ...Array.from({ length: 216 }, (_, index) => {
    const red = Math.floor(index / 36) * 51;
    const green = (Math.floor(index / 6) % 6) * 51;
    const blue = (index % 6) * 51;
    return `#${[red, green, blue].map((value) => value.toString(16).padStart(2, '0')).join('')}`;
  }),
  ...Array.from({ length: 8 }, (_, index) => {
    const value = (index * 29 + 26).toString(16).padStart(2, '0');
    return `#${value}${value}${value}`;
  }),
]);

export const MASTER_PALETTE_RGBA = Object.freeze(
  MASTER_PALETTE.flatMap((hex) => [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
    255,
  ]),
);
