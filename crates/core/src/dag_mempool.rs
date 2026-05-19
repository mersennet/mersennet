//! Narwhal-style DAG mempool for parallel data dissemination.
//!
//! Each validator creates transaction batches, which are certified by 2f+1
//! validators. Certified batches form vertices in a DAG. The consensus layer
//! orders vertices, not individual transactions, enabling horizontal bandwidth scaling.

use revm::primitives::{Address, B256, keccak256};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

/// A batch of transactions created by a validator.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionBatch {
    pub id: B256,
    pub author: Address,
    pub round: u64,
    pub transactions: Vec<Vec<u8>>, // Serialized transactions
    pub timestamp: u64,
    pub digest: B256, // Hash of all transactions
}

/// A certificate proving 2f+1 validators have seen a batch.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Certificate {
    pub batch_id: B256,
    pub author: Address,
    pub round: u64,
    pub parents: Vec<B256>,                  // Certificate IDs from round-1
    pub signatures: Vec<(Address, Vec<u8>)>, // Validator signatures
    pub digest: B256,
}

/// A vertex in the DAG (a certified batch with links to parents).
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DagVertex {
    pub certificate: Certificate,
    pub batch: TransactionBatch,
    pub children: HashSet<B256>, // Certificates in round+1 that reference this
    pub is_committed: bool,
}

/// Vote on a batch (pre-certificate).
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchVote {
    pub batch_id: B256,
    pub voter: Address,
    pub round: u64,
    pub signature: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum DagError {
    #[error("batch already exists")]
    DuplicateBatch,
    #[error("batch not found")]
    BatchNotFound,
    #[error("invalid certificate: {0}")]
    InvalidCertificate(String),
    #[error("duplicate vote from {0:?}")]
    DuplicateVote(Address),
    #[error("invalid round: expected {expected}, got {got}")]
    InvalidRound { expected: u64, got: u64 },
    #[error("quorum not reached")]
    QuorumNotReached,
    #[error("missing parent certificate")]
    MissingParent,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DagConfig {
    pub max_batch_size: usize,   // Max transactions per batch
    pub batch_timeout_ms: u64,   // Create batch after timeout even if not full
    pub gc_depth: u64,           // Garbage collect rounds older than this
    pub quorum_threshold: usize, // 2f+1 signatures needed
    pub num_validators: usize,
}

impl Default for DagConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 500,
            batch_timeout_ms: 100,
            gc_depth: 50,
            quorum_threshold: 3, // Default 3 of 4
            num_validators: 4,
        }
    }
}

#[allow(dead_code)]
pub struct DagStats {
    pub total_batches: u64,
    pub total_certificates: u64,
    pub total_committed: u64,
    pub total_transactions: u64,
    pub current_round: u64,
    pub dag_width: usize, // Avg certificates per round
    pub gc_rounds_collected: u64,
    pub throughput_tx_per_sec: f64,
}

impl Default for DagStats {
    fn default() -> Self {
        Self {
            total_batches: 0,
            total_certificates: 0,
            total_committed: 0,
            total_transactions: 0,
            current_round: 0,
            dag_width: 0,
            gc_rounds_collected: 0,
            throughput_tx_per_sec: 0.0,
        }
    }
}

fn compute_batch_digest(transactions: &[Vec<u8>]) -> B256 {
    let mut buf = Vec::new();
    for tx in transactions {
        buf.extend_from_slice(tx);
    }
    keccak256(&buf)
}

fn compute_certificate_digest(
    batch_id: B256,
    author: Address,
    round: u64,
    parents: &[B256],
) -> B256 {
    let mut buf = Vec::new();
    buf.extend_from_slice(batch_id.as_slice());
    buf.extend_from_slice(author.as_slice());
    buf.extend_from_slice(&round.to_be_bytes());
    for p in parents {
        buf.extend_from_slice(p.as_slice());
    }
    keccak256(&buf)
}

