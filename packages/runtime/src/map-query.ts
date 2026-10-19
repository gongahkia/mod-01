export interface MapQueryLayer {
  readonly width: number;
  readonly height: number;
  readonly cells: Uint16Array;
  readonly tileFlags: Uint8Array;
}

export interface MapQueryAsset {
  readonly name: string;
  readonly layers: readonly MapQueryLayer[];
}

/** Read-only map view exposed to cartridge query calls. */
export class MapQueryStore {
  private readonly maps = new Map<string, MapQueryAsset>();

  public constructor(assets: readonly MapQueryAsset[] = []) {
    for (const asset of assets) {
      if (asset.name.length === 0 || this.maps.has(asset.name) || asset.layers.length === 0) {
        throw new TypeError('map query catalog contains an invalid or duplicate map');
      }
      for (const layer of asset.layers) {
        if (
          !Number.isSafeInteger(layer.width) ||
          !Number.isSafeInteger(layer.height) ||
          layer.width <= 0 ||
          layer.height <= 0 ||
          layer.cells.length !== layer.width * layer.height ||
          layer.cells.some((tile) => tile >= layer.tileFlags.length)
        ) {
          throw new TypeError(`map query data for '${asset.name}' is incoherent`);
        }
      }
      this.maps.set(asset.name, asset);
    }
  }

  public cell(name: string, layerIndex: number, x: number, y: number): number {
    const layer = this.maps.get(name)?.layers[layerIndex];
    if (layer === undefined || x < 0 || y < 0 || x >= layer.width || y >= layer.height) {
      return -1;
    }
    return layer.cells[y * layer.width + x] ?? -1;
  }

  public flag(name: string, layerIndex: number, x: number, y: number, flagIndex: number): boolean {
    if (flagIndex < 0 || flagIndex > 7) {
      return false;
    }
    const layer = this.maps.get(name)?.layers[layerIndex];
    const tile = this.cell(name, layerIndex, x, y);
    return (
      layer !== undefined && tile >= 0 && ((layer.tileFlags[tile] ?? 0) & (1 << flagIndex)) !== 0
    );
  }
}

export function isMapQueryCatalog(value: unknown): value is readonly MapQueryAsset[] {
  return Array.isArray(value) && value.every(isMapQueryAsset);
}

function isMapQueryAsset(value: unknown): value is MapQueryAsset {
  return (
    isRecord(value) &&
    typeof value.name === 'string' &&
    value.name.length > 0 &&
    Array.isArray(value.layers) &&
    value.layers.length > 0 &&
    value.layers.every(isMapQueryLayer)
  );
}

function isMapQueryLayer(value: unknown): value is MapQueryLayer {
  if (
    !isRecord(value) ||
    !Number.isSafeInteger(value.width) ||
    typeof value.width !== 'number' ||
    value.width <= 0 ||
    !Number.isSafeInteger(value.height) ||
    typeof value.height !== 'number' ||
    value.height <= 0 ||
    !(value.cells instanceof Uint16Array) ||
    value.cells.length !== value.width * value.height ||
    !(value.tileFlags instanceof Uint8Array)
  ) {
    return false;
  }
  const tileFlags = value.tileFlags;
  return value.cells.every((tile) => tile < tileFlags.length);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
