import { describe, expect, it } from 'vitest';
import { execFileSync } from 'node:child_process';

import { HARDWARE } from './index';

describe('revision 2 hardware profile', () => {
  it('loads the shared palette directly in Node without a bundler JSON loader', () => {
    const url = new URL('./hardware.ts', import.meta.url).href;
    const output = execFileSync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        `import {MASTER_PALETTE} from ${JSON.stringify(url)}; process.stdout.write(JSON.stringify(MASTER_PALETTE));`,
      ],
      { encoding: 'utf8' },
    );
    const palette = JSON.parse(output) as string[];
    expect(palette).toHaveLength(256);
    expect(palette[25]).toBe('#345581');
  });
  it('keeps the display geometry and expanded limits explicit', () => {
    expect(HARDWARE).toEqual({
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
    });
  });
});
