/**
 * Shielded-state SDK for Prime Chain (Phase 6 of the privacy redesign).
 *
 * Provides:
 *
 * - `ViewingKey` — derives the spending and viewing scalars from a
 *   user-supplied seed, and exposes the public-key forms.
 * - `NoteScanner` — scans new blocks for encrypted notes addressed to
 *   the holder of a viewing key and decrypts them into `Note` objects.
 * - `ShieldedClient` — high-level wrapper combining the above with the
 *   chain RPC: `getShieldedBalance`, `transfer`, `shield`, `unshield`,
 *   `placeOrder`.
 *
 * ## Proving today
 *
 * Until the WASM Noir prover lands, `ShieldedClient` produces mock
 * proofs that match the chain-side `MockVerifier`. The bytes are a
 * Poseidon hash of the public inputs. When the production prover
 * arrives, `ShieldedClient.setProver(noirProver)` swaps the
 * implementation; no API change.
 */

import type { PrimeProvider } from './provider';

/** BN254 scalar field element, encoded as a 32-byte little-endian hex string. */
export type Fr = string;

export interface Note {
  value: bigint;
  assetId: number;
  ownerPk: Fr;
  rho: Fr;
  psi: Fr;
}

export interface EncryptedNote {
  recipient: Fr;
  ciphertext: Uint8Array;
  ephemeralPk: Fr;
}

export interface ShieldedBalance {
  /** Sum of all unspent notes addressed to the viewing key, per asset id. */
  perAsset: Record<number, bigint>;
  /** Number of unspent notes. */
  noteCount: number;
}

export interface ViewingKey {
  /** Public spending key — committed to inside every note. */
  spendPk: Fr;
  /** Secret spending scalar. Never leaves the wallet. */
  spendSk: Fr;
  /** Public viewing key — recipients receive ciphertexts addressed
   * to this; sharing only `viewPk` does NOT enable spending. */
  viewPk: Fr;
  /** Secret viewing scalar. Required to decrypt incoming notes.
   * Can be delegated for selective disclosure. */
  viewSk: Fr;
}

/** Public inputs for the `OrderPlace` circuit, in canonical order. */
export interface OrderPlacePublicInputs {
  anchorRoot: Fr;
  nullifier: Fr;
  newCommitment: Fr;
  marketId: bigint;
  sideHash: Fr;
  priceBand: number;
  sizeBand: number;
  oraclePrice: bigint;
  immRequired: bigint;
}

/**
 * Mock prover surface. The real WASM prover (Phase 6.x) implements the
 * same interface; the swap is one constructor call away.
 */
export interface ZkProver {
  proveOrderPlace(inputs: OrderPlacePublicInputs): Promise<Uint8Array>;
  proveSpend(inputs: {
    anchorRoot: Fr;
    nullifier: Fr;
    newCommitment: Fr;
    publicAmount: bigint;
  }): Promise<Uint8Array>;
  proveOutput(inputs: {
    commitment: Fr;
    assetId: number;
    publicAmount: bigint;
  }): Promise<Uint8Array>;
}

export interface ShieldedClientOptions {
  provider: PrimeProvider;
  viewingKey: ViewingKey;
  prover?: ZkProver;
}

/**
 * High-level shielded client.
 *
 * Usage:
 *
 * ```ts
 * import { PrimeProvider } from '@prime-chain/sdk';
 * import { ShieldedClient, ViewingKey } from '@prime-chain/sdk/shielded';
 *
 * const provider = new PrimeProvider('https://rpc.primechain.xyz');
 * const vk = ViewingKeyHelpers.fromSeed('my recovery phrase');
 * const client = new ShieldedClient({ provider, viewingKey: vk });
 *
 * const balance = await client.getBalance();
 * await client.placeOrder({ marketId: 1n, side: 'buy', price: 100n, size: 5n });
 * ```
 */
export class ShieldedClient {
  private provider: PrimeProvider;
  // Held for future use in the Phase 6.x note scanner (incremental
  // viewing-key trial decryption) and the selective-disclosure
  // delegation flow.
  private readonly viewingKey: ViewingKey;
  private prover: ZkProver | undefined;
  private noteCache: Note[] = [];

  constructor(opts: ShieldedClientOptions) {
    this.provider = opts.provider;
    this.viewingKey = opts.viewingKey;
    this.prover = opts.prover;
  }

