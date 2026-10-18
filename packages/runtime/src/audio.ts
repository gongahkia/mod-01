import { HARDWARE } from './hardware';
import { MEMORY, type MemoryRegion } from './bus';
import { RuntimeFault } from './errors';
import type { ConsoleCommand, SourceSpan } from './protocol';

export type Waveform = 'pulse' | 'triangle' | 'saw' | 'noise' | 'wavetable';

export interface VolumeEnvelope {
  readonly attackFrames: number;
  readonly decayFrames: number;
  readonly sustainLevel: number;
  readonly releaseFrames: number;
}

export interface PitchEffect {
  readonly slideSemitonesPerFrame: number;
  readonly vibratoDepthSemitones: number;
  readonly vibratoPeriodFrames: number;
}

export interface SoundAsset {
  readonly kind: 'sound';
  readonly name: string;
  readonly waveform: Waveform;
  readonly note: number;
  readonly durationFrames: number;
  readonly volume: number;
  readonly pan: number;
  readonly duty?: number;
  readonly wavetable?: readonly number[];
  readonly envelope: VolumeEnvelope;
  readonly pitch: PitchEffect;
}

export interface TrackerCell {
  readonly note: number;
  readonly sound: string;
  readonly volume?: number;
}

export interface TrackerPattern {
  readonly rows: readonly (readonly (TrackerCell | null)[])[];
}

export interface MusicAsset {
  readonly kind: 'music';
  readonly name: string;
  readonly framesPerRow: number;
  readonly order: readonly string[];
  readonly patterns: Readonly<Record<string, TrackerPattern>>;
  readonly loop: boolean;
}

export type AudioAsset = SoundAsset | MusicAsset;

interface Voice {
  active: boolean;
  slot: number;
  sound: string;
  note: number;
  volumeScale: number;
  ageFrames: number;
  phase: number;
  noiseState: number;
  sequence: number;
}

interface TrackerState {
  music: string;
  orderIndex: number;
  row: number;
  frameInRow: number;
}

export interface AudioFrame {
  readonly left: Float32Array;
  readonly right: Float32Array;
  readonly activeVoices: number;
  readonly tracker: TrackerInspection | null;
}

export interface TrackerInspection {
  readonly music: string;
  readonly orderIndex: number;
  readonly row: number;
  readonly frameInRow: number;
}

export interface SynthSnapshot {
  readonly revision: 1;
  readonly frame: number;
  readonly nextSequence: number;
  readonly voices: readonly Voice[];
  readonly tracker: TrackerState | null;
}

export function isSynthSnapshot(value: unknown): value is SynthSnapshot {
  if (
    !audioRecord(value) ||
    Object.keys(value).length !== 5 ||
    value.revision !== 1 ||
    !audioCounter(value.frame) ||
    !audioCounter(value.nextSequence) ||
    value.nextSequence < 1 ||
    !denseAudioArray(value.voices) ||
    value.voices.length !== HARDWARE.audioVoices
  )
    return false;
  const nextSequence = value.nextSequence;
  if (!value.voices.every((voice, slot) => isVoice(voice, slot, nextSequence))) return false;
  return (
    value.tracker === null ||
    (audioRecord(value.tracker) &&
      Object.keys(value.tracker).length === 4 &&
      typeof value.tracker.music === 'string' &&
      value.tracker.music.length > 0 &&
      value.tracker.music.length <= HARDWARE.cartridgeCapacityBytes &&
      audioCounter(value.tracker.orderIndex) &&
      audioCounter(value.tracker.row) &&
      audioCounter(value.tracker.frameInRow))
  );
}

function isVoice(voice: unknown, slot: number, nextSequence: number): voice is Voice {
  return (
    audioRecord(voice) &&
    Object.keys(voice).length === 9 &&
    typeof voice.active === 'boolean' &&
    voice.slot === slot &&
    typeof voice.sound === 'string' &&
    voice.sound.length <= HARDWARE.cartridgeCapacityBytes &&
    audioNumber(voice.note, 0, 127) &&
    audioNumber(voice.volumeScale, 0, 1) &&
    audioCounter(voice.ageFrames) &&
    audioNumber(voice.phase, 0, 1) &&
    voice.phase < 1 &&
    audioCounter(voice.noiseState) &&
    voice.noiseState > 0 &&
    voice.noiseState <= 0xffffffff &&
    audioCounter(voice.sequence) &&
    voice.sequence < nextSequence
  );
}

