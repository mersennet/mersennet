use crate::bridge::{BridgeDomain, BridgeMessage, BridgeQueue};
use crate::code_publication::{CodePublicationRegistry, PublishedCodeAttestation};
use crate::commit_reveal::{CommitRevealError, CommitRevealPool, TxCommitment, TxReveal};
use crate::consensus::{
    Consensus, EvidenceKind, Finalization, Reward, RoundResult, Slashing, SlashingEvidence,
    Unbonding, Validator, ValidatorChange,
};
use crate::crypto::{self, SignedTransaction};
use crate::errors::PrimeOrdersError;
use crate::events::{
    BridgeEvent, BridgeQueueKind, DomainEvent, DomainEventRecord, PrimeOrdersEvent,
};
use crate::fba::{AuctionResult, BatchOrder, FBAEngine};
use crate::hotstuff2::{HotStuff2, HotStuff2Result};
use crate::mempool::{Mempool, TxRejection};
use crate::network::NetworkSim;
use crate::parallel::ParallelExecutor;
use crate::precompiles;
use crate::prime_orders::{
    MarketId, Order, OrderBookView, OrderId, OrderOutcome, PrimeOrdersState, Side, TimeInForce,
};
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
use crate::shielded_orders::ShieldedOrdersEngine;
use crate::shielded_persistence::ShieldedPersistence;
use crate::threshold_mempool::ThresholdMempool;

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
    pub state: PrimeOrdersState,
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

    // ───── Privacy redesign (Phase 4 wiring) ─────
    /// Master switch — gates `apply_shielded_tx`, the FBA tick, the
    /// SP1 prover hook, and the 0x7E EIP-2718 type-byte path.
    /// `false` until the hard fork activates (see ADR-018).
    pub privacy_mode_activated: bool,
    /// Activation height for the privacy hard fork, set by genesis
    /// or governance. When `Some(h)` and `block_number >= h`, the
    /// engine auto-flips `privacy_mode_activated` to `true` at the
    /// start of the block-production loop.
    pub privacy_activation_height: Option<u64>,
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
        let mut prime_orders = PrimeOrdersState::new();
        state
            .load_prime_orders(&mut prime_orders)
            .expect("prime orders load");
        let mut bridge_orders_to_evm = BridgeQueue::new();
        let mut bridge_evm_to_orders = BridgeQueue::new();
        state
            .load_bridge_queues(&mut bridge_orders_to_evm, &mut bridge_evm_to_orders)
            .expect("bridge queues load");

        Self {
            chain_id,
            block_number: 1,
            base_fee: U256::from(1),
            coinbase: Address::ZERO,
            gas_limit_per_block: 30_000_000,
            spec_id: SpecId::SHANGHAI,
            consensus: ConsensusEngine {
                inner: Consensus::default(),
                network: NetworkSim::default(),
                hotstuff2: None,
            },
            fee_max_change_denominator: 8,
            fee_elasticity_multiplier: 2,
            fee_target_gas: 15_000_000,
            evm: EvmEngine { state, db },
            chain: Vec::new(),
            orders: OrdersEngine {
                state: prime_orders,
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

            // Privacy-redesign Phase 4 — off until the hard fork
            // flips the master switch (ADR-018). All subsystems live
            // in-memory; persistence is wired in Workstream A7.
            privacy_mode_activated: false,
            privacy_activation_height: None,
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
            "Prime Chain privacy hard fork activated — shielded tx type 0x7E now accepted"
        );
    }

    /// Block-level convenience: are shielded paths live?
    pub fn privacy_mode_activated(&self) -> bool {
        self.privacy_mode_activated
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
            return true;
        }
        false
    }

    pub fn latest_height(&self) -> u64 {
        self.chain.last().map(|block| block.number).unwrap_or(0)
    }

    pub fn block_by_number(&self, number: u64) -> Option<&Block> {
        self.chain.iter().find(|block| block.number == number)
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

    pub fn prime_orders_add_market(
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
        self.record_event(DomainEvent::PrimeOrders(PrimeOrdersEvent::MarketAdded {
            market_id,
            symbol,
            tick_size,
            lot_size,
        }));
        market_id
    }

    pub fn prime_orders_submit_order(
        &mut self,
        owner: Address,
        market: MarketId,
        side: Side,
        price: U256,
        size: U256,
        tif: TimeInForce,
    ) -> Result<OrderOutcome, PrimeOrdersError> {
        let outcome = self
            .orders
            .state
            .submit_order(owner, market, side, price, size, tif)?;
        self.record_event(DomainEvent::PrimeOrders(PrimeOrdersEvent::OrderSubmitted {
            order_id: outcome.order_id,
            owner,
            market_id: market,
            side,
            price,
            size,
            tif,
            filled: outcome.filled,
            remaining: outcome.remaining,
        }));
        for trade in &outcome.trades {
            self.record_event(DomainEvent::PrimeOrders(PrimeOrdersEvent::Trade {
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

    pub fn prime_orders_cancel_order(&mut self, order_id: OrderId) -> Option<Order> {
        let order = self.orders.state.cancel_order(order_id);
        if let Some(order) = &order {
            self.record_event(DomainEvent::PrimeOrders(PrimeOrdersEvent::OrderCancelled {
                order_id: order.id,
                owner: order.owner,
                market_id: order.market,
            }));
        }
        order
    }

    #[allow(dead_code)]
    pub fn prime_orders_set_margin_params(&mut self, initial_bps: u64, maintenance_bps: u64) {
        self.orders
            .state
            .set_margin_params(initial_bps, maintenance_bps);
        self.record_event(DomainEvent::PrimeOrders(
            PrimeOrdersEvent::MarginParamsUpdated {
                initial_bps,
                maintenance_bps,
            },
        ));
    }

    #[allow(dead_code)]
    pub fn prime_orders_deposit_collateral(&mut self, owner: Address, amount: U256) {
        self.orders.state.deposit_collateral(owner, amount);
        self.record_event(DomainEvent::PrimeOrders(
            PrimeOrdersEvent::CollateralDeposited { owner, amount },
        ));
    }

    #[allow(dead_code)]
    pub fn prime_orders_is_liquidatable(&self, owner: Address) -> bool {
        self.orders.state.is_liquidatable(owner)
    }

    #[allow(dead_code)]
    pub fn prime_orders_liquidate(&mut self, owner: Address) -> bool {
        let liquidated = self.orders.state.liquidate(owner);
        self.record_event(DomainEvent::PrimeOrders(PrimeOrdersEvent::Liquidation {
            owner,
            liquidated,
        }));
        liquidated
    }

    pub fn prime_orders_order_book(&self, market: MarketId) -> Option<OrderBookView> {
        self.orders.state.order_book(market)
    }

    pub fn prime_orders_open_orders(&self, owner: Address) -> Vec<Order> {
        self.orders.state.open_orders(owner)
    }

    #[allow(dead_code)]
    pub fn bridge_enqueue_orders_to_evm(&mut self, payload: Bytes) -> BridgeMessage {
        let msg = self.bridge.orders_to_evm.push(
            BridgeDomain::PrimeOrders,
            BridgeDomain::PrimeEvm,
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
            BridgeDomain::PrimeEvm,
            BridgeDomain::PrimeOrders,
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
        self.consensus.stake(address, stake)
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
                "prime_chain_dkg_ceremonies_started_total",
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
        let mut nonce_cache: HashMap<Address, u64> = HashMap::new();
        let mut progressed = true;

        let orders_state = std::mem::take(&mut self.orders.state);
        let shared_orders = Arc::new(Mutex::new(orders_state));
        precompiles::set_prime_orders_context(shared_orders.clone());
        precompiles::set_transparent_prime_orders_enabled(!self.privacy_mode_activated);

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
                        crate::prime_orders::MarketId(*market_id),
                        crate::prime_orders::Side::Buy,
                        U256::from(*price),
                        U256::from(1),
                        crate::prime_orders::TimeInForce::Ioc,
                    );
                }
                for (price, _size) in &quote.asks {
                    let _ = orders.place_order(
                        quote.owner,
                        crate::prime_orders::MarketId(*market_id),
                        crate::prime_orders::Side::Sell,
                        U256::from(*price),
                        U256::from(1),
                        crate::prime_orders::TimeInForce::Ioc,
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

                if gas_used.saturating_add(tx_gas) > self.gas_limit_per_block {
                    continue;
                }

                let Some(tx) = self.mempool.take_ready(sender, expected_nonce) else {
                    continue;
                };

                let execution = if tx.tx_type == crate::shielded_evm::SHIELDED_TX_TYPE {
                    self.apply_shielded_tx(&tx)
                } else {
                    self.execute_tx(&tx)?
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

        precompiles::clear_prime_orders_context();
        precompiles::set_transparent_prime_orders_enabled(true);
        self.orders.state = Arc::try_unwrap(shared_orders)
            .expect("no other Arc references")
            .into_inner()
            .expect("mutex not poisoned");

        // Privacy-redesign Phase 4 — discrete-time auction + sealed-
        // bid liquidation tick. No-op pre-fork.
        if self.privacy_mode_activated {
            self.run_shielded_tick();
        }

        for tx in &transactions {
            self.mempool.remove_mined(tx.from, tx.nonce);
        }

        let hash = self.compute_block_hash(
            self.block_number,
            self.chain_id,
            self.gas_limit_per_block,
            gas_used,
            self.base_fee,
            self.coinbase,
            transactions.len() as u64,
        );
        let applied_validator_changes = self.consensus.apply_pending_changes(self.block_number);
        let (finality_rounds, slashing_evidence) =
            self.consensus
                .run_finality_rounds(hash, self.block_number, 2);
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
        let mut consensus = self.consensus.finalize(hash, self.block_number);
        let finalized = finality_rounds.iter().any(|round| round.finalized);
        consensus.finalized = finalized;
        let unbonded = self.consensus.process_unbonding(self.block_number);
        let slashes = self.consensus.apply_pending_slashes(self.block_number);
        let rewards = consensus.rewards.clone();
        let total_reward = consensus.total_reward;
        let burned_reward = consensus.burned_reward;

        self.apply_rewards(&rewards)?;

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

        // Privacy-redesign Phase 4 — compute shielded roots + the SP1
        // state-transition proof. Pre-fork the roots stay
        // `B256::ZERO` and no proof is generated.
        let (shielded_state_root, nullifier_root, shielded_event_root, state_proof) =
            self.shielded_block_header(&transactions, &domain_events, hash);

        let block = Block {
            number: self.block_number,
            chain_id: self.chain_id,
            timestamp,
            gas_limit: self.gas_limit_per_block,
            gas_used,
            base_fee: self.base_fee,
            coinbase: consensus.proposer,
            hash,
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
        };

        self.chain.push(block.clone());
        self.evm.state.store_block(&block)?;

        // Privacy-redesign Phase 4 — persist shielded state at end of
        // block. No-op pre-fork (the subsystems are still in their
        // initial state and saving an empty snapshot is harmless).
        if self.privacy_mode_activated
            && let Some(p) = &self.shielded_persistence
            && let Err(e) = p.save_shielded_evm(&self.shielded_evm, block.number)
        {
            tracing::warn!(error = ?e, block = block.number, "shielded persistence save failed");
        }

        // Run formal verification invariant checks
        {
            use crate::formal_verification::BlockReport;
            let report = BlockReport {
                height: block.number,
                balances_before: HashMap::new(),
                balances_after: HashMap::new(),
                minted: block.total_reward,
                burned: block.burned_reward,
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

        metrics::increment_counter!("prime_chain_blocks_produced_total");
        metrics::gauge!("prime_chain_height", self.block_number as f64);
        metrics::gauge!("prime_chain_block_gas_used", gas_used as f64);
        metrics::gauge!("prime_chain_block_tx_count", tx_count as f64);
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
        metrics::gauge!(
            "prime_chain_base_fee_wei",
            self.base_fee.as_limbs()[0] as f64
        );

        self.block_number += 1;
        metrics::histogram!(
            "prime_chain_block_execution_seconds",
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
        let hash = self.compute_block_hash(
            self.block_number,
            self.chain_id,
            self.gas_limit_per_block,
            gas_used,
            self.base_fee,
            self.coinbase,
            transactions.len() as u64,
        );
        let applied_validator_changes = self.consensus.apply_pending_changes(self.block_number);
        let (finality_rounds, slashing_evidence) =
            self.consensus
                .run_finality_rounds(hash, self.block_number, 2);
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
        let mut consensus = self.consensus.finalize(hash, self.block_number);
        let finalized = finality_rounds.iter().any(|round| round.finalized);
        consensus.finalized = finalized;
        let unbonded = self.consensus.process_unbonding(self.block_number);
        let slashes = self.consensus.apply_pending_slashes(self.block_number);
        let rewards = consensus.rewards.clone();
        let total_reward = consensus.total_reward;
        let burned_reward = consensus.burned_reward;

        self.apply_rewards(&rewards)?;

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

        let (shielded_state_root, nullifier_root, shielded_event_root, state_proof) =
            self.shielded_block_header(&transactions, &domain_events, hash);

        let block = Block {
            number: self.block_number,
            chain_id: self.chain_id,
            timestamp,
            gas_limit: self.gas_limit_per_block,
            gas_used,
            base_fee: self.base_fee,
            coinbase: consensus.proposer,
            hash,
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
        };

        self.chain.push(block.clone());
        self.evm.state.store_block(&block)?;

        // Privacy-redesign Phase 4 — persist shielded state.
        if self.privacy_mode_activated
            && let Some(p) = &self.shielded_persistence
            && let Err(e) = p.save_shielded_evm(&self.shielded_evm, block.number)
        {
            tracing::warn!(error = ?e, block = block.number, "shielded persistence save failed (parallel)");
        }

        metrics::increment_counter!("prime_chain_blocks_produced_total");
        metrics::gauge!("prime_chain_height", self.block_number as f64);
        metrics::gauge!("prime_chain_block_gas_used", gas_used as f64);
        metrics::gauge!("prime_chain_block_tx_count", tx_count as f64);
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
        metrics::gauge!(
            "prime_chain_base_fee_wei",
            self.base_fee.as_limbs()[0] as f64
        );

        self.block_number += 1;
        metrics::histogram!(
            "prime_chain_block_execution_seconds",
            start.elapsed().as_secs_f64()
        );
        Ok(block)
    }

    pub fn import_block(&mut self, block: Block) {
        if self.latest_height() < block.number {
            self.chain.push(block.clone());
        }
        if self.block_number <= block.number {
            self.block_number = block.number.saturating_add(1);
            self.base_fee = block.base_fee;
        }
    }

    pub fn mempool_is_empty(&self) -> bool {
        self.mempool.is_empty()
    }

    pub fn mempool_pending_count(&self) -> usize {
        self.mempool.pending_count()
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
        self.evm.state.load_prime_orders(&mut self.orders.state)?;
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
        precompiles::set_prime_orders_context(shared_orders);
        precompiles::set_transparent_prime_orders_enabled(!self.privacy_mode_activated);

        // Privacy-redesign Phase 4 — install shielded EVM context.
        // Take ownership for the duration of the tx, restore after.
        let shielded_evm = std::mem::take(&mut self.shielded_evm);
        let shared_shielded = Arc::new(Mutex::new(shielded_evm));
        precompiles::set_shielded_evm_context(shared_shielded.clone());

        let mut evm = Evm::builder()
            .with_db(self.evm.db.clone())
            .with_spec_id(self.spec_id)
            .with_env(Box::new(env))
            .append_handler_register(precompiles::register_prime_orders_precompile)
            .append_handler_register(precompiles::register_shielded_precompiles)
            .build();

        let result = evm.transact_preverified()?;
        precompiles::clear_prime_orders_context();
        precompiles::set_transparent_prime_orders_enabled(true);
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
        precompiles::set_prime_orders_context(shared_orders);
        precompiles::set_transparent_prime_orders_enabled(!self.privacy_mode_activated);

        let shielded_evm = std::mem::take(&mut self.shielded_evm);
        let shared_shielded = Arc::new(Mutex::new(shielded_evm));
        precompiles::set_shielded_evm_context(shared_shielded.clone());

        let mut evm = Evm::builder()
            .with_db(self.evm.db.clone())
            .with_spec_id(self.spec_id)
            .with_env(Box::new(env))
            .append_handler_register(precompiles::register_prime_orders_precompile)
            .append_handler_register(precompiles::register_shielded_precompiles)
            .build();

        let result = evm.transact_preverified()?;
        precompiles::clear_prime_orders_context();
        precompiles::set_transparent_prime_orders_enabled(true);
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
    /// Pre-fork, every component is the canonical zero value and the
    /// SP1 prover is not invoked. Post-fork, the digests are taken
    /// from the live shielded subsystems and the proof is generated
    /// via [`crate::state_proof::prove_block`] (mock prover today,
    /// real SP1 once `prime-zkp/sp1` is enabled in Workstream E).
    fn shielded_block_header(
        &self,
        transactions: &[Transaction],
        domain_events: &[crate::events::DomainEvent],
        block_hash: B256,
    ) -> (
        B256,
        B256,
        B256,
        Option<crate::zk_proofs::StateTransitionProof>,
    ) {
        if !self.privacy_mode_activated {
            return (B256::ZERO, B256::ZERO, B256::ZERO, None);
        }

        let shielded_state_root = B256::from(self.shielded_evm.state.current_root().to_bytes());
        // Cheap nullifier-root digest: a real SMT lives in
        // ShieldedState (Workstream A7 persistence) — for header
        // commitment purposes we hash the canonical count.
        let mut nbuf = Vec::with_capacity(16);
        nbuf.extend_from_slice(&(self.shielded_evm.state.nullifier_count() as u64).to_le_bytes());
        let nullifier_root = keccak256(&nbuf);

        // Shielded event root: keccak of bincode-serialised
        // `ShieldedEvent` records, in block-emission order. Light
        // clients re-derive this from the public event stream.
        let mut ebuf = Vec::new();
        for ev in domain_events
            .iter()
            .filter(|e| matches!(e, crate::events::DomainEvent::Shielded(_)))
        {
            if let Ok(bytes) = bincode::serialize(ev) {
                ebuf.extend_from_slice(&bytes);
            }
        }
        let shielded_event_root = if ebuf.is_empty() {
            B256::ZERO
        } else {
            keccak256(&ebuf)
        };

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
            txs: transactions
                .iter()
                .filter(|t| t.tx_type == crate::shielded_evm::SHIELDED_TX_TYPE)
                .filter_map(|t| bincode::serialize(&t.shielded_payload).ok())
                .collect(),
            prev_market_state: Vec::new(),
            block_hash,
        };
        let state_proof = crate::state_proof::prove_block(
            &request,
            &self.shielded_evm.state,
            shielded_event_root,
        )
        .ok();

        (
            shielded_state_root,
            nullifier_root,
            shielded_event_root,
            state_proof,
        )
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
        use crate::events::{DomainEvent, ShieldedEvent};

        // Step 1 — drain decrypted intents. The plaintext routing
        // into `shielded_orders.admit_intent` is deferred to D3 once
        // the threshold-decryption pipe is real; we still flush so
        // the bookkeeping stays consistent.
        let drained = self.threshold_mempool.drain_decrypted(usize::MAX);
        if !drained.is_empty() {
            metrics::counter!(
                "prime_chain_threshold_mempool_admitted_total",
                drained.len() as u64
            );
            self.pending_events
                .push(DomainEvent::Shielded(ShieldedEvent::MempoolBatchAdmitted {
                    block_number: self.block_number,
                    intent_count: drained.len() as u64,
                }));
        }
        metrics::gauge!(
            "prime_chain_threshold_mempool_pending",
            self.threshold_mempool.pending_count() as f64
        );

        // Step 2 — uniform-price auction per market.
        let market_ids: Vec<crate::prime_orders::MarketId> =
            self.shielded_orders.aggregates.keys().copied().collect();
        for market_id in market_ids {
            match self.shielded_orders.run_fba(market_id) {
                Ok(result) => {
                    if !result.matched_size.is_zero() {
                        metrics::counter!(
                            "prime_chain_fba_cleared_total",
                            1,
                            "market_id" => market_id.0.to_string()
                        );
                        // Gauges carry only market-level aggregates;
                        // no per-trader labels (CI K2).
                        metrics::gauge!(
                            "prime_chain_fba_clearing_price",
                            f64_from_u256(result.clearing_price),
                            "market_id" => market_id.0.to_string()
                        );
                        metrics::gauge!(
                            "prime_chain_fba_matched_size",
                            f64_from_u256(result.matched_size),
                            "market_id" => market_id.0.to_string()
                        );
                        self.pending_events.push(DomainEvent::Shielded(
                            ShieldedEvent::FbaCleared {
                                market_id,
                                clearing_price: result.clearing_price,
                                matched_size: result.matched_size,
                                intent_count: result.fills.len() as u64,
                            },
                        ));
                    }
                }
                Err(err) => {
                    tracing::warn!(market = %market_id.0, error = ?err, "shielded FBA failed");
                }
            }
        }

        // Step 3 — settle sealed-bid liquidations. `settle_block`
        // returns (claim_tag, liquidator_id, winning_bid) for every
        // tag that received bids. `claim_tag` is
        // `Poseidon(victim_commitment, oracle_price)`, so it doesn't
        // re-link to a market or trader; `liquidator_id` is the
        // bond commitment (no address leak either).
        let winners = self.liquidation_auction.settle_block(self.block_number);
        for (_claim_tag, bond_commitment, winning_bid) in winners {
            metrics::counter!("prime_chain_liquidation_auctions_settled_total", 1);
            self.pending_events
                .push(DomainEvent::Shielded(ShieldedEvent::LiquidationSettled {
                    market_id: crate::prime_orders::MarketId(0), // see ADR-016: market is unbound at settle time
                    winner_bond_commitment: bond_commitment.to_bytes(),
                    winning_bid,
                }));
        }
        metrics::gauge!(
            "prime_chain_liquidator_count",
            self.liquidation_auction.liquidators.len() as f64
        );

        // Step 4 — emit the new shielded-state root for light clients.
        let new_root = self.shielded_evm.state.current_root().to_bytes();
        metrics::counter!("prime_chain_shielded_root_advanced_total", 1);
        let snap = self.shielded_evm.state.snapshot();
        metrics::gauge!("prime_chain_shielded_notes_total", snap.leaves.len() as f64);
        metrics::gauge!(
            "prime_chain_shielded_nullifiers_total",
            snap.nullifiers.len() as f64
        );
        metrics::gauge!(
            "prime_chain_privacy_mode_active",
            if self.privacy_mode_activated {
                1.0
            } else {
                0.0
            }
        );
        metrics::gauge!(
            "prime_chain_privacy_activation_height",
            self.privacy_activation_height.unwrap_or(0) as f64
        );
        self.pending_events
            .push(DomainEvent::Shielded(ShieldedEvent::ShieldedRootAdvanced {
                block_number: self.block_number,
                new_root,
                notes_added: 0,
                nullifiers_added: 0,
            }));
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
        precompiles::set_code_publication_context(
            shared_publication.clone(),
            self.evm.db.clone(),
            self.block_number,
        );

        let mut evm = Evm::builder()
            .with_db(self.evm.db.clone())
            .with_spec_id(self.spec_id)
            .with_env(Box::new(env))
            .append_handler_register(precompiles::register_prime_orders_precompile)
            .append_handler_register(precompiles::register_shielded_precompiles)
            .append_handler_register(precompiles::register_code_publication_precompile)
            .build();

        precompiles::set_transparent_prime_orders_enabled(!self.privacy_mode_activated);
        let result = evm.transact_commit()?;
        self.evm.db = std::mem::take(&mut evm.context.evm.db);
        precompiles::set_transparent_prime_orders_enabled(true);
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
    fn compute_block_hash(
        &self,
        number: u64,
        chain_id: u64,
        gas_limit: u64,
        gas_used: u64,
        base_fee: U256,
        coinbase: Address,
        tx_count: u64,
    ) -> B256 {
        let mut payload = Vec::with_capacity(8 + 8 + 8 + 8 + 32 + 20 + 8);
        payload.extend_from_slice(&number.to_be_bytes());
        payload.extend_from_slice(&chain_id.to_be_bytes());
        payload.extend_from_slice(&gas_limit.to_be_bytes());
        payload.extend_from_slice(&gas_used.to_be_bytes());
        payload.extend_from_slice(&base_fee.to_be_bytes::<32>());
        payload.extend_from_slice(coinbase.as_slice());
        payload.extend_from_slice(&tx_count.to_be_bytes());
        keccak256(payload)
    }

    fn apply_rewards(&mut self, rewards: &[Reward]) -> Result<()> {
        for reward in rewards {
            if reward.amount.is_zero() {
                continue;
            }
            self.evm.state.mark_dirty(reward.address);
            let mut info = self.evm.db.basic(reward.address)?.unwrap_or_default();
            info.balance = info.balance.saturating_add(reward.amount);
            self.evm.db.insert_account_info(reward.address, info);
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
