import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { root } from './lib.mjs';

const [manifestFile, signatureFile, ...files] = process.argv.slice(2);
if (!manifestFile || !signatureFile) {
  console.error('Usage: node scripts/verify-update.mjs update.json update.json.sig [downloaded files...]');
  process.exit(2);
}

const raw = Buffer.from(fs.readFileSync(path.join(root, 'src-tauri', 'update-key.pub'), 'utf8').trim(), 'base64');
if (raw.length !== 32) {
  console.error('src-tauri/update-key.pub does not hold an Ed25519 public key.');
  process.exit(1);
}
const key = crypto.createPublicKey({
  key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), raw]),
  format: 'der',
  type: 'spki'
});

const manifest = fs.readFileSync(manifestFile);
const signature = Buffer.from(fs.readFileSync(signatureFile, 'utf8').trim(), 'base64');
if (!crypto.verify(null, manifest, key, signature)) {
  console.error('INVALID: update.json was not signed with the Multron Download Manager key.');
  process.exit(1);
}
const data = JSON.parse(manifest);
console.log(`Signature OK: ${data.name} ${data.version}, ${Object.keys(data.files).length} files`);

let failed = false;
for (const file of files) {
  const name = path.basename(file);
  const expected = data.files[name];
  const actual = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  if (!expected) {
    console.log(`NOT LISTED  ${name}`);
    failed = true;
  } else if (expected !== actual) {
    console.log(`MISMATCH    ${name}`);
    failed = true;
  } else {
    console.log(`OK          ${name}`);
  }
}
process.exit(failed ? 1 : 0);
