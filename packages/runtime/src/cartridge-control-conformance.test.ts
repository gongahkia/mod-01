import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { CartridgeControl } from './cartridge-control';
import {
  createConsoleRuntime,
  isConsoleRuntimeSnapshot,
  type ConsoleFrame,
} from './console-runtime';
import { emptyInputFrame } from './input';
import type { CartridgeFactory } from './machine';

describe('actual typed MODL cartridge requests in the production console', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  let temporary: string;
  beforeAll(() => {
    temporary = mkdtempSync(join(tmpdir(), 'mod01-cartridge-control-'));
    execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
  }, 120_000);
  afterAll(() => {
    if (temporary) rmSync(temporary, { recursive: true, force: true });
  });

  for (const debug of [false, true]) {
    it(`queues and restores a menu-driven request in real ${debug ? 'debug' : 'release'} code`, async () => {
      const output = join(temporary, `control-${String(debug)}.mjs`);
      execFileSync(join(root, 'target/debug/mod01'), [
        'build',
        join(root, 'tests/conformance/cartridge-control.modl'),
        '--output',
        output,
        ...(debug ? ['--debug'] : []),
      ]);
      const generated = (await import(/* @vite-ignore */ pathToFileURL(output).href)) as {
        default: CartridgeFactory;
      };
      const control = new CartridgeControl({
        alias: 'main',
        parameter: 'lives=3',
        aliases: ['main', 'arena'],
        reloadCosts: { main: 20, arena: 20 },
      });
      const configuration = { seed: 7, workUnitsPerFrame: 50_000, updateRate: 60 as const, debug };
      const runtime = createConsoleRuntime(generated.default, configuration, {
        cartridge: control,
      });
      if (debug) {
        for (let step = 0; ; step++) {
          expect(step).toBeLessThan(100);
          if ('booted' in runtime.stepDebug(emptyInputFrame())) break;
        }
      }
      const initial = runtime.snapshot();
      expect(initial.cartridgeControl?.pending).toBeNull();
      const input = { ...emptyInputFrame(), menuAction: 1 };
      let frame: ConsoleFrame | undefined;
      let sawPendingInsideStep = false;
      if (debug) {
        for (let step = 0; ; step++) {
          expect(step).toBeLessThan(100);
          const result = runtime.stepDebug(step === 0 ? input : emptyInputFrame());
          if ('frame' in result) {
            frame = result.frame;
            break;
          }
          if (control.pendingRequest() !== null) {
            sawPendingInsideStep = true;
            expect(() => runtime.snapshot()).toThrow(/completed/);
          }
        }
      } else frame = runtime.runFrame(input);
      expect(frame.output.indexedPixels[0]).toBe(1);
      expect(sawPendingInsideStep).toBe(debug);
      const completed = runtime.snapshot();
      expect(completed.machine.frame).toBe(1);
      expect(completed.cartridgeControl).toMatchObject({
        alias: 'main',
        parameter: 'lives=3',
        pending: { kind: 'load', alias: 'arena', parameter: 'lives=2' },
      });
      expect(isConsoleRuntimeSnapshot(completed)).toBe(true);
      expect(isConsoleRuntimeSnapshot({ ...completed, revision: 6 })).toBe(false);
      runtime.restore(initial);
      expect(control.pendingRequest()).toBeNull();
      expect(runtime.runFrame(input)).toEqual(frame);
      const before = runtime.snapshot();
      const mismatch = {
        ...before,
        cartridgeControl: { ...before.cartridgeControl, alias: 'arena' },
      };
      expect(() => {
        runtime.restore(mismatch);
      }).toThrow(/snapshot/);
      expect(runtime.snapshot()).toEqual(before);
      const invalid = structuredClone(initial);
      const ram = invalid.memory.regions[0];
      if (ram === undefined) throw new Error('fixture RAM missing');
      Object.assign(ram, { bytes: new Uint8Array(1) });
      expect(() => {
        runtime.restore(invalid);
      }).toThrow(/snapshot/);
      expect(runtime.snapshot()).toEqual(before);
    });
  }

  it('defers release boot for a prepared transition without executing start twice on restore', async () => {
    const output = join(temporary, 'deferred.mjs');
    execFileSync(join(root, 'target/debug/mod01'), [
      'build',
      join(root, 'tests/conformance/cartridge-control.modl'),
      '--output',
      output,
    ]);
    const generated = (await import(/* @vite-ignore */ pathToFileURL(output).href)) as {
      default: CartridgeFactory;
    };
    const environment = {
      alias: 'main',
      parameter: 'lives=3',
      aliases: ['main', 'arena'],
      reloadCosts: {},
    };
    const configuration = { seed: 7, workUnitsPerFrame: 50_000, updateRate: 60 as const };
    const source = createConsoleRuntime(generated.default, configuration, {
      cartridge: new CartridgeControl(environment),
    });
    const expected = source.runFrame(emptyInputFrame());
    const saved = source.snapshot();
    const deferred = createConsoleRuntime(generated.default, configuration, {
      cartridge: new CartridgeControl(environment),
      deferBoot: true,
    });
    expect(deferred.snapshot().machine.execution.booted).toBe(false);
    expect(deferred.inspectMenu()).toEqual([]);
    expect(deferred.runFrame(emptyInputFrame())).toEqual(expected);
    const restored = createConsoleRuntime(generated.default, configuration, {
      cartridge: new CartridgeControl(environment),
      deferBoot: true,
    });
    restored.restore(saved);
    expect(restored.snapshot()).toEqual(saved);
    expect(restored.runFrame(emptyInputFrame()).output.indexedPixels[0]).toBe(2);
    const withoutEnvironment = createConsoleRuntime(generated.default, configuration, {
      deferBoot: true,
    });
    expect(() => {
      withoutEnvironment.restore(saved);
    }).toThrow(/environment/);
    expect(() => {
      withoutEnvironment.runFrame(emptyInputFrame());
    }).toThrow(expect.objectContaining({ code: 'M019017' }));
  });
});
