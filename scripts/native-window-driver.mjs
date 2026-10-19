// Test infrastructure only: never called by native export or included in its archives.
import { strict as assert } from 'node:assert';
import { constants, copyFileSync, existsSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

export function stageWindowsGraphicsDriver(source, destination, { platform, ci }) {
  assert.ok(platform === 'win32' && ci, 'Software graphics staging is Windows CI-only');
  for (const required of ['opengl32.dll', 'libgallium_wgl.dll']) {
    const path = join(source, required);
    assert.ok(existsSync(path) && statSync(path).isFile(), `Missing Mesa driver: ${required}`);
  }
  const files = readdirSync(source, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /\.dll$/i.test(entry.name))
    .map((entry) => entry.name)
    .sort();
  // Preflight every collision before copying anything into the owned QA package.
  for (const file of files)
    assert.ok(!existsSync(join(destination, file)), `QA driver target already exists: ${file}`);
  for (const file of files)
    copyFileSync(join(source, file), join(destination, file), constants.COPYFILE_EXCL);
  return files;
}
