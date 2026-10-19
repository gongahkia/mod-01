import { RuntimeFault } from './errors';
import type { SourceSpan } from './protocol';

// The sole current machine map. Each bank has one address and cannot overlap descriptor tables.
export const MEMORY = Object.freeze({
  size: 0x400000,
  ram: 0x00000,
  ramBytes: 0x10000,
  front: 0x10000,
  back: 0x19000,
  display: 0x22000,
  visual: 0x210000,
  draw: 0x50000,
  transparency: 0x50050,
  pattern: 0x50070,
  palette: 0x200000,
  input: 0x50100,
  inputBytes: 48,
  system: 0x50200,
  systemBytes: 64,
  rasterLive: 0x50400,
  visualInfo: 0x50300,
  saveControl: 0x50500,
  cartridgeInfo: 0x50600,
  save: 0x58000,
  saveCommitted: 0x5a000,
  cartridgeRom: 0x100000,
  audio: 0x51000,
  audioTracker: 0x51020,
  audioVoices: 0x51100,
  voiceStride: 64,
  audioAssets: 0x3c0000,
  audioAssetStride: 32,
  raster: 0x55000,
  rasterStride: 40,
  assets: 0xa0000,
  assetStride: 32,
  allocations: 0xc0000,
  allocationStride: 24,
} as const);

export interface ByteMemoryRegion {
  readonly name: string;
  readonly address: number;
  readonly bytes: Uint8Array;
  readonly writable: boolean;
  readonly retained?: boolean;
  readonly rasterWritable?: boolean;
  readonly validate?: (offset: number, bytes: Uint8Array) => boolean;
}

/** Read-only MMIO serializes the owner's current state without retaining a second image. */
export interface ReadOnlyMemoryRegion {
  readonly name: string;
  readonly address: number;
  readonly length: number;
  readonly writable: false;
  readonly readByte: (offset: number) => number;
}

/** Preparation may charge work but cannot mutate devices; a commit must not fail or revalidate. */
export interface WritableMemoryRegion extends Omit<ReadOnlyMemoryRegion, 'writable'> {
  readonly writable: true;
  readonly rasterWritable?: boolean;
  readonly prepareWrite: (
    offset: number,
    bytes: Uint8Array,
    span: SourceSpan,
    debugEdit?: boolean,
  ) => (() => void) | undefined;
}

export type MemoryRegion = ByteMemoryRegion | ReadOnlyMemoryRegion | WritableMemoryRegion;

export interface MemoryRegionDescriptor {
  readonly name: string;
  readonly address: number;
  readonly length: number;
  readonly writable: boolean;
}

function regionLength(region: MemoryRegion): number {
  return 'bytes' in region ? region.bytes.length : region.length;
}

export interface MemorySnapshot {
  readonly revision: 1;
  readonly regions: readonly { readonly address: number; readonly bytes: Uint8Array }[];
}

export function isMemorySnapshot(value: unknown): value is MemorySnapshot {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('revision' in value) ||
    value.revision !== 1 ||
    !('regions' in value) ||
    !Array.isArray(value.regions) ||
    Object.keys(value).length !== 2 ||
    value.regions.length > 4096
  )
    return false;
  let end = 0;
  for (const region of value.regions as unknown[]) {
    if (
      typeof region !== 'object' ||
      region === null ||
      Object.keys(region).length !== 2 ||
      !('address' in region) ||
      typeof region.address !== 'number' ||
      !Number.isSafeInteger(region.address) ||
      region.address < end ||
      !('bytes' in region) ||
      !(region.bytes instanceof Uint8Array) ||
      region.bytes.length === 0 ||
      region.bytes.length > MEMORY.size - region.address
    )
      return false;
    end = region.address + region.bytes.length;
  }
  return true;
}

/** Byte-addressed access to real device storage; reserved holes read zero and reject writes. */
export class MemoryBus {
  private readonly regions: readonly MemoryRegion[];
  private readonly charge: (units: number, span: SourceSpan) => void;

  public constructor(
    regions: readonly MemoryRegion[],
    charge: (units: number, span: SourceSpan) => void,
  ) {
    this.regions = [...regions].sort((left, right) => left.address - right.address);
    this.charge = charge;
    let end = 0;
    for (const region of this.regions) {
      const length = regionLength(region);
      if (
        !Number.isSafeInteger(region.address) ||
        region.address < end ||
        !Number.isSafeInteger(length) ||
        length <= 0 ||
        length > MEMORY.size - region.address
      )
        throw new TypeError('invalid or overlapping hardware region');
      end = region.address + length;
    }
  }

  public read(address: number, width: 1 | 2, span: SourceSpan): number {
    this.range(address, width, span);
    this.charge(width, span);
    const low = this.byte(address);
    return width === 1 ? low : low + this.byte(address + 1) * 256;
  }

  public write(
    address: number,
    value: number,
    width: 1 | 2,
    span: SourceSpan,
    raster = false,
  ): void {
    this.range(address, width, span);
    this.value(value, width === 1 ? 255 : 65535, span);
    this.charge(width, span);
    const bytes = width === 1 ? Uint8Array.of(value) : Uint8Array.of(value & 255, value >>> 8);
    this.store(address, bytes, span, raster);
  }

