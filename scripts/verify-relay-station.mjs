/* global process, console */
import { execFileSync } from 'node:child_process';
import { deepStrictEqual, strictEqual, ok } from 'node:assert';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const suffix = process.platform === 'win32' ? '.exe' : '';
const player = resolve(process.argv[2] ?? join(root, 'target/debug', `mod01-studio${suffix}`));
const cli = resolve(process.argv[3] ?? join(dirname(player), `mod01${suffix}`));
const source = join(root, 'cartridges/relay-station');
const input = join(root, 'tests/replays/relay-station.json');
const temporary = mkdtempSync(join(tmpdir(), 'mod01-relay-station-'));
const protectedPaths = [join(source, 'cart.m01w'), input];
const fingerprint = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
const before = protectedPaths.map(fingerprint);

function run(executable, args, options = {}) {
  return execFileSync(executable, args, {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
    timeout: 120_000,
    ...options,
  });
}

function headless(path) {
  const trace = JSON.parse(
    run(cli, ['run', path, '--headless', '--frames', '110', '--input', input]),
  );
  strictEqual(trace.fault, undefined, 'headless game faulted');
  strictEqual(trace.summary.completedFrames, 110);
  strictEqual(trace.frames.length, 110);
  return trace;
}

function native(path, executable = player, options = {}) {
  return JSON.parse(
    run(executable, ['--verify', path, '--frames', '110', '--input', input], options),
  );
}

function compareHosts(headless, native) {
  strictEqual(native.frames, 110);
  strictEqual(native.gameIdentity, headless.game.identity);
  strictEqual(native.workPeak, headless.summary.workPeak);
  deepStrictEqual(native.memberFrames, { main: 6, vault: 104 });
  deepStrictEqual(native.saveCommitFrames, { main: 2, vault: 2 });
  ok(native.audioPeak > 0, 'route must exercise production PCM');
  ok(native.workPeak >= 21_000 && native.workPeak <= 50_000, 'bank copies must be charged');
  deepStrictEqual(
    native.frameChecks,
    headless.frames.map((frame) =>
      Object.fromEntries(
        [
          'frame',
          'cartridgeFrame',
          'cartridgeAlias',
          'cartridgeId',
          'workUnits',
          'framebufferSha256',
          'pcmSha256',
          'audioCommandSha256',
        ].map((key) => [key, frame[key]]),
      ),
    ),
    'every produced display frame, PCM frame and command list must match across hosts',
  );
  for (const key of ['finalStateSha256', 'finalSnapshotSha256', 'finalSaveSha256', 'finalSaves'])
    deepStrictEqual(native[key], headless.summary[key], key);
  strictEqual(
    native.finalSaves['relay-station'],
    '36bc4828a5766c7cca119e57ad5a38a84d1b9cbb8a139fe7700eb237491316ac',
  );
  strictEqual(
    native.finalSaves['relay-station-vault'],
    '6d4b9e07f8e505b8b6cd82d9ae9e952ec466e5888e1129aaf9438bddf5ad58d3',
  );
  strictEqual(
    native.finalSaves['relay-station-night'],
    headless.configuration.initialSaves['relay-station-night'],
  );
}

try {
  const raw = join(temporary, 'relay.m01c');
  const png = join(temporary, 'relay.m01c.png');
  run(cli, ['pack', source, '--output', raw]);
  run(cli, ['export', 'png', source, '--output', png]);
  const expectedHeadless = headless(source);
  const expectedNative = native(source);
  compareHosts(expectedHeadless, expectedNative);
  for (const path of [raw, png]) {
    const actualHeadless = headless(path);
    const actualNative = native(path);
    deepStrictEqual(actualHeadless, expectedHeadless);
    deepStrictEqual(actualNative, expectedNative);
    compareHosts(actualHeadless, actualNative);
  }
  const output = join(temporary, 'package');
  run(cli, ['export', 'native', source, '--output', output, '--player', player]);
  const metadata = JSON.parse(readFileSync(join(output, 'mod01-game.json'), 'utf8'));
  const executable = join(output, metadata.executable);
  const game = join(dirname(executable), 'game.m01c');
  deepStrictEqual(readFileSync(game), readFileSync(raw));
  strictEqual(metadata.gameIdentity, expectedNative.gameIdentity);
  deepStrictEqual(Object.keys(metadata.members).sort(), ['main', 'night', 'vault']);
  const packaged = native(game, executable, {
    cwd: temporary,
    env: { PATH: '', ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}) },
  });
  deepStrictEqual(packaged, expectedNative);
  compareHosts(expectedHeadless, packaged);
  deepStrictEqual(protectedPaths.map(fingerprint), before, 'QA cannot modify the game or replay');
  console.log(
    `Verified ${process.platform}/${process.arch} Relay Station: 110 per-frame CLI/native comparisons from source/raw/PNG and the actual standalone package; matching pixels, interleaved PCM, audio commands, work, routing, final state/checkpoint and separate saves; empty PATH.`,
  );
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
