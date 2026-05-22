//! Redb-backed persistence for the shielded subsystems.
//!
//! Workstream A7 of the privacy-redesign plan (see
//! `docs/internal/zk-privacy-plan.md`). Provides crash-safe storage
//! for:
//!
//! - the shielded note-commitment leaves (append-only)
//! - the spent-nullifier set
//! - the recent-roots ring buffer (rebuilt on load)
//! - transparent balances for shielded EOAs (the bridge mirror)
//! - the liquidation auction's pending state
//!
//! The persistence layer is intentionally separate from the main
//! `RedbState` because the privacy hard fork ships these tables
//! lazily (pre-fork they're empty / zero-sized) and we don't want
//! every legacy node to migrate its schema before activation.

#![allow(dead_code)]

use anyhow::{Context, Result};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use revm::primitives::{Address, U256};
use std::collections::HashMap;
use std::path::Path;

use crate::shielded_evm::ShieldedEvm;
use crate::shielded_state::{ShieldedSnapshot, ShieldedState};

const SHIELDED_LEAVES: TableDefinition<u64, &[u8]> = TableDefinition::new("shielded_leaves");
const SHIELDED_NULLIFIERS: TableDefinition<&[u8], ()> = TableDefinition::new("shielded_nullifiers");
const TRANSPARENT_BALANCES: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("transparent_balances");
const AUCTION_STATE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("auction_state");
const META: TableDefinition<&[u8], &[u8]> = TableDefinition::new("shielded_meta");

const META_KEY_LATEST_HEIGHT: &[u8] = b"latest_height";
const META_KEY_LATEST_ROOT: &[u8] = b"latest_root";

/// Redb-backed storage for the shielded subsystems. One per node.
pub struct ShieldedPersistence {
    db: Database,
}

impl std::fmt::Debug for ShieldedPersistence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShieldedPersistence").finish()
    }
}

impl ShieldedPersistence {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db_path = path.as_ref().join("prime_chain_shielded.redb");
        let db =
            Database::create(&db_path).with_context(|| format!("opening {}", db_path.display()))?;

        {
            let write_txn = db.begin_write()?;
            let _ = write_txn.open_table(SHIELDED_LEAVES)?;
            let _ = write_txn.open_table(SHIELDED_NULLIFIERS)?;
            let _ = write_txn.open_table(TRANSPARENT_BALANCES)?;
            let _ = write_txn.open_table(AUCTION_STATE)?;
            let _ = write_txn.open_table(META)?;
            write_txn.commit()?;
        }