export function isAudioFrame(value: unknown): value is AudioFrame {
  const samples = HARDWARE.audioSampleRate / HARDWARE.frameRate;
  return (
    audioRecord(value) &&
    Object.keys(value).length === 4 &&
    value.left instanceof Float32Array &&
    value.left.length === samples &&
    value.right instanceof Float32Array &&
    value.right.length === samples &&
    value.left.every((sample) => audioNumber(sample, -1, 1)) &&
    value.right.every((sample) => audioNumber(sample, -1, 1)) &&
    audioCounter(value.activeVoices) &&
    value.activeVoices <= HARDWARE.audioVoices &&
    (value.tracker === null ||
      (audioRecord(value.tracker) &&
        Object.keys(value.tracker).length === 4 &&
        typeof value.tracker.music === 'string' &&
        value.tracker.music.length > 0 &&
        value.tracker.music.length <= HARDWARE.cartridgeCapacityBytes &&
        audioCounter(value.tracker.orderIndex) &&
        audioCounter(value.tracker.row) &&
        audioCounter(value.tracker.frameInRow)))
  );
}

function audioCounter(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

function audioNumber(value: unknown, minimum: number, maximum: number): value is number {
  return (
    typeof value === 'number' && Number.isFinite(value) && value >= minimum && value <= maximum
  );
}

function audioRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Validated oscillator and tracker data. Arbitrary PCM samples are intentionally unrepresentable. */
export class AudioAssetStore {
  private readonly entries = new Map<string, AudioAsset>();
  private readonly ordered: readonly AudioAsset[];
  private readonly ids = new Map<string, number>();
  private readonly descriptors: Uint8Array;

  public constructor(assets: readonly AudioAsset[] = []) {
    for (const asset of assets) {
      validateAudioAsset(asset);
      if (this.entries.has(asset.name)) {
        throw new TypeError(`duplicate audio asset '${asset.name}'`);
      }
      this.entries.set(asset.name, structuredClone(asset));
    }
    for (const asset of this.entries.values()) {
      if (asset.kind !== 'music') {
        continue;
      }
      for (const pattern of Object.values(asset.patterns)) {
        for (const row of pattern.rows) {
          for (const cell of row) {
            if (cell !== null && this.entries.get(cell.sound)?.kind !== 'sound') {
              throw new TypeError(`music '${asset.name}' references missing sound '${cell.sound}'`);
            }
          }
        }
      }
    }
    this.ordered = [...this.entries.values()].sort((left, right) =>
      left.name < right.name ? -1 : left.name > right.name ? 1 : 0,
    );
    this.descriptors = new Uint8Array(this.ordered.length * MEMORY.audioAssetStride);
    const view = new DataView(this.descriptors.buffer);
    for (const [id, asset] of this.ordered.entries()) {
      this.ids.set(asset.name, id);
      const fields =
        asset.kind === 'sound'
          ? [
              1,
              ['pulse', 'triangle', 'saw', 'noise', 'wavetable'].indexOf(asset.waveform) + 1,
              asset.note,
              asset.durationFrames,
              asset.envelope.releaseFrames,
              0,
              0,
              0,
            ]
          : [2, 0, 0, 0, 0, asset.framesPerRow, asset.order.length, Number(asset.loop)];
      for (const [field, value] of fields.entries())
        view.setUint32(id * MEMORY.audioAssetStride + field * 4, value, true);
    }
  }

  public get(name: string): AudioAsset | undefined {
    return this.entries.get(name);
  }

  public id(name: string): number {
    return this.ids.get(name) ?? -1;
  }
  public byId(id: number): AudioAsset | undefined {
    return this.ordered[id];
  }
  public get count(): number {
    return this.ordered.length;
  }
  public memoryRegions(): readonly MemoryRegion[] {
    return this.descriptors.length === 0
      ? []
      : [
          {
            name: 'audio asset descriptors',
            address: MEMORY.audioAssets,
            bytes: this.descriptors,
            writable: false,
          },
        ];
  }
}

/** Eight-voice deterministic synthesizer and eight-channel order/pattern tracker. */
export class Synthesizer {
  private readonly assets: AudioAssetStore;
  private readonly voices: Voice[];
  private frame = 0;
  private nextSequence = 1;
  private tracker: TrackerState | null = null;

  public constructor(assets = new AudioAssetStore()) {
    this.assets = assets;
    this.voices = Array.from({ length: HARDWARE.audioVoices }, (_, slot) => emptyVoice(slot));
  }

  public executeFrame(commands: readonly ConsoleCommand[]): AudioFrame {
    for (const command of commands) {
      this.executeCommand(command);
    }
    return this.finishFrame();
  }

  public finishFrame(): AudioFrame {
    if (this.frame === Number.MAX_SAFE_INTEGER)
      throw new RuntimeFault('M019012', 'audio-frame counter is exhausted', { start: 0, end: 0 });
    this.advanceTracker();
    const sampleCount = HARDWARE.audioSampleRate / HARDWARE.frameRate;
    const left = new Float32Array(sampleCount);
    const right = new Float32Array(sampleCount);
    for (const voice of this.voices) {
      if (voice.active) {
        this.mixVoice(voice, left, right);
      }
    }
    for (const voice of this.voices) {
      if (voice.active) {
        voice.ageFrames += 1;
        const sound = this.assets.get(voice.sound);
        if (
          sound?.kind !== 'sound' ||
          voice.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames
        ) {
          voice.active = false;
        }
      }
    }
    const inspection = this.inspectTracker();
    this.frame += 1;
    return {
      left,
      right,
      activeVoices: this.voices.filter((voice) => voice.active).length,
      tracker: inspection,
    };
  }

  public snapshot(): SynthSnapshot {
    return structuredClone({
      revision: 1,
      frame: this.frame,
      nextSequence: this.nextSequence,
      voices: this.voices,
      tracker: this.tracker,
    });
  }

  public restore(snapshot: unknown): void {
    if (!isSynthSnapshot(snapshot)) {
      throw new TypeError('invalid MOD-01 synthesizer snapshot');
    }
    for (const voice of snapshot.voices) {
      const sound = this.assets.get(voice.sound);
      if ((voice.active || voice.sound !== '') && sound?.kind !== 'sound')
        throw new TypeError('snapshot references a missing sound');
      if (
        voice.active &&
        sound?.kind === 'sound' &&
        voice.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames
      )
        throw new TypeError('snapshot contains a voice beyond its sound lifetime');
    }
    if (snapshot.tracker !== null) {
      const tracker = snapshot.tracker;
      const music = this.assets.get(tracker.music);
      const patternName = music?.kind === 'music' ? music.order[tracker.orderIndex] : undefined;
      if (
        music?.kind !== 'music' ||
        patternName === undefined ||
        music.patterns[patternName]?.rows[tracker.row] === undefined ||
        tracker.frameInRow >= music.framesPerRow
      )
        throw new TypeError('snapshot references an invalid tracker position');
    }
    this.frame = snapshot.frame;
    this.nextSequence = snapshot.nextSequence;
    for (let index = 0; index < this.voices.length; index += 1) {
      Object.assign(this.voices[index] as Voice, structuredClone(snapshot.voices[index]));
    }
    this.tracker = structuredClone(snapshot.tracker);
  }

  public inspectVoices(): readonly Readonly<Voice>[] {
    return structuredClone(this.voices);
  }

  public inspectTracker(): TrackerInspection | null {
    return this.tracker === null ? null : { ...this.tracker };
  }

  public memoryRegions(): readonly MemoryRegion[] {
    return [
      ...this.assets.memoryRegions(),
      {
        name: 'synth status',
        address: MEMORY.audio,
        length: 32,
        writable: false,
        readByte: (offset) => this.statusByte(offset),
      },
      {
        name: 'tracker controls',
        address: MEMORY.audioTracker,
        length: 16,
        writable: true,
        readByte: (offset) => this.trackerByte(offset),
        prepareWrite: (offset, bytes) => this.prepareTrackerWrite(offset, bytes),
      },
      ...this.voices.flatMap((voice): MemoryRegion[] => [
        {
          name: `voice ${String(voice.slot)} controls`,
          address: MEMORY.audioVoices + voice.slot * MEMORY.voiceStride,
          length: 48,
          writable: true,
          readByte: (offset) => this.voiceByte(voice, offset),
          prepareWrite: (offset, bytes) => this.prepareVoiceWrite(voice, offset, bytes),
        },
        {
          name: `voice ${String(voice.slot)} allocation`,
          address: MEMORY.audioVoices + voice.slot * MEMORY.voiceStride + 48,
          length: 16,
          writable: false,
          readByte: (offset) => (offset < 8 ? integerByte(voice.sequence, offset) : 0),
        },
      ]),
    ];
  }

  private statusByte(offset: number): number {
    if (offset < 8) return integerByte(this.frame, offset);
    if (offset < 16) return integerByte(this.nextSequence, offset - 8);
    if (offset === 16) return this.voices.filter((voice) => voice.active).length;
    if (offset === 17) return HARDWARE.audioVoices;
    if (offset === 18) return HARDWARE.trackerChannels;
    if (offset === 19) return Number(this.tracker !== null);
    if (offset < 24) return integerByte(HARDWARE.audioSampleRate, offset - 20);
    if (offset < 28) return integerByte(this.assets.count, offset - 24);
    return integerByte(MEMORY.audioAssets, offset - 28);
  }

  private trackerByte(offset: number): number {
    if (this.tracker === null) return 0;
    const values = [
      this.assets.id(this.tracker.music) + 1,
      this.tracker.orderIndex,
      this.tracker.row,
      this.tracker.frameInRow,
    ];
    return integerByte(values[Math.floor(offset / 4)] ?? 0, offset % 4);
  }

  private prepareTrackerWrite(offset: number, bytes: Uint8Array): (() => void) | undefined {
    const staged = Uint8Array.from({ length: 16 }, (_, index) => this.trackerByte(index));
    staged.set(bytes, offset);
    const view = new DataView(staged.buffer);
    const id = view.getUint32(0, true);
    if (id === 0)
      return () => {
        this.tracker = null;
      };
    const music = this.assets.byId(id - 1);
    if (music?.kind !== 'music') return undefined;
    const next = {
      music: music.name,
      orderIndex: view.getUint32(4, true),
      row: view.getUint32(8, true),
      frameInRow: view.getUint32(12, true),
    };
    const patternName = music.order[next.orderIndex];
    if (
      patternName === undefined ||
      music.patterns[patternName]?.rows[next.row] === undefined ||
      next.frameInRow >= music.framesPerRow
    )
      return undefined;
    return () => {
      this.tracker = next;
    };
  }

  private voiceByte(voice: Voice, offset: number): number {
    if (offset === 0) return Number(voice.active);
    if (offset < 4) return 0;
    if (offset < 8) return integerByte(this.assets.id(voice.sound) + 1, offset - 4);
    if (offset < 16) return floatByte(voice.note, offset - 8);
    if (offset < 24) return floatByte(voice.volumeScale, offset - 16);
    if (offset < 32) return integerByte(voice.ageFrames, offset - 24);
    if (offset < 40) return floatByte(voice.phase, offset - 32);
    if (offset < 44) return integerByte(voice.noiseState, offset - 40);
    return 0;
  }

  private prepareVoiceWrite(
    voice: Voice,
    offset: number,
    bytes: Uint8Array,
  ): (() => void) | undefined {
    const staged = Uint8Array.from({ length: 48 }, (_, index) => this.voiceByte(voice, index));
    staged.set(bytes, offset);
    if (staged[0] !== 0 && staged[0] !== 1) return undefined;
    if ([1, 2, 3, 44, 45, 46, 47].some((index) => staged[index] !== 0)) return undefined;
    const view = new DataView(staged.buffer);
    const id = view.getUint32(4, true);
    const sound = this.assets.byId(id - 1);
    if (id !== 0 && sound?.kind !== 'sound') return undefined;
    const next: Voice = {
      ...voice,
      active: staged[0] === 1,
      sound: sound?.name ?? '',
      note: view.getFloat64(8, true),
      volumeScale: view.getFloat64(16, true),
      ageFrames: Number(view.getBigUint64(24, true)),
      phase: view.getFloat64(32, true),
      noiseState: view.getUint32(40, true),
    };
    if (
      !isVoice(next, voice.slot, this.nextSequence) ||
      (next.active &&
        (sound?.kind !== 'sound' ||
          next.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames))
    )
      return undefined;
    return () => {
      Object.assign(voice, next);
    };
  }

  public executeCommand(command: ConsoleCommand): void {
    switch (command.name) {
      case 'sfx': {
        const [handle] = expectArguments(command, 1);
        this.trigger(readAssetName(handle, 'Sound'), undefined, 1, command.sourceSpan);
        return;
      }
      case 'music': {
        const [handle] = expectArguments(command, 1);
        const name = readAssetName(handle, 'Music');
        const music = this.assets.get(name);
        if (music?.kind !== 'music') {
          throw new TypeError(`missing Music asset '${name}'`);
        }
        this.tracker = { music: name, orderIndex: 0, row: 0, frameInRow: 0 };
        return;
      }
      case 'music_stop':
        expectArguments(command, 0);
        this.tracker = null;
        return;
      default:
        throw new TypeError(`unknown audio command '${command.name}'`);
    }
  }

  private trigger(
    soundName: string,
    noteOverride?: number,
    volumeScale = 1,
    sourceSpan: SourceSpan = { start: 0, end: 0 },
  ): void {
    const sound = this.assets.get(soundName);
    if (sound?.kind !== 'sound') {
      throw new TypeError(`missing Sound asset '${soundName}'`);
    }
    if (this.nextSequence === Number.MAX_SAFE_INTEGER)
      throw new RuntimeFault('M019012', 'voice-allocation counter is exhausted', sourceSpan);
    const voice =
      this.voices.find((candidate) => !candidate.active) ??
      this.voices.reduce((oldest, candidate) =>
        candidate.sequence < oldest.sequence ? candidate : oldest,
      );
    Object.assign(voice, {
      active: true,
      sound: soundName,
      note: noteOverride ?? sound.note,
      volumeScale,
      ageFrames: 0,
      phase: 0,
      noiseState: nonZeroNoiseSeed(this.frame, voice.slot, this.nextSequence),
      sequence: this.nextSequence,
    });
    this.nextSequence += 1;
  }

  private advanceTracker(): void {
    const tracker = this.tracker;
    if (tracker === null) {
      return;
    }
    const music = this.assets.get(tracker.music);
    if (music?.kind !== 'music') {
      this.tracker = null;
      return;
    }
    if (tracker.frameInRow === 0) {
      const patternName = music.order[tracker.orderIndex];
      const pattern = patternName === undefined ? undefined : music.patterns[patternName];
      const row = pattern?.rows[tracker.row];
      row?.forEach((cell) => {
        if (cell !== null) {
          this.trigger(cell.sound, cell.note, cell.volume ?? 1);
        }
      });
    }
    tracker.frameInRow += 1;
    if (tracker.frameInRow < music.framesPerRow) {
      return;
    }
    tracker.frameInRow = 0;
    tracker.row += 1;
    const patternName = music.order[tracker.orderIndex];
    const pattern = patternName === undefined ? undefined : music.patterns[patternName];
    if (pattern !== undefined && tracker.row < pattern.rows.length) {
      return;
    }
    tracker.row = 0;
    tracker.orderIndex += 1;
    if (tracker.orderIndex >= music.order.length) {
      if (music.loop) {
        tracker.orderIndex = 0;
      } else {
        this.tracker = null;
      }
    }
  }

  private mixVoice(voice: Voice, left: Float32Array, right: Float32Array): void {
    const sound = this.assets.get(voice.sound);
    if (sound?.kind !== 'sound') {
      voice.active = false;
      return;
    }
    const frameEnvelope = envelopeAt(sound, voice.ageFrames);
    const pan = Math.max(-1, Math.min(1, sound.pan));
    const leftGain = (1 - pan) / 2;
    const rightGain = (1 + pan) / 2;
    const note =
      voice.note +
      sound.pitch.slideSemitonesPerFrame * voice.ageFrames +
      triangleLfo(voice.ageFrames, sound.pitch.vibratoPeriodFrames) *
        sound.pitch.vibratoDepthSemitones;
    const phaseStep = noteFrequency(note) / HARDWARE.audioSampleRate;
    for (let index = 0; index < left.length; index += 1) {
      const oscillator = oscillatorValue(sound, voice);
      const sample = oscillator * frameEnvelope * sound.volume * voice.volumeScale * 0.24;
      left[index] = clampSample((left[index] ?? 0) + sample * leftGain);
      right[index] = clampSample((right[index] ?? 0) + sample * rightGain);
      voice.phase = (voice.phase + phaseStep) % 1;
    }
  }
}

function emptyVoice(slot: number): Voice {
  return {
    active: false,
    slot,
    sound: '',
    note: 0,
    volumeScale: 1,
    ageFrames: 0,
    phase: 0,
    noiseState: 1,
    sequence: 0,
  };
}

function integerByte(value: number, offset: number): number {
  return Math.floor(value / 2 ** (offset * 8)) & 255;
}

function floatByte(value: number, offset: number): number {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value, true);
  return view.getUint8(offset);
}

