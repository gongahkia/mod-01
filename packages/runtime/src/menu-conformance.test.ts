import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createConsoleRuntime, type ConsoleFrame } from './console-runtime';
import { runHeadless } from './headless';
import { emptyInputFrame, type InputFrame } from './input';
import type { CartridgeFactory } from './machine';
import { decodeReplayTrace, encodeReplayTrace, replayInputFrames } from './replay';
import { runtimeGameFixture } from './test-support/game';

describe('actual MODL pause actions across release, debug, native and replay', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  let temporary: string;
  beforeAll(() => {
    temporary = mkdtempSync(join(tmpdir(), 'mod01-menu-'));
    execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
  }, 120_000);
  afterAll(() => {
    if (temporary) rmSync(temporary, { recursive: true, force: true });
  });
  const inputs: InputFrame[] = [
    emptyInputFrame(),
    { ...emptyInputFrame(), menuAction: 1 },
    emptyInputFrame(),
    { ...emptyInputFrame(), menuAction: 5 },
    emptyInputFrame(),
  ];
  const initialMenu = [
    { slot: 1, label: 'ADD TEN' },
    { slot: 5, label: 'FINISH' },
  ];
  const finalMenu = [{ slot: 1, label: 'ADD TEN AGAIN' }];

  for (const debug of [false, true]) {
    it(`dispatches and restores registered actions in ${debug ? 'debug' : 'release'} at 30 Hz`, async () => {
      const output = join(temporary, `menu-${String(debug)}.mjs`);
      execFileSync(join(root, 'target/debug/mod01'), [
        'build',
        join(root, 'tests/conformance/menu.modl'),
        '--output',
        output,
        ...(debug ? ['--debug'] : []),
      ]);
      const generated = (await import(/* @vite-ignore */ pathToFileURL(output).href)) as {
        default: CartridgeFactory;
      };
      const runtime = createConsoleRuntime(generated.default, {
        seed: 7,
        updateRate: 30,
        workUnitsPerFrame: 50_000,
        debug,
        rom: Uint8Array.of(1),
      });
      if (debug) {
        expect(runtime.inspectMenu()).toEqual([]);
        for (let step = 0; ; step++) {
          expect(step).toBeLessThan(100);
          if ('booted' in runtime.stepDebug(emptyInputFrame())) break;
        }
      }
      expect(runtime.inspectMenu()).toEqual(initialMenu);
      const frames: ConsoleFrame[] = [];
      let checkpoint = runtime.snapshot();
      let sawMenuPause = false;
      for (const [index, input] of inputs.entries()) {
        if (debug) {
          for (let step = 0; ; step++) {
            expect(step).toBeLessThan(100);
            // Later step requests deliberately omit the action; the machine must retain it.
            const result = runtime.stepDebug(step === 0 ? input : emptyInputFrame());
            if ('frame' in result) {
              frames.push(result.frame);
              break;
            }
            if ('event' in result && input.menuAction !== undefined) {
              sawMenuPause = true;
              expect(runtime.inspectMemory(0x50200, 1).bytes[0]).toBe(index);
              expect(() => runtime.snapshot()).toThrow(/completed/);
            }
          }
        } else frames.push(runtime.runFrame(input));
        expect(frames[index]?.output.indexedPixels[0]).toBe([1, 7, 7, 9, 9][index]);
        if (index === 1) checkpoint = runtime.snapshot();
      }
      expect(sawMenuPause).toBe(debug);
      expect(runtime.snapshot().machine.execution.updates).toBe(3);
      expect(runtime.inspectMenu()).toEqual(finalMenu);
      expect(frames.filter((frame) => frame.saveCommit !== undefined)).toHaveLength(2);
      runtime.restore(checkpoint);
      expect(runtime.inspectMenu()).toEqual([
        { slot: 1, label: 'ADD TEN AGAIN' },
        { slot: 5, label: 'FINISH' },
      ]);
      for (let index = 2; index < inputs.length; index++)
        expect(runtime.runFrame(inputs[index] ?? emptyInputFrame())).toEqual(frames[index]);

      const trace = decodeReplayTrace(
        encodeReplayTrace(inputs.map((input, frame) => ({ frame, input }))),
      );
      expect([...replayInputFrames(trace).values()]).toEqual(inputs);
      const javascript = readFileSync(output, 'utf8');
      const manifest = {
        id: 'menu-conformance',
        updateRate: 30 as const,
        display: null,
        assets: {},
      };
      const headless = await runHeadless({
        revision: 2,
        game: runtimeGameFixture({
          javascript,
          manifest,
          debug,
          sourceMap: readFileSync(output.replace(/\.mjs$/, '.js.map'), 'utf8'),
        }),
        seed: 7,
        frames: inputs.length,
        trace,
        parameter: '',
        saves: {},
      });
      expect(headless.fault).toBeUndefined();
      expect(headless.frames.map((frame) => frame.workUnits)).toEqual(
        frames.map((frame) => frame.workUnits),
      );
      expect(headless.frames.map((frame) => frame.framebufferSha256)).toEqual(
        frames.map((frame) =>
          createHash('sha256').update(frame.output.indexedPixels).digest('hex'),
        ),
      );

      const host = join(root, 'packages/runtime/standalone/native-host.mjs');
      const load = {
        type: 'load',
        game: runtimeGameFixture({
          javascript,
          manifest,
          files: {},
          debug,
        }),
        parameter: '',
        saves: {},
        seed: 7,
      };
      const native = (commands: unknown[]) =>
        execFileSync(process.execPath, ['--permission', `--allow-fs-read=${host}`, host], {
          maxBuffer: 32 * 1024 * 1024,
          timeout: 15_000,
          input:
            [...commands, { type: 'stop' }].map((value) => JSON.stringify(value)).join('\n') + '\n',
        })
          .toString()
          .trim()
          .split('\n')
          .map((line) => JSON.parse(line) as Record<string, unknown>);
      const replies = native([
        load,
        ...inputs.flatMap((input, index) => [
          { type: 'frame', input },
          ...(index === 1 ? [{ type: 'snapshot' }] : []),
        ]),
      ]);
      expect(replies.map((reply) => reply.type)).toEqual([
        'loaded',
        'frame',
        'frame',
        'snapshot',
        'frame',
        'frame',
        'frame',
        'stopped',
      ]);
      const nativeFrames = replies.filter((reply) => reply.type === 'frame');
      const compare = (reply: Record<string, unknown>, frame: ConsoleFrame) => {
        expect(reply.frame).toBe(frame.frame);
        expect(reply.workUnits).toBe(frame.workUnits);
        expect(Buffer.from(reply.pixels as string, 'base64')).toEqual(
          Buffer.from(frame.output.indexedPixels),
        );
        for (const channel of ['left', 'right'] as const)
          expect(Buffer.from(reply[channel] as string, 'base64')).toEqual(
            Buffer.from(frame.output.audio[channel].buffer),
          );
        expect(reply.saveCommit).toBe(
          frame.saveCommit === undefined ? null : Buffer.from(frame.saveCommit).toString('base64'),
        );
      };
      nativeFrames.forEach((reply, index) => {
        const frame = frames[index];
        if (frame === undefined) throw new Error('missing expected frame');
        compare(reply, frame);
      });
      expect(nativeFrames.at(-1)?.menuItems).toEqual(finalMenu);
      const saved = replies.find((reply) => reply.type === 'snapshot');
      expect(saved).toMatchObject({ frame: 2, booted: true });
      const restored = native([
        load,
        { type: 'restore', snapshot: saved?.snapshot },
        ...inputs.slice(2).map((input) => ({ type: 'frame', input })),
      ]);
      expect(restored[1]).toMatchObject({
        type: 'restored',
        frame: 2,
        menuItems: checkpoint.machine.menu.flatMap((label, index) =>
          label === null ? [] : [{ slot: index + 1, label }],
        ),
      });
      restored
        .filter((reply) => reply.type === 'frame')
        .forEach((reply, index) => {
          const frame = frames[index + 2];
          if (frame === undefined) throw new Error('missing restored frame');
          compare(reply, frame);
        });
    });
  }
});
