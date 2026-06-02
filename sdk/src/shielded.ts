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

import { createHash } from 'crypto';
import type { PrimeProvider } from './provider';
import { reconstructPortfolio } from './reconstruction';
import type { ReconstructOptions, ReconstructedPortfolio } from './reconstruction';

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
  grantedViewingMaterial?: GrantedViewingMaterial;
}

export interface GrantedViewingMaterial {
  grantIdHex: string;
  recipientPublicKey?: Fr;
  decryptNoteCiphertext(input: {
    noteCommitment: string;
    encryptedNoteHex: string;
    envelope: EncryptedNote;
  }): Uint8Array | null | Promise<Uint8Array | null>;
}

export interface GrantedNoteScanOptions {
  limit?: number;
  cursorHex?: string;
  maxPages?: number;
  ignoreMalformed?: boolean;
}

export interface GrantedDecryptedNote {
  noteCommitment: string;
  envelope: EncryptedNote;
  note: Note;
}

export interface GrantedNoteScanResult {
  grantId: string;
  blockNumber: number;
  totalEncryptedNoteCount: number;
  fetchedEncryptedNoteCount: number;
  nextCursor: string | null;
  skippedMalformedCount: number;
  notes: GrantedDecryptedNote[];
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
  private grantedViewingMaterial?: GrantedViewingMaterial;
  private prover: ZkProver | undefined;
  private noteCache: Note[] = [];

