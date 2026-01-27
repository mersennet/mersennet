use anyhow::Result;
use revm::db::InMemoryDB;
use crate::consensus::{
    EvidenceKind, Finalization, Consensus, Reward, RoundResult, Slashing, SlashingEvidence,
    Unbonding, Validator, ValidatorChange,
};
use crate::bridge::{BridgeDomain, BridgeMessage, BridgeQueue};
use crate::events::{BridgeEvent, BridgeQueueKind, DomainEvent, DomainEventRecord, PrimeOrdersEvent};
use crate::errors::PrimeOrdersError;
use crate::mempool::{Mempool, TxRejection};
use crate::network::NetworkSim;
use crate::prime_orders::{MarketId, Order, OrderBookView, OrderId, OrderOutcome, PrimeOrdersState, Side, TimeInForce};
use crate::state::{PersistentState, SnapshotMeta};
use revm::primitives::{
    keccak256, AccountInfo, Address, B256, Bytes, Bytecode, Env, ExecutionResult, SpecId, TxKind,
    U256, KECCAK_EMPTY,
};
use revm::{Database, Evm};
use std::collections::HashMap;
use std::time::Instant;

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

#[derive(Clone, Debug)]
pub struct Transaction {
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Bytes,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub nonce: u64,
    pub chain_id: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct TxExecution {
    pub success: bool,
    pub gas_used: u64,
    pub output: Bytes,
    pub created_address: Option<Address>,
    pub logs: Vec<LogEntry>,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Bytes,
}

#[derive(Clone, Debug)]
pub struct Receipt {
    pub success: bool,
    pub gas_used: u64,
    pub output: Bytes,
    pub created_address: Option<Address>,
    pub error: Option<String>,
    pub logs: Vec<LogEntry>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Block {
    pub number: u64,
    pub chain_id: u64,
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
}

#[derive(Debug)]
pub struct EvmEngine {
    pub state: PersistentState,
    pub db: InMemoryDB,
}

#[derive(Debug)]
pub struct ConsensusEngine {
    pub inner: Consensus,
    pub network: NetworkSim,
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

    pub fn slash_amount_for_evidence(&self, evidence: &SlashingEvidence) -> U256 {
        self.inner.slash_amount_for_evidence(evidence)
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
}

impl Engine {
    pub fn new_with_state(chain_id: u64, path: impl AsRef<std::path::Path>) -> Self {
        let state = PersistentState::open(path).expect("state DB open");
        let mut db = InMemoryDB::default();
        state
            .load_into_db(&mut db)
            .expect("state DB load");
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
            },
            fee_max_change_denominator: 8,
            fee_elasticity_multiplier: 2,
            fee_target_gas: 15_000_000,
            evm: EvmEngine { state, db },
            chain: Vec::new(),
            orders: OrdersEngine { state: prime_orders },
            bridge: BridgeEngine {
                orders_to_evm: bridge_orders_to_evm,
                evm_to_orders: bridge_evm_to_orders,
            },
            pending_events: Vec::new(),
            mempool: Mempool::with_limits(10_000, 1_000, 1_000),
        }
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
        self.consensus.set_slashing_bps(double_sign_bps, timeout_bps);
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
        self.mempool.update_limits(max_total, max_per_sender, bump_bps);
    }

    pub fn set_bridge_limits(&mut self, max_queue_len: Option<usize>) {
        self.bridge.orders_to_evm.set_max_len(max_queue_len);
        self.bridge.evm_to_orders.set_max_len(max_queue_len);
    }

