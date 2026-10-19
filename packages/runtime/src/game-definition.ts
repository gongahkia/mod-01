import { decodeRuntimeAssets } from './asset-codec';
import type { RuntimeAssetSource } from './asset-codec';
import { CartridgeControl, isCartridgeAlias, isTransferParameter } from './cartridge-control';
import { createConsoleRuntime, type ConsoleRuntime } from './console-runtime';
import { HARDWARE } from './hardware';
import { MEMORY } from './bus';
import type { CartridgeFactory } from './machine';
import { isConsoleConfiguration, type ConsoleConfiguration } from './protocol';
import { SaveMemory, isSaveImage, type SaveImage } from './save';

export type GameMemberConfiguration = Omit<ConsoleConfiguration, 'seed' | 'save' | 'debug'>;
export interface GameMemberDefinition {
  readonly alias: string;
  readonly id: string;
  /** Supplied only after shared Rust source/program admission, never arbitrary cartridge imports. */
  readonly factory: CartridgeFactory;
  readonly configuration: GameMemberConfiguration;
}
export interface GameDefinition {
  readonly identity: string;
  readonly seed: number;
  readonly debug?: boolean;
  readonly parameter?: string;
  readonly members: readonly GameMemberDefinition[];
  /** Persistent identity, not alias, keys initial committed images. */
  readonly saves?: Readonly<Record<string, SaveImage>>;
}
interface AdmittedMember extends GameMemberDefinition {
  readonly signature: string;
  readonly reloadUnits: number;
}

/** Private scheduler-owned copies of immutable member data and initial save images. */
export class GameCatalog {
  public readonly identity: string;
  public readonly seed: number;
  public readonly debug: boolean;
  public readonly parameter: string;
  public readonly initialSaves: ReadonlyMap<string, Uint8Array>;
  private readonly members = new Map<string, AdmittedMember>();

  public constructor(definition: GameDefinition) {
    if (
      !record(definition) ||
      !exact(definition, [
        'identity',
        'seed',
        'members',
        ...(definition.debug === undefined ? [] : ['debug']),
        ...(definition.parameter === undefined ? [] : ['parameter']),
        ...(definition.saves === undefined ? [] : ['saves']),
      ]) ||
      typeof definition.identity !== 'string' ||
      !/^[a-f0-9]{64}$/.test(definition.identity) ||
      !Number.isSafeInteger(definition.seed) ||
      definition.seed < 0 ||
      definition.seed > 0xffff_ffff ||
      (definition.debug !== undefined && typeof definition.debug !== 'boolean') ||
      !isTransferParameter(definition.parameter ?? '') ||
      !Array.isArray(definition.members) ||
      definition.members.length < 1 ||
      definition.members.length > 32 ||
      Object.keys(definition.members).length !== definition.members.length
    )
      throw new TypeError('invalid admitted game definition');
    this.identity = definition.identity;
    this.seed = definition.seed;
    this.debug = definition.debug === true;
    this.parameter = definition.parameter ?? '';
    const ids = new Set<string>();
    for (const raw of definition.members as readonly unknown[]) {
      if (
        !record(raw) ||
        !exact(raw, ['alias', 'id', 'factory', 'configuration']) ||
        !isCartridgeAlias(raw.alias) ||
        typeof raw.id !== 'string' ||
        !/^[a-z0-9][a-z0-9.-]{2,63}$/.test(raw.id) ||
        typeof raw.factory !== 'function' ||
        !record(raw.configuration) ||
        ['seed', 'debug', 'save'].some((key) => Object.hasOwn(raw.configuration as object, key)) ||
        this.members.has(raw.alias) ||
        ids.has(raw.id)
      )
        throw new TypeError('invalid or duplicate game member alias/save identity');
      const configuration = structuredClone(raw.configuration);
      if (!isConsoleConfiguration({ ...configuration, seed: this.seed, debug: this.debug }))
        throw new TypeError(`invalid console configuration for '${raw.alias}'`);
      const data = configuration as unknown as GameMemberConfiguration;
      const decoded =
        data.assets === undefined
          ? decodeRuntimeAssets({}, {})
          : decodeRuntimeAssets(
              data.assets.declarations,
              data.assets.files,
              data.assets.displayPath,
            );
      const names =
        data.assets === undefined
          ? (data.maps ?? []).map((map) => [map.name, 'map'])
          : Object.entries(data.assets.declarations).map(([name, asset]) => [name, asset.kind]);
      names.sort(([a = ''], [b = '']) => (a < b ? -1 : a > b ? 1 : 0));
      const metadata =
        HARDWARE.visualAllocations * MEMORY.allocationStride +
        4096 * MEMORY.assetStride +
        decoded.audio.length * MEMORY.audioAssetStride;
      this.members.set(raw.alias, {
        alias: raw.alias,
        id: raw.id,
        factory: raw.factory as CartridgeFactory,
        configuration: data,
        signature: JSON.stringify(names),
        reloadUnits: 1 + Math.ceil((HARDWARE.visualCapacityBytes + metadata) / 64),
      });
      ids.add(raw.id);
    }
    if (!this.members.has('main')) throw new TypeError('game requires a main entry cartridge');
    if (
      definition.saves !== undefined &&
      (!record(definition.saves) || Object.keys(definition.saves).some((id) => !ids.has(id)))
    )
      throw new TypeError('initial saves contain an undeclared cartridge identity');
    const saves = new Map<string, Uint8Array>();
    for (const id of [...ids].sort()) {
      const image =
        definition.saves !== undefined && Object.hasOwn(definition.saves, id)
          ? definition.saves[id]
          : undefined;
      if (image !== undefined && !isSaveImage(image))
        throw new TypeError(`invalid save image for '${id}'`);
      saves.set(id, new SaveMemory(image).deviceSnapshot().committed);
    }
    this.initialSaves = saves;
  }

