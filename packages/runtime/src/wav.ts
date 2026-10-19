import {
  AudioAssetStore,
  Synthesizer,
  type AudioFrame,
  type MusicAsset,
  type SoundAsset,
} from './audio';
import { HARDWARE } from './hardware';
import type { ConsoleCommand } from './protocol';

const span = { start: 0, end: 0 };

/** Encodes production-synth frames as deterministic stereo 16-bit PCM WAV. */
export function encodeAudioFramesWav(frames: readonly AudioFrame[]): Uint8Array {
  const samples = frames.reduce((total, frame) => total + frame.left.length, 0);
  const dataBytes = samples * 4;
  const output = new Uint8Array(44 + dataBytes);
  const view = new DataView(output.buffer);
  writeAscii(output, 0, 'RIFF');
  view.setUint32(4, 36 + dataBytes, true);
  writeAscii(output, 8, 'WAVEfmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 2, true);
  view.setUint32(24, HARDWARE.audioSampleRate, true);
  view.setUint32(28, HARDWARE.audioSampleRate * 4, true);
  view.setUint16(32, 4, true);
  view.setUint16(34, 16, true);
  writeAscii(output, 36, 'data');
  view.setUint32(40, dataBytes, true);
  let offset = 44;
  for (const frame of frames) {
    if (frame.left.length !== frame.right.length)
      throw new TypeError('audio frame channel lengths differ');
    for (let sample = 0; sample < frame.left.length; sample += 1) {
      view.setInt16(offset, pcm16(frame.left[sample] ?? 0), true);
      view.setInt16(offset + 2, pcm16(frame.right[sample] ?? 0), true);
      offset += 4;
    }
  }
  return output;
}

export function renderSoundWav(
  sound: SoundAsset,
  maximumBytes = Number.MAX_SAFE_INTEGER,
): Uint8Array {
  const assets = new AudioAssetStore([sound]);
  const frames = sound.durationFrames + sound.envelope.releaseFrames;
  admitWavSize(frames, maximumBytes);
  const synth = new Synthesizer(assets);
  return encodeAudioFramesWav(
    Array.from({ length: frames }, (_, frame) =>
      synth.executeFrame(frame === 0 ? [audioCommand('sfx', sound.name, 'Sound')] : []),
    ),
  );
}

export function renderMusicWav(
  music: MusicAsset,
  sounds: readonly SoundAsset[],
  maximumBytes = Number.MAX_SAFE_INTEGER,
): Uint8Array {
  const frames =
    music.framesPerRow *
    music.order.reduce((total, name) => total + (music.patterns[name]?.rows.length ?? 0), 0);
  if (!Number.isSafeInteger(frames) || frames < 1 || frames > HARDWARE.frameRate * 60 * 10)
    throw new RangeError('music WAV duration must be between one frame and ten minutes');
  admitWavSize(frames, maximumBytes);
  const synth = new Synthesizer(new AudioAssetStore([...sounds, music]));
  return encodeAudioFramesWav(
    Array.from({ length: frames }, (_, frame) =>
      synth.executeFrame(frame === 0 ? [audioCommand('music', music.name, 'Music')] : []),
    ),
  );
}

/** Preflight the exact fixed-rate stereo PCM16 size before synth frame allocation. */
function admitWavSize(frames: number, maximumBytes: number): void {
  if (!Number.isSafeInteger(maximumBytes) || maximumBytes < 44)
    throw new RangeError('maximum WAV bytes must be a safe integer of at least 44');
  const bytes = 44 + frames * (HARDWARE.audioSampleRate / HARDWARE.frameRate) * 4;
  if (!Number.isSafeInteger(bytes) || bytes > maximumBytes)
    throw new RangeError(`WAV output exceeds ${String(maximumBytes)}-byte limit`);
}

function audioCommand(name: string, asset: string, kind: 'Sound' | 'Music'): ConsoleCommand {
  return { name, arguments: [{ name: asset, kind }], sourceSpan: span };
}

function pcm16(value: number): number {
  if (!Number.isFinite(value)) throw new TypeError('audio sample is not finite');
  const clamped = Math.max(-1, Math.min(1, value));
  return clamped < 0 ? Math.round(clamped * 32_768) : Math.round(clamped * 32_767);
}

function writeAscii(target: Uint8Array, offset: number, text: string): void {
  for (let index = 0; index < text.length; index += 1)
    target[offset + index] = text.charCodeAt(index);
}
