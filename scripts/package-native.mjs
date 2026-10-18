/* global process, console */

import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, rmSync, statSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { realpathSync } from 'node:fs';

const root = resolve(import.meta.dirname, '..');
const executableName = process.platform === 'win32' ? 'mod01-studio.exe' : 'mod01-studio';
const release = join(root, 'target', 'release', executableName);
const cliName = process.platform === 'win32' ? 'mod01.exe' : 'mod01';
const cli = join(root, 'target', 'release', cliName);
const host = join(root, 'packages', 'runtime', 'standalone', 'native-host.mjs');
const nodeDirectory = dirname(realpathSync(process.execPath));
const nodeLicense = [join(nodeDirectory, 'LICENSE'), join(nodeDirectory, '../LICENSE')].find(
  existsSync,
);
if (!nodeLicense) throw new Error('Node distribution LICENSE is required for redistribution');
for (const file of [release, cli, host, process.execPath]) {
  if (!existsSync(file) || !statSync(file).isFile())
    throw new Error(`missing native package input: ${file}`);
}

const output = join(root, 'dist', `mod01-native-${process.platform}-${process.arch}`);
rmSync(output, { recursive: true, force: true });
let binaries = output;
if (process.platform === 'darwin') {
  binaries = join(output, 'MOD-01 Studio.app', 'Contents', 'MacOS');
  mkdirSync(binaries, { recursive: true });
  const plist = join(output, 'MOD-01 Studio.app', 'Contents', 'Info.plist');
  const { writeFileSync } = await import('node:fs');
  writeFileSync(
    plist,
    `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.mod01.studio</string>
<key>CFBundleName</key><string>MOD-01 Studio</string>
<key>CFBundleExecutable</key><string>mod01-studio</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
</dict></plist>\n`,
  );
} else {
  mkdirSync(binaries, { recursive: true });
}
copyFileSync(release, join(binaries, executableName));
copyFileSync(cli, join(binaries, cliName));
copyFileSync(nodeLicense, join(binaries, 'LICENSE'));
copyFileSync(host, join(binaries, 'native-host.mjs'));
const bundledNode = join(binaries, process.platform === 'win32' ? 'node.exe' : 'node');
copyFileSync(process.execPath, bundledNode);
if (process.platform !== 'win32') {
  chmodSync(join(binaries, executableName), 0o755);
  chmodSync(bundledNode, 0o755);
}
const bundledHost = join(binaries, 'native-host.mjs');
const smoke = execFileSync(
  bundledNode,
  ['--permission', `--allow-fs-read=${bundledHost}`, bundledHost],
  {
    input: '{"type":"stop"}\n',
    encoding: 'utf8',
    env: {
      PATH: process.env.PATH ?? '',
      ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}),
    },
  },
);
if (JSON.parse(smoke).type !== 'stopped') throw new Error('bundled runtime smoke test failed');
const archive = `${output}.tar.gz`;
const samples =
  process.platform === 'darwin'
    ? join(output, 'MOD-01 Studio.app', 'Contents', 'Resources', 'cartridges')
    : join(output, 'cartridges');
cpSync(join(root, 'cartridges'), samples, { recursive: true });
const visualCredits =
  process.platform === 'darwin'
    ? join(output, 'MOD-01 Studio.app', 'Contents', 'Resources', 'visual-credits')
    : join(output, 'visual-credits');
mkdirSync(visualCredits, { recursive: true });
copyFileSync(
  join(root, 'crates', 'mod01-native', 'assets', 'ATTRIBUTION.md'),
  join(visualCredits, 'ATTRIBUTION.md'),
);
copyFileSync(
  join(root, 'crates', 'mod01-native', 'assets', 'bigblue', 'LICENSE.TXT'),
  join(visualCredits, 'BigBlueTerm-LICENSE.TXT'),
);
copyFileSync(
  join(root, 'crates', 'mod01-native', 'assets', 'kenney', 'License.txt'),
  join(visualCredits, 'Kenney-LICENSE.txt'),
);
rmSync(archive, { force: true });
execFileSync('tar', ['-czf', archive, '-C', join(root, 'dist'), basename(output)]);
console.log(`Packaged native Studio: ${output}`);
console.log(`Portable archive: ${archive}`);
console.log(`Bundled JavaScript host uses ${basename(process.execPath)} ${process.version}.`);
