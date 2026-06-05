// Workstream F1 — wiring a real Noir WASM prover into the shielded SDK.
//
// `NoirWasmProver` is a `ZkProver` that maps the typed prover calls to the
// named Noir circuit inputs and delegates the heavy lifting to an injected
// `NoirProvingBackend`. The backend is the ONLY thing that touches the WASM
// toolchain, so the SDK stays dependency-free.
//
// In production the backend wraps `@noir-lang/noir_js` + `@aztec/bb.js` and
// the compiled circuit artifacts (the `nargo`/`bb` outputs from Workstream
// D6). A sketch of that backend:
//
//   const { Noir } = require('@noir-lang/noir_js');
//   const { UltraHonkBackend } = require('@aztec/bb.js');
//   const circuits = {
//     OrderPlace: require('./artifacts/order_place.json'),
//     Spend: require('./artifacts/spend.json'),
//     Output: require('./artifacts/output.json'),
//   };
//   const productionBackend = {
//     async prove(circuit, inputs) {
//       const program = circuits[circuit];
//       const noir = new Noir(program);
//       const { witness } = await noir.execute(inputs);
//       const bb = new UltraHonkBackend(program.bytecode);
//       const { proof } = await bb.generateProof(witness);
//       return proof; // Uint8Array in the chain-accepted encoding
//     },
//   };
//
// This example uses a fake backend so the wiring is runnable today without
// the WASM toolchain. Swap in `productionBackend` to produce real proofs.

const { NoirWasmProver } = require('../dist');

const fakeBackend = {
  async prove(circuit, inputs) {
    console.log(`[backend] proving ${circuit} with inputs:`, inputs);
    // A real backend returns the proof bytes; here we return a deterministic
    // placeholder so the typed mapping can be exercised end-to-end.
    return new Uint8Array([0xde, 0xad, 0xbe, 0xef]);
  },
};

async function main() {
  const prover = new NoirWasmProver({ backend: fakeBackend });

  const proof = await prover.proveOrderPlace({
    anchorRoot: '0x' + '11'.repeat(32),
    nullifier: '0x' + '22'.repeat(32),
    newCommitment: '0x' + '33'.repeat(32),
    marketId: 1n,
    sideHash: '0x' + '44'.repeat(32),
    priceBand: '0x' + '55'.repeat(32),
    sizeBand: '0x' + '66'.repeat(32),
    oraclePrice: 100n,
    immRequired: 5n,
  });

  console.log('order-place proof bytes:', Buffer.from(proof).toString('hex'));
  console.log(
    'wire into ShieldedClient via: new ShieldedClient({ provider, viewingKey, prover })'
  );
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
