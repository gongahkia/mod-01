/* global process, console */
// Real window startup + first committed frame + orderly close, not --verify.
import { execFileSync, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { strict as assert } from 'node:assert';
import {
  existsSync,
  mkdtempSync,
  openSync,
  closeSync,
  copyFileSync,
  readFileSync,
  mkdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { stageWindowsGraphicsDriver } from './native-window-driver.mjs';

const root = resolve(import.meta.dirname, '..');
const platform = process.platform;
assert.ok(['linux', 'win32', 'darwin'].includes(platform), 'unsupported desktop OS');
// directories uses native known folders on Windows/macOS, not APPDATA/XDG overrides.
// Do not change a developer's real library metadata just to run an automated test.
assert.ok(platform === 'linux' || process.env.CI === 'true', 'Windows/macOS window QA is CI-only');
const suffix = platform === 'win32' ? '.exe' : '';
const player = resolve(process.argv[2] ?? `target/release/mod01-studio${suffix}`);
const cli = resolve(process.argv[3] ?? join(dirname(player), `mod01${suffix}`));
const temporary = mkdtempSync(join(tmpdir(), 'mod01-window-'));
const id = `gui-qa-${randomUUID()}`;
const windowsRoaming =
  platform === 'win32'
    ? execute('powershell.exe', [
        '-NoProfile',
        '-NonInteractive',
        '-Command',
        '[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); [Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData)',
      ]).trim()
    : undefined;
const data =
  platform === 'linux'
    ? join(temporary, 'data', 'nativestudio')
    : platform === 'darwin'
      ? join(homedir(), 'Library', 'Application Support', 'MOD-01.Native-Studio')
      : join(windowsRoaming, 'MOD-01', 'Native Studio', 'data');
const save = join(data, 'saves', `cart-${id}.bin`);
const log = join(temporary, 'player.log');
const closeX11 = join(temporary, 'close-window');
let child;
let ownsSave = false;
let succeeded = false;
let window;
let failure;

function windowsWindow(extra = []) {
  return execute('powershell.exe', [
    '-NoProfile',
    '-ExecutionPolicy',
    'Bypass',
    '-File',
    join(root, 'scripts/native-window.ps1'),
    '-GameProcessId',
    String(child.pid),
    ...extra,
  ]);
}

function execute(command, args) {
  return execFileSync(command, args, { encoding: 'utf8', timeout: 45_000 });
}

async function waitUntil(predicate, description, milliseconds = 10_000) {
  const deadline = Date.now() + milliseconds;
  while (!predicate()) {
    assert.ok(Date.now() < deadline, `Timed out: ${description}`);
    assert.ok(child.exitCode === null && child.signalCode === null, 'Game exited prematurely');
    await delay(100);
  }
}

try {
  assert.ok(!existsSync(save), 'fresh QA ID must never overlap existing saves');
  if (platform === 'linux') {
    assert.ok(process.env.DISPLAY, 'Linux window QA requires X11 (for example xvfb-run -a)');
    execute('cc', [
      '-Wall',
      '-Wextra',
      '-Werror',
      join(root, 'scripts/native-window-close.c'),
      '-lX11',
      '-o',
      closeX11,
    ]);
  }
  const source = readFileSync(join(root, 'templates/blank/cart.m01w'), 'utf8')
    .replace('id = "blank-cart"', `id = "${id}"`)
    .replace(
      'on draw:\n  clear(1)',
      'on start:\n  save_set_int("gui_ready", 73)\n  save_commit()\non draw:\n  clear(200)\n  rect_fill(8, 8, 32, 32, 7)',
    );
  const workcart = join(temporary, 'cart.m01w');
  const output = join(temporary, 'game');
  writeFileSync(workcart, source);
  execute(cli, ['export', 'native', workcart, '--output', output, '--player', player]);
  const metadata = JSON.parse(readFileSync(join(output, 'mod01-game.json'), 'utf8'));
  const softwareDriver = process.env.MOD01_QA_MESA_DIR;
  if (softwareDriver)
    stageWindowsGraphicsDriver(softwareDriver, dirname(join(output, metadata.executable)), {
      platform,
      ci: process.env.CI === 'true',
    });
  const environment = { ...process.env, PATH: '' };
  delete environment.NODE_OPTIONS;
  if (softwareDriver) {
    environment.GALLIUM_DRIVER = 'llvmpipe';
    environment.LIBGL_ALWAYS_SOFTWARE = '1';
  }
  if (platform === 'linux') {
    delete environment.WAYLAND_DISPLAY;
    environment.LIBGL_ALWAYS_SOFTWARE = '1';
    environment.XDG_DATA_HOME = join(temporary, 'data');
    environment.XDG_CONFIG_HOME = join(temporary, 'config');
    environment.XDG_CACHE_HOME = join(temporary, 'cache');
  }
  const descriptor = openSync(log, 'wx');
  try {
    // No arguments: the normal shipped-game entrypoint must discover game.m01c.
    child = spawn(join(output, metadata.executable), [], {
      cwd: temporary,
      env: environment,
      stdio: ['ignore', descriptor, descriptor],
    });
    child.on('error', (error) => console.error(`Game launch failed: ${error.message}`));
    assert.ok(child.pid, 'could not spawn the packaged game');
    ownsSave = true;
  } finally {
    closeSync(descriptor);
  }
  let x11Window;
  if (platform === 'win32') {
    window = JSON.parse(windowsWindow(['-ExpectedTitle', 'BLANK CART']));
  } else if (platform === 'darwin') {
    window = JSON.parse(
      execute('swift', [join(root, 'scripts/native-window.swift'), String(child.pid)]),
    );
  } else {
    await waitUntil(
      () => {
        try {
          x11Window = execute('xdotool', ['search', '--onlyvisible', '--pid', String(child.pid)])
            .trim()
            .split('\n')[0];
          return Boolean(x11Window);
        } catch {
          return false;
        }
      },
      'visible owned X11 window',
      30_000,
    );
    const geometry = Object.fromEntries(
      execute('xdotool', ['getwindowgeometry', '--shell', x11Window])
        .trim()
        .split('\n')
        .map((line) => line.split('=')),
    );
    window = {
      pid: child.pid,
      width: Number(geometry.WIDTH),
      height: Number(geometry.HEIGHT),
      visible: true,
    };
  }
  assert.equal(window.pid, child.pid);
  assert.equal(window.visible, true);
  assert.ok(window.width >= 480 && window.height >= 320);
  console.log(`Observed owned game window ${JSON.stringify(window)}; expected save: ${save}`);
  await waitUntil(
    () => {
      try {
        const bytes = readFileSync(save);
        return (
          bytes.length === 8192 &&
          JSON.parse(bytes.subarray(0, bytes.indexOf(0)).toString()).gui_ready === 73
        );
      } catch {
        return false;
      }
    },
    'first real game frame committed by this fresh cartridge',
    softwareDriver ? 30_000 : 10_000,
  );
  if (platform === 'win32') {
    windowsWindow([
      '-ExpectedTitle',
      'BLANK CART',
      '-CapturePath',
      join(temporary, 'window-ready.png'),
    ]);
    windowsWindow(['-ExpectedTitle', 'BLANK CART', '-Close']);
  } else if (platform === 'darwin') {
    execute('swift', [join(root, 'scripts/native-window.swift'), String(child.pid), '--close']);
  } else {
    assert.equal(Number(execute('xdotool', ['getwindowpid', x11Window]).trim()), child.pid);
    execute(closeX11, [x11Window]);
  }
  // Let Node receive the child's exit event; never call a forced kill a clean shutdown.
  const deadline = Date.now() + 10_000;
  while (child.exitCode === null && child.signalCode === null && Date.now() < deadline)
    await delay(100);
  assert.equal(child.signalCode, null, 'game died from a signal');
  assert.equal(child.exitCode, 0, 'game did not close normally');
  console.log(
    `Verified ${platform}: actual ${window.width}x${window.height} packaged window, no-argument/empty-PATH launch, first real frame/save and clean close${softwareDriver ? ' (Mesa llvmpipe, CI-only)' : ''}.`,
  );
  succeeded = true;
} catch (error) {
  failure = error.message;
  if (platform === 'win32' && child?.pid && child.exitCode === null && child.signalCode === null) {
    try {
      windowsWindow(['-CapturePath', join(temporary, 'window-failure.png')]);
    } catch (captureError) {
      console.error(`Window diagnostics: ${captureError.message}`);
    }
  }
  throw error;
} finally {
  if (child?.pid && child.exitCode === null && child.signalCode === null) {
    child.kill();
    const deadline = Date.now() + 5000;
    while (child.exitCode === null && child.signalCode === null && Date.now() < deadline)
      await delay(100);
  }
  const stillRunning = child?.pid && child.exitCode === null && child.signalCode === null;
  if (process.env.CI === 'true' && process.env.RUNNER_TEMP) {
    const evidenceRoot = join(process.env.RUNNER_TEMP, 'mod01-native-window');
    const evidence = join(evidenceRoot, id);
    mkdirSync(evidenceRoot, { recursive: true });
    mkdirSync(evidence);
    for (const file of ['player.log', 'window-ready.png', 'window-failure.png']) {
      const source = join(temporary, file);
      if (existsSync(source)) copyFileSync(source, join(evidence, file));
    }
    writeFileSync(
      join(evidence, 'report.json'),
      JSON.stringify(
        {
          id,
          window,
          save,
          saveExists: existsSync(save),
          saveBytes: existsSync(save) ? readFileSync(save).length : 0,
          succeeded,
          failure,
        },
        null,
        2,
      ),
      { flag: 'wx' },
    );
  }
  if (ownsSave && !stillRunning) rmSync(save, { force: true });
  if (succeeded) rmSync(temporary, { recursive: true, force: true });
  else {
    if (existsSync(log)) console.error(readFileSync(log, 'utf8'));
    console.error(`Retained owned GUI QA fixture: ${temporary}`);
  }
}
