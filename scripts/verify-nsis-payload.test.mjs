import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verifyPayload } from './verify-nsis-payload.mjs';

test('accepts only the NSIS bundle marker transformation', () => {
  const built = Buffer.from('binary-prefix\0__TAURI_BUNDLE_TYPE_VAR_UNK\0binary-suffix');
  const installed = Buffer.from('binary-prefix\0__TAURI_BUNDLE_TYPE_VAR_NSS\0binary-suffix');
  assert.match(verifyPayload(built, installed), /^[a-f0-9]{64}$/);
  assert.throws(() => verifyPayload(built, built));
  const corrupted = Buffer.from(installed); corrupted[0] ^= 1;
  assert.throws(() => verifyPayload(built, corrupted));
});
test('rejects missing markers and incomplete payloads', () => {
  assert.throws(() => verifyPayload(Buffer.from('no marker'), Buffer.from('no marker')));
  const built = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK\0application');
  assert.throws(() => verifyPayload(built, Buffer.from('__TAURI_BUNDLE_TYPE_VAR_NSS')));
});
