/* global process, console */
import { ok, strictEqual } from 'node:assert';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** A successful CLI process can still report a modeled cartridge fault in JSON. */
export function assertCompletedHeadlessTrace(trace, frames, cartridgeId) {
  ok(Number.isSafeInteger(frames) && frames >= 1 && frames <= 36_000, 'invalid smoke frame count');
  ok(typeof cartridgeId === 'string' && cartridgeId.length > 0, 'missing expected cartridge ID');
  ok(
    trace !== null && typeof trace === 'object' && !Array.isArray(trace),
    'missing headless trace',
  );
  strictEqual(trace.revision, 2, 'expected the current headless result');
  strictEqual(
    Object.hasOwn(trace, 'fault'),
    false,
    'headless smoke run reported a cartridge fault',
  );
  strictEqual(trace.cartridge?.id, cartridgeId, 'headless smoke cartridge identity differs');
  strictEqual(trace.configuration?.requestedFrames, frames, 'headless request frame count differs');
  strictEqual(trace.summary?.completedFrames, frames, 'headless smoke run ended early');
  ok(Array.isArray(trace.frames), 'headless smoke run has no frame records');
  strictEqual(trace.frames.length, frames, 'headless frame records are incomplete');
  trace.frames.forEach((frame, index) => {
    strictEqual(frame?.frame, index, 'headless frame sequence differs');
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (!process.argv[2]) throw new Error('usage: check-headless-trace FILE FRAMES CARTRIDGE_ID');
    assertCompletedHeadlessTrace(
      JSON.parse(readFileSync(process.argv[2], 'utf8')),
      Number(process.argv[3]),
      process.argv[4],
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : 'invalid headless smoke result');
    process.exitCode = 1;
  }
}
