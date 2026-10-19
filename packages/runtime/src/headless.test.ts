import { describe, expect, it } from 'vitest';

import { runHeadless, type HeadlessRequest } from './headless';

const javascript = `export default function createCartridge(api) {
  let frame = 0;
  return {
    start() {},
    update() { frame += 1; },
    draw() { api.call('pixel', [frame, 0, 7], { start: 10, end: 20 }); },
    raster() {},
    snapshot() { return { state: { frame }, tasks: [], nextTaskId: 1 }; },
    restore(value) { frame = value.state.frame; },
    inspect() { return { state: { frame }, tasks: [], callStack: [] }; }
  };
}`;

function request(overrides: Partial<HeadlessRequest> = {}): HeadlessRequest {
  return {
    revision: 1,
    javascript,
    manifest: { id: 'headless-test', updateRate: 60, display: null, assets: {} },
    entries: {},
    rom: [77, 79, 68, 45, 48, 49, 26, 2, 0, 0, 0, 0],
    seed: 7,
    frames: 3,
    trace: { revision: 1, frames: [] },
    save: [],
    ...overrides,
  };
}

describe('production headless host', () => {
  it('emits byte-stable framebuffer, state, command, PCM and save hashes', async () => {
    const first = await runHeadless(request());
    const second = await runHeadless(request());
    expect(second).toEqual(first);
    expect(first.frames).toHaveLength(3);
    expect(new Set(first.frames.map((frame) => frame.framebufferSha256)).size).toBe(3);
    expect(first.summary).toMatchObject({
      completedFrames: 3,
      workPeak: 1,
      commandPeak: 1,
      drawCommandPeak: 1,
      audioCommandPeak: 0,
      voicePeak: 0,
    });
    expect(first.summary.busMappedBytes).toBeGreaterThan(128 * 1024);
    for (const hash of [
      first.summary.finalFramebufferSha256,
      first.summary.finalStateSha256,
      first.summary.audioCommandsSha256,
      first.summary.pcmSha256,
      first.summary.finalSaveSha256,
    ])
      expect(hash).toMatch(/^[0-9a-f]{64}$/);
  });

  it('reports modeled cartridge faults and rejects malformed traces before execution', async () => {
    const faulting = request({
      javascript: javascript.replace(
        'frame += 1;',
        "api.fault('M019998', 'fixture fault', { start: 4, end: 9 });",
      ),
    });
    await expect(runHeadless(faulting)).resolves.toMatchObject({
      frames: [],
      fault: {
        frame: 0,
        code: 'M019998',
        message: 'fixture fault',
        sourceSpan: { start: 4, end: 9 },
      },
    });
    await expect(
      runHeadless({
        ...request(),
        trace: { revision: 1, frames: [{ frame: 3, controllers: [] }] },
      }),
    ).rejects.toThrow(/invalid MOD-01 headless request/);
  });
});
