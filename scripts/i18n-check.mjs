
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const src = path.join(root, 'frontend', 'src');
const langDir = path.join(src, 'lang');

function files(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) return e.name === 'lang' ? [] : files(p);
    return /\.(svelte|ts)$/.test(e.name) ? [p] : [];
  });
}

const unescape = (s) => s.replace(/\\(['"\\])/g, '$1');
const LIT = String.raw`'((?:[^'\\]|\\.)*)'`;

const EXPLICIT = new RegExp(String.raw`\bt\(\s*` + LIT, 'g');
const CONTEXT = new RegExp(String.raw`\btc\(\s*` + LIT + String.raw`\s*,\s*` + LIT, 'g');

const HEURISTIC = [
  new RegExp(String.raw`@render (?:toggle|row)\(\s*` + LIT + String.raw`(?:\s*,\s*` + LIT + ')?', 'g'),
  new RegExp(String.raw`@render group\(\s*` + LIT, 'g'),
  new RegExp(String.raw`toast\(\s*'\w+'\s*,\s*` + LIT, 'g'),
  new RegExp(String.raw`app\.run\([^\n]*?,\s*` + LIT + String.raw`\s*\)`, 'g'),
  new RegExp(String.raw`\b(?:label|name|title|desc)\s*:\s*` + LIT, 'g'),
  new RegExp(String.raw`\[\s*'[\w-]+'\s*,\s*` + LIT + String.raw`\s*\]`, 'g'),
  new RegExp(String.raw`\b[\w'-]+\s*:\s*'([A-Z][^'\n]*[a-z][^'\n]*)'`, 'g')
];

const IGNORE =
  /^(Multron|GitHub|Chromium|Firefox|SHA-256|MP4|MKV|WebM|English|Türkçe|DHT|UPnP|ETA|URL|IPv4|uTP|Mica|Acrylic|Blur|Vibrancy|application\/.*|[A-Z][a-z]+[A-Z].*)$/;

const isText = (key) => key.trim() && /[A-Za-z]{2}/.test(key) && !/\$\{/.test(key);

export function collectKeys() {
  const keys = new Set();
  for (const file of files(src)) {
    if (file.endsWith('i18n.svelte.ts') || file.endsWith('Icon.svelte')) continue;
    const text = fs.readFileSync(file, 'utf8');
    for (const m of text.matchAll(EXPLICIT)) {
      const key = unescape(m[1]);
      if (isText(key)) keys.add(key);
    }
    for (const m of text.matchAll(CONTEXT)) keys.add(`${unescape(m[1])}|${unescape(m[2])}`);
    for (const re of HEURISTIC) {
      for (const m of text.matchAll(re)) {
        for (const raw of m.slice(1)) {
          if (raw === undefined) continue;
          const key = unescape(raw);
          if (isText(key) && !IGNORE.test(key) && !/^[a-z0-9_-]+$/.test(key)) keys.add(key);
        }
      }
    }
  }
  const native = fs.readFileSync(path.join(root, 'src-tauri', 'src', 'i18n.rs'), 'utf8');
  for (const m of native.matchAll(/"\w+" => "([^"]+)"/g)) keys.add(m[1]);
  return [...keys].sort((a, b) => a.localeCompare(b));
}

function main() {
  const args = new Set(process.argv.slice(2));
  const keys = collectKeys();
  if (args.has('--keys')) {
    console.log(JSON.stringify(keys, null, 2));
    return;
  }
  const langs = fs.readdirSync(langDir).filter((f) => f.endsWith('.json'));
  let failed = false;
  for (const f of langs) {
    const dict = JSON.parse(fs.readFileSync(path.join(langDir, f), 'utf8'));
    const missing = keys.filter((k) => !(k in dict) || !String(dict[k]).trim());
    const unused = Object.keys(dict).filter((k) => !keys.includes(k));
    if (missing.length) {
      failed = true;
      console.log(`${f}: ${missing.length} missing`);
      for (const k of missing) console.log(`  - ${JSON.stringify(k)}`);
    } else {
      console.log(`${f}: complete (${keys.length} strings)`);
    }
    if (args.has('--unused') && unused.length) {
      console.log(`${f}: ${unused.length} unused`);
      for (const k of unused) console.log(`  ~ ${JSON.stringify(k)}`);
    }
  }
  if (failed) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
