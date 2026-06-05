/**
 * Client-side portfolio reconstruction (ADR-019 `balances:read`).
 *
 * The privacy fork deliberately keeps the node from ever returning a
 * decrypted per-account balance. Instead the owner — or a grantee holding a
 * scoped viewing grant — decrypts the notes addressed to them
 * (`scanGrantedNotes` in `./shielded`) and reconstructs spendable balances
 * locally, here.
 *
 * Reconstruction rule: a note contributes to the balance only if its
 * nullifier has NOT been published on chain. The caller supplies the set of
 * spent nullifiers (e.g. derived from the public nullifier stream) and a
 * deriver that maps an owned note to its nullifier. A deterministic default
 * deriver is provided for tests and for wallets that use the matching
 * chain-side scheme; production wallets pass their real `nvk`-based deriver.
 */

import { createHash } from 'crypto';
import type { PrimeProvider } from './provider';
import {
  parseEncryptedNotePayload,
  parseShieldedNotePlaintext,
  type EncryptedNote,
  type GrantedNoteScanOptions,
  type GrantedViewingMaterial,
  type Note,
} from './shielded';

/** Maps an owned note to its on-chain nullifier (hex, 0x-prefixed). */
export type NullifierDeriver = (note: Note) => string;

export interface PortfolioNote {
  note: Note;
  /** Derived nullifier for this note. */
  nullifier: string;
  /** True iff the nullifier appears in the supplied spent set. */
  spent: boolean;
}

export interface ReconstructedPortfolio {
  /** Spendable balance per asset id (unspent notes only). */
  perAsset: Record<number, bigint>;
  unspentNoteCount: number;
  spentNoteCount: number;
  /** Distinct notes considered (after de-duplicating by nullifier). */
  totalNoteCount: number;
  notes: PortfolioNote[];
}

export interface ReconstructOptions {
  /** Nullifier deriver. Defaults to {@link defaultNullifierDeriver}. */
  deriveNullifier?: NullifierDeriver;
  /** Set of spent nullifiers (hex). Used when `isSpent` is not provided. */
  spentNullifiers?: Iterable<string>;
  /** Custom spent predicate. Takes precedence over `spentNullifiers`. */
  isSpent?: (nullifierHex: string) => boolean;
}

function normalizeHex(hex: string): string {
  const body = hex.startsWith('0x') || hex.startsWith('0X') ? hex.slice(2) : hex;
  return '0x' + body.toLowerCase();
}

/**
 * Deterministic nullifier derivation used by tests and by wallets that mirror
 * the chain's mock scheme: sha256(ownerPk || rho || psi). Production wallets
 * override this with their real nullifier-viewing-key derivation.
 */
export function defaultNullifierDeriver(note: Note): string {
  const h = createHash('sha256');
  h.update(hexToBytes(note.ownerPk));
  h.update(hexToBytes(note.rho));
  h.update(hexToBytes(note.psi));
  return '0x' + h.digest('hex');
}

/**
 * Reconstruct spendable balances from a set of decrypted notes.
 *
 * Notes are de-duplicated by nullifier so a note seen twice during scanning is
 * never double-counted. Only unspent notes contribute to `perAsset`.
 */
