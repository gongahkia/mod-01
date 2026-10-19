import { execFileSync, spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { deepStrictEqual } from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createConsoleRuntime } from './console-runtime';
import { decodeReplayTrace, replayInputFrames } from './replay';
import { emptyInputFrame } from './input';
import { decodeSaveValues } from './save';
import type { ProjectAssetDeclaration } from './asset-codec';
import type { CartridgeFactory } from './machine';
import { runtimeGameFixture } from './test-support/game';

describe('source-visible software 3D game', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  const cli = join(root, 'target/debug/mod01');
  const project = join(root, 'cartridges/prism-corridor');
  let directory: string;
  beforeAll(() => {
    directory = mkdtempSync(join(tmpdir(), 'mod01-software-3d-'));
    execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
  }, 120_000);
  afterAll(() => {
    if (directory) rmSync(directory, { recursive: true, force: true });
  });

  for (const debug of [false, true])
    it(`plays the complete maze and restores real pixels/audio/saves in ${debug ? 'debug' : 'release'}`, async () => {
      const output = join(directory, `maze-${String(debug)}.mjs`);
      execFileSync(cli, ['build', project, '--output', output, ...(debug ? ['--debug'] : [])]);
      const generated = (await import(/* @vite-ignore */ pathToFileURL(output).href)) as {
        default: CartridgeFactory;
      };
      const view = JSON.parse(
        execFileSync(cli, ['info', project, '--source'], { encoding: 'utf8' }),
      ) as {
        manifest: { assets: Record<string, ProjectAssetDeclaration> };
        files: Record<string, number[]>;
      };
      const model = JSON.parse(execFileSync(cli, ['graph', project], { encoding: 'utf8' })) as {
        symbols: { id: number; name: string; kind: string }[];
      };
      const symbols = Object.fromEntries(
        model.symbols
          .filter((symbol) => symbol.kind === 'state')
          .map((symbol) => [symbol.name, `s${String(symbol.id)}`]),
      );
      const complete = model.symbols.find(
        (symbol) => symbol.name === 'Complete' && symbol.kind === 'variant',
      );
      const runtime = createConsoleRuntime(generated.default, {
        seed: 1,
        updateRate: 60,
        workUnitsPerFrame: 50_000,
        debug,
        assets: {
          declarations: view.manifest.assets,
          files: Object.fromEntries(
            Object.entries(view.files).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
          ),
        },
      });
      const state = () => {
        const current = runtime.snapshot().machine.cartridge.state as Record<string, unknown>;
        return Object.fromEntries(
          Object.entries(symbols).map(([name, key]) => [name, current[key]]),
        );
      };
      const inputs = replayInputFrames(
        decodeReplayTrace(readFileSync(join(root, 'tests/replays/prism-corridor.json'))),
      );
      let workPeak = 0,
        textureCalls = 0,
        chimes = 0;
      let committed: Uint8Array | undefined;
      const pixels = new Map<number, string>();
      const pcm = new Map<number, string>();
      for (let frame = 0; frame < 416; frame++) {
        const input = inputs.get(frame) ?? emptyInputFrame();
        const checkpoint = frame === 220 || frame === 403 ? runtime.snapshot() : undefined;
        const result = runtime.runFrame(input);
        workPeak = Math.max(workPeak, result.workUnits);
        textureCalls += result.drawCommands.filter(
          (command) => command.name === 'texture_line',
        ).length;
        chimes += result.audioCommands.filter((command) => command.name === 'sfx').length;
        if (result.audioCommands.length > 0)
          pcm.set(
            frame,
            createHash('sha256')
              .update(
                Buffer.from(
                  result.output.audio.left.buffer,
                  result.output.audio.left.byteOffset,
                  result.output.audio.left.byteLength,
                ),
              )
              .update(
                Buffer.from(
                  result.output.audio.right.buffer,
                  result.output.audio.right.byteOffset,
                  result.output.audio.right.byteLength,
                ),
              )
              .digest('hex'),
          );
        if (result.saveCommit) committed = result.saveCommit;
        if ([0, 130, 220, 315, 415].includes(frame))
          pixels.set(frame, createHash('sha256').update(result.output.indexedPixels).digest('hex'));
        if (checkpoint) {
          const after = runtime.snapshot();
          runtime.restore(checkpoint);
          deepStrictEqual(runtime.runFrame(input), result);
          deepStrictEqual(runtime.snapshot(), after);
        }
        if (frame === 130) {
          expect(state().px).toBeCloseTo(1.5);
          expect(state().py).toBeCloseTo(5.5);
        }
      }
      const final = state();
      expect(complete).toBeDefined();
      expect(final.mode).toMatchObject({ __variant: complete?.id });
      expect(final.has_prism).toBe(true);
      expect(final.px).toBeCloseTo(6.5);
      expect(final.py as number).toBeLessThan(2);
      expect(final.best).toBe(final.elapsed);
      expect(decodeSaveValues(committed ?? new Uint8Array())).toEqual({ best: final.best });
      expect(chimes).toBe(2);
      expect(textureCalls).toBe(416 * 295);
      expect(workPeak).toBeLessThanOrEqual(50_000);
      expect(new Set(pixels.values()).size).toBe(5);

      // Stream replies: a long debug route legitimately exceeds a whole-process stdout buffer.
      const host = join(root, 'packages/runtime/standalone/native-host.mjs');
      const child = spawn(process.execPath, ['--permission', `--allow-fs-read=${host}`, host], {
        timeout: 120_000,
      });
      let stderr = '';
      child.stderr.on('data', (bytes: Buffer) => {
        stderr = (stderr + bytes.toString()).slice(-4096);
      });
      const exited = new Promise<number | null>((resolve) => {
        child.once('error', () => {
          resolve(-1);
        });
        child.once('close', resolve);
      });
      const commands = [
        {
          type: 'load',
          game: runtimeGameFixture({
            javascript: readFileSync(output, 'utf8'),
            debug,
            manifest: {
              id: 'prism-corridor',
              updateRate: 60,
              display: null,
              assets: view.manifest.assets,
            },
            files: view.files,
          }),
          parameter: '',
          saves: {},
          seed: 1,
        },
        ...Array.from({ length: 416 }, (_, frame) => ({
          type: 'frame',
          input: inputs.get(frame) ?? emptyInputFrame(),
        })),
        { type: 'stop' },
      ];
      child.stdin.end(commands.map((value) => JSON.stringify(value)).join('\n') + '\n');
      const nativeCommits: Record<string, number>[] = [];
      let replies = 0,
        frames = 0,
        nativeWorkPeak = 0;
      try {
        for await (const line of createInterface({ input: child.stdout, crlfDelay: Infinity })) {
          expect(line.length).toBeLessThanOrEqual(16 * 1024 * 1024);
          const result = JSON.parse(line) as {
            type: string;
            frame?: number;
            pixels: string;
            left: string;
            right: string;
            workUnits: number;
            saveCommit: string | null;
          };
          if (replies === 0) expect(result.type).toBe('loaded');
          else if (frames < 416) {
            expect(result.type, line.slice(0, 500)).toBe('frame');
            expect(result.frame).toBe(frames);
            nativeWorkPeak = Math.max(nativeWorkPeak, result.workUnits);
            const expected = pixels.get(frames);
            if (expected)
              expect(
                createHash('sha256').update(Buffer.from(result.pixels, 'base64')).digest('hex'),
              ).toBe(expected);
            if (pcm.has(frames))
              expect(
                createHash('sha256')
                  .update(Buffer.from(result.left, 'base64'))
                  .update(Buffer.from(result.right, 'base64'))
                  .digest('hex'),
              ).toBe(pcm.get(frames));
            if (result.saveCommit !== null)
              nativeCommits.push(decodeSaveValues(Buffer.from(result.saveCommit, 'base64')));
            frames++;
          } else expect(result.type).toBe('stopped');
          replies++;
        }
        expect(await exited, stderr).toBe(0);
      } finally {
        child.kill();
      }
      expect(replies).toBe(418);
      expect(frames).toBe(416);
      expect(nativeWorkPeak).toBe(workPeak);
      expect(nativeCommits).toEqual([{ best: final.best }]);
    }, 240_000);
});
