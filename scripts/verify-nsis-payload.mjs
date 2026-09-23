import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Tauri patches the first bundle marker for NSIS, then restores the build binary.
// Mirror only that exact documented patch; every other byte must match.
export function verifyPayload(built, installed) {
  const original = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK');
  const nsis = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_NSS');
  const expected = Buffer.from(built);
  const offset = expected.indexOf(original);
  if (offset < 0) throw new Error('Expected Tauri bundle marker not found');
  nsis.copy(expected, offset);
  if (!expected.equals(installed)) throw new Error('Installed executable differs from expected NSIS payload');
  return createHash('sha256').update(installed).digest('hex');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [builtPath, installedPath] = process.argv.slice(2);
  if (!builtPath || !installedPath) throw new Error('Expected built and installed executable paths');
  const [built, installed] = await Promise.all([readFile(builtPath), readFile(installedPath)]);
  console.log(JSON.stringify({ installedPayloadSha256: verifyPayload(built, installed) }));
}
