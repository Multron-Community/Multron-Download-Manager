import path from 'node:path';
import { appVersion, root, zipDirectory } from './lib.mjs';
import { firefox } from './extension-builds.mjs';

const version = appVersion();
for (const [name, dir] of [
  ['chromium', path.join(root, 'extension')],
  ['firefox', firefox]
]) {
  const out = path.join(root, 'dist', `multron-browser-extension-${name}-v${version}.zip`);
  const count = zipDirectory(dir, out);
  console.log(`Packed ${count} files -> ${path.relative(root, out)}`);
}
