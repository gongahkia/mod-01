/* global process, console */
import { execFileSync } from 'node:child_process';
import { deepStrictEqual } from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const suffix = process.platform === 'win32' ? '.exe' : '';
const player = resolve(process.argv[2] ?? join(root, 'target/debug', `mod01-studio${suffix}`));
const cli = resolve(process.argv[3] ?? join(dirname(player), `mod01${suffix}`));
const cartridge = resolve(process.argv[4] ?? join(root, 'cartridges/pocket-relay'));
const temporary = mkdtempSync(join(tmpdir(), 'mod01-native-game-'));
const output = join(temporary, 'game');
try {
  const expected = JSON.parse(
    execFileSync(player, ['--verify', cartridge, '--frames', '120'], { encoding: 'utf8' }),
  );
  execFileSync(cli, ['export', 'native', cartridge, '--output', output, '--player', player], {
    stdio: 'pipe',
  });
  const metadata = JSON.parse(readFileSync(join(output, 'mod01-game.json'), 'utf8'));
  const executable = join(output, metadata.executable);
  const binaries = dirname(executable);
  const rom = join(binaries, 'game.m01c');
  if (
    !existsSync(join(binaries, `node${suffix}`)) ||
    !existsSync(join(output, 'licenses/Node.txt'))
  )
    throw new Error('game package is missing its runtime or notices');
  const trace = JSON.parse(
    execFileSync(executable, ['--verify', rom, '--frames', '120'], {
      cwd: temporary,
      encoding: 'utf8',
      env: { PATH: '', ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}) },
    }),
  );
  deepStrictEqual(trace, expected);
  const packed = join(temporary, 'expected.m01c');
  execFileSync(cli, ['pack', cartridge, '--output', packed]);
  deepStrictEqual(readFileSync(rom), readFileSync(packed));
  console.log(
    `Verified ${metadata.platform}/${metadata.architecture} game: 120 native frames, canonical cart, bundled Node, no PATH dependency.`,
  );
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
