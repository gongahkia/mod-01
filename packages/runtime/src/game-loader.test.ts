import { deepStrictEqual } from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { encodeAssetFile } from './asset-codec';
import { decodeRuntimeBytes, loadRuntimeGame, type RuntimeGamePayload } from './game-loader';
import { emptyInputFrame } from './input';

describe('shared Rust-to-host current game handoff', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  const payloads = new Map<boolean, RuntimeGamePayload>();
  let temporary: string;
  beforeAll(() => {
    temporary = mkdtempSync(join(tmpdir(), 'mod01-game-loader-'));
    execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
    for (const debug of [false, true]) {
      const members: Record<string, RuntimeGamePayload['members'][string]> = {};
      for (const alias of ['main', 'arena']) {
        const base = join(temporary, `${alias}-${String(debug)}`);
        execFileSync(join(root, 'target/debug/mod01'), [
          'build',
          join(root, `tests/conformance/game-${alias}.modl`),
          '--output',
          `${base}.mjs`,
          ...(debug ? ['--debug'] : []),
        ]);
        members[alias] = {
          javascript: readFileSync(`${base}.mjs`, 'utf8'),
          sourceMap: readFileSync(`${base}.js.map`, 'utf8'),
          symbolNames: { s1: `${alias}_state` },
          manifest: {
            id: `loader-${alias}`,
            hardwareRevision: 2,
            updateRate: alias === 'main' ? 60 : 30,
            display: null,
            assets: {},
          },
          files: {},
          rom: 'AQ==',
        };
      }
      payloads.set(debug, { revision: 1, identity: 'a'.repeat(64), debug, members });
    }
  }, 120_000);
  afterAll(() => {
    if (temporary) rmSync(temporary, { recursive: true, force: true });
  });
  function payload(debug = false): RuntimeGamePayload {
    const value = payloads.get(debug);
    if (value === undefined) throw new Error('missing compiled payload');
    return structuredClone(value);
  }

  for (const debug of [false, true])
    it(`loads a complete ${debug ? 'debug' : 'release'} catalog with per-alias metadata`, async () => {
      const source = payload(debug);
      const loaded = await loadRuntimeGame(source, {
        seed: 19,
        parameter: 'root',
        saves: { 'loader-arena': { visits: 4 } },
      });
      expect(loaded.metadata.size).toBe(2);
      expect(loaded.metadata.get('main')?.symbolNames).toEqual({ s1: 'main_state' });
      expect(loaded.metadata.get('arena')?.symbolNames).toEqual({ s1: 'arena_state' });
      expect(loaded.metadata.get('main')?.sourceMap).not.toBe(
        loaded.metadata.get('arena')?.sourceMap,
      );
      const game = loaded.runtime;
      game.runFrame(emptyInputFrame());
      expect(game.runFrame({ ...emptyInputFrame(), menuAction: 1 }).cartridgeAlias).toBe('main');
      expect(game.inspect()).toMatchObject({ cartridgeAlias: 'arena', booted: false });
      const saved = game.snapshot();
      const arena = game.runFrame(emptyInputFrame());
      expect(arena).toMatchObject({
        frame: 2,
        cartridgeFrame: 0,
        cartridgeAlias: 'arena',
        cartridgeId: 'loader-arena',
      });
      expect(arena.output.indexedPixels[0]).toBe(7);
      game.restore(saved);
      deepStrictEqual(game.runFrame(emptyInputFrame()), arena);
      expect(loaded.metadata.get('arena')?.manifest.updateRate).toBe(30);
    });

  it('validates every unused asset before importing even the first compiled module', async () => {
    const source = payload(),
      main = source.members.main,
      arena = source.members.arena;
    if (main === undefined || arena === undefined) throw new Error('fixture member missing');
    const invalid = {
      ...source,
      members: {
        main: {
          ...main,
          javascript: 'throw new Error("IMPORTED BEFORE ADMISSION"); export default () => null;',
        },
        arena: {
          ...arena,
          manifest: { ...arena.manifest, assets: { hero: { kind: 'sprite', path: 'hero.m01g' } } },
          files: { 'hero.m01g': Buffer.from('{}').toString('base64') },
        },
      },
    };
    await expect(loadRuntimeGame(invalid, { seed: 0 })).rejects.toThrow(/sprite/);
    await expect(
      loadRuntimeGame(
        {
          ...invalid,
          members: {
            ...invalid.members,
            arena: { ...arena, manifest: { ...arena.manifest, id: main.manifest.id } },
          },
        },
        { seed: 0 },
      ),
    ).rejects.toThrow(/identity/);
  });

  it('does not boot main until every module provides a valid factory', async () => {
    const source = payload(),
      main = source.members.main,
      arena = source.members.arena;
    if (main === undefined || arena === undefined) throw new Error('fixture member missing');
    const invalid = {
      ...source,
      members: {
        main: {
          ...main,
          javascript: 'export default () => { throw new Error("BOOT BEFORE ALL FACTORIES"); };',
        },
        arena: { ...arena, javascript: 'export default 42;' },
      },
    };
    await expect(loadRuntimeGame(invalid, { seed: 0 })).rejects.toThrow(/arena.*factory/);
  });

  it('preflights aggregate file bytes before decoding a collection of individually bounded files', async () => {
    const source = payload(),
      main = source.members.main;
    if (main === undefined) throw new Error('fixture member missing');
    const oversized = {
      ...source,
      members: {
        ...source.members,
        main: {
          ...main,
          files: {
            'a.bin': 'AAAA'.repeat(3 * 1024 * 1024),
            'b.bin': 'AAAA'.repeat(3 * 1024 * 1024),
          },
        },
      },
    };
    await expect(loadRuntimeGame(oversized, { seed: 19, parameter: 'root' })).rejects.toThrow(
      /asset byte transport exceeds/,
    );
  });

  it('rejects old/unknown shapes, member context forgery and malformed byte/map transport', async () => {
    const source = payload(),
      main = source.members.main;
    if (main === undefined) throw new Error('fixture member missing');
    const cases: unknown[] = [
      null,
      { ...source, revision: 0 },
      { ...source, revision: 2 },
      { ...source, extra: true },
      { ...source, identity: 'not a hash' },
      { ...source, debug: 1 },
      { ...source, members: {} },
      { ...source, members: { arena: main } },
      { ...source, members: { ...source.members, '../remote': main } },
      ...[
        { ...main, extra: true },
        { ...main, javascript: '' },
        { ...main, javascript: '\ud800' },
        { ...main, javascript: ' '.repeat(2 * 1024 * 1024 + 1) },
        { ...main, sourceMap: '{' },
        { ...main, sourceMap: '{}' },
        { ...main, symbolNames: { bad: 'state' } },
        { ...main, symbolNames: { s1: null } },
        { ...main, rom: '' },
        { ...main, rom: 'AQ==\n' },
        { ...main, rom: 'AR==' },
        { ...main, manifest: { ...main.manifest, hardwareRevision: 1 } },
        { ...main, manifest: { ...main.manifest, updateRate: 45 } },
        {
          ...main,
          manifest: { ...main.manifest, assets: { bad: { kind: 'sprite', path: '../escape' } } },
        },
      ].map((main) => ({ ...source, members: { ...source.members, main } })),
    ];
    for (const invalid of cases)
      await expect(loadRuntimeGame(invalid, { seed: 19, parameter: 'root' })).rejects.toThrow();
  });

  it('preserves prototype-named ordinary file keys and owns a defensive metadata copy', async () => {
    const source = payload(),
      main = source.members.main;
    if (main === undefined) throw new Error('fixture member missing');
    const sprite = { kind: 'sprite' as const, path: '__proto__' };
    const assets = { hero: sprite };
    const value = {
      ...source,
      members: {
        ...source.members,
        main: {
          ...main,
          manifest: { ...main.manifest, assets },
          files: Object.fromEntries([
            [
              '__proto__',
              Buffer.from(
                encodeAssetFile({
                  revision: 2,
                  kind: 'sprite',
                  width: 1,
                  height: 1,
                  frames: [[7]],
                }),
              ).toString('base64'),
            ],
          ]),
        },
      },
    };
    const loaded = await loadRuntimeGame(value, { seed: 19, parameter: 'root' });
    sprite.path = 'mutated';
    expect(loaded.metadata.get('main')?.manifest.assets.hero?.path).toBe('__proto__');
    expect(Object.hasOwn(loaded.metadata.get('main')?.files ?? {}, '__proto__')).toBe(true);
    loaded.runtime.runFrame(emptyInputFrame());
  });
});

describe('canonical bounded byte transport', () => {
  it('enforces exact inclusive decoded bounds without repairing base64 input', () => {
    expect(decodeRuntimeBytes('', 0)).toEqual(new Uint8Array());
    expect(decodeRuntimeBytes('AQI=', 2)).toEqual(Uint8Array.of(1, 2));
    for (const value of ['AQI=', 'AQ==\n', 'AQ', 'AR==', '@@==', 'AQ===', 1, null])
      expect(() => decodeRuntimeBytes(value, 1)).toThrow();
    for (const limit of [-1, Infinity, 0.5]) expect(() => decodeRuntimeBytes('', limit)).toThrow();
  });
});
