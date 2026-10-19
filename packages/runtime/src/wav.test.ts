import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';

import { renderMusicWav, renderSoundWav } from './wav';
import { Synthesizer, type MusicAsset, type SoundAsset } from './audio';

const sound: SoundAsset = {
  kind: 'sound',
  name: 'tone',
  waveform: 'pulse',
  note: 60,
  durationFrames: 2,
  volume: 0.5,
  pan: 0,
  duty: 0.5,
  envelope: { attackFrames: 0, decayFrames: 0, sustainLevel: 1, releaseFrames: 1 },
  pitch: { slideSemitonesPerFrame: 0, vibratoDepthSemitones: 0, vibratoPeriodFrames: 0 },
};

describe('offline production-synth WAV', () => {
  it('writes a parseable deterministic stereo PCM sound fixture', () => {
    const wav = renderSoundWav(sound);
    const view = new DataView(wav.buffer);
    expect(new TextDecoder().decode(wav.subarray(0, 4))).toBe('RIFF');
    expect(new TextDecoder().decode(wav.subarray(8, 12))).toBe('WAVE');
    expect(view.getUint32(24, true)).toBe(48_000);
    expect(view.getUint16(22, true)).toBe(2);
    expect(view.getUint16(34, true)).toBe(16);
    expect(view.getUint32(40, true)).toBe(3 * 800 * 4);
    expect(createHash('sha256').update(wav).digest('hex')).toBe(
      '094f8a60e627c49f6c6209704bd24bb3e6c64e0517cc938b05fee1360e85c503',
    );
  });

  it('renders one tracker order through the same synthesizer', () => {
    const music: MusicAsset = {
      kind: 'music',
      name: 'song',
      framesPerRow: 2,
      order: ['00'],
      patterns: {
        '00': { rows: [[{ note: 60, sound: 'tone' }, null, null, null, null, null, null, null]] },
      },
      loop: true,
    };
    const wav = renderMusicWav(music, [sound]);
    expect(new DataView(wav.buffer).getUint32(40, true)).toBe(2 * 800 * 4);
    expect(() => renderMusicWav({ ...music, framesPerRow: 60 * 60 * 11 }, [sound])).toThrow(
      /ten minutes/,
    );
  });

  it('preflights exact byte limits including release before executing any synth frames', () => {
    const frames = sound.durationFrames + sound.envelope.releaseFrames;
    const bytes = 44 + frames * 800 * 4;
    expect(renderSoundWav(sound, bytes)).toEqual(renderSoundWav(sound));
    const song: MusicAsset = {
      kind: 'music',
      name: 'song',
      framesPerRow: 3,
      order: ['A'],
      patterns: { A: { rows: [Array<null>(8).fill(null)] } },
      loop: false,
    };
    expect(renderMusicWav(song, [sound], bytes)).toEqual(renderMusicWav(song, [sound]));
    const execute = vi.spyOn(Synthesizer.prototype, 'executeFrame');
    try {
      expect(() => renderSoundWav(sound, bytes - 1)).toThrow(/byte limit/);
      expect(() => renderMusicWav(song, [sound], bytes - 1)).toThrow(/byte limit/);
      const nativeMaximum = 8 * 1024 * 1024;
      expect(() =>
        renderSoundWav(
          { ...sound, durationFrames: 2621, envelope: { ...sound.envelope, releaseFrames: 1 } },
          nativeMaximum,
        ),
      ).toThrow(/8388608-byte limit/);
      expect(() =>
        renderMusicWav(
          {
            ...song,
            framesPerRow: 240,
            patterns: { A: { rows: Array.from({ length: 256 }, () => Array<null>(8).fill(null)) } },
          },
          [sound],
          nativeMaximum,
        ),
      ).toThrow(/ten minutes|byte limit/);
      expect(() =>
        renderMusicWav(
          {
            ...song,
            framesPerRow: 240,
            patterns: { A: { rows: Array.from({ length: 128 }, () => Array<null>(8).fill(null)) } },
          },
          [sound],
          nativeMaximum,
        ),
      ).toThrow(/8388608-byte limit/);
      expect(execute).not.toHaveBeenCalled();
      for (const maximum of [43, -1, 44.5, Number.NaN, Number.POSITIVE_INFINITY]) {
        expect(() => renderSoundWav(sound, maximum)).toThrow(/maximum WAV bytes/);
      }
      expect(() => renderSoundWav({ ...sound, durationFrames: 1.5 }, nativeMaximum)).toThrow();
      expect(execute).not.toHaveBeenCalled();
    } finally {
      execute.mockRestore();
    }
  });

  it('admits the largest whole-frame native output without reducing the default renderer capacity', () => {
    const maximum = 8 * 1024 * 1024;
    const frames = Math.floor((maximum - 44) / 3200);
    const patch = {
      ...sound,
      durationFrames: frames,
      envelope: { ...sound.envelope, releaseFrames: 0 },
    };
    const wav = renderSoundWav(patch, maximum);
    expect(wav.byteLength).toBe(44 + frames * 3200);
    expect(wav.byteLength).toBeLessThanOrEqual(maximum);
    expect(new DataView(wav.buffer).getUint32(40, true)).toBe(frames * 3200);
    expect(() => renderSoundWav({ ...patch, durationFrames: frames + 1 }, maximum)).toThrow(
      /byte limit/,
    );
    expect(renderSoundWav({ ...patch, durationFrames: frames + 1 }).byteLength).toBeGreaterThan(
      maximum,
    );
  });
});
