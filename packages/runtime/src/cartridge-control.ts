import { RuntimeFault } from './errors';
import { HARDWARE } from './hardware';
import type { ExecutionContext } from './machine';
import type { SourceSpan } from './protocol';

export const TRANSFER_PARAMETER_BYTES = 1024;

export interface CartridgeEnvironment {
  readonly alias: string;
  readonly parameter: string;
  readonly aliases: readonly string[];
  /** Only compatible, pre-admitted asset banks appear here. Values are synthetic work costs. */
  readonly reloadCosts: Readonly<Record<string, number>>;
}

export type CartridgeRequest =
  | { readonly kind: 'load'; readonly alias: string; readonly parameter: string }
  | { readonly kind: 'reload'; readonly alias: string };

export interface CartridgeControlSnapshot {
  readonly revision: 1;
  readonly alias: string;
  readonly parameter: string;
  readonly pending: CartridgeRequest | null;
}

export function isCartridgeAlias(value: unknown): value is string {
  return typeof value === 'string' && /^[a-z][a-z0-9_-]{0,31}$/.test(value);
}

export function isTransferParameter(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    value.length <= TRANSFER_PARAMETER_BYTES &&
    value.isWellFormed() &&
    new TextEncoder().encode(value).length <= TRANSFER_PARAMETER_BYTES
  );
}

export function isCartridgeEnvironment(value: unknown): value is CartridgeEnvironment {
  if (!record(value) || !Array.isArray(value.aliases)) return false;
  const aliases: readonly unknown[] = value.aliases;
  return (
    exact(value, ['alias', 'parameter', 'aliases', 'reloadCosts']) &&
    isCartridgeAlias(value.alias) &&
    isTransferParameter(value.parameter) &&
    aliases.length >= 1 &&
    aliases.length <= 32 &&
    Object.keys(aliases).length === aliases.length &&
    Array.from(aliases).every(isCartridgeAlias) &&
    new Set(aliases).size === aliases.length &&
    aliases.includes('main') &&
    aliases.includes(value.alias) &&
    record(value.reloadCosts) &&
    Object.entries(value.reloadCosts).every(
      ([alias, cost]) =>
        aliases.includes(alias) &&
        typeof cost === 'number' &&
        Number.isSafeInteger(cost) &&
        cost >= 1 &&
        cost <= HARDWARE.workUnitsPerFrame,
    )
  );
}

function request(value: unknown): value is CartridgeRequest {
  if (!record(value) || !isCartridgeAlias(value.alias)) return false;
  return value.kind === 'load'
    ? exact(value, ['kind', 'alias', 'parameter']) && isTransferParameter(value.parameter)
    : value.kind === 'reload' && exact(value, ['kind', 'alias']);
}

export function isCartridgeControlSnapshot(value: unknown): value is CartridgeControlSnapshot {
  return (
    record(value) &&
    exact(value, ['revision', 'alias', 'parameter', 'pending']) &&
    value.revision === 1 &&
    isCartridgeAlias(value.alias) &&
    isTransferParameter(value.parameter) &&
    (value.pending === null || request(value.pending))
  );
}

/** A cartridge can request a boundary operation, never a filesystem read or an immediate reset. */
export class CartridgeControl {
  private readonly alias: string;
  private readonly parameter: string;
  private readonly aliases: ReadonlySet<string>;
  private readonly reloadCosts: ReadonlyMap<string, number>;
  private pending: CartridgeRequest | null = null;

  public constructor(environment: CartridgeEnvironment) {
    if (!isCartridgeEnvironment(environment)) throw new TypeError('invalid cartridge environment');
    this.alias = environment.alias;
    this.parameter = environment.parameter;
    this.aliases = new Set(environment.aliases);
    this.reloadCosts = new Map(Object.entries(environment.reloadCosts));
  }

  public call(
    name: string,
    args: readonly unknown[],
    span: SourceSpan,
    context: ExecutionContext,
    charge: (units: number, span: SourceSpan) => void,
  ): unknown {
    if (name === 'cart_parameter') {
      if (args.length !== 0) fail('cart_parameter expects no arguments', span);
      charge(1 + new TextEncoder().encode(this.parameter).length, span);
      return this.parameter;
    }
    if (name !== 'load_cart' && name !== 'reload_assets')
      fail('unknown cartridge control call', span);
    if (!['start', 'update', 'draw', 'menu'].includes(context.phase))
      fail('cartridge changes require a non-raster cartridge callback', span);
    if (
      args.length !== (name === 'load_cart' ? 2 : 1) ||
      !isCartridgeAlias(args[0]) ||
      !this.aliases.has(args[0])
    )
      fail('cartridge change requires a declared local alias', span);
    if (this.pending !== null) fail('only one cartridge change can be requested per frame', span);
    const alias = args[0];
    if (name === 'load_cart') {
      if (!isTransferParameter(args[1]))
        fail('transfer parameter exceeds 1024 UTF-8 bytes or is not well-formed Text', span);
      const parameter = args[1];
      charge(1 + alias.length + new TextEncoder().encode(parameter).length, span);
      this.pending = { kind: 'load', alias, parameter };
    } else {
      const cost = this.reloadCosts.get(alias);
      if (cost === undefined)
        fail('asset bank names/kinds are incompatible with the current cartridge', span);
      charge(cost, span);
      this.pending = { kind: 'reload', alias };
    }
    return undefined;
  }

  public pendingRequest(): CartridgeRequest | null {
    return this.pending === null ? null : { ...this.pending };
  }

  public snapshot(): CartridgeControlSnapshot {
    return {
      revision: 1,
      alias: this.alias,
      parameter: this.parameter,
      pending: this.pendingRequest(),
    };
  }

  public restore(value: unknown): void {
    if (
      !isCartridgeControlSnapshot(value) ||
      value.alias !== this.alias ||
      value.parameter !== this.parameter ||
      (value.pending !== null &&
        (!this.aliases.has(value.pending.alias) ||
          (value.pending.kind === 'reload' && !this.reloadCosts.has(value.pending.alias))))
    )
      fail('invalid or mismatched cartridge-control snapshot', { start: 0, end: 0 });
    this.pending = value.pending === null ? null : { ...value.pending };
  }
}

function fail(message: string, span: SourceSpan): never {
  throw new RuntimeFault('M019017', message, span);
}
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function exact(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value);
  return actual.length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}
