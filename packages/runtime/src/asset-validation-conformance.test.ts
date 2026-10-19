import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { decodeRuntimeAssets, encodeAssetFile, type ProjectAssetDeclaration } from './asset-codec';
import { VisualAssetStore } from './graphics';

interface Bank {
  declarations: Record<string, ProjectAssetDeclaration>;
  files: Record<string, unknown>;
  display?: string;
}

const sprite = {
  revision: 2,
  kind: 'sprite',
  width: 2,
  height: 1,
  frames: [
    [0, 255],
    [254, 1],
  ],
};
const tiles = {
  revision: 2,
  kind: 'tile_set',
  tiles: [Array<number>(64).fill(1), Array<number>(64).fill(255)],
  flags: [0, 255],
};
const map = {
  revision: 1,
  kind: 'map',
  layers: [{ width: 2, height: 1, tileSet: 'tiles', cells: [0, 1] }],
};
const font = {
  revision: 1,
  kind: 'font',
  glyphWidth: 2,
  glyphHeight: 2,
  baseline: 1,
  advanceX: 2,
  advanceY: 2,
  missingGlyph: 63,
  glyphs: [{ code: 63, pixels: [0, 1, 1, 0] }],
};
const sound = {
  revision: 1,
  kind: 'sound',
  waveform: 'triangle',
  note: 60,
  durationFrames: 4,
  volume: 0.5,
  pan: 0,
  envelope: { attackFrames: 0, decayFrames: 0, sustainLevel: 1, releaseFrames: 1 },
  pitch: { slideSemitonesPerFrame: 0, vibratoDepthSemitones: 0, vibratoPeriodFrames: 0 },
};
const music = {
  revision: 1,
  kind: 'music',
  framesPerRow: 240,
  loop: false,
  order: ['A'],
  patterns: {
    A: {
      rows: [[{ note: 127, sound: 'sound', volume: 0 }, null, null, null, null, null, null, null]],
    },
  },
};
const display = {
  revision: 2,
  kind: 'display',
  remap: Array.from({ length: 32 }, (_, index) => index),
  raster: [{ line: 143, scrollX: -32768, scrollY: 32767, remap: Array<number>(32).fill(255) }],
};

function bank(): Bank {
  return {
    declarations: {
      sprite: { kind: 'sprite', path: 'sprite.json' },
      animation: { kind: 'animation', path: 'sprite.json' },
      tiles: { kind: 'tile_set', path: 'tiles.json' },
      map: { kind: 'map', path: 'map.json' },
      font: { kind: 'font', path: 'font.json' },
      sound: { kind: 'sound', path: 'sound.json' },
      music: { kind: 'music', path: 'music.json' },
    },
    files: {
      'sprite.json': sprite,
      'tiles.json': tiles,
      'map.json': map,
      'font.json': font,
      'sound.json': sound,
      'music.json': music,
      'display.json': display,
    },
    display: 'display.json',
  };
}

const cases: { name: string; bank: Bank; valid: boolean }[] = [
  {
    name: 'every current asset including shared sprite source and raster defaults',
    bank: bank(),
    valid: true,
  },
];
function replace(name: string, path: string, asset: unknown, valid = false) {
  const source = bank();
  source.files[path] = asset;
  cases.push({ name, bank: source, valid });
}
for (const [name, asset] of Object.entries({
  oldRevision: { ...sprite, revision: 1 },
  oversized: { ...sprite, width: 129 },
  emptyAxis: { ...sprite, height: 0 },
  badPixel: { ...sprite, frames: [[0, 256]] },
  missingFrame: { ...sprite, frames: [] },
}))
  replace(`sprite ${name}`, 'sprite.json', asset);
replace('tile shape', 'tiles.json', {
  ...tiles,
  tiles: [Array<number>(63).fill(1), Array<number>(64).fill(255)],
});
replace('tile flags', 'tiles.json', { ...tiles, flags: [] });
for (const tileSet of ['missing', 'sound'])
  replace(`map target ${tileSet}`, 'map.json', {
    ...map,
    layers: [{ width: 1, height: 1, tileSet, cells: [0] }],
  });
replace('out of range tile', 'map.json', {
  ...map,
  layers: [{ width: 1, height: 1, tileSet: 'tiles', cells: [2] }],
});
replace('map shape', 'map.json', { ...map, layers: [] });
replace('font fallback', 'font.json', { ...font, missingGlyph: 65 });
replace('font duplicate code', 'font.json', { ...font, glyphs: [...font.glyphs, ...font.glyphs] });
replace('font mask', 'font.json', { ...font, glyphs: [{ code: 63, pixels: [0, 2, 1, 0] }] });
for (const waveform of ['pulse', 'triangle', 'saw', 'noise', 'wavetable'])
  replace(
    `oscillator ${waveform}`,
    'sound.json',
    { ...sound, waveform, duty: 0.5, wavetable: [-1, 0, 1, 0] },
    true,
  );
for (const duty of [0, 1, '0.5'])
  replace(`pulse duty ${String(duty)}`, 'sound.json', { ...sound, waveform: 'pulse', duty });
