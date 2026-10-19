import { describe, expect, it } from 'vitest';
import { Synthesizer } from './audio';
import { HARDWARE } from './hardware';
import { isConsoleConfiguration, isConsoleOutput } from './protocol';

describe('shared native console protocol', () => {
  it('enforces the fixed work ceiling and bounded cartridge ROM', () => {
    const config = { seed: 1, updateRate: 60, workUnitsPerFrame: 50_000 };
    expect(isConsoleConfiguration(config)).toBe(true);
    for (const workUnitsPerFrame of [0, -1, 50_001, NaN, 1.5])
      expect(isConsoleConfiguration({ ...config, workUnitsPerFrame })).toBe(false);
    expect(isConsoleConfiguration({ ...config, rom: Uint8Array.of(1) })).toBe(true);
    expect(
      isConsoleConfiguration({
        ...config,
        rom: new Uint8Array(HARDWARE.cartridgeCapacityBytes + 1),
      }),
    ).toBe(false);
    expect(isConsoleConfiguration({ ...config, ambient: true })).toBe(false);
  });

  it('rejects malformed device output', () => {
    const synthesizer = new Synthesizer();
    const audio = synthesizer.finishFrame();
    const output = {
      indexedPixels: new Uint8Array(HARDWARE.width * HARDWARE.height),
      audio,
      audioState: synthesizer.snapshot(),
    };
    expect(isConsoleOutput(output)).toBe(true);
    for (const invalid of [
      undefined,
      { ...output, indexedPixels: new Uint8Array(1) },
      { ...output, indexedPixels: new Uint16Array(HARDWARE.width * HARDWARE.height) },
      { ...output, audio: { ...audio, left: new Float32Array(audio.left.length).fill(NaN) } },
      { ...output, audio: { ...audio, activeVoices: 9 } },
      { ...output, audioState: { ...output.audioState, voices: [] } },
      { ...output, ambient: true },
    ])
      expect(isConsoleOutput(invalid)).toBe(false);
  });
});
