import { createHash } from 'node:crypto';

import type { ConsoleFrame } from './console-runtime';
import type { GameSnapshot } from './game-runtime';

/** Shared CLI/native evidence encoding, not a restorable checkpoint format. */
export function pcmBytes(frame: ConsoleFrame): Uint8Array {
  const samples = frame.output.audio.left.length;
  const bytes = new Uint8Array(samples * 8);
  const view = new DataView(bytes.buffer);
  for (let index = 0; index < samples; index += 1) {
    view.setFloat32(index * 8, frame.output.audio.left[index] ?? 0, true);
    view.setFloat32(index * 8 + 4, frame.output.audio.right[index] ?? 0, true);
  }
  return bytes;
}

export function canonicalBytes(value: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(canonicalValue(value)));
}

// Identity and compiler mode bind restorable checkpoints, not game behaviour. Keeping
// them in a separate snapshot hash lets source-embedded tests assert state without
// requiring their own expected hash to be a fixed point of the enclosing game hash.
export function stateBytes(snapshot: GameSnapshot): Uint8Array {
  return canonicalBytes(
    Object.fromEntries(
      Object.entries(snapshot).filter(([key]) => key !== 'identity' && key !== 'debug'),
    ),
  );
}

export function saveHashes(snapshot: GameSnapshot): Readonly<Record<string, string>> {
  return Object.fromEntries(
    Object.entries(snapshot.saves).map(([id, image]) => [id, sha256(image)]),
  );
}

function canonicalValue(value: unknown): unknown {
  // Explicit byte tags avoid huge JS number arrays and host typed-array JSON or
  // native Float32 byte-order conventions. Keep this identical for both hosts.
  if (value instanceof Uint8Array)
    return {
      $bytes: Buffer.from(value.buffer, value.byteOffset, value.byteLength).toString('hex'),
    };
  if (value instanceof Float32Array) {
    const bytes = new Uint8Array(value.length * 4);
    const view = new DataView(bytes.buffer);
    value.forEach((sample, index) => {
      view.setFloat32(index * 4, sample, true);
    });
    return { $float32le: Buffer.from(bytes).toString('hex') };
  }
  if (Array.isArray(value)) return value.map(canonicalValue);
  if (typeof value === 'object' && value !== null)
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
        .map(([key, item]) => [key, canonicalValue(item)]),
    );
  return value;
}

export function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}
