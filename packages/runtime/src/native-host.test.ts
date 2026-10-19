import { execFileSync, spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { emptyInputFrame } from './input';
import { runtimeGameFixture } from './test-support/game';

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
const debugJavascript = `export default function createCartridge(api) {
  let stepped = false;
  let score = 2;
  return {
    start() {
      if (!stepped) { stepped = true; api.work(12, {start: 0, end: 1}); api.call('pixel', [1, 2, 255], {start: 0, end: 1}); return { done: false, event: { id: 1, sourceSpan: { start: 0, end: 1, source: 'src/main.modl' }, locals: { s1: 3 } } }; }
      return { done: true };
    },
    update() { score = 4; return { done: true }; },
    draw() { return { done: true }; },
    raster() { return { done: true }; },
    snapshot() { return { state: { s0: score }, tasks: [], nextTaskId: 1 }; },
    restore(value) { score = value.state.s0; },
    inspect() { return { state: { s0: score }, tasks: [], callStack: [] }; }
  };
}`;

describe('native process host', () => {
  it('requires the current whole-game load command without an entry-only fallback', () => {
    const game = runtimeGameFixture({
      javascript,
      manifest: { id: 'current-native', updateRate: 60, display: null, assets: {} },
    });
    const commands = [
      {
        type: 'load',
        javascript,
        manifest: game.members.main?.manifest,
        files: {},
        rom: 'AQ==',
        save: '',
      },
      { type: 'load', game, parameter: '', saves: null },
      { type: 'load', game, parameter: '', saves: { 'current-native': 'AB==' } },
      { type: 'load', game, parameter: '', saves: {} },
      { type: 'stop' },
    ];
    const replies = execFileSync(
      process.execPath,
      ['--permission', `--allow-fs-read=${host}`, host],
      {
        input: commands.map((command) => JSON.stringify(command)).join('\n') + '\n',
      },
    )
      .toString()
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line) as Record<string, unknown>);
    expect(replies.map((reply) => reply.type)).toEqual([
      'error',
      'error',
      'error',
      'loaded',
      'stopped',
    ]);
    expect(replies[3]).toMatchObject({
      cartridgeAlias: 'main',
      cartridgeId: 'current-native',
      active: { frame: 0, cartridgeFrame: 0, booted: true },
    });
  });
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
          game: runtimeGameFixture({
            javascript,
            manifest: {
              id: 'native-test',
              updateRate: 60,
              display: null,
              assets: {},
            },
          }),
          parameter: '',
          saves: {},
        }),
      ).toMatchObject({ type: 'loaded', cartridgeId: 'native-test' });
      const first = await request({ type: 'frame', input: emptyInputFrame() });
      expect(first.type).toBe('frame');
      expect(Buffer.from(first.pixels as string, 'base64')[0]).toBe(255);
      expect(Buffer.from(first.left as string, 'base64').length).toBe(800 * 4);
      const snapshot = await request({ type: 'snapshot' });
      expect(snapshot).toMatchObject({ type: 'snapshot', frame: 1, booted: true });
      const second = await request({ type: 'frame', input: emptyInputFrame() });
      expect(await request({ type: 'restore', snapshot: snapshot.snapshot })).toMatchObject({
        type: 'restored',
        frame: 1,
        booted: true,
      });
      const replayed = await request({ type: 'frame', input: emptyInputFrame() });
      expect(replayed.pixels).toBe(second.pixels);
      expect(await request({ type: 'stop' })).toMatchObject({ type: 'stopped' });
      const debugCommand = {
        type: 'load',
        game: runtimeGameFixture({
          javascript: debugJavascript,
          debug: true,
          manifest: {
            id: 'native-debug',
            updateRate: 60,
            display: null,
            assets: {},
          },
          files: {},
          symbolNames: { s0: 'score', s1: 'health' },
        }),
        parameter: '',
        saves: {},
      };
      const debugLoad = await request(debugCommand);
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
      expect(await request(debugCommand)).toMatchObject({ type: 'loaded' });
      const scratch = await request({ type: 'scratch', input: emptyInputFrame() });
      expect(scratch).toMatchObject({
        type: 'scratch',
        locals: { health: 3 },
        state: { score: 4 },
        bootWorkUnits: 13,
      });
      expect(Buffer.from(scratch.pixels as string, 'base64')[2 * 240 + 1]).toBe(255);
      expect(Buffer.from(scratch.left as string, 'base64').length).toBe(800 * 4);
      expect(await request({ type: 'scratch', input: emptyInputFrame() })).toMatchObject({
        type: 'error',
        code: 'M019100',
        message: 'scratchpad requires a freshly loaded debug program',
      });
      expect(await request(debugCommand)).toMatchObject({ type: 'loaded' });
      expect(
        await request({ type: 'memory-edit', address: 0x200000, bytes: 'AA==' }),
      ).toMatchObject({
        type: 'error',
        code: 'M019021',
        sourceSpan: { start: 0, end: 0 },
      });
      expect(await request({ type: 'stop' })).toMatchObject({ type: 'stopped' });
    } finally {
      output.close();
      child.kill();
    }
  });
});