  public member(alias: string): AdmittedMember {
    const member = this.members.get(alias);
    if (member === undefined) throw new TypeError(`unknown local cartridge '${alias}'`);
    return member;
  }
  public ids(): readonly string[] {
    return [...this.members.values()].map((member) => member.id).sort();
  }
  public compatible(alias: string, bank: string): boolean {
    const a = this.members.get(alias),
      b = this.members.get(bank);
    return (
      a !== undefined &&
      b !== undefined &&
      a.signature === b.signature &&
      (a.configuration.assets === undefined) === (b.configuration.assets === undefined) &&
      isConsoleConfiguration({
        ...(alias === bank
          ? a.configuration
          : reloadConfiguration(a.configuration, b.configuration)),
        seed: this.seed,
        debug: this.debug,
      })
    );
  }
  public prepare(
    alias: string,
    parameter: string,
    bank: string,
    save: Uint8Array,
    deferBoot: boolean,
  ): {
    readonly runtime: ConsoleRuntime;
    readonly control: CartridgeControl;
  } {
    if (!isTransferParameter(parameter) || !this.compatible(alias, bank))
      throw new TypeError('incompatible game cartridge context/bank');
    const member = this.member(alias),
      donor = this.member(bank);
    const aliases = [...this.members.keys()].sort();
    const costs = Object.fromEntries(
      aliases
        .filter((name) => this.compatible(alias, name))
        .map((name) => [name, this.member(name).reloadUnits]),
    );
    const control = new CartridgeControl({ alias, parameter, aliases, reloadCosts: costs });
    const configuration =
      alias === bank
        ? member.configuration
        : reloadConfiguration(member.configuration, donor.configuration);
    const runtime = createConsoleRuntime(
      member.factory,
      { ...configuration, seed: this.seed, debug: this.debug, save },
      { cartridge: control, deferBoot },
    );
    return { runtime, control };
  }
}

function reloadConfiguration(
  active: GameMemberConfiguration,
  donor: GameMemberConfiguration,
): GameMemberConfiguration {
  const base: GameMemberConfiguration = {
    updateRate: active.updateRate,
    workUnitsPerFrame: active.workUnitsPerFrame,
    ...(active.hardwareRevision === undefined ? {} : { hardwareRevision: active.hardwareRevision }),
    ...(active.rom === undefined ? {} : { rom: active.rom }),
  };
  const bank = donor.assets;
  if (bank === undefined)
    return { ...base, ...(donor.maps === undefined ? {} : { maps: donor.maps }) };
  const files: Record<string, Uint8Array> = Object.fromEntries(
    Object.values(bank.declarations).map(({ path }) => [path, bank.files[path] as Uint8Array]),
  );
  const ownDisplay = active.assets?.displayPath;
  let displayPath: string | null = null;
  if (typeof ownDisplay === 'string') {
    let suffix = 0;
    do {
      displayPath = `__mod01__/retained-display-${String(suffix++)}.json`;
    } while (Object.hasOwn(files, displayPath));
    const display = active.assets?.files[ownDisplay];
    if (display === undefined) throw new TypeError('active display source missing');
    files[displayPath] = display;
  }
  const assets: RuntimeAssetSource = { declarations: bank.declarations, files, displayPath };
  return { ...base, assets };
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function exact(value: object, keys: readonly string[]): boolean {
  return (
    Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key))
  );
}
