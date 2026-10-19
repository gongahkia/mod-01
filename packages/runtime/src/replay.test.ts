import { describe, expect, it } from 'vitest';

import { emptyInputFrame } from './input';
import {
  decodeReplayTrace,
  encodeReplayTrace,
  replayInputFrames,
  REPLAY_MAX_BYTES,
} from './replay';

describe('M01REC revision 1', () => {
  it('round-trips exact bounded controller and pointer frames', () => {
    const input = emptyInputFrame();
    (input.controllers[1].buttons as Record<string, boolean>).a = true;
    const selected = {
      ...input,
      pointer: { x: 17, y: 23, primary: true, secondary: false, inside: true },
    };
    const bytes = encodeReplayTrace([{ frame: 2, input: selected }]);
    expect(encodeReplayTrace([{ frame: 2, input: selected }])).toEqual(bytes);
    const trace = decodeReplayTrace(bytes);
    expect(replayInputFrames(trace).get(2)).toEqual(selected);
    const compact = decodeReplayTrace(
      encodeReplayTrace([
        { frame: 0, input: selected },
        { frame: 1, input: selected },
      ]),
    );
    expect(compact.frames).toEqual([
      {
        frame: 0,
        duration: 2,
        controllers: [{ port: 2, buttons: ['a'] }],
        pointer: selected.pointer,
      },
    ]);
  });

  it('rejects oversize, overlapping, duplicate-port, and invalid-pointer traces', () => {
    expect(() => decodeReplayTrace(new Uint8Array(REPLAY_MAX_BYTES + 1))).toThrow(/length/);
    const decode = (frames: unknown[]): void => {
      decodeReplayTrace(new TextEncoder().encode(JSON.stringify({ revision: 1, frames })));
    };
    expect(() => {
      decode([
        { frame: 2, duration: 2, controllers: [] },
        { frame: 3, controllers: [] },
      ]);
    }).toThrow(/schema/);
    expect(() => {
      decode([
        {
          frame: 0,
          controllers: [
            { port: 1, buttons: [] },
            { port: 1, buttons: [] },
          ],
        },
      ]);
    }).toThrow(/schema/);
    expect(() => {
      decode([
        {
          frame: 0,
          controllers: [],
          pointer: { x: 240, y: 0, primary: false, secondary: false, inside: false },
        },
      ]);
    }).toThrow(/schema/);
  });

  it('rejects obsolete or unversioned replay envelopes', () => {
    const legacy = new TextEncoder().encode(
      JSON.stringify({
        revision: 0,
        frames: [{ frame: 0, controllers: [{ port: 1, buttons: ['a'] }] }],
      }),
    );
    expect(() => decodeReplayTrace(legacy)).toThrow(/schema/);
    expect(() =>
      decodeReplayTrace(
        new TextEncoder().encode(
          JSON.stringify({
            frames: [{ frame: 0, controllers: [] }],
          }),
        ),
      ),
    ).toThrow(/schema/);
  });
});
