import { HARDWARE } from './hardware';
import { MEMORY, type MemoryRegion } from './bus';
import { RuntimeFault } from './errors';
import type { SourceSpan } from './protocol';

export type SaveValues = Readonly<Record<string, number>>;
export type SaveImage = SaveValues | Uint8Array;

export interface SaveSnapshot {
  readonly revision: 1;
  readonly bytes: Uint8Array;
  readonly committed: Uint8Array;
  readonly pendingCommit: boolean;
  readonly commits: number;
}

export interface SaveWrite {
  readonly key: string;
  readonly value: number;
}

/** One byte image, with a commit latch delivered to the trusted host after a successful frame. */
export class SaveMemory {
  private readonly bytes = new Uint8Array(HARDWARE.saveCapacityBytes);
  private readonly committed = new Uint8Array(HARDWARE.saveCapacityBytes);
  private values: SaveValues | undefined;
  private pendingCommit = false;
  private commits = 0;
  private dirtyBytes = 0;
  private readonly writes = new Map<string, number>();

  public constructor(initial: SaveImage = {}) {
    if (!isSaveImage(initial)) {
      throw new TypeError('invalid MOD-01 save image');
    }
    this.bytes.set(initial instanceof Uint8Array ? initial : encodeValues(initial));
    this.committed.set(this.bytes);
  }

  public get(key: string, fallback: number): number {
    validateKey(key);
    validateInteger(fallback);
    const values = this.readValues();
    return Object.hasOwn(values, key) ? (values[key] ?? fallback) : fallback;
  }

  public set(key: string, value: number): void {
    validateKey(key);
    validateInteger(value);
    this.checkCommitCounter();
    const next = { ...this.readValues(), [key]: value };
    const bytes = encodeValues(next);
    this.bytes.fill(0);
    this.bytes.set(bytes);
    this.values = next;
    this.latch();
    for (const [pendingKey, pendingValue] of this.writes)
      if (!Object.hasOwn(next, pendingKey) || next[pendingKey] !== pendingValue)
        this.writes.delete(pendingKey);
    this.writes.set(key, value);
  }

  public snapshot(): SaveValues {
    return sortedValues(this.readValues());
  }

  public deviceSnapshot(): SaveSnapshot {
    return {
      revision: 1,
      bytes: this.bytes.slice(),
      committed: this.committed.slice(),
      pendingCommit: this.pendingCommit,
      commits: this.commits,
    };
  }

  public restoreDevice(value: unknown, pendingWrites: readonly SaveWrite[] = []): void {
    if (!isSaveSnapshot(value) || !isPendingDeviceWrites(pendingWrites, value))
      throw new TypeError('invalid MOD-01 save snapshot');
    this.bytes.set(value.bytes);
    this.committed.set(value.committed);
    this.values = undefined;
    this.pendingCommit = value.pendingCommit;
    this.commits = value.commits;
    this.dirtyBytes = this.bytes.reduce(
      (count, byte, index) => count + Number(byte !== this.committed[index]),
      0,
    );
    this.writes.clear();
    for (const write of pendingWrites) this.writes.set(write.key, write.value);
  }

  public commit(): void {
    this.checkCommitCounter();
    this.latch();
    this.writes.clear();
  }

  public takeCommit(): Uint8Array | undefined {
    if (!this.pendingCommit) return undefined;
    this.pendingCommit = false;
    this.writes.clear();
    return this.committed.slice();
  }

  public restore(value: unknown, pendingWrites: readonly SaveWrite[] = []): void {
    if (!isSaveValues(value) || !isPendingSaveWrites(pendingWrites, value)) {
      throw new TypeError('invalid MOD-01 save snapshot');
    }
    const initial = new SaveMemory(value).deviceSnapshot();
    this.restoreDevice({ ...initial, pendingCommit: pendingWrites.length > 0 }, pendingWrites);
  }

  public pendingWrites(): readonly SaveWrite[] {
    return [...this.writes]
      .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
      .map(([key, value]) => ({ key, value }));
  }

  public takeWrites(): readonly SaveWrite[] {
    const writes = this.pendingWrites();
    this.writes.clear();
    return writes;
  }

  public memoryRegions(charge: (units: number, span: SourceSpan) => void): readonly MemoryRegion[] {
    return [
      {
        name: 'save working bytes',
        address: MEMORY.save,
        length: this.bytes.length,
        writable: true,
        readByte: (offset) => this.bytes[offset] ?? 0,
        prepareWrite: (offset, bytes) => () => {
          for (let index = 0; index < bytes.length; index += 1) {
            const cursor = offset + index;
            this.dirtyBytes +=
              Number(bytes[index] !== this.committed[cursor]) -
              Number(this.bytes[cursor] !== this.committed[cursor]);
          }
          this.bytes.set(bytes, offset);
          this.values = undefined;
        },
      },
      {
        name: 'save committed latch',
        address: MEMORY.saveCommitted,
        bytes: this.committed,
        writable: false,
      },
      {
        name: 'save commit command',
        address: MEMORY.saveControl,
        length: 1,
        writable: true,
        readByte: () => 0,
        prepareWrite: (_offset, bytes, span, debugEdit) => {
          if (bytes[0] === 0) return () => undefined;
          if (bytes[0] !== 1) return undefined;
          try {
            this.checkCommitCounter();
          } catch (error) {
            throw new RuntimeFault(
              'M019012',
              error instanceof Error ? error.message : 'save counter exhausted',
              span,
            );
          }
          if (debugEdit !== true) charge(HARDWARE.saveCapacityBytes, span);
          return () => {
            this.latch();
            this.writes.clear();
          };
        },
      },
      {
        name: 'save status',
        address: MEMORY.saveControl + 1,
        length: 31,
        writable: false,
        readByte: (offset) => {
          const status = new DataView(new ArrayBuffer(32));
          status.setUint8(1, (this.dirtyBytes > 0 ? 1 : 0) | (this.pendingCommit ? 2 : 0));
          status.setUint32(4, HARDWARE.saveCapacityBytes, true);
          status.setBigUint64(8, BigInt(this.commits), true);
          status.setUint32(16, this.dirtyBytes, true);
          return status.getUint8(offset + 1);
        },
      },
    ];
  }

