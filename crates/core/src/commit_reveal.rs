#![allow(dead_code)]

use revm::primitives::{keccak256, Address, B256, Bytes};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct TxCommitment {
    pub commitment_hash: B256,
    pub sender: Address,
    pub block_number: u64,
}

#[derive(Clone, Debug)]
pub struct TxReveal {
    pub commitment_hash: B256,
    pub encrypted_tx: Bytes,
    pub salt: B256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitRevealError {
    DuplicateCommitment,
    CommitmentNotFound,
    CommitmentExpired,
    InvalidReveal,
    AlreadyRevealed,
}

impl std::fmt::Display for CommitRevealError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitRevealError::DuplicateCommitment => write!(f, "duplicate commitment"),
            CommitRevealError::CommitmentNotFound => write!(f, "commitment not found"),
            CommitRevealError::CommitmentExpired => write!(f, "commitment expired"),
            CommitRevealError::InvalidReveal => write!(f, "invalid reveal"),
            CommitRevealError::AlreadyRevealed => write!(f, "already revealed"),
        }
    }
}

impl std::error::Error for CommitRevealError {}

#[derive(Debug, Default)]
pub struct CommitRevealPool {
    commitments: HashMap<B256, TxCommitment>,
    revealed: Vec<(B256, Bytes)>,
    revealed_set: HashMap<B256, ()>,
    pub commit_window: u64,
    pub current_block: u64,
}

impl CommitRevealPool {
    pub fn new(commit_window: u64) -> Self {
        Self {
            commitments: HashMap::new(),
            revealed: Vec::new(),
            revealed_set: HashMap::new(),
            commit_window,
            current_block: 0,
        }
    }

    pub fn commit(&mut self, commitment: TxCommitment) -> Result<(), CommitRevealError> {
        if self.commitments.contains_key(&commitment.commitment_hash) {
            return Err(CommitRevealError::DuplicateCommitment);
        }
        if self.revealed_set.contains_key(&commitment.commitment_hash) {
            return Err(CommitRevealError::AlreadyRevealed);
        }
        self.commitments
            .insert(commitment.commitment_hash, commitment);
        Ok(())
    }

    pub fn reveal(&mut self, reveal: TxReveal) -> Result<Bytes, CommitRevealError> {
        if self.revealed_set.contains_key(&reveal.commitment_hash) {
            return Err(CommitRevealError::AlreadyRevealed);
        }

        let commitment = self
            .commitments
            .get(&reveal.commitment_hash)
            .ok_or(CommitRevealError::CommitmentNotFound)?;

        if self.current_block > commitment.block_number + self.commit_window {
            self.commitments.remove(&reveal.commitment_hash);
            return Err(CommitRevealError::CommitmentExpired);
        }

        if !Self::verify_reveal(commitment, &reveal) {
            return Err(CommitRevealError::InvalidReveal);
        }

        self.commitments.remove(&reveal.commitment_hash);
        let tx_data = reveal.encrypted_tx.clone();
        self.revealed
            .push((reveal.commitment_hash, reveal.encrypted_tx));
        self.revealed_set.insert(reveal.commitment_hash, ());

        Ok(tx_data)
    }

    pub fn drain_revealed(&mut self) -> Vec<Bytes> {
        let drained: Vec<Bytes> = self.revealed.drain(..).map(|(_, tx)| tx).collect();
        drained
    }

    pub fn prune_expired(&mut self, current_block: u64) {
        self.current_block = current_block;
        self.commitments
            .retain(|_, c| current_block <= c.block_number + self.commit_window);
        self.revealed_set.clear();
    }

    fn verify_reveal(commitment: &TxCommitment, reveal: &TxReveal) -> bool {
        let mut data = Vec::new();
        data.extend_from_slice(reveal.encrypted_tx.as_ref());
        data.extend_from_slice(reveal.salt.as_ref());
        keccak256(data) == commitment.commitment_hash
    }

    pub fn pending_commitments(&self) -> usize {
        self.commitments.len()
    }

    pub fn revealed_count(&self) -> usize {
        self.revealed.len()
    }
}