function validateAudioAsset(asset: unknown): asserts asset is AudioAsset {
  if (!audioRecord(asset) || typeof asset.name !== 'string' || asset.name.length === 0) {
    throw new TypeError('audio assets require a nonempty name');
  }
  if (asset.kind === 'sound') {
    if (
      typeof asset.waveform !== 'string' ||
      !['pulse', 'triangle', 'saw', 'noise', 'wavetable'].includes(asset.waveform) ||
      !audioInteger(asset.note, 0, 127) ||
      !audioInteger(asset.durationFrames, 1, 3600) ||
      !audioNumber(asset.volume, 0, 1) ||
      !audioNumber(asset.pan, -1, 1) ||
      !validEnvelope(asset.envelope) ||
      !validPitch(asset.pitch, asset.durationFrames + asset.envelope.releaseFrames)
    ) {
      throw new RangeError(`sound '${asset.name}' is outside MOD-01 limits`);
    }
    if (
      asset.waveform === 'pulse' &&
      (!audioNumber(asset.duty, 0, 1) || asset.duty === 0 || asset.duty === 1)
    ) {
      throw new RangeError(`pulse sound '${asset.name}' requires a duty between zero and one`);
    }
    if (
      asset.waveform === 'wavetable' &&
      (!denseAudioArray(asset.wavetable) ||
        asset.wavetable.length < 4 ||
        asset.wavetable.length > 32 ||
        asset.wavetable.some((sample) => !audioNumber(sample, -1, 1)))
    ) {
      throw new RangeError(`wavetable sound '${asset.name}' requires 4-32 normalized entries`);
    }
    return;
  }
  if (
    asset.kind !== 'music' ||
    typeof asset.loop !== 'boolean' ||
    !audioInteger(asset.framesPerRow, 1, 240) ||
    !denseAudioArray(asset.order) ||
    asset.order.length === 0 ||
    !audioRecord(asset.patterns) ||
    asset.order.some(
      (name) => typeof name !== 'string' || !Object.hasOwn(asset.patterns as object, name),
    )
  ) {
    throw new RangeError(`music '${asset.name}' has an invalid order list or tempo`);
  }
  for (const pattern of Object.values(asset.patterns)) {
    if (
      !audioRecord(pattern) ||
      !denseAudioArray(pattern.rows) ||
      pattern.rows.length === 0 ||
      pattern.rows.length > 256
    ) {
      throw new RangeError(`music '${asset.name}' has an invalid pattern length`);
    }
    for (const row of pattern.rows) {
      if (!denseAudioArray(row) || row.length !== HARDWARE.trackerChannels) {
        throw new RangeError(`music '${asset.name}' patterns must have eight channels`);
      }
      for (const cell of row) {
        if (
          cell !== null &&
          (!audioRecord(cell) ||
            !audioInteger(cell.note, 0, 127) ||
            typeof cell.sound !== 'string' ||
            cell.sound.length === 0 ||
            (cell.volume !== undefined && !audioNumber(cell.volume, 0, 1)))
        ) {
          throw new RangeError(`music '${asset.name}' contains an invalid note`);
        }
      }
    }
  }
}

