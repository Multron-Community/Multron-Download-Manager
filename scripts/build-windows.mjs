import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { appVersion, root } from './lib.mjs';

if (process.platform !== 'win32') {
  console.error('The Inno Setup installer can only be built on Windows.');
  process.exit(1);
}

const args = process.argv.slice(2);
const version = appVersion();
const exe = path.join(root, 'src-tauri', 'target', 'release', 'multron-download-manager.exe');

execFileSync(process.execPath, [path.join(root, 'scripts', 'extension-builds.mjs')], { stdio: 'inherit' });
if (!args.includes('--skip-build')) {
  execFileSync('npx', ['tauri', 'build', '--no-bundle'], { cwd: root, stdio: 'inherit', shell: true });
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