#[allow(dead_code)]
pub struct NarwhalDag {
    /// All vertices indexed by certificate digest.
    vertices: HashMap<B256, DagVertex>,
    /// Vertices by round number.
    rounds: HashMap<u64, Vec<B256>>,
    /// Current round.
    current_round: u64,
    /// Pending batches awaiting certification.
    pending_batches: HashMap<B256, TransactionBatch>,
    /// Votes collected per batch.
    pending_votes: HashMap<B256, Vec<BatchVote>>,
    /// Committed (ordered) vertex IDs.
    committed: HashSet<B256>,
    /// Commit order (total order of certificates).
    commit_order: Vec<B256>,
    /// Configuration.
    config: DagConfig,
    /// Stats.
    stats: DagStats,
}

impl NarwhalDag {
    pub fn new(config: DagConfig) -> Self {
        Self {
            vertices: HashMap::new(),
            rounds: HashMap::new(),
            current_round: 0,
            pending_batches: HashMap::new(),
            pending_votes: HashMap::new(),
            committed: HashSet::new(),
            commit_order: Vec::new(),
            config,
            stats: DagStats::default(),
        }
    }

    /// Create a new transaction batch from pending transactions.
    pub fn create_batch(
        &mut self,
        author: Address,
        transactions: Vec<Vec<u8>>,
    ) -> TransactionBatch {
        let transactions = transactions
            .into_iter()
            .take(self.config.max_batch_size)
            .collect::<Vec<_>>();
        let digest = compute_batch_digest(&transactions);
        let round = self.current_round;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let id = keccak256(
            [
                digest.as_slice(),
                author.as_slice(),
                &round.to_be_bytes(),
                &timestamp.to_be_bytes(),
            ]
            .concat(),
        );
        let batch = TransactionBatch {
            id,
            author,
            round,
            transactions: transactions.clone(),
            timestamp,
            digest,
        };
        self.stats.total_batches += 1;
        self.stats.total_transactions += batch.transactions.len() as u64;
        batch
    }

    /// Submit a batch (from this validator or received from network).
    pub fn submit_batch(&mut self, batch: TransactionBatch) -> Result<B256, DagError> {
        if self.pending_batches.contains_key(&batch.id) || self.vertices.contains_key(&batch.id) {
            return Err(DagError::DuplicateBatch);
        }
        let id = batch.id;
        self.pending_batches.insert(batch.id, batch);
        Ok(id)
    }

    /// Vote on a batch (this validator has received and validated it).
    pub fn vote_on_batch(&mut self, batch_id: B256, voter: Address) -> Result<BatchVote, DagError> {
        let batch = self
            .pending_batches
            .get(&batch_id)
            .ok_or(DagError::BatchNotFound)?;
        let round = batch.round;
        let vote = BatchVote {
            batch_id,
            voter,
            round,
            signature: keccak256(format!("vote-{:?}-{:?}-{}", batch_id, voter, round).as_bytes())
                .0
                .to_vec(),
        };
        Ok(vote)
    }

    /// Collect a vote. Returns Some(Certificate) if quorum is reached.
    pub fn collect_vote(&mut self, vote: BatchVote) -> Result<Option<Certificate>, DagError> {
        let batch = self
            .pending_batches
            .get(&vote.batch_id)
            .ok_or(DagError::BatchNotFound)?;

        let votes = self.pending_votes.entry(vote.batch_id).or_default();
        if votes.iter().any(|v| v.voter == vote.voter) {
            return Err(DagError::DuplicateVote(vote.voter));
        }
        votes.push(vote.clone());

        if votes.len() >= self.config.quorum_threshold {
            let parents = if batch.round == 0 {
                vec![B256::ZERO]
            } else {
                self.rounds
                    .get(&(batch.round - 1))
                    .cloned()
                    .unwrap_or_default()
            };
            let digest = compute_certificate_digest(batch.id, batch.author, batch.round, &parents);
            let cert = Certificate {
                batch_id: batch.id,
                author: batch.author,
                round: batch.round,
                parents: parents.clone(),
                signatures: votes
                    .iter()
                    .map(|v| (v.voter, v.signature.clone()))
                    .collect(),
                digest,
            };
            let batch_clone = batch.clone();
            self.pending_batches.remove(&vote.batch_id);
            self.pending_votes.remove(&vote.batch_id);
            self.add_certificate(cert.clone(), batch_clone)?;
            return Ok(Some(cert));
        }
        Ok(None)
    }