        Ok(Self { db })
    }

    /// Persist the entire shielded EVM state and the canonical
    /// shielded-state snapshot. Called at end-of-block when
    /// `privacy_mode_activated`. Idempotent.
    pub fn save_shielded_evm(&self, evm: &ShieldedEvm, block_height: u64) -> Result<()> {
        let snapshot = evm.state.snapshot();
        self.save_shielded_state_snapshot(&snapshot, block_height)?;
        self.save_transparent_balances(&evm.transparent_balances)?;
        Ok(())
    }

    /// Persist just the shielded-state snapshot. Used by the
    /// state_proof checkpoint flow.
    pub fn save_shielded_state_snapshot(
        &self,
        snapshot: &ShieldedSnapshot,
        block_height: u64,
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;

        // Append new leaves: redb append is just an upsert on
        // monotonic keys.
        {
            let mut leaves_t = write_txn.open_table(SHIELDED_LEAVES)?;
            for (i, leaf) in snapshot.leaves.iter().enumerate() {
                leaves_t.insert(i as u64, leaf.as_slice())?;
            }
        }

        {
            let mut nul_t = write_txn.open_table(SHIELDED_NULLIFIERS)?;
            for n in &snapshot.nullifiers {
                nul_t.insert(n.as_slice(), ())?;
            }
        }

        {
            let mut meta_t = write_txn.open_table(META)?;
            meta_t.insert(
                META_KEY_LATEST_HEIGHT,
                block_height.to_le_bytes().as_slice(),
            )?;
            if let Some(latest) = snapshot.recent_roots.last() {
                meta_t.insert(META_KEY_LATEST_ROOT, latest.as_slice())?;
            }
        }

        write_txn.commit()?;
        Ok(())
    }

    fn save_transparent_balances(
        &self,
        balances: &HashMap<Address, U256>, // privacy-allow: transparent-side mirror persistence
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut bal_t = write_txn.open_table(TRANSPARENT_BALANCES)?;
            for (addr, amount) in balances {
                let value_bytes = amount.to_be_bytes::<32>();
                bal_t.insert(addr.as_slice(), value_bytes.as_slice())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Reload the shielded state from disk. Called at node startup.
    /// Returns `None` if no snapshot is present yet (i.e. the node
    /// has never run with privacy mode activated).
    pub fn load_shielded_state(&self) -> Result<Option<ShieldedState>> {
        let read_txn = self.db.begin_read()?;

        let leaves_t = read_txn.open_table(SHIELDED_LEAVES)?;
        let mut leaves: Vec<(u64, [u8; 32])> = Vec::new();
        for entry in leaves_t.iter()? {
            let (k, v) = entry?;
            let mut arr = [0u8; 32];
            if v.value().len() != 32 {
                continue;
            }
            arr.copy_from_slice(v.value());
            leaves.push((k.value(), arr));
        }
        if leaves.is_empty() {
            return Ok(None);
        }
        leaves.sort_by_key(|(i, _)| *i);

        let nul_t = read_txn.open_table(SHIELDED_NULLIFIERS)?;
        let mut nullifiers: Vec<[u8; 32]> = Vec::new();
        for entry in nul_t.iter()? {
            let (k, _) = entry?;
            if k.value().len() != 32 {
                continue;
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(k.value());
            nullifiers.push(arr);
        }

        let snapshot = ShieldedSnapshot {
            leaves: leaves.into_iter().map(|(_, l)| l).collect(),
            nullifiers,
            recent_roots: Vec::new(),
        };
        Ok(Some(ShieldedState::restore(&snapshot)))
    }

    /// Reload transparent balances. Used at startup.
    pub fn load_transparent_balances(&self) -> Result<HashMap<Address, U256>> {
        // privacy-allow: transparent-side mirror reload
        let read_txn = self.db.begin_read()?;
        let bal_t = read_txn.open_table(TRANSPARENT_BALANCES)?;
        let mut out = HashMap::new();
        for entry in bal_t.iter()? {
            let (k, v) = entry?;
            if k.value().len() != 20 || v.value().len() != 32 {
                continue;
            }
            let addr = Address::from_slice(k.value());
            let amount = U256::from_be_slice(v.value());
            out.insert(addr, amount);
        }
        Ok(out)
    }

    /// Latest persisted block height. Returns `None` if nothing has
    /// been written yet.
    pub fn latest_height(&self) -> Result<Option<u64>> {
        let read_txn = self.db.begin_read()?;
        let meta_t = read_txn.open_table(META)?;
        match meta_t.get(META_KEY_LATEST_HEIGHT)? {
            Some(v) => {
                if v.value().len() == 8 {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(v.value());
                    Ok(Some(u64::from_le_bytes(b)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prime_zkp::{Fr, NoteCommitment, Nullifier};

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = ShieldedPersistence::open(dir.path()).unwrap();

        let mut state = ShieldedState::new();
        let cm = NoteCommitment(Fr::from_u64(0xc0ffee));
        let _ = state.insert_note(cm).unwrap();
        state.spend(Nullifier(Fr::from_u64(42))).unwrap();
        let original_root = state.current_root();

        p.save_shielded_state_snapshot(&state.snapshot(), 7)
            .unwrap();

        let loaded = p.load_shielded_state().unwrap().expect("snapshot exists");
        assert_eq!(loaded.current_root(), original_root);
        assert_eq!(loaded.nullifier_count(), 1);
        assert!(loaded.is_spent(&Nullifier(Fr::from_u64(42))));
        assert_eq!(p.latest_height().unwrap(), Some(7));
    }

    #[test]
    fn empty_load_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let p = ShieldedPersistence::open(dir.path()).unwrap();
        assert!(p.load_shielded_state().unwrap().is_none());
        assert_eq!(p.latest_height().unwrap(), None);
    }
}
