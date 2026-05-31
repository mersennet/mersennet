/**
 * Privacy-fork migration UX helpers (Workstream F4).
 *
 * At the hard fork the chain mints one shielded note per migrated account
 * (see `ShieldedEvm::migrate`). These helpers let a wallet, before and after
 * the fork:
 *
 *  - derive the deterministic note it will receive (so the UI can show
 *    "you'll receive a shielded note worth X" pre-fork), and
 *  - recognize that note among the encrypted notes it scans post-fork
 *    (`matchesMigrationNote`), confirming the migrated balance landed.
 *
 * The `rho`/`psi` blinding factors are derived from the viewing key via
 * labelled hashing. The note commitment is computed by an injected
 * `commitmentHasher` so production wallets pass the chain's Poseidon
 * commitment while tests use the deterministic default.
 *
 * Integration contract: the labels and commitment scheme here MUST mirror
 * the chain-side migration tool for the derived note to match on chain.
 */

import { createHash } from 'crypto';
import type { Fr, Note, ViewingKey } from './shielded';

export const MIGRATION_RHO_LABEL = 'PrimeChain-MigrationRho';
export const MIGRATION_PSI_LABEL = 'PrimeChain-MigrationPsi';

export type NoteCommitmentHasher = (note: Note) => Fr;

export interface MigrationNoteParams {
  viewingKey: ViewingKey;
  assetId: number;
  /** Pre-fork transparent balance being migrated into the note. */
  balance: bigint;
  /** Distinguishes notes when a wallet migrates several accounts. */
  account?: number;
  /** Commitment hasher; defaults to the deterministic placeholder. */
  commitmentHasher?: NoteCommitmentHasher;
}

export interface MigrationNote {
  note: Note;
  /** The note commitment the chain inserts into the note tree at the fork. */
  commitment: Fr;
}

/**
 * Derive the shielded note a given account receives at migration.
 */
export function deriveMigrationNote(params: MigrationNoteParams): MigrationNote {
  const account = params.account ?? 0;
  const base = `${params.viewingKey.spendPk}:${account}:${params.assetId}`;
  const note: Note = {
    value: params.balance,
    assetId: params.assetId,
    ownerPk: params.viewingKey.spendPk,
    rho: labelledField(MIGRATION_RHO_LABEL, base),
    psi: labelledField(MIGRATION_PSI_LABEL, base),
  };
  const hasher = params.commitmentHasher ?? defaultNoteCommitment;
  return { note, commitment: hasher(note) };
}

/**
 * True iff a scanned note is the migration note derived from `params`
 * (matches on value, asset, owner, and both blinding factors).
 */
export function matchesMigrationNote(scanned: Note, params: MigrationNoteParams): boolean {
  const { note } = deriveMigrationNote(params);
  return (
    scanned.value === note.value &&
    scanned.assetId === note.assetId &&
    eqHex(scanned.ownerPk, note.ownerPk) &&
    eqHex(scanned.rho, note.rho) &&
    eqHex(scanned.psi, note.psi)
  );
}

/** Deterministic placeholder commitment: sha256 over the canonical fields. */
export function defaultNoteCommitment(note: Note): Fr {
  const h = createHash('sha256');
  const value = new Uint8Array(16);
  let remaining = note.value;
  for (let i = 0; i < 16; i += 1) {
    value[i] = Number(remaining & 0xffn);
    remaining >>= 8n;
  }
  h.update(value);
  const asset = new Uint8Array(4);
  new DataView(asset.buffer).setUint32(0, note.assetId, true);
  h.update(asset);
  h.update(hexToBytes(note.ownerPk));
  h.update(hexToBytes(note.rho));
  h.update(hexToBytes(note.psi));
  return '0x' + h.digest('hex');
}

function labelledField(label: string, base: string): Fr {
  const h = createHash('sha256');
  h.update(label);
  h.update(':');
  h.update(base);
  return '0x' + h.digest('hex');
}

function eqHex(a: string, b: string): boolean {
  return normalizeHex(a) === normalizeHex(b);
}

function normalizeHex(hex: string): string {
  const body = hex.startsWith('0x') || hex.startsWith('0X') ? hex.slice(2) : hex;
  return body.toLowerCase();
}

function hexToBytes(hex: string): Uint8Array {
  const normalized = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (normalized.length % 2 !== 0) {
    throw new Error('hex string must have an even number of characters');
  }
  const out = new Uint8Array(normalized.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = parseInt(normalized.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}
