import { describe, expect, it } from 'vitest';

import { lockDownWorkerGlobals } from './capabilities';
import { Synthesizer } from './audio';
import { HARDWARE } from './hardware';
import { emptyInputFrame } from './input';
import { isHostRequest, isWorkerResponse } from './protocol';

describe('sandbox protocol', () => {
  it('rejects a load that raises the fixed 50,000-unit work ceiling', () => {
    const load = {
      id: 1,
      type: 'load',
      moduleUrl: 'blob:https://mod01.test/module',
      configuration: { seed: 1, updateRate: 60, workUnitsPerFrame: 50_000 },
    };
    expect(isHostRequest(load)).toBe(true);
    expect(
      isHostRequest({
        ...load,
        configuration: { ...load.configuration, workUnitsPerFrame: 50_001 },
      }),
    ).toBe(false);
    expect(
      isHostRequest({
        ...load,
        configuration: { ...load.configuration, rom: Uint8Array.of(80, 88) },
      }),
    ).toBe(true);
    expect(
      isHostRequest({
        ...load,
        configuration: {
          ...load.configuration,
          rom: new Uint8Array(HARDWARE.cartridgeCapacityBytes + 1),
        },
      }),
    ).toBe(false);
  });

  it('accepts complete frame messages and denies non-blob cartridge module URLs', () => {
    expect(isHostRequest({ id: 1, type: 'frame', input: emptyInputFrame() })).toBe(true);
    expect(
      isHostRequest({
        id: 2,
        type: 'load',
        moduleUrl: 'https://example.test/cartridge.js',
        configuration: { seed: 1, workUnitsPerFrame: 100, updateRate: 60 },
      }),
    ).toBe(false);
    expect(isHostRequest({ id: 3, type: 'memory', address: 0x50000, length: 64 })).toBe(true);
    expect(isHostRequest({ id: 4, type: 'memory-edit', address: 0, bytes: Uint8Array.of(7) })).toBe(
      true,
    );
    expect(isHostRequest({ id: 5, type: 'memory', address: 0x400000, length: 1 })).toBe(false);
    expect(
      isHostRequest({ id: 6, type: 'memory-edit', address: 0, bytes: new Uint8Array(257) }),
    ).toBe(false);
  });

  it('removes ambient network, time, storage, random and dynamic-code capabilities', () => {
    const target: Record<string, unknown> = {
      Date,
      fetch: () => undefined,
      WebSocket: () => undefined,
      indexedDB: {},
      crypto: {},
      eval,
      Function,
      Math,
    };
    lockDownWorkerGlobals(target);
    for (const name of ['Date', 'fetch', 'WebSocket', 'indexedDB', 'crypto', 'eval', 'Function']) {
      expect(target[name]).toBeUndefined();
    }
    expect(target.Math).toMatchObject({ max: Math.max, trunc: Math.trunc });
    expect(target.Math).not.toHaveProperty('random');
  });

  it('rejects shallow or over-specified worker responses', () => {
    const synthesizer = new Synthesizer();
    const audio = synthesizer.finishFrame();
    const frame = {
      id: 1,
      type: 'frame',
      frame: 0,
      workUnits: 4,
      attribution: [{ sourceSpan: { start: 1, end: 2 }, units: 4 }],
      drawCommands: [
        { name: 'clear', arguments: [0], sourceSpan: { start: 1, end: 2 } },
        {
          name: 'pal',
          arguments: [1, 2],
          sourceSpan: { start: 3, end: 4 },
          rasterLine: 12,
        },
      ],
      audioCommands: [],
      saveWrites: [],
      output: {
        indexedPixels: new Uint8Array(HARDWARE.width * HARDWARE.height),
        audio,
        audioState: synthesizer.snapshot(),
      },
    };
    expect(isWorkerResponse(frame)).toBe(true);
    expect(
      isWorkerResponse({
        ...frame,
        debug: {
          truncated: false,
          trace: [
            {
              id: 0,
              sourceSpan: { source: 'src/main.modl', start: 1, end: 2 },
              locals: { s1: 4 },
              callStack: [{ name: 'update', sourceSpan: { start: 0, end: 8 } }],
            },
          ],
          inspection: { state: { s0: 4 }, tasks: [], callStack: [] },
        },
      }),
    ).toBe(true);
    expect(
      isWorkerResponse({
        ...frame,
        debug: {
          truncated: false,
          trace: [
            {
              id: 0,
              sourceSpan: { source: 'src/\u0000main.modl', start: 1, end: 2 },
              locals: {},
              callStack: [],
            },
          ],
          inspection: { state: {}, tasks: [], callStack: [] },
        },
      }),
    ).toBe(false);
    expect(isWorkerResponse({ id: 1, type: 'frame' })).toBe(false);
    expect(isWorkerResponse({ ...frame, ambient: true })).toBe(false);
    expect(isWorkerResponse({ ...frame, drawCommands: [{ name: 'clear' }] })).toBe(false);
    expect(isWorkerResponse({ ...frame, debug: { trace: [], inspection: {} } })).toBe(false);
    const memory = {
      id: 2,
      type: 'memory',
      address: 0,
      bytes: Uint8Array.of(1, 2),
      regions: [{ name: 'RAM', address: 0, length: 65_536, writable: true }],
    };
    expect(isWorkerResponse(memory)).toBe(true);
    expect(isWorkerResponse({ ...memory, bytes: new Uint8Array(257) })).toBe(false);
    expect(isWorkerResponse({ ...memory, regions: [{ ...memory.regions[0], extra: true }] })).toBe(
      false,
    );
    for (const output of [
      undefined,
      { ...frame.output, indexedPixels: new Uint8Array(1) },
      { ...frame.output, indexedPixels: new Uint16Array(HARDWARE.width * HARDWARE.height) },
      { ...frame.output, audio: { ...audio, left: new Float32Array(audio.left.length).fill(NaN) } },
      { ...frame.output, audio: { ...audio, activeVoices: 9 } },
      { ...frame.output, audio: { ...audio, extra: true } },
      { ...frame.output, audioState: { ...frame.output.audioState, voices: [] } },
      { ...frame.output, audioState: { ...frame.output.audioState, extra: true } },
    ]) {
      expect(isWorkerResponse({ ...frame, output })).toBe(false);
    }
  });
});
