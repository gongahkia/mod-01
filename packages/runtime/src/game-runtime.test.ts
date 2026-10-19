import { deepStrictEqual } from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { encodeAssetFile, type RuntimeAssetSource } from './asset-codec';
import { MEMORY } from './bus';
import type { GameDefinition } from './game-definition';
import {
  createGameRuntime,
  type GameFrame,
  type GameRuntime,
  type GameSnapshot,
} from './game-runtime';
import { emptyInputFrame, type ControllerState, type InputFrame } from './input';
import type { CartridgeFactory } from './machine';
import { decodeSaveValues } from './save';
import { decodeReplayTrace, encodeReplayTrace, replayInputFrames } from './replay';

const configuration = { workUnitsPerFrame: 50_000, updateRate: 60 as const };
const span = { start: 0, end: 0 };
const blank: CartridgeFactory = () => ({
  start() {},
  update() {},
  draw() {},
  raster() {},
  snapshot: () => ({ state: {}, tasks: [], nextTaskId: 1 }),
  restore() {},
  inspect: () => ({ state: {}, tasks: [], callStack: [] }),
});

function controllers(
  input: InputFrame,
  edit: (value: ControllerState, port: number) => ControllerState,
): InputFrame['controllers'] {
  return [
    edit(input.controllers[0], 0),
    edit(input.controllers[1], 1),
    edit(input.controllers[2], 2),
    edit(input.controllers[3], 3),
  ];
}

function held(menuAction?: number): InputFrame {
  const input = emptyInputFrame();
  return {
    controllers: controllers(input, (controller) => ({
      buttons: { ...controller.buttons, a: true },
    })),
    pointer: { x: 31, y: 47, primary: true, secondary: true, inside: true },
    ...(menuAction === undefined ? {} : { menuAction }),
  };
}

function boot(game: GameRuntime): void {
  for (let step = 0; !game.inspect().booted; step++) {
    expect(step).toBeLessThan(250);
    expect('frame' in game.stepDebug(emptyInputFrame())).toBe(false);
  }
}
function frame(game: GameRuntime, input: InputFrame, debug: boolean): GameFrame {
  if (!debug) return game.runFrame(input);
  boot(game);
  const location = game.inspect().cartridgeAlias;
  const time = game.inspect().frame;
  for (let step = 0; ; step++) {
    expect(step).toBeLessThan(250);
    // Later messages cannot replace the raw input owned by the first statement.
    const result = game.stepDebug(step === 0 ? input : emptyInputFrame());
    if ('frame' in result) return result.frame;
    expect(result.cartridgeAlias).toBe(location);
    expect(game.inspect().frame).toBe(time);
    expect(game.inspect().cartridgeAlias).toBe(location);
    expect(() => game.snapshot()).toThrow(/completed/);
  }
}
function bank(color: number, remap: number): RuntimeAssetSource {
  return {
    declarations: { hero: { kind: 'sprite', path: 'hero.m01g' } },
    files: {
      'hero.m01g': encodeAssetFile({
        revision: 2,
        kind: 'sprite',
        width: 1,
        height: 1,
        frames: [[color]],
      }),
      'display.m01p': encodeAssetFile({
        revision: 2,
        kind: 'display',
        remap: Array.from({ length: 32 }, (_, value) =>
          value === 1 || value === 7 ? remap + value : value,
        ),
        raster: [],
      }),
    },
    displayPath: 'display.m01p',
  };
}