    /// Add a certificate to the DAG.
    pub fn add_certificate(
        &mut self,
        cert: Certificate,
        batch: TransactionBatch,
    ) -> Result<(), DagError> {
        if self.vertices.contains_key(&cert.digest) {
            return Ok(()); // Already present
        }
        if !self.verify_certificate(&cert) {
            return Err(DagError::InvalidCertificate(
                "verification failed".to_string(),
            ));
        }
        for parent in &cert.parents {
            if *parent != B256::ZERO && !self.vertices.contains_key(parent) {
                return Err(DagError::MissingParent);
            }
        }

        let vertex = DagVertex {
            certificate: cert.clone(),
            batch,
            children: HashSet::new(),
            is_committed: false,
        };

        for parent in &cert.parents {
            if let Some(pv) = self.vertices.get_mut(parent) {
                pv.children.insert(cert.digest);
            }
        }

        self.vertices.insert(cert.digest, vertex);
        self.rounds.entry(cert.round).or_default().push(cert.digest);
        self.stats.total_certificates += 1;
        Ok(())
    }

    /// Advance to the next round (called after collecting enough certificates).
    pub fn advance_round(&mut self) -> u64 {
        self.current_round += 1;
        self.current_round
    }

    /// Get parent certificates for the current round (to include in new certificates).
    pub fn get_parents(&self) -> Vec<B256> {
        if self.current_round == 0 {
            return vec![B256::ZERO];
        }
        self.rounds
            .get(&(self.current_round - 1))
            .cloned()
            .unwrap_or_default()
    }

    /// Order vertices using a deterministic rule (Bullshark-style).
    /// Returns newly committed certificates in total order.
    pub fn try_commit(&mut self) -> Vec<Certificate> {
        let f = (self.config.num_validators.saturating_sub(1)) / 3;
        let anchor_support_threshold = f + 1;
        let mut newly_committed = Vec::new();

        let min_round = self.rounds.keys().copied().min().unwrap_or(0);
        let max_round = self.rounds.keys().copied().max().unwrap_or(0);

        for round in min_round..=max_round {
            if round % 2 != 0 {
                continue; // Only even rounds have anchors
            }
            let Some(cert_ids) = self.rounds.get(&round) else {
                continue;
            };
            if cert_ids.is_empty() {
                continue;
            }

            let mut sorted: Vec<B256> = cert_ids.to_vec();
            sorted.sort(); // Deterministic by digest
            let anchor_id = sorted[0];

            if self.committed.contains(&anchor_id) {
                continue;
            }

            let next_round = round + 1;
            let next_certs = self
                .rounds
                .get(&next_round)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let mut support_count = 0;
            for cid in next_certs {
                if let Some(v) = self.vertices.get(cid)
                    && v.certificate.parents.contains(&anchor_id)
                {
                    support_count += 1;
                }
            }

            if support_count >= anchor_support_threshold {
                let mut to_commit: Vec<B256> = self.causal_history(&anchor_id);
                to_commit.reverse(); // Parents before children
                for cid in to_commit {
                    if !self.committed.contains(&cid) {
                        self.committed.insert(cid);
                        self.commit_order.push(cid);
                        if let Some(v) = self.vertices.get_mut(&cid) {
                            v.is_committed = true;
                            newly_committed.push(v.certificate.clone());
                        }
                        self.stats.total_committed += 1;
                    }
                }
            }
        }

        newly_committed
    }

    /// Extract ordered transactions from committed certificates.
    pub fn drain_committed_transactions(&mut self) -> Vec<Vec<u8>> {
        let mut txs = Vec::new();
        for &cid in &self.commit_order {
            if let Some(v) = self.vertices.get(&cid) {
                txs.extend(v.batch.transactions.clone());
            }
        }
        self.commit_order.clear();
        self.committed.clear();
        for v in self.vertices.values_mut() {
            v.is_committed = false;
        }
        txs
    }

