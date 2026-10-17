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
    } finally {
      output.close();
      child.kill();
    }
  });
});
