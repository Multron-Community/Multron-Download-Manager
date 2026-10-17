import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { root } from './lib.mjs';

const STEPS = 10;

export function nextVersion(version) {
  let [major, minor, patch] = version.split('-')[0].split('.').map(Number);
  patch += 1;
  if (patch > STEPS) {
    patch = 0;
    minor += 1;
  }
  if (minor > STEPS) {
    minor = 0;
    major += 1;
  }
  return `${major}.${minor}.${patch}`;
}

function currentVersion() {
  return JSON.parse(fs.readFileSync(path.join(root, 'src-tauri', 'tauri.conf.json'), 'utf8')).version;
}

function tagExists(tag) {
  try {
    execFileSync('git', ['rev-parse', '-q', '--verify', `refs/tags/${tag}`], { cwd: root, stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

function updateJson(rel, mutate) {
  const file = path.join(root, rel);
  const data = JSON.parse(fs.readFileSync(file, 'utf8'));
  mutate(data);
  fs.writeFileSync(file, JSON.stringify(data, null, 2) + '\n');
  console.log(`  ${rel}`);
}

function setVersion(version) {
  console.log(`Setting version ${version} in:`);
  updateJson('package.json', (d) => (d.version = version));
  updateJson('frontend/package.json', (d) => (d.version = version));
  updateJson('src-tauri/tauri.conf.json', (d) => (d.version = version));
  updateJson('extension/manifest.json', (d) => (d.version = version.split('-')[0]));

  const cargo = path.join(root, 'src-tauri', 'Cargo.toml');
  const toml = fs.readFileSync(cargo, 'utf8').replace(/^version = ".*"$/m, `version = "${version}"`);
  fs.writeFileSync(cargo, toml);
  console.log('  src-tauri/Cargo.toml');

  execFileSync('cargo', ['update', '--workspace', '--offline'], { cwd: path.join(root, 'src-tauri'), stdio: 'inherit' });
}

function main() {
  const args = process.argv.slice(2);
  const flags = new Set(args.filter((a) => a.startsWith('--')));
  const explicit = args.find((a) => !a.startsWith('--'));
  if (explicit && !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(explicit)) {
    console.error('Usage: npm run release [-- <major.minor.patch>] [--dry-run] [--no-git]');
    process.exit(1);
  }

  const current = currentVersion();
  const version = explicit ?? (tagExists(`v${current}`) ? nextVersion(current) : current);
  if (tagExists(`v${version}`)) {
    console.error(`Tag v${version} already exists.`);
    process.exit(1);
  }
  if (flags.has('--dry-run')) {
    console.log(version);
    return;
  }
  if (!flags.has('--no-git')) {
    const dirty = execFileSync('git', ['status', '--porcelain'], { cwd: root }).toString().trim();
    if (dirty) {
      console.error('Commit your changes first; the release tag would not include them:\n' + dirty);
      process.exit(1);
    }
  }

  if (version !== current) setVersion(version);
  else console.log(`Releasing the current version ${version}.`);

  const notes = path.join(root, 'release-notes', `v${version}.md`);
  if (!fs.existsSync(notes)) console.log(`Note: release-notes/v${version}.md does not exist; the release notes are generated from the commits and pull requests.`);

  if (flags.has('--no-git')) return;

  const git = (...a) => execFileSync('git', a, { cwd: root, stdio: 'inherit' });
  if (version !== current) {
    git('add', 'package.json', 'frontend/package.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'extension/manifest.json');
    git('commit', '-m', `chore: release v${version}`);
  }
  git('tag', '-a', `v${version}`, '-m', `Multron Download Manager v${version}`);
  console.log(`\nTagged v${version} locally. Publish it with:\n  git push origin HEAD --follow-tags\nThe "Release" workflow then builds Windows, macOS and Linux and attaches everything to one GitHub release.`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
