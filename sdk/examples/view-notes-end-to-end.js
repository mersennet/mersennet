const http = require('http');
const crypto = require('crypto');
const { PrimeProvider, createMockNoteDecryptor, scanGrantedNotes } = require('../dist');

const grantIdHex = '0x' + '11'.repeat(32);
const recipientPublicKey = '0x' + '22'.repeat(32);
const viewSecretHex = '0x' + '33'.repeat(32);
const ephemeralPkHex = '0x' + '44'.repeat(32);

function hexToBytes(value) {
  const trimmed = value.startsWith('0x') ? value.slice(2) : value;
  return Buffer.from(trimmed, 'hex');
}

function bytesToHex(value) {
  return '0x' + Buffer.from(value).toString('hex');
}

function hashOnce(value) {
  return crypto.createHash('sha256').update(value).digest();
}

function expandKey(seed, len) {
  const chunks = [];
  let hash = hashOnce(seed);
  while (Buffer.concat(chunks).length < len) {
    chunks.push(hash);
    hash = hashOnce(hash);
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

function deriveSharedSecret(viewSecret, ephemeralPk) {
  return hashOnce(Buffer.concat([hexToBytes(viewSecret), hexToBytes(ephemeralPk)]));
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

function buildSamplePayload() {
  const note = {
    value: 2500n,
    assetId: 7,
    ownerPk: recipientPublicKey,
    rho: '0x' + '55'.repeat(32),
    psi: '0x' + '66'.repeat(32),
  };
  const plaintext = encodeNotePlaintext(note);
  const key = expandKey(deriveSharedSecret(viewSecretHex, ephemeralPkHex), plaintext.length);
  const ciphertext = xorBytes(plaintext, key);
  return {
    note,
    encryptedNoteHex: encodeEncryptedNotePayload({
      recipient: recipientPublicKey,
      ciphertext,
      ephemeralPk: ephemeralPkHex,
    }),
  };
}

async function main() {
  const sample = buildSamplePayload();
  const server = http.createServer((req, res) => {
    let body = '';
    req.on('data', (chunk) => {
      body += chunk;
    });
    req.on('end', () => {
      const payload = JSON.parse(body || '{}');
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({
        jsonrpc: '2.0',
        id: payload.id ?? 1,
        result: {
          grantId: grantIdHex,
          grantorCommitment: '0x' + '77'.repeat(32),
          blockNumber: 42,
          shieldedStateRoot: '0x' + '88'.repeat(32),
          totalEncryptedNoteCount: 1,
          returnedEncryptedNoteCount: 1,
          nextCursor: null,
          notes: [
            {
              noteCommitment: '0x' + '99'.repeat(32),
              encryptedNote: sample.encryptedNoteHex,
            },
          ],
          signatureVerified: true,
        },
      }));
    });
  });

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  const provider = new PrimeProvider(`http://127.0.0.1:${address.port}`);

  const result = await scanGrantedNotes(provider, {
    grantIdHex,
    recipientPublicKey,
    decryptNoteCiphertext: createMockNoteDecryptor(viewSecretHex),
  });

  console.log(JSON.stringify({
    decryptedNoteCount: result.notes.length,
    firstNote: result.notes[0]
      ? {
          noteCommitment: result.notes[0].noteCommitment,
          value: result.notes[0].note.value.toString(),
          assetId: result.notes[0].note.assetId,
          ownerPk: result.notes[0].note.ownerPk,
        }
      : null,
  }, null, 2));

  server.close();
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});