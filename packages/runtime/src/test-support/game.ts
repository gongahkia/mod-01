import { createHash } from 'node:crypto';
import { isRuntimeAssetSource } from '../asset-codec';
import type { RuntimeGamePayload } from '../game-loader';

/** Current wire fixture builder only; production payloads must come from shared Rust admission. */
export function runtimeGameFixture(input: {
  readonly javascript: string;
  readonly manifest: {
    readonly id: string;
    readonly updateRate: number;
    readonly display: string | null;
    readonly assets: unknown;
  };
  readonly files?: Readonly<Record<string, Uint8Array | readonly number[]>>;
  readonly rom?: Uint8Array | readonly number[];
  readonly debug?: boolean;
  readonly sourceMap?: string;
  readonly symbolNames?: Readonly<Record<string, string>>;
}): RuntimeGamePayload {
  const { updateRate } = input.manifest;
  const files = Object.fromEntries(
    Object.entries(input.files ?? {}).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
  );
  const assets = {
    declarations: input.manifest.assets,
    files,
    displayPath: input.manifest.display,
  };
  if ((updateRate !== 30 && updateRate !== 60) || !isRuntimeAssetSource(assets))
    throw new TypeError('invalid current game test fixture');
  const rom = Buffer.from(input.rom ?? [1]);
  return {
    revision: 1,
    identity: createHash('sha256').update(rom).digest('hex'),
    debug: input.debug ?? false,
    members: {
      main: {
        javascript: input.javascript,
        manifest: {
          ...input.manifest,
          updateRate,
          hardwareRevision: 2,
          assets: assets.declarations,
        },
        files: Object.fromEntries(
          Object.entries(files).map(([path, bytes]) => [
            path,
            Buffer.from(bytes).toString('base64'),
          ]),
        ),
        rom: rom.toString('base64'),
        symbolNames: input.symbolNames ?? {},
        sourceMap:
          input.sourceMap ??
          JSON.stringify({
            version: 3,
            file: 'cartridge.js',
            sources: ['fixture.modl'],
            sourcesContent: [''],
            names: [],
            mappings: '',
          }),
      },
    },
  };
}