export function reconstructPortfolio(
  notes: Note[],
  options: ReconstructOptions = {}
): ReconstructedPortfolio {
  const deriveNullifier = options.deriveNullifier ?? defaultNullifierDeriver;

  let spentPredicate: (nullifierHex: string) => boolean;
  if (options.isSpent) {
    spentPredicate = options.isSpent;
  } else if (options.spentNullifiers) {
    const spent = new Set<string>();
    for (const n of options.spentNullifiers) {
      spent.add(normalizeHex(n));
    }
    spentPredicate = (nullifierHex: string) => spent.has(normalizeHex(nullifierHex));
  } else {
    spentPredicate = () => false;
  }

  const perAsset: Record<number, bigint> = {};
  const portfolioNotes: PortfolioNote[] = [];
  const seen = new Set<string>();
  let unspentNoteCount = 0;
  let spentNoteCount = 0;

  for (const note of notes) {
    const nullifier = normalizeHex(deriveNullifier(note));
    if (seen.has(nullifier)) {
      continue;
    }
    seen.add(nullifier);

    const spent = spentPredicate(nullifier);
    portfolioNotes.push({ note, nullifier, spent });

    if (spent) {
      spentNoteCount += 1;
      continue;
    }
    unspentNoteCount += 1;
    perAsset[note.assetId] = (perAsset[note.assetId] ?? 0n) + note.value;
  }

  return {
    perAsset,
    unspentNoteCount,
    spentNoteCount,
    totalNoteCount: portfolioNotes.length,
    notes: portfolioNotes,
  };
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

export interface ScanReconstructOptions extends GrantedNoteScanOptions {
  /** Nullifier deriver. Defaults to {@link defaultNullifierDeriver}. */
  deriveNullifier?: NullifierDeriver;
}

/** Spendable portfolio reconstructed from a grant-gated `balances:read`. */
export interface BalanceReconstructionResult extends ReconstructedPortfolio {
  grantId: string;
  blockNumber: number;
  shieldedStateRoot: string;
  totalEncryptedNoteCount: number;
  fetchedEncryptedNoteCount: number;
  skippedMalformedCount: number;
  spentNullifierCount: number;
}

/**
 * End-to-end balance read (Workstream F2 + F5).
 *
 * Pages through the grant-gated `prime_viewBalances` RPC, decrypts the notes
 * addressed to the grantor via the supplied viewing material, collects the
 * on-chain spent-nullifier set, and reconstructs spendable per-asset balances
 * locally. The node never sees a decrypted balance.
 *
 * The wallet must supply a `deriveNullifier` that matches the chain's
 * nullifier scheme so unspent notes are filtered correctly; the default
 * mirrors the mock scheme used in tests.
 */
export async function scanAndReconstructBalances(
  provider: PrimeProvider,
  grantedViewingMaterial: GrantedViewingMaterial,
  options: ScanReconstructOptions = {}
): Promise<BalanceReconstructionResult> {
  const maxPages = options.maxPages ?? Number.MAX_SAFE_INTEGER;
  let cursorHex = options.cursorHex;
  let pageCount = 0;
  let blockNumber = 0;
  let shieldedStateRoot = '0x';
  let totalEncryptedNoteCount = 0;
  let fetchedEncryptedNoteCount = 0;
  let skippedMalformedCount = 0;
  const ownedNotes: Note[] = [];
  const spentNullifiers = new Set<string>();

  while (pageCount < maxPages) {
    const page = await provider.viewBalances(grantedViewingMaterial.grantIdHex, {
      limit: options.limit,
      cursorHex,
    });
    blockNumber = page.blockNumber;
    shieldedStateRoot = page.shieldedStateRoot;
    totalEncryptedNoteCount = page.totalEncryptedNoteCount;
    fetchedEncryptedNoteCount += page.returnedEncryptedNoteCount;
    for (const nullifier of page.spentNullifiers) {
      spentNullifiers.add(normalizeHex(nullifier));
    }

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
      ownedNotes.push(parseShieldedNotePlaintext(plaintext));
    }

    pageCount += 1;
    if (!page.nextCursor) {
      break;
    }
    cursorHex = page.nextCursor;
  }

  const portfolio = reconstructPortfolio(ownedNotes, {
    deriveNullifier: options.deriveNullifier,
    spentNullifiers,
  });

  return {
    ...portfolio,
    grantId: grantedViewingMaterial.grantIdHex,
    blockNumber,
    shieldedStateRoot,
    totalEncryptedNoteCount,
    fetchedEncryptedNoteCount,
    skippedMalformedCount,
    spentNullifierCount: spentNullifiers.size,
  };
}
