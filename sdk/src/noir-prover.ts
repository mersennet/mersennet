/**
 * Client-side Noir proving integration (Workstream F1).
 *
 * `ShieldedClient` accepts any `ZkProver`. This module provides a
 * `NoirWasmProver` that maps the typed prover calls to the named Noir
 * circuit inputs and delegates the heavy lifting to an injected
 * `NoirProvingBackend`.
 *
 * The backend is the only thing that touches the WASM toolchain
 * (`@noir-lang/noir_js` + `@aztec/bb.js`), so the SDK itself stays
 * dependency-free and tree-shakeable: a Node service can ship a native
 * backend, a browser wallet ships the WASM backend, and tests ship a fake.
 * The wallet wires the compiled circuit artifacts (the `nargo`/`bb` outputs
 * from Workstream D6) into its backend implementation.
 */

import type { OrderPlacePublicInputs, ZkProver, Fr } from './shielded';

/** Canonical circuit names, matching the chain-side `Circuit` enum. */
export type NoirCircuitName = 'OrderPlace' | 'Spend' | 'Output';

/** A single Noir witness value: a field element (hex) or a small integer. */
export type NoirInputValue = Fr | number | string;

/**
 * Proving backend the wallet wires to `@noir-lang/noir_js` + `@aztec/bb.js`.
 * `inputs` are the circuit's named public/private inputs; the returned bytes
 * are the proof in the chain-accepted encoding.
 */
export interface NoirProvingBackend {
  prove(circuit: NoirCircuitName, inputs: Record<string, NoirInputValue>): Promise<Uint8Array>;
}

export interface NoirWasmProverOptions {
  backend: NoirProvingBackend;
}

/** Render a bigint as a decimal field literal (Noir's expected form). */
function fieldFromBigint(value: bigint): string {
  if (value < 0n) {
    throw new Error('field values must be non-negative');
  }
  return value.toString(10);
}

/**
 * `ZkProver` implementation that produces real Noir proofs via an injected
 * WASM backend. Drop-in for the mock prover: `client.setProver(new
 * NoirWasmProver({ backend }))`.
 */
export class NoirWasmProver implements ZkProver {
  private readonly backend: NoirProvingBackend;

  constructor(options: NoirWasmProverOptions) {
    this.backend = options.backend;
  }

  async proveOrderPlace(inputs: OrderPlacePublicInputs): Promise<Uint8Array> {
    return this.backend.prove('OrderPlace', {
      anchor_root: inputs.anchorRoot,
      nullifier: inputs.nullifier,
      new_commitment: inputs.newCommitment,
      market_id: fieldFromBigint(inputs.marketId),
      side_hash: inputs.sideHash,
      price_band: inputs.priceBand,
      size_band: inputs.sizeBand,
      oracle_price: fieldFromBigint(inputs.oraclePrice),
      imm_required: fieldFromBigint(inputs.immRequired),
    });
  }

  async proveSpend(inputs: {
    anchorRoot: Fr;
    nullifier: Fr;
    newCommitment: Fr;
    publicAmount: bigint;
  }): Promise<Uint8Array> {
    return this.backend.prove('Spend', {
      anchor_root: inputs.anchorRoot,
      nullifier: inputs.nullifier,
      new_commitment: inputs.newCommitment,
      public_amount: fieldFromBigint(inputs.publicAmount),
    });
  }

  async proveOutput(inputs: {
    commitment: Fr;
    assetId: number;
    publicAmount: bigint;
  }): Promise<Uint8Array> {
    return this.backend.prove('Output', {
      commitment: inputs.commitment,
      asset_id: inputs.assetId,
      public_amount: fieldFromBigint(inputs.publicAmount),
    });
  }
}