  private readValues(): SaveValues {
    this.values ??= decodeSaveValues(this.bytes);
    return this.values;
  }

  private checkCommitCounter(): void {
    if (this.commits === Number.MAX_SAFE_INTEGER)
      throw new RangeError('save commit counter exhausted');
  }

  private latch(): void {
    this.committed.set(this.bytes);
    this.pendingCommit = true;
    this.dirtyBytes = 0;
    this.commits += 1;
  }
}

export function isSaveImage(value: unknown): value is SaveImage {
  return value instanceof Uint8Array
    ? value.length <= HARDWARE.saveCapacityBytes
    : isSaveValues(value);
}

export function isSaveSnapshot(value: unknown): value is SaveSnapshot {
  return (
    isRecord(value) &&
    Object.keys(value).length === 5 &&
    value.revision === 1 &&
    value.bytes instanceof Uint8Array &&
    value.bytes.length === HARDWARE.saveCapacityBytes &&
    value.committed instanceof Uint8Array &&
    value.committed.length === HARDWARE.saveCapacityBytes &&
    typeof value.pendingCommit === 'boolean' &&
    typeof value.commits === 'number' &&
    Number.isSafeInteger(value.commits) &&
    value.commits >= 0
  );
}

export function isPendingDeviceWrites(
  value: unknown,
  snapshot: SaveSnapshot,
): value is readonly SaveWrite[] {
  if (!Array.isArray(value)) return false;
  if (value.length === 0) return Object.keys(value).length === 0;
  if (!snapshot.pendingCommit) return false;
  try {
    return isPendingSaveWrites(value, decodeSaveValues(snapshot.committed));
  } catch {
    return false;
  }
}

export function decodeSaveValues(bytes: Uint8Array): SaveValues {
  if (bytes.length > HARDWARE.saveCapacityBytes)
    throw new TypeError('save image exceeds the 8 KiB capacity');
  const zero = bytes.indexOf(0);
  const end = zero === -1 ? bytes.length : zero;
  if (
    end > HARDWARE.saveCapacityBytes ||
    (zero !== -1 && bytes.subarray(zero).some((byte) => byte !== 0))
  )
    throw new TypeError('save bytes are not an integer-save image');
  if (end === 0) return {};
  let values: unknown;
  try {
    values = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, end)));
  } catch {
    throw new TypeError('save bytes are not an integer-save image');
  }
  if (!isSaveValues(values)) throw new TypeError('save bytes are not an integer-save image');
  return values;
}

export function isPendingSaveWrites(
  value: unknown,
  values: SaveValues,
): value is readonly SaveWrite[] {
  if (
    !Array.isArray(value) ||
    value.length > Object.keys(values).length ||
    Object.keys(value).length !== value.length
  )
    return false;
  const keys = new Set<string>();
  for (const write of value as unknown[]) {
    if (
      !isRecord(write) ||
      Object.keys(write).length !== 2 ||
      typeof write.key !== 'string' ||
      !Object.hasOwn(values, write.key) ||
      write.value !== values[write.key] ||
      keys.has(write.key)
    )
      return false;
    keys.add(write.key);
  }
  return true;
}

export function isSaveValues(value: unknown): value is SaveValues {
  if (!isRecord(value) || Array.isArray(value)) {
    return false;
  }
  try {
    for (const [key, entry] of Object.entries(value)) {
      validateKey(key);
      validateInteger(entry);
    }
    return encodedBytes(value as SaveValues) <= HARDWARE.saveCapacityBytes;
  } catch {
    return false;
  }
}

function sortedValues(values: SaveValues): SaveValues {
  return Object.fromEntries(
    Object.entries(values).sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0)),
  );
}

function encodedBytes(values: SaveValues): number {
  return new TextEncoder().encode(JSON.stringify(sortedValues(values))).byteLength;
}

function encodeValues(values: SaveValues): Uint8Array {
  if (Object.keys(values).length === 0) return new Uint8Array();
  const bytes = new TextEncoder().encode(JSON.stringify(sortedValues(values)));
  if (bytes.length > HARDWARE.saveCapacityBytes)
    throw new RangeError('cartridge save exceeds the 8 KiB capacity');
  return bytes;
}

function validateKey(key: string): void {
  if (
    !/^[A-Za-z_][A-Za-z0-9_.-]{0,63}$/.test(key) ||
    ['__proto__', 'constructor', 'prototype'].includes(key)
  ) {
    throw new TypeError('save keys must be 1-64 canonical ASCII characters');
  }
}

function validateInteger(value: unknown): asserts value is number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) {
    throw new TypeError('save values must be safe integers');
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
