import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { DEFAULT_SEED } from './hardware';
import { emptyInputFrame } from './input';
import type { HeadlessResult } from './headless';

describe('native/CLI seeded execution parity', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  const cli = join(root, 'target/debug/mod01');
  const host = join(root, 'packages/runtime/standalone/native-host.mjs');
  let directory: string;
  let cartridge: string;
  let rom: string;
  beforeAll(() => {
    directory = mkdtempSync(join(tmpdir(), 'mod01-default-seed-'));
    execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
    cartridge = join(directory, 'seed.m01w');
    rom = join(directory, 'seed.m01c');
    const source = readFileSync(join(root, 'tests/conformance/rng.modl'), 'utf8');
    writeFileSync(
      cartridge,
      `format = 2
[cartridge]
language = "MODL/1"
id = "seed-parity"
title = "SEED PARITY"
author = "@test"
version = "1.0"
entry = "src/main.modl"
update_rate = 60
[[module]]
path = "src/main.modl"
source = '''
${source}'''
`,
    );
    execFileSync(cli, ['pack', cartridge, '--output', rom]);
  }, 120_000);
  afterAll(() => {
    if (directory) rmSync(directory, { recursive: true, force: true });
  });
  for (const debug of [false, true]) {
    it(`matches seeded frames in ${debug ? 'debug' : 'release'}, including host defaults`, () => {
      const output = join(directory, `seed-${String(debug)}.mjs`);
      execFileSync(cli, ['build', cartridge, '--output', output, ...(debug ? ['--debug'] : [])]);
      const javascript = readFileSync(output, 'utf8');
      for (const seed of [undefined, 1]) {
        const tracePath = join(directory, `trace-${String(debug)}-${String(seed)}.json`);
        execFileSync(cli, [
          'run',
          cartridge,
          '--headless',
          '--frames',
          '2',
          '--output',
          tracePath,
          ...(seed === undefined ? [] : ['--seed', String(seed)]),
        ]);
        const trace = JSON.parse(readFileSync(tracePath, 'utf8')) as HeadlessResult;
        expect(trace.configuration.seed).toBe(seed ?? DEFAULT_SEED);
        const commands = [
          {
            type: 'load',
            javascript,
            debug,
            manifest: {
              id: 'seed-parity',
              hardwareRevision: 2,
              updateRate: 60,
              display: null,
              assets: {},
            },
            files: {},
            rom: readFileSync(rom).toString('base64'),
            save: '',
            ...(seed === undefined ? {} : { seed }),
          },
          { type: 'frame', input: emptyInputFrame() },
          { type: 'frame', input: emptyInputFrame() },
          { type: 'stop' },
        ];
        const replies = execFileSync(
          process.execPath,
          ['--permission', `--allow-fs-read=${host}`, host],
          { input: commands.map((command) => JSON.stringify(command)).join('\n') + '\n' },
        )
          .toString()
          .trim()
          .split('\n')
          .map((line) => JSON.parse(line) as Record<string, unknown>);
        expect(replies.map((reply) => reply.type)).toEqual(['loaded', 'frame', 'frame', 'stopped']);
        for (let frame = 0; frame < 2; frame++) {
          const reply = replies[frame + 1];
          expect(
            createHash('sha256')
              .update(Buffer.from(reply?.pixels as string, 'base64'))
              .digest('hex'),
          ).toBe(trace.frames[frame]?.framebufferSha256);
          expect(reply?.workUnits).toBe(trace.frames[frame]?.workUnits);
        }
      }
    });
  }
});
