//! Flat state architecture for Prime Chain.
//!
//! Separates current account state from Merkle trie computation.
//! Maintains a flat key-value cache for O(1) reads; Merkle proofs
//! are only computed when needed (light clients, bridges).

use anyhow::Result;
use revm::primitives::{AccountInfo, Address, B256, Bytecode, Bytes, U256, KECCAK_EMPTY};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// Flat State Types
// ---------------------------------------------------------------------------

/// Flat state stores current account state in a HashMap for O(1) reads.
/// Merkle proofs are only computed when needed (for light clients/bridges).
#[allow(dead_code)]
pub struct FlatState {
    accounts: Arc<RwLock<HashMap<Address, FlatAccount>>>,
    storage: Arc<RwLock<HashMap<(Address, U256), U256>>>,
    code: Arc<RwLock<HashMap<B256, Vec<u8>>>>,
    block_height: Arc<RwLock<u64>>,
    state_root: Arc<RwLock<B256>>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlatAccount {
    pub balance: U256,
    pub nonce: u64,
    pub code_hash: B256,
    pub storage_root: B256,
}

#[allow(dead_code)]
pub struct StateChangeset {
    pub account_changes: Vec<(Address, FlatAccount)>,
    pub storage_changes: Vec<(Address, U256, U256)>,
    pub code_changes: Vec<(B256, Vec<u8>)>,
}

#[allow(dead_code)]
pub struct FlatStateStats {
    pub accounts: usize,
    pub storage_slots: usize,
    pub code_entries: usize,
    pub block_height: u64,
}

// ---------------------------------------------------------------------------
// FlatState Implementation
// ---------------------------------------------------------------------------

impl FlatState {
    pub fn new() -> Self {
        Self {
            accounts: Arc::new(RwLock::new(HashMap::new())),
            storage: Arc::new(RwLock::new(HashMap::new())),
            code: Arc::new(RwLock::new(HashMap::new())),
            block_height: Arc::new(RwLock::new(0)),
            state_root: Arc::new(RwLock::new(B256::ZERO)),
        }
    }

    pub fn get_account(&self, address: &Address) -> Option<FlatAccount> {
        self.accounts.read().unwrap().get(address).cloned()
    }

    pub fn get_storage(&self, address: &Address, slot: &U256) -> U256 {
        self.storage
            .read()
            .unwrap()
            .get(&(*address, *slot))
            .copied()
            .unwrap_or(U256::ZERO)
    }

    pub fn get_code(&self, code_hash: &B256) -> Option<Vec<u8>> {
        self.code.read().unwrap().get(code_hash).cloned()
    }

    pub fn update_account(&self, address: Address, account: FlatAccount) {
        self.accounts.write().unwrap().insert(address, account);
    }

    pub fn update_storage(&self, address: Address, slot: U256, value: U256) {
        self.storage.write().unwrap().insert((address, slot), value);
    }

    pub fn store_code(&self, code_hash: B256, code: Vec<u8>) {
        self.code.write().unwrap().insert(code_hash, code);
    }

    pub fn commit_block(&self, height: u64, state_root: B256, changes: StateChangeset) -> Result<()> {
        let mut accounts = self.accounts.write().unwrap();
        let mut storage = self.storage.write().unwrap();
        let mut code = self.code.write().unwrap();

        for (addr, account) in changes.account_changes {
            accounts.insert(addr, account);
        }
        for (addr, slot, value) in changes.storage_changes {
            storage.insert((addr, slot), value);
        }
        for (hash, bytes) in changes.code_changes {
            code.insert(hash, bytes);
        }

        *self.block_height.write().unwrap() = height;
        *self.state_root.write().unwrap() = state_root;

        Ok(())
    }

    pub fn rollback(&self, _height: u64) {
        // Placeholder for future rollback support
    }

    pub fn to_account_info(&self, address: &Address) -> Option<AccountInfo> {
        let account = self.get_account(address)?;
        let code = if account.code_hash == KECCAK_EMPTY {
            Bytecode::new()
        } else {
            self.get_code(&account.code_hash)
                .map(|b| Bytecode::new_raw(Bytes::from(b)))
                .unwrap_or_else(Bytecode::new)
        };
        Some(AccountInfo::new(
            account.balance,
            account.nonce,
            account.code_hash,
            code,
        ))
    }

    pub fn current_height(&self) -> u64 {
        *self.block_height.read().unwrap()
    }

    pub fn current_state_root(&self) -> B256 {
        *self.state_root.read().unwrap()
    }

    pub fn account_count(&self) -> usize {
        self.accounts.read().unwrap().len()
    }

    pub fn stats(&self) -> FlatStateStats {
        let accounts = self.accounts.read().unwrap();
        let storage = self.storage.read().unwrap();
        let code = self.code.read().unwrap();
        let block_height = *self.block_height.read().unwrap();

        FlatStateStats {
            accounts: accounts.len(),
            storage_slots: storage.len(),
            code_entries: code.len(),
            block_height,
        }
    }
}

impl std::fmt::Debug for FlatState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlatState")
            .field("block_height", &*self.block_height.read().unwrap())
            .field("accounts", &self.accounts.read().unwrap().len())
            .finish()
    }
}

impl Default for FlatState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// FlatStateCache - Hot-path reads
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct FlatStateCache {
    hot_accounts: HashMap<Address, FlatAccount>,
    hot_storage: HashMap<(Address, U256), U256>,
    max_entries: usize,
    hits: u64,
    misses: u64,
}

impl FlatStateCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            hot_accounts: HashMap::new(),
            hot_storage: HashMap::new(),
            max_entries,
            hits: 0,
            misses: 0,
        }
    }

    pub fn get_account(&mut self, address: &Address) -> Option<FlatAccount> {
        if let Some(acc) = self.hot_accounts.get(address) {
            self.hits += 1;
            Some(acc.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn get_storage(&mut self, address: &Address, slot: &U256) -> Option<U256> {
        if let Some(val) = self.hot_storage.get(&(*address, *slot)) {
            self.hits += 1;
            Some(*val)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn insert_account(&mut self, address: Address, account: FlatAccount) {
        if self.hot_accounts.len() >= self.max_entries && !self.hot_accounts.contains_key(&address) {
            // Evict oldest (arbitrary) - in production use LRU
            if let Some(k) = self.hot_accounts.keys().next().copied() {
                self.hot_accounts.remove(&k);
            }
        }
        self.hot_accounts.insert(address, account);
    }

    pub fn insert_storage(&mut self, address: Address, slot: U256, value: U256) {
        if self.hot_storage.len() >= self.max_entries
            && !self.hot_storage.contains_key(&(address, slot))
        {
            if let Some(k) = self.hot_storage.keys().next().copied() {
                self.hot_storage.remove(&k);
            }
        }
        self.hot_storage.insert((address, slot), value);
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn clear(&mut self) {
        self.hot_accounts.clear();
        self.hot_storage.clear();
        self.hits = 0;
        self.misses = 0;
    }

    pub fn len(&self) -> usize {
        self.hot_accounts.len() + self.hot_storage.len()
    }
}
