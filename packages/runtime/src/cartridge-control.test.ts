import { describe, expect, it } from 'vitest';
import { WorkBudget } from './budget';
import {
  CartridgeControl,
  isCartridgeEnvironment,
  isCartridgeControlSnapshot,
  isTransferParameter,
  type CartridgeEnvironment,
} from './cartridge-control';

const span = { start: 5, end: 12 };
const context = { frame: 1, phase: 'update' } as const;
const environment: CartridgeEnvironment = {
  alias: 'main',
  parameter: 'lives=3',
  aliases: ['main', 'arena', 'incompatible'],
  reloadCosts: { main: 20, arena: 25 },
};
const ignoreCharge = () => {};

describe('bounded deferred cartridge controls', () => {
  it('copies admission metadata and queues one local load without changing current state', () => {
    const external = structuredClone(environment);
    const control = new CartridgeControl(external);
    Object.assign(external.reloadCosts, { arena: 1 });
    Object.assign(external.aliases, { 1: 'another' });
    const costs: number[] = [];
    const charge = (cost: number) => costs.push(cost);
    expect(control.call('cart_parameter', [], span, context, charge)).toBe('lives=3');
    control.call('load_cart', ['arena', 'é🙂'], span, context, charge);
    expect(costs).toEqual([8, 12]); // parameter bytes, then 1 + 5 alias bytes + 6 parameter bytes
    expect(control.pendingRequest()).toEqual({ kind: 'load', alias: 'arena', parameter: 'é🙂' });
    expect(control.snapshot().alias).toBe('main');
    expect(control.call('cart_parameter', [], span, context, ignoreCharge)).toBe('lives=3');
    const pending = control.pendingRequest();
    Object.assign(pending ?? {}, { alias: 'host-mutated' });
    expect(control.pendingRequest()?.alias).toBe('arena');
    expect(() => control.call('reload_assets', ['main'], span, context, charge)).toThrow(
      /one cartridge change/,
    );
  });

  it('validates UTF-8 transfer boundaries and rejects unpaired UTF-16 surrogates', () => {
    for (const value of ['', 'a'.repeat(1024), 'é'.repeat(512), '🙂'.repeat(256)])
      expect(isTransferParameter(value)).toBe(true);
    for (const value of [
      'a'.repeat(1025),
      'é'.repeat(513),
      '🙂'.repeat(257),
      '\ud800',
      '\udc00',
      7,
      null,
    ])
      expect(isTransferParameter(value)).toBe(false);
  });

  it('charges load/reload before mutation and cannot queue requests after budget exhaustion', () => {
    for (const [name, args, cost] of [
      ['load_cart', ['arena', 'é🙂'], 12],
      ['reload_assets', ['arena'], 25],
    ] as const) {
      const control = new CartridgeControl(environment);
      const before = control.snapshot();
      const budget = new WorkBudget(cost - 1);
      expect(() =>
        control.call(name, args, span, context, (units, at) => {
          budget.charge(units, at);
        }),
      ).toThrow(/work/);
      expect(control.snapshot()).toEqual(before);
      const full = new WorkBudget(cost);
      control.call(name, args, span, context, (units, at) => {
        full.charge(units, at);
      });
      expect(full.used).toBe(cost);
      expect(control.pendingRequest()?.alias).toBe('arena');
    }
  });

  it('rejects unknown/local-path aliases, invalid arguments and incompatible banks atomically', () => {
    for (const [name, args] of [
      ['load_cart', []],
      ['load_cart', ['arena']],
      ['load_cart', ['arena', '', true]],
      ['load_cart', ['../arena', '']],
      ['load_cart', ['https://cart', '']],
      ['load_cart', ['unknown', '']],
      ['load_cart', ['arena', null]],
      ['load_cart', ['arena', '🙂'.repeat(257)]],
      ['reload_assets', ['incompatible']],
      ['reload_assets', []],
      ['cart_parameter', ['extra']],
      ['unknown', []],
    ] as const) {
      const control = new CartridgeControl(environment);
      const before = control.snapshot();
      expect(() => control.call(name, args, span, context, ignoreCharge)).toThrow(
        expect.objectContaining({ code: 'M019017', sourceSpan: span }),
      );
      expect(control.snapshot()).toEqual(before);
    }
  });

  it('allows callback requests but never changes the machine during raster/output/idle', () => {
    for (const phase of ['start', 'update', 'draw', 'menu'] as const) {
      const control = new CartridgeControl(environment);
      control.call('load_cart', ['main', 'restart'], span, { frame: 0, phase }, ignoreCharge);
      expect(control.pendingRequest()?.alias).toBe('main');
    }
    for (const phase of ['raster', 'output', 'idle'] as const) {
      const control = new CartridgeControl(environment);
      expect(() =>
        control.call('load_cart', ['arena', ''], span, { frame: 0, phase }, ignoreCharge),
      ).toThrow(/callback/);
      expect(() =>
        control.call('reload_assets', ['arena'], span, { frame: 0, phase }, ignoreCharge),
      ).toThrow(/callback/);
      expect(control.pendingRequest()).toBeNull();
    }
  });

  it('restores strict context-bound pending operations defensively and without partial updates', () => {
    const control = new CartridgeControl(environment);
    const before = control.snapshot();
    control.call('load_cart', ['arena', 'lives=2'], span, context, ignoreCharge);
    const saved = control.snapshot();
    control.restore(before);
    expect(control.pendingRequest()).toBeNull();
    control.restore(saved);
    Object.assign(saved.pending ?? {}, { parameter: 'mutated' });
    expect(control.pendingRequest()).toEqual({
      kind: 'load',
      alias: 'arena',
      parameter: 'lives=2',
    });
    const expected = control.snapshot();
    for (const bad of [
      null,
      {},
      { ...expected, revision: 2 },
      { ...expected, extra: true },
      { ...expected, alias: 'arena' },
      { ...expected, parameter: '' },
      { ...expected, pending: { kind: 'load', alias: 'unknown', parameter: '' } },
      { ...expected, pending: { kind: 'load', alias: 'arena', parameter: '', extra: 1 } },
      { ...expected, pending: { kind: 'reload', alias: 'incompatible' } },
      { ...expected, pending: { kind: 'reload', alias: 'arena', parameter: '' } },
    ]) {
      expect(() => {
        control.restore(bad);
      }).toThrow(/snapshot/);
      expect(control.snapshot()).toEqual(expected);
    }
    expect(isCartridgeControlSnapshot(expected)).toBe(true);
    expect(isCartridgeControlSnapshot({ ...expected, pending: undefined })).toBe(false);
  });

  it('requires a complete bounded flat environment with main, unique aliases and valid costs', () => {
    expect(isCartridgeEnvironment(environment)).toBe(true);
    for (const bad of [
      null,
      { ...environment, alias: 'unknown' },
      { ...environment, parameter: '\ud800' },
      { ...environment, aliases: ['arena'] },
      { ...environment, aliases: ['main', 'main'] },
      { ...environment, aliases: Array(3) },
      {
        ...environment,
        aliases: ['main', ...Array.from({ length: 32 }, (_, index) => `cart-${String(index)}`)],
      },
      { ...environment, reloadCosts: { unknown: 20 } },
      { ...environment, reloadCosts: { main: 0 } },
      { ...environment, reloadCosts: { main: 50_001 } },
      { ...environment, reloadCosts: { main: NaN } },
      { ...environment, reloadCosts: { main: 2.5 } },
      { ...environment, extra: true },
    ]) {
      expect(isCartridgeEnvironment(bad)).toBe(false);
      expect(() => new CartridgeControl(bad as CartridgeEnvironment)).toThrow(/environment/);
    }
  });
});