describe('whole-game scheduler with actual MODL release/debug factories', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  let temporary: string;
  const factories = new Map<string, CartridgeFactory>();
  beforeAll(async () => {
    temporary = mkdtempSync(join(tmpdir(), 'mod01-game-runtime-'));
    execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
    for (const debug of [false, true])
      for (const name of ['main', 'arena', 'reload']) {
        const output = join(temporary, `${name}-${String(debug)}.mjs`);
        execFileSync(join(root, 'target/debug/mod01'), [
          'build',
          join(root, `tests/conformance/game-${name}.modl`),
          '--output',
          output,
          ...(debug ? ['--debug'] : []),
        ]);
        const generated = (await import(/* @vite-ignore */ pathToFileURL(output).href)) as {
          default: CartridgeFactory;
        };
        factories.set(`${name}-${String(debug)}`, generated.default);
      }
  }, 120_000);
  afterAll(() => {
    if (temporary) rmSync(temporary, { recursive: true, force: true });
  });
  function factory(name: string, debug: boolean): CartridgeFactory {
    const result = factories.get(`${name}-${String(debug)}`);
    if (result === undefined) throw new Error('compiled fixture missing');
    return result;
  }
  function definition(debug: boolean): GameDefinition {
    return {
      identity: 'a'.repeat(64),
      seed: 19,
      debug,
      parameter: 'root',
      members: [
        {
          alias: 'main',
          id: 'game-main',
          factory: factory('main', debug),
          configuration: { ...configuration, rom: Uint8Array.of(1) },
        },
        {
          alias: 'arena',
          id: 'game-arena',
          factory: factory('arena', debug),
          configuration: { ...configuration, updateRate: 30, rom: Uint8Array.of(7) },
        },
      ],
    };
  }
  function reloadDefinition(debug: boolean): GameDefinition {
    return {
      identity: 'b'.repeat(64),
      seed: 29,
      debug,
      parameter: 'reload',
      members: [
        {
          alias: 'main',
          id: 'game-main',
          factory: factory('reload', debug),
          configuration: { ...configuration, rom: Uint8Array.of(11), assets: bank(1, 11) },
        },
        {
          alias: 'donor',
          id: 'game-bank',
          factory: blank,
          configuration: {
            ...configuration,
            updateRate: 30,
            workUnitsPerFrame: 1000,
            rom: Uint8Array.of(77),
            assets: bank(7, 22),
          },
        },
      ],
    };
  }

  for (const debug of [false, true]) {
    const mode = debug ? 'debug' : 'release';
    it(`isolates state/save identities and applies loads after the old ${mode} frame`, () => {
      const game = createGameRuntime(definition(debug));
      const first = frame(game, emptyInputFrame(), debug);
      const original = game.snapshot();
      expect(first).toMatchObject({
        frame: 0,
        cartridgeFrame: 0,
        cartridgeAlias: 'main',
        cartridgeId: 'game-main',
      });
      expect(decodeSaveValues(first.saveCommit ?? new Uint8Array())).toEqual({ visits: 1 });
      const random = original.console.memory.regions[0]?.bytes[0];
      const loading = frame(game, { ...emptyInputFrame(), menuAction: 1 }, debug);
      expect(loading).toMatchObject({ frame: 1, cartridgeFrame: 1, cartridgeAlias: 'main' });
      expect(loading.output.indexedPixels[0]).toBe(2);
      expect(game.inspect()).toMatchObject({
        frame: 2,
        cartridgeFrame: 0,
        cartridgeAlias: 'arena',
        parameter: 'score=7',
        booted: false,
      });
      expect(game.inspectMenu()).toEqual([]);
      const transition = game.snapshot();
      expect(transition.console.memory.regions[0]?.bytes.every((byte) => byte === 0)).toBe(true);
      const arena = frame(game, emptyInputFrame(), debug);
      expect(arena).toMatchObject({
        frame: 2,
        cartridgeFrame: 0,
        cartridgeAlias: 'arena',
        cartridgeId: 'game-arena',
      });
      expect(arena.output.indexedPixels[0]).toBe(7);
      expect(decodeSaveValues(arena.saveCommit ?? new Uint8Array())).toEqual({ visits: 1 });
      const returning = frame(game, { ...emptyInputFrame(), menuAction: 1 }, debug);
      expect(returning).toMatchObject({ frame: 3, cartridgeFrame: 1, cartridgeAlias: 'arena' });
      const restarted = frame(game, emptyInputFrame(), debug);
      expect(restarted).toMatchObject({ frame: 4, cartridgeFrame: 0, cartridgeAlias: 'main' });
      expect(restarted.output.indexedPixels[0]).toBe(1);
      expect(decodeSaveValues(restarted.saveCommit ?? new Uint8Array())).toEqual({ visits: 2 });
      expect(game.snapshot().console.memory.regions[0]?.bytes[0]).toBe(random);
      expect(
        Object.fromEntries(
          Object.entries(game.snapshot().saves).map(([id, bytes]) => [id, decodeSaveValues(bytes)]),
        ),
      ).toEqual({ 'game-arena': { visits: 1 }, 'game-main': { visits: 2 } });
      game.restore(transition);
      deepStrictEqual(frame(game, emptyInputFrame(), debug), arena);
      deepStrictEqual(frame(game, { ...emptyInputFrame(), menuAction: 1 }, debug), returning);
      deepStrictEqual(frame(game, emptyInputFrame(), debug), restarted);
      game.restore(original);
      deepStrictEqual(frame(game, { ...emptyInputFrame(), menuAction: 1 }, debug), loading);
    });

    it(`keeps the ${mode} input owner through boot and checkpoint restoration`, () => {
      const game = createGameRuntime(definition(debug));
      frame(game, emptyInputFrame(), debug);
      frame(game, held(1), debug);
      const saved = game.snapshot();
      expect(saved.inputBarrier).toMatchObject({ controllers: [16, 16, 16, 16], pointer: 3 });
      const input = held();
      const fresh = {
        ...input,
        controllers: controllers(input, (controller, port) => ({
          buttons: { ...controller.buttons, x: port === 3 },
        })),
      };
      const blocked = frame(game, fresh, debug);
      expect(blocked.output.indexedPixels[0]).toBe(7);
      const sampled = game.snapshot().console.machine.input;
      expect(sampled.controllers.every((controller) => !controller.buttons.a)).toBe(true);
      expect(sampled.controllers[3].buttons.x).toBe(true);
      expect(sampled.pointer).toEqual({ ...input.pointer, primary: false, secondary: false });
      const released = frame(game, emptyInputFrame(), debug);
      const repressed = frame(game, held(), debug);
      expect(repressed.output.indexedPixels[0]).toBe(11);
      game.restore(saved);
      deepStrictEqual(frame(game, fresh, debug), blocked);
      deepStrictEqual(frame(game, emptyInputFrame(), debug), released);
      deepStrictEqual(frame(game, held(), debug), repressed);
    });

    it(`does not transfer or persist a faulted ${mode} frame`, () => {
      const game = createGameRuntime(definition(debug));
      frame(game, emptyInputFrame(), debug);
      const healthy = game.snapshot();
      expect(() => frame(game, { ...emptyInputFrame(), menuAction: 3 }, debug)).toThrow(
        /failed frames/,
      );
      expect(game.inspect()).toMatchObject({
        frame: 1,
        cartridgeAlias: 'main',
        pending: { kind: 'load', alias: 'arena', parameter: 'bad' },
      });
      const faulted = game.snapshot();
      expect(faulted.saves).toEqual(healthy.saves);
      expect(faulted.console.save.pendingCommit).toBe(true);
      game.restore(faulted);
      deepStrictEqual(game.snapshot(), faulted);
      expect(() => game.runFrame(emptyInputFrame())).toThrow(/faulted/);
      expect(game.inspect().cartridgeAlias).toBe('main');
      game.restore(healthy);
      frame(game, { ...emptyInputFrame(), menuAction: 1 }, debug);
      frame(game, emptyInputFrame(), debug);
      expect(decodeSaveValues(game.snapshot().saves['game-main'] ?? new Uint8Array())).toEqual({
        visits: 1,
      });
    });

    it(`reloads a compatible bank without restarting ${mode} code, RAM, RNG, inputs or saves`, () => {
      const game = createGameRuntime(reloadDefinition(debug));
      const baseline = createGameRuntime(reloadDefinition(debug));
      const pressed = { ...emptyInputFrame(), controllers: held().controllers };
      const first = frame(game, pressed, debug);
      frame(baseline, pressed, debug);
      expect(first.output.indexedPixels[0]).toBe(1); // raw DMA bypasses draw remapping
      expect(first.output.indexedPixels[1]).toBe(12);
      expect(first.output.indexedPixels[2]).toBe(9);
      const reload = frame(game, { ...pressed, menuAction: 1 }, debug);
      frame(baseline, pressed, debug);
      expect(reload.output.indexedPixels[0]).toBe(1);
      const saved = game.snapshot();
      expect(game.inspect()).toMatchObject({
        bankAlias: 'donor',
        cartridgeAlias: 'main',
        cartridgeFrame: 2,
        parameter: 'reload',
        booted: true,
        pending: null,
      });
      expect(saved.console.machine.rngState).toBe(baseline.snapshot().console.machine.rngState);
      expect(saved.console.machine.updateRate).toBe(60);
      expect(saved.console.machine.cartridge.tasks).not.toEqual([]);
      deepStrictEqual(
        saved.console.machine.cartridge.tasks,
        baseline.snapshot().console.machine.cartridge.tasks,
      );
      expect(saved.console.machine.input).toEqual({
        ...baseline.snapshot().console.machine.input,
        menuAction: 1,
      });
      deepStrictEqual(
        saved.console.memory.regions[0]?.bytes,
        baseline.snapshot().console.memory.regions[0]?.bytes,
      );
      deepStrictEqual(saved.console.graphics, baseline.snapshot().console.graphics);
      expect(saved.console.audio.frame).toBe(0);
      expect(decodeSaveValues(saved.console.save.committed)).toEqual({ visits: 1 });
      expect(game.inspectMenu()).toEqual(baseline.inspectMenu());
      expect(saved.inputBarrier.controllers).toEqual([0, 0, 0, 0]);
      const changed = frame(game, pressed, debug);
      frame(baseline, pressed, debug);
      deepStrictEqual(
        game.snapshot().console.machine.cartridge.tasks,
        baseline.snapshot().console.machine.cartridge.tasks,
      );
      deepStrictEqual(
        game.snapshot().console.memory.regions[0]?.bytes,
        baseline.snapshot().console.memory.regions[0]?.bytes,
      );
      expect(changed.output.indexedPixels[0]).toBe(7);
      expect(changed.output.indexedPixels[1]).toBe(12); // the active display, not the donor's 23
      expect(changed.output.indexedPixels[2]).toBe(12); // held A did not become a fresh press
      expect(changed.saveCommit).toBeUndefined();
      game.restore(saved);
      deepStrictEqual(frame(game, pressed, debug), changed);
      const back = frame(game, { ...pressed, menuAction: 2 }, debug);
      expect(back.output.indexedPixels[0]).toBe(7);
      expect(frame(game, pressed, debug).output.indexedPixels[0]).toBe(1);
    });
  }

  it('rejects context rebinding and malformed snapshots without mutating the live game', () => {
    const game = createGameRuntime(definition(false));
    frame(game, emptyInputFrame(), false);
    const before = game.snapshot();
    const mutations: ((value: GameSnapshot) => unknown)[] = [
      (s) => ({ ...s, revision: 0 }),
      (s) => ({ ...s, revision: 2 }),
      (s) => ({ ...s, identity: 'c'.repeat(64) }),
      (s) => ({ ...s, seed: 0 }),
      (s) => ({ ...s, debug: true }),
      (s) => ({ ...s, frame: -1 }),
      (s) => ({ ...s, frame: Number.MAX_SAFE_INTEGER + 1 }),
      (s) => ({ ...s, frame: 0 }),
      (s) => ({ ...s, alias: 'absent' }),
      (s) => ({ ...s, parameter: '\ud800' }),
      (s) => ({ ...s, bankAlias: 'absent' }),
      (s) => ({ ...s, extra: true }),
      (s) => ({ ...s, saves: {} }),
      (s) => ({ ...s, saves: { ...s.saves, another: new Uint8Array(8192) } }),
      (s) => ({ ...s, saves: { ...s.saves, 'game-main': new Uint8Array(8191) } }),
      (s) => ({ ...s, inputBarrier: { ...s.inputBarrier, pointer: 4 } }),
      (s) => ({ ...s, console: { ...s.console, cartridgeControl: null } }),
      (s) => ({
        ...s,
        console: {
          ...s.console,
          cartridgeControl: { ...s.console.cartridgeControl, alias: 'arena' },
        },
      }),
      (s) => ({ ...s, console: { ...s.console, memory: { ...s.console.memory, regions: [] } } }),
      (s) => ({
        ...s,
        console: { ...s.console, machine: { ...s.console.machine, updateRate: 30 } },
      }),
    ];
    for (const mutate of mutations) {
      expect(() => {
        game.restore(mutate(structuredClone(before)));
      }).toThrow(expect.objectContaining({ code: 'M019103' }));
      deepStrictEqual(game.snapshot(), before);
    }
    const copy = game.snapshot();
    copy.saves['game-main']?.fill(255);
    copy.console.memory.regions[0]?.bytes.fill(255);
    deepStrictEqual(game.snapshot(), before);
  });

  it('leaves a suspended source continuation intact when a candidate restore fails', () => {
    const game = createGameRuntime(definition(true));
    boot(game);
    const checkpoint = game.snapshot();
    const reference = createGameRuntime(definition(true));
    reference.restore(checkpoint);
    const input = { ...emptyInputFrame(), menuAction: 1 };
    expect('event' in game.stepDebug(input)).toBe(true);
    const before = game.inspect();
    expect(() => game.runFrame(held())).toThrow(/executing/);
    expect(game.inspect()).toEqual(before);
    const malformed = {
      ...checkpoint,
      console: { ...checkpoint.console, memory: { revision: 1, regions: [] } },
    };
    expect(() => {
      game.restore(malformed);
    }).toThrow(expect.objectContaining({ code: 'M019103' }));
    expect(game.inspect()).toEqual(before);
    deepStrictEqual(frame(game, emptyInputFrame(), true), frame(reference, input, true));
  });

  it('produces identical release/debug outputs and work across loads and reloads', () => {
    for (const make of [definition, reloadDefinition]) {
      const release = createGameRuntime(make(false)),
        debug = createGameRuntime(make(true));
      const inputs = [
        emptyInputFrame(),
        { ...emptyInputFrame(), menuAction: 1 },
        emptyInputFrame(),
        emptyInputFrame(),
        { ...emptyInputFrame(), menuAction: 1 },
        emptyInputFrame(),
      ];
      const trace = decodeReplayTrace(
        encodeReplayTrace(inputs.map((input, frame) => ({ frame, input }))),
      );
      for (const input of replayInputFrames(trace).values()) {
        const a = frame(release, input, false),
          b = frame(debug, input, true);
        const { debug: discardedDebug, ...withoutDebug } = b;
        expect(discardedDebug).toBeDefined();
        deepStrictEqual(withoutDebug, a);
      }
    }
  });
});

