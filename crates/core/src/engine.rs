use crate::bridge::{BridgeDomain, BridgeMessage, BridgeQueue};
use crate::code_publication::{CodePublicationRegistry, PublishedCodeAttestation};
use crate::commit_reveal::{CommitRevealError, CommitRevealPool, TxCommitment, TxReveal};
use crate::consensus::{
    Consensus, EvidenceKind, Finalization, Reward, RoundResult, Slashing, SlashingEvidence,
    Unbonding, Validator, ValidatorChange,
};
use crate::crypto::{self, SignedTransaction};

/// On import, a block's proposer is accepted if it is the elected leader for
/// this height at any round in `0..MAX_LEADER_ROUND_WINDOW`. The leader rotates
/// each round on timeout, so this bounds how far a legitimately-elected leader
/// can be from round 0 while still rejecting non-leaders.
const MAX_LEADER_ROUND_WINDOW: u64 = 32;
use crate::errors::MersennetOrdersError;
use crate::events::{
    BridgeEvent, BridgeQueueKind, DomainEvent, DomainEventRecord, MersennetOrdersEvent,
};
use crate::fba::{AuctionResult, BatchOrder, FBAEngine};
use crate::hotstuff2::{HotStuff2, HotStuff2Result};
use crate::mempool::{Mempool, TxRejection};
use crate::mersennet_orders::{
    MarketId, MersennetOrdersState, Order, OrderBookView, OrderId, OrderOutcome, Side, TimeInForce,
};
use crate::network::NetworkSim;
use crate::parallel::ParallelExecutor;
use crate::precompiles;
use crate::state::PersistentState;
use crate::state::SnapshotMeta;
use crate::state_redb::RedbState;
use crate::state_trait::StateBackend;
use anyhow::Result;
use revm::db::InMemoryDB;
use revm::primitives::{
    AccountInfo, Address, B256, Bytecode, Bytes, Env, ExecutionResult, KECCAK_EMPTY, SpecId,
    TxKind, U256, keccak256,
};
use revm::{Database, Evm};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Best-effort conversion of a `U256` to `f64` for Prometheus
/// gauges. Saturates at `u128::MAX` worth of precision; that's
/// fine for monitoring (no integer correctness depends on it).
#[inline]
fn f64_from_u256(v: U256) -> f64 {
    let lo: u128 = v.try_into().unwrap_or(u128::MAX);
    lo as f64
}

use crate::account_abstraction::Bundler;
use crate::encrypted_mempool::EncryptedMempool;
use crate::flat_state::{FlatAccount, FlatState, StateChangeset};
use crate::formal_verification::InvariantChecker;
use crate::intents::IntentEngine;
use crate::liquidation_auction::LiquidationAuction;
use crate::mainnet::MainnetGuard;
use crate::market_maker::MarketMakerEngine;
use crate::shielded_evm::{ShieldedEnvelope, ShieldedEvm};
use crate::shielded_orders::{ShieldedOrdersEngine, decode_threshold_order_intent};
use crate::shielded_persistence::ShieldedPersistence;
use crate::threshold_mempool::ThresholdMempool;
use mersennet_zkp::sp1::{CanonicalShieldedEvent, U256Bytes, build_shielded_tick_events};

/// Synthetic execution result returned by `apply_shielded_tx` when
/// the shielded path is unavailable (e.g. pre-fork, missing payload).
fn shielded_failed_execution(reason: &str) -> TxExecution {
    let topic = B256::from(keccak256(b"ShieldedTxRejected(string)"));
    TxExecution {
        success: false,
        gas_used: 0,
        output: Bytes::new(),
        created_address: None,
        logs: vec![LogEntry {
            address: Address::ZERO,
            topics: vec![topic],
            data: Bytes::from(reason.as_bytes().to_vec()),
        }],
    }
}

pub mod abi {
    use super::*;

