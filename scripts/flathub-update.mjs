import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { root } from './lib.mjs';

const ID = 'io.github.Multron_Community.MultronDownloadManager';
const REPO = 'https://github.com/Multron-Community/Multron-Download-Manager';
const manifest = path.join(root, 'packaging', 'flathub', `${ID}.yml`);
const flathubInfo = path.join(root, 'packaging', 'flathub', `${ID}.metainfo.xml`);
const sourceInfo = path.join(root, 'flatpak', `${ID}.metainfo.xml`);

const version = process.argv[2] ?? JSON.parse(fs.readFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`Not a release version: ${version}`);
  process.exit(1);
}

async function sha256Of(url) {
  const res = await fetch(url, { redirect: 'follow' });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}. Is v${version} published with its .deb files?`);
  const hash = crypto.createHash('sha256').update(Buffer.from(await res.arrayBuffer())).digest('hex');
  console.log(`${path.basename(url)}  ${hash}`);
  return hash;
}

const debs = {};
for (const arch of ['amd64', 'arm64']) {
  const url = `${REPO}/releases/download/v${version}/Multron.Download.Manager_${version}_${arch}.deb`;
  debs[arch] = { url, sha256: await sha256Of(url) };
}

let yml = fs.readFileSync(manifest, 'utf8');
for (const arch of ['amd64', 'arm64']) {
  const re = new RegExp(`(url: )\\S+_${arch}\\.deb(\\r?\\n\\s*sha256: )[0-9a-f]{64}`);
  if (!re.test(yml)) throw new Error(`No ${arch} .deb source found in ${path.relative(root, manifest)}`);
  yml = yml.replace(re, `$1${debs[arch].url}$2${debs[arch].sha256}`);
}
fs.writeFileSync(manifest, yml);

const newer = (a, b) => {
  const [x, y] = [a, b].map((v) => v.split('.').map(Number));
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
};
const source = fs.readFileSync(sourceInfo, 'utf8');
const found = [...source.matchAll(/[ \t]*<release version="([^"]+)"[\s\S]*?<\/release>/g)].filter((m) => !newer(m[1], version));
const entries = found.map((m) => m[0].replace(/^[ \t]*/, '    '));
if (!found.some((m) => m[1] === version)) {
  const today = new Date().toISOString().slice(0, 10);
  entries.unshift(`    <release version="${version}" date="${today}">\n      <url type="details">${REPO}/releases/tag/v${version}</url>\n    </release>`);
}
const info = fs.readFileSync(flathubInfo, 'utf8');
const updated = info.replace(/<releases>[\s\S]*?<\/releases>/, `<releases>\n${entries.join('\n')}\n  </releases>`);
fs.writeFileSync(flathubInfo, updated);

console.log(`Flathub manifest and metainfo now point at v${version} (${entries.length} release entries).`);
