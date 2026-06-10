//! ERC-4337 Account Abstraction module for Mersennet.
//!
//! Enables smart contract wallets to pay gas with any token, batch transactions,
//! and use social recovery — critical for UX on a trading chain.

#![allow(dead_code)]

use revm::primitives::{Address, B256, Bytes, U256, keccak256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// --- UserOperation ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserOperation {
    pub sender: Address,
    pub nonce: U256,
    pub init_code: Bytes,
    pub call_data: Bytes,
    pub call_gas_limit: u64,
    pub verification_gas_limit: u64,
    pub pre_verification_gas: u64,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub paymaster_and_data: Bytes,
    pub signature: Bytes,
}

impl UserOperation {
    /// Compute UserOperation hash per ERC-4337:
    /// hash = keccak256(abi.encode(keccak256(pack(op)), entry_point, chain_id))
    /// where pack(op) excludes signature and hashes dynamic bytes.
    pub fn hash(&self, entry_point: Address, chain_id: u64) -> B256 {
        let packed = self.pack_for_hash();
        let packed_hash = keccak256(&packed);
        let mut buf = Vec::with_capacity(32 + 32 + 32);
        buf.extend_from_slice(packed_hash.as_slice());
        buf.extend_from_slice(&[0u8; 12]); // address left-pad
        buf.extend_from_slice(entry_point.as_slice());
        buf.extend_from_slice(&U256::from(chain_id).to_be_bytes::<32>());
        keccak256(buf)
    }

    /// Pack UserOperation fields (excluding signature) for hash.
    /// Dynamic fields (init_code, call_data, paymaster_and_data) are keccak256-hashed.
    fn pack_for_hash(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 * 10);
        // sender (address, 32 bytes left-padded)
        buf.extend_from_slice(&[0u8; 12]);
        buf.extend_from_slice(self.sender.as_slice());
        // nonce
        buf.extend_from_slice(&self.nonce.to_be_bytes::<32>());
        // keccak256(init_code)
        buf.extend_from_slice(keccak256(self.init_code.as_ref()).as_slice());
        // keccak256(call_data)
        buf.extend_from_slice(keccak256(self.call_data.as_ref()).as_slice());
        // call_gas_limit
        buf.extend_from_slice(&U256::from(self.call_gas_limit).to_be_bytes::<32>());
        // verification_gas_limit
        buf.extend_from_slice(&U256::from(self.verification_gas_limit).to_be_bytes::<32>());
        // pre_verification_gas
        buf.extend_from_slice(&U256::from(self.pre_verification_gas).to_be_bytes::<32>());
        // max_fee_per_gas
        buf.extend_from_slice(&self.max_fee_per_gas.to_be_bytes::<32>());
        // max_priority_fee_per_gas
        buf.extend_from_slice(&self.max_priority_fee_per_gas.to_be_bytes::<32>());
        // keccak256(paymaster_and_data)
        buf.extend_from_slice(keccak256(self.paymaster_and_data.as_ref()).as_slice());
        buf
    }
}

// --- ValidationResult & UserOpResult ---

#[derive(Clone, Debug)]
pub struct ValidationResult {
    pub valid: bool,
    pub pre_fund: U256,
    pub valid_after: u64,
    pub valid_until: u64,
    pub paymaster: Option<Address>,
}

#[derive(Clone, Debug)]
pub struct UserOpResult {
    pub op_hash: B256,
    pub success: bool,
    pub gas_used: u64,
    pub error: Option<String>,
}

// --- StakeInfo ---

#[derive(Clone, Debug)]
pub struct StakeInfo {
    pub stake: U256,
    pub unstake_delay: u64,
}

// --- AAError ---