function denseAudioArray(value: unknown): value is unknown[] {
  return (
    Array.isArray(value) &&
    Object.keys(value).length === value.length &&
    Array.from(value.keys()).every((index) => Object.hasOwn(value, index))
  );
}

function audioInteger(value: unknown, minimum: number, maximum: number): value is number {
  return audioNumber(value, minimum, maximum) && Number.isSafeInteger(value);
}

function validEnvelope(envelope: unknown): envelope is VolumeEnvelope {
  return (
    audioRecord(envelope) &&
    [envelope.attackFrames, envelope.decayFrames, envelope.releaseFrames].every((value) =>
      audioInteger(value, 0, 3600),
    ) &&
    audioNumber(envelope.sustainLevel, 0, 1)
  );
}

function validPitch(pitch: unknown, lifetime: number): pitch is PitchEffect {
  if (
    !audioRecord(pitch) ||
    typeof pitch.slideSemitonesPerFrame !== 'number' ||
    !Number.isFinite(pitch.slideSemitonesPerFrame) ||
    typeof pitch.vibratoDepthSemitones !== 'number' ||
    !Number.isFinite(pitch.vibratoDepthSemitones) ||
    !audioInteger(pitch.vibratoPeriodFrames, 0, 3600)
  )
    return false;
  // include every tracker note and the full voice lifetime, not only the patch's default note.
  const excursion =
    Math.abs(pitch.slideSemitonesPerFrame) * lifetime + Math.abs(pitch.vibratoDepthSemitones);
  return Number.isFinite(excursion) && Number.isFinite(noteFrequency(127 + excursion));
}