  public fill(
    address: number,
    value: number,
    length: number,
    span: SourceSpan,
    raster = false,
  ): void {
    this.range(address, length, span);
    this.value(value, 255, span);
    this.charge(1 + length, span);
    this.store(address, new Uint8Array(length).fill(value), span, raster);
  }

  public copy(
    destination: number,
    source: number,
    length: number,
    span: SourceSpan,
    raster = false,
  ): void {
    this.range(destination, length, span);
    this.range(source, length, span);
    this.charge(1 + 2 * length, span);
    const bytes = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) bytes[index] = this.byte(source + index);
    this.store(destination, bytes, span, raster);
  }

  /** Bounded debugger read at an idle host message boundary; does not charge cartridge work. */
  public inspect(address: number, length: number): Uint8Array {
    const span = { start: 0, end: 0 };
    this.range(address, length, span);
    const bytes = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) bytes[index] = this.byte(address + index);
    return bytes;
  }

  /** Transactional debugger edit at an idle host message boundary; does not charge work. */
  public edit(address: number, bytes: Uint8Array): void {
    const span = { start: 0, end: 0 };
    this.range(address, bytes.length, span);
    this.store(address, bytes.slice(), span, false, true);
  }

  public describe(): readonly MemoryRegionDescriptor[] {
    return this.regions.map((region) => ({
      name: region.name,
      address: region.address,
      length: regionLength(region),
      writable: region.writable,
    }));
  }

  public snapshot(): MemorySnapshot {
    return {
      revision: 1,
      regions: this.retained().map(({ address, bytes }) => ({ address, bytes: bytes.slice() })),
    };
  }

  public restore(snapshot: unknown): void {
    const retained = this.retained();
    if (!isMemorySnapshot(snapshot) || snapshot.regions.length !== retained.length)
      throw new TypeError('invalid hardware memory snapshot');
    for (let index = 0; index < retained.length; index += 1) {
      const target = retained[index];
      const source = snapshot.regions[index];
      if (
        target === undefined ||
        source === undefined ||
        target.address !== source.address ||
        target.bytes.length !== source.bytes.length ||
        target.validate?.(0, source.bytes) === false
      )
        throw new TypeError('hardware memory snapshot does not match this cartridge');
    }
    for (let index = 0; index < retained.length; index += 1)
      retained[index]?.bytes.set(snapshot.regions[index]?.bytes ?? []);
  }

  private retained(): readonly ByteMemoryRegion[] {
    return this.regions.filter(
      (region): region is ByteMemoryRegion =>
        'bytes' in region && (region.retained ?? region.writable),
    );
  }

  private byte(address: number): number {
    const region = this.regions.find(
      (entry) => address >= entry.address && address < entry.address + regionLength(entry),
    );
    if (region === undefined) return 0;
    return 'bytes' in region
      ? (region.bytes[address - region.address] ?? 0)
      : region.readByte(address - region.address);
  }

  private store(
    address: number,
    bytes: Uint8Array,
    span: SourceSpan,
    raster: boolean,
    debugEdit = false,
  ): void {
    const writes: (() => void)[] = [];
    for (let index = 0; index < bytes.length;) {
      const cursor = address + index;
      const region = this.regions.find(
        (entry) => cursor >= entry.address && cursor < entry.address + regionLength(entry),
      );
      if (region === undefined || !region.writable)
        throw new RuntimeFault('M019021', 'write to read-only or reserved hardware memory', span);
      if (raster && !region.rasterWritable)
        throw new RuntimeFault(
          'M019011',
          'hardware write is not valid in the raster callback',
          span,
        );
      const offset = cursor - region.address;
      const count = Math.min(bytes.length - index, regionLength(region) - offset);
      const part = bytes.subarray(index, index + count);
      const commit =
        'bytes' in region
          ? region.validate?.(offset, part) === false
            ? undefined
            : () => {
                region.bytes.set(part, offset);
              }
          : region.prepareWrite(offset, part, span, debugEdit);
      if (commit === undefined)
        throw new RuntimeFault(
          'M019022',
          `invalid value for hardware region '${region.name}'`,
          span,
        );
      writes.push(commit);
      index += count;
    }
    // all range, permission and value checks precede the first mutation, including cross-region writes.
    for (const write of writes) write();
  }

  private range(address: number, length: number, span: SourceSpan): void {
    if (
      !Number.isSafeInteger(address) ||
      !Number.isSafeInteger(length) ||
      address < 0 ||
      length < 0 ||
      address > MEMORY.size ||
      length > MEMORY.size - address
    )
      throw new RuntimeFault('M019020', 'hardware address or range is out of bounds', span);
  }

  private value(value: number, maximum: number, span: SourceSpan): void {
    if (!Number.isSafeInteger(value) || value < 0 || value > maximum)
      throw new RuntimeFault(
        'M019022',
        'hardware value is outside the unsigned operation width',
        span,
      );
  }
}