#[derive(Debug, Error)]
pub enum AAError {
    #[error("invalid nonce: expected {expected}, got {got}")]
    InvalidNonce { expected: U256, got: U256 },
    #[error("insufficient prefund: need {needed}, have {available}")]
    InsufficientPrefund { needed: U256, available: U256 },
    #[error("invalid signature")]
    InvalidSignature,
    #[error("gas limit too low")]
    GasLimitTooLow,
    #[error("paymaster not staked")]
    PaymasterNotStaked,
    #[error("sender already exists")]
    SenderExists,
    #[error("mempool full")]
    MempoolFull,
    #[error("user op expired")]
    Expired,
}

// --- EntryPoint ---

/// Standard EntryPoint address: 0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789
pub const ENTRY_POINT_ADDRESS: Address = Address::new([
    0x5F, 0xF1, 0x37, 0xD4, 0xb0, 0xFD, 0xCD, 0x49, 0xDC, 0xA3, 0x0c, 0x7C, 0xF5, 0x7E, 0x57, 0x8a,
    0x02, 0x6d, 0x27, 0x89,
]);

pub struct EntryPoint {
    pub address: Address,
    nonce_map: HashMap<Address, U256>,
    stakes: HashMap<Address, StakeInfo>,
}

impl EntryPoint {
    pub fn new() -> Self {
        Self {
            address: ENTRY_POINT_ADDRESS,
            nonce_map: HashMap::new(),
            stakes: HashMap::new(),
        }
    }

    pub fn validate_user_op(
        &self,
        op: &UserOperation,
        _chain_id: u64,
    ) -> Result<ValidationResult, AAError> {
        // Basic gas limit checks
        if op.call_gas_limit < 21000 {
            return Err(AAError::GasLimitTooLow);
        }
        if op.verification_gas_limit < 10000 {
            return Err(AAError::GasLimitTooLow);
        }
        if op.pre_verification_gas < 21000 {
            return Err(AAError::GasLimitTooLow);
        }

        // Nonce check
        let expected = self.get_nonce(op.sender);
        if op.nonce != expected {
            return Err(AAError::InvalidNonce {
                expected,
                got: op.nonce,
            });
        }

        // Signature length check (basic; full verification would use EVM)
        if op.signature.is_empty() {
            return Err(AAError::InvalidSignature);
        }

        // Paymaster check if present
        let paymaster = if op.paymaster_and_data.len() >= 20 {
            Some(Address::from_slice(&op.paymaster_and_data[0..20]))
        } else {
            None
        };
        if let Some(pm) = paymaster
            && !self.stakes.contains_key(&pm)
        {
            return Err(AAError::PaymasterNotStaked);
        }

        let pre_fund = U256::from(op.pre_verification_gas)
            .saturating_add(U256::from(op.verification_gas_limit))
            .saturating_mul(op.max_fee_per_gas);

        Ok(ValidationResult {
            valid: true,
            pre_fund,
            valid_after: 0,
            valid_until: u64::MAX,
            paymaster,
        })
    }

