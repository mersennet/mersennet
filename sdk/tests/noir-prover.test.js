const assert = require('node:assert/strict');
const test = require('node:test');

const { NoirWasmProver } = require('../dist');

function recordingBackend() {
  const calls = [];
  return {
    calls,
    async prove(circuit, inputs) {
      calls.push({ circuit, inputs });
      return new Uint8Array([circuit.length, Object.keys(inputs).length]);
    },
  };
}

test('NoirWasmProver maps OrderPlace inputs to named circuit fields', async () => {
  const backend = recordingBackend();
  const prover = new NoirWasmProver({ backend });
  const proof = await prover.proveOrderPlace({
    anchorRoot: '0xaa',
    nullifier: '0xbb',
    newCommitment: '0xcc',
    marketId: 7n,
    sideHash: '0xdd',
    priceBand: 3,
    sizeBand: 4,
    oraclePrice: 1234n,
    immRequired: 99n,
  });

  assert.equal(backend.calls.length, 1);
  assert.equal(backend.calls[0].circuit, 'OrderPlace');
  assert.deepEqual(backend.calls[0].inputs, {
    anchor_root: '0xaa',
    nullifier: '0xbb',
    new_commitment: '0xcc',
    market_id: '7',
    side_hash: '0xdd',
    price_band: 3,
    size_band: 4,
    oracle_price: '1234',
    imm_required: '99',
  });
  assert.ok(proof instanceof Uint8Array);
});

test('NoirWasmProver maps Spend and Output inputs', async () => {
  const backend = recordingBackend();
  const prover = new NoirWasmProver({ backend });

  await prover.proveSpend({
    anchorRoot: '0x01',
    nullifier: '0x02',
    newCommitment: '0x03',
    publicAmount: 500n,
  });
  await prover.proveOutput({
    commitment: '0x04',
    assetId: 2,
    publicAmount: 0n,
  });

  assert.equal(backend.calls[0].circuit, 'Spend');
  assert.equal(backend.calls[0].inputs.public_amount, '500');
  assert.equal(backend.calls[1].circuit, 'Output');
  assert.equal(backend.calls[1].inputs.asset_id, 2);
  assert.equal(backend.calls[1].inputs.public_amount, '0');
});

test('NoirWasmProver rejects negative field values', async () => {
  const prover = new NoirWasmProver({ backend: recordingBackend() });
  await assert.rejects(
    prover.proveSpend({
      anchorRoot: '0x01',
      nullifier: '0x02',
      newCommitment: '0x03',
      publicAmount: -1n,
    }),
    /non-negative/
  );
});