    fn record_event(&mut self, event: DomainEvent) {
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
                if let Some(domain) = domain {
                    if event.domain() != domain {
                        continue;
                    }
                }
                if let Some(kind) = kind {
                    if event.kind() != kind {
                        continue;
                    }
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
        let market_id = self.orders.state.add_market(symbol.clone(), tick_size, lot_size);
        self.record_event(DomainEvent::PrimeOrders(
            PrimeOrdersEvent::MarketAdded {
                market_id,
                symbol,
                tick_size,
                lot_size,
            },
        ));
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
        self.record_event(DomainEvent::PrimeOrders(
            PrimeOrdersEvent::OrderSubmitted {
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
            self.record_event(DomainEvent::PrimeOrders(
                PrimeOrdersEvent::OrderCancelled {
                    order_id: order.id,
                    owner: order.owner,
                    market_id: order.market,
                },
            ));
        }
        order
    }

    #[allow(dead_code)]
    pub fn prime_orders_set_margin_params(&mut self, initial_bps: u64, maintenance_bps: u64) {
        self.orders.state.set_margin_params(initial_bps, maintenance_bps);
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
        self.record_event(DomainEvent::PrimeOrders(
            PrimeOrdersEvent::Liquidation { owner, liquidated },
        ));
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
        let msg = self
            .bridge
            .orders_to_evm
            .push(BridgeDomain::PrimeOrders, BridgeDomain::PrimeEvm, payload);
        self.record_event(DomainEvent::Bridge(BridgeEvent::Enqueued {
            queue: BridgeQueueKind::OrdersToEvm,
            message: msg.clone(),
        }));
        msg
    }

    #[allow(dead_code)]
    pub fn bridge_enqueue_evm_to_orders(&mut self, payload: Bytes) -> BridgeMessage {
        let msg = self
            .bridge
            .evm_to_orders
            .push(BridgeDomain::PrimeEvm, BridgeDomain::PrimeOrders, payload);
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
        let code = Bytecode::new();
        let code_hash = KECCAK_EMPTY;
        let info = AccountInfo::new(balance, nonce, code_hash, code);
        self.evm.db.insert_account_info(address, info);
    }

    pub fn submit_tx(&mut self, tx: Transaction) -> Result<(), TxRejection> {
        self.validate_tx_basic(&tx)?;
        self.validate_tx_state(&tx)?;
        self.mempool.insert(tx)?;
        metrics::increment_counter!("tx_submitted_total");
        Ok(())
    }

    pub fn execute_block(&mut self) -> Result<Block> {
        let start = Instant::now();
        let mut gas_used = 0u64;
        let mut receipts = Vec::new();
        let mut transactions = Vec::new();
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

                if gas_used.saturating_add(tx_gas) > self.gas_limit_per_block {
                    continue;
                }

                let Some(tx) = self.mempool.take_ready(sender, expected_nonce) else {
                    continue;
                };

                let execution = self.execute_tx(&tx)?;
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
            self.consensus.run_finality_rounds(hash, self.block_number, 2);
        if !finality_rounds.is_empty() {
            for _ in 0..finality_rounds.len() {
                metrics::increment_counter!("consensus_rounds_total");
            }
        }
        for evidence in &slashing_evidence {
            metrics::increment_counter!(
                "consensus_slashing_evidence_total",
                "kind" => match evidence.kind {
                    EvidenceKind::DoubleSign => "double_sign",
                    EvidenceKind::PrecommitTimeout => "precommit_timeout",
                }
            );
            let amount = self.consensus.slash_amount_for_evidence(evidence);
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
        if finalized {
            metrics::increment_counter!("blocks_finalized_total");
        }
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
        let block = Block {
            number: self.block_number,
            chain_id: self.chain_id,
            gas_limit: self.gas_limit_per_block,
            gas_used,
            base_fee: self.base_fee,
            coinbase: self.coinbase,
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
        };

        self.chain.push(block.clone());

        metrics::increment_counter!("blocks_executed_total");
        metrics::gauge!("block_height_gauge", self.block_number as f64);
        tracing::info!(
            block_number = self.block_number,
            txs = tx_count,
            gas_used,
            finalized,
            "block executed"
        );

        self.base_fee = self.next_base_fee(gas_used);

        self.block_number += 1;
        metrics::histogram!("block_exec_duration_seconds", start.elapsed().as_secs_f64());
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

    #[allow(dead_code)]
    pub fn export_state_snapshot(&mut self, path: impl AsRef<std::path::Path>) -> Result<B256> {
        let root = self.evm.state.commit_state(
            &self.evm.db,
            &self.orders.state,
            &self.bridge.orders_to_evm,
            &self.bridge.evm_to_orders,
        )?;
        let height = self.block_number.saturating_sub(1);
        self.evm.state.export_snapshot(path, height, root)?;
        Ok(root)
    }

    #[allow(dead_code)]
    pub fn import_state_snapshot(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<SnapshotMeta> {
        let meta = self.evm.state.import_snapshot(path)?;
        self.evm.db = InMemoryDB::default();
        self.evm.state.load_into_db(&mut self.evm.db)?;
        self.evm.state.load_prime_orders(&mut self.orders.state)?;
        self.evm
            .state
            .load_bridge_queues(&mut self.bridge.orders_to_evm, &mut self.bridge.evm_to_orders)?;
        self.block_number = meta.height.saturating_add(1);
        Ok(meta)
    }

    pub fn get_balance(&mut self, address: Address) -> Result<U256> {
        Ok(self
            .evm.db
            .basic(address)?
            .map(|info| info.balance)
            .unwrap_or_default())
    }

    pub fn get_account_nonce(&mut self, address: Address) -> Result<u64> {
        Ok(self
            .evm.db
            .basic(address)?
            .map(|info| info.nonce)
            .unwrap_or_default())
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
        let result = self.submit_tx(Transaction {
            from,
            to: None,
            value,
            data: bytecode,
            gas_limit,
            gas_price,
            nonce,
            chain_id: Some(self.chain_id),
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

        let mut evm = Evm::builder()
            .with_db(self.evm.db.clone())
            .with_spec_id(self.spec_id)
            .with_env(Box::new(env))
            .build();

        let result = evm.transact()?;
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
        let result = self.submit_tx(Transaction {
            from,
            to: Some(to),
            value,
            data: Bytes::new(),
            gas_limit,
            gas_price,
            nonce,
            chain_id: Some(self.chain_id),
        });
        result.map_err(|err| anyhow::anyhow!("tx rejected: {} ({})", err.code(), err))
    }

    fn validate_tx_basic(&self, tx: &Transaction) -> Result<(), TxRejection> {
        if tx.gas_limit > self.gas_limit_per_block {
            return Err(TxRejection::GasLimitTooHigh);
        }
        if let Some(chain_id) = tx.chain_id {
            if chain_id != self.chain_id {
                return Err(TxRejection::InvalidChainId);
            }
        }
        Ok(())
    }

    fn validate_tx_state(&mut self, tx: &Transaction) -> Result<(), TxRejection> {
        let account = self
            .evm.db
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

        let mut evm = Evm::builder()
            .with_db(self.evm.db.clone())
            .with_spec_id(self.spec_id)
            .with_env(Box::new(env))
            .build();

        let result = evm.transact_commit()?;
        self.evm.db = std::mem::take(&mut evm.context.evm.db);
        let execution = match result {
            ExecutionResult::Success { gas_used, output, logs, .. } => TxExecution {
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

        Ok(execution)
    }

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