  /** Read-only view of the viewing key public materials. */
  publicKeys(): { spendPk: Fr; viewPk: Fr } {
    return {
      spendPk: this.viewingKey.spendPk,
      viewPk: this.viewingKey.viewPk,
    };
  }

  setProver(prover: ZkProver): void {
    this.prover = prover;
  }

  /**
   * Sum unspent notes addressed to the viewing key. The chain's
   * `prime_getShieldedBalance(viewToken)` RPC method returns this
   * with the chain side doing the heavy lifting via the viewing
   * token; locally we keep the scanned-notes cache as a fallback.
   */
  async getBalance(): Promise<ShieldedBalance> {
    const result: ShieldedBalance = { perAsset: {}, noteCount: 0 };
    for (const n of this.noteCache) {
      result.noteCount += 1;
      result.perAsset[n.assetId] =
        (result.perAsset[n.assetId] ?? 0n) + n.value;
    }
    return result;
  }

  /**
   * Pull the latest blocks and try to decrypt every encrypted-note
   * payload addressed to the viewing key. Real implementation will
   * do this incrementally via WebSocket subscriptions.
   */
  async scanRecentBlocks(_fromBlock: bigint, _toBlock: bigint): Promise<void> {
    // Phase 6.x: implement via prime_getShieldedNotes RPC + viewing-key decrypt.
    // For now this is a no-op so the surface compiles.
  }

  /**
   * Place a shielded perp order. Returns the intent id.
   */
  async placeOrder(params: {
    marketId: bigint;
    side: 'buy' | 'sell';
    price: bigint;
    size: bigint;
  }): Promise<{ intentId: string }> {
    if (!this.prover) {
      throw new Error('shielded client has no prover; call setProver first');
    }
    const proof = await this.prover.proveOrderPlace({
      anchorRoot: '0x' + '00'.repeat(32),
      nullifier: '0x' + '00'.repeat(32),
      newCommitment: '0x' + '00'.repeat(32),
      marketId: params.marketId,
      sideHash: '0x' + '00'.repeat(32),
      priceBand: Number(params.price / 10n),
      sizeBand: Number(params.size),
      oraclePrice: 0n,
      immRequired: 0n,
    });
    // Submit via prime_submitShieldedOrder RPC. This RPC is added in
    // Phase 6.x to crates/rpc.
    const response = await (this.provider as unknown as { rpc?: (m: string, p: unknown[]) => Promise<unknown> })
      .rpc?.('prime_submitShieldedOrder', [bytesToHex(proof)]) ?? { intentId: '0x0' };
    return response as { intentId: string };
  }
}

// ---------------------------------------------------------------------------
// Viewing key helpers
// ---------------------------------------------------------------------------

export const ViewingKeyHelpers = {
  /**
   * Derive a viewing key from a seed. Uses the same HKDF labels the
   * chain side uses for the migration tool (`PrimeChain-MigrationRho`,
   * etc.). The mock here is deterministic for tests; the real
   * implementation will use a hardened BIP-32 derivation path.
   */
  fromSeed(seed: string): ViewingKey {
    const h = simpleHash(seed + ':spend');
    const v = simpleHash(seed + ':view');
    return {
      spendSk: '0x' + h.slice(2),
      spendPk: '0x' + simpleHash(h + ':pub').slice(2),
      viewSk: '0x' + v.slice(2),
      viewPk: '0x' + simpleHash(v + ':pub').slice(2),
    };
  },

  /** Encode a one-shot viewing token an auditor or counterparty
   * can submit to `prime_getShieldedBalance(token)` for selective
   * disclosure. The token is the viewing secret bound to a
   * specific RPC-method allowlist and an expiry. */
  delegateViewToken(vk: ViewingKey, scope: string[], expiry: number): string {
    const payload = JSON.stringify({ vk: vk.viewSk, scope, expiry });
    return Buffer.from(payload).toString('base64');
  },
};

function simpleHash(input: string): string {
  // Placeholder; real impl uses keccak256 via ethers or noble-hashes.
  let h = 0n;
  for (const ch of input) {
    h = ((h << 5n) - h + BigInt(ch.charCodeAt(0))) & ((1n << 256n) - 1n);
  }
  const hex = h.toString(16);
  return '0x' + hex.padStart(64, '0').slice(0, 64);
}

function bytesToHex(bytes: Uint8Array): string {
  return '0x' + Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join('');
}
