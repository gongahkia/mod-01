import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { emptyInputFrame, type Button, type PointerState } from './input';
import {
  decodeReplayTrace,
  encodeReplayTrace,
  replayInputFrames,
  REPLAY_MAX_BYTES,
} from './replay';

describe('M01REC revision 1', () => {
  const corpus = JSON.parse(
    readFileSync(new URL('../../../tests/conformance/replays.json', import.meta.url), 'utf8'),
  ) as {
    name: string;
    json: string;
    expanded?: {
      frame: number;
      ports?: Record<string, Button[]>;
      pointer?: PointerState;
      menuAction?: number;
    }[];
  }[];

  it.each(corpus)('agrees with the shared Rust/CLI corpus: $name', (item) => {
    const bytes = new TextEncoder().encode(item.json);
    if (item.expanded === undefined) {
      expect(() => decodeReplayTrace(bytes)).toThrow();
      return;
    }
    const expected = new Map(
      item.expanded.map((frame) => {
        const input = emptyInputFrame();
        for (const [port, buttons] of Object.entries(frame.ports ?? {})) {
          const target = input.controllers[Number(port) - 1];
          if (target === undefined) throw new Error('invalid conformance fixture');
          for (const button of buttons) (target.buttons as Record<Button, boolean>)[button] = true;
        }
        return [
          frame.frame,
          {
            ...input,
            ...(frame.pointer === undefined ? {} : { pointer: frame.pointer }),
            ...(frame.menuAction === undefined ? {} : { menuAction: frame.menuAction }),
          },
        ] as const;
      }),
    );
    expect(replayInputFrames(decodeReplayTrace(bytes))).toEqual(expected);
    expect(
      replayInputFrames(
        decodeReplayTrace(
          encodeReplayTrace([...expected].map(([frame, input]) => ({ frame, input }))),
        ),
      ),
    ).toEqual(expected);
  });

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