    /// Garbage collect old rounds.
    pub fn garbage_collect(&mut self) {
        let gc_round = self.current_round.saturating_sub(self.config.gc_depth);
        let rounds_to_remove: Vec<u64> = self
            .rounds
            .keys()
            .copied()
            .filter(|&r| r < gc_round)
            .collect();
        for r in &rounds_to_remove {
            if let Some(cert_ids) = self.rounds.remove(r) {
                for cid in cert_ids {
                    self.vertices.remove(&cid);
                }
                self.stats.gc_rounds_collected += 1;
            }
        }
    }

    /// Check if a certificate is valid (has valid parents, quorum signatures).
    pub fn verify_certificate(&self, cert: &Certificate) -> bool {
        if cert.signatures.len() < self.config.quorum_threshold {
            return false;
        }
        let expected_digest =
            compute_certificate_digest(cert.batch_id, cert.author, cert.round, &cert.parents);
        cert.digest == expected_digest
    }

    /// Get the causal history of a certificate (all ancestors).
    pub fn causal_history(&self, cert_id: &B256) -> Vec<B256> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(*cert_id);
        let mut seen = HashSet::new();
        seen.insert(*cert_id);

        while let Some(cid) = queue.pop_front() {
            result.push(cid);
            if let Some(v) = self.vertices.get(&cid) {
                for parent in &v.certificate.parents {
                    if *parent != B256::ZERO && seen.insert(*parent) {
                        queue.push_back(*parent);
                    }
                }
            }
        }
        result
    }

    /// Get statistics.
    pub fn stats(&self) -> DagStats {
        let total_rounds = self.rounds.len() as u64;
        let dag_width = if total_rounds > 0 {
            self.vertices.len() / total_rounds as usize
        } else {
            0
        };
        DagStats {
            total_batches: self.stats.total_batches,
            total_certificates: self.stats.total_certificates,
            total_committed: self.stats.total_committed,
            total_transactions: self.stats.total_transactions,
            current_round: self.current_round,
            dag_width,
            gc_rounds_collected: self.stats.gc_rounds_collected,
            throughput_tx_per_sec: self.stats.throughput_tx_per_sec,
        }
    }

    /// Current round.
    pub fn current_round(&self) -> u64 {
        self.current_round
    }

    /// Number of vertices in the DAG.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Certificates in a specific round.
    pub fn certificates_in_round(&self, round: u64) -> Vec<&Certificate> {
        self.rounds
            .get(&round)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.vertices.get(id))
                    .map(|v| &v.certificate)
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        Address::from_slice(&[byte; 20])
    }

    #[test]
    fn test_create_batch() {
        let config = DagConfig::default();
        let mut dag = NarwhalDag::new(config);
        let txs = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let batch = dag.create_batch(addr(1), txs.clone());
        assert_eq!(batch.author, addr(1));
        assert_eq!(batch.round, 0);
        assert_eq!(batch.transactions, txs);
        let expected_digest = compute_batch_digest(&batch.transactions);
        assert_eq!(batch.digest, expected_digest);
        assert_ne!(batch.id, B256::ZERO);
    }

    #[test]
    fn test_vote_and_certify() {
        let config = DagConfig {
            quorum_threshold: 3,
            num_validators: 4,
            ..Default::default()
        };
        let mut dag = NarwhalDag::new(config);
        let batch = dag.create_batch(addr(1), vec![vec![1, 2, 3]]);
        dag.submit_batch(batch.clone()).unwrap();

        let v1 = dag.vote_on_batch(batch.id, addr(10)).unwrap();
        assert!(dag.collect_vote(v1).unwrap().is_none());

        let v2 = dag.vote_on_batch(batch.id, addr(11)).unwrap();
        assert!(dag.collect_vote(v2).unwrap().is_none());

        let v3 = dag.vote_on_batch(batch.id, addr(12)).unwrap();
        let cert_opt = dag.collect_vote(v3).unwrap();
        assert!(cert_opt.is_some());
        let cert = cert_opt.unwrap();
        assert_eq!(cert.batch_id, batch.id);
        assert_eq!(cert.signatures.len(), 3);
    }

    #[test]
    fn test_add_certificate_builds_dag() {
        let config = DagConfig {
            quorum_threshold: 2,
            num_validators: 3,
            ..Default::default()
        };
        let mut dag = NarwhalDag::new(config);

        let b0 = dag.create_batch(addr(1), vec![vec![0]]);
        dag.submit_batch(b0.clone()).unwrap();
        for i in 0..2 {
            let v = dag.vote_on_batch(b0.id, addr(10 + i)).unwrap();
            dag.collect_vote(v).unwrap();
        }

        dag.advance_round();
        let parents_r1 = dag.get_parents();
        assert_eq!(parents_r1.len(), 1);

        let b1 = dag.create_batch(addr(2), vec![vec![1]]);
        dag.submit_batch(b1.clone()).unwrap();
        for i in 0..2 {
            let v = dag.vote_on_batch(b1.id, addr(10 + i)).unwrap();
            dag.collect_vote(v).unwrap();
        }

        dag.advance_round();
        let b2 = dag.create_batch(addr(3), vec![vec![2]]);
        dag.submit_batch(b2.clone()).unwrap();
        for i in 0..2 {
            let v = dag.vote_on_batch(b2.id, addr(10 + i)).unwrap();
            dag.collect_vote(v).unwrap();
        }

        assert_eq!(dag.vertex_count(), 3);
        assert_eq!(dag.certificates_in_round(0).len(), 1);
        assert_eq!(dag.certificates_in_round(1).len(), 1);
        assert_eq!(dag.certificates_in_round(2).len(), 1);
    }

    #[test]
    fn test_commit_rule() {
        // f=0 for 3 validators, so anchor needs 1 cert in next round
        let config = DagConfig {
            quorum_threshold: 2,
            num_validators: 3,
            ..Default::default()
        };
        let mut dag = NarwhalDag::new(config);

        let b0 = dag.create_batch(addr(1), vec![vec![0]]);
        dag.submit_batch(b0.clone()).unwrap();
        for i in 0..2 {
            let v = dag.vote_on_batch(b0.id, addr(10 + i)).unwrap();
            dag.collect_vote(v).unwrap();
        }

        dag.advance_round();
        let b1a = dag.create_batch(addr(2), vec![vec![1]]);
        dag.submit_batch(b1a.clone()).unwrap();
        for i in 0..2 {
            let v = dag.vote_on_batch(b1a.id, addr(10 + i)).unwrap();
            dag.collect_vote(v).unwrap();
        }

        dag.advance_round();
        let b2 = dag.create_batch(addr(3), vec![vec![2]]);
        dag.submit_batch(b2.clone()).unwrap();
        for i in 0..2 {
            let v = dag.vote_on_batch(b2.id, addr(10 + i)).unwrap();
            dag.collect_vote(v).unwrap();
        }

        let committed = dag.try_commit();
        assert!(!committed.is_empty(), "anchor at round 0 should commit");
        let txs = dag.drain_committed_transactions();
        assert!(!txs.is_empty(), "should have committed transactions");
    }

    #[test]
    fn test_garbage_collect() {
        let config = DagConfig {
            quorum_threshold: 2,
            num_validators: 3,
            gc_depth: 2,
            ..Default::default()
        };
        let mut dag = NarwhalDag::new(config);

        for r in 0..5 {
            let batch = dag.create_batch(addr(1), vec![vec![r as u8]]);
            dag.submit_batch(batch.clone()).unwrap();
            for i in 0..2 {
                let v = dag.vote_on_batch(batch.id, addr(10 + i)).unwrap();
                dag.collect_vote(v).unwrap();
            }
            dag.advance_round();
        }

        let count_before = dag.vertex_count();
        dag.garbage_collect();
        let count_after = dag.vertex_count();
        assert!(count_after < count_before || count_before <= 3);
        let stats = dag.stats();
        assert!(stats.gc_rounds_collected > 0 || count_before <= 4);
    }
}