function oscillatorValue(sound: SoundAsset, voice: Voice): number {
  switch (sound.waveform) {
    case 'pulse':
      return voice.phase < (sound.duty ?? 0.5) ? 1 : -1;
    case 'triangle':
      return 1 - 4 * Math.abs(voice.phase - 0.5);
    case 'saw':
      return voice.phase * 2 - 1;
    case 'noise':
      voice.noiseState = xorshift(voice.noiseState);
      return (voice.noiseState & 0xffff) / 0x7fff - 1;
    case 'wavetable': {
      const table = sound.wavetable ?? [0];
      return table[Math.floor(voice.phase * table.length) % table.length] ?? 0;
    }
  }
}

function envelopeAt(sound: SoundAsset, age: number): number {
  const envelope = sound.envelope;
  if (envelope.attackFrames > 0 && age < envelope.attackFrames) {
    return age / envelope.attackFrames;
  }
  const decayAge = age - envelope.attackFrames;
  if (envelope.decayFrames > 0 && decayAge < envelope.decayFrames) {
    return 1 - (1 - envelope.sustainLevel) * (decayAge / envelope.decayFrames);
  }
  if (age < sound.durationFrames) {
    return envelope.sustainLevel;
  }
  if (envelope.releaseFrames === 0) {
    return 0;
  }
  return (
    envelope.sustainLevel * Math.max(0, 1 - (age - sound.durationFrames) / envelope.releaseFrames)
  );
}

