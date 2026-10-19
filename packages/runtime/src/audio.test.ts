import { describe, expect, it } from 'vitest';

import {
  AudioAssetStore,
  Synthesizer,
  isSynthSnapshot,
  type MusicAsset,
  type SoundAsset,
  type TrackerCell,
  type Waveform,
} from './audio';
import { HARDWARE } from './hardware';
import { MEMORY, MemoryBus } from './bus';
import type { ConsoleCommand } from './protocol';

const sourceSpan = { start: 0, end: 1 };

function sound(name: string, waveform: Waveform = 'pulse'): SoundAsset {
  return {
    kind: 'sound',
    name,
    waveform,
    note: 69,
    durationFrames: 4,
    volume: 1,
    pan: 0,
    ...(waveform === 'pulse' ? { duty: 0.5 } : {}),
    ...(waveform === 'wavetable' ? { wavetable: [-1, -0.25, 0.25, 1] } : {}),
    envelope: { attackFrames: 0, decayFrames: 0, sustainLevel: 1, releaseFrames: 1 },
    pitch: {
      slideSemitonesPerFrame: 0,
      vibratoDepthSemitones: 0,
      vibratoPeriodFrames: 0,
    },
  };
}

function command(name: string, arguments_: readonly unknown[]): ConsoleCommand {
  return { name, arguments: arguments_, sourceSpan };
}

function handle(name: string, kind: 'Sound' | 'Music'): object {
  return { name, kind };
}