    #[allow(dead_code)]
    pub fn selector(signature: &str) -> [u8; 4] {
        let hash = keccak256(signature.as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    }

    #[allow(dead_code)]
    pub fn decode_u64(output: &Bytes) -> Option<u64> {
        if output.len() < 32 {
            return None;
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&output[24..32]);
        Some(u64::from_be_bytes(bytes))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Bytes,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub nonce: u64,
    pub chain_id: Option<u64>,
    /// ECDSA signature (r, s, v). None means unsigned (legacy/backward-compat).
    pub signature: Option<(U256, U256, u64)>,
    /// EIP-2718 typed-transaction type byte. `0x00` = legacy /
    /// pre-2718, `0x02` = EIP-1559, `0x7E` = shielded
    /// (privacy-redesign Phase 4). Default zero for backward compat.
    #[serde(default)]
    pub tx_type: u8,
    /// Body of a `0x7E` shielded transaction. `None` for legacy /
    /// EIP-1559 txs. The discriminated union of variants lives in
    /// [`crate::shielded_evm::ShieldedEnvelope`].
    #[serde(default)]
    pub shielded_payload: Option<crate::shielded_evm::ShieldedEnvelope>,
    /// Canonical transaction hash. For txs decoded from standard
    /// Ethereum RLP (`eth_sendRawTransaction`) this is
    /// `keccak256(raw_rlp)` — exactly what MetaMask/ethers compute — so
    /// the wallet can track the tx and `eth_getTransactionByHash` finds
    /// it. `None` for internally-built txs, which fall back to the
    /// field-derived hash.
    #[serde(default)]
    pub hash: Option<B256>,
}

impl Default for Transaction {
    fn default() -> Self {
        Self {
            from: Address::ZERO,
            to: None,
            value: U256::ZERO,
            data: Bytes::new(),
            gas_limit: 0,
            gas_price: U256::ZERO,
            nonce: 0,
            chain_id: None,
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxExecution {
    pub success: bool,
    pub gas_used: u64,
    pub output: Bytes,
    pub created_address: Option<Address>,
    pub logs: Vec<LogEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Bytes,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub success: bool,
    pub gas_used: u64,
    pub output: Bytes,
    pub created_address: Option<Address>,
    pub error: Option<String>,
    pub logs: Vec<LogEntry>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub number: u64,
    pub chain_id: u64,
    pub timestamp: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub base_fee: U256,
    pub coinbase: Address,
    pub hash: B256,
    /// Hash of the parent block (`B256::ZERO` at genesis). Makes the
    /// chain a real hash-linked structure; validated on import.
    #[serde(default)]
    pub parent_hash: B256,
    pub proposer: Address,
    pub finalized: bool,
    pub consensus: Finalization,
    pub unbonded: Vec<Unbonding>,
    pub applied_validator_changes: Vec<ValidatorChange>,
    pub slashes: Vec<Slashing>,
    pub finality_rounds: Vec<RoundResult>,
    pub slashing_evidence: Vec<SlashingEvidence>,
    pub rewards: Vec<Reward>,
    pub total_reward: U256,
    pub burned_reward: U256,
    pub state_root: B256,
    pub transactions: Vec<Transaction>,
    pub receipts: Vec<Receipt>,
    pub bridge_orders_to_evm: Vec<BridgeMessage>,
    pub bridge_evm_to_orders: Vec<BridgeMessage>,
    pub domain_events: Vec<DomainEvent>,
    /// Root of the shielded note commitment tree at end-of-block.
    /// `B256::ZERO` for pre-privacy-fork blocks. Privacy-redesign
    /// Phase 4.
    #[serde(default)]
    pub shielded_state_root: B256,
    /// Root of the nullifier set at end-of-block.
    /// `B256::ZERO` for pre-fork blocks.
    #[serde(default)]
    pub nullifier_root: B256,
    /// Root of public market-aggregate state (clearing prices,
    /// bucketed depth, auction stats). `B256::ZERO` pre-fork.
    #[serde(default)]
    pub shielded_event_root: B256,
    /// SP1 state-transition proof for this block.
    /// `None` until Workstream E delivers the real prover.
    #[serde(default)]
    pub state_proof: Option<crate::zk_proofs::StateTransitionProof>,
    /// Proposer's signature over `(height, hash)` as `(r, s, y_parity)`.
    /// Proves the block was authored by `proposer`; verified on import so a
    /// non-validator cannot inject blocks. `None` only for locally produced
    /// blocks before signing and for genesis.
    #[serde(default)]
    pub proposer_sig: Option<(U256, U256, u64)>,
}

impl Default for Block {
    fn default() -> Self {
        Self {
            number: 0,
            chain_id: 0,
            timestamp: 0,
            gas_limit: 0,
            gas_used: 0,
            base_fee: U256::ZERO,
            coinbase: Address::ZERO,
            hash: B256::ZERO,
            parent_hash: B256::ZERO,
            proposer: Address::ZERO,
            finalized: false,
            consensus: Finalization::default(),
            unbonded: Vec::new(),
            applied_validator_changes: Vec::new(),
            slashes: Vec::new(),
            finality_rounds: Vec::new(),
            slashing_evidence: Vec::new(),
            rewards: Vec::new(),
            total_reward: U256::ZERO,
            burned_reward: U256::ZERO,
            state_root: B256::ZERO,
            transactions: Vec::new(),
            receipts: Vec::new(),
            bridge_orders_to_evm: Vec::new(),
            bridge_evm_to_orders: Vec::new(),
            domain_events: Vec::new(),
            shielded_state_root: B256::ZERO,
            nullifier_root: B256::ZERO,
            shielded_event_root: B256::ZERO,
            state_proof: None,
            proposer_sig: None,
        }
    }
}

#[derive(Debug)]
pub struct EvmEngine {
    pub state: Box<dyn StateBackend>,
    pub db: InMemoryDB,
}

#[derive(Debug)]
pub struct ConsensusEngine {
    pub inner: Consensus,
    pub network: NetworkSim,
    pub hotstuff2: Option<HotStuff2>,
}

#[derive(Debug)]
pub struct OrdersEngine {
    pub state: MersennetOrdersState,
}

#[derive(Debug)]
pub struct BridgeEngine {
    pub orders_to_evm: BridgeQueue,
    pub evm_to_orders: BridgeQueue,
}

impl ConsensusEngine {
    pub fn validators(&self) -> &[Validator] {
        self.inner.validators()
    }

    pub fn total_stake(&self) -> U256 {
        self.inner.total_stake()
    }

    pub fn set_unbonding_period(&mut self, period: u64) {
        self.inner.set_unbonding_period(period);
    }

    pub fn set_round_timeout_ms(&mut self, timeout_ms: u64) {
        self.inner.set_round_timeout_ms(timeout_ms);
    }

    pub fn replace_validators(&mut self, validators: Vec<Validator>) {
        self.inner.replace_validators(validators);
    }

    pub fn set_slashing_bps(&mut self, double_sign_bps: u64, timeout_bps: u64) {
        self.inner.set_slashing_bps(double_sign_bps, timeout_bps);
    }

    pub fn set_slashing_escalation(&mut self, step_bps: u64, max_bps: u64) {
        self.inner.set_slashing_escalation(step_bps, max_bps);
    }

    pub fn set_miss_precommit(&mut self, validator: Address) {
        self.inner.set_miss_precommit(validator);
    }

    pub fn set_miss_prevote(&mut self, validator: Address) {
        self.inner.set_miss_prevote(validator);
    }

    pub fn set_token_economics(
        &mut self,
        max_supply: U256,
        initial_reward_per_block: U256,
        halving_interval: u64,
    ) {
        self.inner
            .set_token_economics(max_supply, initial_reward_per_block, halving_interval);
    }

    pub fn stake(&mut self, address: Address, stake: U256) -> Result<()> {
        self.inner.stake(address, stake)
    }

    pub fn queue_stake(&mut self, address: Address, amount: U256) {
        self.inner.queue_stake(address, amount);
    }

    pub fn queue_unbond(&mut self, address: Address, amount: U256) {
        self.inner.queue_unbond(address, amount);
    }

    pub fn queue_slash(&mut self, address: Address, amount: U256, reason: impl Into<String>) {
        self.inner.queue_slash(address, amount, reason);
    }

    pub fn apply_pending_changes(&mut self, height: u64) -> Vec<ValidatorChange> {
        self.inner.apply_pending_changes(height)
    }

    pub fn run_finality_rounds(
        &mut self,
        block_hash: B256,
        height: u64,
        rounds: u64,
    ) -> (Vec<RoundResult>, Vec<SlashingEvidence>) {
        self.inner
            .run_finality_rounds(block_hash, height, rounds, &mut self.network)
    }

    pub fn slash_amount_for_evidence(
        &self,
        evidence: &SlashingEvidence,
        current_height: u64,
    ) -> U256 {
        self.inner
            .slash_amount_for_evidence(evidence, current_height)
    }

    pub fn record_offense(&mut self, validator: Address) {
        self.inner.record_offense(validator);
    }

    pub fn finalize(&mut self, block_hash: B256, height: u64) -> Finalization {
        self.inner.finalize(block_hash, height)
    }

    pub fn process_unbonding(&mut self, height: u64) -> Vec<Unbonding> {
        self.inner.process_unbonding(height)
    }

    pub fn apply_pending_slashes(&mut self, height: u64) -> Vec<Slashing> {
        self.inner.apply_pending_slashes(height)
    }

    pub fn pending_slash_count(&self) -> usize {
        self.inner.pending_slash_count()
    }

    /// Run a single HotStuff-2 round. Lazily initialises the HotStuff-2 state
    /// machine from the current validator set on first call.
    pub fn run_hotstuff2_round(&mut self, block_hash: B256, height: u64) -> HotStuff2Result {
        let hs = self.hotstuff2.get_or_insert_with(|| {
            let validators = self.inner.validators().to_vec();
            let proposer = validators
                .first()
                .map(|v| v.address)
                .unwrap_or(Address::ZERO);
            HotStuff2::new(proposer, validators)
        });
        hs.run_simulated_round(block_hash, height)
    }
}

/// A live gossip peer as reported by the network layer (address plus how
/// many seconds ago it was first and last heard from). Kept here so the RPC
/// layer can serve `mersennet_peers` without depending on the network crate.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PeerSnapshot {
    pub addr: String,
    pub first_seen_secs: u64,
    pub last_seen_secs: u64,
    /// True when this node has received packets from the peer itself.
    pub heard: bool,
}

/// Everything a block application mutates that consensus depends on — the
/// same set `commit_state` persists — plus the block counters. Cloned before
/// each block at the tip (the live state is a few thousand entries; the 12 GB
/// on disk is sled history).
#[derive(Debug)]
struct StateCheckpoint {
    /// Height this checkpoint precedes: the state *before* applying `height`.
    height: u64,
    evm_db: InMemoryDB,
    orders: MersennetOrdersState,
    bridge_orders_to_evm: crate::bridge::BridgeQueueSnapshot,
    bridge_evm_to_orders: crate::bridge::BridgeQueueSnapshot,
    base_fee: U256,
    /// Reward/participation accounting feeds balances, so it is part of
    /// consensus state.
    consensus: Consensus,
}

/// How many recent heights can be rolled back. Finality normally lands
/// within the same block interval, so 1 would do; 16 covers a slow round.
const REORG_DEPTH: usize = 16;

#[derive(Debug)]
pub struct Engine {
    pub chain_id: u64,
    pub block_number: u64,
    pub base_fee: U256,
    pub coinbase: Address,
    pub gas_limit_per_block: u64,
    pub spec_id: SpecId,
    pub consensus: ConsensusEngine,
    pub fee_max_change_denominator: u64,
    pub fee_elasticity_multiplier: u64,
    pub fee_target_gas: u64,
    pub evm: EvmEngine,
    pub chain: Vec<Block>,
    #[allow(dead_code)]
    pub orders: OrdersEngine,
    #[allow(dead_code)]
    pub bridge: BridgeEngine,
    pub pending_events: Vec<DomainEvent>,
    mempool: Mempool,
    pub fba_engine: FBAEngine,
    pub commit_reveal: CommitRevealPool,
    pub flat_state: FlatState,
    pub market_maker: MarketMakerEngine,
    pub intent_engine: IntentEngine,
    pub encrypted_mempool: EncryptedMempool,
    pub mainnet_guard: MainnetGuard,
    pub aa_bundler: Bundler,
    pub invariant_checker: InvariantChecker,
    pub peer_count: std::sync::atomic::AtomicUsize,
    /// Live gossip peers as of the last discovery tick (for `mersennet_peers`).
    pub peer_list: Vec<PeerSnapshot>,

    // ───── Privacy redesign (Phase 4 wiring) ─────
    /// Master switch — gates `apply_shielded_tx`, the FBA tick, the
    /// SP1 prover hook, and the 0x7E EIP-2718 type-byte path.
    /// `false` until the hard fork activates (see ADR-018).
    pub privacy_mode_activated: bool,
    /// Gates the state-MUTATING transparent `mersennet_orders_*` JSON-RPC
    /// methods (addMarket / submitOrder / cancelOrder / depositCollateral /
    /// setMarginParams / liquidate), which act for an arbitrary `owner` with
    /// no signature. Convenient for testnet seeding (the market-maker bot),
    /// but it lets any caller trade or credit collateral as any address, so it
    /// MUST be `false` on mainnet — orders should arrive only as signed txs to
    /// the CLOB precompile. Read-only `mersennet_orders_*` queries are never
    /// gated by this flag. Defaults to `true` to preserve testnet behavior.
    pub allow_unsigned_orders_rpc: bool,
    /// Activation height for the privacy hard fork, set by genesis
    /// or governance. When `Some(h)` and `block_number >= h`, the
    /// engine auto-flips `privacy_mode_activated` to `true` at the
    /// start of the block-production loop.
    pub privacy_activation_height: Option<u64>,
    /// When set, block production fails closed if the post-fork SP1
    /// state proof cannot be produced.
    pub sp1_proof_required: bool,
    /// Canonical shielded-event root for the current block, computed
    /// in [`Engine::run_shielded_tick`] from the same
    /// `CanonicalShieldedEvent` list the SP1 executor re-derives. The
    /// block header reuses this so the host and zkVM agree on the
    /// `shielded_event_root` byte-for-byte (they must use the same
    /// encoding, not the `DomainEvent` serialization).
    shielded_tick_event_root: B256,
    /// Discrete-time uniform-price auction state per shielded
    /// market.
    pub shielded_orders: ShieldedOrdersEngine,
    /// Sealed-bid liquidation auctions (replaces address-keyed
    /// liquidation searches).
    pub liquidation_auction: LiquidationAuction,
    /// Shielded EVM envelope state: transparent balances, shielded
    /// transfers, shield/unshield bridge. Its `state` field
    /// (`ShieldedState`) is the **canonical** shielded note tree —
    /// every shielded subsystem mutates this single store via
    /// `&mut self.shielded_evm.state` to avoid double-bookkeeping.
    pub shielded_evm: ShieldedEvm,
    /// Threshold-encrypted mempool. Production builds inject a BLS
    /// provider via [`ThresholdMempool::with_provider`].
    pub threshold_mempool: ThresholdMempool,
    /// Disk-backed persistence for shielded subsystems. `None` if the
    /// engine was constructed in pure-memory mode (tests, benches).
    /// Writes happen at end-of-block when `privacy_mode_activated`.
    pub shielded_persistence: Option<ShieldedPersistence>,
    /// Distributed key generation coordinator for the threshold
    /// mempool. Tracks per-epoch ceremony status and produces the
    /// inputs the HotStuff-2 sub-round needs to drive the protocol.
    /// Lives on every node — non-validators observe + audit but do
    /// not contribute shares.
    pub dkg: crate::dkg::DkgCoordinator,
    /// Opt-in public contract code-hash attestations.
    pub code_publication_registry: CodePublicationRegistry,
    /// Buffer of gossiped blocks awaiting in-order application. Blocks
    /// can arrive out of order over UDP gossip; they are applied
    /// strictly by ascending height so every node re-executes the same
    /// sequence and converges on identical state (see `import_block`).
    import_buffer: std::collections::BTreeMap<u64, Block>,
    /// Raw RLP bytes of wallet-submitted (eth_sendRawTransaction) txs,
    /// keyed by canonical hash. Ethereum-format signatures verify against
    /// the EIP-155/typed signing hash, which peers cannot reconstruct from
    /// the parsed fields alone — so the relay attaches the raw envelope and
    /// receivers re-decode it (self-authenticating). In-memory only.
    raw_tx_cache: std::collections::HashMap<B256, Vec<u8>>,
    /// This node's validator address, if it runs as a validator. Set at
    /// boot from the node identity. Used for leader election (only the
    /// elected leader for a height/round produces that block) so
    /// validators no longer all produce competing blocks at the same
    /// height — the root cause of state-root divergence.
    local_validator: Option<Address>,
    /// BFT finality votes: height -> block_hash -> set of voter
    /// addresses. A block is finalized once voters representing >= 2/3
    /// of total stake have voted for it. Populated from signed `Vote`
    /// gossip messages routed in by the network layer.
    finality_votes: std::collections::HashMap<
        u64,
        std::collections::HashMap<B256, std::collections::HashSet<Address>>,
    >,
    /// Heights that reached a 2/3-stake quorum, with the winning hash.
    finalized_heights: std::collections::HashMap<u64, B256>,
    /// Highest block height ever observed from the network (gossip or sync),
    /// whether or not it applied locally. Used to gate block production: a
    /// validator that is behind the network head must NOT produce (it would
    /// fork off a stale height), it must sync first. This is the guard that
    /// stops a restarted/lagging validator from re-forking itself.
    highest_observed_height: u64,
    /// When we last applied a block (produced or imported). Used to tell a
    /// real "catching up" state from a stalled network.
    last_block_applied_at: Option<std::time::Instant>,
    /// When we last rejected a block for a parent-hash mismatch. Diagnostic
    /// only: it cannot tell "I am forked" from "a peer is forked".
    fork_detected_at: Option<std::time::Instant>,
    /// Consensus state captured *before* applying each of the last
    /// `REORG_DEPTH` heights (what `commit_state` persists, plus the block
    /// counters). When finality lands on a different hash than the block we
    /// applied at that height, we roll back to the checkpoint and re-import
    /// the canonical block — fork choice by finality, no external heal.
    checkpoints: std::collections::VecDeque<StateCheckpoint>,
    /// Competing proposals seen for recent heights, so a finalized block we
    /// rejected as "duplicate height" can be applied straight after rollback.
    alt_blocks: std::collections::HashMap<u64, Vec<Block>>,
}

impl Engine {
    pub fn new_with_state(chain_id: u64, path: impl AsRef<std::path::Path>) -> Self {
        Self::new_with_backend(chain_id, path, "sled")
    }

    pub fn new_with_backend(
        chain_id: u64,
        path: impl AsRef<std::path::Path>,
        backend: &str,
    ) -> Self {
        let path_buf = path.as_ref().to_path_buf();
        let state: Box<dyn StateBackend> = match backend {
            "redb" => {
                tracing::info!("initializing redb storage backend (ACID, pure-Rust)");
                Box::new(RedbState::open(&path_buf).expect("redb state DB open"))
            }
            _ => {
                tracing::info!("initializing sled storage backend");
                Box::new(PersistentState::open(&path_buf).expect("sled state DB open"))
            }
        };

        // Open the shielded persistence DB next to the EVM DB. It's
        // always present — pre-fork it stays empty.
        let shielded_persistence = ShieldedPersistence::open(&path_buf)
            .map_err(|e| {
                tracing::warn!(error = ?e, "shielded persistence open failed, continuing without disk-backed shielded state");
                e
            })
            .ok();

        let mut shielded_evm = ShieldedEvm::new();
        if let Some(p) = &shielded_persistence {
            match p.load_shielded_state() {
                Ok(Some(restored)) => {
                    tracing::info!("restored shielded state from disk");
                    shielded_evm.state = restored;
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(error = ?e, "shielded state restore failed");
                }
            }
            match p.load_transparent_balances() {
                Ok(balances) => shielded_evm.transparent_balances = balances,
                Err(e) => tracing::warn!(error = ?e, "transparent-balance restore failed"),
            }
            match p.load_encrypted_note_payloads() {
                Ok(payloads) => shielded_evm.encrypted_note_payloads = payloads,
                Err(e) => tracing::warn!(error = ?e, "encrypted-note-payload restore failed"),
            }
            match p.load_viewing_grants() {
                Ok(grants) => shielded_evm.viewing_grants = grants,
                Err(e) => tracing::warn!(error = ?e, "viewing-grant restore failed"),
            }
            match p.load_viewing_grant_revocations() {
                Ok(revocations) => shielded_evm.viewing_grant_revocations = revocations,
                Err(e) => tracing::warn!(error = ?e, "viewing-grant revocation restore failed"),
            }
        }

        let mut db = InMemoryDB::default();
        state.load_into_db(&mut db).expect("state DB load");
        let mut mersennet_orders = MersennetOrdersState::new();
        state
            .load_mersennet_orders(&mut mersennet_orders)
            .expect("mersennet orders load");
        let mut bridge_orders_to_evm = BridgeQueue::new();
        let mut bridge_evm_to_orders = BridgeQueue::new();
        state
            .load_bridge_queues(&mut bridge_orders_to_evm, &mut bridge_evm_to_orders)
            .expect("bridge queues load");

        // Resume the chain at the persisted height instead of
        // re-producing from genesis. Without this, every restart resets
        // `block_number` to 1 while balances load from disk — an
        // inconsistent state that corrupts the chain and strands nodes
        // that restart after the rest of the fleet has advanced.
        // A commit that was cut off between its writes leaves account state
        // ahead of the recorded height; resuming would fail every import
        // with "nonce too low" and wedge the node. Refuse to start so the
        // watchdog restores the latest snapshot (or resyncs) instead.
        if let Ok(Some(h)) = state.interrupted_commit() {
            tracing::error!(
                height = h,
                "INTERRUPTED COMMIT: the last block commit did not complete; persisted state is inconsistent. \
                 Restore the latest snapshot (mersennet-snapshot restore) or move data/state aside to resync."
            );
            std::process::exit(78);
        }
        let resume_height = state.persisted_height().ok().flatten();
        let (start_block_number, restored_chain) = match resume_height {
            Some(h) if h >= 1 => {
                // Keep a bounded window of recent blocks in memory so
                // `latest_height`/`block_by_number` stay consistent and
                // recent history is queryable; older blocks remain on
                // disk via the state backend.
                let from = h.saturating_sub(1023).max(1);
                let blocks = state.load_blocks_range(from, h).unwrap_or_default();
                tracing::info!(
                    persisted_height = h,
                    next_block = h.saturating_add(1),
                    restored_blocks = blocks.len(),
                    "resuming chain from persisted height"
                );
                // Integrity check for restored state (snapshots, heals): the
                // persisted state's Merkle root must equal the state root the
                // head block commits to. Since 2026-09-13 every producer's
                // root matches a fresh re-execution, so a mismatch means a
                // tampered or corrupted snapshot — or a lineage drift we want
                // to know about. Non-fatal for now (metric + error); made
                // fatal once the fleet has run clean for a while.
                if let Some(head) = blocks.iter().find(|b| b.number == h)
                    && head.state_root != B256::ZERO
                {
                    let computed = state.compute_state_root();
                    if computed != head.state_root {
                        metrics::increment_counter!("mersennet_resume_state_root_mismatch_total");
                        tracing::error!(
                            height = h,
                            computed = %computed,
                            header = %head.state_root,
                            "RESTORED STATE ROOT MISMATCH: persisted state does not match the head block's state root (tampered/corrupted snapshot or lineage drift)"
                        );
                    } else {
                        tracing::info!(height = h, state_root = %computed, "restored state root matches the head block");
                    }
                }
                (h.saturating_add(1), blocks)
            }
            _ => (1, Vec::new()),
        };

        Self {
            chain_id,
            block_number: start_block_number,
            base_fee: U256::from(1),
            coinbase: Address::ZERO,
            gas_limit_per_block: 30_000_000,
            spec_id: SpecId::PRAGUE_EOF,
            consensus: ConsensusEngine {
                inner: Consensus::default(),
                network: NetworkSim::default(),
                hotstuff2: None,
            },
            fee_max_change_denominator: 8,
            fee_elasticity_multiplier: 2,
            fee_target_gas: 15_000_000,
            evm: EvmEngine { state, db },
            chain: restored_chain,
            orders: OrdersEngine {
                state: mersennet_orders,
            },
            bridge: BridgeEngine {
                orders_to_evm: bridge_orders_to_evm,
                evm_to_orders: bridge_evm_to_orders,
            },
            pending_events: Vec::new(),
            mempool: Mempool::with_limits(10_000, 1_000, 1_000),
            fba_engine: FBAEngine::new(100),
            commit_reveal: CommitRevealPool::new(2),
            flat_state: FlatState::new(),
            market_maker: MarketMakerEngine::new(),
            intent_engine: IntentEngine::new(10, U256::ZERO),
            encrypted_mempool: EncryptedMempool::new(2, 3, 10_000),
            mainnet_guard: MainnetGuard::new(chain_id),
            aa_bundler: Bundler::new(10),
            invariant_checker: InvariantChecker::new(),
            peer_count: std::sync::atomic::AtomicUsize::new(0),
            peer_list: Vec::new(),

            // Privacy-redesign Phase 4 — off until the hard fork
            // flips the master switch (ADR-018). All subsystems live
            // in-memory; persistence is wired in Workstream A7.
            privacy_mode_activated: false,
            allow_unsigned_orders_rpc: true,
            privacy_activation_height: None,
            sp1_proof_required: false,
            shielded_tick_event_root: B256::ZERO,
            shielded_orders: ShieldedOrdersEngine::new(),
            liquidation_auction: LiquidationAuction::new(),
            shielded_evm,
            // 2-of-3 threshold by default; node operators override
            // the provider when joining a real committee (Workstream
            // D3).
            threshold_mempool: ThresholdMempool::new(2, 3, 10_000),
            shielded_persistence,
            // DKG coordinator with the production-default epoch
            // length. Operators override at boot via
            // `set_dkg_epoch_length` to match their chain's
            // block-time × desired-rotation cadence.
            dkg: crate::dkg::DkgCoordinator::new(crate::dkg::DEFAULT_EPOCH_LENGTH_BLOCKS),
            code_publication_registry: CodePublicationRegistry::default(),
            import_buffer: std::collections::BTreeMap::new(),
            highest_observed_height: 0,
            last_block_applied_at: None,
            fork_detected_at: None,
            checkpoints: std::collections::VecDeque::new(),
            alt_blocks: std::collections::HashMap::new(),
            raw_tx_cache: std::collections::HashMap::new(),
            local_validator: None,
            finality_votes: std::collections::HashMap::new(),
            finalized_heights: std::collections::HashMap::new(),
        }
    }

    /// Flip the privacy hard-fork switch. Called at the activation
    /// block height (see ADR-018) after the node has synced past the
    /// genesis-of-privacy snapshot. Once `true`, the engine accepts
    /// `tx_type = 0x7E` transactions and runs the shielded subsystems
    /// inside `execute_block`.
    pub fn activate_privacy_mode(&mut self) {
        self.privacy_mode_activated = true;
        tracing::warn!(
            block = self.block_number,
            "Mersennet privacy hard fork activated — shielded tx type 0x7E now accepted"
        );
    }

    /// Block-level convenience: are shielded paths live?
    pub fn privacy_mode_activated(&self) -> bool {
        self.privacy_mode_activated
    }

    /// Enable/disable the unsigned state-mutating `mersennet_orders_*` RPC
    /// methods. Set from `mersennet_orders.allow_unsigned_orders_rpc` at
    /// startup; should be `false` on mainnet.
    pub fn set_allow_unsigned_orders_rpc(&mut self, allow: bool) {
        self.allow_unsigned_orders_rpc = allow;
    }

    /// Are unsigned, owner-spoofable `mersennet_orders_*` mutations allowed?
    pub fn allow_unsigned_orders_rpc(&self) -> bool {
        self.allow_unsigned_orders_rpc
    }

    // ───── BFT leader election + finality (Workstream A) ─────

    /// Register this node's validator address. Only set on validators.
    pub fn set_local_validator(&mut self, address: Address) {
        self.local_validator = Some(address);
    }

    pub fn local_validator(&self) -> Option<Address> {
        self.local_validator
    }

    /// The deterministic set of validator addresses, sorted so every
    /// node agrees on ordering regardless of insertion order.
    pub fn validator_addresses(&self) -> Vec<Address> {
        let mut v: Vec<Address> = self
            .consensus
            .validators()
            .iter()
            .map(|val| val.address)
            .collect();
        v.sort();
        v
    }

    // ─── Open validator set ────────────────────────────────────────────────

    pub fn set_validator_set_params(&mut self, params: crate::staking::ValidatorSetParams) {
        self.orders.state.staking.set_params(params);
    }

    /// Startup. The open set replaces the genesis set at every epoch boundary,
    /// but a fresh process starts from the genesis validators in its config.
    /// Reinstall the set that was active when the node stopped, otherwise a
    /// restarted validator computes a different leader schedule and reward
    /// split than the rest of the network until the next boundary (this is
    /// what happened on 2026-09-15 when the fleet was rolled with a fifth
    /// validator active). Stakes come from the node-local cache written at
    /// the transition; if the cache is missing or names a different set,
    /// fall back to the registry's current voting stakes.
    pub fn restore_consensus_set(&mut self) {
        let height = self.latest_height();
        let (members, fallback): (Vec<Address>, Vec<crate::consensus::Validator>) = {
            let st = &self.orders.state.staking;
            if !st.is_open_set_active(height) || st.active_set.is_empty() {
                return;
            }
            let members = st.active_set.clone();
            let fallback = members
                .iter()
                .map(|id| crate::consensus::Validator {
                    address: *id,
                    stake: st.voting_stake(*id),
                })
                .collect();
            (members, fallback)
        };
        let cached = self
            .evm
            .state
            .load_consensus_set()
            .ok()
            .flatten()
            .filter(|c| {
                let mut a: Vec<Address> = c.iter().map(|(x, _)| *x).collect();
                let mut m = members.clone();
                a.sort();
                m.sort();
                a == m
            });
        let from_cache = cached.is_some();
        let set: Vec<crate::consensus::Validator> = match cached {
            Some(c) => c
                .into_iter()
                .map(|(address, stake)| crate::consensus::Validator { address, stake })
                .collect(),
            None => fallback,
        };
        let active = set.len();
        self.consensus.replace_validators(set);
        tracing::info!(
            height,
            active,
            from_cache,
            "consensus set restored from the persisted active set"
        );
    }

    /// Runs at the first block of every epoch once the open set is active
    /// (and once at the activation height): judges the past epoch, then
    /// recomputes the active set from the registry and installs it as the
    /// consensus validator set. Identical on producer and importer.
    fn maybe_epoch_transition(&mut self, height: u64) {
        let st = &self.orders.state.staking;
        if !st.is_open_set_active(height) {
            return;
        }
        let activation = st.params.activation_height;
        let epoch_blocks = st.params.epoch_blocks.max(1);
        let at_boundary = height == activation || height.is_multiple_of(epoch_blocks);
        if !at_boundary {
            return;
        }
        // Idempotency: the transition for this epoch already ran.
        let epoch = height / epoch_blocks;
        if !st.active_set.is_empty() && st.current_epoch == epoch && height != activation {
            return;
        }
        if height == activation && st.current_epoch == epoch && !st.active_set.is_empty() {
            return;
        }
        if st.registry.is_empty() {
            // First activation: the genesis validators become registry
            // entries with their consensus stake as self-stake.
            let genesis: Vec<(Address, U256)> = self
                .consensus
                .validators()
                .iter()
                .map(|v| (v.address, v.stake))
                .collect();
            self.orders.state.staking.seed_genesis(&genesis, height);
        }
        let transition = self.orders.state.staking.epoch_transition(height);
        let new_set: Vec<crate::consensus::Validator> = transition
            .active_set
            .iter()
            .map(|id| crate::consensus::Validator {
                address: *id,
                stake: self.orders.state.staking.voting_stake(*id),
            })
            .collect();
        if !new_set.is_empty() {
            let snapshot: Vec<(Address, U256)> =
                new_set.iter().map(|v| (v.address, v.stake)).collect();
            self.consensus.replace_validators(new_set);
            if let Err(err) = self.evm.state.save_consensus_set(&snapshot) {
                tracing::warn!(%err, "could not cache the consensus set");
            }
        }
        metrics::increment_counter!("mersennet_epoch_transitions_total");
        tracing::info!(
            height,
            epoch = transition.epoch,
            active = transition.active_set.len(),
            jailed = transition.jailed.len(),
            removed = transition.removed.len(),
            rotated = transition.rotated.len(),
            "epoch transition: validator set recomputed"
        );
    }

    /// Leader-slot accounting for jailing: the proposer took height `height`
    /// at the smallest round where it leads; every leader for an earlier
    /// round missed its slot. Deterministic from (height, proposer, set).
    fn record_leader_slots(&mut self, height: u64, proposer: Address) {
        if !self.orders.state.staking.is_open_set_active(height) {
            return;
        }
        let mut missed = Vec::new();
        for round in 0..MAX_LEADER_ROUND_WINDOW {
            match self.leader_for_height(height, round) {
                Some(l) if l == proposer => break,
                Some(l) => missed.push(l),
                None => return,
            }
        }
        self.orders.state.staking.record_slots(proposer, &missed);
    }

    /// Deterministic leader for a given block height and failover round.
    /// Stake is equal per genesis validator, so a simple stake-agnostic
    /// rotation over the sorted set is fair and unambiguous. `round`
    /// advances only when the current leader fails to produce in time,
    /// rotating to the next validator so a dead leader cannot halt the
    /// chain.
    pub fn leader_for_height(&self, height: u64, round: u64) -> Option<Address> {
        let vs = self.leader_rotation(height);
        if vs.is_empty() {
            return None;
        }
        let idx = (height.wrapping_add(round) % vs.len() as u64) as usize;
        Some(vs[idx])
    }

    /// The active set minus validators benched for the rest of the epoch
    /// (`bench_height` reached, >= 3 leader slots missed and misses at least a
    /// tenth of what they proposed). Derived from the registry counters every
    /// node maintains from block data, so every node computes the same
    /// rotation; never empties the rotation.
    fn leader_rotation(&self, height: u64) -> Vec<Address> {
        let vs = self.validator_addresses();
        let st = &self.orders.state.staking;
        let bh = st.params.bench_height;
        if bh == 0 || height < bh || vs.len() <= 1 {
            return vs;
        }
        let kept: Vec<Address> = vs.iter().copied().filter(|a| !st.is_benched(*a)).collect();
        if kept.is_empty() { vs } else { kept }
    }

    /// Validators currently out of the leader rotation (RPC/UI).
    pub fn benched_validators(&self, height: u64) -> Vec<Address> {
        let st = &self.orders.state.staking;
        if st.params.bench_height == 0 || height < st.params.bench_height {
            return Vec::new();
        }
        self.validator_addresses()
            .into_iter()
            .filter(|a| st.is_benched(*a))
            .collect()
    }

    /// Is this node the elected leader for `(height, round)`?
    pub fn is_leader(&self, height: u64, round: u64) -> bool {
        match (self.local_validator, self.leader_for_height(height, round)) {
            (Some(me), Some(leader)) => me == leader,
            _ => false,
        }
    }

    fn finality_threshold(&self) -> U256 {
        let total = self.consensus.total_stake();
        if total.is_zero() {
            return U256::ZERO;
        }
        total
            .saturating_mul(U256::from(2u64))
            .checked_div(U256::from(3u64))
            .unwrap_or(U256::ZERO)
            .saturating_add(U256::from(1u64))
    }

    fn stake_of(&self, addr: Address) -> U256 {
        self.consensus
            .validators()
            .iter()
            .find(|v| v.address == addr)
            .map(|v| v.stake)
            .unwrap_or(U256::ZERO)
    }

    /// Record a finality vote for `(height, block_hash)` from `voter`.
    /// Returns `true` when this vote pushes the block to a >= 2/3-stake
    /// quorum (i.e. the height just finalized). Votes from non-validators
    /// are ignored by the caller (which verifies the signature first).
    pub fn record_finality_vote(&mut self, height: u64, block_hash: B256, voter: Address) -> bool {
        // Only count known validators.
        if self.stake_of(voter).is_zero() {
            return false;
        }
        if self.finalized_heights.contains_key(&height) {
            return false;
        }
        // Conflicting votes: a validator that already voted for a DIFFERENT
        // hash at this height. Two honest causes exist in this protocol —
        // a failover round after the round-0 proposal timed out, and a node
        // that healed onto the canonical branch after voting on a fork — so a
        // second vote is not proof of a safety fault. Never count both toward
        // a quorum: a vote that agrees with the block we applied supersedes a
        // stale one; otherwise the first vote stands and the new one is
        // dropped. Nothing is slashed here: evidence observed from gossip is
        // not the same on every node, and applying it locally would change the
        // consensus set non-deterministically (that is what forked the fleet
        // after each "equivocation" this weekend). Slashing for double-signing
        // only happens through evidence carried in blocks.
        let local_hash = self.block_by_number(height).map(|b| b.hash);
        let mut stale_hashes: Vec<B256> = Vec::new();
        if let Some(per_hash) = self.finality_votes.get(&height) {
            stale_hashes = per_hash
                .iter()
                .filter(|(h, voters)| **h != block_hash && voters.contains(&voter))
                .map(|(h, _)| *h)
                .collect();
        }
        if !stale_hashes.is_empty() {
            metrics::increment_counter!("mersennet_bft_conflicting_vote_total");
            let agrees_with_local = local_hash == Some(block_hash);
            tracing::warn!(
                height,
                validator = %voter,
                new_hash = %block_hash,
                previous_hash = %stale_hashes[0],
                supersedes = agrees_with_local,
                "conflicting vote at one height (failover round or healed node); not slashing"
            );
            if !agrees_with_local {
                return false;
            }
            if let Some(per_hash) = self.finality_votes.get_mut(&height) {
                for h in &stale_hashes {
                    if let Some(voters) = per_hash.get_mut(h) {
                        voters.remove(&voter);
                    }
                }
            }
        }
        let voters_snapshot: Vec<Address> = {
            let per_hash = self.finality_votes.entry(height).or_default();
            let voters = per_hash.entry(block_hash).or_default();
            voters.insert(voter);
            voters.iter().copied().collect()
        };

        let voted_stake = voters_snapshot
            .iter()
            .fold(U256::ZERO, |acc, a| acc.saturating_add(self.stake_of(*a)));

        let threshold = self.finality_threshold();
        if !threshold.is_zero() && voted_stake >= threshold {
            self.finalized_heights.insert(height, block_hash);
            // Fork choice: if we applied a different block at this height,
            // the network has overruled us — roll back and follow.
            if let Some(local) = self.block_by_number(height).map(|b| b.hash)
                && local != block_hash
            {
                self.reorg_to_finalized(height, block_hash);
            }
            // Bound memory: drop vote-tracking for heights well behind.
            let cutoff = height.saturating_sub(256);
            self.finality_votes.retain(|h, _| *h >= cutoff);
            self.finalized_heights.retain(|h, _| *h >= cutoff);
            metrics::increment_counter!("mersennet_bft_finalized_total");
            tracing::info!(height, block_hash = %block_hash, "block finalized by 2/3 stake quorum");
            return true;
        }
        false
    }

    /// Has `height` reached a 2/3-stake finality quorum?
    pub fn is_height_finalized(&self, height: u64) -> bool {
        self.finalized_heights.contains_key(&height)
    }

    /// The finalized block hash for `height`, if any.
    pub fn finalized_hash(&self, height: u64) -> Option<B256> {
        self.finalized_heights.get(&height).copied()
    }

    /// Set the scheduled activation height for the privacy hard
    /// fork. Genesis migration and governance both call this. Once
    /// `block_number >= height`, the engine auto-flips the master
    /// switch on the next `execute_block`.
    pub fn set_privacy_activation_height(&mut self, height: u64) {
        self.privacy_activation_height = Some(height);
        tracing::info!(
            activation_height = height,
            "privacy activation height scheduled"
        );
    }

    pub fn set_sp1_proof_required(&mut self, required: bool) {
        self.sp1_proof_required = required;
    }

    /// Configure the DKG epoch length, in blocks. The privacy
    /// testnet ships with a 1800-block (~6 min @ 200 ms blocks)
    /// epoch for faster rotation testing; mainnet uses
    /// [`crate::dkg::DEFAULT_EPOCH_LENGTH_BLOCKS`].
    pub fn set_dkg_epoch_length(&mut self, blocks: u64) {
        self.dkg.epoch_length_blocks = blocks;
        tracing::info!(epoch_length_blocks = blocks, "DKG epoch length configured");
    }

    /// Internal: called from the block-production loop to honour the
    /// scheduled activation height. Returns `true` iff the master
    /// switch was just flipped on this call.
    pub(crate) fn auto_activate_privacy_if_scheduled(&mut self) -> bool {
        if self.privacy_mode_activated {
            return false;
        }
        if let Some(h) = self.privacy_activation_height
            && self.block_number >= h
        {
            self.activate_privacy_mode();
            self.sp1_proof_required = true;
            return true;
        }
        false
    }

    pub fn latest_height(&self) -> u64 {
        self.chain.last().map(|block| block.number).unwrap_or(0)
    }

    /// Highest block height seen from the network (may be ahead of our applied
    /// head if we are syncing or wedged).
    pub fn highest_observed_height(&self) -> u64 {
        self.highest_observed_height
    }

    /// True while this node has recently rejected blocks for a parent-hash
    /// mismatch. Diagnostic only (see `fork_detected_at`).
    pub fn is_forked(&self) -> bool {
        self.fork_detected_at
            .map(|t| t.elapsed().as_secs() < 120)
            .unwrap_or(false)
    }

    /// Capture the pre-application state for `height` (the block about to be
    /// produced or imported). Only at the tip: a node replaying history has
    /// finalized blocks and nothing to roll back.
    fn checkpoint_before(&mut self, height: u64) {
        // Drop any checkpoint at or above this height (re-application after
        // a rollback, or a stale entry) and anything too deep.
        self.checkpoints.retain(|c| c.height < height);
        while self.checkpoints.len() >= REORG_DEPTH {
            self.checkpoints.pop_front();
        }
        self.checkpoints.push_back(StateCheckpoint {
            height,
            evm_db: self.evm.db.clone(),
            orders: self.orders.state.clone(),
            bridge_orders_to_evm: self.bridge.orders_to_evm.snapshot(),
            bridge_evm_to_orders: self.bridge.evm_to_orders.snapshot(),
            base_fee: self.base_fee,
            consensus: self.consensus.inner.clone(),
        });
        self.alt_blocks
            .retain(|h, _| *h + (REORG_DEPTH as u64) >= height);
    }

    /// Remember a competing proposal for a height we already hold, so it can
    /// be applied immediately if finality picks it over ours.
    fn remember_alt_block(&mut self, block: Block) {
        let entry = self.alt_blocks.entry(block.number).or_default();
        if entry.len() < 4 && !entry.iter().any(|b| b.hash == block.hash) {
            entry.push(block);
        }
    }

    /// Finality chose `finalized_hash` at `height` but we applied a different
    /// block there: roll back to the checkpoint taken before `height`, drop
    /// the orphaned blocks, and re-apply the canonical block if we have it
    /// (block-sync backfills otherwise). Returns false if the fork is deeper
    /// than our checkpoints — the watchdog's snapshot heal covers that.
    fn reorg_to_finalized(&mut self, height: u64, finalized_hash: B256) -> bool {
        let Some(pos) = self.checkpoints.iter().position(|c| c.height == height) else {
            metrics::increment_counter!("mersennet_reorg_too_deep_total");
            tracing::error!(
                height,
                %finalized_hash,
                depth = REORG_DEPTH,
                "FORK DETECTED beyond reorg depth: finality conflicts with a block we can no longer roll back; state resync needed"
            );
            return false;
        };
        let cp = self.checkpoints.remove(pos).expect("position exists");
        let orphaned: Vec<Block> = self
            .chain
            .iter()
            .filter(|b| b.number >= height)
            .cloned()
            .collect();
        let local_hash = orphaned.first().map(|b| b.hash).unwrap_or(B256::ZERO);

        // Restore consensus state to just before `height`.
        self.evm.db = cp.evm_db;
        self.orders.state = cp.orders;
        self.bridge.orders_to_evm.restore(cp.bridge_orders_to_evm);
        self.bridge.evm_to_orders.restore(cp.bridge_evm_to_orders);
        self.base_fee = cp.base_fee;
        self.consensus.inner = cp.consensus;
        self.chain.retain(|b| b.number < height);
        self.block_number = height;
        self.checkpoints.retain(|c| c.height < height);
        // Persist the rolled-back image so a crash here resumes at height-1
        // instead of on the orphaned branch (the orphan's block record is
        // overwritten when the canonical block at `height` is stored).
        let prev = height.saturating_sub(1);
        if let Err(err) = self
            .evm
            .state
            .begin_commit(prev)
            .and_then(|_| {
                self.evm.state.commit_state(
                    &self.evm.db,
                    &self.orders.state,
                    &self.bridge.orders_to_evm,
                    &self.bridge.evm_to_orders,
                    prev,
                )
            })
            .and_then(|_| self.evm.state.end_commit())
        {
            tracing::error!(%err, "reorg: failed to persist rolled-back state");
        }
        // Flat (RPC read) cache mirrors the committed EVM state; rebuild it.
        self.rebuild_flat_state(prev);
        // Give the orphaned blocks' transactions back to the mempool so they
        // are not lost if the canonical block did not include them.
        for b in &orphaned {
            for tx in &b.transactions {
                // Re-validated against the rolled-back state; duplicates of
                // txs the canonical block already includes are rejected on
                // nonce and simply dropped.
                let _ = self.submit_tx(tx.clone());
            }
        }
        self.import_buffer.retain(|h, _| *h >= height);
        self.fork_detected_at = None;
        metrics::increment_counter!("mersennet_reorg_total");
        tracing::warn!(
            height,
            %local_hash,
            %finalized_hash,
            rolled_back = orphaned.len(),
            "REORG: finality chose a different block at this height; rolled back to the checkpoint and following the canonical chain"
        );

        // Apply the finalized block right away if we saw it as a competitor.
        if let Some(alts) = self.alt_blocks.remove(&height)
            && let Some(canon) = alts.into_iter().find(|b| b.hash == finalized_hash)
        {
            self.import_block(canon);
        }
        true
    }

    fn rebuild_flat_state(&mut self, height: u64) {
        let mut account_changes = Vec::new();
        for (addr, info) in self.evm.db.accounts.iter() {
            account_changes.push((
                *addr,
                FlatAccount {
                    balance: info.info.balance,
                    nonce: info.info.nonce,
                    code_hash: info.info.code_hash,
                    storage_root: B256::ZERO,
                },
            ));
        }
        let changeset = StateChangeset {
            account_changes,
            storage_changes: Vec::new(),
            code_changes: Vec::new(),
        };
        let root = self.evm.state.compute_state_root();
        let _ = self.flat_state.commit_block(height, root, changeset);
    }

    /// Seconds since this node last applied a block, if it ever did.
    pub fn secs_since_last_applied(&self) -> Option<u64> {
        self.last_block_applied_at.map(|t| t.elapsed().as_secs())
    }

    /// True if the network head is ahead of our applied head — i.e. we are
    /// behind (syncing) or wedged on a fork. A validator in this state must
    /// not produce: its next block would build on a stale head and fork the
    /// chain (the restarted-validator re-fork failure mode). It should sync
    /// to the canonical head first. `slack` tolerates the normal 1-block race
    /// where we are the elected proposer for `head+1` and haven't produced yet.
    pub fn is_behind_network(&self, slack: u64) -> bool {
        if self.highest_observed_height <= self.latest_height().saturating_add(slack) {
            return false;
        }
        // "Behind" is only meaningful while blocks keep arriving. The
        // high-water mark never decreases, so a minority fork advertising a
        // higher height (validator 2 building alone after a split, Sep 12)
        // would otherwise convince every honest validator it was behind and
        // stop all production for good — a permanent halt with no reorg to
        // resolve it. If nothing has been applied for a few round timeouts,
        // the network is stalled, not ahead of us: allow production.
        // > 2 leader-timeout rounds (the producer rotates every ~19s).
        const STALL_SECS: u64 = 45;
        match self.secs_since_last_applied() {
            Some(secs) if secs > STALL_SECS => {
                metrics::increment_counter!("mersennet_behind_override_stalled_total");
                false
            }
            _ => true,
        }
    }

    pub fn block_by_number(&self, number: u64) -> Option<&Block> {
        self.chain.iter().find(|block| block.number == number)
    }

    /// Fetch a block by height from the in-memory window, falling back to
    /// the on-disk store for older heights. Returns an owned `Block`.
    ///
    /// The in-memory `chain` is bounded to a recent window (see
    /// `trim_chain_window`) to cap memory, so historical lookups — block
    /// explorers, and crucially peer block-sync — must read from disk.
    /// Without this, a node more than a window behind could never be
    /// served the missing range and would strand.
    pub fn get_block(&self, number: u64) -> Option<Block> {
        if let Some(b) = self.chain.iter().find(|block| block.number == number) {
            return Some(b.clone());
        }
        self.evm.state.load_block(number).ok().flatten()
    }

    /// Cap the in-memory chain to a recent window. Older blocks remain on
    /// disk (served via `get_block`). Prevents the unbounded RAM growth
    /// that otherwise accumulates one `Block` per height for the life of
    /// the process.
    fn trim_chain_window(&mut self) {
        const IN_MEMORY_CHAIN_WINDOW: usize = 2048;
        if self.chain.len() > IN_MEMORY_CHAIN_WINDOW {
            let excess = self.chain.len() - IN_MEMORY_CHAIN_WINDOW;
            self.chain.drain(0..excess);
        }
    }

    pub fn set_fee_market_params(
        &mut self,
        gas_limit_per_block: u64,
        elasticity_multiplier: u64,
        max_change_denominator: u64,
    ) {
        self.gas_limit_per_block = gas_limit_per_block;
        self.fee_elasticity_multiplier = elasticity_multiplier.max(1);
        self.fee_max_change_denominator = max_change_denominator.max(1);
        self.fee_target_gas = gas_limit_per_block / self.fee_elasticity_multiplier;
    }

    pub fn set_unbonding_period(&mut self, period: u64) {
        self.consensus.set_unbonding_period(period);
    }

    pub fn set_round_timeout_ms(&mut self, timeout_ms: u64) {
        self.consensus.set_round_timeout_ms(timeout_ms);
    }

    pub fn set_slashing_bps(&mut self, double_sign_bps: u64, timeout_bps: u64) {
        self.consensus
            .set_slashing_bps(double_sign_bps, timeout_bps);
    }

    pub fn set_slashing_escalation(&mut self, step_bps: u64, max_bps: u64) {
        self.consensus.set_slashing_escalation(step_bps, max_bps);
    }

    pub fn set_miss_precommit(&mut self, validator: Address) {
        self.consensus.set_miss_precommit(validator);
    }

    pub fn set_token_economics(
        &mut self,
        max_supply: U256,
        initial_reward_per_block: U256,
        halving_interval: u64,
    ) {
        self.consensus
            .set_token_economics(max_supply, initial_reward_per_block, halving_interval);
    }

    pub fn set_mempool_limits(&mut self, max_total: usize, max_per_sender: usize, bump_bps: u64) {
        self.mempool
            .update_limits(max_total, max_per_sender, bump_bps);
    }

    pub fn set_bridge_limits(&mut self, max_queue_len: Option<usize>) {
        self.bridge.orders_to_evm.set_max_len(max_queue_len);
        self.bridge.evm_to_orders.set_max_len(max_queue_len);
    }

    fn record_event(&mut self, event: DomainEvent) {
        if self.privacy_mode_activated && !event.is_privacy_safe_after_activation() {
            return;
        }
        self.pending_events.push(event);
    }

    pub fn domain_events_in_range(
        &self,
        from_block: u64,
        to_block: u64,
        domain: Option<&str>,
        kind: Option<&str>,
    ) -> Vec<DomainEventRecord> {
        let mut records = Vec::new();
        for block in &self.chain {
            if block.number < from_block || block.number > to_block {
                continue;
            }
            for (index, event) in block.domain_events.iter().enumerate() {
                if let Some(domain) = domain
                    && event.domain() != domain
                {
                    continue;
                }
                if let Some(kind) = kind
                    && event.kind() != kind
                {
                    continue;
                }
                records.push(DomainEventRecord {
                    block_number: block.number,
                    event_index: index as u64,
                    event: event.clone(),
                });
            }
        }
        records
    }

    pub fn mersennet_orders_add_market(
        &mut self,
        symbol: impl Into<String>,
        tick_size: U256,
        lot_size: U256,
    ) -> MarketId {
        let symbol = symbol.into();
        let market_id = self
            .orders
            .state
            .add_market(symbol.clone(), tick_size, lot_size);
        self.record_event(DomainEvent::MersennetOrders(
            MersennetOrdersEvent::MarketAdded {
                market_id,
                symbol,
                tick_size,
                lot_size,
            },
        ));
        market_id
    }

    pub fn mersennet_orders_submit_order(
        &mut self,
        owner: Address,
        market: MarketId,
        side: Side,
        price: U256,
        size: U256,
        tif: TimeInForce,
    ) -> Result<OrderOutcome, MersennetOrdersError> {
        let outcome = self
            .orders
            .state
            .submit_order(owner, market, side, price, size, tif)?;
        self.record_event(DomainEvent::MersennetOrders(
            MersennetOrdersEvent::OrderSubmitted {
                order_id: outcome.order_id,
                owner,
                market_id: market,
                side,
                price,
                size,
                tif,
                filled: outcome.filled,
                remaining: outcome.remaining,
            },
        ));
        for trade in &outcome.trades {
            self.record_event(DomainEvent::MersennetOrders(MersennetOrdersEvent::Trade {
                taker: trade.taker,
                maker: trade.maker,
                market_id: trade.market,
                side: trade.side,
                price: trade.price,
                size: trade.size,
            }));
        }
        Ok(outcome)
    }

    pub fn mersennet_orders_cancel_order(&mut self, order_id: OrderId) -> Option<Order> {
        let order = self.orders.state.cancel_order(order_id);
        if let Some(order) = &order {
            self.record_event(DomainEvent::MersennetOrders(
                MersennetOrdersEvent::OrderCancelled {
                    order_id: order.id,
                    owner: order.owner,
                    market_id: order.market,
                },
            ));
        }
        order
    }

    #[allow(dead_code)]
    pub fn mersennet_orders_set_margin_params(&mut self, initial_bps: u64, maintenance_bps: u64) {
        self.orders
            .state
            .set_margin_params(initial_bps, maintenance_bps);
        self.record_event(DomainEvent::MersennetOrders(
            MersennetOrdersEvent::MarginParamsUpdated {
                initial_bps,
                maintenance_bps,
            },
        ));
    }

    #[allow(dead_code)]
    pub fn mersennet_orders_deposit_collateral(&mut self, owner: Address, amount: U256) {
        self.orders.state.deposit_collateral(owner, amount);
        self.record_event(DomainEvent::MersennetOrders(
            MersennetOrdersEvent::CollateralDeposited { owner, amount },
        ));
    }

    /// The next nonce to assign a server-submitted CLOB transaction for
    /// `owner`: one past the highest of (account nonce, highest queued
    /// mempool nonce). Keeps rapid-fire order txs from colliding.
    pub fn next_clob_nonce(&mut self, owner: Address) -> u64 {
        let account_nonce = self.get_account_nonce(owner).unwrap_or(0);
        // Fill the lowest free nonce from the account nonce upward. If a
        // prior CLOB tx was dropped (mempool full / gossip loss), always
        // incrementing past the highest queued nonce would leave a
        // permanent gap at account_nonce that stalls every later tx (none
        // can become "ready"), which starved all but the first market.
        self.mempool.next_free_nonce(owner, account_nonce)
    }

    /// Route a CLOB operation through consensus: build an unsigned
    /// precompile-call transaction (to 0x…0100) and submit it to the
    /// mempool. It gossips to the leader, is included in a block, and
    /// executes via the precompile deterministically on every node.
    ///
    /// This replaces the direct-state-mutation `mersennet_orders_*` RPC
    /// path, which mutated only the receiving node's local state — under
    /// single-leader BFT the leader never saw it and the CLOB diverged.
    /// The caller must be funded for gas + collateral. `value` is always
    /// zero (the precompile is non-payable).
    pub fn submit_orders_call(
        &mut self,
        owner: Address,
        calldata: Vec<u8>,
        precompile_gas: u64,
    ) -> Result<B256, TxRejection> {
        let nonce = self.next_clob_nonce(owner);
        // The EVM charges intrinsic gas (21k + calldata) before the
        // precompile runs, so the tx gas limit must cover intrinsic +
        // the precompile's internal gas requirement + headroom, or the
        // precompile OOGs and the order silently reverts.
        let gas_limit =
            21_000 + calldata.len() as u64 * 16 + precompile_gas.saturating_mul(2) + 30_000;
        let tx = Transaction {
            from: owner,
            to: Some(crate::precompile_abi::MERSENNET_ORDERS_PRECOMPILE),
            value: U256::ZERO,
            data: revm::primitives::Bytes::from(calldata),
            gas_limit,
            gas_price: self.base_fee.max(U256::from(1)),
            nonce,
            chain_id: Some(self.chain_id),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
        };
        // Use the same hash the tx index/lookup will compute, so the
        // returned txHash resolves via eth_getTransactionByHash.
        let hash = crate::crypto::tx_signing_hash(&tx);
        self.submit_tx_unsigned(tx)?;
        Ok(hash)
    }

    #[allow(dead_code)]
    pub fn mersennet_orders_is_liquidatable(&self, owner: Address) -> bool {
        self.orders.state.is_liquidatable(owner)
    }

    #[allow(dead_code)]
    pub fn mersennet_orders_liquidate(&mut self, owner: Address) -> bool {
        let liquidated = self.orders.state.liquidate(owner);
        self.record_event(DomainEvent::MersennetOrders(
            MersennetOrdersEvent::Liquidation { owner, liquidated },
        ));
        liquidated
    }

    pub fn mersennet_orders_order_book(&self, market: MarketId) -> Option<OrderBookView> {
        self.orders.state.order_book(market)
    }

    pub fn mersennet_orders_open_orders(&self, owner: Address) -> Vec<Order> {
        self.orders.state.open_orders(owner)
    }

    #[allow(dead_code)]
    pub fn bridge_enqueue_orders_to_evm(&mut self, payload: Bytes) -> BridgeMessage {
        let msg = self.bridge.orders_to_evm.push(
            BridgeDomain::MersennetOrders,
            BridgeDomain::MersennetEvm,
            payload,
        );
        self.record_event(DomainEvent::Bridge(BridgeEvent::Enqueued {
            queue: BridgeQueueKind::OrdersToEvm,
            message: msg.clone(),
        }));
        msg
    }

    #[allow(dead_code)]
    pub fn bridge_enqueue_evm_to_orders(&mut self, payload: Bytes) -> BridgeMessage {
        let msg = self.bridge.evm_to_orders.push(
            BridgeDomain::MersennetEvm,
            BridgeDomain::MersennetOrders,
            payload,
        );
        self.record_event(DomainEvent::Bridge(BridgeEvent::Enqueued {
            queue: BridgeQueueKind::EvmToOrders,
            message: msg.clone(),
        }));
        msg
    }

    #[allow(dead_code)]
    pub fn bridge_dequeue_orders_to_evm(&mut self) -> Option<BridgeMessage> {
        let msg = self.bridge.orders_to_evm.pop();
        if let Some(msg) = &msg {
            self.record_event(DomainEvent::Bridge(BridgeEvent::Dequeued {
                queue: BridgeQueueKind::OrdersToEvm,
                message: msg.clone(),
            }));
        }
        msg
    }

    #[allow(dead_code)]
    pub fn bridge_dequeue_evm_to_orders(&mut self) -> Option<BridgeMessage> {
        let msg = self.bridge.evm_to_orders.pop();
        if let Some(msg) = &msg {
            self.record_event(DomainEvent::Bridge(BridgeEvent::Dequeued {
                queue: BridgeQueueKind::EvmToOrders,
                message: msg.clone(),
            }));
        }
        msg
    }

    pub fn add_validator(&mut self, address: Address, stake: U256) -> Result<()> {
        // Every consensus validator gets a delegation pool so third parties
        // can stake to it via the 0x…0400 precompile.
        self.orders.state.staking.ensure_pool(address);
        self.consensus.stake(address, stake)
    }

    /// Register a non-native collateral asset (genesis / admin path).
    pub fn register_collateral_asset(
        &mut self,
        token: Address,
        weight_bps: u64,
        value_num: U256,
        value_den: U256,
        balances_slot: U256,
    ) {
        self.orders.state.register_collateral_asset(
            token,
            crate::mersennet_orders::CollateralAsset {
                weight_bps,
                value_num,
                value_den,
                balances_slot,
            },
        );
    }

    pub fn stake_validator(&mut self, address: Address, amount: U256) -> Result<()> {
        self.consensus.queue_stake(address, amount);
        Ok(())
    }

    pub fn unbond_validator(&mut self, address: Address, amount: U256) -> Result<()> {
        self.consensus.queue_unbond(address, amount);
        Ok(())
    }

    pub fn slash_validator(
        &mut self,
        address: Address,
        amount: U256,
        reason: impl Into<String>,
    ) -> Result<()> {
        self.consensus.queue_slash(address, amount, reason);
        Ok(())
    }

    pub fn fund_account(&mut self, address: Address, balance: U256, nonce: u64) {
        self.evm.state.mark_dirty(address);
        let code = Bytecode::new();
        let code_hash = KECCAK_EMPTY;
        let info = AccountInfo::new(balance, nonce, code_hash, code);
        self.evm.db.insert_account_info(address, info);
    }

    pub fn submit_tx(&mut self, tx: Transaction) -> Result<(), TxRejection> {
        self.validate_tx_basic(&tx)?;
        self.insert_validated_tx(tx)
    }

    /// Submit a transaction from eth_sendTransaction (unsigned, from field is trusted).
    /// Skips signature verification since the caller specifies `from` directly.
    pub fn submit_tx_unsigned(&mut self, tx: Transaction) -> Result<(), TxRejection> {
        self.validate_tx_basic_no_sig(&tx)?;
        self.insert_validated_tx(tx)
    }

    fn insert_validated_tx(&mut self, tx: Transaction) -> Result<(), TxRejection> {
        let account = self
            .evm
            .db
            .basic(tx.from)
            .map_err(|_| TxRejection::DatabaseError)?
            .unwrap_or_default();

        match self
            .mempool
            .validate(&tx, self.base_fee, account.nonce, account.balance)
        {
            Ok(()) => {}
            Err(TxRejection::FutureNonce) => {}
            Err(TxRejection::GasPriceTooLow) => {}
            Err(e) => return Err(e),
        }

        self.mempool.insert(tx, self.base_fee, account.nonce)?;
        metrics::increment_counter!("tx_submitted_total");
        Ok(())
    }

    pub fn execute_block(&mut self) -> Result<Block> {
        // Never produce for a height the network has already finalized.
        // Votes (tiny UDP packets) routinely arrive before the block they
        // vote for (large): on Sep 13 validator 3 reached the 2/3 quorum for
        // validator 4's block while still waiting for the block itself, then
        // its round timer fired and it produced a competing block — a
        // self-fork that finality could no longer undo because the height
        // was already marked finalized. Block-sync fetches the canonical
        // block within seconds; producing here can only create an orphan.
        if let Some(finalized) = self.finalized_hash(self.block_number) {
            metrics::increment_counter!("mersennet_produce_skipped_finalized_total");
            anyhow::bail!(
                "height {} is already finalized by the network ({finalized}); not producing a competing block",
                self.block_number
            );
        }
        self.checkpoint_before(self.block_number);
        self.maybe_epoch_transition(self.block_number);
        if let Some(me) = self.local_validator {
            self.record_leader_slots(self.block_number, me);
        }
        // Mainnet safety checks
        if let Err(e) = self.mainnet_guard.pre_block_checks(self.block_number) {
            return Err(anyhow::anyhow!("mainnet guard: {}", e));
        }

        // Auto-flip the privacy hard-fork switch if the scheduled
        // activation height has been reached. This must happen
        // *before* any tx is processed so the dispatch tables are
        // consistent for the whole block.
        let _flipped = self.auto_activate_privacy_if_scheduled();

        // DKG epoch boundary hook. Returns Some(new_epoch) iff a
        // new ceremony just started; in that case the consensus
        // layer will include DKG inputs in this block's HotStuff-2
        // sub-round.
        if self.privacy_mode_activated
            && let Some(new_epoch) = self.dkg.on_block(self.block_number)
        {
            metrics::counter!(
                "mersennet_dkg_ceremonies_started_total",
                1,
                "epoch" => new_epoch.to_string()
            );
            tracing::info!(
                block = self.block_number,
                epoch = new_epoch,
                "DKG ceremony started for new epoch"
            );
        }

        let start = Instant::now();
        let mut gas_used = 0u64;
        let mut receipts = Vec::new();
        let mut transactions = Vec::new();
        // Total supply before any state mutation in this block. The
        // full ledger is mirrored in memory, so this is exact and lets
        // the conservation invariant compare the block's net balance
        // delta against (minted - gas burned).
        let supply_before = self.sum_all_balances();
        let pre_shielded_snapshot = self.shielded_evm.state.snapshot();
        let pre_transparent_balances = self.shielded_evm.transparent_balances.clone();
        let pre_tick_witness = crate::state_proof::shielded_tick_witness(
            &self.shielded_orders,
            &self.threshold_mempool,
            &self.liquidation_auction,
        );
        let mut nonce_cache: HashMap<Address, u64> = HashMap::new();
        let mut progressed = true;

        let orders_state = std::mem::take(&mut self.orders.state);
        let shared_orders = Arc::new(Mutex::new(orders_state));
        precompiles::set_mersennet_orders_context(shared_orders.clone());
        precompiles::set_transparent_mersennet_orders_enabled(!self.privacy_mode_activated);
        // Capture CLOB events (fills/cancels/deposits) emitted by precompile
        // txs during this block so they land in block.domain_events.
        precompiles::set_orders_event_recording(true);

        // Generate market maker quotes and submit to orders engine
        let mm_markets: Vec<u64> = {
            let orders = shared_orders.lock().unwrap();
            orders.markets.values().map(|m| m.id.0).collect()
        };
        for market_id in &mm_markets {
            let quotes = self.market_maker.generate_quotes(*market_id);
            for quote in quotes {
                let mut orders = shared_orders.lock().unwrap();
                for (price, _size) in &quote.bids {
                    let _ = orders.place_order(
                        quote.owner,
                        crate::mersennet_orders::MarketId(*market_id),
                        crate::mersennet_orders::Side::Buy,
                        U256::from(*price),
                        U256::from(1),
                        crate::mersennet_orders::TimeInForce::Ioc,
                    );
                }
                for (price, _size) in &quote.asks {
                    let _ = orders.place_order(
                        quote.owner,
                        crate::mersennet_orders::MarketId(*market_id),
                        crate::mersennet_orders::Side::Sell,
                        U256::from(*price),
                        U256::from(1),
                        crate::mersennet_orders::TimeInForce::Ioc,
                    );
                }
            }
        }

        // Process expired intents
        self.intent_engine.expire_intents(self.block_number);

        // Execute ready intents
        let pending_intent_ids: Vec<B256> = self
            .intent_engine
            .pending_intents()
            .iter()
            .map(|i| i.id)
            .collect();
        for intent_id in pending_intent_ids {
            let _ = self
                .intent_engine
                .execute_intent(intent_id, self.block_number);
        }

        // Process Account Abstraction bundles
        if let Some(bundle) = self.aa_bundler.create_bundle(self.coinbase) {
            tracing::debug!(
                ops = bundle.ops.len(),
                gas = bundle.total_gas,
                "processing AA bundle"
            );
        }

        // CRITICAL: no early return (`?`) inside this loop — orders.state is
        // taken (left at Default) until the restore below, so bailing out
        // mid-loop would wipe the entire CLOB (markets, books, collateral)
        // and the next commit would persist the empty state. A tx that fails
        // validation is dropped from the mempool and skipped instead.
        while progressed {
            progressed = false;
            let mut senders = self.mempool.senders();
            senders.sort_by(|a, b| {
                let a_nonce = nonce_cache
                    .get(a)
                    .copied()
                    .unwrap_or_else(|| self.get_account_nonce(*a).unwrap_or_default());
                let b_nonce = nonce_cache
                    .get(b)
                    .copied()
                    .unwrap_or_else(|| self.get_account_nonce(*b).unwrap_or_default());
                let a_fee = self.mempool.ready_fee(*a, a_nonce).unwrap_or_default();
                let b_fee = self.mempool.ready_fee(*b, b_nonce).unwrap_or_default();
                b_fee.cmp(&a_fee)
            });

            for sender in senders {
                let expected_nonce = match nonce_cache.entry(sender) {
                    std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let Ok(nonce) = self.get_account_nonce(sender) else {
                            continue;
                        };
                        *entry.insert(nonce)
                    }
                };

                let Some(tx_gas) = self.mempool.ready_gas(sender, expected_nonce) else {
                    continue;
                };

                if gas_used.saturating_add(tx_gas) > self.gas_limit_per_block {
                    continue;
                }

                let Some(tx) = self.mempool.take_ready(sender, expected_nonce) else {
                    continue;
                };

                let execution = if tx.tx_type == crate::shielded_evm::SHIELDED_TX_TYPE {
                    self.apply_shielded_tx(&tx)
                } else {
                    match self.execute_tx(&tx) {
                        Ok(exec) => exec,
                        Err(e) => {
                            // Invalid tx (bad nonce, EIP-3607, …): drop it and
                            // move on — never abort the block mid-production.
                            tracing::warn!(
                                from = %tx.from,
                                nonce = tx.nonce,
                                error = %e,
                                "dropping invalid mempool tx during block production"
                            );
                            progressed = true;
                            continue;
                        }
                    }
                };
                gas_used = gas_used.saturating_add(execution.gas_used);
                if let Some(nonce) = nonce_cache.get_mut(&tx.from) {
                    *nonce = nonce.saturating_add(1);
                }
                receipts.push(Receipt {
                    success: execution.success,
                    gas_used: execution.gas_used,
                    output: execution.output.clone(),
                    created_address: execution.created_address,
                    error: None,
                    logs: execution.logs.clone(),
                });
                transactions.push(tx);
                progressed = true;
            }
        }

        precompiles::clear_mersennet_orders_context();
        precompiles::set_transparent_mersennet_orders_enabled(true);
        self.pending_events
            .extend(precompiles::drain_orders_events());
        precompiles::set_orders_event_recording(false);
        self.orders.state = Arc::try_unwrap(shared_orders)
            .expect("no other Arc references")
            .into_inner()
            .expect("mutex not poisoned");

        // Good-till-date sweep: drop resting orders whose expiry height has
        // passed. Runs at the same point (post-tx, pre-commit) on producers
        // and importers, so every node cancels the same orders per block.
        let _ = self.orders.state.expire_orders(self.block_number);

        // Privacy-redesign Phase 4 — discrete-time auction + sealed-
        // bid liquidation tick. No-op pre-fork.
        if self.privacy_mode_activated {
            self.run_shielded_tick();
        }

        for tx in &transactions {
            self.mempool.remove_mined(tx.from, tx.nonce);
        }

        // Preliminary header-only hash: a stable label for the
        // (simulated) NetworkSim finality rounds + reward computation.
        // The canonical, content-committing block hash is derived below
        // once the state root is known, then stamped back in.
        let prelim_hash = B256::from(mersennet_zkp::sp1::derive_block_hash(
            self.block_number,
            &self.header_witness(
                gas_used,
                transactions.len() as u64,
                B256::ZERO,
                0,
                B256::ZERO,
                B256::ZERO,
                B256::ZERO,
            ),
        ));
        let applied_validator_changes = self.consensus.apply_pending_changes(self.block_number);
        let (finality_rounds, slashing_evidence) =
            self.consensus
                .run_finality_rounds(prelim_hash, self.block_number, 2);
        for evidence in &slashing_evidence {
            let amount = self
                .consensus
                .slash_amount_for_evidence(evidence, self.block_number);
            if amount.is_zero() {
                continue;
            }
            let reason = match evidence.kind {
                EvidenceKind::DoubleSign => "double_sign_evidence",
                EvidenceKind::PrecommitTimeout => "precommit_timeout",
            };
            self.consensus
                .queue_slash(evidence.validator, amount, reason);
            self.consensus.record_offense(evidence.validator);
        }
        let mut consensus = self.consensus.finalize(prelim_hash, self.block_number);
        let finalized = finality_rounds.iter().any(|round| round.finalized);
        consensus.finalized = finalized;
        let unbonded = self.consensus.process_unbonding(self.block_number);
        let slashes = self.consensus.apply_pending_slashes(self.block_number);
        let rewards = consensus.rewards.clone();
        let total_reward = consensus.total_reward;
        let burned_reward = consensus.burned_reward;

        self.apply_rewards(&rewards)?;

        self.evm.state.begin_commit(self.block_number)?;
        let state_root = self.evm.state.commit_state(
            &self.evm.db,
            &self.orders.state,
            &self.bridge.orders_to_evm,
            &self.bridge.evm_to_orders,
            self.block_number,
        )?;

        // Update flat state with committed changes
        {
            let mut account_changes = Vec::new();
            for (addr, info) in self.evm.db.accounts.iter() {
                account_changes.push((
                    *addr,
                    FlatAccount {
                        balance: info.info.balance,
                        nonce: info.info.nonce,
                        code_hash: info.info.code_hash,
                        storage_root: B256::ZERO,
                    },
                ));
            }
            let changeset = StateChangeset {
                account_changes,
                storage_changes: Vec::new(),
                code_changes: Vec::new(),
            };
            let _ = self
                .flat_state
                .commit_block(self.block_number, state_root, changeset);
        }

        let mut bridge_orders_to_evm = Vec::new();
        let mut bridge_evm_to_orders = Vec::new();
        while let Some(msg) = self.bridge.orders_to_evm.pop() {
            bridge_orders_to_evm.push(msg);
        }
        while let Some(msg) = self.bridge.evm_to_orders.pop() {
            bridge_evm_to_orders.push(msg);
        }

        let domain_events = std::mem::take(&mut self.pending_events);

        let tx_count = transactions.len();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Canonical, content-committing block hash: binds the parent
        // hash (hash-linked chain), timestamp, transaction root, state
        // root, and receipts root. Computed here, after the state root
        // is known, so the hash actually commits to the block's content.
        let parent_hash = self.chain.last().map(|b| b.hash).unwrap_or(B256::ZERO);
        let tx_root = Self::compute_tx_root(&transactions);
        let receipts_root = Self::compute_receipts_root(&receipts);
        let header_witness = self.header_witness(
            gas_used,
            transactions.len() as u64,
            parent_hash,
            timestamp,
            tx_root,
            state_root,
            receipts_root,
        );
        let hash = B256::from(mersennet_zkp::sp1::derive_block_hash(
            self.block_number,
            &header_witness,
        ));
        // Stamp the canonical hash into the consensus record.
        consensus.block_hash = hash;

        // Privacy-redesign Phase 4 — compute shielded roots + the SP1
        // state-transition proof. Pre-fork the roots stay
        // `B256::ZERO` and no proof is generated. The proof binds to the
        // canonical content hash.
        let (shielded_state_root, nullifier_root, shielded_event_root, state_proof) = self
            .shielded_block_header(
                &transactions,
                &header_witness,
                gas_used,
                Some(&pre_shielded_snapshot),
                Some(&pre_transparent_balances),
                Some(&pre_tick_witness),
            )?;

        let block = Block {
            number: self.block_number,
            chain_id: self.chain_id,
            timestamp,
            gas_limit: self.gas_limit_per_block,
            gas_used,
            base_fee: self.base_fee,
            coinbase: consensus.proposer,
            hash,
            parent_hash,
            proposer: consensus.proposer,
            finalized: consensus.finalized,
            consensus,
            unbonded,
            applied_validator_changes,
            slashes,
            finality_rounds,
            slashing_evidence,
            rewards,
            total_reward,
            burned_reward,
            state_root,
            transactions,
            receipts,
            bridge_orders_to_evm,
            bridge_evm_to_orders,
            domain_events,
            shielded_state_root,
            nullifier_root,
            shielded_event_root,
            state_proof,
            proposer_sig: None,
        };

        self.chain.push(block.clone());
        self.trim_chain_window();
        self.evm.state.store_block(&block)?;
        self.evm.state.end_commit()?;
        self.last_block_applied_at = Some(std::time::Instant::now());

        // Privacy-redesign Phase 4 — persist shielded state at end of
        // block. No-op pre-fork (the subsystems are still in their
        // initial state and saving an empty snapshot is harmless).
        if self.privacy_mode_activated
            && let Some(p) = &self.shielded_persistence
            && let Err(e) = p.save_shielded_evm(&self.shielded_evm, block.number)
        {
            tracing::warn!(error = ?e, block = block.number, "shielded persistence save failed");
        }

        // Run formal verification invariant checks.
        //
        // Conservation of value: the only source of newly minted value
        // is the block reward (`total_reward`, credited to validators by
        // `apply_rewards`); the only burn is the base fee, which revm
        // removes from senders without crediting the coinbase
        // (`base_fee * gas_used`). Transfers and priority tips conserve
        // value. `burned_reward` is scheduled-but-undistributed dust
        // that is never minted, so it must NOT appear here. We encode
        // the pre/post total supply as single-entry maps because the
        // checker compares the sums.
        {
            use crate::formal_verification::BlockReport;
            let supply_after = self.sum_all_balances();
            let gas_burned = self.base_fee.saturating_mul(U256::from(gas_used));
            let report = BlockReport {
                height: block.number,
                balances_before: HashMap::from([(Address::ZERO, supply_before)]),
                balances_after: HashMap::from([(Address::ZERO, supply_after)]),
                minted: block.total_reward,
                burned: gas_burned,
                fills: Vec::new(),
                best_bid: 0,
                best_ask: u64::MAX,
                nonces: HashMap::new(),
                fill_prices: Vec::new(),
            };
            let violations = self.invariant_checker.check_all(block.number, &report);
            if !violations.is_empty() {
                for v in &violations {
                    tracing::warn!(
                        invariant = ?v.invariant,
                        severity = ?v.severity,
                        desc = %v.description,
                        "invariant violation detected"
                    );
                }
            }
        }

        metrics::increment_counter!("mersennet_blocks_produced_total");
        metrics::gauge!("mersennet_height", self.block_number as f64);
        metrics::gauge!("mersennet_block_gas_used", gas_used as f64);
        metrics::gauge!("mersennet_block_tx_count", tx_count as f64);
        tracing::info!(
            height = self.block_number,
            txs = tx_count,
            gas = gas_used,
            finalized,
            base_fee = %self.base_fee,
            "block produced"
        );

        self.base_fee = self.next_base_fee(gas_used);
        self.mempool.promote(self.base_fee);
        self.mempool.demote(self.base_fee);
        metrics::gauge!("mersennet_base_fee_wei", self.base_fee.as_limbs()[0] as f64);

        self.block_number += 1;
        metrics::histogram!(
            "mersennet_block_execution_seconds",
            start.elapsed().as_secs_f64()
        );

        // Post-block mainnet checks
        if let Err(e) = self.mainnet_guard.post_block_checks(
            self.block_number.saturating_sub(1),
            block.gas_used,
            block.transactions.len(),
        ) {
            tracing::warn!(%e, "mainnet post-block check warning");
        }

        Ok(block)
    }

    /// Execute a block using parallel EVM execution (Block-STM / Grevm pattern).
    ///
    /// Collects transactions from the mempool identically to `execute_block`, then
    /// delegates execution to `ParallelExecutor` which groups independent txs and
    /// runs them concurrently on forked DB snapshots. Falls back to sequential if
    /// the batch is small or conflicts are detected.
    pub fn execute_block_parallel(&mut self) -> Result<Block> {
        let start = Instant::now();

        // Phase 1: Collect transactions from mempool (same priority logic as execute_block)
        let mut estimated_gas = 0u64;
        let mut collected_txs: Vec<Transaction> = Vec::new();
        let mut nonce_cache: HashMap<Address, u64> = HashMap::new();
        let mut progressed = true;

        while progressed {
            progressed = false;
            let mut senders = self.mempool.senders();
            senders.sort_by(|a, b| {
                let a_nonce = nonce_cache
                    .get(a)
                    .copied()
                    .unwrap_or_else(|| self.get_account_nonce(*a).unwrap_or_default());
                let b_nonce = nonce_cache
                    .get(b)
                    .copied()
                    .unwrap_or_else(|| self.get_account_nonce(*b).unwrap_or_default());
                let a_fee = self.mempool.ready_fee(*a, a_nonce).unwrap_or_default();
                let b_fee = self.mempool.ready_fee(*b, b_nonce).unwrap_or_default();
                b_fee.cmp(&a_fee)
            });

            for sender in senders {
                let expected_nonce = match nonce_cache.entry(sender) {
                    std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let nonce = self.get_account_nonce(sender)?;
                        *entry.insert(nonce)
                    }
                };

                let Some(tx_gas) = self.mempool.ready_gas(sender, expected_nonce) else {
                    continue;
                };

                if estimated_gas.saturating_add(tx_gas) > self.gas_limit_per_block {
                    continue;
                }

                let Some(tx) = self.mempool.take_ready(sender, expected_nonce) else {
                    continue;
                };

                estimated_gas = estimated_gas.saturating_add(tx_gas);
                if let Some(nonce) = nonce_cache.get_mut(&tx.from) {
                    *nonce = nonce.saturating_add(1);
                }
                collected_txs.push(tx);
                progressed = true;
            }
        }

        // Phase 2a: Split shielded txs out — they don't traverse the
        // EVM, so they can't go through ParallelExecutor. They mutate
        // dedicated subsystems (shielded_evm, shielded_orders,
        // liquidation_auction) and are applied sequentially below.
        let pre_shielded_snapshot = self.shielded_evm.state.snapshot();
        let pre_transparent_balances = self.shielded_evm.transparent_balances.clone();
        let pre_tick_witness = crate::state_proof::shielded_tick_witness(
            &self.shielded_orders,
            &self.threshold_mempool,
            &self.liquidation_auction,
        );

        let (evm_txs, shielded_txs): (Vec<Transaction>, Vec<Transaction>) = collected_txs
            .iter()
            .cloned()
            .partition(|t| t.tx_type != crate::shielded_evm::SHIELDED_TX_TYPE);

        // Phase 2b: Execute non-shielded txs via ParallelExecutor.
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let executor = ParallelExecutor::new(num_threads, 3);
        let par_result = executor.execute(
            &evm_txs,
            &self.evm.db,
            self.chain_id,
            self.block_number,
            self.coinbase,
            self.gas_limit_per_block,
            self.base_fee,
            self.spec_id,
        );

        // Phase 3: Apply execution results
        self.evm.db = par_result.merged_db;
        for addr in &par_result.dirty_addresses {
            self.evm.state.mark_dirty(*addr);
        }

        let mut gas_used = 0u64;
        let mut receipts = Vec::new();
        let mut transactions = Vec::new();

        for (idx, exec) in par_result.results {
            gas_used = gas_used.saturating_add(exec.gas_used);
            if let Some(addr) = exec.created_address {
                self.code_publication_registry
                    .record_deployment(addr, evm_txs[idx].from);
            }
            receipts.push(Receipt {
                success: exec.success,
                gas_used: exec.gas_used,
                output: exec.output.clone(),
                created_address: exec.created_address,
                error: None,
                logs: exec.logs.clone(),
            });
            transactions.push(evm_txs[idx].clone());
        }

        // Phase 3b: Sequentially apply shielded txs to the privacy
        // subsystems. They never touch revm state, so applying them
        // after the parallel phase preserves determinism.
        for tx in shielded_txs {
            let execution = self.apply_shielded_tx(&tx);
            gas_used = gas_used.saturating_add(execution.gas_used);
            receipts.push(Receipt {
                success: execution.success,
                gas_used: execution.gas_used,
                output: execution.output.clone(),
                created_address: execution.created_address,
                error: None,
                logs: execution.logs.clone(),
            });
            transactions.push(tx);
        }

        // Phase 3c: Shielded tick (FBA + liquidation auctions).
        if self.privacy_mode_activated {
            self.run_shielded_tick();
        }

        for tx in &transactions {
            self.mempool.remove_mined(tx.from, tx.nonce);
        }

        // Phase 4: Consensus and finalization (identical to execute_block)
        // Preliminary header-only hash: a stable label for the
        // (simulated) NetworkSim finality rounds + reward computation.
        // The canonical, content-committing block hash is derived below
        // once the state root is known, then stamped back in.
        let prelim_hash = B256::from(mersennet_zkp::sp1::derive_block_hash(
            self.block_number,
            &self.header_witness(
                gas_used,
                transactions.len() as u64,
                B256::ZERO,
                0,
                B256::ZERO,
                B256::ZERO,
                B256::ZERO,
            ),
        ));
        let applied_validator_changes = self.consensus.apply_pending_changes(self.block_number);
        let (finality_rounds, slashing_evidence) =
            self.consensus
                .run_finality_rounds(prelim_hash, self.block_number, 2);
        for evidence in &slashing_evidence {
            let amount = self
                .consensus
                .slash_amount_for_evidence(evidence, self.block_number);
            if amount.is_zero() {
                continue;
            }
            let reason = match evidence.kind {
                EvidenceKind::DoubleSign => "double_sign_evidence",
                EvidenceKind::PrecommitTimeout => "precommit_timeout",
            };
            self.consensus
                .queue_slash(evidence.validator, amount, reason);
            self.consensus.record_offense(evidence.validator);
        }
        let mut consensus = self.consensus.finalize(prelim_hash, self.block_number);
        let finalized = finality_rounds.iter().any(|round| round.finalized);
        consensus.finalized = finalized;
        let unbonded = self.consensus.process_unbonding(self.block_number);
        let slashes = self.consensus.apply_pending_slashes(self.block_number);
        let rewards = consensus.rewards.clone();
        let total_reward = consensus.total_reward;
        let burned_reward = consensus.burned_reward;

        self.apply_rewards(&rewards)?;

        self.evm.state.begin_commit(self.block_number)?;
        let state_root = self.evm.state.commit_state(
            &self.evm.db,
            &self.orders.state,
            &self.bridge.orders_to_evm,
            &self.bridge.evm_to_orders,
            self.block_number,
        )?;

        let mut bridge_orders_to_evm = Vec::new();
        let mut bridge_evm_to_orders = Vec::new();
        while let Some(msg) = self.bridge.orders_to_evm.pop() {
            bridge_orders_to_evm.push(msg);
        }
        while let Some(msg) = self.bridge.evm_to_orders.pop() {
            bridge_evm_to_orders.push(msg);
        }

        let domain_events = std::mem::take(&mut self.pending_events);

        let tx_count = transactions.len();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let parent_hash = self.chain.last().map(|b| b.hash).unwrap_or(B256::ZERO);
        let tx_root = Self::compute_tx_root(&transactions);
        let receipts_root = Self::compute_receipts_root(&receipts);
        let header_witness = self.header_witness(
            gas_used,
            transactions.len() as u64,
            parent_hash,
            timestamp,
            tx_root,
            state_root,
            receipts_root,
        );
        let hash = B256::from(mersennet_zkp::sp1::derive_block_hash(
            self.block_number,
            &header_witness,
        ));
        consensus.block_hash = hash;

        let (shielded_state_root, nullifier_root, shielded_event_root, state_proof) = self
            .shielded_block_header(
                &transactions,
                &header_witness,
                gas_used,
                Some(&pre_shielded_snapshot),
                Some(&pre_transparent_balances),
                Some(&pre_tick_witness),
            )?;

        let block = Block {
            number: self.block_number,
            chain_id: self.chain_id,
            timestamp,
            gas_limit: self.gas_limit_per_block,
            gas_used,
            base_fee: self.base_fee,
            coinbase: consensus.proposer,
            hash,
            parent_hash,
            proposer: consensus.proposer,
            finalized: consensus.finalized,
            consensus,
            unbonded,
            applied_validator_changes,
            slashes,
            finality_rounds,
            slashing_evidence,
            rewards,
            total_reward,
            burned_reward,
            state_root,
            transactions,
            receipts,
            bridge_orders_to_evm,
            bridge_evm_to_orders,
            domain_events,
            shielded_state_root,
            nullifier_root,
            shielded_event_root,
            state_proof,
            proposer_sig: None,
        };

        self.chain.push(block.clone());
        self.trim_chain_window();
        self.evm.state.store_block(&block)?;
        self.evm.state.end_commit()?;
        self.last_block_applied_at = Some(std::time::Instant::now());

        // Privacy-redesign Phase 4 — persist shielded state.
        if self.privacy_mode_activated
            && let Some(p) = &self.shielded_persistence
            && let Err(e) = p.save_shielded_evm(&self.shielded_evm, block.number)
        {
            tracing::warn!(error = ?e, block = block.number, "shielded persistence save failed (parallel)");
        }

        metrics::increment_counter!("mersennet_blocks_produced_total");
        metrics::gauge!("mersennet_height", self.block_number as f64);
        metrics::gauge!("mersennet_block_gas_used", gas_used as f64);
        metrics::gauge!("mersennet_block_tx_count", tx_count as f64);
        tracing::info!(
            height = self.block_number,
            txs = tx_count,
            gas = gas_used,
            finalized,
            base_fee = %self.base_fee,
            "block produced (parallel)"
        );

        self.base_fee = self.next_base_fee(gas_used);
        self.mempool.promote(self.base_fee);
        self.mempool.demote(self.base_fee);
        metrics::gauge!("mersennet_base_fee_wei", self.base_fee.as_limbs()[0] as f64);

        self.block_number += 1;
        metrics::histogram!(
            "mersennet_block_execution_seconds",
            start.elapsed().as_secs_f64()
        );
        Ok(block)
    }

    /// After a locally produced block is signed, stamp the proposer's address
    /// and signature onto the copy already stored in `self.chain` so peer
    /// sync (which reads from the chain) serves authenticated blocks — not
    /// the unsigned pre-signature snapshot.
    pub fn attach_proposer_sig(&mut self, height: u64, proposer: Address, sig: (U256, U256, u64)) {
        if let Some(block) = self.chain.iter_mut().rev().find(|b| b.number == height) {
            block.proposer = proposer;
            block.coinbase = proposer;
            block.consensus.proposer = proposer;
            block.proposer_sig = Some(sig);
            // Persist the SIGNED block. `execute_block` calls `store_block`
            // before the signature exists, so the on-disk copy is unsigned;
            // peers serve synced blocks from disk (`get_block`), and a
            // syncing node rejects unsigned blocks ("no proposer signature").
            // Re-store here so block-sync serves authenticated blocks —
            // without this a wiped/behind node can never catch up.
            let signed = block.clone();
            if let Err(e) = self.evm.state.store_block(&signed) {
                tracing::warn!(height, error = %e, "failed to persist signed block");
            }
        }
    }

    /// Ingest a gossiped block. Blocks are buffered and applied
    /// strictly in ascending-height order via [`Engine::apply_imported_block`],
    /// which re-executes the block's transactions so non-producing
    /// nodes converge on the same state as the producer. Out-of-order
    /// or duplicate deliveries are tolerated.
    pub fn import_block(&mut self, block: Block) {
        // Track the network's head regardless of whether we can apply this
        // block, so a behind/wedged validator knows not to produce (see
        // `is_behind_network`).
        if block.number > self.highest_observed_height {
            self.highest_observed_height = block.number;
        }
        // Already applied: if it is a *different* block for a recent height,
        // keep it — finality may pick it over ours (see reorg_to_finalized).
        if block.number < self.block_number {
            if block.number + (REORG_DEPTH as u64) >= self.block_number
                && self
                    .block_by_number(block.number)
                    .map(|b| b.hash != block.hash)
                    .unwrap_or(false)
                && self
                    .finalized_hash(block.number)
                    .is_none_or(|f| f == block.hash)
            {
                self.remember_alt_block(block);
            }
            return;
        }
        // Too far ahead to buffer usefully — block sync will backfill
        // the gap. Bounds memory against a flood of future blocks.
        if block.number > self.block_number.saturating_add(512) {
            return;
        }
        // A proposal for a height the network has already finalized with a
        // different hash lost a leader-timeout race. Applying it would put
        // this node on a dead branch (there is no automatic reorg), which is
        // exactly how a node finishing its catch-up used to fork at the tip:
        // it had buffered every gossip proposal while syncing and applied
        // the orphaned one when block-sync reached that height.
        if let Some(finalized) = self.finalized_hash(block.number)
            && finalized != block.hash
        {
            metrics::increment_counter!("mersennet_import_orphan_dropped_total");
            tracing::warn!(
                height = block.number,
                block_hash = %block.hash,
                finalized_hash = %finalized,
                "dropping competing proposal for an already-finalized height"
            );
            return;
        }
        self.import_buffer.insert(block.number, block);

        // Drain consecutive buffered blocks starting at the next
        // expected height.
        while let Some(next) = self.import_buffer.remove(&self.block_number) {
            let height = next.number;
            // Finality may have arrived after this block was buffered.
            if let Some(finalized) = self.finalized_hash(height)
                && finalized != next.hash
            {
                metrics::increment_counter!("mersennet_import_orphan_dropped_total");
                tracing::warn!(
                    height,
                    block_hash = %next.hash,
                    finalized_hash = %finalized,
                    "dropping buffered competing proposal for an already-finalized height; waiting for the canonical block"
                );
                break;
            }
            if let Err(e) = self.apply_imported_block(next) {
                let msg = e.to_string();
                // A persistent parent-hash mismatch means this node is on a
                // minority fork and can no longer follow the canonical chain
                // (there is no automatic reorg yet — full fork-choice is the
                // remaining consensus item). Surface it loudly and via a metric
                // so monitoring can trigger an operator resync instead of the
                // node silently stalling.
                if msg.contains("parent hash mismatch") {
                    metrics::increment_counter!("mersennet_import_fork_detected_total");
                    self.fork_detected_at = Some(std::time::Instant::now());
                    tracing::error!(
                        height,
                        error = %msg,
                        "FORK DETECTED: local head diverges from canonical chain; node needs a state resync"
                    );
                } else {
                    tracing::warn!(height, error = %msg, "failed to apply imported block");
                }
                // Stop draining; the block will be re-gossiped and
                // retried. Do not loop on a persistently failing block.
                break;
            }
        }
    }

    /// Re-execute an externally produced block against local state.
    ///
    /// `execute_block` *builds* a block by pulling transactions from
    /// the local mempool; this instead replays the block's already
    /// ordered transaction list. Because `coinbase` is never
    /// reconfigured (it stays `Address::ZERO` on every node) and the
    /// per-tx EVM environment is otherwise derived from values carried
    /// by the block, every node executes the identical sequence and
    /// arrives at the same EVM state. Proposer/validator rewards are
    /// carried in the block and re-credited here.
    ///
    /// Note: market-maker quotes, intents and AA bundles that
    /// `execute_block` generates locally are *not* re-run here. They
    /// are no-ops on the current chain (no active markets / privacy not
    /// activated); once trading or privacy is live this path must also
    /// replicate them for full state-root parity.
    fn apply_imported_block(&mut self, mut block: Block) -> Result<()> {
        debug_assert_eq!(block.number, self.block_number);
        // Only worth a checkpoint near the tip; a replay of finalized
        // history cannot be reorged.
        if self.highest_observed_height <= block.number.saturating_add(REORG_DEPTH as u64) {
            self.checkpoint_before(block.number);
        }

        // Hash-linked chain check: the incoming block must reference our
        // current head as its parent. Reject a block that forks off a
        // different history so a node never silently applies an
        // inconsistent chain. (Genesis-adjacent blocks with a zero
        // parent are allowed when we have no prior block.)
        let expected_parent = self.chain.last().map(|b| b.hash).unwrap_or(B256::ZERO);
        // A zero parent hash is only legitimate at genesis (empty local chain).
        // Previously any block with parent_hash == ZERO skipped the link check,
        // which let an injected block bypass it at any height.
        let genesis_adjacent = self.chain.is_empty();
        if !(genesis_adjacent && block.parent_hash == B256::ZERO)
            && block.parent_hash != expected_parent
        {
            anyhow::bail!(
                "parent hash mismatch at height {}: block parent {} != local head {}",
                block.number,
                block.parent_hash,
                expected_parent
            );
        }

        // Open validator set: the active set for this height is decided at
        // the epoch boundary, before the proposer is authenticated against
        // the leader schedule. Idempotent, so a rejected block re-imported
        // later does not apply it twice.
        self.maybe_epoch_transition(block.number);

        // Authenticate the proposer: the block must be signed by the address in
        // `block.proposer`, that address must be a current validator, and it
        // must be a legitimately elected leader for this height (at some early
        // round). Without this, anyone who can deliver a gossip packet could
        // inject an arbitrary block. Genesis (height 0) is exempt, and so are
        // validator-less setups (single-node/dev), where there is no leader
        // schedule to verify against — a live network always has validators.
        if block.number > 0 && !self.consensus.validators().is_empty() {
            let (r, s, y) = block.proposer_sig.ok_or_else(|| {
                anyhow::anyhow!("imported block {} has no proposer signature", block.number)
            })?;
            let recovered = crypto::recover_block_proposer(block.number, block.hash, r, s, y)
                .map_err(|e| anyhow::anyhow!("proposer signature invalid: {e}"))?;
            if recovered != block.proposer {
                anyhow::bail!(
                    "proposer signature signer {} != block proposer {}",
                    recovered,
                    block.proposer
                );
            }
            if self.stake_of(block.proposer).is_zero() {
                anyhow::bail!(
                    "block {} proposer {} is not a staked validator",
                    block.number,
                    block.proposer
                );
            }
            // The elected leader rotates by round on timeout; accept the
            // proposer if it is the leader at any round within a bounded window.
            let is_elected_leader = (0..MAX_LEADER_ROUND_WINDOW)
                .any(|round| self.leader_for_height(block.number, round) == Some(block.proposer));
            if !is_elected_leader {
                anyhow::bail!(
                    "block {} proposer {} is not an elected leader for this height",
                    block.number,
                    block.proposer
                );
            }
        }
        self.record_leader_slots(block.number, block.proposer);

        // Match the producer's per-tx execution environment.
        self.base_fee = block.base_fee;

        let orders_state = std::mem::take(&mut self.orders.state);
        let shared_orders = Arc::new(Mutex::new(orders_state));
        precompiles::set_mersennet_orders_context(shared_orders.clone());
        precompiles::set_transparent_mersennet_orders_enabled(!self.privacy_mode_activated);
        // Re-executing the block's precompile txs deterministically regenerates
        // the producer's CLOB events (the wire strips domain_events), so
        // followers store and serve the same fills/cancels as the producer.
        precompiles::set_orders_event_recording(true);

        // CRITICAL: no early return (`?`) while orders.state is taken — a
        // failed tx used to abort the import with orders.state left at
        // Default, silently destroying the entire CLOB (markets, books,
        // collateral), which the next commit then persisted. Collect the
        // error instead, restore the state unconditionally, then propagate.
        let mut import_err: Option<anyhow::Error> = None;
        let mut gas_used = 0u64;
        // Rebuild receipts from local re-execution. The wire form (`WireReceipt`)
        // drops EVM logs to save bandwidth, so a block imported over gossip would
        // otherwise carry empty `logs`/`logsBloom` — breaking eth_getLogs,
        // eth_getTransactionReceipt logs, and any token-transfer indexing on
        // every follower and the public RPC node. Re-execution is deterministic
        // and the logs are not part of `receipts_root` (success+gas+output only),
        // so this reconstructs the producer's receipts without changing the block
        // hash. This mirrors the domain-events reconstruction just below.
        let mut rebuilt_receipts: Vec<Receipt> = Vec::with_capacity(block.transactions.len());
        for tx in &block.transactions {
            let execution = if tx.tx_type == crate::shielded_evm::SHIELDED_TX_TYPE {
                self.apply_shielded_tx(tx)
            } else {
                match self.execute_tx(tx) {
                    Ok(exec) => exec,
                    Err(e) => {
                        import_err = Some(e);
                        break;
                    }
                }
            };
            gas_used = gas_used.saturating_add(execution.gas_used);
            rebuilt_receipts.push(Receipt {
                success: execution.success,
                gas_used: execution.gas_used,
                output: execution.output.clone(),
                created_address: execution.created_address,
                error: None,
                logs: execution.logs.clone(),
            });
            self.mempool.remove_mined(tx.from, tx.nonce);
        }

        precompiles::clear_mersennet_orders_context();
        precompiles::set_transparent_mersennet_orders_enabled(true);
        if block.domain_events.is_empty() {
            block.domain_events = precompiles::drain_orders_events();
        }
        precompiles::set_orders_event_recording(false);
        self.orders.state = Arc::try_unwrap(shared_orders)
            .expect("no other Arc references")
            .into_inner()
            .expect("mutex not poisoned");
        if let Some(e) = import_err {
            return Err(e);
        }

        // Good-till-date sweep — must mirror the producer's end-of-block
        // sweep in execute_block exactly, or state roots diverge.
        let _ = self.orders.state.expire_orders(block.number);

        // Replace the log-stripped wire receipts with the locally reconstructed
        // ones (see the rebuild note above). Only reached when every tx applied,
        // so the receipt list is complete and index-aligned with the tx list.
        block.receipts = rebuilt_receipts;

        // Re-credit the proposer/validator rewards the producer applied.
        self.apply_rewards(&block.rewards)?;

        // Committing the full state to disk every block dominates import
        // time during deep catch-up (execution takes milliseconds; the
        // commit serializes the whole account set and orders book). While
        // far behind the network head, commit periodically instead —
        // execution state lives in memory, so this only trades crash
        // durability: on restart the node resumes from the last committed
        // height and block-sync re-pulls the gap.
        let catching_up = self.highest_observed_height > block.number.saturating_add(64);
        let should_commit = !catching_up || block.number.is_multiple_of(500);
        if should_commit {
            self.evm.state.begin_commit(block.number)?;
            let state_root = self.evm.state.commit_state(
                &self.evm.db,
                &self.orders.state,
                &self.bridge.orders_to_evm,
                &self.bridge.evm_to_orders,
                block.number,
            )?;

            // The canonical state root is the producer's `block.state_root`
            // (it is what the block hash commits to, what the SP1 proof binds
            // to, and what every node stores and agrees on). A local
            // re-derivation that differs here reflects a deterministic
            // produce-vs-import execution asymmetry, not a cross-node fork —
            // all nodes still store the identical canonical root. Track it as
            // a metric and log a rate-limited sample instead of flooding the
            // log every block; a genuine, growing divergence would still be
            // visible via the counter and the periodic sample.
            if block.state_root != B256::ZERO && state_root != block.state_root {
                metrics::increment_counter!("mersennet_import_state_root_recompute_diff_total");
                tracing::debug!(
                    height = block.number,
                    proposer = %block.proposer,
                    txs = block.transactions.len(),
                    local = %state_root,
                    canonical = %block.state_root,
                    "state-root recompute differs (per-block detail)"
                );
                if block.number.is_multiple_of(500) {
                    tracing::warn!(
                        height = block.number,
                        local = %state_root,
                        canonical = %block.state_root,
                        "imported-block local state-root recompute differs from canonical (rate-limited sample; consensus uses the canonical root)"
                    );
                }
            }

            // Mirror the producer's flat-state update so flat reads stay
            // consistent with the committed EVM state.
            {
                let mut account_changes = Vec::new();
                for (addr, info) in self.evm.db.accounts.iter() {
                    account_changes.push((
                        *addr,
                        FlatAccount {
                            balance: info.info.balance,
                            nonce: info.info.nonce,
                            code_hash: info.info.code_hash,
                            storage_root: B256::ZERO,
                        },
                    ));
                }
                let changeset = StateChangeset {
                    account_changes,
                    storage_changes: Vec::new(),
                    code_changes: Vec::new(),
                };
                let _ = self
                    .flat_state
                    .commit_block(block.number, state_root, changeset);
            }
        }

        self.evm.state.store_block(&block)?;
        if should_commit {
            self.evm.state.end_commit()?;
        }
        self.last_block_applied_at = Some(std::time::Instant::now());
        self.chain.push(block);
        self.trim_chain_window();
        self.block_number = self.block_number.saturating_add(1);
        metrics::gauge!(
            "mersennet_height",
            self.block_number.saturating_sub(1) as f64
        );
        Ok(())
    }

    pub fn mempool_is_empty(&self) -> bool {
        self.mempool.is_empty()
    }

    pub fn mempool_pending_count(&self) -> usize {
        self.mempool.pending_count()
    }

    /// Non-consuming snapshot of pending mempool transactions for P2P relay.
    pub fn mempool_pending_snapshot(&self) -> Vec<Transaction> {
        self.mempool.pending_snapshot()
    }

    /// Remember the raw signed envelope of a wallet-submitted tx so the
    /// relay can gossip it verbatim. Ethereum-format signatures only verify
    /// against the raw RLP signing payload, which peers cannot rebuild from
    /// parsed fields — relaying the envelope keeps the tx self-authenticating.
    pub fn cache_raw_tx(&mut self, hash: B256, raw: Vec<u8>) {
        if self.raw_tx_cache.len() >= 8192 {
            self.raw_tx_cache.clear();
        }
        self.raw_tx_cache.insert(hash, raw);
    }

    /// Flush the state backend; used by the shutdown path after the engine
    /// lock has been acquired (so no commit is in flight).
    pub fn flush_state(&self) -> Result<()> {
        self.evm.state.flush()
    }

    pub fn raw_tx_for(&self, hash: &B256) -> Option<Vec<u8>> {
        self.raw_tx_cache.get(hash).cloned()
    }

    pub fn mempool_queued_count(&self) -> usize {
        self.mempool.queued_count()
    }

    #[allow(dead_code)]
    pub fn export_state_snapshot(&mut self, path: impl AsRef<std::path::Path>) -> Result<B256> {
        let root = self.evm.state.commit_state(
            &self.evm.db,
            &self.orders.state,
            &self.bridge.orders_to_evm,
            &self.bridge.evm_to_orders,
            self.block_number.saturating_sub(1),
        )?;
        let height = self.block_number.saturating_sub(1);
        let evm_snapshot = self.evm.state.export_snapshot_bytes(height, root)?;

        // Phase 4 — bundle the shielded subsystems into the same
        // snapshot. The envelope is forward-compatible: legacy
        // (pre-fork) snapshots still decode via
        // `EngineSnapshotEnvelope::decode` thanks to the magic
        // prefix detection.
        let shielded_data = crate::engine_snapshot::ShieldedSnapshotData {
            state: self.shielded_evm.state.snapshot(),
            auction: self.liquidation_auction.snapshot(),
            transparent_balances: crate::engine_snapshot::encode_transparent_balances(
                &self.shielded_evm.transparent_balances,
            ),
            // The migration flag is true once a non-empty shielded
            // state exists (the migration tool has run); otherwise
            // false.
            migration_plan_applied: !self.shielded_evm.state.snapshot().leaves.is_empty(),
            code_publication_registry: self.code_publication_registry.snapshot(),
            encrypted_note_payloads: crate::engine_snapshot::encode_encrypted_note_payloads(
                &self.shielded_evm.encrypted_note_payloads,
            ),
            viewing_grants: crate::engine_snapshot::encode_viewing_grants(
                &self.shielded_evm.viewing_grants,
            ),
            viewing_grant_revocations: crate::engine_snapshot::encode_viewing_grant_revocations(
                &self.shielded_evm.viewing_grant_revocations,
            ),
        };

        let envelope = crate::engine_snapshot::EngineSnapshotEnvelope {
            height,
            chain_id: self.chain_id,
            privacy_mode_activated: self.privacy_mode_activated,
            privacy_activation_height: None, // unset on a regular export; the migration tool stamps this
            evm_snapshot,
            shielded: Some(shielded_data),
        };
        let bytes = envelope.encode()?;
        std::fs::write(path, bytes)?;
        Ok(root)
    }

    #[allow(dead_code)]
    pub fn import_state_snapshot(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<SnapshotMeta> {
        let data = std::fs::read(path)?;
        let envelope = crate::engine_snapshot::EngineSnapshotEnvelope::decode(&data)?;

        if envelope.chain_id != 0 && envelope.chain_id != self.chain_id {
            return Err(anyhow::anyhow!(
                "snapshot chain_id {} does not match local chain_id {}",
                envelope.chain_id,
                self.chain_id
            ));
        }

        // EVM side first.
        let meta = self
            .evm
            .state
            .import_snapshot_bytes(&envelope.evm_snapshot)?;
        self.evm.db = InMemoryDB::default();
        self.evm.state.load_into_db(&mut self.evm.db)?;
        self.evm
            .state
            .load_mersennet_orders(&mut self.orders.state)?;
        self.evm.state.load_bridge_queues(
            &mut self.bridge.orders_to_evm,
            &mut self.bridge.evm_to_orders,
        )?;

        // Shielded side.
        if let Some(shielded) = envelope.shielded {
            self.shielded_evm.state =
                crate::shielded_state::ShieldedState::restore(&shielded.state);
            self.liquidation_auction =
                crate::liquidation_auction::LiquidationAuction::restore(shielded.auction);
            self.shielded_evm.transparent_balances =
                crate::engine_snapshot::decode_transparent_balances(&shielded.transparent_balances);
            self.code_publication_registry =
                crate::code_publication::CodePublicationRegistry::restore(
                    shielded.code_publication_registry,
                );
            self.shielded_evm.encrypted_note_payloads =
                crate::engine_snapshot::decode_encrypted_note_payloads(
                    &shielded.encrypted_note_payloads,
                );
            self.shielded_evm.viewing_grants =
                crate::engine_snapshot::decode_viewing_grants(&shielded.viewing_grants);
            self.shielded_evm.viewing_grant_revocations =
                crate::engine_snapshot::decode_viewing_grant_revocations(
                    &shielded.viewing_grant_revocations,
                );
        }

        // Privacy-mode flags.
        self.privacy_mode_activated = envelope.privacy_mode_activated;
        if let Some(h) = envelope.privacy_activation_height {
            self.privacy_activation_height = Some(h);
        }

        self.evm.state.load_bridge_queues(
            &mut self.bridge.orders_to_evm,
            &mut self.bridge.evm_to_orders,
        )?;
        self.block_number = meta.height.saturating_add(1);
        Ok(meta)
    }

    pub fn get_balance(&mut self, address: Address) -> Result<U256> {
        Ok(self
            .evm
            .db
            .basic(address)?
            .map(|info| info.balance)
            .unwrap_or_default())
    }

    pub fn get_account_nonce(&mut self, address: Address) -> Result<u64> {
        Ok(self
            .evm
            .db
            .basic(address)?
            .map(|info| info.nonce)
            .unwrap_or_default())
    }

    pub fn get_code(&mut self, address: Address) -> Result<Bytes> {
        let info = self.evm.db.basic(address)?.unwrap_or_default();
        Ok(info.code.map(|c| c.original_bytes()).unwrap_or_default())
    }

    pub fn get_code_hash(&mut self, address: Address) -> Result<B256> {
        Ok(self
            .evm
            .db
            .basic(address)?
            .map(|info| info.code_hash)
            .unwrap_or(KECCAK_EMPTY))
    }

    pub fn published_code_attestation(&self, address: Address) -> Option<PublishedCodeAttestation> {
        self.code_publication_registry.attestation(address)
    }

    pub fn get_storage_at(&mut self, address: Address, slot: U256) -> Result<U256> {
        Ok(self.evm.db.storage(address, slot)?)
    }

    pub fn block_by_hash(&self, hash: B256) -> Option<&Block> {
        self.chain.iter().find(|block| block.hash == hash)
    }

    pub fn simulate_call(
        &mut self,
        from: Address,
        to: Option<Address>,
        data: Bytes,
        gas_limit: u64,
        value: U256,
    ) -> Result<TxExecution> {
        let mut env = Env::default();
        env.cfg.chain_id = self.chain_id;
        env.block.number = U256::from(self.block_number);
        env.block.coinbase = self.coinbase;
        env.block.gas_limit = U256::from(self.gas_limit_per_block);
        env.block.basefee = self.base_fee;
        env.tx.caller = from;
        env.tx.gas_limit = gas_limit;
        env.tx.gas_price = self.base_fee;
        env.tx.nonce = None;
        env.tx.chain_id = None;
        env.tx.value = value;
        env.tx.data = data;
        env.tx.transact_to = match to {
            Some(addr) => TxKind::Call(addr),
            None => TxKind::Create,
        };

        let shared_orders = Arc::new(Mutex::new(self.orders.state.clone()));
        precompiles::set_mersennet_orders_context(shared_orders);
        precompiles::set_transparent_mersennet_orders_enabled(!self.privacy_mode_activated);

        // Privacy-redesign Phase 4 — install shielded EVM context.
        // Take ownership for the duration of the tx, restore after.
        let shielded_evm = std::mem::take(&mut self.shielded_evm);
        let shared_shielded = Arc::new(Mutex::new(shielded_evm));
        precompiles::set_shielded_evm_context(shared_shielded.clone());

        let mut evm = Evm::builder()
            .with_db(self.evm.db.clone())
            .with_spec_id(self.spec_id)
            .with_env(Box::new(env))
            .append_handler_register(precompiles::register_mersennet_orders_precompile)
            .append_handler_register(precompiles::register_shielded_precompiles)
            .build();

        let result = evm.transact_preverified()?;
        precompiles::clear_mersennet_orders_context();
        precompiles::set_transparent_mersennet_orders_enabled(true);
        precompiles::clear_shielded_evm_context();
        drop(evm);
        self.shielded_evm = Arc::try_unwrap(shared_shielded)
            .expect("no other Arc references to shielded_evm")
            .into_inner()
            .expect("shielded_evm mutex not poisoned");

        match result.result {
            ExecutionResult::Success {
                gas_used,
                output,
                logs,
                ..
            } => {
                let created_address = output.address().cloned();
                Ok(TxExecution {
                    success: true,
                    gas_used,
                    output: output.into_data(),
                    created_address,
                    logs: logs
                        .into_iter()
                        .map(|log| LogEntry {
                            address: log.address,
                            topics: log.data.topics().to_vec(),
                            data: log.data.data.clone(),
                        })
                        .collect(),
                })
            }
            ExecutionResult::Revert { gas_used, output } => Ok(TxExecution {
                success: false,
                gas_used,
                output,
                created_address: None,
                logs: Vec::new(),
            }),
            ExecutionResult::Halt { gas_used, .. } => Ok(TxExecution {
                success: false,
                gas_used,
                output: Bytes::new(),
                created_address: None,
                logs: Vec::new(),
            }),
        }
    }

    pub fn deploy_contract(
        &mut self,
        from: Address,
        bytecode: Bytes,
        gas_limit: u64,
        gas_price: U256,
        nonce: u64,
        value: U256,
    ) -> Result<()> {
        let result = self.submit_tx_unsigned(Transaction {
            from,
            to: None,
            value,
            data: bytecode,
            gas_limit,
            gas_price,
            nonce,
            chain_id: Some(self.chain_id),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
        });
        result.map_err(|err| anyhow::anyhow!("tx rejected: {} ({})", err.code(), err))
    }

    #[allow(dead_code)]
    pub fn call_contract(
        &mut self,
        from: Address,
        to: Address,
        data: Bytes,
        gas_limit: u64,
        value: U256,
    ) -> Result<Bytes> {
        let mut env = Env::default();
        env.cfg.chain_id = self.chain_id;
        env.block.number = U256::from(self.block_number);
        env.block.coinbase = self.coinbase;
        env.block.gas_limit = U256::from(self.gas_limit_per_block);
        env.block.basefee = self.base_fee;
        env.tx.caller = from;
        env.tx.gas_limit = gas_limit;
        env.tx.gas_price = self.base_fee;
        env.tx.nonce = None;
        env.tx.chain_id = None;
        env.tx.value = value;
        env.tx.data = data;
        env.tx.transact_to = TxKind::Call(to);

        let shared_orders = Arc::new(Mutex::new(self.orders.state.clone()));
        precompiles::set_mersennet_orders_context(shared_orders);
        precompiles::set_transparent_mersennet_orders_enabled(!self.privacy_mode_activated);

        let shielded_evm = std::mem::take(&mut self.shielded_evm);
        let shared_shielded = Arc::new(Mutex::new(shielded_evm));
        precompiles::set_shielded_evm_context(shared_shielded.clone());

        let mut evm = Evm::builder()
            .with_db(self.evm.db.clone())
            .with_spec_id(self.spec_id)
            .with_env(Box::new(env))
            .append_handler_register(precompiles::register_mersennet_orders_precompile)
            .append_handler_register(precompiles::register_shielded_precompiles)
            .build();

        let result = evm.transact_preverified()?;
        precompiles::clear_mersennet_orders_context();
        precompiles::set_transparent_mersennet_orders_enabled(true);
        precompiles::clear_shielded_evm_context();
        drop(evm);
        self.shielded_evm = Arc::try_unwrap(shared_shielded)
            .expect("no other Arc references to shielded_evm")
            .into_inner()
            .expect("shielded_evm mutex not poisoned");

        Ok(result.result.output().cloned().unwrap_or_default())
    }

    #[allow(dead_code)]
    pub fn call_u64(
        &mut self,
        from: Address,
        to: Address,
        signature: &str,
        gas_limit: u64,
    ) -> Result<Option<u64>> {
        let data = Bytes::copy_from_slice(&abi::selector(signature));
        let output = self.call_contract(from, to, data, gas_limit, U256::ZERO)?;
        Ok(abi::decode_u64(&output))
    }

    pub fn transfer(
        &mut self,
        from: Address,
        to: Address,
        value: U256,
        gas_limit: u64,
        gas_price: U256,
        nonce: u64,
    ) -> Result<()> {
        let result = self.submit_tx_unsigned(Transaction {
            from,
            to: Some(to),
            value,
            data: Bytes::new(),
            gas_limit,
            gas_price,
            nonce,
            chain_id: Some(self.chain_id),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
        });
        result.map_err(|err| anyhow::anyhow!("tx rejected: {} ({})", err.code(), err))
    }

    pub fn submit_batch_order(&mut self, order: BatchOrder) {
        self.fba_engine.submit_order(order);
    }

    pub fn execute_batch_auctions(&mut self) -> Vec<AuctionResult> {
        let results = self.fba_engine.execute_all();
        self.fba_engine
            .apply_results(&results, &mut self.orders.state);
        results
    }

    pub fn commit_tx(&mut self, commitment: TxCommitment) -> Result<(), CommitRevealError> {
        self.commit_reveal.commit(commitment)
    }

    pub fn reveal_tx(&mut self, reveal: TxReveal) -> Result<Bytes, CommitRevealError> {
        self.commit_reveal.reveal(reveal)
    }

    fn validate_tx_basic(&self, tx: &Transaction) -> Result<(), TxRejection> {
        self.validate_tx_basic_no_sig(tx)?;
        self.verify_tx_signature(tx)?;
        Ok(())
    }

    fn validate_tx_basic_no_sig(&self, tx: &Transaction) -> Result<(), TxRejection> {
        if tx.gas_limit > self.gas_limit_per_block {
            return Err(TxRejection::GasLimitTooHigh);
        }
        if let Some(chain_id) = tx.chain_id
            && chain_id != self.chain_id
        {
            return Err(TxRejection::InvalidChainId);
        }
        Ok(())
    }

    fn verify_tx_signature(&self, tx: &Transaction) -> Result<(), TxRejection> {
        let Some((r, s, v)) = &tx.signature else {
            return Err(TxRejection::InvalidSignature(
                "unsigned transactions are not accepted".to_string(),
            ));
        };

        let signed = SignedTransaction {
            tx: tx.clone(),
            v: U256::from(*v),
            r: *r,
            s: *s,
            tx_type: 0,
        };

        let recovered = crypto::recover_signer(&signed)
            .map_err(|e| TxRejection::InvalidSignature(e.to_string()))?;

        if recovered != tx.from {
            return Err(TxRejection::InvalidSignature(format!(
                "signer {recovered} does not match from {}",
                tx.from
            )));
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn validate_tx_state(&mut self, tx: &Transaction) -> Result<(), TxRejection> {
        let account = self
            .evm
            .db
            .basic(tx.from)
            .map_err(|_| TxRejection::DatabaseError)?
            .unwrap_or_default();
        if tx.nonce < account.nonce {
            return Err(TxRejection::NonceTooLow);
        }

        if tx.gas_price < self.base_fee {
            return Err(TxRejection::GasPriceTooLow);
        }

        let gas_cost = U256::from(tx.gas_limit).saturating_mul(tx.gas_price);
        let total_cost = gas_cost.saturating_add(tx.value);
        if account.balance < total_cost {
            return Err(TxRejection::InsufficientBalance);
        }

        Ok(())
    }

    /// Dispatch a shielded (`tx_type == 0x7E`) transaction to the
    /// appropriate subsystem. Returns a synthetic [`TxExecution`] so
    /// the receipt list keeps a consistent shape with EVM-side txs.
    ///
    /// **Pre-fork behaviour**: if `privacy_mode_activated == false`,
    /// the tx is recorded as a failed execution with a deterministic
    /// error log. This is the safe default for chains pinned to the
    /// pre-privacy fork. See ADR-018.
    pub fn apply_shielded_tx(&mut self, tx: &Transaction) -> TxExecution {
        if !self.privacy_mode_activated {
            return shielded_failed_execution("privacy_mode_inactive");
        }
        let Some(envelope) = tx.shielded_payload.as_ref() else {
            return shielded_failed_execution("shielded_tx_missing_payload");
        };

        let outcome = match envelope {
            ShieldedEnvelope::Transfer(t) => self
                .shielded_evm
                .apply_shielded_transfer(t)
                .map(|_| ())
                .map_err(|e| format!("shielded_transfer:{e:?}")),
            ShieldedEnvelope::Shield(t) => self
                .shielded_evm
                .apply_shield(t)
                .map(|_| ())
                .map_err(|e| format!("shield:{e:?}")),
            ShieldedEnvelope::Unshield(t) => self
                .shielded_evm
                .apply_unshield(t)
                .map(|_| ())
                .map_err(|e| format!("unshield:{e:?}")),
            ShieldedEnvelope::Order(_) | ShieldedEnvelope::LiquidationClaim(_) => {
                // Shielded order admission happens through the
                // threshold-mempool drain at the FBA tick (A5), not
                // the per-tx path; liquidation claims arrive through
                // their own sealed-bid path. Either is a usage error
                // when seen as a 0x7E tx.
                Err("shielded_order_or_claim_via_evm_tx_path".to_string())
            }
            ShieldedEnvelope::LiquidationExecute(exec) => self
                .liquidation_auction
                .execute(&mut self.shielded_evm.state, (**exec).clone())
                .map(|_| ())
                .map_err(|e| format!("liquidation_execute:{e:?}")),
        };

        let success = outcome.is_ok();
        let mut logs = Vec::new();
        let topic = if success {
            B256::from(keccak256(b"ShieldedTxApplied(uint8)"))
        } else {
            B256::from(keccak256(b"ShieldedTxRejected(uint8,string)"))
        };
        logs.push(LogEntry {
            address: Address::ZERO,
            topics: vec![
                topic,
                B256::from_slice(&{
                    let mut padded = [0u8; 32];
                    padded[31] = envelope.discriminant();
                    padded
                }),
            ],
            data: outcome
                .as_ref()
                .err()
                .map(|s| Bytes::from(s.clone().into_bytes()))
                .unwrap_or_default(),
        });

        TxExecution {
            success,
            // Fixed shielded gas charge; refined in Workstream B
            // when the precompile-gas table goes live.
            gas_used: 50_000,
            output: Bytes::new(),
            created_address: None,
            logs,
        }
    }

    /// Compute the shielded portion of the block header. Returns a
    /// 4-tuple `(shielded_state_root, nullifier_root,
    /// shielded_event_root, state_proof)`.
    ///
    /// The digests are taken from the live shielded subsystems and the
    /// proof is generated via [`crate::state_proof::prove_block`]
    /// (mock prover today, real SP1 once `mersennet-zkp/sp1` is
    /// enabled in Workstream E).
    ///
    /// **Proof-only mode (pre-fork)**: proofs are generated and
    /// attached from genesis so the chain is verifiable before the
    /// privacy hard fork, WITHOUT flipping any privacy gating —
    /// transparent RPC, precompiles and subscriptions stay enabled
    /// until `privacy_mode_activated` is true. Pre-fork blocks carry
    /// no shielded txs and no tick, so each proof attests an empty
    /// shielded state transition (root continuity from block to
    /// block).
    fn shielded_block_header(
        &self,
        transactions: &[Transaction],
        header: &mersennet_zkp::sp1::BlockHeaderWitness,
        _gas_used: u64,
        pre_shielded_snapshot: Option<&crate::shielded_state::ShieldedSnapshot>,
        pre_transparent_balances: Option<&HashMap<Address, U256>>,
        pre_tick_witness: Option<&mersennet_zkp::sp1::ShieldedTickWitness>,
    ) -> Result<(
        B256,
        B256,
        B256,
        Option<crate::zk_proofs::StateTransitionProof>,
    )> {
        // The canonical block hash is fully determined by the header
        // witness, so the SP1 proof binds to exactly the hash the block
        // carries.
        let block_hash = B256::from(mersennet_zkp::sp1::derive_block_hash(
            self.block_number,
            header,
        ));
        let shielded_state_root = B256::from(self.shielded_evm.state.current_root().to_bytes());
        // Cheap nullifier-root digest: a real SMT lives in
        // ShieldedState (Workstream A7 persistence) — for header
        // commitment purposes we hash the canonical count.
        let mut nbuf = Vec::with_capacity(16);
        nbuf.extend_from_slice(&(self.shielded_evm.state.nullifier_count() as u64).to_le_bytes());
        let nullifier_root = keccak256(&nbuf);

        // Shielded event root: reuse the canonical root pinned in
        // `run_shielded_tick`, which hashes the same
        // `CanonicalShieldedEvent` list the SP1 executor re-derives.
        // Hashing `DomainEvent` bincode here instead would use a
        // different encoding and make `prove_block`'s event-root
        // equality check fail on every block with shielded activity.
        let host_shielded_event_root = if self.privacy_mode_activated {
            self.shielded_tick_event_root
        } else {
            // Proof-only mode: no shielded tick ran, so re-derive the
            // canonical event list the SP1 executor rebuilds for an
            // empty block (a single ShieldedRootAdvanced entry) instead
            // of using the never-pinned tick root.
            let events = mersennet_zkp::sp1::build_shielded_tick_events(
                self.block_number,
                0,
                &[],
                &[],
                self.shielded_evm.state.current_root().to_bytes(),
            );
            B256::from(mersennet_zkp::sp1::shielded_event_root(&events))
        };
        let market_state_hash = crate::state_proof::snapshot_subsystem_digests(
            &self.shielded_orders,
            &self.liquidation_auction,
            &self.shielded_evm,
        )
        .market_state_hash;

        // Drive the (mock) SP1 prover. Returns `None` when proving
        // fails — the block is still produced, but it won't pass the
        // light-client gate. Real validators will gate finalisation
        // on `state_proof.is_some()` post-fork.
        let request = crate::state_proof::BlockProofRequest {
            block_number: self.block_number,
            timestamp: 0,
            prev_state_root: self
                .chain
                .last()
                .map(|b| b.shielded_state_root)
                .unwrap_or(B256::ZERO),
            prev_nullifier_root: self
                .chain
                .last()
                .map(|b| b.nullifier_root)
                .unwrap_or(B256::ZERO),
            header: header.clone(),
            txs: crate::state_proof::encode_block_txs(transactions),
            prev_market_state: Vec::new(),
            block_hash,
            expected_market_state_hash: market_state_hash,
            prev_shielded_state: pre_shielded_snapshot
                .map(crate::state_proof::shielded_state_witness)
                .unwrap_or_default(),
            transparent_balances: pre_transparent_balances
                .map(crate::state_proof::transparent_balance_entries)
                .unwrap_or_default(),
            pre_tick_witness: pre_tick_witness.cloned().unwrap_or_default(),
        };
        let state_proof_result = crate::state_proof::prove_block(
            &request,
            &self.shielded_evm.state,
            host_shielded_event_root,
        );
        let proof_required = self.sp1_proof_required;
        let state_proof = match state_proof_result {
            Ok(proof) => Some(proof),
            Err(err) if proof_required => {
                return Err(anyhow::anyhow!(
                    "mandatory SP1 proof generation failed: {err}"
                ));
            }
            Err(err) => {
                tracing::warn!(error = ?err, block = self.block_number, "SP1 proof generation failed; falling back to host-derived shielded header fields");
                None
            }
        };

        let (shielded_state_root, nullifier_root, shielded_event_root) = state_proof
            .as_ref()
            .map(|proof| {
                (
                    proof.new_state_root,
                    proof.new_nullifier_root,
                    proof.shielded_event_root,
                )
            })
            .unwrap_or((
                shielded_state_root,
                nullifier_root,
                host_shielded_event_root,
            ));

        Ok((
            shielded_state_root,
            nullifier_root,
            shielded_event_root,
            state_proof,
        ))
    }

    /// Run the per-block shielded tick:
    ///
    /// 1. Drain decrypted intents from the threshold-encrypted
    ///    mempool (a no-op until the BLS provider in D3 is wired).
    /// 2. Iterate every shielded market and run its uniform-price
    ///    auction. Emit one [`ShieldedEvent::FbaCleared`] per market
    ///    that traded a non-zero clearing volume.
    /// 3. Settle the block's sealed-bid liquidation auctions. Emit
    ///    one [`ShieldedEvent::LiquidationSettled`] per winner.
    /// 4. Emit a [`ShieldedEvent::ShieldedRootAdvanced`] summarising
    ///    the post-tick canonical state. Light clients sync off this.
    fn run_shielded_tick(&mut self) {
        let admitted_intent_count = self.admit_decrypted_threshold_orders();
        let fba_events = self.run_shielded_market_auctions();
        let liquidation_events = self.settle_shielded_liquidations();

        // Step 4 — emit the new shielded-state root for light clients.
        let new_root = self.shielded_evm.state.current_root().to_bytes();
        metrics::counter!("mersennet_shielded_root_advanced_total", 1);
        let snap = self.shielded_evm.state.snapshot();
        metrics::gauge!("mersennet_shielded_notes_total", snap.leaves.len() as f64);
        metrics::gauge!(
            "mersennet_shielded_nullifiers_total",
            snap.nullifiers.len() as f64
        );
        metrics::gauge!(
            "mersennet_privacy_mode_active",
            if self.privacy_mode_activated {
                1.0
            } else {
                0.0
            }
        );
        metrics::gauge!(
            "mersennet_privacy_activation_height",
            self.privacy_activation_height.unwrap_or(0) as f64
        );
        let events = build_shielded_tick_events(
            self.block_number,
            admitted_intent_count,
            &fba_events,
            &liquidation_events,
            new_root,
        );
        // Pin the canonical event root from the exact list the SP1
        // executor re-derives, so `prove_block`'s equality check
        // against the host root can actually succeed on blocks with
        // shielded activity.
        self.shielded_tick_event_root =
            B256::from(mersennet_zkp::sp1::shielded_event_root(&events));
        self.pending_events.extend(
            events
                .into_iter()
                .map(Self::domain_event_from_canonical_shielded_event),
        );
    }

    fn admit_decrypted_threshold_orders(&mut self) -> u64 {
        // Step 1 — drain decrypted intents and replay canonical
        // order admission from the recovered plaintext payloads.
        let mut drained = self.threshold_mempool.drain_decrypted(usize::MAX);
        drained.sort_by_key(|(intent_id, _)| intent_id.0);
        if !drained.is_empty() {
            metrics::counter!(
                "mersennet_threshold_mempool_admitted_total",
                drained.len() as u64
            );
            for (intent_id, plaintext) in &drained {
                let payload = match decode_threshold_order_intent(plaintext) {
                    Ok(payload) => payload,
                    Err(err) => {
                        tracing::warn!(intent_id = %hex::encode(intent_id.as_slice()), error = ?err, "failed to decode decrypted order intent");
                        continue;
                    }
                };
                let oracle_price = self
                    .shielded_orders
                    .oracle_price_for_market(payload.tx.market_id)
                    .unwrap_or_default();
                if let Err(err) = self.shielded_orders.admit_intent(
                    &mut self.shielded_evm.state,
                    payload.tx,
                    payload.intent,
                    oracle_price,
                ) {
                    tracing::warn!(intent_id = %hex::encode(intent_id.as_slice()), error = ?err, "shielded order admission failed");
                }
            }
        }
        metrics::gauge!(
            "mersennet_threshold_mempool_pending",
            self.threshold_mempool.pending_count() as f64
        );
        drained.len() as u64
    }

    fn run_shielded_market_auctions(&mut self) -> Vec<CanonicalShieldedEvent> {
        // Step 2 — uniform-price auction per market.
        let mut events = Vec::new();
        let market_ids = self.shielded_orders.market_ids_in_canonical_order();
        for market_id in market_ids {
            match self.shielded_orders.run_fba(market_id) {
                Ok(result) => {
                    if !result.matched_size.is_zero() {
                        metrics::counter!(
                            "mersennet_fba_cleared_total",
                            1,
                            "market_id" => market_id.0.to_string()
                        );
                        // Gauges carry only market-level aggregates;
                        // no per-trader labels (CI K2).
                        metrics::gauge!(
                            "mersennet_fba_clearing_price",
                            f64_from_u256(result.clearing_price),
                            "market_id" => market_id.0.to_string()
                        );
                        metrics::gauge!(
                            "mersennet_fba_matched_size",
                            f64_from_u256(result.matched_size),
                            "market_id" => market_id.0.to_string()
                        );
                        events.push(CanonicalShieldedEvent::FbaCleared {
                            market_id: market_id.0,
                            clearing_price: Self::u256_bytes(result.clearing_price),
                            matched_size: Self::u256_bytes(result.matched_size),
                            intent_count: result.fills.len() as u64,
                        });
                    }
                }
                Err(err) => {
                    tracing::warn!(market = %market_id.0, error = ?err, "shielded FBA failed");
                }
            }
        }
        events
    }

    fn settle_shielded_liquidations(&mut self) -> Vec<CanonicalShieldedEvent> {
        // Step 3 — settle sealed-bid liquidations. Runtime and zk replay
        // now consume the same witness-level settlement result, including
        // canonical event emission.
        let settlement = self
            .liquidation_auction
            .settle_block_witness(self.block_number);
        for winner in &settlement.winners {
            metrics::counter!("mersennet_liquidation_auctions_settled_total", 1);
            let _ = winner;
        }
        metrics::gauge!(
            "mersennet_liquidator_count",
            self.liquidation_auction.liquidators.len() as f64
        );
        settlement.events
    }

    fn domain_event_from_canonical_shielded_event(event: CanonicalShieldedEvent) -> DomainEvent {
        DomainEvent::Shielded(match event {
            CanonicalShieldedEvent::FbaCleared {
                market_id,
                clearing_price,
                matched_size,
                intent_count,
            } => crate::events::ShieldedEvent::FbaCleared {
                market_id: MarketId(market_id),
                clearing_price: Self::u256_from_bytes(clearing_price),
                matched_size: Self::u256_from_bytes(matched_size),
                intent_count,
            },
            CanonicalShieldedEvent::MempoolBatchAdmitted {
                block_number,
                intent_count,
            } => crate::events::ShieldedEvent::MempoolBatchAdmitted {
                block_number,
                intent_count,
            },
            CanonicalShieldedEvent::LiquidationSettled {
                market_id,
                winner_bond_commitment,
                winning_bid,
            } => crate::events::ShieldedEvent::LiquidationSettled {
                market_id: MarketId(market_id),
                winner_bond_commitment,
                winning_bid: Self::u256_from_bytes(winning_bid),
            },
            CanonicalShieldedEvent::ShieldedRootAdvanced {
                block_number,
                new_root,
                notes_added,
                nullifiers_added,
            } => crate::events::ShieldedEvent::ShieldedRootAdvanced {
                block_number,
                new_root,
                notes_added,
                nullifiers_added,
            },
        })
    }

    fn u256_bytes(value: U256) -> U256Bytes {
        let mut out = [0u8; 32];
        for (index, limb) in value.as_limbs().iter().enumerate() {
            out[index * 8..(index + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        U256Bytes(out)
    }

    fn u256_from_bytes(value: U256Bytes) -> U256 {
        let limbs = value
            .0
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        U256::from_limbs([limbs[0], limbs[1], limbs[2], limbs[3]])
    }

    fn execute_tx(&mut self, tx: &Transaction) -> Result<TxExecution> {
        let mut env = Env::default();
        env.cfg.chain_id = self.chain_id;
        env.block.number = U256::from(self.block_number);
        env.block.coinbase = self.coinbase;
        env.block.gas_limit = U256::from(self.gas_limit_per_block);
        env.block.basefee = self.base_fee;
        env.tx.caller = tx.from;
        env.tx.gas_limit = tx.gas_limit;
        env.tx.gas_price = tx.gas_price;
        env.tx.nonce = Some(tx.nonce);
        env.tx.chain_id = Some(tx.chain_id.unwrap_or(self.chain_id));
        env.tx.value = tx.value;
        env.tx.data = tx.data.clone();
        env.tx.transact_to = match tx.to {
            Some(to) => TxKind::Call(to),
            None => TxKind::Create,
        };

        // Privacy-redesign Phase 4 — install shielded EVM context.
        let shielded_evm = std::mem::take(&mut self.shielded_evm);
        let shared_shielded = Arc::new(Mutex::new(shielded_evm));
        precompiles::set_shielded_evm_context(shared_shielded.clone());
        let publication_registry = std::mem::take(&mut self.code_publication_registry);
        let shared_publication = Arc::new(Mutex::new(publication_registry));
        // The publication precompile only reads code hashes; snapshot just
        // those instead of cloning the whole database (see
        // `CodePublicationContext::code_hashes`).
        let code_hashes: std::collections::HashMap<Address, B256> = self
            .evm
            .db
            .accounts
            .iter()
            .map(|(addr, acc)| (*addr, acc.info.code_hash))
            .collect();
        precompiles::set_code_publication_context(
            shared_publication.clone(),
            code_hashes,
            self.block_number,
        );

        // Move the database into the EVM rather than cloning it — a full
        // `InMemoryDB` clone per transaction dominated execution time. The
        // db is restored below on every path (including errors) before `?`
        // can propagate.
        let mut evm = Evm::builder()
            .with_db(std::mem::take(&mut self.evm.db))
            .with_spec_id(self.spec_id)
            .with_env(Box::new(env))
            .append_handler_register(precompiles::register_mersennet_orders_precompile)
            .append_handler_register(precompiles::register_shielded_precompiles)
            .append_handler_register(precompiles::register_code_publication_precompile)
            .build();

        precompiles::set_transparent_mersennet_orders_enabled(!self.privacy_mode_activated);
        let result = evm.transact_commit();
        self.evm.db = std::mem::take(&mut evm.context.evm.db);
        precompiles::set_transparent_mersennet_orders_enabled(true);
        precompiles::clear_shielded_evm_context();
        precompiles::clear_code_publication_context();
        drop(evm);
        self.shielded_evm = Arc::try_unwrap(shared_shielded)
            .expect("no other Arc references to shielded_evm")
            .into_inner()
            .expect("shielded_evm mutex not poisoned");
        self.code_publication_registry = Arc::try_unwrap(shared_publication)
            .expect("no other Arc references to code_publication_registry")
            .into_inner()
            .expect("code_publication_registry mutex not poisoned");
        // Propagate an execution error only after the db and contexts have
        // been restored, so a failed tx can never wipe engine state.
        let result = result?;
        self.evm.state.mark_dirty(tx.from);
        if let Some(to) = tx.to {
            self.evm.state.mark_dirty(to);
        }
        self.evm.state.mark_dirty(self.coinbase);
        let execution = match result {
            ExecutionResult::Success {
                gas_used,
                output,
                logs,
                ..
            } => TxExecution {
                success: true,
                gas_used,
                output: output.clone().into_data(),
                created_address: output.address().cloned(),
                logs: logs
                    .into_iter()
                    .map(|log| LogEntry {
                        address: log.address,
                        topics: log.data.topics().to_vec(),
                        data: log.data.data.clone(),
                    })
                    .collect(),
            },
            ExecutionResult::Revert { gas_used, output } => TxExecution {
                success: false,
                gas_used,
                output,
                created_address: None,
                logs: Vec::new(),
            },
            ExecutionResult::Halt { gas_used, .. } => TxExecution {
                success: false,
                gas_used,
                output: Bytes::new(),
                created_address: None,
                logs: Vec::new(),
            },
        };

        if let Some(addr) = &execution.created_address {
            self.evm.state.mark_dirty(*addr);
            self.code_publication_registry
                .record_deployment(*addr, tx.from);
        }

        Ok(execution)
    }

    #[allow(clippy::too_many_arguments)]
    /// Build the canonical block-header witness. `derive_block_hash`
    /// over this witness is the block hash; the same witness is fed to
    /// the SP1 proof so the proof binds to exactly that hash.
    #[allow(clippy::too_many_arguments)]
    fn header_witness(
        &self,
        gas_used: u64,
        tx_count: u64,
        parent_hash: B256,
        timestamp: u64,
        tx_root: B256,
        state_root: B256,
        receipts_root: B256,
    ) -> mersennet_zkp::sp1::BlockHeaderWitness {
        let mut coinbase_bytes = [0u8; 20];
        coinbase_bytes.copy_from_slice(self.coinbase.as_slice());
        mersennet_zkp::sp1::BlockHeaderWitness {
            chain_id: self.chain_id,
            gas_limit: self.gas_limit_per_block,
            gas_used,
            base_fee_be: self.base_fee.to_be_bytes::<32>(),
            coinbase: coinbase_bytes,
            tx_count,
            parent_hash: parent_hash.0,
            timestamp,
            tx_root: tx_root.0,
            state_root: state_root.0,
            receipts_root: receipts_root.0,
        }
    }

    /// Keccak Merkle-ish root over the ordered transaction hashes. A
    /// simple sequential keccak accumulator is sufficient to bind the
    /// exact transaction list + order into the block hash.
    fn compute_tx_root(transactions: &[Transaction]) -> B256 {
        if transactions.is_empty() {
            return B256::ZERO;
        }
        let mut buf = Vec::with_capacity(transactions.len() * 32);
        for tx in transactions {
            buf.extend_from_slice(crate::crypto::tx_signing_hash(tx).as_slice());
        }
        keccak256(&buf)
    }

    /// Keccak accumulator over the receipts (success + gas + output) so
    /// the block hash binds execution results, not just inputs.
    fn compute_receipts_root(receipts: &[Receipt]) -> B256 {
        if receipts.is_empty() {
            return B256::ZERO;
        }
        let mut buf = Vec::new();
        for r in receipts {
            buf.push(if r.success { 1u8 } else { 0u8 });
            buf.extend_from_slice(&r.gas_used.to_be_bytes());
            buf.extend_from_slice(r.output.as_ref());
        }
        keccak256(&buf)
    }

    /// Sum of every account balance currently held in state. The full
    /// ledger is mirrored in memory (`load_into_db` loads all accounts
    /// from the backing store), so this is a complete total-supply
    /// figure used by the per-block conservation-of-value invariant.
    fn sum_all_balances(&self) -> U256 {
        self.evm
            .db
            .accounts
            .values()
            .fold(U256::ZERO, |acc, account| {
                acc.saturating_add(account.info.balance)
            })
    }

    fn apply_rewards(&mut self, rewards: &[Reward]) -> Result<()> {
        for reward in rewards {
            if reward.amount.is_zero() {
                continue;
            }
            // Delegated-staking split: delegators earn a pro-rata share of
            // the validator's reward (minus commission). The cut is credited
            // to the staking escrow; delegators claim it via the precompile.
            // Deterministic across produce/import (same state, same stake).
            let delegator_cut = {
                let self_stake = self.stake_of(reward.address);
                self.orders
                    .state
                    .staking
                    .on_reward(reward.address, reward.amount, self_stake)
            };
            if !delegator_cut.is_zero() {
                self.evm
                    .state
                    .mark_dirty(crate::precompile_abi::STAKING_PRECOMPILE);
                let mut escrow = self
                    .evm
                    .db
                    .accounts
                    .get(&crate::precompile_abi::STAKING_PRECOMPILE)
                    .map(|account| account.info.clone())
                    .unwrap_or_default();
                escrow.balance = escrow.balance.saturating_add(delegator_cut);
                self.evm
                    .db
                    .insert_account_info(crate::precompile_abi::STAKING_PRECOMPILE, escrow);
            }
            let validator_amount = reward.amount.saturating_sub(delegator_cut);
            if validator_amount.is_zero() {
                continue;
            }
            // The validator's share goes to the operator wallet once
            // `rewards_to_operator_height` is reached (the node key is an
            // identity, not a wallet anyone wants to hold funds on); before
            // that, and for identities without a registration, to the
            // identity itself. Registry + height are state, so this is
            // identical on producer and importer.
            let recipient = self
                .orders
                .state
                .staking
                .reward_recipient(reward.address, self.block_number);
            self.evm.state.mark_dirty(recipient);
            // Read the current account straight from the cache rather
            // than via `basic()`. `basic()` lazily loads a missing
            // address as `AccountState::NotExisting`; a following
            // `insert_account_info` only overwrites `.info` and leaves
            // that state, so `basic()`/`get_balance` would then report
            // zero and each block would clobber (not accumulate) the
            // credit. Genesis-funded validators already sit in a normal
            // state, so this is behaviorally identical for them and
            // does not change the state root.
            let mut info = self
                .evm
                .db
                .accounts
                .get(&recipient)
                .map(|account| account.info.clone())
                .unwrap_or_default();
            info.balance = info.balance.saturating_add(validator_amount);
            self.evm.db.insert_account_info(recipient, info);
        }
        Ok(())
    }

    fn next_base_fee(&self, gas_used: u64) -> U256 {
        let target = self.fee_target_gas.max(1);
        if gas_used == target {
            return self.base_fee;
        }

        let gas_used_u256 = U256::from(gas_used);
        let target_u256 = U256::from(target);
        let base_fee = self.base_fee.max(U256::from(1u64));
        let denominator = U256::from(self.fee_max_change_denominator.max(1));

        if gas_used > target {
            let delta = gas_used_u256.saturating_sub(target_u256);
            let fee_delta = base_fee
                .saturating_mul(delta)
                .checked_div(target_u256)
                .unwrap_or(U256::ZERO)
                .checked_div(denominator)
                .unwrap_or(U256::ZERO);
            base_fee.saturating_add(fee_delta).max(U256::from(1u64))
        } else {
            let delta = target_u256.saturating_sub(gas_used_u256);
            let fee_delta = base_fee
                .saturating_mul(delta)
                .checked_div(target_u256)
                .unwrap_or(U256::ZERO)
                .checked_div(denominator)
                .unwrap_or(U256::ZERO);
            base_fee.saturating_sub(fee_delta).max(U256::from(1u64))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{DomainEvent, ShieldedEvent};
    use crate::mersennet_orders::{Market, MarketStatus};
    use crate::shielded_orders::{DecryptedIntent, ShieldedOrderTx, ThresholdOrderIntent};
    use mersennet_zkp::Fr;
    use mersennet_zkp::noir::{Circuit, MockVerifier};
    use mersennet_zkp::note::Note;
    #[cfg(feature = "sp1")]
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn fresh_inactive_engine() -> Engine {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("engine");
        std::fs::create_dir_all(&sub).unwrap();
        let engine = Engine::new_with_backend(131071, sub, "redb");
        std::mem::forget(dir);
        engine
    }

    fn fresh_engine() -> Engine {
        let mut engine = fresh_inactive_engine();
        engine.activate_privacy_mode();
        engine
    }

    #[cfg(feature = "sp1")]
    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[cfg(feature = "sp1")]
    struct EnvVarGuard {
        key: &'static str,
        value: Option<std::ffi::OsString>,
    }

    #[cfg(feature = "sp1")]
    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key,
                value: previous,
            }
        }
    }

    #[cfg(feature = "sp1")]
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.value {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    fn market(id: u64) -> Market {
        Market {
            id: MarketId(id),
            symbol: format!("M{id}"),
            tick_size: U256::from(10u64),
            lot_size: U256::from(1u64),
            last_price: U256::from(1_000u64),
            status: MarketStatus::Active,
        }
    }

    fn build_threshold_order_intent(
        engine: &Engine,
        market_id: MarketId,
        side: Side,
        price: U256,
        size: U256,
        owner_pk: Fr,
        seed: u64,
    ) -> ThresholdOrderIntent {
        let poseidon = &engine.shielded_orders.poseidon;
        let collateral = Note {
            value: 1_000_000_000,
            asset_id: 0,
            owner_pk,
            rho: Fr::from_u64(seed),
            psi: Fr::from_u64(seed + 1),
        };
        let spend_sk = Fr::from_u64(seed + 0xdead);
        let nullifier = collateral.nullifier(poseidon, &spend_sk).0;
        let new_collateral = Note {
            value: 999_900_000,
            asset_id: 0,
            owner_pk,
            rho: Fr::from_u64(seed + 100),
            psi: Fr::from_u64(seed + 101),
        };
        let new_commitment = new_collateral.commit(poseidon).0;
        let price_band =
            u32::try_from((price / engine.shielded_orders.price_tick).as_limbs()[0]).unwrap();
        let size_band =
            u32::try_from(size.div_ceil(engine.shielded_orders.size_lot).as_limbs()[0]).unwrap();
        let oracle_price = U256::from(1_000u64);
        let imm_required = engine
            .shielded_orders
            .derive_imm_required(market_id, price_band, size_band, oracle_price)
            .unwrap();
        let salt = Fr::from_u64(seed + 0xbeef);
        let side_fr = match side {
            Side::Buy => Fr::ZERO,
            Side::Sell => Fr::ONE,
        };
        let side_hash = poseidon.hash_two(&side_fr, &salt);
        let anchor_root = engine.shielded_evm.state.current_root();
        let public_inputs = vec![
            anchor_root,
            nullifier,
            new_commitment,
            Fr::from_u64(market_id.0),
            side_hash,
            Fr::from_u64(price_band as u64),
            Fr::from_u64(size_band as u64),
            Fr::from_u64(oracle_price.as_limbs()[0]),
            Fr::from_u64(imm_required.as_limbs()[0]),
        ];
        let proof = MockVerifier::new().prove(Circuit::OrderPlace, public_inputs);
        ThresholdOrderIntent {
            tx: ShieldedOrderTx {
                anchor_root,
                nullifier,
                new_commitment,
                market_id,
                side_hash,
                price_band,
                size_band,
                oracle_price,
                imm_required,
                encrypted_payload: Vec::new(),
                proof,
                tif: TimeInForce::Gtc,
            },
            intent: DecryptedIntent {
                side,
                price,
                size,
                owner_pk,
                salt,
            },
        }
    }

    #[test]
    fn run_shielded_tick_admits_decrypted_threshold_order_payload() {
        let mut engine = fresh_engine();
        let market = market(7);
        engine.shielded_orders.add_market(market.clone());
        engine
            .shielded_orders
            .markets
            .get_mut(&market.id)
            .unwrap()
            .last_price = U256::from(1_000u64);

        let threshold_intent = build_threshold_order_intent(
            &engine,
            market.id,
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            Fr::from_u64(0xa11ce),
            1,
        );
        let plaintext = bincode::serialize(&threshold_intent).unwrap();
        engine
            .threshold_mempool
            .insert_decrypted_for_test(B256::from([0x11; 32]), plaintext);

        engine.run_shielded_tick();

        let book = engine.shielded_orders.books.get(&market.id).unwrap();
        assert_eq!(book.bids.len(), 1);
        assert_eq!(book.total_bid_size(), U256::from(5u64));
        assert_eq!(engine.threshold_mempool.decrypted_count(), 0);
        assert!(engine.pending_events.iter().any(|event| matches!(
            event,
            DomainEvent::Shielded(ShieldedEvent::MempoolBatchAdmitted {
                intent_count: 1,
                ..
            })
        )));
    }

    #[cfg(feature = "sp1")]
    #[test]
    fn execute_block_fails_closed_when_privacy_fork_enables_required_sp1_proofs() {
        let _env_guard = env_lock().lock().unwrap();
        let _prove_adapter = EnvVarGuard::set(
            "MERSENNET_SP1_PROVE_ADAPTER",
            "definitely-not-a-real-sp1-prover",
        );
        let _verify_adapter = EnvVarGuard::set(
            "MERSENNET_SP1_VERIFY_ADAPTER",
            "definitely-not-a-real-sp1-verifier",
        );
        let _mode = EnvVarGuard::set("MERSENNET_SP1_MODE", "local");

        let mut engine = fresh_inactive_engine();
        engine.block_number = 3;
        engine.set_privacy_activation_height(3);

        let error = engine.execute_block().unwrap_err();

        assert!(engine.privacy_mode_activated());
        assert!(engine.sp1_proof_required);
        assert!(
            error
                .to_string()
                .contains("mandatory SP1 proof generation failed"),
            "unexpected error: {error:#}"
        );
    }
}

#[cfg(test)]
mod reorg_tests {
    use super::*;
    use revm::primitives::{Address, U256};
    use tempfile::tempdir;

    fn engine() -> Engine {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("engine");
        std::fs::create_dir_all(&sub).unwrap();
        let e = Engine::new_with_backend(131071, sub, "redb");
        std::mem::forget(dir);
        e
    }

    /// Four deterministic validator keys; imports authenticate the proposer
    /// signature against the leader schedule, so blocks must be signed by
    /// the elected leader (`leader_for_height` = sorted addresses, (h+r)%4).
    fn keys() -> Vec<k256::ecdsa::SigningKey> {
        (1u8..=4)
            .map(|b| k256::ecdsa::SigningKey::from_bytes(&[b; 32].into()).unwrap())
            .collect()
    }

    fn validators() -> Vec<Address> {
        keys()
            .iter()
            .map(crate::crypto::address_from_signing_key)
            .collect()
    }

    fn with_validators(mut e: Engine) -> Engine {
        for v in validators() {
            e.add_validator(v, U256::from(1_000_000u64)).unwrap();
        }
        e
    }

    /// Produce a block as the leader elected for the engine's next height at
    /// round `round`, signing it like the node binary does.
    fn produce_as_leader(e: &mut Engine, round: u64) -> Block {
        let leader = e.leader_for_height(e.block_number, round).expect("leader");
        let key = keys()
            .into_iter()
            .find(|k| crate::crypto::address_from_signing_key(k) == leader)
            .expect("leader key");
        // `self.coinbase` stays Address::ZERO on every node (production
        // invariant: the import path re-executes with the local coinbase, so
        // it must be identical everywhere); only the block's fields name the
        // proposer.
        e.set_local_validator(leader);
        let mut b = e.execute_block().expect("produce");
        b.proposer = leader;
        b.coinbase = leader;
        b.consensus.proposer = leader;
        let (r, s_, v) = crate::crypto::sign_block_proposal(b.number, b.hash, &key);
        b.proposer_sig = Some((r, s_, v));
        // The engine keeps its own copy of the block in `chain`; mirror the
        // signature there too, as the producer path does before broadcast.
        if let Some(last) = e.chain.last_mut()
            && last.number == b.number
        {
            last.proposer = leader;
            last.proposer_sig = b.proposer_sig;
        }
        b
    }

    /// Two nodes produce different blocks at height 1 (different coinbase).
    /// Node A applied its own; finality (3 of 4 validators) lands on B's block:
    /// A must roll back and adopt B's block and state.
    #[test]
    fn finality_on_a_different_hash_reorgs_the_local_block() {
        let vals = validators();
        let alice = Address::from_slice(&[0x11; 20]);
        let bob = Address::from_slice(&[0x22; 20]);
        let carol = Address::from_slice(&[0x33; 20]);
        let mut a = with_validators(engine());
        let mut b = with_validators(engine());
        for e in [&mut a, &mut b] {
            e.fund_account(alice, U256::from(2_000_000u64), 0);
        }
        // A's block pays bob; B's block pays carol — same height, different
        // blocks, different resulting states. A proposes at round 0, B is
        // the round-1 leader (a leader-timeout race, as in production).
        a.transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            0,
        )
        .unwrap();
        b.transfer(
            alice,
            carol,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            0,
        )
        .unwrap();
        let block_a = produce_as_leader(&mut a, 0);
        let block_b = produce_as_leader(&mut b, 1);
        assert_eq!(block_a.number, 1);
        assert_eq!(block_b.number, 1);
        assert_ne!(block_a.hash, block_b.hash);
        assert_eq!(a.latest_height(), 1);
        assert_eq!(a.get_balance(bob).unwrap(), U256::from(1_000u64));
        assert_eq!(a.get_balance(carol).unwrap(), U256::ZERO);
        let root_b = b.evm.state.compute_state_root();
        assert_ne!(
            a.evm.state.compute_state_root(),
            root_b,
            "the two branches differ in state"
        );

        // A hears B's competing proposal for a height it already holds.
        a.import_block(block_b.clone());
        assert_eq!(
            a.block_by_number(1).unwrap().hash,
            block_a.hash,
            "competitor is remembered, not applied"
        );

        // Finality: three validators vote for B's block.
        for v in vals.iter().take(3) {
            a.record_finality_vote(1, block_b.hash, *v);
        }
        assert_eq!(a.finalized_hash(1), Some(block_b.hash));

        // A rolled back and re-applied the canonical block: bob's payment is
        // undone, carol's applied, state identical to the canonical producer.
        assert_eq!(
            a.latest_height(),
            1,
            "back at height 1 on the canonical block"
        );
        assert_eq!(
            a.block_by_number(1).unwrap().hash,
            block_b.hash,
            "local head is now the finalized block"
        );
        assert_eq!(
            a.get_balance(bob).unwrap(),
            U256::ZERO,
            "orphaned branch undone"
        );
        assert_eq!(
            a.get_balance(carol).unwrap(),
            U256::from(1_000u64),
            "canonical branch applied"
        );
        // Compare with an importer that never saw the orphan: the reorged
        // node must end in exactly the state a clean follower has.
        let mut c = with_validators(engine());
        c.fund_account(alice, U256::from(2_000_000u64), 0);
        c.import_block(block_b.clone());
        assert_eq!(c.block_by_number(1).unwrap().hash, block_b.hash);
        assert_eq!(
            a.evm.state.compute_state_root(),
            c.evm.state.compute_state_root(),
            "reorged node matches a clean follower"
        );
        assert_eq!(
            a.evm.state.compute_state_root(),
            root_b,
            "and the canonical producer"
        );
        assert_eq!(a.block_number, 2);
        // The orphaned block's transaction went back to the mempool (same
        // nonce as carol's transfer, so it is rejected once that is applied).
        assert!(a.mempool_queued_count() <= 1);
    }

    /// Votes can arrive before the block they finalize. Once a height is
    /// finalized the node must not produce a competing block for it.
    #[test]
    fn no_production_for_an_already_finalized_height() {
        let vals = validators();
        let mut a = with_validators(engine());
        a.set_local_validator(vals[0]);
        let canonical = B256::from([7u8; 32]);
        for v in vals.iter().take(3) {
            a.record_finality_vote(1, canonical, *v);
        }
        assert_eq!(a.finalized_hash(1), Some(canonical));
        assert_eq!(a.latest_height(), 0, "we never saw the block itself");
        let err = a
            .execute_block()
            .expect_err("must refuse to produce height 1");
        assert!(err.to_string().contains("already finalized"), "{err}");
        assert_eq!(a.latest_height(), 0);
    }

    /// A validator that voted on a fork and then healed onto the canonical
    /// branch votes again for the same height with the canonical hash. That
    /// is not equivocation: nothing is slashed, and the canonical vote
    /// supersedes the stale one so the quorum can still complete.
    #[test]
    fn heal_then_vote_produces_no_evidence_and_supersedes() {
        let vals = validators();
        let mut a = with_validators(engine());
        let block_a = produce_as_leader(&mut a, 0);
        let fork = B256::from([0xAAu8; 32]);
        // vals[1] voted for a fork block first (as seen by us), then for ours.
        a.record_finality_vote(1, fork, vals[1]);
        let stake_before: Vec<_> = a.consensus.validators().iter().map(|v| v.stake).collect();
        a.record_finality_vote(1, block_a.hash, vals[1]);
        assert_eq!(
            a.consensus.pending_slash_count(),
            0,
            "no slash from gossip-observed votes"
        );
        assert_eq!(a.finalized_hash(1), None);
        // Two more votes for our block: the superseded vote counts toward quorum.
        a.record_finality_vote(1, block_a.hash, vals[0]);
        let finalized = a.record_finality_vote(1, block_a.hash, vals[2]);
        assert!(finalized, "3 of 4 equal stakes reach the 2/3 quorum");
        assert_eq!(a.finalized_hash(1), Some(block_a.hash));
        let stake_after: Vec<_> = a.consensus.validators().iter().map(|v| v.stake).collect();
        assert_eq!(stake_before, stake_after, "stakes untouched");
    }

    /// A conflicting vote for a hash we do not hold does not replace the
    /// first vote and is not slashed either.
    #[test]
    fn conflicting_vote_for_unknown_hash_is_dropped_without_slashing() {
        let vals = validators();
        let mut a = with_validators(engine());
        let block_a = produce_as_leader(&mut a, 0);
        a.record_finality_vote(1, block_a.hash, vals[1]);
        let other = B256::from([0xBBu8; 32]);
        assert!(!a.record_finality_vote(1, other, vals[1]));
        assert_eq!(a.consensus.pending_slash_count(), 0);
        // The original vote still counts.
        a.record_finality_vote(1, block_a.hash, vals[0]);
        assert!(a.record_finality_vote(1, block_a.hash, vals[2]));
        assert_eq!(a.finalized_hash(1), Some(block_a.hash));
    }

    /// Finality on the block we already hold changes nothing.
    #[test]
    fn finality_on_our_own_block_is_a_no_op() {
        let vals = validators();
        let mut a = with_validators(engine());
        let block_a = produce_as_leader(&mut a, 0);
        let root = a.evm.state.compute_state_root();
        for v in vals.iter().take(3) {
            a.record_finality_vote(1, block_a.hash, *v);
        }
        assert_eq!(a.finalized_hash(1), Some(block_a.hash));
        assert_eq!(a.block_by_number(1).unwrap().hash, block_a.hash);
        assert_eq!(a.evm.state.compute_state_root(), root);
    }

    /// A conflict deeper than the checkpoint window is reported, not applied.
    #[test]
    fn conflict_beyond_reorg_depth_is_left_to_the_heal_path() {
        let vals = validators();
        let mut a = with_validators(engine());
        let first = produce_as_leader(&mut a, 0);
        for _ in 0..(REORG_DEPTH + 2) {
            produce_as_leader(&mut a, 0);
        }
        let other = B256::from([9u8; 32]);
        for v in vals.iter().take(3) {
            a.record_finality_vote(first.number, other, *v);
        }
        // Too deep: our block at height 1 stays, chain untouched.
        assert_eq!(a.block_by_number(1).unwrap().hash, first.hash);
        assert_eq!(a.latest_height() as usize, REORG_DEPTH + 3);
    }
}

#[cfg(test)]
mod open_validator_set_tests {
    use super::*;
    use crate::staking::{ValidatorSetParams, ValidatorStatus};
    use revm::primitives::{Address, U256};
    use tempfile::tempdir;

    const MRSN: u64 = 1_000_000_000_000_000_000;

    fn key(i: u8) -> k256::ecdsa::SigningKey {
        k256::ecdsa::SigningKey::from_bytes(&[i; 32].into()).unwrap()
    }
    fn addr(i: u8) -> Address {
        crate::crypto::address_from_signing_key(&key(i))
    }

    fn params() -> ValidatorSetParams {
        ValidatorSetParams {
            activation_height: 1,
            epoch_blocks: 10,
            min_self_stake: U256::from(1_000u64) * U256::from(MRSN),
            max_validators: 12,
            unbonding_blocks: 50,
            jail_miss_bps: 2_000,
            jail_min_slots: 2,
            rewards_to_operator_height: 0,
            jail_escalation_height: 0,
            bench_height: 0,
        }
    }

    fn engine() -> Engine {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("engine");
        std::fs::create_dir_all(&sub).unwrap();
        let mut e = Engine::new_with_backend(131071, sub, "redb");
        std::mem::forget(dir);
        for i in 1..=4u8 {
            e.add_validator(addr(i), U256::from(1_000_000u64) * U256::from(MRSN))
                .unwrap();
        }
        e.set_validator_set_params(params());
        e
    }

    /// Produce the next block signed by the leader at `round` (the set for
    /// that height is fixed by the epoch transition first, as in production).
    fn produce(e: &mut Engine, round: u64, skip: &[Address]) -> Block {
        e.maybe_epoch_transition(e.block_number);
        let mut r = round;
        let leader = loop {
            let l = e.leader_for_height(e.block_number, r).expect("leader");
            if skip.contains(&l) {
                r += 1;
                continue;
            }
            break l;
        };
        let k = (1..=6u8)
            .map(key)
            .find(|k| crate::crypto::address_from_signing_key(k) == leader)
            .expect("leader key");
        e.set_local_validator(leader);
        let mut b = e.execute_block().expect("produce");
        b.proposer = leader;
        b.coinbase = leader;
        b.consensus.proposer = leader;
        let (sr, ss, v) = crate::crypto::sign_block_proposal(b.number, b.hash, &k);
        b.proposer_sig = Some((sr, ss, v));
        if let Some(last) = e.chain.last_mut()
            && last.number == b.number
        {
            last.proposer = leader;
            last.proposer_sig = b.proposer_sig;
        }
        b
    }

    /// A fresh process starts from the genesis validators in its config; the
    /// active set installed at the last boundary (with its stakes) must be
    /// restored, or the restarted node runs a different schedule than the
    /// network until the next boundary.
    #[test]
    fn restart_restores_the_active_set_and_its_stakes_sled() {
        restart_restores_the_active_set_and_its_stakes("sled");
    }

    #[test]
    fn restart_restores_the_active_set_and_its_stakes_redb() {
        restart_restores_the_active_set_and_its_stakes("redb");
    }

    fn restart_restores_the_active_set_and_its_stakes(backend: &str) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("engine");
        std::fs::create_dir_all(&path).unwrap();
        // Both backends persist the registry and the installed set.
        let build = |path: &std::path::Path| {
            let mut e = Engine::new_with_backend(131071, path.to_path_buf(), backend);
            for i in 1..=4u8 {
                e.add_validator(addr(i), U256::from(1_000_000u64) * U256::from(MRSN))
                    .unwrap();
            }
            e.set_validator_set_params(params());
            e
        };
        let mut p = build(&path);
        let v5 = addr(5);
        let operator = Address::from_slice(&[0x55; 20]);
        while p.block_number < 13 {
            if p.block_number == 2 {
                // After activation seeded the genesis validators, as in production.
                p.orders
                    .state
                    .staking
                    .register_validator(operator, v5, U256::from(5_000u64) * U256::from(MRSN), 0, 2)
                    .unwrap();
            }
            produce(&mut p, 0, &[]);
        }
        assert!(
            p.validator_addresses().contains(&v5),
            "v5 active after the boundary at 10"
        );
        assert_eq!(p.validator_addresses().len(), 5, "genesis four plus v5");
        p.evm.state.flush().unwrap();
        let mut expected: Vec<(Address, U256)> = p
            .consensus
            .validators()
            .iter()
            .map(|v| (v.address, v.stake))
            .collect();
        expected.sort();
        drop(p);

        let mut r = build(&path);
        assert_eq!(
            r.validator_addresses().len(),
            4,
            "a fresh process knows only the genesis set"
        );
        r.restore_consensus_set();
        let mut got: Vec<(Address, U256)> = r
            .consensus
            .validators()
            .iter()
            .map(|v| (v.address, v.stake))
            .collect();
        got.sort();
        assert_eq!(
            got, expected,
            "restored set and stakes match what was installed"
        );
    }

    /// A validator that misses three leader slots is dropped from the rotation
    /// for the rest of the epoch: from then on the chain does not spend a
    /// failover round on it, the producer and an importer compute the same
    /// rotation, and the epoch boundary jails it. It comes back when the jail
    /// ends and is benched again after three misses, not one round earlier.
    #[test]
    fn dead_leader_is_benched_after_three_misses() {
        let mut p = engine();
        let mut c = engine();
        let mut prm = params();
        prm.bench_height = 1;
        prm.epoch_blocks = 40; // five validators → eight leader slots each per epoch
        prm.jail_min_slots = 5; // more than the bench threshold: the bench must make the jail happen
        for e in [&mut p, &mut c] {
            e.set_validator_set_params(prm.clone());
        }
        let v5 = addr(5);
        let operator = Address::from_slice(&[0x55; 20]);
        // Bring both to the boundary at 40 where v5 is active. Registration
        // happens after activation seeded the genesis validators, as in
        // production (registering before it would leave the registry without
        // them and the set with v5 alone).
        while p.block_number < 40 {
            if p.block_number == 2 {
                for e in [&mut p, &mut c] {
                    e.orders
                        .state
                        .staking
                        .register_validator(
                            operator,
                            v5,
                            U256::from(5_000u64) * U256::from(MRSN),
                            0,
                            2,
                        )
                        .unwrap();
                }
            }
            let b = produce(&mut p, 0, &[]);
            c.maybe_epoch_transition(c.block_number);
            c.import_block(b);
        }
        p.maybe_epoch_transition(p.block_number);
        assert!(p.validator_addresses().contains(&v5), "v5 active from 40");
        assert_eq!(p.validator_addresses().len(), 5, "genesis four plus v5");
        assert!(p.benched_validators(p.block_number).is_empty());

        // v5 is dead. Count the failover rounds the network spends on it.
        let mut rounds_on_v5 = 0u64;
        let mut benched_at = None;
        while p.block_number < 80 {
            let h = p.block_number;
            let was_leader = p.leader_for_height(h, 0) == Some(v5);
            let b = produce(&mut p, 0, &[v5]);
            if was_leader {
                rounds_on_v5 += 1;
            }
            c.maybe_epoch_transition(c.block_number);
            c.import_block(b.clone());
            assert_eq!(
                c.latest_height(),
                p.latest_height(),
                "follower imported #{h}"
            );
            assert_eq!(
                c.evm.state.compute_state_root(),
                p.evm.state.compute_state_root(),
                "state roots agree at #{}",
                b.number
            );
            if benched_at.is_none() && p.benched_validators(p.block_number).contains(&v5) {
                benched_at = Some(p.block_number);
            }
        }
        let benched_at = benched_at.expect("v5 benched within the epoch");
        assert_eq!(
            p.orders.state.staking.registry[&v5].missed_slots, 3,
            "exactly three misses, then out of the rotation"
        );
        assert_eq!(
            rounds_on_v5, 3,
            "the network spent three failover rounds on v5, not more"
        );
        // With five validators, three misses take at most 15 blocks; the bench
        // happened well before the boundary at 80.
        assert!(benched_at <= 55, "benched mid-epoch at #{benched_at}");
        for h in benched_at..80 {
            for r in 0..MAX_LEADER_ROUND_WINDOW {
                assert_ne!(
                    p.leader_for_height(h, r),
                    Some(v5),
                    "benched: never leader at {h}/{r}"
                );
                assert_eq!(
                    p.leader_for_height(h, r),
                    c.leader_for_height(h, r),
                    "same rotation"
                );
            }
        }
        assert!(
            p.validator_addresses().contains(&v5),
            "benched, not removed: still votes"
        );

        // Boundary at 80: jailed although it saw fewer than jail_min_slots slots.
        p.maybe_epoch_transition(p.block_number);
        assert_eq!(
            p.orders.state.staking.status_of(v5),
            Some(ValidatorStatus::Jailed),
            "boundary jails the benched validator"
        );
        assert!(!p.validator_addresses().contains(&v5));
    }

    /// From `rewards_to_operator_height`, a registered validator's block
    /// reward lands on its operator wallet; before that on the node identity.
    /// Producer and importer agree on the state root either way.
    #[test]
    fn block_reward_moves_to_the_operator_wallet_at_the_configured_height() {
        let mut p = engine();
        let mut c = engine();
        let mut prm = params();
        prm.rewards_to_operator_height = 25;
        for e in [&mut p, &mut c] {
            e.set_validator_set_params(prm.clone());
            // Test engines have no token economics: switch on the testnet
            // schedule so blocks actually pay rewards.
            e.set_token_economics(
                U256::from(2u64).pow(U256::from(89u64)) - U256::from(1u64),
                U256::from(2u64).pow(U256::from(61u64)) - U256::from(1u64),
                33_550_336,
            );
        }
        let v5 = addr(5);
        let operator = Address::from_slice(&[0x55; 20]);
        let stake = U256::from(5_000u64) * U256::from(MRSN);
        for e in [&mut p, &mut c] {
            e.fund_account(v5, U256::ZERO, 0);
            e.fund_account(operator, U256::ZERO, 0);
        }
        // Register after activation (block 1) seeded the genesis validators,
        // so the set is the genesis four plus v5 rather than v5 alone.
        for e in [&mut p, &mut c] {
            if e.block_number == 0 {
                let b = produce(e, 0, &[]);
                drop(b);
            }
        }
        for e in [&mut p, &mut c] {
            e.orders
                .state
                .staking
                .register_validator(operator, v5, stake, 0, 1)
                .unwrap();
        }
        let mut identity_gain_before = U256::ZERO;
        let mut identity_gain_after = U256::ZERO;
        let mut operator_gain_before = U256::ZERO;
        let mut operator_gain_after = U256::ZERO;
        // v5 is eligible from epoch 2 (registered in epoch 0, active from 10).
        while p.block_number < 45 {
            let h = p.block_number;
            let id0 = p.get_balance(v5).unwrap();
            let op0 = p.get_balance(operator).unwrap();
            let b = produce(&mut p, 0, &[]);
            c.maybe_epoch_transition(c.block_number);
            c.import_block(b.clone());
            assert_eq!(
                c.latest_height(),
                p.latest_height(),
                "follower imported #{h}"
            );
            assert_eq!(
                c.evm.state.compute_state_root(),
                p.evm.state.compute_state_root(),
                "state roots agree at #{}",
                b.number
            );
            let id_gain = p.get_balance(v5).unwrap().saturating_sub(id0);
            let op_gain = p.get_balance(operator).unwrap().saturating_sub(op0);
            if b.number < 25 {
                identity_gain_before += id_gain;
                operator_gain_before += op_gain;
            } else {
                identity_gain_after += id_gain;
                operator_gain_after += op_gain;
            }
        }
        assert!(
            p.validator_addresses().contains(&v5),
            "v5 is in the active set"
        );
        assert!(
            !identity_gain_before.is_zero(),
            "identity earned before the switch"
        );
        assert!(
            operator_gain_before.is_zero(),
            "operator earned nothing before the switch"
        );
        assert!(
            identity_gain_after.is_zero(),
            "identity earns nothing after the switch"
        );
        assert!(
            !operator_gain_after.is_zero(),
            "operator earns after the switch"
        );
    }

    #[test]
    fn register_activate_jail_rehabilitate_exit_and_importer_agrees() {
        let mut p = engine(); // producer
        let mut c = engine(); // clean follower, imports each block as it is produced
        let v5 = addr(5);
        let operator = Address::from_slice(&[0x55; 20]);
        let stake = U256::from(5_000u64) * U256::from(MRSN);

        // Drives one height on both nodes: state mutations first (same height
        // on both, like a tx in that block would), then produce and import.
        let mut step = |p: &mut Engine, c: &mut Engine, skip: &[Address]| {
            let h = p.block_number;
            if h == 2 {
                for e in [&mut *p, &mut *c] {
                    e.orders
                        .state
                        .staking
                        .register_validator(operator, v5, stake, 500, h)
                        .unwrap();
                }
            }
            if h == 31 {
                for e in [&mut *p, &mut *c] {
                    e.orders
                        .state
                        .staking
                        .unregister_validator(operator, v5)
                        .unwrap();
                }
            }
            let b = produce(p, 0, skip);
            c.import_block(b.clone());
            assert_eq!(
                c.latest_height(),
                b.number,
                "follower applied block {}",
                b.number
            );
            assert_eq!(
                c.evm.state.compute_state_root(),
                p.evm.state.compute_state_root(),
                "roots agree at height {}",
                b.number
            );
        };

        for _ in 1..=3u64 {
            step(&mut p, &mut c, &[]);
        }
        assert_eq!(
            p.consensus.validators().len(),
            4,
            "not yet an epoch boundary"
        );
        assert_eq!(
            p.orders.state.staking.status_of(v5),
            Some(ValidatorStatus::Pending)
        );

        for _ in 4..=9u64 {
            step(&mut p, &mut c, &[]);
        }
        // Epoch 1 (10..19): v5 is admitted at 10 but never shows up.
        for _ in 10..=19u64 {
            step(&mut p, &mut c, &[v5]);
        }
        assert_eq!(
            p.consensus.validators().len(),
            5,
            "epoch 1 active set includes the newcomer"
        );
        assert!(p.validator_addresses().contains(&v5));
        assert_eq!(
            p.orders.state.staking.status_of(v5),
            Some(ValidatorStatus::Active)
        );
        let missed = p.orders.state.staking.registry[&v5].missed_slots;
        assert!(
            missed >= 2,
            "v5 had leader slots in epoch 1 and missed them all (missed={missed})"
        );

        // Height 20 judges epoch 1: jailed for epoch 2.
        step(&mut p, &mut c, &[]);
        assert_eq!(
            p.orders.state.staking.status_of(v5),
            Some(ValidatorStatus::Jailed)
        );
        assert_eq!(p.consensus.validators().len(), 4);
        assert!(!p.validator_addresses().contains(&v5));

        // Epoch 2 passes; at 30 it is eligible again and returns.
        for _ in 21..=30u64 {
            step(&mut p, &mut c, &[]);
        }
        assert_eq!(
            p.orders.state.staking.status_of(v5),
            Some(ValidatorStatus::Active),
            "rehabilitated after one epoch"
        );
        assert_eq!(p.consensus.validators().len(), 5);

        // Operator leaves at 31; removed at 40, self-stake unbonding.
        for _ in 31..=40u64 {
            step(&mut p, &mut c, &[]);
        }
        assert!(
            p.orders.state.staking.registry.get(&v5).is_none(),
            "removed at the epoch boundary"
        );
        assert_eq!(p.consensus.validators().len(), 4);
        let unb = p
            .orders
            .state
            .staking
            .unbondings
            .get(&operator)
            .expect("unbonding entry for the operator");
        assert_eq!(unb[0].amount, stake);
        assert_eq!(unb[0].unlock_at, 40 + 50);

        // Follower ends identical.
        assert_eq!(c.validator_addresses(), p.validator_addresses());
        assert_eq!(
            c.orders.state.staking.current_epoch,
            p.orders.state.staking.current_epoch
        );
        assert_eq!(
            c.evm.state.compute_state_root(),
            p.evm.state.compute_state_root()
        );
    }

    /// ABI-encode registerValidator(address,uint256,uint256,bytes).
    fn register_calldata(identity: Address, stake: U256, commission: u64, proof: &[u8]) -> Vec<u8> {
        let mut d = crate::precompile_abi::register_validator_selector().to_vec();
        let mut w = [0u8; 32];
        w[12..].copy_from_slice(identity.as_slice());
        d.extend_from_slice(&w);
        d.extend_from_slice(&stake.to_be_bytes::<32>());
        d.extend_from_slice(&U256::from(commission).to_be_bytes::<32>());
        d.extend_from_slice(&U256::from(128u64).to_be_bytes::<32>()); // offset of bytes
        d.extend_from_slice(&U256::from(proof.len() as u64).to_be_bytes::<32>());
        d.extend_from_slice(proof);
        d.resize(d.len() + (32 - proof.len() % 32) % 32, 0);
        d
    }

    #[test]
    fn register_through_the_precompile_with_the_node_key_proof() {
        let mut e = engine();
        let operator = Address::from_slice(&[0x77; 20]);
        let node_key = key(9);
        let identity = crate::crypto::address_from_signing_key(&node_key);
        e.fund_account(operator, U256::from(20_000u64) * U256::from(MRSN), 0);
        let stake = U256::from(1_000u64) * U256::from(MRSN);
        let proof = crate::crypto::sign_validator_registration(operator, identity, &node_key);

        // Wrong proof (signed for a different operator) is rejected: nothing registered, nothing escrowed.
        let bad = crate::crypto::sign_validator_registration(Address::ZERO, identity, &node_key);
        for (nonce, pr, expect_ok) in [(0u64, bad.clone(), false), (1u64, proof.clone(), true)] {
            let before = e.get_balance(operator).unwrap();
            e.submit_tx_unsigned(Transaction {
                from: operator,
                to: Some(crate::precompile_abi::STAKING_PRECOMPILE),
                value: U256::ZERO,
                data: Bytes::from(register_calldata(identity, stake, 500, &pr)),
                gas_limit: 300_000,
                gas_price: U256::from(1u64),
                nonce,
                chain_id: Some(e.chain_id),
                signature: None,
                tx_type: 0,
                shielded_payload: None,
                hash: None,
            })
            .expect("tx accepted into mempool");
            produce(&mut e, 0, &[]);
            let registered = e.orders.state.staking.registry.contains_key(&identity);
            let after = e.get_balance(operator).unwrap();
            assert_eq!(
                registered, expect_ok,
                "registration outcome for proof #{nonce}"
            );
            if expect_ok {
                assert!(before - after >= stake, "self-stake escrowed");
                assert_eq!(
                    e.get_balance(crate::precompile_abi::STAKING_PRECOMPILE)
                        .unwrap(),
                    stake
                );
            } else {
                assert!(
                    before - after < U256::from(MRSN),
                    "only gas spent, stake refunded"
                );
            }
        }
        let reg = &e.orders.state.staking.registry[&identity];
        assert_eq!(reg.operator, operator);
        assert_eq!(reg.commission_bps, 500);
        assert_eq!(
            e.orders.state.staking.status_of(identity),
            Some(ValidatorStatus::Pending)
        );
    }

    #[test]
    fn registration_rules() {
        let mut e = engine();
        let op = Address::from_slice(&[0x66; 20]);
        let st = &mut e.orders.state.staking;
        assert_eq!(
            st.register_validator(op, addr(5), U256::from(999u64) * U256::from(MRSN), 0, 5),
            Err(crate::staking::StakingError::StakeTooLow)
        );
        assert!(
            st.register_validator(op, addr(5), U256::from(1_000u64) * U256::from(MRSN), 0, 5)
                .is_ok()
        );
        assert_eq!(
            st.register_validator(op, addr(5), U256::from(1_000u64) * U256::from(MRSN), 0, 5),
            Err(crate::staking::StakingError::AlreadyRegistered)
        );
        assert_eq!(
            st.unregister_validator(Address::ZERO, addr(5)),
            Err(crate::staking::StakingError::NotOperator)
        );
        assert!(st.rotate_identity(op, addr(5), addr(6)).is_ok());
        // Before activation nothing can register.
        let mut off = ValidatorSetParams::default();
        off.activation_height = 0;
        st.set_params(off);
        assert_eq!(
            st.register_validator(
                op,
                Address::from_slice(&[0x77; 20]),
                U256::from(1_000u64) * U256::from(MRSN),
                0,
                5
            ),
            Err(crate::staking::StakingError::NotActive)
        );
    }
}
