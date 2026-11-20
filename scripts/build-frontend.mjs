import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { root } from './lib.mjs';

const frontend = path.join(root, 'frontend');
const fresh = path.join(frontend, '.build-out');
const dist = path.join(frontend, 'dist');

function listFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    return e.isDirectory() ? listFiles(p) : [p];
  });
}

const vite = path.join(frontend, 'node_modules', 'vite', 'bin', 'vite.js');
execFileSync(process.execPath, [vite, 'build', '--outDir', fresh, '--emptyOutDir'], { cwd: frontend, stdio: 'inherit' });

const wanted = new Set();
let changed = 0;
for (const file of listFiles(fresh)) {
  const rel = path.relative(fresh, file);
  wanted.add(rel);
  const target = path.join(dist, rel);
  const bytes = fs.readFileSync(file);
  let same = false;
  try {
    same = fs.readFileSync(target).equals(bytes);
  } catch {}
  if (!same) {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, bytes);
    changed++;
  }
}
for (const file of listFiles(dist)) {
  if (!wanted.has(path.relative(dist, file))) {
    fs.rmSync(file, { force: true });
    changed++;
  }
}
fs.rmSync(fresh, { recursive: true, force: true });

console.log(`frontend/dist ${changed ? `updated (${changed} file${changed === 1 ? '' : 's'})` : 'unchanged'}`);
