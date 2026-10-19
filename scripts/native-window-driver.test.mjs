import { afterEach, beforeEach, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { stageWindowsGraphicsDriver } from './native-window-driver.mjs';

let temporary;
let source;
let destination;
const windowsCI = { platform: 'win32', ci: true };
beforeEach(() => {
  temporary = mkdtempSync(join(tmpdir(), 'mod01-qa-driver-'));
  source = join(temporary, 'driver');
  destination = join(temporary, 'game');
  mkdirSync(source);
  mkdirSync(destination);
});
afterEach(() => {
  if (temporary) rmSync(temporary, { recursive: true, force: true });
});

it('rejects non-Windows and personal-profile staging before touching the destination', () => {
  for (const environment of [
    { platform: 'linux', ci: true },
    { platform: 'win32', ci: false },
  ])
    expect(() => stageWindowsGraphicsDriver(source, destination, environment)).toThrow(/CI-only/);
  expect(readdirSync(destination)).toEqual([]);
});

it('requires both OpenGL entrypoint and its actual Gallium driver before copying', () => {
  writeFileSync(join(source, 'opengl32.dll'), 'fixture entrypoint, not an executable');
  expect(() => stageWindowsGraphicsDriver(source, destination, windowsCI)).toThrow(/Missing Mesa/);
  expect(readdirSync(destination)).toEqual([]);
});

it('stages DLL dependencies only, without running upstream deployment scripts', () => {
  for (const file of ['opengl32.dll', 'libgallium_wgl.dll', 'LLVM.dll'])
    writeFileSync(join(source, file), `owned fixture: ${file}`);
  writeFileSync(join(source, 'deploy.cmd'), 'never execute or copy this fixture');
  const files = stageWindowsGraphicsDriver(source, destination, windowsCI);
  expect(files).toEqual(['LLVM.dll', 'libgallium_wgl.dll', 'opengl32.dll']);
  expect(readdirSync(destination).sort()).toEqual(files);
  for (const file of files)
    expect(readFileSync(join(destination, file), 'utf8')).toBe(`owned fixture: ${file}`);
});

it('refuses every existing target without overwriting it or partially staging other DLLs', () => {
  for (const file of ['opengl32.dll', 'libgallium_wgl.dll'])
    writeFileSync(join(source, file), file);
  writeFileSync(join(destination, 'libgallium_wgl.dll'), 'keep this existing file');
  expect(() => stageWindowsGraphicsDriver(source, destination, windowsCI)).toThrow(
    /already exists/,
  );
  expect(readdirSync(destination)).toEqual(['libgallium_wgl.dll']);
  expect(readFileSync(join(destination, 'libgallium_wgl.dll'), 'utf8')).toBe(
    'keep this existing file',
  );
});
