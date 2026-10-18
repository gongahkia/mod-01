import { createHash } from 'node:crypto';
//#region src/hardware.ts
var HARDWARE = Object.freeze({
  width: 240,
  height: 144,
  frameRate: 60,
  paletteSize: 256,
  rasterPaletteSize: 32,
  transparentColor: 0,
  visualCapacityBytes: 1048576,
  saveCapacityBytes: 8192,
  cartridgeCapacityBytes: 1048576,
  drawCommandsPerFrame: 4096,
  workUnitsPerFrame: 5e4,
  audioVoices: 8,
  audioSampleRate: 48e3,
  trackerChannels: 8,
  controllerPorts: 4,
  spriteMaximumAxis: 128,
  tileSize: 8,
});
/** Hardware Revision 2 keeps the original 32 colours, then adds a 216-colour cube and eight greys. */
var MASTER_PALETTE = Object.freeze([
  ...[
    '#17141f',
    '#292532',
    '#403946',
    '#5d5054',
    '#806a63',
    '#aa8b74',
    '#d5b992',
    '#f4e5bd',
    '#5b2938',
    '#8b3c47',
    '#bf5558',
    '#ed7b69',
    '#5a3928',
    '#89572e',
    '#c18436',
    '#e7bd50',
    '#263c32',
    '#345f46',
    '#4b8b58',
    '#7fbd68',
    '#203b47',
    '#2e6571',
    '#43969a',
    '#75cbc0',
    '#243451',
    '#345581',
    '#4b7db3',
    '#73a9d1',
    '#3e3154',
    '#654777',
    '#936397',
    '#c38aae',
  ],
  ...Array.from({ length: 216 }, (_, index) => {
    return `#${[Math.floor(index / 36) * 51, (Math.floor(index / 6) % 6) * 51, (index % 6) * 51]
      .map((value) => value.toString(16).padStart(2, '0'))
      .join('')}`;
  }),
  ...Array.from({ length: 8 }, (_, index) => {
    const value = (index * 29 + 26).toString(16).padStart(2, '0');
    return `#${value}${value}${value}`;
  }),
]);
var MASTER_PALETTE_RGBA = Object.freeze(
  MASTER_PALETTE.flatMap((hex) => [
    Number.parseInt(hex.slice(1, 3), 16),
    Number.parseInt(hex.slice(3, 5), 16),
    Number.parseInt(hex.slice(5, 7), 16),
    255,
  ]),
);
//#endregion
//#region src/errors.ts
var RuntimeFault = class extends Error {
  code;
  sourceSpan;
  constructor(code, message, sourceSpan) {
    super(message);
    this.name = 'RuntimeFault';
    this.code = code;
    this.sourceSpan = sourceSpan;
  }
};
var BudgetExceeded = class extends RuntimeFault {
  used;
  limit;
  constructor(used, limit, sourceSpan) {
    super(
      'M019001',
      `frame used ${String(used)} synthetic work units; limit is ${String(limit)}`,
      sourceSpan,
    );
    this.name = 'BudgetExceeded';
    this.used = used;
    this.limit = limit;
  }
};
//#endregion
//#region src/bus.ts
var MEMORY = Object.freeze({
  size: 4194304,
  ram: 0,
  ramBytes: 65536,
  front: 65536,
  back: 102400,
  display: 139264,
  visual: 196608,
  visualExtended: 2162688,
  draw: 327680,
  transparency: 327760,
  palette: 327808,
  input: 327936,
  inputBytes: 48,
  system: 328192,
  systemBytes: 64,
  rasterLive: 328704,
  visualInfo: 328448,
  saveControl: 328960,
  cartridgeInfo: 329216,
  save: 360448,
  saveCommitted: 368640,
  cartridgeRom: 393216,
  cartridgeRomExtension: 1048576,
  paletteExtended: 2097152,
  audio: 331776,
  audioTracker: 331808,
  audioVoices: 332032,
  voiceStride: 64,
  audioAssets: 3932160,
  audioAssetStride: 32,
  raster: 348160,
  rasterStride: 40,
  assets: 655360,
  assetStride: 32,
  allocations: 786432,
  allocationStride: 24,
});
function regionLength(region) {
  return 'bytes' in region ? region.bytes.length : region.length;
}
function isMemorySnapshot(value) {
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
  for (const region of value.regions) {
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
var MemoryBus = class {
  regions;
  charge;
  constructor(regions, charge) {
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
  read(address, width, span) {
    this.range(address, width, span);
    this.charge(width, span);
    const low = this.byte(address);
    return width === 1 ? low : low + this.byte(address + 1) * 256;
  }
  write(address, value, width, span, raster = false) {
    this.range(address, width, span);
    this.value(value, width === 1 ? 255 : 65535, span);
    this.charge(width, span);
    const bytes = width === 1 ? Uint8Array.of(value) : Uint8Array.of(value & 255, value >>> 8);
    this.store(address, bytes, span, raster);
  }
  fill(address, value, length, span, raster = false) {
    this.range(address, length, span);
    this.value(value, 255, span);
    this.charge(1 + length, span);
    this.store(address, new Uint8Array(length).fill(value), span, raster);
  }
  copy(destination, source, length, span, raster = false) {
    this.range(destination, length, span);
    this.range(source, length, span);
    this.charge(1 + 2 * length, span);
    const bytes = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) bytes[index] = this.byte(source + index);
    this.store(destination, bytes, span, raster);
  }
  /** Bounded debugger read at an idle Worker message boundary; does not charge cartridge work. */
  inspect(address, length) {
    this.range(address, length, {
      start: 0,
      end: 0,
    });
    const bytes = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) bytes[index] = this.byte(address + index);
    return bytes;
  }
  /** Transactional debugger edit at an idle Worker message boundary; does not charge work. */
  edit(address, bytes) {
    const span = {
      start: 0,
      end: 0,
    };
    this.range(address, bytes.length, span);
    this.store(address, bytes.slice(), span, false, true);
  }
  describe() {
    return this.regions.map((region) => ({
      name: region.name,
      address: region.address,
      length: regionLength(region),
      writable: region.writable,
    }));
  }
  snapshot() {
    return {
      revision: 1,
      regions: this.retained().map(({ address, bytes }) => ({
        address,
        bytes: bytes.slice(),
      })),
    };
  }
  restore(snapshot) {
    const retained = this.retained();
    if (!isMemorySnapshot(snapshot) || snapshot.regions.length !== retained.length)
      throw new TypeError('invalid hardware memory snapshot');
    for (let index = 0; index < retained.length; index += 1) {
      const target = retained[index];
      const source = snapshot.regions[index];
      if (
        target === void 0 ||
        source === void 0 ||
        target.address !== source.address ||
        target.bytes.length !== source.bytes.length ||
        target.validate?.(0, source.bytes) === false
      )
        throw new TypeError('hardware memory snapshot does not match this cartridge');
    }
    for (let index = 0; index < retained.length; index += 1)
      retained[index]?.bytes.set(snapshot.regions[index]?.bytes ?? []);
  }
  retained() {
    return this.regions.filter(
      (region) => 'bytes' in region && (region.retained ?? region.writable),
    );
  }
  byte(address) {
    const region = this.regions.find(
      (entry) => address >= entry.address && address < entry.address + regionLength(entry),
    );
    if (region === void 0) return 0;
    return 'bytes' in region
      ? (region.bytes[address - region.address] ?? 0)
      : region.readByte(address - region.address);
  }
  store(address, bytes, span, raster, debugEdit = false) {
    const writes = [];
    for (let index = 0; index < bytes.length;) {
      const cursor = address + index;
      const region = this.regions.find(
        (entry) => cursor >= entry.address && cursor < entry.address + regionLength(entry),
      );
      if (region === void 0 || !region.writable)
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
            ? void 0
            : () => {
                region.bytes.set(part, offset);
              }
          : region.prepareWrite(offset, part, span, debugEdit);
      if (commit === void 0)
        throw new RuntimeFault(
          'M019022',
          `invalid value for hardware region '${region.name}'`,
          span,
        );
      writes.push(commit);
      index += count;
    }
    for (const write of writes) write();
  }
  range(address, length, span) {
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
  value(value, maximum, span) {
    if (!Number.isSafeInteger(value) || value < 0 || value > maximum)
      throw new RuntimeFault(
        'M019022',
        'hardware value is outside the unsigned operation width',
        span,
      );
  }
};
//#endregion
//#region src/audio.ts
function isSynthSnapshot(value) {
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
function isVoice(voice, slot, nextSequence) {
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
    voice.noiseState <= 4294967295 &&
    audioCounter(voice.sequence) &&
    voice.sequence < nextSequence
  );
}
function audioCounter(value) {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
function audioNumber(value, minimum, maximum) {
  return (
    typeof value === 'number' && Number.isFinite(value) && value >= minimum && value <= maximum
  );
}
function audioRecord(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
/** Validated oscillator and tracker data. Arbitrary PCM samples are intentionally unrepresentable. */
var AudioAssetStore = class {
  entries = /* @__PURE__ */ new Map();
  ordered;
  ids = /* @__PURE__ */ new Map();
  descriptors;
  constructor(assets = []) {
    for (const asset of assets) {
      validateAudioAsset(asset);
      if (this.entries.has(asset.name))
        throw new TypeError(`duplicate audio asset '${asset.name}'`);
      this.entries.set(asset.name, structuredClone(asset));
    }
    for (const asset of this.entries.values()) {
      if (asset.kind !== 'music') continue;
      for (const pattern of Object.values(asset.patterns))
        for (const row of pattern.rows)
          for (const cell of row)
            if (cell !== null && this.entries.get(cell.sound)?.kind !== 'sound')
              throw new TypeError(`music '${asset.name}' references missing sound '${cell.sound}'`);
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
  get(name) {
    return this.entries.get(name);
  }
  id(name) {
    return this.ids.get(name) ?? -1;
  }
  byId(id) {
    return this.ordered[id];
  }
  get count() {
    return this.ordered.length;
  }
  memoryRegions() {
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
};
/** Eight-voice deterministic synthesizer and eight-channel order/pattern tracker. */
var Synthesizer = class {
  assets;
  voices;
  frame = 0;
  nextSequence = 1;
  tracker = null;
  constructor(assets = new AudioAssetStore()) {
    this.assets = assets;
    this.voices = Array.from({ length: HARDWARE.audioVoices }, (_, slot) => emptyVoice(slot));
  }
  executeFrame(commands) {
    for (const command of commands) this.executeCommand(command);
    return this.finishFrame();
  }
  finishFrame() {
    if (this.frame === Number.MAX_SAFE_INTEGER)
      throw new RuntimeFault('M019012', 'audio-frame counter is exhausted', {
        start: 0,
        end: 0,
      });
    this.advanceTracker();
    const sampleCount = HARDWARE.audioSampleRate / HARDWARE.frameRate;
    const left = new Float32Array(sampleCount);
    const right = new Float32Array(sampleCount);
    for (const voice of this.voices) if (voice.active) this.mixVoice(voice, left, right);
    for (const voice of this.voices)
      if (voice.active) {
        voice.ageFrames += 1;
        const sound = this.assets.get(voice.sound);
        if (
          sound?.kind !== 'sound' ||
          voice.ageFrames >= sound.durationFrames + sound.envelope.releaseFrames
        )
          voice.active = false;
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
  snapshot() {
    return structuredClone({
      revision: 1,
      frame: this.frame,
      nextSequence: this.nextSequence,
      voices: this.voices,
      tracker: this.tracker,
    });
  }
  restore(snapshot) {
    if (!isSynthSnapshot(snapshot)) throw new TypeError('invalid MOD-01 synthesizer snapshot');
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
      const patternName = music?.kind === 'music' ? music.order[tracker.orderIndex] : void 0;
      if (
        music?.kind !== 'music' ||
        patternName === void 0 ||
        music.patterns[patternName]?.rows[tracker.row] === void 0 ||
        tracker.frameInRow >= music.framesPerRow
      )
        throw new TypeError('snapshot references an invalid tracker position');
    }
    this.frame = snapshot.frame;
    this.nextSequence = snapshot.nextSequence;
    for (let index = 0; index < this.voices.length; index += 1)
      Object.assign(this.voices[index], structuredClone(snapshot.voices[index]));
    this.tracker = structuredClone(snapshot.tracker);
  }
  inspectVoices() {
    return structuredClone(this.voices);
  }
  inspectTracker() {
    return this.tracker === null ? null : { ...this.tracker };
  }
  memoryRegions() {
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
      ...this.voices.flatMap((voice) => [
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
  statusByte(offset) {
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
  trackerByte(offset) {
    if (this.tracker === null) return 0;
    return integerByte(
      [
        this.assets.id(this.tracker.music) + 1,
        this.tracker.orderIndex,
        this.tracker.row,
        this.tracker.frameInRow,
      ][Math.floor(offset / 4)] ?? 0,
      offset % 4,
    );
  }
  prepareTrackerWrite(offset, bytes) {
    const staged = Uint8Array.from({ length: 16 }, (_, index) => this.trackerByte(index));
    staged.set(bytes, offset);
    const view = new DataView(staged.buffer);
    const id = view.getUint32(0, true);
    if (id === 0)
      return () => {
        this.tracker = null;
      };
    const music = this.assets.byId(id - 1);
    if (music?.kind !== 'music') return void 0;
    const next = {
      music: music.name,
      orderIndex: view.getUint32(4, true),
      row: view.getUint32(8, true),
      frameInRow: view.getUint32(12, true),
    };
    const patternName = music.order[next.orderIndex];
    if (
      patternName === void 0 ||
      music.patterns[patternName]?.rows[next.row] === void 0 ||
      next.frameInRow >= music.framesPerRow
    )
      return void 0;
    return () => {
      this.tracker = next;
    };
  }
  voiceByte(voice, offset) {
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
  prepareVoiceWrite(voice, offset, bytes) {
    const staged = Uint8Array.from({ length: 48 }, (_, index) => this.voiceByte(voice, index));
    staged.set(bytes, offset);
    if (staged[0] !== 0 && staged[0] !== 1) return void 0;
    if ([1, 2, 3, 44, 45, 46, 47].some((index) => staged[index] !== 0)) return void 0;
    const view = new DataView(staged.buffer);
    const id = view.getUint32(4, true);
    const sound = this.assets.byId(id - 1);
    if (id !== 0 && sound?.kind !== 'sound') return void 0;
    const next = {
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
      return void 0;
    return () => {
      Object.assign(voice, next);
    };
  }
  executeCommand(command) {
    switch (command.name) {
      case 'sfx': {
        const [handle] = expectArguments$1(command, 1);
        this.trigger(readAssetName$1(handle, 'Sound'), void 0, 1, command.sourceSpan);
        return;
      }
      case 'music': {
        const [handle] = expectArguments$1(command, 1);
        const name = readAssetName$1(handle, 'Music');
        if (this.assets.get(name)?.kind !== 'music')
          throw new TypeError(`missing Music asset '${name}'`);
        this.tracker = {
          music: name,
          orderIndex: 0,
          row: 0,
          frameInRow: 0,
        };
        return;
      }
      case 'music_stop':
        expectArguments$1(command, 0);
        this.tracker = null;
        return;
      default:
        throw new TypeError(`unknown audio command '${command.name}'`);
    }
  }
  trigger(
    soundName,
    noteOverride,
    volumeScale = 1,
    sourceSpan = {
      start: 0,
      end: 0,
    },
  ) {
    const sound = this.assets.get(soundName);
    if (sound?.kind !== 'sound') throw new TypeError(`missing Sound asset '${soundName}'`);
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
  advanceTracker() {
    const tracker = this.tracker;
    if (tracker === null) return;
    const music = this.assets.get(tracker.music);
    if (music?.kind !== 'music') {
      this.tracker = null;
      return;
    }
    if (tracker.frameInRow === 0) {
      const patternName = music.order[tracker.orderIndex];
      (patternName === void 0 ? void 0 : music.patterns[patternName])?.rows[tracker.row]?.forEach(
        (cell) => {
          if (cell !== null) this.trigger(cell.sound, cell.note, cell.volume ?? 1);
        },
      );
    }
    tracker.frameInRow += 1;
    if (tracker.frameInRow < music.framesPerRow) return;
    tracker.frameInRow = 0;
    tracker.row += 1;
    const patternName = music.order[tracker.orderIndex];
    const pattern = patternName === void 0 ? void 0 : music.patterns[patternName];
    if (pattern !== void 0 && tracker.row < pattern.rows.length) return;
    tracker.row = 0;
    tracker.orderIndex += 1;
    if (tracker.orderIndex >= music.order.length) {
      if (music.loop) tracker.orderIndex = 0;
      else this.tracker = null;
    }
  }
  mixVoice(voice, left, right) {
    const sound = this.assets.get(voice.sound);
    if (sound?.kind !== 'sound') {
      voice.active = false;
      return;
    }
    const frameEnvelope = envelopeAt(sound, voice.ageFrames);
    const pan = Math.max(-1, Math.min(1, sound.pan));
    const leftGain = (1 - pan) / 2;
    const rightGain = (1 + pan) / 2;
    const phaseStep =
      noteFrequency(
        voice.note +
          sound.pitch.slideSemitonesPerFrame * voice.ageFrames +
          triangleLfo(voice.ageFrames, sound.pitch.vibratoPeriodFrames) *
            sound.pitch.vibratoDepthSemitones,
      ) / HARDWARE.audioSampleRate;
    for (let index = 0; index < left.length; index += 1) {
      const sample =
        oscillatorValue(sound, voice) * frameEnvelope * sound.volume * voice.volumeScale * 0.24;
      left[index] = clampSample((left[index] ?? 0) + sample * leftGain);
      right[index] = clampSample((right[index] ?? 0) + sample * rightGain);
      voice.phase = (voice.phase + phaseStep) % 1;
    }
  }
};
function emptyVoice(slot) {
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
function integerByte(value, offset) {
  return Math.floor(value / 2 ** (offset * 8)) & 255;
}
function floatByte(value, offset) {
  const view = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(8));
  view.setFloat64(0, value, true);
  return view.getUint8(offset);
}
function validateAudioAsset(asset) {
  if (!audioRecord(asset) || typeof asset.name !== 'string' || asset.name.length === 0)
    throw new TypeError('audio assets require a nonempty name');
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
    )
      throw new RangeError(`sound '${asset.name}' is outside MOD-01 limits`);
    if (
      asset.waveform === 'pulse' &&
      (!audioNumber(asset.duty, 0, 1) || asset.duty === 0 || asset.duty === 1)
    )
      throw new RangeError(`pulse sound '${asset.name}' requires a duty between zero and one`);
    if (
      asset.waveform === 'wavetable' &&
      (!denseAudioArray(asset.wavetable) ||
        asset.wavetable.length < 4 ||
        asset.wavetable.length > 32 ||
        asset.wavetable.some((sample) => !audioNumber(sample, -1, 1)))
    )
      throw new RangeError(`wavetable sound '${asset.name}' requires 4-32 normalized entries`);
    return;
  }
  if (
    asset.kind !== 'music' ||
    typeof asset.loop !== 'boolean' ||
    !audioInteger(asset.framesPerRow, 1, 240) ||
    !denseAudioArray(asset.order) ||
    asset.order.length === 0 ||
    !audioRecord(asset.patterns) ||
    asset.order.some((name) => typeof name !== 'string' || !Object.hasOwn(asset.patterns, name))
  )
    throw new RangeError(`music '${asset.name}' has an invalid order list or tempo`);
  for (const pattern of Object.values(asset.patterns)) {
    if (
      !audioRecord(pattern) ||
      !denseAudioArray(pattern.rows) ||
      pattern.rows.length === 0 ||
      pattern.rows.length > 256
    )
      throw new RangeError(`music '${asset.name}' has an invalid pattern length`);
    for (const row of pattern.rows) {
      if (!denseAudioArray(row) || row.length !== HARDWARE.trackerChannels)
        throw new RangeError(`music '${asset.name}' patterns must have eight channels`);
      for (const cell of row)
        if (
          cell !== null &&
          (!audioRecord(cell) ||
            !audioInteger(cell.note, 0, 127) ||
            typeof cell.sound !== 'string' ||
            cell.sound.length === 0 ||
            (cell.volume !== void 0 && !audioNumber(cell.volume, 0, 1)))
        )
          throw new RangeError(`music '${asset.name}' contains an invalid note`);
    }
  }
}
function denseAudioArray(value) {
  return (
    Array.isArray(value) &&
    Object.keys(value).length === value.length &&
    Array.from(value.keys()).every((index) => Object.hasOwn(value, index))
  );
}
function audioInteger(value, minimum, maximum) {
  return audioNumber(value, minimum, maximum) && Number.isSafeInteger(value);
}
function validEnvelope(envelope) {
  return (
    audioRecord(envelope) &&
    [envelope.attackFrames, envelope.decayFrames, envelope.releaseFrames].every((value) =>
      audioInteger(value, 0, 3600),
    ) &&
    audioNumber(envelope.sustainLevel, 0, 1)
  );
}
function validPitch(pitch, lifetime) {
  if (
    !audioRecord(pitch) ||
    typeof pitch.slideSemitonesPerFrame !== 'number' ||
    !Number.isFinite(pitch.slideSemitonesPerFrame) ||
    typeof pitch.vibratoDepthSemitones !== 'number' ||
    !Number.isFinite(pitch.vibratoDepthSemitones) ||
    !audioInteger(pitch.vibratoPeriodFrames, 0, 3600)
  )
    return false;
  const excursion =
    Math.abs(pitch.slideSemitonesPerFrame) * lifetime + Math.abs(pitch.vibratoDepthSemitones);
  return Number.isFinite(excursion) && Number.isFinite(noteFrequency(127 + excursion));
}
function oscillatorValue(sound, voice) {
  switch (sound.waveform) {
    case 'pulse':
      return voice.phase < (sound.duty ?? 0.5) ? 1 : -1;
    case 'triangle':
      return 1 - 4 * Math.abs(voice.phase - 0.5);
    case 'saw':
      return voice.phase * 2 - 1;
    case 'noise':
      voice.noiseState = xorshift(voice.noiseState);
      return (voice.noiseState & 65535) / 32767 - 1;
    case 'wavetable': {
      const table = sound.wavetable ?? [0];
      return table[Math.floor(voice.phase * table.length) % table.length] ?? 0;
    }
  }
}
function envelopeAt(sound, age) {
  const envelope = sound.envelope;
  if (envelope.attackFrames > 0 && age < envelope.attackFrames) return age / envelope.attackFrames;
  const decayAge = age - envelope.attackFrames;
  if (envelope.decayFrames > 0 && decayAge < envelope.decayFrames)
    return 1 - (1 - envelope.sustainLevel) * (decayAge / envelope.decayFrames);
  if (age < sound.durationFrames) return envelope.sustainLevel;
  if (envelope.releaseFrames === 0) return 0;
  return (
    envelope.sustainLevel * Math.max(0, 1 - (age - sound.durationFrames) / envelope.releaseFrames)
  );
}
function triangleLfo(frame, period) {
  if (period === 0) return 0;
  const phase = (frame % period) / period;
  return 1 - 4 * Math.abs(phase - 0.5);
}
function noteFrequency(note) {
  return 440 * 2 ** ((note - 69) / 12);
}
function xorshift(state) {
  let value = state >>> 0;
  value ^= value << 13;
  value ^= value >>> 17;
  value ^= value << 5;
  return value >>> 0;
}
function nonZeroNoiseSeed(frame, slot, sequence) {
  return ((frame + 1) * 2654435761 + (slot + 1) * 2246822507 + sequence) >>> 0 || 1;
}
function clampSample(sample) {
  return Math.max(-1, Math.min(1, sample));
}
function expectArguments$1(command, count) {
  if (command.arguments.length !== count)
    throw new TypeError(
      `${command.name} expected ${String(count)} arguments, received ${String(command.arguments.length)}`,
    );
  return [...command.arguments];
}
function readAssetName$1(value, kind) {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('name' in value) ||
    typeof value.name !== 'string' ||
    !('kind' in value) ||
    value.kind !== kind
  )
    throw new TypeError(`expected a ${kind} asset handle`);
  return value.name;
}
//#endregion
//#region src/font.ts
var BITMAP_FONT = Object.freeze({
  glyphWidth: 5,
  glyphHeight: 7,
  advanceX: 6,
  advanceY: 8,
});
var GLYPHS = Object.freeze({
  ' ': [0, 0, 0, 0, 0, 0, 0],
  '!': [4, 4, 4, 4, 4, 0, 4],
  '"': [10, 10, 10, 0, 0, 0, 0],
  '#': [10, 31, 10, 10, 31, 10, 0],
  $: [4, 15, 20, 14, 5, 30, 4],
  '%': [25, 26, 4, 4, 11, 19, 0],
  '&': [12, 18, 20, 8, 21, 18, 13],
  "'": [4, 4, 8, 0, 0, 0, 0],
  '(': [2, 4, 8, 8, 8, 4, 2],
  ')': [8, 4, 2, 2, 2, 4, 8],
  '*': [0, 21, 14, 31, 14, 21, 0],
  '+': [0, 4, 4, 31, 4, 4, 0],
  ',': [0, 0, 0, 0, 4, 4, 8],
  '-': [0, 0, 0, 31, 0, 0, 0],
  '.': [0, 0, 0, 0, 0, 12, 12],
  '/': [1, 2, 2, 4, 8, 8, 16],
  0: [14, 17, 19, 21, 25, 17, 14],
  1: [4, 12, 4, 4, 4, 4, 14],
  2: [14, 17, 1, 2, 4, 8, 31],
  3: [30, 1, 1, 14, 1, 1, 30],
  4: [2, 6, 10, 18, 31, 2, 2],
  5: [31, 16, 16, 30, 1, 1, 30],
  6: [14, 16, 16, 30, 17, 17, 14],
  7: [31, 1, 2, 4, 8, 8, 8],
  8: [14, 17, 17, 14, 17, 17, 14],
  9: [14, 17, 17, 15, 1, 1, 14],
  ':': [0, 12, 12, 0, 12, 12, 0],
  ';': [0, 12, 12, 0, 4, 4, 8],
  '<': [2, 4, 8, 16, 8, 4, 2],
  '=': [0, 0, 31, 0, 31, 0, 0],
  '>': [8, 4, 2, 1, 2, 4, 8],
  '?': [14, 17, 1, 2, 4, 0, 4],
  '@': [14, 17, 23, 21, 23, 16, 14],
  A: [14, 17, 17, 31, 17, 17, 17],
  B: [30, 17, 17, 30, 17, 17, 30],
  C: [14, 17, 16, 16, 16, 17, 14],
  D: [28, 18, 17, 17, 17, 18, 28],
  E: [31, 16, 16, 30, 16, 16, 31],
  F: [31, 16, 16, 30, 16, 16, 16],
  G: [14, 17, 16, 23, 17, 17, 15],
  H: [17, 17, 17, 31, 17, 17, 17],
  I: [14, 4, 4, 4, 4, 4, 14],
  J: [7, 2, 2, 2, 2, 18, 12],
  K: [17, 18, 20, 24, 20, 18, 17],
  L: [16, 16, 16, 16, 16, 16, 31],
  M: [17, 27, 21, 21, 17, 17, 17],
  N: [17, 25, 25, 21, 19, 19, 17],
  O: [14, 17, 17, 17, 17, 17, 14],
  P: [30, 17, 17, 30, 16, 16, 16],
  Q: [14, 17, 17, 17, 21, 18, 13],
  R: [30, 17, 17, 30, 20, 18, 17],
  S: [15, 16, 16, 14, 1, 1, 30],
  T: [31, 4, 4, 4, 4, 4, 4],
  U: [17, 17, 17, 17, 17, 17, 14],
  V: [17, 17, 17, 17, 17, 10, 4],
  W: [17, 17, 17, 21, 21, 21, 10],
  X: [17, 17, 10, 4, 10, 17, 17],
  Y: [17, 17, 10, 4, 4, 4, 4],
  Z: [31, 1, 2, 4, 8, 16, 31],
  '[': [14, 8, 8, 8, 8, 8, 14],
  '\\': [16, 8, 8, 4, 2, 2, 1],
  ']': [14, 2, 2, 2, 2, 2, 14],
  '^': [4, 10, 17, 0, 0, 0, 0],
  _: [0, 0, 0, 0, 0, 0, 31],
  '`': [8, 4, 2, 0, 0, 0, 0],
  '{': [3, 4, 4, 24, 4, 4, 3],
  '|': [4, 4, 4, 4, 4, 4, 4],
  '}': [24, 4, 4, 3, 4, 4, 24],
  '~': [0, 0, 9, 22, 0, 0, 0],
});
var FALLBACK = GLYPHS['?'];
/** Lowercase has a compact small-cap treatment in revision 1 of the original console font. */
function glyphRows(character) {
  if (character.length !== 1) return FALLBACK;
  return GLYPHS[character] ?? GLYPHS[character.toUpperCase()] ?? FALLBACK;
}
//#endregion
//#region src/visual-store.ts
var LEGACY_VISUAL_BYTES = 131072;
/** A single packed, little-endian visual image; all drawing views alias these bytes. */
var VisualAssetStore = class {
  entries = /* @__PURE__ */ new Map();
  ids = /* @__PURE__ */ new Map();
  bytes = new Uint8Array(HARDWARE.visualCapacityBytes);
  allocations = [];
  descriptors;
  allocationTable;
  info = /* @__PURE__ */ new Uint8Array(24);
  addressBase;
  display;
  usedBytes;
  constructor(assets = [], display) {
    if (assets.length > 4096) throw new RangeError('too many visual assets');
    const sorted = [...assets].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    const source = /* @__PURE__ */ new Map();
    let usedBytes = 0;
    for (const [id, asset] of sorted.entries()) {
      if (source.has(asset.name)) throw new TypeError(`duplicate visual asset '${asset.name}'`);
      validateAsset(asset);
      usedBytes += visualAssetBytes(asset);
      source.set(asset.name, asset);
      this.ids.set(asset.name, id);
    }
    if (display !== void 0) usedBytes += 32 + display.raster.length * 38;
    if (usedBytes > HARDWARE.visualCapacityBytes)
      throw new RangeError('visual assets exceed the 1 MiB shared capacity');
    this.usedBytes = usedBytes;
    this.addressBase = usedBytes > LEGACY_VISUAL_BYTES ? MEMORY.visualExtended : MEMORY.visual;
    this.descriptors = new Uint8Array(sorted.length * MEMORY.assetStride);
    const descriptorView = new DataView(this.descriptors.buffer);
    for (const [id, asset] of sorted.entries()) {
      const first = this.allocations.length;
      const stored = this.store(asset, source);
      this.entries.set(asset.name, stored);
      const offset = id * MEMORY.assetStride;
      descriptorView.setUint32(
        offset,
        ['sprite', 'animation', 'tile_set', 'map', 'font'].indexOf(asset.kind) + 1,
        true,
      );
      descriptorView.setUint32(offset + 4, this.allocations.length - first, true);
      descriptorView.setUint32(
        offset + 8,
        MEMORY.allocations + first * MEMORY.allocationStride,
        true,
      );
      descriptorView.setUint32(offset + 12, visualAssetBytes(asset), true);
    }
    this.display = display === void 0 ? void 0 : this.storeDisplay(display);
    this.allocationTable = new Uint8Array(this.allocations.length * MEMORY.allocationStride);
    const allocationView = new DataView(this.allocationTable.buffer);
    for (const [index, entry] of this.allocations.entries()) {
      const offset = index * MEMORY.allocationStride;
      [
        entry.kind,
        this.addressBase + entry.offset,
        entry.bytes.length,
        entry.width,
        entry.height,
        entry.reference,
      ].forEach((value, field) => {
        allocationView.setUint32(offset + field * 4, value, true);
      });
    }
    const info = new DataView(this.info.buffer);
    [
      sorted.length,
      usedBytes,
      MEMORY.assets,
      this.allocations.length,
      MEMORY.allocations,
      display === void 0 ? 0 : this.addressBase + usedBytes - 32 - display.raster.length * 38,
    ].forEach((value, field) => {
      info.setUint32(field * 4, value, true);
    });
  }
  get(name) {
    return this.entries.get(name);
  }
  id(name) {
    return this.ids.get(name) ?? -1;
  }
  mapCell(name, layer, x, y) {
    const asset = this.entries.get(name);
    if (asset?.kind !== 'map') return void 0;
    const selected = asset.layers[layer];
    if (selected === void 0 || x < 0 || y < 0 || x >= selected.width || y >= selected.height)
      return void 0;
    return selected.cells.getUint16((y * selected.width + x) * 2, true);
  }
  mapFlag(name, layer, x, y, flag) {
    const asset = this.entries.get(name);
    if (asset?.kind !== 'map' || flag < 0 || flag > 7) return false;
    const selected = asset.layers[layer];
    if (selected === void 0) return false;
    const tile = this.mapCell(name, layer, x, y);
    const tileSet = this.entries.get(selected.tileSet);
    return (
      tile !== void 0 &&
      tileSet?.kind === 'tile_set' &&
      ((tileSet.flags[tile] ?? 0) & (1 << flag)) !== 0
    );
  }
  memoryRegions() {
    return [
      {
        name: 'visual store',
        address: MEMORY.visual,
        bytes: this.bytes.subarray(0, LEGACY_VISUAL_BYTES),
        writable: true,
        validate: (offset, bytes) => this.validate(offset, bytes),
      },
      ...(this.addressBase === MEMORY.visualExtended
        ? [
            {
              name: 'visual store extended',
              address: MEMORY.visualExtended,
              bytes: this.bytes,
              writable: true,
              validate: (offset, bytes) => this.validate(offset, bytes),
            },
          ]
        : []),
      {
        name: 'visual allocation status',
        address: MEMORY.visualInfo,
        bytes: this.info,
        writable: false,
      },
      ...(this.descriptors.length === 0
        ? []
        : [
            {
              name: 'visual asset descriptors',
              address: MEMORY.assets,
              bytes: this.descriptors,
              writable: false,
            },
            {
              name: 'visual allocations',
              address: MEMORY.allocations,
              bytes: this.allocationTable,
              writable: false,
            },
          ]),
      ...(this.descriptors.length === 0 && this.allocationTable.length > 0
        ? [
            {
              name: 'visual allocations',
              address: MEMORY.allocations,
              bytes: this.allocationTable,
              writable: false,
            },
          ]
        : []),
    ];
  }
  allocate(kind, width, height, data, reference = 0, validate) {
    const previous = this.allocations.at(-1);
    const offset = previous === void 0 ? 0 : previous.offset + previous.bytes.length;
    const bytes = this.bytes.subarray(offset, offset + data.length);
    bytes.set(data);
    this.allocations.push({
      offset,
      bytes,
      kind,
      width,
      height,
      reference,
      ...(validate === void 0 ? {} : { validate }),
    });
    return bytes;
  }
  sprite(sprite) {
    return {
      ...sprite,
      pixels: this.allocate(1, sprite.width, sprite.height, sprite.pixels, 0, (_offset, bytes) =>
        bytes.every((color) => color < HARDWARE.paletteSize),
      ),
    };
  }
  store(asset, source) {
    switch (asset.kind) {
      case 'sprite':
        return this.sprite(asset);
      case 'animation':
        return {
          ...asset,
          frames: asset.frames.map((frame) => this.sprite(frame)),
        };
      case 'tile_set':
        return {
          ...asset,
          tiles: asset.tiles.map((tile) => this.sprite(tile)),
          flags: this.allocate(3, asset.flags.length, 1, asset.flags),
        };
      case 'map':
        return {
          ...asset,
          layers: asset.layers.map((layer) => {
            const tileSet = source.get(layer.tileSet);
            if (
              tileSet?.kind !== 'tile_set' ||
              layer.cells.some((tile) => tile >= tileSet.tiles.length)
            )
              throw new TypeError(`map '${asset.name}' references an invalid tile set or tile`);
            const encoded = new Uint8Array(layer.cells.length * 2);
            const view = new DataView(encoded.buffer);
            for (let index = 0; index < layer.cells.length; index += 1)
              view.setUint16(index * 2, layer.cells[index] ?? 0, true);
            const bytes = this.allocate(
              2,
              layer.width,
              layer.height,
              encoded,
              (this.ids.get(layer.tileSet) ?? -1) + 1,
              (offset, part) => {
                const byte = (index) =>
                  (index >= offset && index < offset + part.length
                    ? part[index - offset]
                    : bytes[index]) ?? 0;
                for (let index = offset - (offset % 2); index < offset + part.length; index += 2)
                  if (byte(index) + byte(index + 1) * 256 >= tileSet.tiles.length) return false;
                return true;
              },
            );
            return {
              ...layer,
              cells: new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength),
            };
          }),
        };
      case 'font': {
        const glyphs = [...asset.glyphs.entries()].sort(([left], [right]) => left - right);
        const glyphBytes = asset.glyphWidth * asset.glyphHeight;
        const encoded = new Uint8Array(8 + glyphs.length * (2 + glyphBytes));
        encoded.set(
          [
            asset.glyphWidth,
            asset.glyphHeight,
            asset.baseline,
            asset.advanceX,
            asset.advanceY,
            asset.missingGlyph,
          ],
          0,
        );
        new DataView(encoded.buffer).setUint16(6, glyphs.length, true);
        const bitmapOffsets = /* @__PURE__ */ new Set();
        for (const [index, [code, pixels]] of glyphs.entries()) {
          const offset = 8 + index * (2 + glyphBytes);
          new DataView(encoded.buffer).setUint16(offset, code, true);
          encoded.set(pixels, offset + 2);
          for (let byte = offset + 2; byte < offset + 2 + glyphBytes; byte += 1)
            bitmapOffsets.add(byte);
        }
        const bytes = this.allocate(
          6,
          asset.glyphWidth,
          asset.glyphHeight,
          encoded,
          asset.missingGlyph,
          (offset, part) =>
            part.every((value, index) => {
              const absolute = offset + index;
              return bitmapOffsets.has(absolute) ? value <= 1 : value === encoded[absolute];
            }),
        );
        return {
          ...asset,
          glyphs: new Map(
            glyphs.map(([code], index) => {
              const offset = 8 + index * (2 + glyphBytes) + 2;
              return [code, bytes.subarray(offset, offset + glyphBytes)];
            }),
          ),
        };
      }
    }
  }
  storeDisplay(display) {
    if (display.remap.length !== HARDWARE.rasterPaletteSize || display.raster.length > 144)
      throw new TypeError('invalid display defaults');
    const remap = this.allocate(4, 32, 1, display.remap, 0, (_offset, bytes) =>
      bytes.every((value) => value < HARDWARE.paletteSize),
    );
    let previous = -1;
    return {
      remap,
      raster: display.raster.map((row) => {
        if (
          !Number.isInteger(row.line) ||
          row.line <= previous ||
          row.line >= 144 ||
          ![row.scrollX, row.scrollY].every(
            (value) => Number.isInteger(value) && value >= -32768 && value <= 32767,
          ) ||
          row.remap.length !== HARDWARE.rasterPaletteSize
        )
          throw new TypeError('invalid display raster defaults');
        previous = row.line;
        const encoded = /* @__PURE__ */ new Uint8Array(38);
        const view = new DataView(encoded.buffer);
        view.setUint16(0, row.line, true);
        view.setInt16(2, row.scrollX, true);
        view.setInt16(4, row.scrollY, true);
        encoded.set(row.remap, 6);
        const bytes = this.allocate(5, 38, 1, encoded, 0, (offset, part) =>
          part.every((value, index) =>
            offset + index < 2
              ? value === encoded[offset + index]
              : offset + index < 6 || value < HARDWARE.paletteSize,
          ),
        );
        const stored = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        return {
          line: row.line,
          get scrollX() {
            return stored.getInt16(2, true);
          },
          get scrollY() {
            return stored.getInt16(4, true);
          },
          remap: bytes.subarray(6),
        };
      }),
    };
  }
  validate(offset, bytes) {
    let low = 0;
    let high = this.allocations.length;
    while (low < high) {
      const mid = (low + high) >>> 1;
      const entry = this.allocations[mid];
      if (entry !== void 0 && entry.offset + entry.bytes.length <= offset) low = mid + 1;
      else high = mid;
    }
    for (let index = low; index < this.allocations.length; index += 1) {
      const entry = this.allocations[index];
      if (entry === void 0 || entry.offset >= offset + bytes.length) break;
      const start = Math.max(offset, entry.offset);
      const end = Math.min(offset + bytes.length, entry.offset + entry.bytes.length);
      if (
        entry.validate?.(start - entry.offset, bytes.subarray(start - offset, end - offset)) ===
        false
      )
        return false;
    }
    return true;
  }
};
function validateAsset(asset) {
  if (asset.name.length === 0) throw new TypeError('visual asset names cannot be empty');
  switch (asset.kind) {
    case 'sprite':
      validateSprite(asset);
      return;
    case 'animation':
      if (asset.frames.length === 0)
        throw new RangeError(`animation '${asset.name}' has no frames`);
      asset.frames.forEach(validateSprite);
      return;
    case 'tile_set':
      if (asset.tiles.length === 0 || asset.flags.length !== asset.tiles.length)
        throw new RangeError(`tile set '${asset.name}' has incoherent tiles or flags`);
      for (const tile of asset.tiles) {
        validateSprite(tile);
        if (tile.width !== 8 || tile.height !== 8)
          throw new RangeError(`tile set '${asset.name}' contains a non-8x8 tile`);
      }
      return;
    case 'map':
      if (asset.layers.length === 0) throw new RangeError(`map '${asset.name}' has no layers`);
      for (const layer of asset.layers)
        if (
          !Number.isSafeInteger(layer.width) ||
          !Number.isSafeInteger(layer.height) ||
          layer.width <= 0 ||
          layer.height <= 0 ||
          layer.cells.length !== layer.width * layer.height ||
          layer.tileSet.length === 0
        )
          throw new RangeError(`map '${asset.name}' has an invalid layer`);
      return;
    case 'font':
      if (
        !Number.isSafeInteger(asset.glyphWidth) ||
        !Number.isSafeInteger(asset.glyphHeight) ||
        asset.glyphWidth < 1 ||
        asset.glyphWidth > 16 ||
        asset.glyphHeight < 1 ||
        asset.glyphHeight > 16 ||
        !Number.isSafeInteger(asset.baseline) ||
        asset.baseline < 0 ||
        asset.baseline >= asset.glyphHeight ||
        !Number.isSafeInteger(asset.advanceX) ||
        asset.advanceX < 1 ||
        asset.advanceX > 32 ||
        !Number.isSafeInteger(asset.advanceY) ||
        asset.advanceY < 1 ||
        asset.advanceY > 32 ||
        !Number.isSafeInteger(asset.missingGlyph) ||
        asset.missingGlyph < 0 ||
        asset.missingGlyph > 255 ||
        asset.glyphs.size === 0 ||
        asset.glyphs.size > 256 ||
        !asset.glyphs.has(asset.missingGlyph)
      )
        throw new RangeError(`font '${asset.name}' has invalid metrics or glyph map`);
      for (const [code, pixels] of asset.glyphs)
        if (
          !Number.isSafeInteger(code) ||
          code < 0 ||
          code > 255 ||
          pixels.length !== asset.glyphWidth * asset.glyphHeight ||
          pixels.some((value) => value > 1)
        )
          throw new RangeError(`font '${asset.name}' has an invalid glyph`);
      return;
  }
}
function validateSprite(sprite) {
  if (
    !Number.isSafeInteger(sprite.width) ||
    !Number.isSafeInteger(sprite.height) ||
    sprite.width < 1 ||
    sprite.width > HARDWARE.spriteMaximumAxis ||
    sprite.height < 1 ||
    sprite.height > HARDWARE.spriteMaximumAxis ||
    sprite.pixels.length !== sprite.width * sprite.height ||
    sprite.pixels.some((color) => color >= HARDWARE.paletteSize)
  )
    throw new RangeError(`sprite '${sprite.name}' is outside MOD-01 limits`);
}
function visualAssetBytes(asset) {
  switch (asset.kind) {
    case 'sprite':
      return asset.pixels.byteLength;
    case 'animation':
      return asset.frames.reduce((total, frame) => total + frame.pixels.byteLength, 0);
    case 'tile_set':
      return (
        asset.flags.byteLength +
        asset.tiles.reduce((total, tile) => total + tile.pixels.byteLength, 0)
      );
    case 'map':
      return asset.layers.reduce((total, layer) => total + layer.cells.byteLength, 0);
    case 'font':
      return 8 + asset.glyphs.size * (2 + asset.glyphWidth * asset.glyphHeight);
  }
}
//#endregion
//#region src/graphics.ts
function isGraphicsSnapshot(value) {
  const pixels = HARDWARE.width * HARDWARE.height;
  return (
    isRecord$10(value) &&
    value.revision === 1 &&
    value.front instanceof Uint8Array &&
    value.front.length === pixels &&
    value.front.every((color) => color < HARDWARE.paletteSize) &&
    value.resolved instanceof Uint8Array &&
    value.resolved.length === pixels &&
    value.resolved.every((color) => color < HARDWARE.paletteSize)
  );
}
/** Deterministic indexed immediate-mode rasterizer with double-buffered storage. */
var IndexedGraphics = class {
  front = new Uint8Array(HARDWARE.width * HARDWARE.height);
  back = new Uint8Array(HARDWARE.width * HARDWARE.height);
  resolved = new Uint8Array(HARDWARE.width * HARDWARE.height);
  assets;
  display;
  drawRegisters = /* @__PURE__ */ new Uint8Array(80);
  transparency = Uint8Array.of(HARDWARE.transparentColor);
  state = new MemoryDrawState(this.drawRegisters);
  rasterBytes = new Uint8Array(HARDWARE.height * MEMORY.rasterStride);
  rasterView = new DataView(this.rasterBytes.buffer);
  rasterRemaps = Array.from({ length: HARDWARE.height }, (_, line) =>
    this.rasterBytes.subarray(line * MEMORY.rasterStride + 8, (line + 1) * MEMORY.rasterStride),
  );
  rasterLive = /* @__PURE__ */ new Uint8Array(48);
  rasterLiveView = new DataView(this.rasterLive.buffer);
  displayRemap = this.rasterLive.subarray(16);
  commandCount = 0;
  activeFrame = false;
  constructor(assets = new VisualAssetStore(), display) {
    this.assets = assets;
    this.display =
      display === void 0
        ? (assets.display ?? copyDisplayConfiguration())
        : copyDisplayConfiguration(display);
    this.state.clipWidth = HARDWARE.width;
    this.state.clipHeight = HARDWARE.height;
    this.state.remap.set(this.display.remap);
    this.displayRemap.set(identityRemap());
  }
  executeFrame(commands) {
    if (commands.length > HARDWARE.drawCommandsPerFrame)
      throw new RangeError('draw-command ceiling exceeded');
    this.beginFrame();
    for (const command of commands) this.executeCommand(command);
    return this.finishFrame();
  }
  beginFrame() {
    this.back.set(this.front);
    this.drawRegisters.fill(0);
    this.state.clipWidth = HARDWARE.width;
    this.state.clipHeight = HARDWARE.height;
    this.state.remap.set(this.display.remap);
    this.rasterBytes.fill(0);
    this.rasterLive.fill(0);
    this.displayRemap.set(identityRemap());
    this.commandCount = 0;
    this.activeFrame = true;
    for (const raster of this.display.raster) {
      this.rasterRemaps[raster.line]?.set(raster.remap);
      this.rasterView.setInt16(raster.line * MEMORY.rasterStride + 4, raster.scrollX, true);
      this.rasterView.setInt16(raster.line * MEMORY.rasterStride + 6, raster.scrollY, true);
      this.rasterBytes[raster.line * MEMORY.rasterStride] = 1;
    }
  }
  executeCommand(command) {
    if (!this.activeFrame) throw new Error('graphics frame has not begun');
    if (this.commandCount >= HARDWARE.drawCommandsPerFrame)
      throw new RangeError('draw-command ceiling exceeded');
    this.commandCount += 1;
    if (command.rasterLine === void 0) {
      const writesState = ['camera', 'clip', 'clip_reset', 'pal', 'pal_reset'].includes(
        command.name,
      );
      this.executeDraw(command, writesState ? this.state : this.state.capture());
      return;
    }
    const line = command.rasterLine;
    if (line < 0 || line >= HARDWARE.height)
      throw new RangeError('raster command has an invalid scanline');
    if (command.name === 'pal') {
      const [from, to] = expectIntegers(command, 2);
      this.displayRemap[expectRemappableColor(from)] = expectColor(to);
    } else if (command.name === 'raster_scroll') {
      const [x, y] = expectIntegers(command, 2);
      this.rasterLiveView.setFloat64(0, x, true);
      this.rasterLiveView.setFloat64(8, y, true);
    } else throw new TypeError(`'${command.name}' is not valid during raster display`);
    this.rasterView.setInt16(
      line * MEMORY.rasterStride + 4,
      clampInt16(this.rasterLiveView.getFloat64(0, true)),
      true,
    );
    this.rasterView.setInt16(
      line * MEMORY.rasterStride + 6,
      clampInt16(this.rasterLiveView.getFloat64(8, true)),
      true,
    );
    this.rasterRemaps[line]?.set(this.displayRemap);
    this.rasterBytes[line * MEMORY.rasterStride] = 1;
  }
  finishFrame() {
    if (!this.activeFrame) throw new Error('graphics frame has not begun');
    let previousRemap = identityRemap();
    let previousScrollX = 0;
    let previousScrollY = 0;
    for (let y = 0; y < HARDWARE.height; y += 1) {
      const lineRemap = this.rasterRemaps[y];
      if (lineRemap !== void 0 && this.rasterBytes[y * MEMORY.rasterStride] === 1) {
        previousRemap = lineRemap;
        previousScrollX = this.rasterView.getInt16(y * MEMORY.rasterStride + 4, true);
        previousScrollY = this.rasterView.getInt16(y * MEMORY.rasterStride + 6, true);
      }
      for (let x = 0; x < HARDWARE.width; x += 1) {
        const sourceX = wrap(x + previousScrollX, HARDWARE.width);
        const sourceY = wrap(y + previousScrollY, HARDWARE.height);
        const color = this.back[sourceY * HARDWARE.width + sourceX] ?? 0;
        this.resolved[y * HARDWARE.width + x] = previousRemap[color] ?? color;
      }
    }
    this.front.set(this.back);
    this.activeFrame = false;
    return {
      indexedPixels: this.resolved.slice(),
      commands: this.commandCount,
    };
  }
  snapshot() {
    return {
      revision: 1,
      front: this.front.slice(),
      resolved: this.resolved.slice(),
    };
  }
  memoryRegions() {
    const validate = (_offset, bytes) => bytes.every((color) => color < HARDWARE.paletteSize);
    return [
      {
        name: 'front',
        address: MEMORY.front,
        bytes: this.front,
        writable: true,
        validate,
      },
      {
        name: 'back',
        address: MEMORY.back,
        bytes: this.back,
        writable: true,
        validate,
      },
      {
        name: 'display',
        address: MEMORY.display,
        bytes: this.resolved,
        writable: false,
        retained: true,
        validate,
      },
      {
        name: 'draw state',
        address: MEMORY.draw,
        bytes: this.drawRegisters,
        writable: true,
        validate: (offset, bytes) => {
          const candidate = this.drawRegisters.slice();
          candidate.set(bytes, offset);
          const view = new DataView(candidate.buffer);
          for (let field = 0; field < 48; field += 8)
            if (!Number.isSafeInteger(view.getFloat64(field, true))) return false;
          return (
            view.getFloat64(32, true) >= 0 &&
            view.getFloat64(40, true) >= 0 &&
            candidate.subarray(48).every((color) => color < HARDWARE.paletteSize)
          );
        },
      },
      {
        name: 'transparency index',
        address: MEMORY.transparency,
        bytes: this.transparency,
        writable: false,
      },
      {
        name: 'raster callback state',
        address: MEMORY.rasterLive,
        bytes: this.rasterLive,
        writable: false,
        retained: true,
        validate: (_offset, bytes) => {
          const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
          return (
            bytes.length === 48 &&
            Number.isSafeInteger(view.getFloat64(0, true)) &&
            Number.isSafeInteger(view.getFloat64(8, true)) &&
            bytes.subarray(16).every((color) => color < HARDWARE.paletteSize)
          );
        },
      },
      {
        name: 'raster table',
        address: MEMORY.raster,
        bytes: this.rasterBytes,
        writable: true,
        rasterWritable: true,
        validate: (offset, bytes) =>
          bytes.every((value, index) => {
            const field = (offset + index) % MEMORY.rasterStride;
            return field === 0
              ? value <= 1
              : field < 4
                ? value === 0
                : field < 8 || value < HARDWARE.paletteSize;
          }),
      },
    ];
  }
  restore(snapshot) {
    if (!isGraphicsSnapshot(snapshot)) throw new TypeError('invalid indexed graphics snapshot');
    this.front.set(snapshot.front);
    this.back.set(snapshot.front);
    this.resolved.set(snapshot.resolved);
    this.activeFrame = false;
  }
  executeDraw(command, state) {
    switch (command.name) {
      case 'clear': {
        const [color] = expectIntegers(command, 1);
        this.back.fill(state.remap[expectColor(color)] ?? color);
        return;
      }
      case 'pixel': {
        const [x, y, color] = expectIntegers(command, 3);
        this.plot(x, y, color, state);
        return;
      }
      case 'line': {
        const [x0, y0, x1, y1, color] = expectIntegers(command, 5);
        this.line(x0, y0, x1, y1, color, state);
        return;
      }
      case 'rect':
      case 'rect_fill': {
        const [x, y, width, height, color] = expectIntegers(command, 5);
        this.rectangle(x, y, width, height, color, command.name === 'rect_fill', state);
        return;
      }
      case 'circle':
      case 'circle_fill': {
        const [x, y, radius, color] = expectIntegers(command, 4);
        this.circle(x, y, radius, color, command.name === 'circle_fill', state);
        return;
      }
      case 'triangle': {
        const [x0, y0, x1, y1, x2, y2, color] = expectIntegers(command, 7);
        this.triangle(x0, y0, x1, y1, x2, y2, color, state);
        return;
      }
      case 'camera':
        [state.cameraX, state.cameraY] = expectIntegers(command, 2);
        return;
      case 'clip': {
        const [x, y, width, height] = expectIntegers(command, 4);
        state.clipX = x;
        state.clipY = y;
        state.clipWidth = Math.max(0, width);
        state.clipHeight = Math.max(0, height);
        return;
      }
      case 'clip_reset':
        expectIntegers(command, 0);
        state.clipX = 0;
        state.clipY = 0;
        state.clipWidth = HARDWARE.width;
        state.clipHeight = HARDWARE.height;
        return;
      case 'pal': {
        const [from, to] = expectIntegers(command, 2);
        state.remap[expectRemappableColor(from)] = expectColor(to);
        return;
      }
      case 'pal_reset':
        expectIntegers(command, 0);
        state.remap.set(identityRemap());
        return;
      case 'sprite': {
        const [handle, x, y] = command.arguments;
        this.drawSprite(
          readAssetName(handle, 'Sprite'),
          expectInteger$1(x),
          expectInteger$1(y),
          state,
        );
        return;
      }
      case 'animation': {
        const [handle, frame, x, y] = command.arguments;
        this.drawAnimation(
          readAssetName(handle, 'Animation'),
          expectInteger$1(frame),
          expectInteger$1(x),
          expectInteger$1(y),
          state,
        );
        return;
      }
      case 'sprite_xform': {
        const [handle, x, y, scale, quarterTurns, flipX, flipY] = command.arguments;
        this.drawTransformedSprite(
          readAssetName(handle, 'Sprite'),
          expectInteger$1(x),
          expectInteger$1(y),
          expectInteger$1(scale),
          expectInteger$1(quarterTurns),
          expectBoolean(flipX),
          expectBoolean(flipY),
          state,
        );
        return;
      }
      case 'map': {
        const [handle, x, y] = command.arguments;
        this.drawMap(readAssetName(handle, 'Map'), expectInteger$1(x), expectInteger$1(y), state);
        return;
      }
      case 'raster_scroll':
        throw new TypeError('raster_scroll is only valid in the raster callback');
      case 'print': {
        const [text, x, y, color] = command.arguments;
        this.print(
          expectText(text),
          expectInteger$1(x),
          expectInteger$1(y),
          expectInteger$1(color),
          state,
        );
        return;
      }
      case 'font_print': {
        const [handle, text, x, y, color] = command.arguments;
        this.printFont(
          readAssetName(handle, 'Font'),
          expectText(text),
          expectInteger$1(x),
          expectInteger$1(y),
          expectInteger$1(color),
          state,
        );
        return;
      }
      default:
        throw new TypeError(`unknown graphics command '${command.name}'`);
    }
  }
  plot(x, y, color, state) {
    const screenX = x - state.cameraX;
    const screenY = y - state.cameraY;
    if (
      screenX < 0 ||
      screenX >= HARDWARE.width ||
      screenY < 0 ||
      screenY >= HARDWARE.height ||
      screenX < state.clipX ||
      screenX >= state.clipX + state.clipWidth ||
      screenY < state.clipY ||
      screenY >= state.clipY + state.clipHeight
    )
      return;
    this.back[screenY * HARDWARE.width + screenX] = state.remap[expectColor(color)] ?? color;
  }
  line(startX, startY, endX, endY, color, state) {
    let x = startX;
    let y = startY;
    const deltaX = Math.abs(endX - startX);
    const stepX = startX < endX ? 1 : -1;
    const deltaY = -Math.abs(endY - startY);
    const stepY = startY < endY ? 1 : -1;
    let error = deltaX + deltaY;
    for (;;) {
      this.plot(x, y, color, state);
      if (x === endX && y === endY) return;
      const doubled = error * 2;
      if (doubled >= deltaY) {
        error += deltaY;
        x += stepX;
      }
      if (doubled <= deltaX) {
        error += deltaX;
        y += stepY;
      }
    }
  }
  rectangle(x, y, width, height, color, filled, state) {
    if (width <= 0 || height <= 0) return;
    if (!filled) {
      this.line(x, y, x + width - 1, y, color, state);
      this.line(x, y + height - 1, x + width - 1, y + height - 1, color, state);
      this.line(x, y, x, y + height - 1, color, state);
      this.line(x + width - 1, y, x + width - 1, y + height - 1, color, state);
      return;
    }
    for (let row = 0; row < height; row += 1)
      this.line(x, y + row, x + width - 1, y + row, color, state);
  }
  circle(centerX, centerY, radius, color, filled, state) {
    if (radius < 0) return;
    let x = radius;
    let y = 0;
    let error = 1 - radius;
    while (x >= y) {
      if (filled) {
        this.line(centerX - x, centerY + y, centerX + x, centerY + y, color, state);
        this.line(centerX - x, centerY - y, centerX + x, centerY - y, color, state);
        this.line(centerX - y, centerY + x, centerX + y, centerY + x, color, state);
        this.line(centerX - y, centerY - x, centerX + y, centerY - x, color, state);
      } else
        for (const [plotX, plotY] of [
          [centerX + x, centerY + y],
          [centerX + y, centerY + x],
          [centerX - y, centerY + x],
          [centerX - x, centerY + y],
          [centerX - x, centerY - y],
          [centerX - y, centerY - x],
          [centerX + y, centerY - x],
          [centerX + x, centerY - y],
        ])
          this.plot(plotX, plotY, color, state);
      y += 1;
      if (error < 0) error += 2 * y + 1;
      else {
        x -= 1;
        error += 2 * (y - x) + 1;
      }
    }
  }
  triangle(x0, y0, x1, y1, x2, y2, color, state) {
    const minimumX = Math.min(x0, x1, x2);
    const maximumX = Math.max(x0, x1, x2);
    const minimumY = Math.min(y0, y1, y2);
    const maximumY = Math.max(y0, y1, y2);
    const area = edge(x0, y0, x1, y1, x2, y2);
    if (area === 0) {
      this.line(x0, y0, x1, y1, color, state);
      this.line(x1, y1, x2, y2, color, state);
      return;
    }
    for (let y = minimumY; y <= maximumY; y += 1)
      for (let x = minimumX; x <= maximumX; x += 1) {
        const first = edge(x1, y1, x2, y2, x, y);
        const second = edge(x2, y2, x0, y0, x, y);
        const third = edge(x0, y0, x1, y1, x, y);
        if (
          (area > 0 && first >= 0 && second >= 0 && third >= 0) ||
          (area < 0 && first <= 0 && second <= 0 && third <= 0)
        )
          this.plot(x, y, color, state);
      }
  }
  drawSprite(name, x, y, state) {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'sprite') throw new TypeError(`missing Sprite asset '${name}'`);
    this.blit(asset, x, y, 1, 0, false, false, state);
  }
  drawAnimation(name, frame, x, y, state) {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'animation' || asset.frames.length === 0)
      throw new TypeError(`missing Animation asset '${name}'`);
    const selected = asset.frames[wrap(frame, asset.frames.length)];
    if (selected !== void 0) this.blit(selected, x, y, 1, 0, false, false, state);
  }
  drawTransformedSprite(name, x, y, scale, quarterTurns, flipX, flipY, state) {
    const asset = this.assets.get(name);
    if (asset?.kind !== 'sprite') throw new TypeError(`missing Sprite asset '${name}'`);
    if (scale < 1 || scale > 16) throw new RangeError('sprite scale must be between 1 and 16');
    this.blit(asset, x, y, scale, wrap(quarterTurns, 4), flipX, flipY, state);
  }
  blit(sprite, x, y, scale, quarterTurns, flipX, flipY, state) {
    const outputWidth = (quarterTurns % 2 === 0 ? sprite.width : sprite.height) * scale;
    const outputHeight = (quarterTurns % 2 === 0 ? sprite.height : sprite.width) * scale;
    for (let outputY = 0; outputY < outputHeight; outputY += 1)
      for (let outputX = 0; outputX < outputWidth; outputX += 1) {
        let sourceX = Math.floor(outputX / scale);
        let sourceY = Math.floor(outputY / scale);
        [sourceX, sourceY] = unrotate(sourceX, sourceY, sprite.width, sprite.height, quarterTurns);
        if (flipX) sourceX = sprite.width - 1 - sourceX;
        if (flipY) sourceY = sprite.height - 1 - sourceY;
        const color = sprite.pixels[sourceY * sprite.width + sourceX] ?? HARDWARE.transparentColor;
        if (color !== this.transparency[0]) this.plot(x + outputX, y + outputY, color, state);
      }
  }
  drawMap(name, x, y, state) {
    const map = this.assets.get(name);
    if (map?.kind !== 'map') throw new TypeError(`missing Map asset '${name}'`);
    for (const layer of map.layers) {
      const tileSet = this.assets.get(layer.tileSet);
      if (tileSet?.kind !== 'tile_set')
        throw new TypeError(`missing TileSet asset '${layer.tileSet}'`);
      const clipRight = Math.min(HARDWARE.width, state.clipX + state.clipWidth);
      const clipBottom = Math.min(HARDWARE.height, state.clipY + state.clipHeight);
      const firstColumn = Math.max(
        0,
        Math.floor((state.clipX + state.cameraX - x) / HARDWARE.tileSize),
      );
      const lastColumn = Math.min(
        layer.width,
        Math.ceil((clipRight + state.cameraX - x) / HARDWARE.tileSize),
      );
      const firstRow = Math.max(
        0,
        Math.floor((state.clipY + state.cameraY - y) / HARDWARE.tileSize),
      );
      const lastRow = Math.min(
        layer.height,
        Math.ceil((clipBottom + state.cameraY - y) / HARDWARE.tileSize),
      );
      for (let row = firstRow; row < lastRow; row += 1)
        for (let column = firstColumn; column < lastColumn; column += 1) {
          const tile = tileSet.tiles[layer.cells.getUint16((row * layer.width + column) * 2, true)];
          if (tile !== void 0)
            this.blit(
              tile,
              x + column * HARDWARE.tileSize,
              y + row * HARDWARE.tileSize,
              1,
              0,
              false,
              false,
              state,
            );
        }
    }
  }
  print(text, x, y, color, state) {
    let cursorX = x;
    let cursorY = y;
    for (const character of text) {
      if (character === '\n') {
        cursorX = x;
        cursorY += BITMAP_FONT.advanceY;
        continue;
      }
      glyphRows(character).forEach((bits, row) => {
        for (let column = 0; column < BITMAP_FONT.glyphWidth; column += 1)
          if ((bits & (1 << (BITMAP_FONT.glyphWidth - 1 - column))) !== 0)
            this.plot(cursorX + column, cursorY + row, color, state);
      });
      cursorX += BITMAP_FONT.advanceX;
    }
  }
  printFont(name, text, x, y, color, state) {
    const font = this.assets.get(name);
    if (font?.kind !== 'font') throw new TypeError(`missing Font asset '${name}'`);
    let cursorX = x;
    let cursorY = y;
    for (const character of text) {
      if (character === '\n') {
        cursorX = x;
        cursorY += font.advanceY;
        continue;
      }
      const code = character.codePointAt(0) ?? font.missingGlyph;
      const glyph = font.glyphs.get(code) ?? font.glyphs.get(font.missingGlyph);
      if (glyph !== void 0) {
        for (let row = 0; row < font.glyphHeight; row += 1)
          for (let column = 0; column < font.glyphWidth; column += 1)
            if (glyph[row * font.glyphWidth + column] === 1)
              this.plot(cursorX + column, cursorY + row, color, state);
      }
      cursorX += font.advanceX;
    }
  }
};
/** Deterministic four-by-four Bayer choice between two palette indices. */
function orderedDither(x, y, first, second, level) {
  return ([0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5][wrap(y, 4) * 4 + wrap(x, 4)] ??
    0) < Math.max(0, Math.min(16, level))
    ? expectColor(second)
    : expectColor(first);
}
var MemoryDrawState = class {
  view;
  remap;
  constructor(bytes) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.remap = bytes.subarray(48);
  }
  capture() {
    return {
      cameraX: this.cameraX,
      cameraY: this.cameraY,
      clipX: this.clipX,
      clipY: this.clipY,
      clipWidth: this.clipWidth,
      clipHeight: this.clipHeight,
      remap: this.remap,
    };
  }
  get cameraX() {
    return this.view.getFloat64(0, true);
  }
  set cameraX(value) {
    this.view.setFloat64(0, value, true);
  }
  get cameraY() {
    return this.view.getFloat64(8, true);
  }
  set cameraY(value) {
    this.view.setFloat64(8, value, true);
  }
  get clipX() {
    return this.view.getFloat64(16, true);
  }
  set clipX(value) {
    this.view.setFloat64(16, value, true);
  }
  get clipY() {
    return this.view.getFloat64(24, true);
  }
  set clipY(value) {
    this.view.setFloat64(24, value, true);
  }
  get clipWidth() {
    return this.view.getFloat64(32, true);
  }
  set clipWidth(value) {
    this.view.setFloat64(32, value, true);
  }
  get clipHeight() {
    return this.view.getFloat64(40, true);
  }
  set clipHeight(value) {
    this.view.setFloat64(40, value, true);
  }
};
function copyDisplayConfiguration(display) {
  if (display === void 0)
    return {
      remap: identityRemap(),
      raster: [],
    };
  if (!validRemap(display.remap))
    throw new TypeError('display configuration has an invalid base remap');
  let previousLine = -1;
  const raster = display.raster.map((state) => {
    if (
      !Number.isInteger(state.line) ||
      state.line <= previousLine ||
      state.line >= HARDWARE.height ||
      !Number.isSafeInteger(state.scrollX) ||
      !Number.isSafeInteger(state.scrollY) ||
      state.scrollX < -32768 ||
      state.scrollX > 32767 ||
      state.scrollY < -32768 ||
      state.scrollY > 32767 ||
      !validRemap(state.remap)
    )
      throw new TypeError('display configuration has invalid raster state');
    previousLine = state.line;
    return {
      ...state,
      remap: state.remap.slice(),
    };
  });
  return {
    remap: display.remap.slice(),
    raster,
  };
}
function validRemap(remap) {
  return (
    remap.length === HARDWARE.rasterPaletteSize &&
    remap.every((color) => color < HARDWARE.paletteSize)
  );
}
function isRecord$10(value) {
  return typeof value === 'object' && value !== null;
}
function identityRemap() {
  return Uint8Array.from({ length: HARDWARE.rasterPaletteSize }, (_, index) => index);
}
function expectIntegers(command, count) {
  if (command.arguments.length !== count)
    throw new TypeError(
      `${command.name} expected ${String(count)} arguments, received ${String(command.arguments.length)}`,
    );
  return command.arguments.map(expectInteger$1);
}
function expectInteger$1(value) {
  if (typeof value !== 'number' || !Number.isSafeInteger(value))
    throw new TypeError('graphics arguments must be safe integers');
  return value;
}
function expectBoolean(value) {
  if (typeof value !== 'boolean') throw new TypeError('graphics argument must be Bool');
  return value;
}
function expectText(value) {
  if (typeof value !== 'string') throw new TypeError('graphics argument must be Text');
  return value;
}
function expectColor(value) {
  if (value < 0 || value >= HARDWARE.paletteSize)
    throw new RangeError('palette index must be between 0 and 255');
  return value;
}
function expectRemappableColor(value) {
  if (value < 0 || value >= HARDWARE.rasterPaletteSize)
    throw new RangeError('palette remap source must be between 0 and 31');
  return value;
}
function readAssetName(value, expectedKind) {
  if (
    typeof value !== 'object' ||
    value === null ||
    !('name' in value) ||
    typeof value.name !== 'string' ||
    !('kind' in value) ||
    value.kind !== expectedKind
  )
    throw new TypeError(`expected a ${expectedKind} asset handle`);
  return value.name;
}
function edge(firstX, firstY, secondX, secondY, pointX, pointY) {
  return (pointX - firstX) * (secondY - firstY) - (pointY - firstY) * (secondX - firstX);
}
function unrotate(x, y, width, height, quarterTurns) {
  switch (quarterTurns) {
    case 1:
      return [y, height - 1 - x];
    case 2:
      return [width - 1 - x, height - 1 - y];
    case 3:
      return [width - 1 - y, x];
    default:
      return [x, y];
  }
}
function wrap(value, modulus) {
  return ((value % modulus) + modulus) % modulus;
}
function clampInt16(value) {
  return Math.max(-32768, Math.min(32767, value));
}
//#endregion
//#region src/asset-codec.ts
/** Bounded, data-only source bank sent to the restricted Worker for authoritative decoding. */
function isRuntimeAssetSource(value) {
  if (
    !isRecord$9(value) ||
    !isRecord$9(value.declarations) ||
    !isRecord$9(value.files) ||
    Array.isArray(value.declarations) ||
    Array.isArray(value.files) ||
    Object.keys(value).some((key) => !['declarations', 'files', 'displayPath'].includes(key)) ||
    Object.keys(value.declarations).length > 4096 ||
    Object.keys(value.files).length > 4096 ||
    (value.displayPath !== void 0 &&
      value.displayPath !== null &&
      !canonicalAssetPath(value.displayPath))
  )
    return false;
  let bytes = typeof value.displayPath === 'string' ? value.displayPath.length : 0;
  for (const [path, data] of Object.entries(value.files)) {
    if (!canonicalAssetPath(path) || !(data instanceof Uint8Array)) return false;
    bytes += path.length + data.byteLength;
    if (bytes > 16777216) return false;
  }
  for (const [name, declaration] of Object.entries(value.declarations)) {
    if (
      !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name) ||
      !isRecord$9(declaration) ||
      Object.keys(declaration).length !== 2 ||
      !canonicalAssetPath(declaration.path) ||
      typeof declaration.kind !== 'string' ||
      !['sprite', 'animation', 'tile_set', 'map', 'font', 'sound', 'music'].includes(
        declaration.kind,
      )
    )
      return false;
    bytes += name.length + declaration.path.length + declaration.kind.length;
    if (bytes > 16777216) return false;
  }
  return true;
}
function canonicalAssetPath(value) {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    value.length <= 1024 &&
    value
      .split('/')
      .every((part) => /^[A-Za-z0-9_.-]+$/.test(part) && part !== '.' && part !== '..')
  );
}
/** Decodes documented JSON asset files into validated hardware stores and worker map views. */
function decodeRuntimeAssets(declarations, files, displayPath) {
  const visual = [];
  const audio = [];
  for (const [name, declaration] of Object.entries(declarations).sort(([left], [right]) =>
    left.localeCompare(right),
  )) {
    const bytes = files[declaration.path];
    if (bytes === void 0) throw new TypeError(`asset '${name}' is missing '${declaration.path}'`);
    const value = JSON.parse(new TextDecoder().decode(bytes));
    switch (declaration.kind) {
      case 'sprite':
        visual.push(decodeSprite(name, value, false));
        break;
      case 'animation':
        visual.push(decodeSprite(name, value, true));
        break;
      case 'tile_set':
        visual.push(decodeTileSet(name, value));
        break;
      case 'map':
        visual.push(decodeMap(name, value));
        break;
      case 'sound':
        audio.push(decodeSound(name, value));
        break;
      case 'music':
        audio.push(decodeMusic(name, value));
        break;
      case 'font':
        visual.push(decodeFont(name, value));
    }
  }
  const visualStore = new VisualAssetStore(visual);
  new AudioAssetStore(audio);
  const tileSets = new Map(
    visual.filter((asset) => asset.kind === 'tile_set').map((asset) => [asset.name, asset]),
  );
  const maps = visual
    .filter((asset) => asset.kind === 'map')
    .map((asset) => ({
      name: asset.name,
      layers: asset.layers.map((layer) => {
        const tileSet = tileSets.get(layer.tileSet);
        if (tileSet === void 0)
          throw new TypeError(`map '${asset.name}' references missing tile set '${layer.tileSet}'`);
        return {
          width: layer.width,
          height: layer.height,
          cells: layer.cells.slice(),
          tileFlags: tileSet.flags.slice(),
        };
      }),
    }));
  const display = decodeDisplay(displayPath, files);
  const visualBytes = visualStore.usedBytes + displayBytes(display);
  if (visualBytes > HARDWARE.visualCapacityBytes)
    throw new RangeError('visual assets exceed the 1 MiB shared capacity');
  return {
    visual,
    audio,
    maps,
    ...(display === void 0 ? {} : { display }),
    visualBytes,
  };
}
function decodeSprite(name, value, animation) {
  if (
    !isRecord$9(value) ||
    (value.revision !== 1 && value.revision !== 2) ||
    value.kind !== 'sprite' ||
    !boundedInteger(value.width, 1, value.revision === 1 ? 64 : HARDWARE.spriteMaximumAxis) ||
    !boundedInteger(value.height, 1, value.revision === 1 ? 64 : HARDWARE.spriteMaximumAxis) ||
    !Array.isArray(value.frames) ||
    value.frames.length === 0 ||
    value.frames.length > 256
  )
    throw new TypeError(`sprite asset '${name}' is invalid`);
  const pixelCount = value.width * value.height;
  const frames = value.frames.map((frame) => {
    if (!isNumberArray(frame, pixelCount, 0, value.revision === 1 ? 31 : 255))
      throw new TypeError(`sprite asset '${name}' has invalid indexed pixels`);
    return {
      kind: 'sprite',
      name,
      width: value.width,
      height: value.height,
      pixels: Uint8Array.from(frame),
    };
  });
  const first = frames[0];
  if (first === void 0) throw new TypeError(`sprite asset '${name}' requires a frame`);
  return animation
    ? {
        kind: 'animation',
        name,
        frames,
      }
    : {
        ...first,
        name,
      };
}
function decodeTileSet(name, value) {
  if (
    !isRecord$9(value) ||
    (value.revision !== 1 && value.revision !== 2) ||
    value.kind !== 'tile_set' ||
    !Array.isArray(value.tiles) ||
    value.tiles.length === 0 ||
    value.tiles.length > 4096 ||
    !isNumberArray(value.flags, value.tiles.length, 0, 255)
  )
    throw new TypeError(`tile-set asset '${name}' is invalid`);
  return {
    kind: 'tile_set',
    name,
    tiles: value.tiles.map((pixels, index) => {
      if (!isNumberArray(pixels, 64, 0, value.revision === 1 ? 31 : 255))
        throw new TypeError(`tile ${String(index)} in '${name}' has invalid indexed pixels`);
      return {
        kind: 'sprite',
        name: `${name}:${String(index)}`,
        width: 8,
        height: 8,
        pixels: Uint8Array.from(pixels),
      };
    }),
    flags: Uint8Array.from(value.flags),
  };
}
function decodeMap(name, value) {
  if (
    !isRecord$9(value) ||
    value.revision !== 1 ||
    value.kind !== 'map' ||
    !Array.isArray(value.layers) ||
    value.layers.length === 0 ||
    value.layers.length > 8
  )
    throw new TypeError(`map asset '${name}' is invalid`);
  return {
    kind: 'map',
    name,
    layers: value.layers.map((layer) => {
      if (
        !isRecord$9(layer) ||
        !boundedInteger(layer.width, 1, 256) ||
        !boundedInteger(layer.height, 1, 256) ||
        typeof layer.tileSet !== 'string' ||
        !isNumberArray(layer.cells, layer.width * layer.height, 0, 65535)
      )
        throw new TypeError(`map asset '${name}' has an invalid layer`);
      return {
        width: layer.width,
        height: layer.height,
        cells: Uint16Array.from(layer.cells),
        tileSet: layer.tileSet,
      };
    }),
  };
}
function decodeFont(name, value) {
  if (
    !isRecord$9(value) ||
    value.revision !== 1 ||
    value.kind !== 'font' ||
    !boundedInteger(value.glyphWidth, 1, 16) ||
    !boundedInteger(value.glyphHeight, 1, 16) ||
    !boundedInteger(value.baseline, 0, value.glyphHeight - 1) ||
    !boundedInteger(value.advanceX, 1, 32) ||
    !boundedInteger(value.advanceY, 1, 32) ||
    !boundedInteger(value.missingGlyph, 0, 255) ||
    !Array.isArray(value.glyphs) ||
    value.glyphs.length === 0 ||
    value.glyphs.length > 256
  )
    throw new TypeError(`font asset '${name}' is invalid`);
  const glyphs = /* @__PURE__ */ new Map();
  let previous = -1;
  for (const glyph of value.glyphs) {
    if (
      !isRecord$9(glyph) ||
      !boundedInteger(glyph.code, 0, 255) ||
      glyph.code <= previous ||
      !isNumberArray(glyph.pixels, value.glyphWidth * value.glyphHeight, 0, 1)
    )
      throw new TypeError(`font asset '${name}' has an invalid glyph map`);
    previous = glyph.code;
    glyphs.set(glyph.code, Uint8Array.from(glyph.pixels));
  }
  if (!glyphs.has(value.missingGlyph))
    throw new TypeError(`font asset '${name}' is missing its fallback glyph`);
  return {
    kind: 'font',
    name,
    glyphWidth: value.glyphWidth,
    glyphHeight: value.glyphHeight,
    baseline: value.baseline,
    advanceX: value.advanceX,
    advanceY: value.advanceY,
    missingGlyph: value.missingGlyph,
    glyphs,
  };
}
function decodeSound(name, value) {
  if (!isRecord$9(value) || value.revision !== 1 || value.kind !== 'sound')
    throw new TypeError(`sound asset '${name}' is invalid`);
  const sound = {
    ...value,
    name,
  };
  new AudioAssetStore([sound]);
  return sound;
}
function decodeMusic(name, value) {
  if (!isRecord$9(value) || value.revision !== 1 || value.kind !== 'music')
    throw new TypeError(`music asset '${name}' is invalid`);
  return {
    ...value,
    name,
  };
}
function decodeDisplay(path, files) {
  if (path === void 0 || path === null) return void 0;
  const bytes = files[path];
  if (bytes === void 0) throw new TypeError(`display configuration is missing '${path}'`);
  const value = JSON.parse(new TextDecoder().decode(bytes));
  if (
    !isRecord$9(value) ||
    (value.revision !== 1 && value.revision !== 2) ||
    value.kind !== 'display' ||
    !isNumberArray(value.remap, HARDWARE.rasterPaletteSize, 0, value.revision === 1 ? 31 : 255) ||
    !Array.isArray(value.raster) ||
    value.raster.length > HARDWARE.height
  )
    throw new TypeError('display configuration is invalid');
  let previousLine = -1;
  const raster = value.raster.map((state) => {
    if (
      !isRecord$9(state) ||
      !boundedInteger(state.line, 0, HARDWARE.height - 1) ||
      state.line <= previousLine ||
      !boundedInteger(state.scrollX, -32768, 32767) ||
      !boundedInteger(state.scrollY, -32768, 32767) ||
      !isNumberArray(state.remap, HARDWARE.rasterPaletteSize, 0, value.revision === 1 ? 31 : 255)
    )
      throw new TypeError('display configuration has invalid raster state');
    previousLine = state.line;
    return {
      line: state.line,
      scrollX: state.scrollX,
      scrollY: state.scrollY,
      remap: Uint8Array.from(state.remap),
    };
  });
  return {
    remap: Uint8Array.from(value.remap),
    raster,
  };
}
function displayBytes(display) {
  return display === void 0
    ? 0
    : HARDWARE.rasterPaletteSize + display.raster.length * (HARDWARE.rasterPaletteSize + 6);
}
function isNumberArray(value, length, minimum, maximum) {
  return (
    Array.isArray(value) &&
    value.length === length &&
    value.every((entry) => Number.isSafeInteger(entry) && entry >= minimum && entry <= maximum)
  );
}
function boundedInteger(value, minimum, maximum) {
  return (
    Number.isSafeInteger(value) && typeof value === 'number' && value >= minimum && value <= maximum
  );
}
function isRecord$9(value) {
  return typeof value === 'object' && value !== null;
}
//#endregion
//#region src/budget.ts
function isWorkBudgetSnapshot(value) {
  if (
    !isRecord$8(value) ||
    Object.keys(value).length !== 4 ||
    value.revision !== 1 ||
    typeof value.limit !== 'number' ||
    !Number.isSafeInteger(value.limit) ||
    value.limit < 1 ||
    value.limit > HARDWARE.workUnitsPerFrame ||
    typeof value.used !== 'number' ||
    !Number.isSafeInteger(value.used) ||
    value.used < 0 ||
    !Array.isArray(value.attribution) ||
    value.attribution.length > HARDWARE.workUnitsPerFrame + 1
  )
    return false;
  let total = 0;
  let previousUnits = Infinity;
  let previousStart = -1;
  const spans = /* @__PURE__ */ new Set();
  for (const entry of value.attribution) {
    if (
      !isRecord$8(entry) ||
      Object.keys(entry).length !== 2 ||
      typeof entry.units !== 'number' ||
      !Number.isSafeInteger(entry.units) ||
      entry.units < 0 ||
      entry.units > value.used - total ||
      !isRecord$8(entry.sourceSpan) ||
      Object.keys(entry.sourceSpan).length !== 2 ||
      typeof entry.sourceSpan.start !== 'number' ||
      !Number.isSafeInteger(entry.sourceSpan.start) ||
      entry.sourceSpan.start < 0 ||
      typeof entry.sourceSpan.end !== 'number' ||
      !Number.isSafeInteger(entry.sourceSpan.end) ||
      entry.sourceSpan.end < entry.sourceSpan.start ||
      entry.units > previousUnits ||
      (entry.units === previousUnits && entry.sourceSpan.start < previousStart)
    )
      return false;
    const key = `${String(entry.sourceSpan.start)}:${String(entry.sourceSpan.end)}`;
    if (spans.has(key)) return false;
    spans.add(key);
    total += entry.units;
    previousUnits = entry.units;
    previousStart = entry.sourceSpan.start;
  }
  return total === value.used;
}
function isRecord$8(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
/** Per-frame synthetic execution budget and source-span attribution. */
var WorkBudget = class {
  frameLimit;
  usedUnits = 0;
  bySpan = /* @__PURE__ */ new Map();
  constructor(frameLimit) {
    if (
      !Number.isSafeInteger(frameLimit) ||
      frameLimit <= 0 ||
      frameLimit > HARDWARE.workUnitsPerFrame
    )
      throw new RangeError('work-unit limit must be an integer between 1 and 50,000');
    this.frameLimit = frameLimit;
  }
  get limit() {
    return this.frameLimit;
  }
  get used() {
    return this.usedUnits;
  }
  beginFrame() {
    this.usedUnits = 0;
    this.bySpan.clear();
  }
  snapshot() {
    return structuredClone({
      revision: 1,
      limit: this.frameLimit,
      used: this.usedUnits,
      attribution: this.attribution(),
    });
  }
  restore(value) {
    if (!isWorkBudgetSnapshot(value) || value.limit !== this.frameLimit)
      throw new TypeError('invalid or mismatched work-budget snapshot');
    const entries = structuredClone(value.attribution);
    this.usedUnits = value.used;
    this.bySpan.clear();
    for (const entry of entries)
      this.bySpan.set(`${String(entry.sourceSpan.start)}:${String(entry.sourceSpan.end)}`, entry);
  }
  charge(units, sourceSpan) {
    if (!Number.isInteger(units) || units < 0)
      throw new RangeError('work-unit charge must be a non-negative finite integer');
    const charged = Math.min(units, Number.MAX_SAFE_INTEGER - this.usedUnits);
    this.usedUnits += charged;
    const key = `${String(sourceSpan.start)}:${String(sourceSpan.end)}`;
    const previous = this.bySpan.get(key);
    this.bySpan.set(key, {
      sourceSpan,
      units: (previous?.units ?? 0) + charged,
    });
    if (charged !== units || this.usedUnits > this.frameLimit)
      throw new BudgetExceeded(this.usedUnits, this.frameLimit, sourceSpan);
  }
  attribution() {
    return [...this.bySpan.values()].sort(
      (left, right) => right.units - left.units || left.sourceSpan.start - right.sourceSpan.start,
    );
  }
};
//#endregion
//#region src/input.ts
var BUTTONS = ['up', 'down', 'left', 'right', 'a', 'b', 'x', 'y', 'l', 'r', 'start', 'menu'];
function emptyInputFrame() {
  const controller = () => ({
    buttons: Object.fromEntries(BUTTONS.map((button) => [button, false])),
  });
  return {
    controllers: [controller(), controller(), controller(), controller()],
    pointer: {
      x: 0,
      y: 0,
      primary: false,
      secondary: false,
      inside: false,
    },
  };
}
/** Little-endian controller/pointer MMIO over the same frames used by the high-level API. */
function inputRegisterByte(current, previous, offset) {
  if (offset < 0 || offset >= 48 || !Number.isInteger(offset)) return 0;
  if (offset < 32) {
    const port = Math.floor(offset / 8);
    const mask = (frame) =>
      BUTTONS.reduce(
        (bits, button, bit) => bits | (frame.controllers[port]?.buttons[button] ? 1 << bit : 0),
        0,
      );
    const held = mask(current);
    const before = mask(previous);
    const field = offset % 8;
    return (
      ((field < 2 ? held : field < 4 ? before : field < 6 ? held & ~before : before & ~held) >>>
        ((offset % 2) * 8)) &
      255
    );
  }
  const field = offset - 32;
  if (field < 8) {
    const pointer = field < 4 ? current.pointer : previous.pointer;
    return ((field % 4 < 2 ? pointer.x : pointer.y) >>> ((field % 2) * 8)) & 255;
  }
  const flags = (pointer) =>
    Number(pointer.primary) | (Number(pointer.secondary) << 1) | (Number(pointer.inside) << 2);
  const held = flags(current.pointer);
  const before = flags(previous.pointer);
  return field === 8
    ? held
    : field === 9
      ? before
      : field === 10
        ? held & ~before
        : field === 11
          ? before & ~held
          : 0;
}
function isButton(value) {
  return typeof value === 'string' && BUTTONS.includes(value);
}
function isInputFrame(value) {
  if (
    !isRecord$7(value) ||
    !hasExactKeys$3(value, ['controllers', 'pointer']) ||
    !Array.isArray(value.controllers) ||
    value.controllers.length !== 4
  )
    return false;
  if (
    Object.keys(value.controllers).length !== 4 ||
    !Array.from(value.controllers).every(isControllerState) ||
    !isRecord$7(value.pointer)
  )
    return false;
  const pointer = value.pointer;
  return (
    hasExactKeys$3(pointer, ['x', 'y', 'primary', 'secondary', 'inside']) &&
    typeof pointer.x === 'number' &&
    Number.isSafeInteger(pointer.x) &&
    pointer.x >= 0 &&
    pointer.x < HARDWARE.width &&
    typeof pointer.y === 'number' &&
    Number.isSafeInteger(pointer.y) &&
    pointer.y >= 0 &&
    pointer.y < HARDWARE.height &&
    typeof pointer.primary === 'boolean' &&
    typeof pointer.secondary === 'boolean' &&
    typeof pointer.inside === 'boolean'
  );
}
Object.freeze({
  ArrowUp: [0, 'up'],
  ArrowDown: [0, 'down'],
  ArrowLeft: [0, 'left'],
  ArrowRight: [0, 'right'],
  KeyZ: [0, 'a'],
  KeyX: [0, 'b'],
  KeyA: [0, 'x'],
  KeyS: [0, 'y'],
  KeyQ: [0, 'l'],
  KeyW: [0, 'r'],
  Enter: [0, 'start'],
  Escape: [0, 'menu'],
  KeyI: [1, 'up'],
  KeyK: [1, 'down'],
  KeyJ: [1, 'left'],
  KeyL: [1, 'right'],
  KeyF: [1, 'a'],
  KeyG: [1, 'b'],
  KeyR: [1, 'x'],
  KeyT: [1, 'y'],
  KeyV: [1, 'l'],
  KeyB: [1, 'r'],
  Digit1: [1, 'start'],
  Backquote: [1, 'menu'],
  Numpad8: [2, 'up'],
  Numpad5: [2, 'down'],
  Numpad4: [2, 'left'],
  Numpad6: [2, 'right'],
  Numpad1: [2, 'a'],
  Numpad2: [2, 'b'],
  Numpad7: [2, 'x'],
  Numpad9: [2, 'y'],
  NumpadAdd: [2, 'l'],
  NumpadSubtract: [2, 'r'],
  NumpadEnter: [2, 'start'],
  NumpadDecimal: [2, 'menu'],
  KeyY: [3, 'up'],
  KeyH: [3, 'down'],
  KeyU: [3, 'left'],
  KeyO: [3, 'right'],
  KeyC: [3, 'a'],
  KeyD: [3, 'b'],
  KeyE: [3, 'x'],
  KeyM: [3, 'y'],
  KeyN: [3, 'l'],
  KeyP: [3, 'r'],
  Digit2: [3, 'start'],
  Digit3: [3, 'menu'],
});
Object.freeze({
  up: 12,
  down: 13,
  left: 14,
  right: 15,
  a: 0,
  b: 1,
  x: 2,
  y: 3,
  l: 4,
  r: 5,
  start: 9,
  menu: 8,
});
function isControllerState(value) {
  if (!isRecord$7(value) || !hasExactKeys$3(value, ['buttons']) || !isRecord$7(value.buttons))
    return false;
  const buttons = value.buttons;
  return (
    hasExactKeys$3(buttons, BUTTONS) &&
    BUTTONS.every((button) => typeof buttons[button] === 'boolean')
  );
}
function isRecord$7(value) {
  return typeof value === 'object' && value !== null;
}
function hasExactKeys$3(value, expected) {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => keys.includes(key));
}
//#endregion
//#region src/rng.ts
var NON_ZERO_FALLBACK = 1295003697;
/** Console-owned xorshift32 stream with an explicit serializable state. */
var DeterministicRng = class {
  current;
  constructor(seed = NON_ZERO_FALLBACK) {
    this.current = normalizeSeed(seed);
  }
  get state() {
    return this.current >>> 0;
  }
  restore(state) {
    this.current = normalizeSeed(state);
  }
  nextU32() {
    let value = this.current >>> 0;
    value ^= value << 13;
    value ^= value >>> 17;
    value ^= value << 5;
    this.current = value >>> 0;
    return this.current;
  }
  nextNum() {
    return this.nextU32() / 4294967296;
  }
  nextInt(minimum, maximumExclusive) {
    if (
      !Number.isSafeInteger(minimum) ||
      !Number.isSafeInteger(maximumExclusive) ||
      maximumExclusive <= minimum
    )
      throw new RangeError(
        'rng_int bounds must be safe integers with maximum greater than minimum',
      );
    const range = maximumExclusive - minimum;
    if (range > 4294967296) throw new RangeError('rng_int range must not exceed 2^32');
    const rejectionLimit = 4294967296 - (4294967296 % range);
    let sample;
    do sample = this.nextU32();
    while (sample >= rejectionLimit);
    return minimum + (sample % range);
  }
};
function normalizeSeed(seed) {
  if (!Number.isSafeInteger(seed)) throw new RangeError('RNG seed must be a safe integer');
  const normalized = seed >>> 0;
  return normalized === 0 ? NON_ZERO_FALLBACK : normalized;
}
//#endregion
//#region src/system.ts
var EXECUTION_PHASES = ['idle', 'start', 'update', 'draw', 'raster', 'output'];
function isExecutionSnapshot(value, frame, rate, budget) {
  if (
    !isRecord$6(value) ||
    Object.keys(value).length !== 5 ||
    typeof value.booted !== 'boolean' ||
    typeof value.updates !== 'number' ||
    !Number.isSafeInteger(value.updates) ||
    value.updates < 0 ||
    typeof value.phase !== 'string' ||
    !EXECUTION_PHASES.includes(value.phase) ||
    (value.phase === 'raster'
      ? typeof value.rasterLine !== 'number' ||
        !Number.isInteger(value.rasterLine) ||
        value.rasterLine < 0 ||
        value.rasterLine >= HARDWARE.height
      : value.rasterLine !== null) ||
    (value.fault !== null && !isMachineFault(value.fault))
  )
    return false;
  const base = rate === 60 ? frame : Math.ceil(frame / 2);
  const updated =
    (value.phase === 'draw' || value.phase === 'raster' || value.phase === 'output') &&
    (rate === 60 || frame % 2 === 0);
  if (
    value.updates !== base + Number(updated) ||
    (value.phase !== 'idle' && frame === Number.MAX_SAFE_INTEGER) ||
    (!value.booted && (frame !== 0 || (value.phase !== 'idle' && value.phase !== 'start'))) ||
    (value.phase === 'start' && value.booted) ||
    (value.phase === 'update' && rate === 30 && frame % 2 !== 0)
  )
    return false;
  return value.fault !== null || (value.phase === 'idle' && budget.used <= budget.limit);
}
function isMachineFault(value) {
  return (
    isRecord$6(value) &&
    Object.keys(value).length === 2 &&
    typeof value.code === 'number' &&
    Number.isInteger(value.code) &&
    value.code >= 9e3 &&
    value.code <= 9999 &&
    isFaultSpan(value.sourceSpan)
  );
}
function isFaultSpan(value) {
  return (
    isRecord$6(value) &&
    Object.keys(value).length === 2 &&
    typeof value.start === 'number' &&
    Number.isInteger(value.start) &&
    value.start >= 0 &&
    typeof value.end === 'number' &&
    Number.isInteger(value.end) &&
    value.end >= value.start &&
    value.end <= 4294967295
  );
}
function isRecord$6(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
/** Wire encoding of current device-owned state, not a retained register image. */
function systemRegisterByte(state, offset) {
  if (!Number.isInteger(offset) || offset < 0 || offset >= 64) return 0;
  const byte = (value, index) => Math.floor(value / 2 ** (index * 8)) & 255;
  if (offset < 8) return byte(state.frame, offset);
  if (offset < 16) return byte(state.updates, offset - 8);
  if (offset < 24) {
    const encoded = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(8));
    encoded.setFloat64(0, state.seconds, true);
    return encoded.getUint8(offset - 16);
  }
  if (offset < 28) return byte(state.rngState, offset - 24);
  if (offset === 28) return state.updateRate;
  if (offset === 29) return EXECUTION_PHASES.indexOf(state.phase);
  if (offset < 32) return byte(state.rasterLine ?? 65535, offset - 30);
  if (offset < 40) return byte(state.used, offset - 32);
  if (offset < 48) return byte(state.limit, offset - 40);
  if (offset === 48)
    return (
      Number(state.booted) |
      (Number(state.phase !== 'idle' && state.fault === null) << 1) |
      (Number(state.fault !== null) << 2)
    );
  if (offset < 52) return 0;
  if (offset < 54) return byte(state.fault?.code ?? 0, offset - 52);
  if (offset < 56) return 0;
  if (offset < 60) return byte(state.fault?.sourceSpan.start ?? 0, offset - 56);
  return byte(state.fault?.sourceSpan.end ?? 0, offset - 60);
}
//#endregion
//#region src/machine.ts
/** Deterministic callback scheduler and the only API surface visible to generated cartridge code. */
var DeterministicMachine = class {
  budget;
  rng;
  updateRate;
  cartridge;
  hooks;
  debugEnabled;
  currentFrame = 0;
  booted = false;
  input = emptyInputFrame();
  previousInput = emptyInputFrame();
  phase = 'idle';
  rasterLine;
  completedUpdates = 0;
  lastFault = null;
  debugInput;
  constructor(factory, configuration, hooks = {}) {
    if (![30, 60].includes(configuration.updateRate))
      throw new RangeError('update rate must be 30 or 60 Hz');
    this.budget = new WorkBudget(configuration.workUnitsPerFrame);
    this.rng = new DeterministicRng(configuration.seed);
    this.updateRate = configuration.updateRate;
    this.debugEnabled = configuration.debug === true;
    this.hooks = hooks;
    this.cartridge = factory(this);
  }
  get frame() {
    return this.currentFrame;
  }
  get cartridgeTimeSeconds() {
    return this.currentFrame / 60;
  }
  boot() {
    this.assertRunnable();
    if (this.booted) return;
    this.budget.beginFrame();
    this.phase = 'start';
    this.rasterLine = void 0;
    try {
      this.completeDebugCallback(() => this.cartridge.start());
      this.booted = true;
      this.phase = 'idle';
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }
  runFrame(input) {
    if (!isInputFrame(input))
      throw new RuntimeFault('M019008', 'invalid controller input frame', {
        start: 0,
        end: 0,
      });
    this.assertRunnable();
    if (this.debugEnabled)
      for (;;) {
        const step = this.stepDebug(input);
        if (step.report !== void 0) return step.report;
      }
    if (!this.booted) this.boot();
    this.previousInput = this.input;
    this.input = structuredClone(input);
    this.budget.beginFrame();
    try {
      if (this.updateRate === 60 || this.currentFrame % 2 === 0) {
        this.phase = 'update';
        this.completeDebugCallback(() => this.cartridge.update());
        this.completedUpdates += 1;
      }
      this.phase = 'draw';
      this.completeDebugCallback(() => this.cartridge.draw());
      for (let line = 0; line < 144; line += 1) {
        this.phase = 'raster';
        this.rasterLine = line;
        this.completeDebugCallback(() => this.cartridge.raster(line));
      }
      this.rasterLine = void 0;
      this.phase = 'output';
      this.hooks.completeFrame?.();
      const report = {
        frame: this.currentFrame,
        workUnits: this.budget.used,
        attribution: this.budget.attribution(),
      };
      this.currentFrame += 1;
      this.phase = 'idle';
      return report;
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }
  /** Advances a debug build to exactly one statement boundary or one completed frame. */
  stepDebug(input) {
    if (!this.debugEnabled)
      throw new RuntimeFault('M019015', 'statement stepping requires a debug cartridge', {
        start: 0,
        end: 0,
      });
    if (!isInputFrame(input))
      throw new RuntimeFault('M019008', 'invalid controller input frame', {
        start: 0,
        end: 0,
      });
    if (this.lastFault !== null) this.assertRunnable();
    try {
      if (this.phase === 'idle') {
        this.debugInput = structuredClone(input);
        this.budget.beginFrame();
        if (!this.booted) this.phase = 'start';
        else this.beginDebugFrame();
      }
      for (;;) {
        const step = this.stepDebugPhase();
        if (step.event !== void 0) {
          this.probe(step.event.id, step.event.sourceSpan, step.event.locals);
          return { event: structuredClone(step.event) };
        }
        if (!step.done) throw new TypeError('invalid generated debug step');
        const boundary = this.advanceDebugPhase();
        if (boundary === 'booted') return { booted: true };
        if (boundary !== void 0) return { report: boundary };
      }
    } catch (error) {
      this.rememberFault(error);
      throw error;
    }
  }
  beginDebugFrame() {
    const input = this.debugInput ?? emptyInputFrame();
    this.previousInput = this.input;
    this.input = structuredClone(input);
    this.phase = this.updateRate === 60 || this.currentFrame % 2 === 0 ? 'update' : 'draw';
  }
  stepDebugPhase() {
    switch (this.phase) {
      case 'start':
        return this.cartridge.start() ?? { done: true };
      case 'update':
        return this.cartridge.update() ?? { done: true };
      case 'draw':
        return this.cartridge.draw() ?? { done: true };
      case 'raster':
        return this.cartridge.raster(this.rasterLine ?? 0) ?? { done: true };
      case 'idle':
      case 'output':
        throw new TypeError(`cannot step machine phase ${this.phase}`);
    }
  }
  advanceDebugPhase() {
    switch (this.phase) {
      case 'start':
        this.booted = true;
        this.phase = 'idle';
        this.debugInput = void 0;
        return 'booted';
      case 'update':
        this.completedUpdates += 1;
        this.phase = 'draw';
        return;
      case 'draw':
        this.phase = 'raster';
        this.rasterLine = 0;
        return;
      case 'raster': {
        if ((this.rasterLine ?? 0) < 143) {
          this.rasterLine = (this.rasterLine ?? 0) + 1;
          return;
        }
        this.rasterLine = void 0;
        this.phase = 'output';
        this.hooks.completeFrame?.();
        const report = {
          frame: this.currentFrame,
          workUnits: this.budget.used,
          attribution: this.budget.attribution(),
        };
        this.currentFrame += 1;
        this.phase = 'idle';
        this.debugInput = void 0;
        return report;
      }
      case 'idle':
      case 'output':
        throw new TypeError(`cannot advance machine phase ${this.phase}`);
    }
  }
  completeDebugCallback(callback) {
    for (;;) {
      const step = callback();
      if (step === void 0 || step.done) return;
      if (step.event !== void 0)
        this.probe(step.event.id, step.event.sourceSpan, step.event.locals);
    }
  }
  snapshot() {
    if (this.phase !== 'idle' && this.lastFault === null)
      throw new TypeError('machine snapshots require a completed frame or fault boundary');
    return structuredClone({
      revision: 2,
      frame: this.currentFrame,
      rngState: this.rng.state,
      cartridge: this.cartridge.snapshot(),
      input: this.input,
      previousInput: this.previousInput,
      updateRate: this.updateRate,
      budget: this.budget.snapshot(),
      execution: {
        booted: this.booted,
        updates: this.completedUpdates,
        phase: this.phase,
        rasterLine: this.rasterLine ?? null,
        fault: this.lastFault,
      },
    });
  }
  restore(value) {
    if (!isMachineSnapshot(value)) throw new TypeError('invalid MOD-01 machine snapshot');
    const snapshot = value;
    if (
      snapshot.revision === 2 &&
      (snapshot.updateRate !== this.updateRate || snapshot.budget.limit !== this.budget.limit)
    )
      throw new TypeError('machine snapshot does not match the execution configuration');
    const previous = structuredClone(this.cartridge.snapshot());
    try {
      this.cartridge.restore(structuredClone(snapshot.cartridge));
    } catch (error) {
      this.cartridge.restore(previous);
      throw error;
    }
    if (snapshot.revision === 2) {
      this.budget.restore(snapshot.budget);
      this.booted = snapshot.execution.booted;
      this.completedUpdates = snapshot.execution.updates;
      this.phase = snapshot.execution.phase;
      this.rasterLine = snapshot.execution.rasterLine ?? void 0;
      this.lastFault = structuredClone(snapshot.execution.fault);
    } else {
      this.budget.beginFrame();
      this.booted = this.booted || snapshot.frame > 0;
      this.completedUpdates =
        this.updateRate === 60 ? snapshot.frame : Math.ceil(snapshot.frame / 2);
      this.phase = 'idle';
      this.rasterLine = void 0;
      this.lastFault = null;
    }
    this.currentFrame = snapshot.frame;
    this.rng.restore(snapshot.rngState);
    this.input = structuredClone(snapshot.input);
    this.previousInput = structuredClone(snapshot.previousInput);
    this.debugInput = void 0;
  }
  inspect() {
    return this.cartridge.inspect();
  }
  readInputByte(offset) {
    return inputRegisterByte(this.input, this.previousInput, offset);
  }
  readSystemByte(offset) {
    return systemRegisterByte(
      {
        frame: this.currentFrame,
        updates: this.completedUpdates,
        seconds: this.cartridgeTimeSeconds,
        rngState: this.rng.state,
        updateRate: this.updateRate,
        phase: this.phase,
        rasterLine: this.rasterLine,
        booted: this.booted,
        fault: this.lastFault,
        used: this.budget.used,
        limit: this.budget.limit,
      },
      offset,
    );
  }
  assertRunnable() {
    if (this.lastFault !== null)
      throw new RuntimeFault(
        'M019014',
        'cartridge faulted; restart or restore a healthy checkpoint',
        this.lastFault.sourceSpan,
      );
    if (this.phase !== 'idle')
      throw new RuntimeFault('M019014', 'cartridge is already executing', {
        start: 0,
        end: 0,
      });
    if (this.currentFrame === Number.MAX_SAFE_INTEGER)
      this.fault('M019012', 'display-frame counter is exhausted', {
        start: 0,
        end: 0,
      });
  }
  work(units, sourceSpan) {
    try {
      this.budget.charge(units, sourceSpan);
    } catch (error) {
      this.rememberFault(error, sourceSpan);
      throw error;
    }
  }
  call(name, arguments_, sourceSpan) {
    switch (name) {
      case 'rng_num':
        expectArguments(name, arguments_, 0, sourceSpan);
        return this.rng.nextNum();
      case 'rng_int': {
        expectArguments(name, arguments_, 2, sourceSpan);
        const minimum = expectInteger(arguments_[0], sourceSpan);
        const maximum = expectInteger(arguments_[1], sourceSpan);
        try {
          return this.rng.nextInt(minimum, maximum);
        } catch (error) {
          return this.fault(
            'M019007',
            error instanceof Error ? error.message : 'invalid RNG bounds',
            sourceSpan,
          );
        }
      }
      case 'Vec2':
        expectArguments(name, arguments_, 2, sourceSpan);
        return {
          x: expectNumber(arguments_[0], sourceSpan),
          y: expectNumber(arguments_[1], sourceSpan),
        };
      case 'Rect':
        expectArguments(name, arguments_, 4, sourceSpan);
        return {
          x: expectNumber(arguments_[0], sourceSpan),
          y: expectNumber(arguments_[1], sourceSpan),
          w: expectNumber(arguments_[2], sourceSpan),
          h: expectNumber(arguments_[3], sourceSpan),
        };
      case 'dither':
        expectArguments(name, arguments_, 5, sourceSpan);
        return orderedDither(
          expectInteger(arguments_[0], sourceSpan),
          expectInteger(arguments_[1], sourceSpan),
          expectInteger(arguments_[2], sourceSpan),
          expectInteger(arguments_[3], sourceSpan),
          expectInteger(arguments_[4], sourceSpan),
        );
      case 'btn':
      case 'btnp': {
        expectArguments(name, arguments_, 2, sourceSpan);
        const port = expectInteger(arguments_[0], sourceSpan);
        const button = arguments_[1];
        if (port < 0 || port >= 4 || !isButton(button))
          return this.fault('M019008', 'invalid controller port or button', sourceSpan);
        const pressed = this.input.controllers[port]?.buttons[button] ?? false;
        if (name === 'btn') return pressed;
        return pressed && !(this.previousInput.controllers[port]?.buttons[button] ?? false);
      }
      case 'pointer_x':
        expectArguments(name, arguments_, 0, sourceSpan);
        return this.input.pointer.x;
      case 'pointer_y':
        expectArguments(name, arguments_, 0, sourceSpan);
        return this.input.pointer.y;
      case 'pointer_inside':
        expectArguments(name, arguments_, 0, sourceSpan);
        return this.input.pointer.inside;
      case 'pointer_primary':
      case 'pointer_secondary': {
        expectArguments(name, arguments_, 0, sourceSpan);
        const button = name === 'pointer_primary' ? 'primary' : 'secondary';
        return this.input.pointer[button] && !this.previousInput.pointer[button];
      }
      default: {
        const context = {
          frame: this.currentFrame,
          phase: this.phase,
          ...(this.rasterLine === void 0 ? {} : { rasterLine: this.rasterLine }),
        };
        const result = this.hooks.call?.(name, arguments_, sourceSpan, context);
        if (this.hooks.call === void 0)
          return this.fault('M019004', `console API call '${name}' is unavailable`, sourceSpan);
        return result;
      }
    }
  }
  fault(code, message, sourceSpan) {
    const error = new RuntimeFault(code, message, sourceSpan);
    this.rememberFault(error);
    throw error;
  }
  rememberFault(
    error,
    fallback = {
      start: 0,
      end: 0,
    },
  ) {
    const span = error instanceof RuntimeFault ? error.sourceSpan : fallback;
    this.lastFault = {
      code:
        error instanceof RuntimeFault && /^M019\d{3}$/.test(error.code)
          ? Number(error.code.slice(3))
          : 9199,
      sourceSpan: isFaultSpan(span)
        ? { ...span }
        : {
            start: 0,
            end: 0,
          },
    };
  }
  probe(id, sourceSpan, locals) {
    this.hooks.probe?.(id, sourceSpan, locals);
  }
  enter(name, sourceSpan) {
    this.hooks.enter?.(name, sourceSpan);
  }
  leave() {
    this.hooks.leave?.();
  }
};
function isMachineSnapshot(value) {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const candidate = value;
  if (!(
    Number.isSafeInteger(candidate.frame) &&
    typeof candidate.frame === 'number' &&
    candidate.frame >= 0 &&
    Number.isSafeInteger(candidate.rngState) &&
    isInputFrame(candidate.input) &&
    isInputFrame(candidate.previousInput) &&
    typeof candidate.cartridge === 'object' &&
    candidate.cartridge !== null
  ))
    return false;
  if (candidate.revision === 1) return Object.keys(candidate).length === 6;
  return (
    candidate.revision === 2 &&
    Object.keys(candidate).length === 9 &&
    typeof candidate.rngState === 'number' &&
    candidate.rngState > 0 &&
    candidate.rngState <= 4294967295 &&
    (candidate.updateRate === 30 || candidate.updateRate === 60) &&
    isWorkBudgetSnapshot(candidate.budget) &&
    isExecutionSnapshot(
      candidate.execution,
      candidate.frame,
      candidate.updateRate,
      candidate.budget,
    )
  );
}
function expectArguments(name, arguments_, count, sourceSpan) {
  if (arguments_.length !== count)
    throw new RuntimeFault(
      'M019009',
      `${name} expected ${String(count)} arguments, received ${String(arguments_.length)}`,
      sourceSpan,
    );
}
function expectNumber(value, sourceSpan) {
  if (typeof value !== 'number' || !Number.isFinite(value))
    throw new RuntimeFault('M019009', 'expected a finite number', sourceSpan);
  return value;
}
function expectInteger(value, sourceSpan) {
  const number = expectNumber(value, sourceSpan);
  if (!Number.isSafeInteger(number))
    throw new RuntimeFault('M019009', 'expected a safe integer', sourceSpan);
  return number;
}
//#endregion
//#region src/map-query.ts
/** Worker-safe, read-only map view exposed to cartridge query calls. */
var MapQueryStore = class {
  maps = /* @__PURE__ */ new Map();
  constructor(assets = []) {
    for (const asset of assets) {
      if (asset.name.length === 0 || this.maps.has(asset.name) || asset.layers.length === 0)
        throw new TypeError('map query catalog contains an invalid or duplicate map');
      for (const layer of asset.layers)
        if (
          !Number.isSafeInteger(layer.width) ||
          !Number.isSafeInteger(layer.height) ||
          layer.width <= 0 ||
          layer.height <= 0 ||
          layer.cells.length !== layer.width * layer.height ||
          layer.cells.some((tile) => tile >= layer.tileFlags.length)
        )
          throw new TypeError(`map query data for '${asset.name}' is incoherent`);
      this.maps.set(asset.name, asset);
    }
  }
  cell(name, layerIndex, x, y) {
    const layer = this.maps.get(name)?.layers[layerIndex];
    if (layer === void 0 || x < 0 || y < 0 || x >= layer.width || y >= layer.height) return -1;
    return layer.cells[y * layer.width + x] ?? -1;
  }
  flag(name, layerIndex, x, y, flagIndex) {
    if (flagIndex < 0 || flagIndex > 7) return false;
    const layer = this.maps.get(name)?.layers[layerIndex];
    const tile = this.cell(name, layerIndex, x, y);
    return layer !== void 0 && tile >= 0 && ((layer.tileFlags[tile] ?? 0) & (1 << flagIndex)) !== 0;
  }
};
function isMapQueryCatalog(value) {
  return Array.isArray(value) && value.every(isMapQueryAsset);
}
function isMapQueryAsset(value) {
  return (
    isRecord$5(value) &&
    typeof value.name === 'string' &&
    value.name.length > 0 &&
    Array.isArray(value.layers) &&
    value.layers.length > 0 &&
    value.layers.every(isMapQueryLayer)
  );
}
function isMapQueryLayer(value) {
  if (
    !isRecord$5(value) ||
    !Number.isSafeInteger(value.width) ||
    typeof value.width !== 'number' ||
    value.width <= 0 ||
    !Number.isSafeInteger(value.height) ||
    typeof value.height !== 'number' ||
    value.height <= 0 ||
    !(value.cells instanceof Uint16Array) ||
    value.cells.length !== value.width * value.height ||
    !(value.tileFlags instanceof Uint8Array)
  )
    return false;
  const tileFlags = value.tileFlags;
  return value.cells.every((tile) => tile < tileFlags.length);
}
function isRecord$5(value) {
  return typeof value === 'object' && value !== null;
}
//#endregion
//#region src/save.ts
/** One byte image, with a commit latch delivered to the trusted host after a successful frame. */
var SaveMemory = class SaveMemory {
  bytes = new Uint8Array(HARDWARE.saveCapacityBytes);
  committed = new Uint8Array(HARDWARE.saveCapacityBytes);
  values;
  pendingCommit = false;
  commits = 0;
  dirtyBytes = 0;
  writes = /* @__PURE__ */ new Map();
  constructor(initial = {}) {
    if (!isSaveImage(initial)) throw new TypeError('invalid MOD-01 save image');
    this.bytes.set(initial instanceof Uint8Array ? initial : encodeValues(initial));
    this.committed.set(this.bytes);
  }
  get(key, fallback) {
    validateKey(key);
    validateInteger(fallback);
    const values = this.readValues();
    return Object.hasOwn(values, key) ? (values[key] ?? fallback) : fallback;
  }
  set(key, value) {
    validateKey(key);
    validateInteger(value);
    this.checkCommitCounter();
    const next = {
      ...this.readValues(),
      [key]: value,
    };
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
  snapshot() {
    return sortedValues(this.readValues());
  }
  deviceSnapshot() {
    return {
      revision: 1,
      bytes: this.bytes.slice(),
      committed: this.committed.slice(),
      pendingCommit: this.pendingCommit,
      commits: this.commits,
    };
  }
  restoreDevice(value, pendingWrites = []) {
    if (!isSaveSnapshot(value) || !isPendingDeviceWrites(pendingWrites, value))
      throw new TypeError('invalid MOD-01 save snapshot');
    this.bytes.set(value.bytes);
    this.committed.set(value.committed);
    this.values = void 0;
    this.pendingCommit = value.pendingCommit;
    this.commits = value.commits;
    this.dirtyBytes = this.bytes.reduce(
      (count, byte, index) => count + Number(byte !== this.committed[index]),
      0,
    );
    this.writes.clear();
    for (const write of pendingWrites) this.writes.set(write.key, write.value);
  }
  commit() {
    this.checkCommitCounter();
    this.latch();
    this.writes.clear();
  }
  takeCommit() {
    if (!this.pendingCommit) return void 0;
    this.pendingCommit = false;
    this.writes.clear();
    return this.committed.slice();
  }
  restore(value, pendingWrites = []) {
    if (!isSaveValues(value) || !isPendingSaveWrites(pendingWrites, value))
      throw new TypeError('invalid MOD-01 save snapshot');
    const initial = new SaveMemory(value).deviceSnapshot();
    this.restoreDevice(
      {
        ...initial,
        pendingCommit: pendingWrites.length > 0,
      },
      pendingWrites,
    );
  }
  pendingWrites() {
    return [...this.writes]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, value]) => ({
        key,
        value,
      }));
  }
  takeWrites() {
    const writes = this.pendingWrites();
    this.writes.clear();
    return writes;
  }
  memoryRegions(charge) {
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
          this.values = void 0;
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
          if (bytes[0] === 0) return () => void 0;
          if (bytes[0] !== 1) return void 0;
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
          const status = /* @__PURE__ */ new DataView(/* @__PURE__ */ new ArrayBuffer(32));
          status.setUint8(1, (this.dirtyBytes > 0 ? 1 : 0) | (this.pendingCommit ? 2 : 0));
          status.setUint32(4, HARDWARE.saveCapacityBytes, true);
          status.setBigUint64(8, BigInt(this.commits), true);
          status.setUint32(16, this.dirtyBytes, true);
          return status.getUint8(offset + 1);
        },
      },
    ];
  }
  readValues() {
    this.values ??= decodeSaveValues(this.bytes);
    return this.values;
  }
  checkCommitCounter() {
    if (this.commits === Number.MAX_SAFE_INTEGER)
      throw new RangeError('save commit counter exhausted');
  }
  latch() {
    this.committed.set(this.bytes);
    this.pendingCommit = true;
    this.dirtyBytes = 0;
    this.commits += 1;
  }
};
function isSaveImage(value) {
  return value instanceof Uint8Array
    ? value.length <= HARDWARE.saveCapacityBytes
    : isSaveValues(value);
}
function isSaveSnapshot(value) {
  return (
    isRecord$4(value) &&
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
function isPendingDeviceWrites(value, snapshot) {
  if (!Array.isArray(value)) return false;
  if (value.length === 0) return Object.keys(value).length === 0;
  if (!snapshot.pendingCommit) return false;
  try {
    return isPendingSaveWrites(value, decodeSaveValues(snapshot.committed));
  } catch {
    return false;
  }
}
function decodeSaveValues(bytes) {
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
  let values;
  try {
    values = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, end)));
  } catch {
    throw new TypeError('save bytes are not an integer-save image');
  }
  if (!isSaveValues(values)) throw new TypeError('save bytes are not an integer-save image');
  return values;
}
function isPendingSaveWrites(value, values) {
  if (
    !Array.isArray(value) ||
    value.length > Object.keys(values).length ||
    Object.keys(value).length !== value.length
  )
    return false;
  const keys = /* @__PURE__ */ new Set();
  for (const write of value) {
    if (
      !isRecord$4(write) ||
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
function isSaveValues(value) {
  if (!isRecord$4(value) || Array.isArray(value)) return false;
  try {
    for (const [key, entry] of Object.entries(value)) {
      validateKey(key);
      validateInteger(entry);
    }
    return encodedBytes(value) <= HARDWARE.saveCapacityBytes;
  } catch {
    return false;
  }
}
function sortedValues(values) {
  return Object.fromEntries(
    Object.entries(values).sort(([left], [right]) => left.localeCompare(right)),
  );
}
function encodedBytes(values) {
  return new TextEncoder().encode(JSON.stringify(sortedValues(values))).byteLength;
}
function encodeValues(values) {
  if (Object.keys(values).length === 0) return /* @__PURE__ */ new Uint8Array();
  const bytes = new TextEncoder().encode(
    JSON.stringify(
      Object.fromEntries(
        Object.entries(values).sort(([left], [right]) =>
          left < right ? -1 : left > right ? 1 : 0,
        ),
      ),
    ),
  );
  if (bytes.length > HARDWARE.saveCapacityBytes)
    throw new RangeError('cartridge save exceeds the 8 KiB capacity');
  return bytes;
}
function validateKey(key) {
  if (
    !/^[A-Za-z_][A-Za-z0-9_.-]{0,63}$/.test(key) ||
    ['__proto__', 'constructor', 'prototype'].includes(key)
  )
    throw new TypeError('save keys must be 1-64 canonical ASCII characters');
}
function validateInteger(value) {
  if (typeof value !== 'number' || !Number.isSafeInteger(value))
    throw new TypeError('save values must be safe integers');
}
function isRecord$4(value) {
  return typeof value === 'object' && value !== null;
}
//#endregion
//#region src/protocol.ts
function isConsoleConfiguration(value) {
  return (
    isRecord$3(value) &&
    hasExactKeys$2(value, [
      'seed',
      'workUnitsPerFrame',
      'updateRate',
      ...(value.hardwareRevision === void 0 ? [] : ['hardwareRevision']),
      ...(value.maps === void 0 ? [] : ['maps']),
      ...(value.save === void 0 ? [] : ['save']),
      ...(value.debug === void 0 ? [] : ['debug']),
      ...(value.assets === void 0 ? [] : ['assets']),
      ...(value.rom === void 0 ? [] : ['rom']),
    ]) &&
    Number.isSafeInteger(value.seed) &&
    isNonNegativeInteger(value.workUnitsPerFrame) &&
    value.workUnitsPerFrame > 0 &&
    value.workUnitsPerFrame <= HARDWARE.workUnitsPerFrame &&
    (value.updateRate === 30 || value.updateRate === 60) &&
    (value.hardwareRevision === void 0 ||
      value.hardwareRevision === 1 ||
      value.hardwareRevision === 2) &&
    (value.maps === void 0 || isMapQueryCatalog(value.maps)) &&
    (value.save === void 0 || isSaveImage(value.save)) &&
    (value.debug === void 0 || typeof value.debug === 'boolean') &&
    (value.assets === void 0 || isRuntimeAssetSource(value.assets)) &&
    (value.rom === void 0 ||
      (value.rom instanceof Uint8Array &&
        value.rom.length > 0 &&
        value.rom.length <= HARDWARE.cartridgeCapacityBytes))
  );
}
function isRecord$3(value) {
  return typeof value === 'object' && value !== null;
}
function isNonNegativeInteger(value) {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}
function hasExactKeys$2(value, expected) {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length && expected.every((key) => actual.includes(key));
}
//#endregion
//#region src/console-runtime.ts
function isConsoleRuntimeSnapshot(value) {
  return (
    isRecord$2(value) &&
    value.revision === 6 &&
    Object.keys(value).length === 7 &&
    isMachineSnapshot(value.machine) &&
    value.machine.revision === 2 &&
    isSaveSnapshot(value.save) &&
    isGraphicsSnapshot(value.graphics) &&
    isSynthSnapshot(value.audio) &&
    isPendingDeviceWrites(value.pendingSaveWrites, value.save) &&
    isMemorySnapshot(value.memory) &&
    graphicsMatchesMemory(value.graphics, value.memory)
  );
}
/** One production dispatcher for native Studio, standalone players and headless hosts. */
function createConsoleRuntime(factory, configuration) {
  if (!isConsoleConfiguration(configuration))
    throw new RuntimeFault('M019100', 'invalid console configuration', {
      start: 0,
      end: 0,
    });
  const source = configuration.assets;
  const assets =
    source === void 0
      ? decodeRuntimeAssets({}, {})
      : decodeRuntimeAssets(source.declarations, source.files, source.displayPath);
  const visualStore = new VisualAssetStore(assets.visual, assets.display);
  const graphics = new IndexedGraphics(visualStore);
  const audioStore = new AudioAssetStore(assets.audio);
  const synthesizer = new Synthesizer(audioStore);
  let rendering = false;
  let machine = void 0;
  let drawCommands = [];
  let audioCommands = [];
  let frameOutput;
  let debugFrameActive = false;
  const mapQueries = new MapQueryStore(source === void 0 ? (configuration.maps ?? []) : []);
  const saveMemory = new SaveMemory(configuration.save ?? {});
  const cartridgeRom = configuration.rom?.slice() ?? /* @__PURE__ */ new Uint8Array();
  const cartridgeInfo = createCartridgeInfo(cartridgeRom, configuration.hardwareRevision ?? 1);
  const debugEnabled = configuration.debug ?? false;
  let debugTrace = [];
  const debugCallStack = [];
  let debugTraceTruncated = false;
  const ram = new Uint8Array(MEMORY.ramBytes);
  const bus = new MemoryBus(
    [
      {
        name: 'ram',
        address: MEMORY.ram,
        bytes: ram,
        writable: true,
      },
      ...graphics.memoryRegions(),
      ...visualStore.memoryRegions(),
      ...synthesizer.memoryRegions(),
      ...saveMemory.memoryRegions((units, span) => {
        requireMachine().work(units, span);
      }),
      {
        name: 'cartridge status',
        address: MEMORY.cartridgeInfo,
        bytes: cartridgeInfo,
        writable: false,
      },
      ...(cartridgeRom.length === 0
        ? []
        : [
            {
              name: 'canonical cartridge ROM',
              address: MEMORY.cartridgeRom,
              bytes: cartridgeRom.subarray(0, 262144),
              writable: false,
            },
            ...(cartridgeRom.length > 262144
              ? [
                  {
                    name: 'canonical cartridge ROM extension',
                    address: MEMORY.cartridgeRomExtension,
                    bytes: cartridgeRom.subarray(262144),
                    writable: false,
                  },
                ]
              : []),
          ]),
      {
        name: 'controllers and pointer',
        address: MEMORY.input,
        length: MEMORY.inputBytes,
        writable: false,
        readByte: (offset) => requireMachine().readInputByte(offset),
      },
      {
        name: 'scheduler, RNG, work and fault status',
        address: MEMORY.system,
        length: MEMORY.systemBytes,
        writable: false,
        readByte: (offset) => requireMachine().readSystemByte(offset),
      },
      {
        name: 'master palette RGBA',
        address: MEMORY.palette,
        bytes: Uint8Array.from(MASTER_PALETTE_RGBA.slice(0, 128)),
        writable: false,
      },
      {
        name: 'extended master palette RGBA',
        address: MEMORY.paletteExtended,
        bytes: Uint8Array.from(MASTER_PALETTE_RGBA.slice(128)),
        writable: false,
      },
    ],
    (units, span) => {
      requireMachine().work(units, span);
    },
  );
  machine = new DeterministicMachine(factory, configuration, {
    completeFrame: () => {
      frameOutput = {
        indexedPixels: graphics.finishFrame().indexedPixels,
        audio: synthesizer.finishFrame(),
        audioState: synthesizer.snapshot(),
      };
    },
    call: handleConsoleCall,
    ...(debugEnabled
      ? {
          probe: (id, sourceSpan, locals) => {
            if (debugTrace.length >= HARDWARE.drawCommandsPerFrame) {
              debugTraceTruncated = true;
              return;
            }
            debugTrace.push({
              id,
              sourceSpan,
              locals: structuredClone(locals),
              callStack: structuredClone(debugCallStack),
            });
          },
          enter: (name, sourceSpan) => {
            debugCallStack.push({
              name,
              sourceSpan,
            });
          },
          leave: () => {
            debugCallStack.pop();
          },
        }
      : {}),
  });
  graphics.beginFrame();
  rendering = true;
  try {
    if (!debugEnabled) machine.boot();
  } finally {
    rendering = false;
  }
  graphics.finishFrame();
  let boundarySnapshot = captureSnapshot();
  return {
    runFrame(input) {
      if (!isInputFrame(input))
        throw new RuntimeFault('M019008', 'invalid controller input frame', {
          start: 0,
          end: 0,
        });
      const active = requireMachine();
      active.assertRunnable();
      drawCommands = [];
      audioCommands = [];
      frameOutput = void 0;
      debugTrace = [];
      debugTraceTruncated = false;
      graphics.beginFrame();
      rendering = true;
      let report;
      try {
        report = active.runFrame(input);
      } finally {
        rendering = false;
      }
      const output = completedOutput();
      const saveWrites = saveMemory.takeWrites();
      const saveCommit = saveMemory.takeCommit();
      boundarySnapshot = captureSnapshot();
      return {
        ...report,
        drawCommands,
        audioCommands,
        saveWrites,
        ...(saveCommit === void 0 ? {} : { saveCommit }),
        output,
        ...(debugEnabled
          ? {
              debug: {
                trace: debugTrace,
                truncated: debugTraceTruncated,
                inspection: structuredClone(active.inspect()),
              },
            }
          : {}),
      };
    },
    stepDebug(input) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'statement stepping requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!isInputFrame(input))
        throw new RuntimeFault('M019008', 'invalid controller input frame', {
          start: 0,
          end: 0,
        });
      if (!debugFrameActive) {
        drawCommands = [];
        audioCommands = [];
        frameOutput = void 0;
        debugTrace = [];
        debugTraceTruncated = false;
        graphics.beginFrame();
        debugFrameActive = true;
      }
      rendering = true;
      let step;
      try {
        step = requireMachine().stepDebug(input);
      } finally {
        rendering = false;
      }
      if (step.event !== void 0) {
        const inspection = structuredClone(requireMachine().inspect());
        return {
          event: {
            ...step.event,
            callStack: structuredClone(debugCallStack),
          },
          inspection,
        };
      }
      if (step.booted === true) {
        debugFrameActive = false;
        graphics.finishFrame();
        boundarySnapshot = captureSnapshot();
        return { booted: true };
      }
      const report = step.report;
      if (report === void 0) throw new TypeError('debug step produced no event or frame');
      debugFrameActive = false;
      const output = completedOutput();
      const saveWrites = saveMemory.takeWrites();
      const saveCommit = saveMemory.takeCommit();
      const inspection = structuredClone(requireMachine().inspect());
      boundarySnapshot = captureSnapshot();
      return {
        frame: {
          ...report,
          drawCommands,
          audioCommands,
          saveWrites,
          ...(saveCommit === void 0 ? {} : { saveCommit }),
          output,
          debug: {
            trace: debugTrace,
            truncated: debugTraceTruncated,
            inspection,
          },
        },
      };
    },
    snapshot: captureSnapshot,
    restore(value) {
      const snapshot = readWorkerSnapshot(value);
      const before = debugFrameActive ? boundarySnapshot : captureSnapshot();
      try {
        requireMachine().restore(snapshot.machine);
        debugFrameActive = false;
        if (snapshot.revision === 6)
          saveMemory.restoreDevice(snapshot.save, snapshot.pendingSaveWrites);
        else saveMemory.restore(snapshot.save, snapshot.pendingSaveWrites);
        if (snapshot.revision >= 2) {
          graphics.restore(snapshot.graphics);
          synthesizer.restore(snapshot.audio);
        }
        if (snapshot.revision >= 4) bus.restore(snapshot.memory);
        else if (snapshot.revision >= 2) {
          const visual = new VisualAssetStore(assets.visual, assets.display).memoryRegions()[0];
          if (visual === void 0) throw new TypeError('missing visual image');
          if (snapshot.revision === 3 && isMemorySnapshot(snapshot.memory))
            bus.restore({
              revision: 1,
              regions: [
                ...snapshot.memory.regions,
                {
                  address: MEMORY.visual,
                  bytes: visual.bytes,
                },
              ].sort((a, b) => a.address - b.address),
            });
          else {
            ram.fill(0);
            visualStore.memoryRegions()[0]?.bytes.set(visual.bytes);
          }
        }
        boundarySnapshot = captureSnapshot();
      } catch (error) {
        requireMachine().restore(before.machine);
        saveMemory.restoreDevice(before.save, before.pendingSaveWrites);
        graphics.restore(before.graphics);
        synthesizer.restore(before.audio);
        bus.restore(before.memory);
        throw new RuntimeFault(
          'M019103',
          error instanceof Error ? error.message : 'invalid device snapshot',
          {
            start: 0,
            end: 0,
          },
        );
      }
    },
    inspectMemory(address, length) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'memory inspection requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!Number.isSafeInteger(length) || length < 1 || length > 256)
        throw new RuntimeFault('M019020', 'debug memory reads require 1-256 bytes', {
          start: 0,
          end: 0,
        });
      return {
        address,
        bytes: bus.inspect(address, length),
        regions: bus.describe(),
      };
    },
    editMemory(address, bytes) {
      if (!debugEnabled)
        throw new RuntimeFault('M019104', 'memory editing requires a debug cartridge', {
          start: 0,
          end: 0,
        });
      if (!(bytes instanceof Uint8Array) || bytes.length < 1 || bytes.length > 256)
        throw new RuntimeFault('M019020', 'debug memory edits require 1-256 bytes', {
          start: 0,
          end: 0,
        });
      bus.edit(address, bytes);
    },
    memoryRegions() {
      return bus.describe();
    },
  };
  function captureSnapshot() {
    return {
      revision: 6,
      machine: requireMachine().snapshot(),
      save: saveMemory.deviceSnapshot(),
      graphics: graphics.snapshot(),
      audio: synthesizer.snapshot(),
      pendingSaveWrites: saveMemory.pendingWrites(),
      memory: bus.snapshot(),
    };
  }
  function completedOutput() {
    if (frameOutput === void 0)
      throw new RuntimeFault('M019102', 'frame completed without device output', {
        start: 0,
        end: 0,
      });
    return frameOutput;
  }
  function handleConsoleCall(name, arguments_, sourceSpan, context) {
    if (name === 'visual_id' || name === 'audio_id') {
      if (arguments_.length !== 1 || typeof arguments_[0] !== 'string')
        throw new RuntimeFault('M019009', `${name} expects one Text name`, sourceSpan);
      requireMachine().work(1 + arguments_[0].length, sourceSpan);
      return (name === 'visual_id' ? visualStore : audioStore).id(arguments_[0]);
    }
    if (MEMORY_CALLS.has(name)) {
      if (arguments_.length !== MEMORY_CALLS.get(name))
        throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
      const values = arguments_.map((value) => readInteger(value, sourceSpan));
      const address = values[0] ?? 0;
      const second = values[1] ?? 0;
      const third = values[2] ?? 0;
      const raster = context.phase === 'raster';
      switch (name) {
        case 'mem_read':
          return bus.read(address, 1, sourceSpan);
        case 'mem_read16':
          return bus.read(address, 2, sourceSpan);
        case 'mem_write':
          bus.write(address, second, 1, sourceSpan, raster);
          return;
        case 'mem_write16':
          bus.write(address, second, 2, sourceSpan, raster);
          return;
        case 'mem_copy':
          bus.copy(address, second, third, sourceSpan, raster);
          return;
        case 'mem_fill':
          bus.fill(address, second, third, sourceSpan, raster);
          return;
      }
    }
    requireMachine().work(consoleWorkCost(name, arguments_), sourceSpan);
    if (context.phase === 'raster' && name !== 'pal' && name !== 'raster_scroll')
      throw new RuntimeFault(
        'M019011',
        `console API call '${name}' is not valid in the raster callback`,
        sourceSpan,
      );
    if (name === 'raster_scroll' && context.phase !== 'raster')
      throw new RuntimeFault(
        'M019011',
        'raster_scroll is only valid in the raster callback',
        sourceSpan,
      );
    if (name === 'save_commit') {
      if (arguments_.length !== 0)
        throw new RuntimeFault('M019009', 'save_commit expects no arguments', sourceSpan);
      requireMachine().work(HARDWARE.saveCapacityBytes, sourceSpan);
      try {
        saveMemory.commit();
      } catch (error) {
        throw new RuntimeFault(
          'M019012',
          error instanceof Error ? error.message : 'invalid save commit',
          sourceSpan,
        );
      }
      return;
    }
    if (name === 'map_cell' || name === 'map_flag') {
      const handle = arguments_[0];
      if (!isAssetHandle(handle, 'Map'))
        throw new RuntimeFault('M019009', 'expected a Map asset handle', sourceSpan);
      const integers = arguments_.slice(1).map((value) => readInteger(value, sourceSpan));
      if (name === 'map_cell' && integers.length === 3) {
        if (source !== void 0)
          return (
            visualStore.mapCell(
              handle.name,
              integers[0] ?? 0,
              integers[1] ?? 0,
              integers[2] ?? 0,
            ) ?? -1
          );
        return mapQueries.cell(handle.name, integers[0] ?? 0, integers[1] ?? 0, integers[2] ?? 0);
      }
      if (name === 'map_flag' && integers.length === 4) {
        if (source !== void 0)
          return visualStore.mapFlag(
            handle.name,
            integers[0] ?? 0,
            integers[1] ?? 0,
            integers[2] ?? 0,
            integers[3] ?? 0,
          );
        return mapQueries.flag(
          handle.name,
          integers[0] ?? 0,
          integers[1] ?? 0,
          integers[2] ?? 0,
          integers[3] ?? 0,
        );
      }
      throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
    }
    if (name === 'save_get_int' || name === 'save_set_int') {
      const key = arguments_[0];
      if (typeof key !== 'string')
        throw new RuntimeFault('M019009', 'save key must be Text', sourceSpan);
      try {
        if (name === 'save_get_int' && arguments_.length === 2)
          return saveMemory.get(key, readInteger(arguments_[1], sourceSpan));
        if (name === 'save_set_int' && arguments_.length === 2) {
          saveMemory.set(key, readInteger(arguments_[1], sourceSpan));
          return;
        }
      } catch (error) {
        throw new RuntimeFault(
          'M019012',
          error instanceof Error ? error.message : 'invalid cartridge save operation',
          sourceSpan,
        );
      }
      throw new RuntimeFault('M019009', `${name} received the wrong argument count`, sourceSpan);
    }
    const command = {
      name,
      arguments: structuredClone(arguments_),
      sourceSpan,
      ...(context.rasterLine === void 0 ? {} : { rasterLine: context.rasterLine }),
    };
    if (DRAW_CALLS.has(name)) {
      if (drawCommands.length >= HARDWARE.drawCommandsPerFrame)
        throw new RuntimeFault('M019010', 'draw-command ceiling exceeded', sourceSpan);
      drawCommands.push(command);
      if (rendering) graphics.executeCommand(command);
      return;
    }
    if (AUDIO_CALLS.has(name)) {
      audioCommands.push(command);
      if (rendering) synthesizer.executeCommand(command);
      return;
    }
    throw new RuntimeFault('M019004', `console API call '${name}' is unavailable`, sourceSpan);
  }
  function requireMachine() {
    if (machine === void 0)
      throw new RuntimeFault('M019102', 'no cartridge is loaded', {
        start: 0,
        end: 0,
      });
    return machine;
  }
}
function createCartridgeInfo(rom, hardwareRevision) {
  const bytes = /* @__PURE__ */ new Uint8Array(64);
  const view = new DataView(bytes.buffer);
  const validHeader =
    rom.length >= 12 &&
    [77, 79, 68, 45, 48, 49, 26].every((byte, index) => rom[index] === byte) &&
    rom[7] === 1;
  view.setUint16(0, hardwareRevision, true);
  view.setUint16(2, validHeader ? (rom[7] ?? 0) : 0, true);
  view.setUint32(4, rom.length, true);
  view.setUint32(8, hardwareRevision === 1 ? 262144 : HARDWARE.cartridgeCapacityBytes, true);
  view.setUint32(12, MEMORY.cartridgeRom, true);
  view.setUint32(16, Number(rom.length > 0) | (Number(validHeader) << 1), true);
  view.setUint32(
    20,
    validHeader ? new DataView(rom.buffer, rom.byteOffset).getUint32(8, true) : 0,
    true,
  );
  return bytes;
}
var DRAW_CALLS = /* @__PURE__ */ new Set([
  'clear',
  'pixel',
  'line',
  'rect',
  'rect_fill',
  'circle',
  'circle_fill',
  'triangle',
  'camera',
  'clip',
  'clip_reset',
  'sprite',
  'sprite_xform',
  'animation',
  'map',
  'pal',
  'pal_reset',
  'raster_scroll',
  'print',
  'font_print',
]);
var AUDIO_CALLS = /* @__PURE__ */ new Set(['sfx', 'music', 'music_stop']);
function consoleWorkCost(name, arguments_) {
  const integer = (index) => {
    const value = arguments_[index];
    return typeof value === 'number' && Number.isSafeInteger(value) ? value : 0;
  };
  switch (name) {
    case 'clear':
      return Math.ceil((HARDWARE.width * HARDWARE.height) / 32);
    case 'pixel':
      return 1;
    case 'line':
      return Math.max(Math.abs(integer(2) - integer(0)), Math.abs(integer(3) - integer(1))) + 1;
    case 'rect':
      return Math.max(1, 2 * Math.abs(integer(2)) + 2 * Math.abs(integer(3)));
    case 'rect_fill':
      return Math.max(1, Math.ceil((Math.abs(integer(2)) * Math.abs(integer(3))) / 4));
    case 'circle':
      return Math.max(1, Math.abs(integer(2)) * 8);
    case 'circle_fill':
      return Math.max(1, Math.ceil((Math.abs(integer(2)) ** 2 * 3) / 4));
    case 'triangle': {
      const area = Math.abs(
        (integer(2) - integer(0)) * (integer(5) - integer(1)) -
          (integer(4) - integer(0)) * (integer(3) - integer(1)),
      );
      return Math.max(1, Math.ceil(area / 8));
    }
    case 'sprite':
    case 'animation':
      return 32;
    case 'sprite_xform':
      return Math.max(32, 32 * Math.abs(integer(3)) ** 2);
    case 'map':
      return 128;
    case 'print':
      return Math.max(1, (typeof arguments_[0] === 'string' ? arguments_[0].length : 0) * 6);
    case 'font_print':
      return Math.max(1, (typeof arguments_[1] === 'string' ? arguments_[1].length : 0) * 8);
    case 'sfx':
    case 'music':
    case 'music_stop':
      return 8;
    default:
      return 1;
  }
}
function isRecord$2(value) {
  return typeof value === 'object' && value !== null;
}
function isAssetHandle(value, kind) {
  return (
    isRecord$2(value) &&
    typeof value.name === 'string' &&
    value.name.length > 0 &&
    value.kind === kind
  );
}
function readInteger(value, sourceSpan) {
  if (typeof value !== 'number' || !Number.isSafeInteger(value))
    throw new RuntimeFault('M019009', 'console arguments must be safe integers', sourceSpan);
  return value;
}
function readWorkerSnapshot(value) {
  if (
    !isRecord$2(value) ||
    (value.revision === 6 ? !isConsoleRuntimeSnapshot(value) : !isLegacyWorkerSnapshot(value))
  )
    throw new RuntimeFault('M019103', 'invalid worker snapshot', {
      start: 0,
      end: 0,
    });
  return {
    revision: value.revision,
    machine: value.machine,
    save: value.save,
    graphics: value.graphics,
    audio: value.audio,
    pendingSaveWrites: value.revision === 1 ? [] : value.pendingSaveWrites,
    memory: value.memory,
  };
}
function isLegacyWorkerSnapshot(value) {
  if (
    !isMachineSnapshot(value.machine) ||
    value.machine.revision !== (value.revision === 5 ? 2 : 1) ||
    !isSaveValues(value.save)
  )
    return false;
  if (value.revision === 1) return Object.keys(value).length === 3;
  if (value.revision !== 2 && value.revision !== 3 && value.revision !== 4 && value.revision !== 5)
    return false;
  return (
    Object.keys(value).length === (value.revision === 2 ? 6 : 7) &&
    isGraphicsSnapshot(value.graphics) &&
    isSynthSnapshot(value.audio) &&
    isPendingSaveWrites(value.pendingSaveWrites, value.save) &&
    (value.revision === 2 ||
      (isMemorySnapshot(value.memory) && graphicsMatchesMemory(value.graphics, value.memory)))
  );
}
function graphicsMatchesMemory(graphics, memory) {
  return [
    [MEMORY.front, graphics.front],
    [MEMORY.display, graphics.resolved],
  ].every(([address, pixels]) => {
    if (!(pixels instanceof Uint8Array)) return false;
    const bytes = memory.regions.find((region) => region.address === address)?.bytes;
    return (
      bytes !== void 0 &&
      bytes.length === pixels.length &&
      bytes.every((value, index) => value === pixels[index])
    );
  });
}
var MEMORY_CALLS = /* @__PURE__ */ new Map([
  ['mem_read', 1],
  ['mem_read16', 1],
  ['mem_write', 2],
  ['mem_write16', 2],
  ['mem_copy', 3],
  ['mem_fill', 3],
]);
new TextEncoder();
new TextDecoder('utf-8', { fatal: true });
var REPLAY_MAX_FRAMES = 36e3;
function replayInputFrames(trace) {
  if (!isReplayTrace(trace, 36e3)) throw new TypeError('replay schema is invalid');
  const inputs = /* @__PURE__ */ new Map();
  for (const item of trace.frames)
    for (let offset = 0; offset < (item.duration ?? 1); offset += 1)
      inputs.set(item.frame + offset, replayFrameInput(item));
  return inputs;
}
function isReplayTrace(value, frameLimit = REPLAY_MAX_FRAMES) {
  if (
    !isRecord$1(value) ||
    !hasExactKeys$1(value, ['revision', 'frames']) ||
    value.revision !== 1 ||
    !Array.isArray(value.frames) ||
    value.frames.length > frameLimit
  )
    return false;
  let previousEnd = 0;
  for (const item of value.frames) {
    if (
      !isRecord$1(item) ||
      !hasOnlyKeys(item, ['frame', 'duration', 'controllers', 'pointer']) ||
      !('controllers' in item) ||
      typeof item.frame !== 'number' ||
      !Number.isSafeInteger(item.frame) ||
      item.frame < previousEnd ||
      item.frame >= frameLimit ||
      (item.duration !== void 0 &&
        (typeof item.duration !== 'number' ||
          !Number.isSafeInteger(item.duration) ||
          item.duration < 1 ||
          item.duration > frameLimit - item.frame)) ||
      !isReplayControllers(item.controllers) ||
      (item.pointer !== void 0 && !isReplayPointer(item.pointer))
    )
      return false;
    previousEnd = item.frame + (item.duration ?? 1);
  }
  return true;
}
function replayFrameInput(frame) {
  const input = emptyInputFrame();
  for (const controller of frame.controllers) {
    const target = input.controllers[controller.port - 1];
    if (target === void 0) continue;
    for (const button of controller.buttons) target.buttons[button] = true;
  }
  return {
    ...input,
    ...(frame.pointer === void 0 ? {} : { pointer: frame.pointer }),
  };
}
function isReplayPointer(value) {
  return (
    isRecord$1(value) &&
    hasExactKeys$1(value, ['x', 'y', 'primary', 'secondary', 'inside']) &&
    typeof value.x === 'number' &&
    Number.isSafeInteger(value.x) &&
    value.x >= 0 &&
    value.x < HARDWARE.width &&
    typeof value.y === 'number' &&
    Number.isSafeInteger(value.y) &&
    value.y >= 0 &&
    value.y < HARDWARE.height &&
    typeof value.primary === 'boolean' &&
    typeof value.secondary === 'boolean' &&
    typeof value.inside === 'boolean'
  );
}
function isReplayControllers(value) {
  if (!Array.isArray(value) || value.length > 4) return false;
  const ports = /* @__PURE__ */ new Set();
  for (const controller of value) {
    if (
      !isRecord$1(controller) ||
      !hasExactKeys$1(controller, ['port', 'buttons']) ||
      typeof controller.port !== 'number' ||
      ![1, 2, 3, 4].includes(controller.port) ||
      ports.has(controller.port) ||
      !Array.isArray(controller.buttons) ||
      controller.buttons.length > BUTTONS.length ||
      new Set(controller.buttons).size !== controller.buttons.length ||
      !controller.buttons.every(isButton)
    )
      return false;
    ports.add(controller.port);
  }
  return true;
}
function isRecord$1(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function hasExactKeys$1(value, expected) {
  const actual = Object.keys(value).sort();
  return actual.length === expected.length && expected.every((key) => actual.includes(key));
}
function hasOnlyKeys(value, expected) {
  return Object.keys(value).every((key) => expected.includes(key));
}
//#endregion
//#region src/headless.ts
/** Runs validated compiler output through the same production core used by the browser Worker. */
async function runHeadless(value) {
  if (!isHeadlessRequest(value)) throw new TypeError('invalid MOD-01 headless request');
  const request = value;
  const factory = readFactory(
    await import(
      `data:text/javascript;base64,${Buffer.from(request.javascript).toString('base64')}`
    ),
  );
  const files = Object.fromEntries(
    Object.entries(request.entries).map(([path, bytes]) => [path, Uint8Array.from(bytes)]),
  );
  decodeRuntimeAssets(request.manifest.assets, files, request.manifest.display);
  const runtime = createConsoleRuntime(factory, {
    seed: request.seed,
    workUnitsPerFrame: HARDWARE.workUnitsPerFrame,
    updateRate: request.manifest.updateRate,
    assets: {
      declarations: request.manifest.assets,
      files,
      displayPath: request.manifest.display,
    },
    save: Uint8Array.from(request.save),
    rom: Uint8Array.from(request.rom),
  });
  const inputs = replayInputFrames(request.trace);
  const frameResults = [];
  const commandHash = createHash('sha256');
  const pcmHash = createHash('sha256');
  let workPeak = 0;
  let commandPeak = 0;
  let drawCommandPeak = 0;
  let audioCommandPeak = 0;
  let voicePeak = 0;
  let fault;
  for (let frame = 0; frame < request.frames; frame += 1)
    try {
      const result = runtime.runFrame(inputs.get(frame) ?? emptyInputFrame());
      const snapshot = runtime.snapshot();
      const framePcm = pcmBytes(result);
      const commands = canonicalBytes(result.audioCommands);
      commandHash.update(commands);
      pcmHash.update(framePcm);
      workPeak = Math.max(workPeak, result.workUnits);
      drawCommandPeak = Math.max(drawCommandPeak, result.drawCommands.length);
      audioCommandPeak = Math.max(audioCommandPeak, result.audioCommands.length);
      commandPeak = Math.max(commandPeak, result.drawCommands.length + result.audioCommands.length);
      voicePeak = Math.max(voicePeak, result.output.audio.activeVoices);
      frameResults.push({
        frame: result.frame,
        workUnits: result.workUnits,
        framebufferSha256: sha256(result.output.indexedPixels),
        stateSha256: sha256(canonicalBytes(snapshot)),
        audioCommandSha256: sha256(commands),
        pcmSha256: sha256(framePcm),
        saveSha256: sha256(snapshot.save.committed),
      });
    } catch (error) {
      if (!(error instanceof RuntimeFault)) throw error;
      fault = {
        frame,
        code: error.code,
        message: error.message,
        sourceSpan: error.sourceSpan,
      };
      break;
    }
  const finalSnapshot = runtime.snapshot();
  const final = frameResults.at(-1);
  return {
    revision: 1,
    cartridge: {
      id: request.manifest.id,
      bytes: request.rom.length,
      sha256: sha256(Uint8Array.from(request.rom)),
    },
    configuration: {
      seed: request.seed,
      requestedFrames: request.frames,
      updateRate: request.manifest.updateRate,
      inputTraceSha256: sha256(canonicalBytes(request.trace)),
      initialSaveSha256: sha256(Uint8Array.from(request.save)),
    },
    frames: frameResults,
    summary: {
      completedFrames: frameResults.length,
      workPeak,
      commandPeak,
      drawCommandPeak,
      audioCommandPeak,
      voicePeak,
      busMappedBytes: runtime.memoryRegions().reduce((total, region) => total + region.length, 0),
      finalFramebufferSha256: final?.framebufferSha256 ?? sha256(/* @__PURE__ */ new Uint8Array()),
      finalStateSha256: sha256(canonicalBytes(finalSnapshot)),
      audioCommandsSha256: commandHash.digest('hex'),
      pcmSha256: pcmHash.digest('hex'),
      finalSaveSha256: sha256(finalSnapshot.save.committed),
    },
    ...(fault === void 0 ? {} : { fault }),
  };
}
function isHeadlessRequest(value) {
  if (!isRecord(value) || !hasExactKeys(value, HEADLESS_KEYS) || value.revision !== 1) return false;
  if (
    typeof value.javascript !== 'string' ||
    value.javascript.length === 0 ||
    value.javascript.length > 2097152 ||
    !isManifest(value.manifest) ||
    !isByteRecord(value.entries, 4096, 2097152) ||
    !isByteArray(value.rom, HARDWARE.cartridgeCapacityBytes) ||
    typeof value.seed !== 'number' ||
    !Number.isSafeInteger(value.seed) ||
    value.seed < 0 ||
    value.seed > 4294967295 ||
    typeof value.frames !== 'number' ||
    !Number.isSafeInteger(value.frames) ||
    value.frames < 0 ||
    value.frames > 36e3 ||
    !isReplayTrace(value.trace, value.frames) ||
    !isByteArray(value.save, HARDWARE.saveCapacityBytes)
  )
    return false;
  return true;
}
var HEADLESS_KEYS = [
  'revision',
  'javascript',
  'manifest',
  'entries',
  'rom',
  'seed',
  'frames',
  'trace',
  'save',
];
function isManifest(value) {
  return (
    isRecord(value) &&
    hasExactKeys(value, ['id', 'updateRate', 'display', 'assets']) &&
    typeof value.id === 'string' &&
    /^[a-z0-9][a-z0-9.-]{2,63}$/.test(value.id) &&
    (value.updateRate === 30 || value.updateRate === 60) &&
    (value.display === null || typeof value.display === 'string') &&
    isAssetDeclarations(value.assets)
  );
}
function isAssetDeclarations(value) {
  if (!isRecord(value) || Object.keys(value).length > 4096) return false;
  return Object.values(value).every(
    (asset) =>
      isRecord(asset) &&
      hasExactKeys(asset, ['kind', 'path']) &&
      typeof asset.kind === 'string' &&
      ['sprite', 'animation', 'tile_set', 'map', 'font', 'sound', 'music'].includes(asset.kind) &&
      typeof asset.path === 'string' &&
      asset.path.length > 0 &&
      asset.path.length <= 1024,
  );
}
function isByteRecord(value, entryLimit, byteLimit) {
  if (!isRecord(value) || Object.keys(value).length > entryLimit) return false;
  let total = 0;
  for (const [path, bytes] of Object.entries(value)) {
    if (path.length === 0 || path.length > 1024 || !isByteArray(bytes, byteLimit - total))
      return false;
    total += bytes.length;
  }
  return true;
}
function isByteArray(value, limit) {
  return (
    Array.isArray(value) &&
    value.length <= limit &&
    Object.keys(value).length === value.length &&
    value.every((byte) => Number.isSafeInteger(byte) && byte >= 0 && byte <= 255)
  );
}
function readFactory(value) {
  if (!isRecord(value) || typeof value.default !== 'function')
    throw new RuntimeFault('M019101', 'compiled module does not export a cartridge factory', {
      start: 0,
      end: 0,
    });
  return value.default;
}
function pcmBytes(frame) {
  const samples = frame.output.audio.left.length;
  const bytes = new Uint8Array(samples * 8);
  const view = new DataView(bytes.buffer);
  for (let index = 0; index < samples; index += 1) {
    view.setFloat32(index * 8, frame.output.audio.left[index] ?? 0, true);
    view.setFloat32(index * 8 + 4, frame.output.audio.right[index] ?? 0, true);
  }
  return bytes;
}
function canonicalBytes(value) {
  return new TextEncoder().encode(JSON.stringify(canonicalValue(value)));
}
function canonicalValue(value) {
  if (value instanceof Uint8Array || value instanceof Float32Array) return [...value];
  if (Array.isArray(value)) return value.map(canonicalValue);
  if (isRecord(value))
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, item]) => [key, canonicalValue(item)]),
    );
  return value;
}
function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}
function hasExactKeys(value, keys) {
  const actual = Object.keys(value).sort();
  return (
    actual.length === keys.length && [...keys].sort().every((key, index) => actual[index] === key)
  );
}
function isRecord(value) {
  return typeof value === 'object' && value !== null;
}
//#endregion
//#region src/headless-cli.ts
try {
  let source = '';
  for await (const chunk of process.stdin) source += String(chunk);
  const result = await runHeadless(JSON.parse(source));
  process.stdout.write(`${JSON.stringify(result)}\n`);
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : 'headless host failed'}\n`);
  process.exitCode = 1;
}
//#endregion
