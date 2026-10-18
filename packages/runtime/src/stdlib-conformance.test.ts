import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createConsoleRuntime } from './console-runtime';
import { runHeadless } from './headless';
import { emptyInputFrame } from './input';
import type { CartridgeFactory } from './machine';

describe('compiled standard library across production hosts', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  let temporary: string;
  beforeAll(() => {
    temporary = mkdtempSync(join(tmpdir(), 'mod01-stdlib-'));
    execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
  }, 120_000);
  afterAll(() => {
    if (temporary) rmSync(temporary, { recursive: true, force: true });
  });

  for (const fixture of ['math', 'text', 'bits']) {
    for (const debug of [false, true]) {
      it(`executes ${fixture} assertions in ${debug ? 'debug' : 'release'}, native and headless`, async () => {
        const output = join(temporary, `${fixture}-${String(debug)}.mjs`);
        execFileSync(join(root, 'target/debug/mod01'), [
          'build',
          join(root, `tests/conformance/${fixture}.modl`),
          '--output',
          output,
          ...(debug ? ['--debug'] : []),
        ]);
        const generated = (await import(/* @vite-ignore */ pathToFileURL(output).href)) as {
          default: CartridgeFactory;
        };
        const runtime = createConsoleRuntime(generated.default, {
          seed: 0,
          updateRate: 60,
          workUnitsPerFrame: 50_000,
          debug,
        });
        const first = runtime.runFrame(emptyInputFrame());
        expect(first.output.indexedPixels[34 * 240 + 12]).toBe(11);
        const checkpoint = runtime.snapshot();
        const second = runtime.runFrame(emptyInputFrame());
        runtime.restore(checkpoint);
        expect(runtime.runFrame(emptyInputFrame())).toEqual(second);

        const javascript = readFileSync(output, 'utf8');
        const manifest = { id: 'stdlib', updateRate: 60, display: null, assets: {} };
        const headless = await runHeadless({
          revision: 1,
          javascript,
          manifest,
          entries: {},
          rom: [1],
          seed: 0,
          frames: 2,
          trace: { revision: 1, frames: [] },
          save: [],
        });
        expect(headless.fault).toBeUndefined();
        expect(headless.summary.completedFrames).toBe(2);
        const host = join(root, 'packages/runtime/standalone/native-host.mjs');
        const replies = execFileSync(
          process.execPath,
          ['--permission', `--allow-fs-read=${host}`, host],
          {
            input:
              [
                { type: 'load', javascript, manifest, files: {}, rom: 'AQ==', save: '', debug },
                { type: 'frame', input: emptyInputFrame() },
                { type: 'stop' },
              ]
                .map((value) => JSON.stringify(value))
                .join('\n') + '\n',
          },
        )
          .toString()
          .trim()
          .split('\n')
          .map((line) => JSON.parse(line) as Record<string, unknown>);
        expect(replies.map((reply) => reply.type)).toEqual(['loaded', 'frame', 'stopped']);
        const frame = replies[1];
        expect(Buffer.from(frame?.pixels as string, 'base64')).toEqual(
          Buffer.from(first.output.indexedPixels),
        );
        expect(frame?.workUnits).toBe(first.workUnits);
      });
    }
  }
});
