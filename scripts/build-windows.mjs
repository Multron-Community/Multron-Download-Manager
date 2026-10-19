import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { appVersion, root, zipDirectory } from './lib.mjs';
import { PAD_NAME, padXml } from './pad.mjs';

if (process.platform !== 'win32') {
  console.error('The Inno Setup installer can only be built on Windows.');
  process.exit(1);
}

const args = process.argv.slice(2);
const version = appVersion();
const exe = path.join(root, 'src-tauri', 'target', 'release', 'multron-download-manager.exe');

execFileSync(process.execPath, [path.join(root, 'scripts', 'extension-builds.mjs')], { stdio: 'inherit' });
if (!args.includes('--skip-build')) {
  const tauriCli = path.join(root, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
  execFileSync(process.execPath, [tauriCli, 'build', '--no-bundle'], { cwd: root, stdio: 'inherit' });
}
if (!fs.existsSync(exe)) {
  console.error(`Missing ${exe}. Run without --skip-build.`);
  process.exit(1);
}

const candidates = [
  process.env.ISCC,
  path.join(process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)', 'Inno Setup 6', 'ISCC.exe'),
  path.join(process.env.ProgramFiles ?? 'C:\\Program Files', 'Inno Setup 6', 'ISCC.exe'),
  path.join(process.env.LOCALAPPDATA ?? '', 'Programs', 'Inno Setup 6', 'ISCC.exe')
].filter(Boolean);
const iscc = candidates.find((p) => fs.existsSync(p));
if (!iscc) {
  console.error('Inno Setup 6 was not found. Install it with:\n  winget install -e --id JRSoftware.InnoSetup\nor set the ISCC environment variable to ISCC.exe.');
  process.exit(1);
}

const defines = [`/DAppVersion=${version}`, `/DSourceExe=${exe}`, `/DOutputDir=${path.join(root, 'dist')}`];
const wv = args.indexOf('--webview2');
if (wv !== -1 && args[wv + 1]) defines.push(`/DWebView2Bootstrapper=${path.resolve(args[wv + 1])}`);

execFileSync(iscc, [...defines, path.join(root, 'installer', 'windows', 'multron.iss')], { stdio: 'inherit' });
console.log(`\nInstaller: dist/Multron-Download-Manager-${version}-x64-Setup.exe`);

const setup = path.join(root, 'dist', `Multron-Download-Manager-${version}-x64-Setup.exe`);
fs.writeFileSync(
  path.join(root, PAD_NAME),
  padXml({ version, date: new Date().toISOString().slice(0, 10), size: fs.statSync(setup).size })
);
console.log(`PAD file: ${PAD_NAME}`);

const staging = path.join(root, 'dist', 'portable');
const folder = path.join(staging, 'Multron-Download-Manager');
fs.rmSync(staging, { recursive: true, force: true });
fs.mkdirSync(folder, { recursive: true });
fs.copyFileSync(exe, path.join(folder, 'multron-download-manager.exe'));
for (const file of ['LICENSE', 'THIRD-PARTY-NOTICES.md']) fs.copyFileSync(path.join(root, file), path.join(folder, file));
fs.cpSync(path.join(root, 'extension'), path.join(folder, 'browser-extension'), { recursive: true });
fs.cpSync(path.join(root, 'extension-build', 'firefox'), path.join(folder, 'browser-extension-firefox'), { recursive: true });
fs.writeFileSync(
  path.join(folder, 'portable.txt'),
  'This copy of Multron Download Manager is portable.\r\nSettings, the download list and updates are kept in the data folder next to the program.\r\nDelete this file to use the normal per-user folders instead.\r\n'
);
const portable = path.join(root, 'dist', `Multron-Download-Manager-${version}-x64-portable.zip`);
fs.rmSync(portable, { force: true });
zipDirectory(staging, portable);
fs.rmSync(staging, { recursive: true, force: true });
console.log(`Portable: dist/Multron-Download-Manager-${version}-x64-portable.zip`);
