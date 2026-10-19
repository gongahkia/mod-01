/* global process, console */
import { execFileSync } from 'node:child_process';
import { deepStrictEqual } from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const suffix = process.platform === 'win32' ? '.exe' : '';
const player = resolve(process.argv[2] ?? join(root, 'target/debug', `mod01-studio${suffix}`));
const cli = resolve(process.argv[3] ?? join(dirname(player), `mod01${suffix}`));
const cartridge = resolve(process.argv[4] ?? join(root, 'cartridges/pocket-relay'));
const temporary = mkdtempSync(join(tmpdir(), 'mod01-native-game-'));
const isolatedEnvironment = {
  PATH: '',
  ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}),
};

function verifyPackage(cartridge, name) {
  const output = join(temporary, name);
  const expected = JSON.parse(
    execFileSync(player, ['--verify', cartridge, '--frames', '120'], { encoding: 'utf8' }),
  );
  execFileSync(cli, ['export', 'native', cartridge, '--output', output, '--player', player], {
    stdio: 'pipe',
  });
  const metadata = JSON.parse(readFileSync(join(output, 'mod01-game.json'), 'utf8'));
  const executable = join(output, metadata.executable);
  const binaries = dirname(executable);
  const rom = join(binaries, 'game.m01c');
  const bytes = readFileSync(rom);
  deepStrictEqual(metadata.gameIdentity, expected.gameIdentity);
  deepStrictEqual(metadata.gameIdentity, createHash('sha256').update(bytes).digest('hex'));
  deepStrictEqual(metadata.cartridgeBytes, bytes.length);
  deepStrictEqual(metadata.cartridgeId, expected.cartridge);
  deepStrictEqual(metadata.members.main.cartridgeId, expected.cartridge);
  for (const [alias, count] of Object.entries(expected.memberFrames)) {
    if (!metadata.members[alias] || count < 1) throw new Error('invalid packaged member catalog');
  }
  if (
    !existsSync(join(binaries, `node${suffix}`)) ||
    !existsSync(join(output, 'licenses/Node.txt'))
  )
    throw new Error('game package is missing its runtime or notices');
  const trace = JSON.parse(
    execFileSync(executable, ['--verify', rom, '--frames', '120'], {
      cwd: temporary,
      encoding: 'utf8',
      env: isolatedEnvironment,
    }),
  );
  deepStrictEqual(trace, expected);
  const packed = join(temporary, `${name}-expected.m01c`);
  execFileSync(cli, ['pack', cartridge, '--output', packed]);
  deepStrictEqual(bytes, readFileSync(packed));
  const inspected = JSON.parse(
    execFileSync(cli, ['info', rom, '--source'], {
      encoding: 'utf8',
      cwd: temporary,
      env: isolatedEnvironment,
      maxBuffer: 16 * 1024 * 1024,
    }),
  );
  deepStrictEqual(inspected.manifest.id, metadata.cartridgeId);
  deepStrictEqual((inspected.bundle?.length ?? 0) + 1, Object.keys(metadata.members).length);
  console.log(
    `Verified ${metadata.platform}/${metadata.architecture} ${name}: 120 native frames/PCM/routing/save commits, complete canonical source-visible game, bundled Node, no PATH dependency.`,
  );
  return { metadata, trace };
}

function transferGame() {
  const notes = Array.from({ length: 600_000 }, (_, index) =>
    String.fromCharCode(97 + ((index * 17 + Math.floor(index / 29)) % 26)),
  ).join('');
  const sound = JSON.stringify({
    revision: 1,
    kind: 'sound',
    waveform: 'triangle',
    note: 60,
    durationFrames: 4,
    volume: 0.5,
    pan: 0,
    envelope: { attackFrames: 0, decayFrames: 0, sustainLevel: 1, releaseFrames: 1 },
    pitch: { slideSemitonesPerFrame: 0, vibratoDepthSemitones: 0, vibratoPeriodFrames: 0 },
  });
  const member = (alias, owner, next, parameter, rate) => {
    const condition =
      alias === 'main'
        ? 'cart_parameter() == "" or cart_parameter() == "returned"'
        : 'cart_parameter() == "score=7"';
    const code = `state visits: Int = 0
on start:
  assert ${condition}, "transfer parameter"
  assert save_get_int("owner", ${owner}) == ${owner}, "separate save identity"
  save_set_int("owner", ${owner})
  visits = save_get_int("visits", 0) + 1
  save_set_int("visits", visits)
  save_commit()
  sfx(#tone)
on update:
  load_cart("${next}", "${parameter}")
on draw:
  clear(${owner})
  rect_fill(visits + 8, 10, 4, 4, 7)
`;
    return `format = 2
recipe = []
test = []
[cartridge]
language = "MODL/1"
id = ${JSON.stringify(alias === 'main' ? 'node' : `mod01-package-${alias}`)}
title = "PACKAGE ${alias}"
author = "@mod01"
version = "1.0.0"
entry = "src/main.modl"
update_rate = ${rate}
hardware_revision = 2
compile_on_load = true
[[module]]
path = "src/main.modl"
source = ${JSON.stringify(code)}
[[asset]]
name = "tone"
kind = "sound"
path = "assets/tone.m01s"
payload = ${JSON.stringify(sound)}
[[file]]
path = "authoring/notes.txt"
payload = ${JSON.stringify(notes)}
`;
  };
  const path = join(temporary, 'transfer-game.m01w');
  const child = member('arena', 2, 'main', 'returned', 30);
  writeFileSync(
    path,
    `${member('main', 1, 'arena', 'score=7', 60)}
[[bundle]]
alias = "arena"
source = ${JSON.stringify(child)}
`,
  );
  return path;
}

try {
  verifyPackage(cartridge, 'game');
  const { metadata, trace } = verifyPackage(transferGame(), 'transfer-game');
  deepStrictEqual(trace.memberFrames, { arena: 60, main: 60 });
  deepStrictEqual(trace.saveCommitFrames, { arena: 60, main: 60 });
  if (!(trace.audioPeak > 0)) throw new Error('transfer game did not produce audible PCM');
  if (metadata.cartridgeBytes <= 1024 * 1024)
    throw new Error('large game fixture did not exceed one ROM');
  for (const member of Object.values(metadata.members)) {
    if (member.romBytes >= 1024 * 1024) throw new Error('logical ROM exceeds its own capacity');
  }
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
