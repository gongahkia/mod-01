import { describe, expect, it, vi } from 'vitest';

import {
  decodeRuntimeAssets,
  encodeAssetFile,
  encodeMusicAssetFile,
  encodeSoundAssetFile,
  isRuntimeAssetSource,
} from './asset-codec';
import type { SoundAsset } from './audio';
import { IndexedGraphics, VisualAssetStore } from './graphics';

const sound = {
  revision: 1,
  kind: 'sound',
  waveform: 'pulse',
  note: 60,
  durationFrames: 8,
  volume: 0.5,
  pan: 0,
  duty: 0.5,
  envelope: { attackFrames: 0, decayFrames: 1, sustainLevel: 0.8, releaseFrames: 2 },
  pitch: { slideSemitonesPerFrame: 0, vibratoDepthSemitones: 0, vibratoPeriodFrames: 0 },
};

const font = {
  revision: 1,
  kind: 'font',
  glyphWidth: 3,
  glyphHeight: 5,
  baseline: 4,
  advanceX: 4,
  advanceY: 6,
  missingGlyph: 63,
  glyphs: [
    { code: 63, pixels: [1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0] },
    { code: 65, pixels: [0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1] },
  ],
};

describe('source-visible asset codec', () => {
  it('uses ASCII declaration order independently of insertion order or host collation', () => {
    const names = ['a_', 'z', '_a', 'a0', 'a', 'A'];
    const expected = ['A', '_a', 'a', 'a0', 'a_', 'z'];
    const collation = vi.spyOn(String.prototype, 'localeCompare').mockImplementation(() => {
      throw new Error('locale collation must not affect asset admission');
    });
    try {
      for (const kind of ['sprite', 'sound'] as const) {
        for (const order of [names, [...names].reverse()]) {
          const declarations = Object.fromEntries(
            order.map((name) => [name, { kind, path: `${name}.json` }]),
          );
          const files = Object.fromEntries(
            order.map((name) => [
              `${name}.json`,
              encodeAssetFile(
                kind === 'sound'
                  ? sound
                  : { revision: 2, kind: 'sprite', width: 1, height: 1, frames: [[7]] },
              ),
            ]),
          );
          const assets = decodeRuntimeAssets(declarations, files);
          expect(
            (kind === 'sound' ? assets.audio : assets.visual).map((asset) => asset.name),
          ).toEqual(expected);
        }
      }
    } finally {
      collation.mockRestore();
    }
  });

  it('rejects malformed and numerically unsafe sound JSON at the public asset boundary', () => {
    for (const invalid of [
      { ...sound, waveform: 'sample' },
      { ...sound, volume: null },
      { ...sound, pan: undefined },
      { ...sound, duty: '0.5' },
      { ...sound, pitch: { ...sound.pitch, slideSemitonesPerFrame: 1e308 } },
    ]) {
      expect(() =>
        decodeRuntimeAssets(
          { tone: { kind: 'sound', path: 'tone.m01s' } },
          {
            'tone.m01s': encodeAssetFile(invalid),
          },
        ),
      ).toThrow();
    }
  });

  it('bounds console asset banks and rejects noncanonical paths and structural payloads', () => {
    const source = {
      declarations: { hero: { kind: 'sprite', path: 'hero.m01g' } },
      files: { 'hero.m01g': new Uint8Array(4) },
    };
    expect(isRuntimeAssetSource(source)).toBe(true);
    for (const bad of [
      null,
      { declarations: [], files: {} },
      { declarations: {}, files: [] },
      { ...source, host: true },
      { ...source, displayPath: '../display.m01p' },
      { ...source, files: { '/hero.m01g': new Uint8Array(1) } },
      { ...source, files: { 'hero.m01g': [1, 2] } },
      { ...source, files: { 'hero.m01g': new Uint8Array(16 * 1024 * 1024) } },
      { ...source, declarations: { hero: { kind: 'javascript', path: 'hero.m01g' } } },
    ]) {
      expect(isRuntimeAssetSource(bad)).toBe(false);
    }
  });

  it('rejects tracker orders that name inherited object properties', () => {
    expect(() =>
      decodeRuntimeAssets(
        { song: { kind: 'music', path: 'song.m01t' } },
        {
          'song.m01t': encodeAssetFile({
            revision: 1,
            kind: 'music',
            framesPerRow: 2,
            order: ['__proto__'],
            patterns: {},
            loop: true,
          }),
        },
      ),
    ).toThrow(/order/);
  });

  it('adds the file revision and removes runtime names when encoding audio assets', () => {
    const encodedSound: unknown = JSON.parse(
      new TextDecoder().decode(encodeSoundAssetFile({ ...sound, name: 'beep' } as SoundAsset)),
    );
    const encodedMusic: unknown = JSON.parse(
      new TextDecoder().decode(
        encodeMusicAssetFile({
          kind: 'music',
          name: 'theme',
          framesPerRow: 6,
          order: ['00'],
          patterns: { '00': { rows: [[null, null, null, null, null, null, null, null]] } },
          loop: true,
        }),
      ),
    );

    expect(encodedSound).toMatchObject({ revision: 1, kind: 'sound' });
    expect(encodedMusic).toMatchObject({ revision: 1, kind: 'music' });
    expect(encodedSound).not.toHaveProperty('name');
    expect(encodedMusic).not.toHaveProperty('name');
  });

  it('decodes canonical bitmap fonts and charges their exact packed size', () => {
    const bundle = decodeRuntimeAssets(
      { tiny: { kind: 'font', path: 'tiny.m01f' } },
      { 'tiny.m01f': encodeAssetFile(font) },
    );
    expect(bundle.visualBytes).toBe(42);
    expect(bundle.visual).toHaveLength(1);
    const decoded = bundle.visual[0];
    expect(decoded?.kind).toBe('font');
    if (decoded?.kind !== 'font') throw new Error('missing decoded font');
    expect(decoded.glyphs.get(65)).toEqual(Uint8Array.from(font.glyphs[1]?.pixels ?? []));
  });

  it('rejects noncanonical or incomplete bitmap font files', () => {
    for (const invalid of [
      { ...font, glyphs: [...font.glyphs].reverse() },
      { ...font, glyphs: [font.glyphs[0], font.glyphs[0]] },
      { ...font, missingGlyph: 64 },
      {
        ...font,
        glyphs: [{ code: 63, pixels: [2, ...(font.glyphs[0]?.pixels.slice(1) ?? [])] }],
      },
    ]) {
      expect(() =>
        decodeRuntimeAssets(
          { tiny: { kind: 'font', path: 'tiny.m01f' } },
          { 'tiny.m01f': encodeAssetFile(invalid) },
        ),
      ).toThrow(/font/);
    }
  });

  it('loads indexed graphics, map query views, synth patches, and tracker patterns', () => {
    const files = {
      'hero.m01g': encodeAssetFile({
        revision: 2,
        kind: 'sprite',
        width: 2,
        height: 2,
        frames: [[0, 7, 7, 0]],
      }),
      'tiles.m01g': encodeAssetFile({
        revision: 2,
        kind: 'tile_set',
        tiles: [Array.from({ length: 64 }, () => 3)],
        flags: [1],
      }),
      'room.m01m': encodeAssetFile({
        revision: 1,
        kind: 'map',
        layers: [{ width: 2, height: 1, cells: [0, 0], tileSet: 'tiles' }],
      }),
      'beep.m01s': encodeAssetFile(sound),
      'theme.m01t': encodeAssetFile({
        revision: 1,
        kind: 'music',
        framesPerRow: 4,
        order: ['a'],
        patterns: {
          a: {
            rows: [[{ note: 60, sound: 'beep' }, null, null, null, null, null, null, null]],
          },
        },
        loop: true,
      }),
      'display.m01p': encodeAssetFile({
        revision: 2,
        kind: 'display',
        remap: Array.from({ length: 32 }, (_, index) => (index === 1 ? 7 : index)),
        raster: [
          {
            line: 1,
            scrollX: 0,
            scrollY: 0,
            remap: Array.from({ length: 32 }, (_, index) => (index === 7 ? 8 : index)),
          },
        ],
      }),
    };
    const bundle = decodeRuntimeAssets(
      {
        hero: { kind: 'sprite', path: 'hero.m01g' },
        tiles: { kind: 'tile_set', path: 'tiles.m01g' },
        room: { kind: 'map', path: 'room.m01m' },
        beep: { kind: 'sound', path: 'beep.m01s' },
        theme: { kind: 'music', path: 'theme.m01t' },
      },
      files,
      'display.m01p',
    );
    expect(bundle.visual.map((asset) => asset.kind)).toEqual(['sprite', 'map', 'tile_set']);
    expect(bundle.audio.map((asset) => asset.kind)).toEqual(['sound', 'music']);
    expect(bundle.maps[0]?.layers[0]?.tileFlags[0]).toBe(1);
    expect(bundle.visualBytes).toBeGreaterThan(0);
    const frame = new IndexedGraphics(
      new VisualAssetStore(bundle.visual),
      bundle.display,
    ).executeFrame([{ name: 'clear', arguments: [1], sourceSpan: { start: 0, end: 0 } }]);
    expect(frame.indexedPixels[0]).toBe(7);
    expect(frame.indexedPixels[240]).toBe(8);
  });

  it('rejects invalid palette indices and tracker references', () => {
    expect(decodeRuntimeAssets({}, {}, null).display).toBeUndefined();
    expect(() =>
      decodeRuntimeAssets(
        { hero: { kind: 'sprite', path: 'hero.m01g' } },
        {
          'hero.m01g': encodeAssetFile({
            revision: 2,
            kind: 'sprite',
            width: 1,
            height: 1,
            frames: [[256]],
          }),
        },
      ),
    ).toThrow(/indexed pixels/);
    expect(() =>
      decodeRuntimeAssets(
        { theme: { kind: 'music', path: 'theme.m01t' } },
        {
          'theme.m01t': encodeAssetFile({
            revision: 1,
            kind: 'music',
            framesPerRow: 4,
            order: ['a'],
            patterns: {
              a: {
                rows: [[{ note: 60, sound: 'missing' }, null, null, null, null, null, null, null]],
              },
            },
            loop: true,
          }),
        },
      ),
    ).toThrow(/missing sound/);
  });
});