    pub fn handle_ops(&mut self, ops: Vec<UserOperation>, chain_id: u64) -> Vec<UserOpResult> {
        let mut results = Vec::with_capacity(ops.len());
        for op in ops {
            let op_hash = op.hash(self.address, chain_id);
            match self.validate_user_op(&op, chain_id) {
                Ok(_) => {
                    self.increment_nonce(op.sender);
                    results.push(UserOpResult {
                        op_hash,
                        success: true,
                        gas_used: op
                            .pre_verification_gas
                            .saturating_add(op.verification_gas_limit)
                            .saturating_add(op.call_gas_limit),
                        error: None,
                    });
                }
                Err(e) => {
                    results.push(UserOpResult {
                        op_hash,
                        success: false,
                        gas_used: 0,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
        results
    }

    pub fn get_nonce(&self, sender: Address) -> U256 {
        self.nonce_map.get(&sender).copied().unwrap_or(U256::ZERO)
    }

    pub fn increment_nonce(&mut self, sender: Address) {
        let n = self.get_nonce(sender);
        self.nonce_map.insert(sender, n.saturating_add(U256::ONE));
    }

    pub fn stake(&mut self, addr: Address, stake: U256, unstake_delay: u64) {
        self.stakes.insert(
            addr,
            StakeInfo {
                stake,
                unstake_delay,
            },
        );
    }
}

impl Default for EntryPoint {
    fn default() -> Self {
        Self::new()
    }
}

// --- UserOpMempool ---

pub struct UserOpMempool {
    ops: Vec<UserOperation>,
    max_size: usize,
    by_sender: HashMap<Address, Vec<usize>>,
    by_hash: HashMap<B256, usize>,
}

impl UserOpMempool {
    pub fn new(max_size: usize) -> Self {
        Self {
            ops: Vec::new(),
            max_size,
            by_sender: HashMap::new(),
            by_hash: HashMap::new(),
        }
    }

    pub fn add(
        &mut self,
        op: UserOperation,
        entry_point: Address,
        chain_id: u64,
    ) -> Result<(), AAError> {
        if self.ops.len() >= self.max_size {
            return Err(AAError::MempoolFull);
        }
        let op_hash = op.hash(entry_point, chain_id);
        if self.by_hash.contains_key(&op_hash) {
            return Ok(()); // already present
        }
        let idx = self.ops.len();
        self.by_hash.insert(op_hash, idx);
        self.ops.push(op);
        self.by_sender
            .entry(self.ops[idx].sender)
            .or_default()
            .push(idx);
        Ok(())
    }

    pub fn remove(&mut self, op_hash: B256) {
        if let Some(&pos) = self.by_hash.get(&op_hash) {
            self.by_hash.remove(&op_hash);
            self.ops.remove(pos);
            self.rebuild_indexes_after_removal(pos);
        }
    }

    fn rebuild_indexes_after_removal(&mut self, _removed_pos: usize) {
        self.by_hash.clear();
        self.by_sender.clear();
        for (idx, op) in self.ops.iter().enumerate() {
            self.by_sender.entry(op.sender).or_default().push(idx);
        }
    }

    pub fn drain_ready(&mut self, max_ops: usize) -> Vec<UserOperation> {
        let take = max_ops.min(self.ops.len());
        let drained: Vec<UserOperation> = self.ops.drain(..take).collect();
        self.by_hash.clear();
        self.by_sender.clear();
        for (idx, op) in self.ops.iter().enumerate() {
            self.by_sender.entry(op.sender).or_default().push(idx);
        }
        drained
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

// --- Bundler ---

pub struct BundleTransaction {
    pub ops: Vec<UserOperation>,
    pub total_gas: u64,
    pub beneficiary: Address,
}

pub struct Bundler {
    entry_point: EntryPoint,
    mempool: UserOpMempool,
    max_bundle_size: usize,
}

impl std::fmt::Debug for Bundler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bundler")
            .field("max_bundle_size", &self.max_bundle_size)
            .field("mempool_len", &self.mempool.len())
            .finish()
    }
}

impl Bundler {
    pub fn new(max_bundle_size: usize) -> Self {
        Self {
            entry_point: EntryPoint::new(),
            mempool: UserOpMempool::new(1000),
            max_bundle_size,
        }
    }

    pub fn submit_op(&mut self, op: UserOperation, chain_id: u64) -> Result<B256, AAError> {
        self.entry_point.validate_user_op(&op, chain_id)?;
        let op_hash = op.hash(self.entry_point.address, chain_id);
        self.mempool.add(op, self.entry_point.address, chain_id)?;
        Ok(op_hash)
    }

    pub fn create_bundle(&mut self, beneficiary: Address) -> Option<BundleTransaction> {
        let ops = self.mempool.drain_ready(self.max_bundle_size);
        if ops.is_empty() {
            return None;
        }
        let total_gas: u64 = ops
            .iter()
            .map(|op| {
                op.pre_verification_gas
                    .saturating_add(op.verification_gas_limit)
                    .saturating_add(op.call_gas_limit)
            })
            .sum();
        Some(BundleTransaction {
            ops,
            total_gas,
            beneficiary,
        })
    }

    pub fn entry_point(&self) -> &EntryPoint {
        &self.entry_point
    }
}
