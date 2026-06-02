const assert = require('node:assert/strict');
const crypto = require('crypto');
const test = require('node:test');

const {
  createMockNoteDecryptor,
  createOwnerViewingMaterial,
  parseEncryptedNotePayload,
  parseShieldedNotePlaintext,
} = require('../dist');

const VIEW_SECRET_HEX = '0x' + '33'.repeat(32);
const RECIPIENT_HEX = '0x' + '22'.repeat(32);
const EPHEMERAL_PK_HEX = '0x' + '44'.repeat(32);

test('createMockNoteDecryptor round-trips a note payload', async () => {
  const note = {
    value: 2500n,
    assetId: 7,
    ownerPk: RECIPIENT_HEX,
    rho: '0x' + '55'.repeat(32),
    psi: '0x' + '66'.repeat(32),
  };
  const plaintext = encodeNotePlaintext(note);
  const ciphertext = xorBytes(
    plaintext,
    expandKey(deriveSharedSecret(VIEW_SECRET_HEX, EPHEMERAL_PK_HEX), plaintext.length)
  );
  const payloadHex = encodeEncryptedNotePayload({
    recipient: RECIPIENT_HEX,
    ciphertext,
    ephemeralPk: EPHEMERAL_PK_HEX,
  });

  const envelope = parseEncryptedNotePayload(payloadHex);
  const decryptor = createMockNoteDecryptor(VIEW_SECRET_HEX);
  const decrypted = await decryptor({
    noteCommitment: '0x' + '99'.repeat(32),
    encryptedNoteHex: payloadHex,
    envelope,
  });

  assert.ok(decrypted);
  const parsed = parseShieldedNotePlaintext(decrypted);
  assert.equal(parsed.value, 2500n);
  assert.equal(parsed.assetId, 7);
  assert.equal(parsed.ownerPk, RECIPIENT_HEX);
  assert.equal(parsed.rho, note.rho);
  assert.equal(parsed.psi, note.psi);
});

test('createOwnerViewingMaterial keys the decryptor to the owner and round-trips', async () => {
  const viewingKey = {
    spendPk: '0x' + '11'.repeat(32),
    spendSk: '0x' + '12'.repeat(32),
    viewPk: RECIPIENT_HEX,
    viewSk: VIEW_SECRET_HEX,
  };
  const material = createOwnerViewingMaterial(viewingKey, '0xabc');
  assert.equal(material.grantIdHex, '0xabc');
  assert.equal(material.recipientPublicKey, RECIPIENT_HEX);

  const note = {
    value: 777n,
    assetId: 3,
    ownerPk: RECIPIENT_HEX,
    rho: '0x' + '55'.repeat(32),
    psi: '0x' + '66'.repeat(32),
  };
  const plaintext = encodeNotePlaintext(note);
  const ciphertext = xorBytes(
    plaintext,
    expandKey(deriveSharedSecret(VIEW_SECRET_HEX, EPHEMERAL_PK_HEX), plaintext.length)
  );
  const payloadHex = encodeEncryptedNotePayload({
    recipient: RECIPIENT_HEX,
    ciphertext,
    ephemeralPk: EPHEMERAL_PK_HEX,
  });
  const envelope = parseEncryptedNotePayload(payloadHex);
  const decrypted = await material.decryptNoteCiphertext({
    noteCommitment: '0x' + '99'.repeat(32),
    encryptedNoteHex: payloadHex,
    envelope,
  });

  assert.ok(decrypted);
  const parsed = parseShieldedNotePlaintext(decrypted);
  assert.equal(parsed.value, 777n);
  assert.equal(parsed.assetId, 3);
});

test('parseEncryptedNotePayload rejects malformed payloads', () => {
  assert.throws(
    () => parseEncryptedNotePayload('0x1234'),
    /malformed encrypted note payload|field elements must be 32 bytes/
  );
});

function encodeNotePlaintext(note) {
  const out = Buffer.alloc(116);
  let offset = 0;
  encodeU128LE(note.value).copy(out, offset);
  offset += 16;
  out.writeUInt32LE(note.assetId, offset);
  offset += 4;
  hexToBytes(note.ownerPk).copy(out, offset);
  offset += 32;
  hexToBytes(note.rho).copy(out, offset);
  offset += 32;
  hexToBytes(note.psi).copy(out, offset);
  return out;
}

function encodeEncryptedNotePayload({ recipient, ciphertext, ephemeralPk }) {
  const length = Buffer.alloc(8);
  length.writeBigUInt64LE(BigInt(ciphertext.length));
  return bytesToHex(Buffer.concat([
    hexToBytes(recipient),
    length,
    Buffer.from(ciphertext),
    hexToBytes(ephemeralPk),
  ]));
}

function encodeU128LE(value) {
  const out = Buffer.alloc(16);
  let remaining = value;
  for (let index = 0; index < 16; index += 1) {
    out[index] = Number(remaining & 0xffn);
    remaining >>= 8n;
  }
  return out;
}

function deriveSharedSecret(viewSecret, ephemeralPk) {
  return hashBytes(Buffer.concat([hexToBytes(viewSecret), hexToBytes(ephemeralPk)]));
}

function expandKey(seed, len) {
  const chunks = [];
  let total = 0;
  let hash = hashBytes(seed);
  while (total < len) {
    chunks.push(hash);
    total += hash.length;
    hash = hashBytes(hash);
  }
  return Buffer.concat(chunks).subarray(0, len);
}

function xorBytes(left, right) {
  const out = Buffer.alloc(left.length);
  for (let index = 0; index < left.length; index += 1) {
    out[index] = left[index] ^ right[index];
  }
  return out;
}

function hashBytes(value) {
  return crypto.createHash('sha256').update(value).digest();
}

function hexToBytes(value) {
  const trimmed = value.startsWith('0x') ? value.slice(2) : value;
  return Buffer.from(trimmed, 'hex');
}

function bytesToHex(value) {
  return '0x' + Buffer.from(value).toString('hex');
}