function triangleLfo(frame: number, period: number): number {
  if (period === 0) {
    return 0;
  }
  const phase = (frame % period) / period;
  return 1 - 4 * Math.abs(phase - 0.5);
}

function noteFrequency(note: number): number {
  return 440 * 2 ** ((note - 69) / 12);
}

function xorshift(state: number): number {
  let value = state >>> 0;
  value ^= value << 13;
  value ^= value >>> 17;
  value ^= value << 5;
  return value >>> 0;
}

function nonZeroNoiseSeed(frame: number, slot: number, sequence: number): number {
  return ((frame + 1) * 0x9e3779b1 + (slot + 1) * 0x85ebca6b + sequence) >>> 0 || 1;
}

function clampSample(sample: number): number {
  return Math.max(-1, Math.min(1, sample));
}

function expectArguments(command: ConsoleCommand, count: 0): [];
function expectArguments(command: ConsoleCommand, count: 1): [unknown];
function expectArguments(command: ConsoleCommand, count: number): unknown[] {
  if (command.arguments.length !== count) {
    throw new TypeError(
      `${command.name} expected ${String(count)} arguments, received ${String(command.arguments.length)}`,
    );
  }
  return [...command.arguments];
}

function readAssetName(value: unknown, kind: 'Sound' | 'Music'): string {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('name' in value) ||
    typeof value.name !== 'string' ||
    !('kind' in value) ||
    value.kind !== kind
  ) {
    throw new TypeError(`expected a ${kind} asset handle`);
  }
  return value.name;
}
