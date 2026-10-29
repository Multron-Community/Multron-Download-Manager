import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { root } from './lib.mjs';

const source = path.join(root, 'extension');
const firefox = path.join(root, 'extension-build', 'firefox');

fs.copyFileSync(path.join(root, 'LICENSE'), path.join(source, 'LICENSE'));

function listFiles(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    return e.isDirectory() ? listFiles(p) : [p];
  });
}
const hash = crypto.createHash('sha256');
for (const file of listFiles(source).filter((f) => !/[\\/](build\.json|LICENSE)$/.test(f)).sort()) {
  hash.update(path.relative(source, file).replaceAll('\\', '/'));
  hash.update(fs.readFileSync(file));
}
fs.writeFileSync(path.join(source, 'build.json'), JSON.stringify({ build: hash.digest('hex').slice(0, 16) }) + '\n');
fs.rmSync(firefox, { recursive: true, force: true });
fs.cpSync(source, firefox, { recursive: true });

const manifest = JSON.parse(fs.readFileSync(path.join(source, 'manifest.json'), 'utf8'));
delete manifest.key;
manifest.background = { scripts: [manifest.background?.service_worker ?? 'background.js'] };
manifest.browser_specific_settings = {
  gecko: {
    id: 'multron-dm@multron.community',
    strict_min_version: '121.0'
  }
};
fs.writeFileSync(path.join(firefox, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');

console.log(`Firefox extension -> ${path.relative(root, firefox)}`);

export { firefox };
