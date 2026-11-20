import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { root } from './lib.mjs';

const source = path.join(root, 'extension');
const firefox = path.join(root, 'extension-build', 'firefox');

function writeIfChanged(file, data) {
  const bytes = Buffer.isBuffer(data) ? data : Buffer.from(data);
  try {
    if (fs.readFileSync(file).equals(bytes)) return false;
  } catch {}
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, bytes);
  return true;
}

function listFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    return e.isDirectory() ? listFiles(p) : [p];
  });
}

writeIfChanged(path.join(source, 'LICENSE'), fs.readFileSync(path.join(root, 'LICENSE')));

const hash = crypto.createHash('sha256');
for (const file of listFiles(source).filter((f) => !/[\\/](build\.json|LICENSE)$/.test(f)).sort()) {
  hash.update(path.relative(source, file).replaceAll('\\', '/'));
  hash.update(fs.readFileSync(file));
}
writeIfChanged(path.join(source, 'build.json'), JSON.stringify({ build: hash.digest('hex').slice(0, 16) }) + '\n');

const manifest = JSON.parse(fs.readFileSync(path.join(source, 'manifest.json'), 'utf8'));
delete manifest.key;
manifest.background = { scripts: [manifest.background?.service_worker ?? 'background.js'] };
manifest.browser_specific_settings = {
  gecko: {
    id: 'multron-dm@multron.community',
    strict_min_version: '121.0',
    data_collection_permissions: { required: ['none'] }
  }
};

const wanted = new Set();
let changed = 0;
for (const file of listFiles(source)) {
  const rel = path.relative(source, file);
  wanted.add(rel);
  const content = rel === 'manifest.json' ? JSON.stringify(manifest, null, 2) + '\n' : fs.readFileSync(file);
  if (writeIfChanged(path.join(firefox, rel), content)) changed++;
}
for (const file of listFiles(firefox)) {
  if (!wanted.has(path.relative(firefox, file))) {
    fs.rmSync(file, { force: true });
    changed++;
  }
}

console.log(`Firefox extension -> ${path.relative(root, firefox)}${changed ? ` (${changed} file${changed === 1 ? '' : 's'} updated)` : ' (up to date)'}`);

export { firefox };