describe('MOD-01 synthesizer and tracker', () => {
  it('seeks through the same validated order/row state as the MMIO sequencer', () => {
    const music: MusicAsset = {
      kind: 'music',
      name: 'song',
      framesPerRow: 2,
      loop: false,
      order: ['a', 'b'],
      patterns: {
        a: { rows: [[null, null, null, null, null, null, null, null]] },
        b: { rows: [[{ note: 67, sound: 'tone' }, null, null, null, null, null, null, null]] },
      },
    };
    const store = new AudioAssetStore([sound('tone', 'noise'), music]);
    const synth = new Synthesizer(store),
      raw = new Synthesizer(store);
    synth.playMusic('song', 1, 0);
    const bus = new MemoryBus(raw.memoryRegions(), () => {});
    const bytes = new Uint8Array(16),
      view = new DataView(bytes.buffer);
    view.setUint32(0, store.id('song') + 1, true);
    view.setUint32(4, 1, true);
    bus.edit(MEMORY.audioTracker, bytes);
    expect(synth.inspectTracker()).toEqual(raw.inspectTracker());
    expect(synth.finishFrame()).toEqual(raw.finishFrame());
    const before = synth.snapshot();
    for (const [order, row] of [
      [-1, 0],
      [2, 0],
      [0, 1],
      [0.5, 0],
      [0, Infinity],
    ]) {
      expect(() => {
        synth.playMusic('song', order ?? 0, row ?? 0);
      }).toThrow();
      expect(synth.snapshot()).toEqual(before);
    }
    expect(() => {
      synth.playMusic('absent');
    }).toThrow(/Music/);
    expect(synth.snapshot()).toEqual(before);
    synth.playMusic('song', 0, 0);
    expect(synth.snapshot().voices).toEqual(before.voices);
    expect(synth.snapshot().frame).toBe(before.frame);
  });

  it('auditions each oscillator through exactly the tracker note/volume/PCM path', () => {
    for (const waveform of ['pulse', 'triangle', 'saw', 'noise', 'wavetable'] as const) {
      const patch = sound('tone', waveform);
      const audition = new Synthesizer(new AudioAssetStore([patch]));
      audition.audition('tone', 64, 0.4);
      const music: MusicAsset = {
        kind: 'music',
        name: 'test',
        framesPerRow: 1,
        loop: false,
        order: ['p'],
        patterns: {
          p: {
            rows: [
              [{ sound: 'tone', note: 64, volume: 0.4 }, null, null, null, null, null, null, null],
            ],
          },
        },
      };
      const tracker = new Synthesizer(new AudioAssetStore([patch, music]));
      const expected = tracker.executeFrame([command('music', [handle('test', 'Music')])]);
      expect(audition.finishFrame()).toEqual(expected);
      expect(audition.inspectVoices()).toEqual(tracker.inspectVoices());
    }
  });

  it('audition validates before mutation and shares all eight voices and oldest-voice stealing', () => {
    const synth = new Synthesizer(new AudioAssetStore([sound('tone')]));
    const before = synth.snapshot();
    for (const [note, volume] of [
      [-1, 1],
      [128, 1],
      [60.5, 1],
      [NaN, 1],
      [60, -1],
      [60, 2],
      [60, Infinity],
    ]) {
      expect(() => {
        synth.audition('tone', note ?? 0, volume ?? 1);
      }).toThrow();
      expect(synth.snapshot()).toEqual(before);
    }
    expect(() => {
      synth.audition('absent', 60);
    }).toThrow(/Sound/);
    expect(synth.snapshot()).toEqual(before);
    for (let note = 60; note <= 68; note++) synth.audition('tone', note);
    expect(synth.inspectVoices().map((voice) => voice.note)).toEqual([
      68, 61, 62, 63, 64, 65, 66, 67,
    ]);
    expect(synth.finishFrame().activeVoices).toBe(8);
  });

  it('faults at exact audio counters before mutating voices or advancing samples', () => {
    const synth = new Synthesizer(new AudioAssetStore([sound('tone')]));
    const initial = synth.snapshot();
    synth.restore({ ...initial, nextSequence: Number.MAX_SAFE_INTEGER });
    const beforeTrigger = synth.snapshot();
    expect(() => {
      synth.executeCommand(command('sfx', [handle('tone', 'Sound')]));
    }).toThrow(expect.objectContaining({ code: 'M019012', sourceSpan }));
    expect(synth.snapshot()).toEqual(beforeTrigger);
    synth.restore({ ...initial, frame: Number.MAX_SAFE_INTEGER });
    const beforeFrame = synth.snapshot();
    expect(() => synth.finishFrame()).toThrow(expect.objectContaining({ code: 'M019012' }));
    expect(synth.snapshot()).toEqual(beforeFrame);
  });

  it('shares all eight live voices and their exact floating-point controls with writable MMIO', () => {
    const store = new AudioAssetStore([sound('tone')]);
    const synth = new Synthesizer(store);
    const ram = new Uint8Array(64);
    const bus = new MemoryBus(
      [{ name: 'RAM', address: 0, bytes: ram, writable: true }, ...synth.memoryRegions()],
      () => {},
    );
    expect(bus.read(MEMORY.audio + 17, 2, sourceSpan)).toBe(0x0808);
    expect(bus.read(MEMORY.audio + 20, 2, sourceSpan)).toBe(48_000);
    expect(bus.read(MEMORY.audio + 24, 2, sourceSpan)).toBe(1);
    expect(bus.read(MEMORY.audioAssets, 2, sourceSpan)).toBe(1);
    expect(bus.read(MEMORY.audioAssets + 8, 2, sourceSpan)).toBe(69);
    for (let slot = 0; slot < 8; slot += 1) {
      const address = MEMORY.audioVoices + slot * MEMORY.voiceStride;
      expect(bus.read(address, 1, sourceSpan)).toBe(0);
      synth.executeCommand(command('sfx', [handle('tone', 'Sound')]));
      expect(bus.read(address, 1, sourceSpan)).toBe(1);
      expect(bus.read(address + 4, 2, sourceSpan)).toBe(1);
      expect(bus.read(address + 48, 2, sourceSpan)).toBe(slot + 1);
      bus.copy(0, address + 8, 8, sourceSpan);
      expect(new DataView(ram.buffer).getFloat64(0, true)).toBe(69);
      new DataView(ram.buffer).setFloat64(0, 0.25, true);
      bus.copy(address + 32, 0, 8, sourceSpan);
      bus.fill(address + 16, 0, 8, sourceSpan);
    }
    expect(bus.read(MEMORY.audio + 16, 1, sourceSpan)).toBe(8);
    expect(bus.read(MEMORY.audio + 8, 2, sourceSpan)).toBe(9);
    expect(
      synth.inspectVoices().every((voice) => voice.volumeScale === 0 && voice.phase === 0.25),
    ).toBe(true);
    const checkpoint = synth.snapshot();
    const expected = synth.finishFrame();
    expect(expected.left.every((sample) => sample === 0)).toBe(true);
    expect(expected.right.every((sample) => sample === 0)).toBe(true);
    expect(bus.read(MEMORY.audio, 2, sourceSpan)).toBe(1);
    expect(bus.read(MEMORY.audioVoices + 24, 2, sourceSpan)).toBe(1);
    synth.restore(checkpoint);
    expect(bus.read(MEMORY.audio, 2, sourceSpan)).toBe(0);
    expect(bus.read(MEMORY.audioVoices + 24, 2, sourceSpan)).toBe(0);
    expect(synth.finishFrame()).toEqual(expected);
    expect(bus.snapshot().regions).toHaveLength(1);
  });

  it('rejects invalid voice encodings and cross-permission writes before changing synth state', () => {
    const synth = new Synthesizer(new AudioAssetStore([sound('tone')]));
    synth.executeCommand(command('sfx', [handle('tone', 'Sound')]));
    const ram = new Uint8Array(64);
    const bus = new MemoryBus(
      [{ name: 'RAM', address: 0, bytes: ram, writable: true }, ...synth.memoryRegions()],
      () => {},
    );
    const before = synth.snapshot();
    const base = MEMORY.audioVoices;
    for (const [offset, value] of [
      [0, 2],
      [1, 1],
      [4, 0],
      [4, 99],
      [44, 1],
    ]) {
      expect(() => {
        bus.write(base + (offset ?? 0), value ?? 0, 1, sourceSpan);
      }).toThrow(expect.objectContaining({ code: 'M019022' }));
      expect(synth.snapshot()).toEqual(before);
    }
    for (const [offset, value] of [
      [8, NaN],
      [8, 128],
      [16, Infinity],
      [16, -1],
      [32, 1],
    ]) {
      new DataView(ram.buffer).setFloat64(0, value ?? 0, true);
      expect(() => {
        bus.copy(base + (offset ?? 0), 0, 8, sourceSpan);
      }).toThrow(expect.objectContaining({ code: 'M019022' }));
      expect(synth.snapshot()).toEqual(before);
    }
    new DataView(ram.buffer).setBigUint64(0, BigInt(Number.MAX_SAFE_INTEGER), true);
    expect(() => {
      bus.copy(base + 24, 0, 8, sourceSpan);
    }).toThrow(expect.objectContaining({ code: 'M019022' }));
    expect(() => {
      bus.fill(base + 40, 0, 4, sourceSpan);
    }).toThrow(expect.objectContaining({ code: 'M019022' }));
    expect(() => {
      bus.fill(base + 44, 0, 8, sourceSpan);
    }).toThrow(expect.objectContaining({ code: 'M019021' }));
    expect(() => {
      bus.write(base, 0, 1, sourceSpan, true);
    }).toThrow(expect.objectContaining({ code: 'M019011' }));
    expect(synth.snapshot()).toEqual(before);
    bus.write(base, 0, 1, sourceSpan);
    expect(synth.inspectVoices()[0]?.active).toBe(false);
    bus.write(base, 1, 1, sourceSpan);
    expect(synth.inspectVoices()[0]?.active).toBe(true);
  });

  it('shares tracker position/start/stop with raw controls and exposes stable sorted asset IDs', () => {
    const row = [
      { note: 72, sound: 'tone', volume: 0.5 },
      null,
      null,
      null,
      null,
      null,
      null,
      null,
    ];
    const music: MusicAsset = {
      kind: 'music',
      name: 'song',
      framesPerRow: 2,
      order: ['p'],
      patterns: { p: { rows: [row, row] } },
      loop: true,
    };
    const assets = new AudioAssetStore([sound('tone'), music]);
    expect(assets.id('song')).toBe(0);
    expect(assets.id('tone')).toBe(1);
    expect(assets.id('absent')).toBe(-1);
    const high = new Synthesizer(assets);
    const low = new Synthesizer(assets);
    const bus = new MemoryBus(low.memoryRegions(), () => {});
    expect(bus.read(MEMORY.audioAssets, 2, sourceSpan)).toBe(2);
    expect(bus.read(MEMORY.audioAssets + 20, 2, sourceSpan)).toBe(2);
    bus.write(MEMORY.audioTracker, assets.id('song') + 1, 2, sourceSpan);
    high.executeCommand(command('music', [handle('song', 'Music')]));
    expect(low.snapshot()).toEqual(high.snapshot());
    expect(low.finishFrame()).toEqual(high.finishFrame());
    expect(bus.read(MEMORY.audioTracker + 12, 2, sourceSpan)).toBe(1);
    bus.write(MEMORY.audioTracker + 8, 1, 2, sourceSpan);
    expect(low.inspectTracker()).toMatchObject({ row: 1, frameInRow: 1 });
    const before = low.snapshot();
    for (const [offset, value] of [
      [0, 2],
      [4, 1],
      [8, 2],
      [12, 2],
    ]) {
      expect(() => {
        bus.write(MEMORY.audioTracker + (offset ?? 0), value ?? 0, 2, sourceSpan);
      }).toThrow(expect.objectContaining({ code: 'M019022' }));
      expect(low.snapshot()).toEqual(before);
    }
    bus.write(MEMORY.audioTracker, 0, 2, sourceSpan);
    expect(low.inspectTracker()).toBeNull();
    expect(bus.read(MEMORY.audioTracker + 8, 2, sourceSpan)).toBe(0);
    low.executeCommand(command('music', [handle('song', 'Music')]));
    expect(bus.read(MEMORY.audioTracker, 2, sourceSpan)).toBe(1);
    low.executeCommand(command('music_stop', []));
    expect(bus.read(MEMORY.audioTracker, 2, sourceSpan)).toBe(0);
  });

  it('rejects unsupported oscillators and non-finite or missing numeric patch data before rendering', () => {
    const patch = sound('invalid');
    const invalid: unknown[] = [
      { ...patch, waveform: 'sample' },
      { ...patch, volume: NaN },
      { ...patch, volume: undefined },
      { ...patch, pan: NaN },
      { ...patch, duty: NaN },
      { ...patch, pitch: { ...patch.pitch, slideSemitonesPerFrame: 1e308 } },
      { ...patch, pitch: { ...patch.pitch, vibratoDepthSemitones: 1e308 } },
    ];
    for (const value of invalid) expect(() => new AudioAssetStore([value as SoundAsset])).toThrow();
  });

  it('rejects sparse voice snapshots and active voices beyond the patch lifetime transactionally', () => {
    const synth = new Synthesizer(new AudioAssetStore([sound('tone')]));
    synth.executeCommand(command('sfx', [handle('tone', 'Sound')]));
    const before = synth.snapshot();
    expect(isSynthSnapshot({ ...before, voices: Array(8) })).toBe(false);
    const disguised = [...before.voices];
    Reflect.deleteProperty(disguised, '0');
    Object.assign(disguised, { extra: before.voices[0] });
    expect(isSynthSnapshot({ ...before, voices: disguised })).toBe(false);
    expect(() => {
      synth.restore({
        ...before,
        voices: before.voices.map((voice, index) =>
          index === 0 ? { ...voice, ageFrames: Number.MAX_SAFE_INTEGER } : voice,
        ),
      });
    }).toThrow(/voice/);
    expect(synth.snapshot()).toEqual(before);
  });

  it('validates tracker structure and finite cells before following instrument references', () => {
    const row = [{ note: 60, sound: 'tone' }, null, null, null, null, null, null, null];
    const music = {
      kind: 'music',
      name: 'song',
      framesPerRow: 1,
      order: ['p'],
      patterns: { p: { rows: [row] } },
      loop: true,
    };
    const invalid: unknown[] = [
      { ...music, loop: 1 },
      { ...music, order: Array(1) },
      { ...music, order: [null], patterns: { null: { rows: [row] } } },
      { ...music, patterns: { p: { rows: Array(1) } } },
      { ...music, patterns: { p: { rows: [Array(8)] } } },
      {
        ...music,
        patterns: { p: { rows: [[{ note: 60, sound: 'tone', volume: NaN }, ...row.slice(1)]] } },
      },
    ];
    for (const value of invalid)
      expect(() => new AudioAssetStore([sound('tone'), value as MusicAsset])).toThrow();
  });

  it('owns a validated copy of patches and renders extreme admitted pitch without non-finite state', () => {
    const patch = sound('tone');
    const store = new AudioAssetStore([patch]);
    Object.assign(patch, { volume: NaN });
    const owned = new Synthesizer(store);
    expect(
      owned.executeFrame([command('sfx', [handle('tone', 'Sound')])]).left.every(Number.isFinite),
    ).toBe(true);
    const extreme = {
      ...sound('extreme'),
      pitch: { slideSemitonesPerFrame: 1000, vibratoDepthSemitones: 1000, vibratoPeriodFrames: 2 },
    };
    const synth = new Synthesizer(new AudioAssetStore([extreme]));
    synth.executeCommand(command('sfx', [handle('extreme', 'Sound')]));
    for (let frame = 0; frame < 6; frame += 1) {
      const output = synth.finishFrame();
      expect(output.left.every(Number.isFinite)).toBe(true);
      expect(output.right.every(Number.isFinite)).toBe(true);
      expect(isSynthSnapshot(synth.snapshot())).toBe(true);
    }
  });

  it('applies voice commands immediately but advances samples and age only on frame completion', () => {
    const assets = new AudioAssetStore([sound('tone')]);
    const synthesizer = new Synthesizer(assets);
    const trigger = command('sfx', [handle('tone', 'Sound')]);
    synthesizer.executeCommand(trigger);
    expect(synthesizer.inspectVoices()[0]).toMatchObject({ active: true, ageFrames: 0, phase: 0 });
    expect(synthesizer.snapshot().frame).toBe(0);
    expect(synthesizer.finishFrame()).toEqual(new Synthesizer(assets).executeFrame([trigger]));
    expect(synthesizer.inspectVoices()[0]?.ageFrames).toBe(1);
    expect(synthesizer.snapshot().frame).toBe(1);
  });

  it('generates every oscillator source without arbitrary sample assets', () => {
    for (const waveform of ['pulse', 'triangle', 'saw', 'noise', 'wavetable'] as const) {
      const patch = sound(waveform, waveform);
      const synthesizer = new Synthesizer(new AudioAssetStore([patch]));
      const frame = synthesizer.executeFrame([command('sfx', [handle(patch.name, 'Sound')])]);
      expect(frame.left).toHaveLength(HARDWARE.audioSampleRate / HARDWARE.frameRate);
      expect(frame.left.some((sample) => sample !== 0)).toBe(true);
      expect(frame.left).toEqual(frame.right);
      expect(frame.activeVoices).toBe(1);
    }
  });

  it('caps output at eight voices with deterministic oldest-voice stealing', () => {
    const patch = sound('tone');
    const synthesizer = new Synthesizer(new AudioAssetStore([patch]));
    synthesizer.executeFrame(
      Array.from({ length: HARDWARE.audioVoices + 1 }, () =>
        command('sfx', [handle('tone', 'Sound')]),
      ),
    );
    const sequences = synthesizer
      .inspectVoices()
      .map((voice) => voice.sequence)
      .sort((left, right) => left - right);
    expect(sequences).toEqual([2, 3, 4, 5, 6, 7, 8, 9]);
  });

  it('plays an eight-channel pattern/order list and stops non-looping music', () => {
    const patch = sound('lead');
    const emptyRow: (TrackerCell | null)[] = Array.from(
      { length: HARDWARE.trackerChannels },
      () => null,
    );
    const noteRow = [...emptyRow];
    noteRow[0] = { note: 72, sound: 'lead', volume: 0.5 };
    const music: MusicAsset = {
      kind: 'music',
      name: 'theme',
      framesPerRow: 2,
      order: ['intro'],
      patterns: { intro: { rows: [noteRow] } },
      loop: false,
    };
    const synthesizer = new Synthesizer(new AudioAssetStore([patch, music]));
    const first = synthesizer.executeFrame([command('music', [handle('theme', 'Music')])]);
    expect(first.tracker).toMatchObject({ orderIndex: 0, row: 0, frameInRow: 1 });
    const second = synthesizer.executeFrame([]);
    expect(second.tracker).toBeNull();
    expect(synthesizer.inspectVoices().some((voice) => voice.note === 72)).toBe(true);
  });

  it('restores oscillator, noise, voice-allocation, and tracker state exactly', () => {
    const patch = sound('noise', 'noise');
    const synthesizer = new Synthesizer(new AudioAssetStore([patch]));
    synthesizer.executeFrame([command('sfx', [handle('noise', 'Sound')])]);
    const snapshot = synthesizer.snapshot();
    const expected = synthesizer.executeFrame([]);
    synthesizer.restore(snapshot);
    const replayed = synthesizer.executeFrame([]);
    expect(replayed.activeVoices).toBe(expected.activeVoices);
    expect(replayed.left).toEqual(expected.left);
    expect(replayed.right).toEqual(expected.right);
  });

  it('rejects invalid patches, tracker channel counts, and missing instruments', () => {
    expect(() => new AudioAssetStore([{ ...sound('bad'), note: 128 }])).toThrow(/limits/);
    expect(
      () =>
        new AudioAssetStore([
          {
            kind: 'music',
            name: 'bad-song',
            framesPerRow: 1,
            order: ['p'],
            patterns: { p: { rows: [[{ note: 60, sound: 'missing' }]] } },
            loop: false,
          },
        ]),
    ).toThrow(/eight channels/);
  });
});
