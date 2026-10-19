import { isRuntimeAssetSource, type ProjectAssetDeclaration } from './asset-codec';
import { isCartridgeAlias } from './cartridge-control';
import { GameCatalog, type GameDefinition, type GameMemberDefinition } from './game-definition';
import { createGameRuntime, type GameRuntime } from './game-runtime';
import { HARDWARE } from './hardware';
import type { CartridgeFactory } from './machine';

export interface RuntimeGameManifest {
  readonly id: string;
  readonly hardwareRevision: 2;
  readonly updateRate: 30 | 60;
  readonly display: string | null;
  readonly assets: Readonly<Record<string, ProjectAssetDeclaration>>;
}
export interface RuntimeGameMember {
  readonly javascript: string;
  readonly sourceMap: string;
  readonly symbolNames: Readonly<Record<string, string>>;
  readonly manifest: RuntimeGameManifest;
  readonly files: Readonly<Record<string, string>>;
  readonly rom: string;
}
/** Internal data-only handoff emitted by shared Rust, not a user-authored cartridge format. */
export interface RuntimeGamePayload {
  readonly revision: 1;
  readonly identity: string;
  readonly debug: boolean;
  readonly members: Readonly<Record<string, RuntimeGameMember>>;
}
export type GameLoadOptions = Pick<GameDefinition, 'seed' | 'parameter' | 'saves'>;
export interface LoadedRuntimeGame {
  readonly runtime: GameRuntime;
  readonly metadata: ReadonlyMap<string, RuntimeGameMember>;
}

