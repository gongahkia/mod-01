/* global process */
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { assertCompletedHeadlessTrace } from './check-headless-trace.mjs';

function completed() {
  return {
    revision: 2,
    cartridge: { id: 'smoke-test' },
    configuration: { requestedFrames: 5 },
    summary: { completedFrames: 5 },
    frames: Array.from({ length: 5 }, (_, frame) => ({ frame })),
  };
}

describe('release headless smoke admission', () => {
  it('requires complete ordered current results without a fault', () => {
    const good = completed();
    expect(() => assertCompletedHeadlessTrace(good, 5, 'smoke-test')).not.toThrow();
    for (const bad of [
      null,
      [],
      {},
      { ...good, revision: 1 },
      { ...good, fault: null },
      { ...good, fault: { code: 'M019001', frame: 0 } },
      { ...good, summary: { completedFrames: 4 } },
      { ...good, configuration: { requestedFrames: 4 } },
      { ...good, frames: good.frames.slice(1) },
      { ...good, frames: [...good.frames].reverse() },
    ])
      expect(() => assertCompletedHeadlessTrace(bad, 5, 'smoke-test')).toThrow();
    expect(() => assertCompletedHeadlessTrace(good, 5, 'another-cart')).toThrow();
    for (const frames of [0, -1, 1.5, 36_001, Infinity])
      expect(() => assertCompletedHeadlessTrace(good, frames, 'smoke-test')).toThrow();
  });

  it('rejects an actual fault trace despite the CLI returning success', () => {
    const root = resolve(import.meta.dirname, '..');
    const temporary = mkdtempSync(join(tmpdir(), 'mod01-smoke-fault-'));
    const suffix = process.platform === 'win32' ? '.exe' : '';
    try {
      // Build current source instead of trusting a previously generated CLI/host.
      execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
      const source = readFileSync(join(root, 'templates/blank/cart.m01w'), 'utf8');
      const start = source.indexOf('source = """');
      const end = source.indexOf('"""', start + 12);
      expect(start).toBeGreaterThan(0);
      expect(end).toBeGreaterThan(start);
      const path = join(temporary, 'fault.m01w');
      writeFileSync(
        path,
        source.slice(0, start + 12) +
          '\non draw:\n  assert false, "smoke fault"\n' +
          source.slice(end),
      );
      const result = spawnSync(
        join(root, 'target/debug', `mod01${suffix}`),
        ['run', path, '--headless', '--frames', '5'],
        { cwd: root, encoding: 'utf8' },
      );
      expect(result.status, result.stderr).toBe(0);
      const trace = JSON.parse(result.stdout);
      expect(trace.fault).toBeDefined();
      expect(trace.summary.completedFrames).toBe(0);
      expect(() => assertCompletedHeadlessTrace(trace, 5, trace.cartridge.id)).toThrow(/fault/);
      const output = join(temporary, 'fault.json');
      writeFileSync(output, result.stdout);
      const check = spawnSync(
        process.execPath,
        [join(root, 'scripts/check-headless-trace.mjs'), output, '5', trace.cartridge.id],
        { encoding: 'utf8' },
      );
      expect(check.status).toBe(1);
      expect(check.stderr).toContain('reported a cartridge fault');
    } finally {
      rmSync(temporary, { recursive: true, force: true });
    }
  }, 120_000);
});
