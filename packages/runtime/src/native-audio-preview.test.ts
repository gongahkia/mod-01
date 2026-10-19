import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { AudioAssetStore, Synthesizer, type SoundAsset, type MusicAsset } from './audio';
import { emptyInputFrame } from './input';

const sound: SoundAsset = {
  kind: 'sound',
  name: 'tone',
  waveform: 'triangle',
  note: 60,
  durationFrames: 4,
  volume: 0.5,
  pan: 0,
  envelope: { attackFrames: 0, decayFrames: 0, sustainLevel: 1, releaseFrames: 1 },
  pitch: { slideSemitonesPerFrame: 0, vibratoDepthSemitones: 0, vibratoPeriodFrames: 0 },
};
const music: MusicAsset = {
  kind: 'music',
  name: 'song',
  framesPerRow: 2,
  loop: false,
  order: ['a', 'b'],
  patterns: {
    a: { rows: [[null, null, null, null, null, null, null, null]] },
    b: {
      rows: [
        Array.from({ length: 8 }, (_, channel) => ({
          sound: 'tone',
          note: 60 + channel,
          volume: 0.5,
        })),
      ],
    },
  },
};

describe('native live audio-preview transport', () => {
  it('streams actual synth PCM, voices and pre/post row positions with no cartridge or save state', async () => {
    const host = fileURLToPath(new URL('../standalone/native-host.mjs', import.meta.url));
    const child = spawn(process.execPath, ['--permission', `--allow-fs-read=${host}`, host], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    const request = async (command: unknown): Promise<Record<string, unknown>> => {
      child.stdin.write(`${JSON.stringify(command)}\n`);
      const result = await lines.next();
      if (result.done) throw new Error('audio preview process stopped');
      return JSON.parse(result.value) as Record<string, unknown>;
    };
    const synth = new Synthesizer(new AudioAssetStore([sound, music]));
    const frame = async (index: number) => {
      const position = synth.inspectTracker();
      const expected = synth.finishFrame();
      const result = await request({ type: 'audio-preview-frame' });
      expect(result).toMatchObject({
        type: 'audio-preview-frame',
        frame: index,
        position,
        nextPosition: expected.tracker,
        activeVoices: expected.activeVoices,
      });
      expect(Buffer.from(result.left as string, 'base64')).toEqual(
        Buffer.from(expected.left.buffer),
      );
      expect(Buffer.from(result.right as string, 'base64')).toEqual(
        Buffer.from(expected.right.buffer),
      );
      expect(result.voices).toEqual(synth.inspectVoices());
      expect(result.peakLeft).toBe(
        expected.left.reduce((peak, sample) => Math.max(peak, Math.abs(sample)), 0),
      );
      expect(result).not.toHaveProperty('saveCommit');
      expect(result).not.toHaveProperty('pixels');
    };
    try {
      expect(await request({ type: 'audio-preview-frame' })).toMatchObject({ type: 'error' });
      expect(await request({ type: 'audio-preview-load', assets: [sound, music] })).toEqual({
        type: 'audio-preview-loaded',
      });
      synth.audition('tone', 64, 0.25);
      expect(
        await request({ type: 'audio-preview-audition', sound: 'tone', note: 64, volume: 0.25 }),
      ).toEqual({ type: 'audio-preview-auditioned' });
      await frame(0);
      synth.playMusic('song', 1, 0);
      expect(
        await request({ type: 'audio-preview-play', music: 'song', orderIndex: 1, row: 0 }),
      ).toMatchObject({ type: 'audio-preview-playing', position: synth.inspectTracker() });
      await frame(1);
      await frame(2);
      await frame(3);
      expect(
        await request({ type: 'audio-preview-play', music: 'song', orderIndex: 9, row: 0 }),
      ).toMatchObject({ type: 'error' });
      expect(
        await request({ type: 'audio-preview-audition', sound: 'tone', note: 128 }),
      ).toMatchObject({ type: 'error' });
      await frame(4);
      expect(await request({ type: 'frame', input: emptyInputFrame() })).toMatchObject({
        type: 'error',
      });
      expect(await request({ type: 'stop' })).toEqual({ type: 'stopped' });
      expect(await request({ type: 'audio-preview-frame' })).toMatchObject({ type: 'error' });
      const invalid = {
        ...music,
        patterns: {
          b: { rows: [[{ note: 60, sound: 'absent' }, null, null, null, null, null, null, null]] },
        },
      };
      expect(await request({ type: 'audio-preview-load', assets: [sound, invalid] })).toMatchObject(
        { type: 'error' },
      );
      expect(await request({ type: 'audio-preview-frame' })).toMatchObject({ type: 'error' });
    } finally {
      child.stdin.end();
      child.kill();
    }
  });
});
