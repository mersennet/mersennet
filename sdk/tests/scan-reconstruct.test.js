const assert = require('node:assert/strict');
const crypto = require('crypto');
const test = require('node:test');

const {
  scanAndReconstructBalances,
  reconstructPortfolio,
  defaultNullifierDeriver,
  createOwnerViewingMaterial,
} = require('../dist');

const VIEW_SECRET_HEX = '0x' + '33'.repeat(32);
const RECIPIENT_HEX = '0x' + '22'.repeat(32);
const EPHEMERAL_PK_HEX = '0x' + '44'.repeat(32);

const viewingKey = {
  spendPk: '0x' + '11'.repeat(32),
  spendSk: '0x' + '12'.repeat(32),
  viewPk: RECIPIENT_HEX,
  viewSk: VIEW_SECRET_HEX,
};

function ownedNote(value, assetId, rhoTag) {
  return {
    value: BigInt(value),
    assetId,
    ownerPk: RECIPIENT_HEX,
    rho: '0x' + rhoTag.toString(16).padStart(64, '0'),
    psi: '0x' + '66'.repeat(32),
  };
}

function encryptedPayloadFor(note) {
  const plaintext = encodeNotePlaintext(note);
  const ciphertext = xorBytes(
    plaintext,
    expandKey(deriveSharedSecret(VIEW_SECRET_HEX, EPHEMERAL_PK_HEX), plaintext.length)
  );
  return encodeEncryptedNotePayload({
    recipient: RECIPIENT_HEX,
    ciphertext,
    ephemeralPk: EPHEMERAL_PK_HEX,
  });
}

function fakeProvider(pages) {
  let index = 0;
  return {
    async viewBalances() {
      const page = pages[index];
      index += 1;
      return page;
    },
  };
}

test('scanAndReconstructBalances reconstructs spendable balances from a grant', async () => {
  const liveA = ownedNote(100, 1, 1);
  const spent = ownedNote(40, 1, 2);
  const liveB = ownedNote(7, 2, 3);
  const spentNullifier = defaultNullifierDeriver(spent);

  const provider = fakeProvider([
    {
      grantId: '0xabc',
      grantorCommitment: '0x' + '42'.repeat(32),
      blockNumber: 99,
      shieldedStateRoot: '0x' + 'aa'.repeat(32),
      totalEncryptedNoteCount: 3,
      returnedEncryptedNoteCount: 3,
      nextCursor: null,
      notes: [
        { noteCommitment: '0x' + '01'.repeat(32), encryptedNote: encryptedPayloadFor(liveA) },
        { noteCommitment: '0x' + '02'.repeat(32), encryptedNote: encryptedPayloadFor(spent) },
        { noteCommitment: '0x' + '03'.repeat(32), encryptedNote: encryptedPayloadFor(liveB) },
      ],
      spentNullifiers: [spentNullifier],
      spentNullifierCount: 1,
      reconstruction: 'client-side',
      signatureVerified: true,
    },
  ]);

  const material = createOwnerViewingMaterial(viewingKey, '0xabc');
  const result = await scanAndReconstructBalances(provider, material);

  assert.equal(result.perAsset[1], 100n); // 40 spent, only the 100 note counts
  assert.equal(result.perAsset[2], 7n);
  assert.equal(result.unspentNoteCount, 2);
  assert.equal(result.spentNoteCount, 1);
  assert.equal(result.totalNoteCount, 3);
  assert.equal(result.spentNullifierCount, 1);
  assert.equal(result.blockNumber, 99);
  assert.equal(result.grantId, '0xabc');
  assert.equal(result.fetchedEncryptedNoteCount, 3);
});

test('scanAndReconstructBalances follows pagination cursors', async () => {
  const a = ownedNote(10, 1, 1);
  const b = ownedNote(20, 1, 2);

  const provider = fakeProvider([
    {
      grantId: '0xabc',
      grantorCommitment: '0x' + '42'.repeat(32),
      blockNumber: 5,
      shieldedStateRoot: '0x' + 'bb'.repeat(32),
      totalEncryptedNoteCount: 2,
      returnedEncryptedNoteCount: 1,
      nextCursor: '0x' + '01'.repeat(32),
      notes: [{ noteCommitment: '0x' + '01'.repeat(32), encryptedNote: encryptedPayloadFor(a) }],
      spentNullifiers: [],
      spentNullifierCount: 0,
      reconstruction: 'client-side',
      signatureVerified: true,
    },
    {
      grantId: '0xabc',
      grantorCommitment: '0x' + '42'.repeat(32),
      blockNumber: 5,
      shieldedStateRoot: '0x' + 'bb'.repeat(32),
      totalEncryptedNoteCount: 2,
      returnedEncryptedNoteCount: 1,
      nextCursor: null,
      notes: [{ noteCommitment: '0x' + '02'.repeat(32), encryptedNote: encryptedPayloadFor(b) }],
      spentNullifiers: [],
      spentNullifierCount: 0,
      reconstruction: 'client-side',
      signatureVerified: true,
    },
  ]);

  const material = createOwnerViewingMaterial(viewingKey, '0xabc');
  const result = await scanAndReconstructBalances(provider, material);

  assert.equal(result.perAsset[1], 30n);
  assert.equal(result.unspentNoteCount, 2);
  assert.equal(result.fetchedEncryptedNoteCount, 2);
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