  constructor(opts: ShieldedClientOptions) {
    this.provider = opts.provider;
    this.viewingKey = opts.viewingKey;
    this.grantedViewingMaterial = opts.grantedViewingMaterial;
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

  setGrantedViewingMaterial(grantedViewingMaterial: GrantedViewingMaterial): void {
    this.grantedViewingMaterial = grantedViewingMaterial;
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
   * Reconstruct spendable balances from the scanned-notes cache, excluding
   * notes whose nullifiers have been spent on chain (ADR-019 `balances:read`).
   * The node never sees a decrypted balance; this runs entirely client-side.
   *
   * Call {@link scanRecentBlocks} first to populate the note cache, then pass
   * the set of spent nullifiers (and, for production wallets, your real
   * `nvk`-based nullifier deriver) via `options`.
   */
  reconstructBalances(options: ReconstructOptions = {}): ReconstructedPortfolio {
    return reconstructPortfolio(this.noteCache, options);
  }

  /**
   * Pull the latest blocks and try to decrypt every encrypted-note
   * payload addressed to the viewing key. Real implementation will
   * do this incrementally via WebSocket subscriptions.
   */
  async scanRecentBlocks(_fromBlock: bigint, _toBlock: bigint): Promise<void> {
    if (!this.grantedViewingMaterial) {
      return;
    }
    const scanned = await scanGrantedNotes(this.provider, this.grantedViewingMaterial);
    this.noteCache = scanned.notes.map((entry) => entry.note);
  }

  /**
   * Scan and decrypt the wallet's OWN notes (Workstream F2).
   *
   * The owner mints a self-grant (`prime_viewGrantToken` to their own
   * viewing key) and passes its id here. This derives the decryptor from
   * the wallet's own `viewSk`, filters to notes addressed to its `viewPk`,
   * decrypts them, and refreshes the note cache so {@link getBalance} and
   * {@link reconstructBalances} reflect the owner's spendable notes.
   */
  async scanOwnNotes(
    grantIdHex: string,
    options?: GrantedNoteScanOptions
  ): Promise<GrantedNoteScanResult> {
    const material = createOwnerViewingMaterial(this.viewingKey, grantIdHex);
    this.grantedViewingMaterial = material;
    const scanned = await scanGrantedNotes(this.provider, material, options);
    this.noteCache = scanned.notes.map((entry) => entry.note);
    return scanned;
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
    const shieldedRoot = (await this.provider.request('prime_getShieldedRoot')) as {
      shieldedStateRoot?: string;
    };
    const anchorRoot = shieldedRoot.shieldedStateRoot ?? zeroHex32();
    const nullifier = zeroHex32();
    const newCommitment = simpleHash(
      `${this.viewingKey.spendPk}:${params.marketId}:${params.side}:${params.price}:${params.size}`
    );
    const saltHex = simpleHash(`${this.viewingKey.spendPk}:salt:${params.marketId}:${params.side}`);
    const marketIdU64 = Number(params.marketId);
    if (!Number.isSafeInteger(marketIdU64)) {
      throw new Error('marketId exceeds supported u64 range for the current SDK mock path');
    }
    const priceBand = Number(params.price / 10n);
    const sizeBand = Number(params.size);
    const oraclePrice = 0n;
    const immRequired = (params.price * params.size * 500n + 9_999n) / 10_000n;

    let proofBytesHex: string | undefined;
    if (this.prover) {
      const proof = await this.prover.proveOrderPlace({
        anchorRoot,
        nullifier,
        newCommitment,
        marketId: params.marketId,
        sideHash: zeroHex32(),
        priceBand,
        sizeBand,
        oraclePrice,
        immRequired,
      });
      proofBytesHex = bytesToHex(proof);
    }

    const response = await this.provider.request('prime_submitShieldedOrder', [
      {
        anchorRootHex: anchorRoot,
        nullifierHex: nullifier,
        newCommitmentHex: newCommitment,
        marketId: bigintToHex(params.marketId),
        side: params.side,
        price: bigintToHex(params.price),
        size: bigintToHex(params.size),
        ownerPkHex: this.viewingKey.spendPk,
        saltHex,
        tif: 'gtc',
        gasLimit: '0x30d40',
        maxFeePerGas: '0x3b9aca00',
        ...(proofBytesHex ? { proofBytesHex } : {}),
      },
    ]);
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

export async function scanGrantedNotes(
  provider: PrimeProvider,
  grantedViewingMaterial: GrantedViewingMaterial,
  options: GrantedNoteScanOptions = {}
): Promise<GrantedNoteScanResult> {
  const maxPages = options.maxPages ?? Number.MAX_SAFE_INTEGER;
  let cursorHex = options.cursorHex;
  let pageCount = 0;
  let totalEncryptedNoteCount = 0;
  let fetchedEncryptedNoteCount = 0;
  let blockNumber = 0;
  let nextCursor: string | null = null;
  let skippedMalformedCount = 0;
  const notes: GrantedDecryptedNote[] = [];

  while (pageCount < maxPages) {
    const page = await provider.viewNotes(grantedViewingMaterial.grantIdHex, {
      limit: options.limit,
      cursorHex,
    });
    totalEncryptedNoteCount = page.totalEncryptedNoteCount;
    fetchedEncryptedNoteCount += page.returnedEncryptedNoteCount;
    blockNumber = page.blockNumber;
    nextCursor = page.nextCursor;

    for (const entry of page.notes) {
      let envelope: EncryptedNote;
      try {
        envelope = parseEncryptedNotePayload(entry.encryptedNote);
      } catch (error) {
        if (options.ignoreMalformed ?? true) {
          skippedMalformedCount += 1;
          continue;
        }
        throw error;
      }

      if (
        grantedViewingMaterial.recipientPublicKey &&
        envelope.recipient.toLowerCase() !== grantedViewingMaterial.recipientPublicKey.toLowerCase()
      ) {
        continue;
      }

      const plaintext = await grantedViewingMaterial.decryptNoteCiphertext({
        noteCommitment: entry.noteCommitment,
        encryptedNoteHex: entry.encryptedNote,
        envelope,
      });
      if (!plaintext) {
        continue;
      }

      notes.push({
        noteCommitment: entry.noteCommitment,
        envelope,
        note: parseShieldedNotePlaintext(plaintext),
      });
    }

    pageCount += 1;
    if (!page.nextCursor) {
      break;
    }
    cursorHex = page.nextCursor;
  }

  return {
    grantId: grantedViewingMaterial.grantIdHex,
    blockNumber,
    totalEncryptedNoteCount,
    fetchedEncryptedNoteCount,
    nextCursor,
    skippedMalformedCount,
    notes,
  };
}

export function parseEncryptedNotePayload(payloadHex: string): EncryptedNote {
  const payload = hexToBytes(payloadHex);
  let offset = 0;
  const recipient = bytesToFr(payload.subarray(offset, offset + 32));
  offset += 32;
  const ciphertextLen = Number(readU64LE(payload, offset));
  offset += 8;
  if (offset + ciphertextLen + 32 > payload.length) {
    throw new Error('malformed encrypted note payload');
  }
  const ciphertext = payload.slice(offset, offset + ciphertextLen);
  offset += ciphertextLen;
  const ephemeralPk = bytesToFr(payload.subarray(offset, offset + 32));
  offset += 32;
  if (offset !== payload.length) {
    throw new Error('encrypted note payload has trailing bytes');
  }
  return {
    recipient,
    ciphertext,
    ephemeralPk,
  };
}

export function parseShieldedNotePlaintext(plaintext: Uint8Array): Note {
  if (plaintext.length !== 116) {
    throw new Error('malformed note plaintext');
  }
  let offset = 0;
  const value = readU128LE(plaintext, offset);
  offset += 16;
  const assetId = readU32LE(plaintext, offset);
  offset += 4;
  const ownerPk = bytesToFr(plaintext.subarray(offset, offset + 32));
  offset += 32;
  const rho = bytesToFr(plaintext.subarray(offset, offset + 32));
  offset += 32;
  const psi = bytesToFr(plaintext.subarray(offset, offset + 32));

  return {
    value,
    assetId,
    ownerPk,
    rho,
    psi,
  };
}

/**
 * Example/mock decryptor matching the current end-to-end SDK examples.
 *
 * This is not the production viewing-key cryptosystem. It derives a
 * deterministic shared secret from `viewSecretHex` and `ephemeralPk`,
 * expands that into a byte stream, and XORs it with the ciphertext.
 */
/**
 * Build the viewing material a wallet uses to scan its OWN notes
 * (Workstream F2). The decryptor is keyed by the wallet's secret viewing
 * scalar and the scan is filtered to notes addressed to its public viewing
 * key. `grantIdHex` is the wallet's self-grant id.
 */
export function createOwnerViewingMaterial(
  viewingKey: ViewingKey,
  grantIdHex: string
): GrantedViewingMaterial {
  return {
    grantIdHex,
    recipientPublicKey: viewingKey.viewPk,
    decryptNoteCiphertext: createMockNoteDecryptor(viewingKey.viewSk),
  };
}

export function createMockNoteDecryptor(
  viewSecretHex: string
): GrantedViewingMaterial['decryptNoteCiphertext'] {
  return ({ envelope }) => {
    const key = expandMockKey(
      deriveMockSharedSecret(viewSecretHex, envelope.ephemeralPk),
      envelope.ciphertext.length
    );
    return xorBytes(envelope.ciphertext, key);
  };
}

function simpleHash(input: string): string {
  // Placeholder; real impl uses keccak256 via ethers or noble-hashes.
  let h = 0n;
  for (const ch of input) {
    h = ((h << 5n) - h + BigInt(ch.charCodeAt(0))) & ((1n << 256n) - 1n);
  }
  const hex = h.toString(16);
  return '0x' + hex.padStart(64, '0').slice(0, 64);
}

function zeroHex32(): string {
  return '0x' + '00'.repeat(32);
}

function bigintToHex(value: bigint): string {
  return '0x' + value.toString(16);
}

function bytesToHex(bytes: Uint8Array): string {
  return '0x' + Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join('');
}

function hashBytes(bytes: Uint8Array): Uint8Array {
  return createHash('sha256').update(bytes).digest();
}

function deriveMockSharedSecret(viewSecretHex: string, ephemeralPk: Fr): Uint8Array {
  const payload = new Uint8Array([
    ...hexToBytes(viewSecretHex),
    ...hexToBytes(ephemeralPk),
  ]);
  return hashBytes(payload);
}

function expandMockKey(seed: Uint8Array, len: number): Uint8Array {
  const chunks: Uint8Array[] = [];
  let total = 0;
  let hash = hashBytes(seed);
  while (total < len) {
    chunks.push(hash);
    total += hash.length;
    hash = hashBytes(hash);
  }
  return Uint8Array.from(chunks.flatMap((chunk) => Array.from(chunk)).slice(0, len));
}

function xorBytes(left: Uint8Array, right: Uint8Array): Uint8Array {
  const out = new Uint8Array(left.length);
  for (let index = 0; index < left.length; index += 1) {
    out[index] = left[index] ^ right[index];
  }
  return out;
}

function hexToBytes(hex: string): Uint8Array {
  const normalized = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (normalized.length % 2 !== 0) {
    throw new Error('hex string must have an even number of characters');
  }
  const out = new Uint8Array(normalized.length / 2);
  for (let index = 0; index < out.length; index += 1) {
    out[index] = parseInt(normalized.slice(index * 2, index * 2 + 2), 16);
  }
  return out;
}

function bytesToFr(bytes: Uint8Array): Fr {
  if (bytes.length !== 32) {
    throw new Error('field elements must be 32 bytes');
  }
  return bytesToHex(bytes);
}

function readU32LE(bytes: Uint8Array, offset: number): number {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return view.getUint32(offset, true);
}

function readU64LE(bytes: Uint8Array, offset: number): bigint {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return view.getBigUint64(offset, true);
}

function readU128LE(bytes: Uint8Array, offset: number): bigint {
  const low = readU64LE(bytes, offset);
  const high = readU64LE(bytes, offset + 8);
  return low + (high << 64n);
}