replace('fractional note', 'sound.json', { ...sound, note: 60.5 });
replace('nonfinite pitch excursion', 'sound.json', {
  ...sound,
  pitch: { ...sound.pitch, slideSemitonesPerFrame: 1e200 },
});
replace('fractional envelope', 'sound.json', {
  ...sound,
  envelope: { ...sound.envelope, releaseFrames: 0.5 },
});
replace('tempo', 'music.json', { ...music, framesPerRow: 241 });
replace('loop boolean', 'music.json', { ...music, loop: null });
replace('tracker channel count', 'music.json', { ...music, patterns: { A: { rows: [[null]] } } });
replace('unknown order', 'music.json', { ...music, order: ['__proto__'] });
replace('unused pattern reference', 'music.json', {
  ...music,
  patterns: {
    ...music.patterns,
    unused: { rows: [[{ note: 60, sound: 'missing' }, null, null, null, null, null, null, null]] },
  },
});
replace('null cell volume', 'music.json', {
  ...music,
  patterns: {
    A: {
      rows: [
        [{ note: 60, sound: 'sound', volume: null }, null, null, null, null, null, null, null],
      ],
    },
  },
});
replace('32 logical remaps not 256 master colours', 'display.json', {
  ...display,
  remap: Array<number>(256).fill(0),
});
replace('raster order', 'display.json', {
  ...display,
  raster: [...display.raster, ...display.raster],
});
replace('raster scroll range', 'display.json', {
  ...display,
  raster: [{ ...display.raster[0], scrollX: 32768 }],
});
for (const frames of [64, 65])
  cases.push({
    name: `visual bytes ${frames === 64 ? 'exact limit' : 'one frame over'}`,
    valid: frames === 64,
    bank: {
      declarations: { animation: { kind: 'animation', path: 'animation.json' } },
      files: {
        'animation.json': {
          revision: 2,
          kind: 'sprite',
          width: 128,
          height: 128,
          frames: Array.from({ length: frames }, () => Array<number>(128 * 128).fill(1)),
        },
      },
    },
  });
for (const extra of [0, 1])
  cases.push({
    name: `allocation slots ${extra === 0 ? 'exact limit' : 'one over'}`,
    valid: extra === 0,
    bank: {
      declarations: {
        first: { kind: 'tile_set', path: 'first.json' },
        second: { kind: 'tile_set', path: 'second.json' },
      },
      files: {
        'first.json': {
          revision: 2,
          kind: 'tile_set',
          tiles: Array.from({ length: 4095 }, () => Array<number>(64).fill(0)),
          flags: Array<number>(4095).fill(0),
        },
        'second.json': {
          revision: 2,
          kind: 'tile_set',
          tiles: Array.from({ length: 4095 + extra }, () => Array<number>(64).fill(0)),
          flags: Array<number>(4095 + extra).fill(0),
        },
      },
    },
  });

function workcart(source: Bank): string {
  let text = `format = 2\n[cartridge]\nlanguage = "MODL/1"\nid = "asset-conformance"\ntitle = "ASSET CONFORMANCE"\nauthor = "@test"\nversion = "0.1.0"\nentry = "src/main.modl"\nupdate_rate = 60\n${source.display === undefined ? '' : `display = ${JSON.stringify(source.display)}\n`}\n[[module]]\npath = "src/main.modl"\nsource = "on draw:\\n  clear(0)\\n"\n`;
  // Multiple names may share source bytes. Workcart authoring itself requires unique paths,
  // so emit identical bytes under unique canonical paths for this public CLI boundary.
  for (const [name, declaration] of Object.entries(source.declarations)) {
    text += `\n[[asset]]\nname = ${JSON.stringify(name)}\nkind = ${JSON.stringify(declaration.kind)}\npath = ${JSON.stringify(`${name}.json`)}\npayload = ${JSON.stringify(JSON.stringify(source.files[declaration.path]))}\n`;
  }
  if (source.display !== undefined)
    text += `\n[[file]]\npath = ${JSON.stringify(source.display)}\npayload = ${JSON.stringify(JSON.stringify(source.files[source.display]))}\n`;
  return text;
}

describe('Rust authoring validation matches the production console asset boundary', () => {
  const root = fileURLToPath(new URL('../../../', import.meta.url));
  let temporary: string;
  beforeAll(() => {
    temporary = mkdtempSync(join(tmpdir(), 'mod01-asset-conformance-'));
    execFileSync('cargo', ['build', '--quiet', '-p', 'mod01-cli'], { cwd: root });
  }, 120_000);
  afterAll(() => {
    if (temporary) rmSync(temporary, { recursive: true, force: true });
  });
  for (const [index, fixture] of cases.entries())
    it(fixture.name, () => {
      const path = join(temporary, `${String(index)}.m01w`);
      writeFileSync(path, workcart(fixture.bank));
      const result = spawnSync(join(root, 'target/debug/mod01'), ['assets', path], {
        encoding: 'utf8',
        env: { ...process.env, PATH: '' },
      });
      expect(result.error).toBeUndefined();
      const report = JSON.parse(result.stdout) as {
        usage: { visualBytes: number; visualAllocations: number };
        diagnostics: unknown[];
      };
      expect(result.status, result.stderr).toBe(fixture.valid ? 0 : 1);
      expect(report.diagnostics.length === 0).toBe(fixture.valid);
      const decode = () =>
        decodeRuntimeAssets(
          fixture.bank.declarations,
          Object.fromEntries(
            Object.entries(fixture.bank.files).map(([path, value]) => [
              path,
              encodeAssetFile(value),
            ]),
          ),
          fixture.bank.display,
        );
      if (!fixture.valid) {
        expect(decode).toThrow();
        return;
      }
      const decoded = decode();
      expect(report.usage.visualBytes).toBe(decoded.visualBytes);
      const stored = new VisualAssetStore(decoded.visual, decoded.display);
      const info = stored
        .memoryRegions()
        .find((region) => region.name === 'visual allocation status');
      if (!info) throw new Error('missing production allocation info');
      expect(report.usage.visualAllocations).toBe(
        new DataView(info.bytes.buffer, info.bytes.byteOffset, info.bytes.byteLength).getUint32(
          12,
          true,
        ),
      );
    });
});
