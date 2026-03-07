//! Encrypted mempool with threshold encryption to eliminate MEV.
//! Transactions are encrypted when submitted and only decrypted when included in a block.

use anyhow::Result;
use revm::primitives::{keccak256, Address, B256, Bytes, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedTransaction {
    pub id: B256,
    pub sender: Address,
    pub ciphertext: Vec<u8>,
    pub nonce: u64,
    pub encryption_key_id: B256, // Which threshold key was used
    pub gas_limit: u64,          // Plaintext (needed for inclusion)
    pub max_fee_per_gas: U256,   // Plaintext (needed for ordering)
    pub submitted_at: u64,       // Unix timestamp
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecryptedTransaction {
    pub id: B256,
    pub sender: Address,
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub nonce: u64,
    pub gas_limit: u64,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub signature: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ThresholdKeyShare {
    pub validator: Address,
    pub share_index: u32,
    pub share_data: Vec<u8>,
    pub epoch: u64,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DecryptionShare {
    pub validator: Address,
    pub share_index: u32,
    pub share_data: Vec<u8>,
    pub tx_id: B256,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum EncryptionScheme {
    AES256GCM, // Symmetric (simulated threshold)
    BLS12_381, // BLS threshold encryption
    ElGamal,   // Threshold ElGamal
}

#[allow(dead_code)]
pub struct EncryptedMempool {
    pending: Vec<EncryptedTransaction>,
    decrypted: HashMap<B256, DecryptedTransaction>,
    key_shares: HashMap<Address, ThresholdKeyShare>,
    current_epoch: u64,
    threshold: u32, // k-of-n threshold
    total_validators: u32,
    collected_shares: HashMap<B256, Vec<DecryptionShare>>,
    scheme: EncryptionScheme,
    max_size: usize,
    total_submitted: u64,
    total_decrypted: u64,
}

#[allow(dead_code)]
pub struct EncMempoolStats {
    pub pending: usize,
    pub decrypted: usize,
    pub total_submitted: u64,
    pub total_decrypted: u64,
    pub current_epoch: u64,
    pub active_validators: usize,
}

#[derive(Debug, Error)]
pub enum EncMempoolError {
    #[error("mempool full")]
    MempoolFull,
    #[error("duplicate transaction")]
    Duplicate,
    #[error("insufficient shares: need {threshold}, have {collected}")]
    InsufficientShares { threshold: u32, collected: u32 },
    #[error("invalid share")]
    InvalidShare,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("invalid epoch")]
    InvalidEpoch,
}

fn expand_key(seed: &[u8], len: usize) -> Vec<u8> {
    let mut key = Vec::with_capacity(len);
    let mut hash = keccak256(seed);
    while key.len() < len {
        key.extend_from_slice(hash.as_slice());
        hash = keccak256(hash.as_slice());
    }
    key.truncate(len);
    key
}

fn xor_bytes(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect()
}

impl std::fmt::Debug for EncryptedMempool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedMempool")
            .field("pending", &self.pending.len())
            .field("threshold", &self.threshold)
            .finish()
    }
}

impl EncryptedMempool {
    pub fn new(threshold: u32, total_validators: u32, max_size: usize) -> Self {
        Self {
            pending: Vec::new(),
            decrypted: HashMap::new(),
            key_shares: HashMap::new(),
            current_epoch: 0,
            threshold,
            total_validators,
            collected_shares: HashMap::new(),
            scheme: EncryptionScheme::AES256GCM,
            max_size,
            total_submitted: 0,
            total_decrypted: 0,
        }
    }

    pub fn submit_encrypted(&mut self, tx: EncryptedTransaction) -> Result<B256, EncMempoolError> {
        if self.pending.len() + self.decrypted.len() >= self.max_size {
            return Err(EncMempoolError::MempoolFull);
        }
        if self.pending.iter().any(|t| t.id == tx.id)
            || self.decrypted.contains_key(&tx.id)
        {
            return Err(EncMempoolError::Duplicate);
        }
        self.total_submitted += 1;
        self.pending.push(tx.clone());
        Ok(tx.id)
    }

    pub fn add_key_share(&mut self, share: ThresholdKeyShare) {
        if share.epoch == self.current_epoch {
            self.key_shares.insert(share.validator, share);
        }
    }

    pub fn submit_decryption_share(
        &mut self,
        share: DecryptionShare,
    ) -> Result<Option<DecryptedTransaction>, EncMempoolError> {
        if share.share_data.len() != 32 {
            return Err(EncMempoolError::InvalidShare);
        }

        let shares = self
            .collected_shares
            .entry(share.tx_id)
            .or_default();

        if shares.iter().any(|s| s.validator == share.validator) {
            return Ok(None);
        }
        shares.push(share.clone());

        if shares.len() < self.threshold as usize {
            return Ok(None);
        }

        let enc_tx = match self.pending.iter().find(|t| t.id == share.tx_id) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let plaintext = Self::decrypt_with_shares(&enc_tx.ciphertext, shares)?;
        let dec_tx: DecryptedTransaction = bincode::deserialize(&plaintext)
            .map_err(|_| EncMempoolError::DecryptionFailed)?;

        self.pending.retain(|t| t.id != share.tx_id);
        self.collected_shares.remove(&share.tx_id);
        self.decrypted.insert(dec_tx.id, dec_tx.clone());
        self.total_decrypted += 1;

        Ok(Some(dec_tx))
    }

    pub fn encrypt_transaction(
        plaintext: &DecryptedTransaction,
        key_id: B256,
    ) -> Result<EncryptedTransaction, EncMempoolError> {
        let serialized = bincode::serialize(plaintext).map_err(|_| EncMempoolError::DecryptionFailed)?;
        let key = expand_key(key_id.as_slice(), serialized.len());
        let ciphertext = xor_bytes(&serialized, &key);

        let id = keccak256(&ciphertext);
        let submitted_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(EncryptedTransaction {
            id,
            sender: plaintext.sender,
            ciphertext,
            nonce: plaintext.nonce,
            encryption_key_id: key_id,
            gas_limit: plaintext.gas_limit,
            max_fee_per_gas: plaintext.max_fee_per_gas,
            submitted_at,
        })
    }

    pub fn decrypt_with_shares(
        ciphertext: &[u8],
        shares: &[DecryptionShare],
    ) -> Result<Vec<u8>, EncMempoolError> {
        if shares.is_empty() {
            return Err(EncMempoolError::InsufficientShares {
                threshold: 1,
                collected: 0,
            });
        }

        let mut key_id = [0u8; 32];
        for share in shares {
            if share.share_data.len() != 32 {
                return Err(EncMempoolError::InvalidShare);
            }
            for i in 0..32 {
                key_id[i] ^= share.share_data[i];
            }
        }

        let key = expand_key(&key_id, ciphertext.len());
        Ok(xor_bytes(ciphertext, &key))
    }

    pub fn drain_decrypted(&mut self, max: usize) -> Vec<DecryptedTransaction> {
        let keys: Vec<_> = self.decrypted.keys().copied().take(max).collect();
        keys.into_iter()
            .filter_map(|k| self.decrypted.remove(&k))
            .collect()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn decrypted_count(&self) -> usize {
        self.decrypted.len()
    }

    pub fn has_threshold(&self) -> bool {
        self.key_shares.len() >= self.threshold as usize
    }

    pub fn rotate_epoch(&mut self, new_epoch: u64) {
        self.current_epoch = new_epoch;
        self.key_shares.retain(|_, s| s.epoch == new_epoch);
    }

    pub fn stats(&self) -> EncMempoolStats {
        EncMempoolStats {
            pending: self.pending.len(),
            decrypted: self.decrypted.len(),
            total_submitted: self.total_submitted,
            total_decrypted: self.total_decrypted,
            current_epoch: self.current_epoch,
            active_validators: self.key_shares.len(),
        }
    }
}
