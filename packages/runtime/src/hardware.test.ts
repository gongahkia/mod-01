import { describe, expect, it } from 'vitest';

import { HARDWARE } from './index';

describe('revision 2 hardware profile', () => {
  it('keeps the display geometry and expanded limits explicit', () => {
    expect(HARDWARE).toEqual({
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
    });
  });
});