/** Validates every member's data before importing any Rust-admitted compiled module. */
export async function loadRuntimeGame(
  payload: unknown,
  options: GameLoadOptions,
): Promise<LoadedRuntimeGame> {
  if (
    !record(payload) ||
    !exact(payload, ['revision', 'identity', 'debug', 'members']) ||
    payload.revision !== 1 ||
    typeof payload.identity !== 'string' ||
    !/^[a-f0-9]{64}$/.test(payload.identity) ||
    typeof payload.debug !== 'boolean' ||
    !record(payload.members) ||
    Object.keys(payload.members).length < 1 ||
    Object.keys(payload.members).length > 32 ||
    !Object.hasOwn(payload.members, 'main')
  )
    throw new TypeError('invalid current runtime game payload');
  const members: GameMemberDefinition[] = [];
  const metadata = new Map<string, RuntimeGameMember>();
  let transportBytes = 0;
  for (const [alias, raw] of Object.entries(payload.members)) {
    if (
      !isCartridgeAlias(alias) ||
      !record(raw) ||
      !exact(raw, ['javascript', 'sourceMap', 'symbolNames', 'manifest', 'files', 'rom']) ||
      typeof raw.javascript !== 'string' ||
      !raw.javascript.isWellFormed() ||
      raw.javascript.length === 0 ||
      Buffer.byteLength(raw.javascript, 'utf8') > 2 * 1024 * 1024 ||
      typeof raw.sourceMap !== 'string' ||
      Buffer.byteLength(raw.sourceMap, 'utf8') > 128 * 1024 * 1024 ||
      !record(raw.symbolNames) ||
      Object.entries(raw.symbolNames).some(
        ([key, value]) =>
          !/^s[0-9]{1,10}$/.test(key) ||
          typeof value !== 'string' ||
          value.length < 1 ||
          value.length > 1024 ||
          !value.isWellFormed(),
      ) ||
      !record(raw.manifest) ||
      !exact(raw.manifest, ['id', 'hardwareRevision', 'updateRate', 'display', 'assets']) ||
      typeof raw.manifest.id !== 'string' ||
      raw.manifest.hardwareRevision !== 2 ||
      (raw.manifest.updateRate !== 30 && raw.manifest.updateRate !== 60) ||
      (raw.manifest.display !== null && typeof raw.manifest.display !== 'string') ||
      !record(raw.files) ||
      Object.keys(raw.files).length > 4096
    )
      throw new TypeError(`invalid runtime member '${alias}'`);
    validateSourceMap(raw.sourceMap);
    const files: Record<string, Uint8Array> = {};
    for (const [path, value] of Object.entries(raw.files))
      files[path] = decodeRuntimeBytes(value, 16 * 1024 * 1024);
    const assets = { declarations: raw.manifest.assets, files, displayPath: raw.manifest.display };
    if (!isRuntimeAssetSource(assets))
      throw new TypeError(`invalid runtime asset source '${alias}'`);
    const rom = decodeRuntimeBytes(raw.rom, HARDWARE.cartridgeCapacityBytes);
    transportBytes +=
      Buffer.byteLength(raw.javascript, 'utf8') +
      Buffer.byteLength(raw.sourceMap, 'utf8') +
      rom.byteLength +
      Object.values(files).reduce((sum, bytes) => sum + bytes.byteLength, 0);
    if (transportBytes > 512 * 1024 * 1024)
      throw new RangeError('runtime game transport exceeds 512 MiB');
    const manifest: RuntimeGameManifest = {
      id: raw.manifest.id,
      hardwareRevision: raw.manifest.hardwareRevision,
      updateRate: raw.manifest.updateRate,
      display: raw.manifest.display,
      assets: assets.declarations,
    };
    const symbolNames = Object.fromEntries(
      Object.entries(raw.symbolNames).map(([key, value]) => [key, String(value)]),
    );
    const encodedFiles = Object.fromEntries(
      Object.entries(raw.files).map(([key, value]) => [key, String(value)]),
    );
    metadata.set(alias, {
      javascript: raw.javascript,
      sourceMap: raw.sourceMap,
      symbolNames,
      manifest,
      files: encodedFiles,
      rom: String(raw.rom),
    });
    members.push({
      alias,
      id: manifest.id,
      factory: () => {
        throw new Error('compiled module has not been admitted');
      },
      configuration: {
        updateRate: manifest.updateRate,
        workUnitsPerFrame: HARDWARE.workUnitsPerFrame,
        hardwareRevision: manifest.hardwareRevision,
        assets,
        rom,
      },
    });
  }
  const definition: GameDefinition = {
    ...options,
    identity: payload.identity,
    debug: payload.debug,
    members,
  };
  // Includes unused-member cross references/capacities, duplicate save IDs, seed and initial saves.
  // No placeholder factory is ever invoked; failure cannot partially import or boot a game.
  new GameCatalog(definition);
  const compiled: GameMemberDefinition[] = [];
  for (const member of members) {
    const data = metadata.get(member.alias);
    if (data === undefined) throw new TypeError('missing admitted runtime metadata');
    const module: unknown = await import(
      `data:text/javascript;base64,${Buffer.from(data.javascript).toString('base64')}`
    );
    if (!record(module) || typeof module.default !== 'function')
      throw new TypeError(`compiled runtime member '${member.alias}' has no factory`);
    compiled.push({ ...member, factory: module.default as CartridgeFactory });
  }
  return { runtime: createGameRuntime({ ...definition, members: compiled }), metadata };
}

/** Canonical base64 only: Buffer's permissive decoder must not silently repair host data. */
export function decodeRuntimeBytes(value: unknown, maximum: number): Uint8Array {
  if (
    typeof value !== 'string' ||
    value.length > Math.ceil(maximum / 3) * 4 ||
    value.length % 4 !== 0
  )
    throw new TypeError('invalid bounded runtime byte transport');
  const bytes = Buffer.from(value, 'base64');
  if (bytes.byteLength > maximum || bytes.toString('base64') !== value)
    throw new TypeError('noncanonical runtime byte transport');
  return Uint8Array.from(bytes);
}

function validateSourceMap(source: string): void {
  const map: unknown = JSON.parse(source);
  if (
    !record(map) ||
    !exact(map, ['version', 'file', 'sources', 'sourcesContent', 'names', 'mappings']) ||
    map.version !== 3 ||
    map.file !== 'cartridge.js' ||
    typeof map.mappings !== 'string' ||
    !strings(map.sources) ||
    map.sources.length < 1 ||
    map.sources.length > 4096 ||
    !strings(map.sourcesContent) ||
    map.sourcesContent.length !== map.sources.length ||
    !strings(map.names)
  )
    throw new TypeError('invalid compiler source map');
}
function strings(value: unknown): value is string[] {
  return (
    Array.isArray(value) &&
    value.every((item: unknown) => typeof item === 'string' && item.isWellFormed())
  );
}
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function exact(value: object, keys: readonly string[]): boolean {
  return (
    Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key))
  );
}
