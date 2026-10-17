import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { emptyInputFrame } from './input';

const host = fileURLToPath(new URL('../standalone/native-host.mjs', import.meta.url));
const javascript = `export default function createCartridge(api) {
  let frame = 0;
  return {
    start() {}, update() { frame++; },
    draw() { api.call('pixel', [frame - 1, 0, 255], { start: 1, end: 2 }); },
    raster() {},
    snapshot() { return { state: { frame }, tasks: [], nextTaskId: 1 }; },
    restore(value) { frame = value.state.frame; },
    inspect() { return { state: { frame }, tasks: [], callStack: [] }; }
  };
}`;
const debugJavascript = `export default function createCartridge() {
  let stepped = false;
  return {
    start() {
      if (!stepped) { stepped = true; return { done: false, event: { id: 1, sourceSpan: { start: 0, end: 1, source: 'src/main.modl' }, locals: { s1: 3 } } }; }
      return { done: true };
    },
    update() { return { done: true }; },
    draw() { return { done: true }; },
    raster() { return { done: true }; },
    snapshot() { return { state: { s0: 2 }, tasks: [], nextTaskId: 1 }; },
    restore() {},
    inspect() { return { state: { s0: 2 }, tasks: [], callStack: [] }; }
  };
}`;

describe('native process host', () => {
  it('runs revision-2 frames and restores an opaque snapshot', async () => {
    const child = spawn(process.execPath, ['--permission', `--allow-fs-read=${host}`, host], {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { PATH: process.env.PATH ?? '' },
    });
    const output = createInterface({ input: child.stdout });
    const replies = output[Symbol.asyncIterator]();
    const request = async (command: unknown): Promise<Record<string, unknown>> => {
      child.stdin.write(`${JSON.stringify(command)}\n`);
      const reply = await replies.next();
      if (reply.done) throw new Error('native host exited before its response');
      return JSON.parse(reply.value) as Record<string, unknown>;
    };
    try {
      const preview = await request({
        type: 'render-wav',
        kind: 'sound',
        asset: {
          kind: 'sound',
          name: 'blip',
          waveform: 'pulse',
          note: 60,
          durationFrames: 1,
          volume: 0.5,
          pan: 0,
          duty: 0.5,
          envelope: { attackFrames: 0, decayFrames: 0, sustainLevel: 1, releaseFrames: 0 },
          pitch: { slideSemitonesPerFrame: 0, vibratoDepthSemitones: 0, vibratoPeriodFrames: 0 },
        },
        sounds: [],
      });
      expect(preview.type).toBe('wav');
      expect(
        Buffer.from(preview.bytes as string, 'base64')
          .subarray(0, 4)
          .toString(),
      ).toBe('RIFF');
      expect(
        await request({
          type: 'load',
          javascript,
          manifest: {
            id: 'native-test',
            hardwareRevision: 2,
            updateRate: 60,
            display: null,
            assets: {},
          },
          files: {},
          rom: Buffer.from([77, 79, 68, 45, 48, 49, 26, 1, 0, 0, 0, 0]).toString('base64'),
          save: '',
        }),
      ).toMatchObject({ type: 'loaded', cartridgeId: 'native-test' });
      const first = await request({ type: 'frame', input: emptyInputFrame() });
      expect(first.type).toBe('frame');
      expect(Buffer.from(first.pixels as string, 'base64')[0]).toBe(255);
      expect(Buffer.from(first.left as string, 'base64').length).toBe(800 * 4);
      const snapshot = await request({ type: 'snapshot' });
      expect(snapshot.type).toBe('snapshot');
      const second = await request({ type: 'frame', input: emptyInputFrame() });
      expect(await request({ type: 'restore', snapshot: snapshot.snapshot })).toMatchObject({
        type: 'restored',
      });
      const replayed = await request({ type: 'frame', input: emptyInputFrame() });
      expect(replayed.pixels).toBe(second.pixels);
      expect(await request({ type: 'stop' })).toMatchObject({ type: 'stopped' });
      const debugLoad = await request({
        type: 'load',
        javascript: debugJavascript,
        debug: true,
        manifest: {
          id: 'native-debug',
          hardwareRevision: 2,
          updateRate: 60,
          display: null,
          assets: {},
        },
        files: {},
        rom: Buffer.from([77, 79, 68, 45, 48, 49, 26, 1, 0, 0, 0, 0]).toString('base64'),
        save: '',
        symbolNames: { s0: 'score', s1: 'health' },
      });
      if (debugLoad.type === 'error') throw new Error(String(debugLoad.message));
      expect(debugLoad).toMatchObject({ type: 'loaded', cartridgeId: 'native-debug' });
      const paused = await request({
        type: 'step',
        input: emptyInputFrame(),
        watches: ['score + health'],
        conditions: ['score == 2', 'score == 3'],
      });
      expect(paused).toMatchObject({
        type: 'step',
        watches: [{ value: 5 }],
        conditions: [{ value: true }, { value: false }],
      });
    } finally {
      output.close();
      child.kill();
    }
  });
});