describe('whole-game runtime admission and boundary devices', () => {
  const simple = (members: GameDefinition['members']): GameDefinition => ({
    identity: 'c'.repeat(64),
    seed: 0,
    members,
  });
  const member = { alias: 'main', id: 'save-main', factory: blank, configuration };

  it('rejects debug operations on release without capturing a later frame input', () => {
    const game = createGameRuntime(simple([member]));
    const before = game.snapshot();
    expect(() => game.stepDebug(held())).toThrow(expect.objectContaining({ code: 'M019104' }));
    deepStrictEqual(game.snapshot(), before);
    game.runFrame(emptyInputFrame());
    expect(game.snapshot().console.machine.input).toEqual(emptyInputFrame());
  });

  it('rejects invalid definitions, undeclared saves and incompatible bank requests before mutation', () => {
    const valid = simple([member]);
    for (const definition of [
      { ...valid, identity: '' },
      { ...valid, seed: Infinity },
      { ...valid, seed: -1 },
      { ...valid, seed: 0x1_0000_0000 },
      { ...valid, parameter: '\ud800' },
      { ...valid, debug: 'yes' },
      { ...valid, extra: true },
      { ...valid, members: [] },
      {
        ...valid,
        members: Array.from({ length: 33 }, (_, index) => ({
          ...member,
          alias: index === 0 ? 'main' : `cart${String(index)}`,
          id: `save-${String(index)}`,
        })),
      },
      { ...valid, saves: { undeclared: new Uint8Array(8192) } },
      { ...valid, saves: { 'save-main': new Uint8Array(8193) } },
      { ...valid, members: [{ ...member, alias: '../external' }] },
      { ...valid, members: [{ ...member, configuration: { ...configuration, seed: 1 } }] },
    ])
      expect(() => createGameRuntime(definition as GameDefinition)).toThrow();
    const game = createGameRuntime(
      simple([
        {
          ...member,
          factory: (api) => ({
            ...blank(api),
            update() {
              api.call('reload_assets', ['donor'], span);
            },
          }),
        },
        {
          ...member,
          alias: 'donor',
          id: 'save-donor',
          configuration: { ...configuration, assets: bank(7, 0) },
        },
      ]),
    );
    expect(() => game.runFrame(emptyInputFrame())).toThrow(/incompatible/);
    expect(game.inspect()).toMatchObject({ frame: 0, bankAlias: 'main', pending: null });
  });

  it('preserves the last legal global frame and rejects overflow without consuming input', () => {
    const game = createGameRuntime(simple([member]));
    const before = { ...game.snapshot(), frame: Number.MAX_SAFE_INTEGER - 1 };
    game.restore(before);
    expect(game.runFrame(emptyInputFrame()).frame).toBe(Number.MAX_SAFE_INTEGER - 1);
    const exhausted = game.snapshot();
    expect(() => game.runFrame(held())).toThrow(expect.objectContaining({ code: 'M019012' }));
    deepStrictEqual(game.snapshot(), exhausted);
    expect(() => game.runFrame(null as unknown as InputFrame)).toThrow(
      expect.objectContaining({ code: 'M019008' }),
    );
    deepStrictEqual(game.snapshot(), exhausted);
  });

  it('retains saves by persistent identity when a member alias is renamed', () => {
    const game = createGameRuntime({
      ...simple([
        {
          ...member,
          factory: (api) => ({
            ...blank(api),
            update() {
              api.call('load_cart', ['newname', ''], span);
            },
          }),
        },
        { ...member, alias: 'newname', id: 'stable-child' },
      ]),
      saves: { 'stable-child': { score: 51 } },
    });
    game.runFrame(emptyInputFrame());
    expect(game.inspect()).toMatchObject({
      cartridgeAlias: 'newname',
      cartridgeId: 'stable-child',
    });
    expect(decodeSaveValues(game.snapshot().console.save.committed)).toEqual({ score: 51 });
  });

  it('admits all members before boot and owns defensive data/save copies', () => {
    let boots = 0;
    const counting: CartridgeFactory = (api) => ({
      ...blank(api),
      start() {
        boots++;
      },
    });
    expect(() =>
      createGameRuntime(
        simple([
          { ...member, factory: counting },
          {
            ...member,
            alias: 'unused',
            id: 'unused-cart',
            configuration: {
              ...configuration,
              assets: {
                declarations: { hero: { kind: 'sprite', path: 'missing.m01g' } },
                files: {},
              },
            },
          },
        ]),
      ),
    ).toThrow(/missing/);
    expect(boots).toBe(0);
    expect(() => createGameRuntime(simple([member, { ...member, alias: 'other' }]))).toThrow(
      /identity/,
    );
    expect(() => createGameRuntime(simple([{ ...member, alias: 'other' }]))).toThrow(/main/);
    const data = bank(7, 0),
      image = new Uint8Array(8192);
    image[0] = 13;
    const definition = {
      ...simple([{ ...member, configuration: { ...configuration, assets: data } }]),
      saves: { 'save-main': image },
    };
    const game = createGameRuntime(definition);
    image.fill(255);
    data.files['hero.m01g']?.fill(255);
    const snapshot = game.snapshot();
    expect(snapshot.saves['save-main']?.[0]).toBe(13);
    game.restore(snapshot); // redecodes the owned original asset source
    deepStrictEqual(game.snapshot(), snapshot);
  });

  it('cycles through all 32 cartridges on one monotonic timeline with isolated committed saves', () => {
    const aliases = Array.from({ length: 32 }, (_, index) =>
      index === 0 ? 'main' : `cart${String(index)}`,
    );
    const game = createGameRuntime(
      simple(
        aliases.map((alias, index) => ({
          alias,
          id: `save-${String(index)}`,
          configuration,
          factory: (api) => ({
            ...blank(api),
            start() {
              api.call(
                'save_set_int',
                ['visits', (api.call('save_get_int', ['visits', 0], span) as number) + 1],
                span,
              );
            },
            update() {
              api.call('load_cart', [aliases[(index + 1) % aliases.length], String(index)], span);
            },
            draw() {
              api.call('clear', [index], span);
            },
          }),
        })),
      ),
    );
    for (let index = 0; index < 64; index++) {
      const result = game.runFrame(emptyInputFrame());
      expect(result).toMatchObject({
        frame: index,
        cartridgeFrame: 0,
        cartridgeAlias: aliases[index % 32],
      });
      expect(result.output.indexedPixels[0]).toBe(index % 32);
    }
    for (const bytes of Object.values(game.snapshot().saves))
      expect(decodeSaveValues(bytes)).toEqual({ visits: 2 });
  });

  it('resets old voices at reload while returning their last completed audio frame', () => {
    const assets = bank(1, 0);
    const tone = encodeAssetFile({
      revision: 1,
      kind: 'sound',
      waveform: 'triangle',
      note: 60,
      durationFrames: 20,
      volume: 0.5,
      pan: 0,
      envelope: { attackFrames: 0, decayFrames: 0, sustainLevel: 1, releaseFrames: 0 },
      pitch: { slideSemitonesPerFrame: 0, vibratoDepthSemitones: 0, vibratoPeriodFrames: 0 },
    });
    const source = {
      ...assets,
      declarations: { ...assets.declarations, tone: { kind: 'sound' as const, path: 'tone.m01s' } },
      files: { ...assets.files, 'tone.m01s': tone },
    };
    const game = createGameRuntime(
      simple([
        {
          ...member,
          configuration: { ...configuration, assets: source },
          factory: (api) => ({
            ...blank(api),
            start() {
              api.call('sfx', [{ kind: 'Sound', name: 'tone' }], span);
              api.call('menu_item', [1, 'RELOAD'], span);
            },
            menu() {
              api.call('reload_assets', ['donor'], span);
            },
          }),
        },
        {
          ...member,
          alias: 'donor',
          id: 'save-donor',
          configuration: { ...configuration, assets: source },
        },
      ]),
    );
    expect(game.runFrame(emptyInputFrame()).output.audio.activeVoices).toBe(1);
    expect(game.runFrame({ ...emptyInputFrame(), menuAction: 1 }).output.audio.activeVoices).toBe(
      1,
    );
    expect(game.snapshot().console.audio.voices.every((voice) => !voice.active)).toBe(true);
    expect(game.runFrame(emptyInputFrame()).output.audio.activeVoices).toBe(0);
    expect(
      game.snapshot().console.memory.regions.some((region) => region.address === MEMORY.visual),
    ).toBe(true);
  });
});
