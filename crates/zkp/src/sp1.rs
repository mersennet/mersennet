//! SP1 program envelope shared with `crates/core/src/zk_sp1.rs`.
//!
//! Until the real `sp1_sdk` is wired (Phase 5), this module provides
//! the type surface the SP1 state-transition program will read/write:
//! input bytes for what the program proves about, output bytes for
//! what becomes public. Everything is canonical bincode so the same
//! bytes work inside the zkVM and outside.

use crate::field::Fr;
use crate::merkle::MerkleTree;
use crate::noir::{Circuit, CircuitProof, default_verifier};
use crate::poseidon::Poseidon;
use crate::{NoteCommitment, Nullifier, NullifierSet};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::collections::{BTreeMap, HashMap, VecDeque};
use thiserror::Error;

/// Input to the SP1 block-proving program.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockProgramInput {
    pub prev_state_root: [u8; 32],
    pub prev_nullifier_root: [u8; 32],
    pub block_number: u64,
    pub timestamp: u64,
    /// Header witness for deterministic block-hash derivation inside
    /// the shared executor.
    pub header: BlockHeaderWitness,
    /// Bincoded list of transaction envelopes; the program decodes
    /// inside the zkVM. Includes shielded transfers, shielded order
    /// intents, liquidation auction settlements, and (legacy)
    /// transparent EVM transactions.
    pub txs: Vec<Vec<u8>>,
    /// Public market state at the start of the block.
    pub prev_market_state: Vec<u8>,
    /// Pre-block witness for the shielded note tree and nullifier set.
    pub prev_shielded_state: ShieldedStateWitness,
    /// Transparent balances touched by shield/unshield flows.
    pub transparent_balances: Vec<TransparentBalanceEntry>,
    /// Pre-tick witness for the FBA / liquidation settlement path.
    pub pre_tick_witness: ShieldedTickWitness,
    /// Chain-claimed block hash. The shared executor re-derives the
    /// hash from `header` and rejects mismatches.
    pub expected_block_hash: [u8; 32],
    /// Actual post-block market-state digest computed by the chain.
    pub expected_market_state_hash: [u8; 32],
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockHeaderWitness {
    pub chain_id: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub base_fee_be: [u8; 32],
    pub coinbase: [u8; 20],
    pub tx_count: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShieldedStateWitness {
    pub leaves: Vec<[u8; 32]>,
    pub nullifiers: Vec<[u8; 32]>,
    pub recent_roots: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TransparentBalanceEntry {
    pub address: [u8; 20],
    pub balance: U256Bytes,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShieldedTickWitness {
    pub drained_intent_count: u64,
    pub decrypted_intents: Vec<DecryptedIntentWitness>,
    pub orders: ShieldedOrdersTickWitness,
    pub liquidation: LiquidationAuctionTickWitness,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecryptedIntentWitness {
    pub intent_id: [u8; 32],
    pub plaintext: Vec<u8>,
    pub order_admission: Option<OrderAdmissionWitness>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderAdmissionWitness {
    pub sequence: u64,
    pub anchor_root: Fr,
    pub nullifier: Fr,
    pub new_commitment: Fr,
    pub market_id: u64,
    pub side_hash: Fr,
    pub price_band: u32,
    pub size_band: u32,
    pub oracle_price: U256Bytes,
    pub imm_required: U256Bytes,
    pub tif: u8,
    pub proof: CircuitProof,
    pub side: u8,
    pub price: U256Bytes,
    pub size: U256Bytes,
    pub owner_pk: Fr,
    pub salt: Fr,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShieldedOrdersTickWitness {
    pub initial_margin_bps: u64,
    pub maintenance_margin_bps: u64,
    pub next_sequence: u64,
    pub insurance_fund: U256Bytes,
    pub price_tick: U256Bytes,
    pub size_lot: U256Bytes,
    pub markets: Vec<ShieldedMarketWitness>,
    pub aggregates: Vec<ShieldedMarketAggregateWitness>,
    pub books: Vec<ShieldedOrderBookWitness>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShieldedMarketWitness {
    pub market_id: u64,
    pub tick_size: U256Bytes,
    pub lot_size: U256Bytes,
    pub last_price: U256Bytes,
    pub oracle_price: U256Bytes,
    pub status: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShieldedMarketAggregateWitness {
    pub market_id: u64,
    pub mark_price: U256Bytes,
    pub funding_rate_bps: i64,
    pub long_open_interest: U256Bytes,
    pub short_open_interest: U256Bytes,
    pub last_clearing_price: U256Bytes,
    pub bucketed_depth: Vec<ShieldedDepthLevelWitness>,
    pub liquidatable_count: u32,
    pub last_volume: U256Bytes,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShieldedDepthLevelWitness {
    pub price_band: u32,
    pub size: U256Bytes,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShieldedOrderBookWitness {
    pub market_id: u64,
    pub bids: Vec<PendingIntentWitness>,
    pub asks: Vec<PendingIntentWitness>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingIntentWitness {
    pub sequence: u64,
    pub side: u8,
    pub price: U256Bytes,
    pub size: U256Bytes,
    pub owner_pk: Fr,
    pub tif: u8,
    pub anchor_root: Fr,
    pub nullifier: Fr,
    pub new_commitment: Fr,
    pub market_id: u64,
    pub side_hash: Fr,
    pub price_band: u32,
    pub size_band: u32,
    pub oracle_price: U256Bytes,
    pub imm_required: U256Bytes,
    pub encrypted_payload: Vec<u8>,
    pub proof: CircuitProof,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidationAuctionTickWitness {
    pub liquidators: Vec<LiquidatorWitness>,
    pub auctions: Vec<LiquidationAuctionEntryWitness>,
    pub pending_revelation: Vec<RevealedBidBatchWitness>,
    pub insurance_fund: U256Bytes,
    pub stats: LiquidationAuctionStatsWitness,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidatorWitness {
    pub bond_commitment: Fr,
    pub bond_amount: u128,
    pub registered_at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidationAuctionEntryWitness {
    pub claim_tag: Fr,
    pub claims: Vec<LiquidationClaimWitness>,
    pub bids: Vec<LiquidationBidWitness>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiquidationClaimWitness {
    pub anchor_root: Fr,
    pub market_id: u64,
    pub oracle_price: U256Bytes,
    pub liquidator_id: Fr,
    pub claim_tag: Fr,
    pub encrypted_bid: Vec<u8>,
    pub proof: CircuitProof,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidationBidWitness {
    pub claim_tag: Fr,
    pub liquidator_id: Fr,
    pub bid_price: U256Bytes,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RevealedBidBatchWitness {
    pub block_number: u64,
    pub bids: Vec<LiquidationBidWitness>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OrderAdmissionValidationContext {
    pub market_present: bool,
    pub market_status: u8,
    pub market_oracle_price: U256Bytes,
    pub initial_margin_bps: u64,
    pub price_tick: U256Bytes,
    pub size_lot: U256Bytes,
    pub root_is_recent: bool,
    pub nullifier_spent: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidationClaimValidationContext {
    pub root_is_recent: bool,
    pub liquidator_registered: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidationWinnerWitness {
    pub claim_tag: Fr,
    pub liquidator_id: Fr,
    pub bid_price: U256Bytes,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidationSettlementWitness {
    pub winners: Vec<LiquidationWinnerWitness>,
    pub events: Vec<CanonicalShieldedEvent>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LiquidationAuctionStatsWitness {
    pub claims_received: u64,
    pub claims_rejected: u64,
    pub auctions_settled: u64,
    pub total_bounty_paid: U256Bytes,
    pub total_insurance_funded: U256Bytes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct U256Bytes(pub [u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ShieldedBlockTx {
    Transfer(ShieldedTransferBlockTx),
    Shield(ShieldBlockTx),
    Unshield(UnshieldBlockTx),
    LiquidationExecute(LiquidationExecuteBlockTx),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShieldedTransferBlockTx {
    pub anchor_root: Fr,
    pub nullifier: Fr,
    pub output_commitment: Fr,
    pub encrypted_output: Vec<u8>,
    pub proof: CircuitProof,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShieldBlockTx {
    pub from: [u8; 20],
    pub amount: U256Bytes,
    pub output_commitment: Fr,
    pub encrypted_output: Vec<u8>,
    pub proof: CircuitProof,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnshieldBlockTx {
    pub anchor_root: Fr,
    pub nullifier: Fr,
    pub amount: U256Bytes,
    pub to: [u8; 20],
    pub change_commitment: Fr,
    pub encrypted_change: Vec<u8>,
    pub proof: CircuitProof,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiquidationExecuteBlockTx {
    pub anchor_root: Fr,
    pub claim_tag: Fr,
    pub victim_nullifier: Fr,
    pub bounty_commitment: Fr,
    pub insurance_commitment: Fr,
    pub winning_bid: U256Bytes,
    pub market_id: u64,
    pub oracle_price: U256Bytes,
    pub proof: CircuitProof,
}

/// Public output of the SP1 program. Becomes `public_values` of the
/// resulting proof.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockProgramOutput {
    pub prev_state_root: [u8; 32],
    pub new_state_root: [u8; 32],
    pub prev_nullifier_root: [u8; 32],
    pub new_nullifier_root: [u8; 32],
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub new_market_state_hash: [u8; 32],
    pub shielded_event_root: [u8; 32],
    pub tx_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CanonicalShieldedEvent {
    FbaCleared {
        market_id: u64,
        clearing_price: U256Bytes,
        matched_size: U256Bytes,
        intent_count: u64,
    },
    MempoolBatchAdmitted {
        block_number: u64,
        intent_count: u64,
    },
    LiquidationSettled {
        market_id: u64,
        winner_bond_commitment: [u8; 32],
        winning_bid: U256Bytes,
    },
    ShieldedRootAdvanced {
        block_number: u64,
        new_root: [u8; 32],
        notes_added: u64,
        nullifiers_added: u64,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MarketTickTransitionWitness {
    pub aggregate: ShieldedMarketAggregateWitness,
    pub book: ShieldedOrderBookWitness,
    pub event: Option<CanonicalShieldedEvent>,
    pub fills: Vec<MarketFillWitness>,
    pub fill_count: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MarketFillWitness {
    pub market_id: u64,
    pub clearing_price: U256Bytes,
    pub size: U256Bytes,
    pub side: u8,
    pub recipient: Fr,
}

#[derive(Debug, Error)]
pub enum BlockProgramError {
    #[error("invalid block tx encoding at index {index}: {message}")]
    InvalidTxEncoding { index: usize, message: String },
    #[error("anchor root is not in the recent-roots window")]
    StaleAnchor,
    #[error("liquidator is not registered or below the minimum bond")]
    UnregisteredLiquidator,
    #[error("nullifier already spent")]
    DoubleSpend,
    #[error("transparent balance is too low to shield")]
    InsufficientTransparentBalance,
    #[error("zk proof verification failed")]
    InvalidProof,
    #[error("unknown market in tick witness: {0}")]
    UnknownMarket(u64),
    #[error("inactive market in tick witness: {0}")]
    MarketInactive(u64),
    #[error("order oracle price mismatch")]
    OraclePriceMismatch,
    #[error("order price-band mismatch")]
    PriceBandMismatch,
    #[error("order size-band mismatch")]
    SizeBandViolation,
    #[error("order side-hash mismatch")]
    SideHashMismatch,
    #[error("order initial-margin mismatch")]
    InitialMarginMismatch,
    #[error("replayed block-hash mismatch")]
    BlockHashMismatch,
    #[error("replayed market-state hash mismatch")]
    MarketStateHashMismatch,
}

/// Canonical deterministic block executor used by the current SP1
/// proving path. This consumes the private witness and derives the
/// public output without relying on host-supplied post-state fields.
pub fn execute_block_program(input: &BlockProgramInput) -> Result<BlockProgramOutput, BlockProgramError> {
    let mut state = ReplayShieldedState::restore(&input.prev_shielded_state);
    let mut transparent_balances = input
        .transparent_balances
        .iter()
        .map(|entry| (entry.address, entry.balance))
        .collect::<HashMap<_, _>>();
    let verifier = default_verifier();
    let poseidon = Poseidon::default();

    for (index, raw_tx) in input.txs.iter().enumerate() {
        let tx: ShieldedBlockTx = bincode::deserialize(raw_tx).map_err(|err| {
            BlockProgramError::InvalidTxEncoding {
                index,
                message: err.to_string(),
            }
        })?;
        apply_block_tx(&mut state, &mut transparent_balances, &*verifier, &tx)?;
    }

    let (new_market_state_hash, fba_events) = if has_tick_replay_inputs(&input.pre_tick_witness) {
        replay_tick_market_state(
            &mut state,
            &*verifier,
            &poseidon,
            &input.pre_tick_witness,
        )?
    } else {
        (input.expected_market_state_hash, Vec::new())
    };
    let liquidation_events = replay_liquidation_events(&state, &*verifier, &input.pre_tick_witness)?;
    let shielded_event_root = derive_shielded_event_root(
        input.block_number,
        state.current_root().to_bytes(),
        input.pre_tick_witness.drained_intent_count,
        &fba_events,
        &liquidation_events,
    );
    let block_hash = derive_block_hash(input.block_number, &input.header);

    if block_hash != input.expected_block_hash {
        return Err(BlockProgramError::BlockHashMismatch);
    }

    if new_market_state_hash != input.expected_market_state_hash {
        return Err(BlockProgramError::MarketStateHashMismatch);
    }

    Ok(BlockProgramOutput {
        prev_state_root: input.prev_state_root,
        new_state_root: state.current_root().to_bytes(),
        prev_nullifier_root: input.prev_nullifier_root,
        new_nullifier_root: state.nullifier_root(),
        block_number: input.block_number,
        block_hash,
        new_market_state_hash,
        shielded_event_root,
        tx_count: input.header.tx_count,
    })
}

pub fn derive_block_hash(block_number: u64, header: &BlockHeaderWitness) -> [u8; 32] {
    let mut payload = Vec::with_capacity(8 + 8 + 8 + 8 + 32 + 20 + 8);
    payload.extend_from_slice(&block_number.to_be_bytes());
    payload.extend_from_slice(&header.chain_id.to_be_bytes());
    payload.extend_from_slice(&header.gas_limit.to_be_bytes());
    payload.extend_from_slice(&header.gas_used.to_be_bytes());
    payload.extend_from_slice(&header.base_fee_be);
    payload.extend_from_slice(&header.coinbase);
    payload.extend_from_slice(&header.tx_count.to_be_bytes());
    let digest = Keccak256::digest(&payload);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn has_tick_replay_inputs(witness: &ShieldedTickWitness) -> bool {
    !witness.decrypted_intents.is_empty()
        || !witness.orders.markets.is_empty()
        || !witness.orders.aggregates.is_empty()
        || !witness.orders.books.is_empty()
        || !witness.liquidation.liquidators.is_empty()
        || !witness.liquidation.auctions.is_empty()
        || !witness.liquidation.pending_revelation.is_empty()
        || witness.orders.initial_margin_bps != 0
        || witness.orders.maintenance_margin_bps != 0
        || witness.orders.next_sequence != 0
        || witness.orders.insurance_fund != U256Bytes::default()
        || witness.orders.price_tick != U256Bytes::default()
        || witness.orders.size_lot != U256Bytes::default()
        || witness.liquidation.insurance_fund != U256Bytes::default()
        || !witness.liquidation.stats.is_zero()
}

fn replay_tick_market_state(
    state: &mut ReplayShieldedState,
    verifier: &dyn crate::noir::Verifier,
    poseidon: &Poseidon,
    witness: &ShieldedTickWitness,
) -> Result<([u8; 32], Vec<CanonicalShieldedEvent>), BlockProgramError> {
    let admissions = witness
        .decrypted_intents
        .iter()
        .filter_map(|intent| intent.order_admission.clone())
        .collect::<Vec<_>>();
    let mut orders = if admissions.is_empty() {
        ReplayShieldedOrders::from_books(&witness.orders)
    } else {
        ReplayShieldedOrders::from_admissions(&witness.orders)
    };

    if !admissions.is_empty() {
        let mut admissions = admissions;
        admissions.sort_by_key(|admission| admission.sequence);
        for admission in admissions {
            replay_order_admission(state, verifier, poseidon, &mut orders, &admission)?;
        }
    }

    let mut events = Vec::new();
    let mut market_ids: Vec<_> = orders.aggregates.keys().copied().collect();
    market_ids.sort_unstable();
    for market_id in market_ids {
        if let (Some(book), Some(aggregate)) = (
            orders.books.get(&market_id),
            orders.aggregates.get(&market_id),
        ) {
            let transition = apply_market_tick_witness(
                aggregate.clone(),
                book.to_witness(market_id),
            );
            orders.aggregates.insert(market_id, transition.aggregate.clone());
            orders.books.insert(market_id, ReplayOrderBook::from_witness(&transition.book));
            if let Some(event) = transition.event {
                events.push(event);
            }
        }
    }

    let mut aggregates = orders.aggregates.values().cloned().collect::<Vec<_>>();
    aggregates.sort_by_key(|aggregate| aggregate.market_id);
    Ok((hash_market_aggregates(&aggregates), events))
}

fn replay_order_admission(
    state: &mut ReplayShieldedState,
    verifier: &dyn crate::noir::Verifier,
    poseidon: &Poseidon,
    orders: &mut ReplayShieldedOrders,
    admission: &OrderAdmissionWitness,
) -> Result<(), BlockProgramError> {
    let market = orders
        .markets
        .get(&admission.market_id)
        .ok_or(BlockProgramError::UnknownMarket(admission.market_id))?;
    validate_order_admission_witness(
        verifier,
        poseidon,
        admission,
        &OrderAdmissionValidationContext {
            market_present: true,
            market_status: market.status,
            market_oracle_price: market.oracle_price,
            initial_margin_bps: orders.initial_margin_bps,
            price_tick: orders.price_tick,
            size_lot: orders.size_lot,
            root_is_recent: state.is_recent_root(&admission.anchor_root),
            nullifier_spent: state.is_spent(&Nullifier(admission.nullifier)),
        },
    )?;
    state.spend(Nullifier(admission.nullifier))?;
    state.insert_note(NoteCommitment(admission.new_commitment));

    let book = orders.books.entry(admission.market_id).or_default();
    let levels = match admission.side {
        0 => &mut book.bids,
        1 => &mut book.asks,
        _ => unreachable!(),
    };
    levels.entry(admission.price).or_default().push(ReplayPendingIntent {
        pending: PendingIntentWitness {
            sequence: admission.sequence,
            side: admission.side,
            price: admission.price,
            size: admission.size,
            owner_pk: admission.owner_pk,
            tif: admission.tif,
            anchor_root: admission.anchor_root,
            nullifier: admission.nullifier,
            new_commitment: admission.new_commitment,
            market_id: admission.market_id,
            side_hash: admission.side_hash,
            price_band: admission.price_band,
            size_band: admission.size_band,
            oracle_price: admission.oracle_price,
            imm_required: admission.imm_required,
            encrypted_payload: Vec::new(),
            proof: admission.proof.clone(),
        },
    });
    if let Some(aggregate) = orders.aggregates.get_mut(&admission.market_id) {
        if let Some(depth) = aggregate
            .bucketed_depth
            .iter_mut()
            .find(|depth| depth.price_band == admission.price_band)
        {
            depth.size = depth.size.saturating_add(admission.size);
        } else {
            aggregate.bucketed_depth.push(ShieldedDepthLevelWitness {
                price_band: admission.price_band,
                size: admission.size,
            });
            aggregate
                .bucketed_depth
                .sort_by_key(|depth| depth.price_band);
        }
    }
    Ok(())
}

fn derive_imm_required(
    price_band: u32,
    size_band: u32,
    initial_margin_bps: u64,
    price_tick: U256Bytes,
    size_lot: U256Bytes,
) -> U256Bytes {
    let price = u128::from(price_tick.low_u64()).saturating_mul(u128::from(price_band));
    let size = u128::from(size_lot.low_u64()).saturating_mul(u128::from(size_band));
    let notional = price.saturating_mul(size);
    let numerator = notional.saturating_mul(u128::from(initial_margin_bps));
    U256Bytes::from_u128(numerator.div_ceil(10_000))
}

pub fn validate_order_admission_witness(
    verifier: &dyn crate::noir::Verifier,
    poseidon: &Poseidon,
    admission: &OrderAdmissionWitness,
    context: &OrderAdmissionValidationContext,
) -> Result<(), BlockProgramError> {
    if !context.root_is_recent {
        return Err(BlockProgramError::StaleAnchor);
    }
    if !context.market_present {
        return Err(BlockProgramError::UnknownMarket(admission.market_id));
    }
    if context.market_status != 0 {
        return Err(BlockProgramError::MarketInactive(admission.market_id));
    }
    if context.market_oracle_price != admission.oracle_price {
        return Err(BlockProgramError::OraclePriceMismatch);
    }
    let derived_imm = derive_imm_required(
        admission.price_band,
        admission.size_band,
        context.initial_margin_bps,
        context.price_tick,
        context.size_lot,
    );
    if derived_imm != admission.imm_required {
        return Err(BlockProgramError::InitialMarginMismatch);
    }
    let actual_price_band = admission.price.div_floor_u64(context.price_tick.low_u64());
    if actual_price_band > u64::from(u32::MAX) || actual_price_band as u32 != admission.price_band {
        return Err(BlockProgramError::PriceBandMismatch);
    }
    let actual_size_band = admission.size.div_ceil_u64(context.size_lot.low_u64());
    if actual_size_band > u64::from(u32::MAX) || actual_size_band as u32 > admission.size_band {
        return Err(BlockProgramError::SizeBandViolation);
    }
    let side_fr = match admission.side {
        0 => Fr::ZERO,
        1 => Fr::ONE,
        _ => return Err(BlockProgramError::InvalidTxEncoding { index: 0, message: "invalid order side".to_string() }),
    };
    let derived_side_hash = poseidon.hash_two(&side_fr, &admission.salt);
    if derived_side_hash != admission.side_hash {
        return Err(BlockProgramError::SideHashMismatch);
    }
    let public_inputs = vec![
        admission.anchor_root,
        admission.nullifier,
        admission.new_commitment,
        Fr::from_u64(admission.market_id),
        admission.side_hash,
        Fr::from_u64(admission.price_band as u64),
        Fr::from_u64(admission.size_band as u64),
        Fr::from_u64(admission.oracle_price.low_u64()),
        Fr::from_u64(admission.imm_required.low_u64()),
    ];
    verifier
        .verify(&admission.proof, Circuit::OrderPlace, &public_inputs)
        .map_err(|_| BlockProgramError::InvalidProof)?;
    if context.nullifier_spent {
        return Err(BlockProgramError::DoubleSpend);
    }
    Ok(())
}

pub fn select_liquidation_winner(
    entry: &LiquidationAuctionEntryWitness,
) -> Option<LiquidationWinnerWitness> {
    entry
        .bids
        .iter()
        .max_by(|left, right| left.bid_price.cmp(&right.bid_price))
        .map(|winner| LiquidationWinnerWitness {
            claim_tag: entry.claim_tag,
            liquidator_id: winner.liquidator_id,
            bid_price: winner.bid_price,
        })
}

pub fn validate_liquidation_claim_witness(
    claim: &LiquidationClaimWitness,
    verifier: &dyn crate::noir::Verifier,
    context: &LiquidationClaimValidationContext,
) -> Result<(), BlockProgramError> {
    if !context.root_is_recent {
        return Err(BlockProgramError::StaleAnchor);
    }
    if !context.liquidator_registered {
        return Err(BlockProgramError::UnregisteredLiquidator);
    }
    let public_inputs = vec![
        claim.anchor_root,
        Fr::from_u64(claim.market_id),
        Fr::from_u64(claim.oracle_price.low_u64()),
        claim.liquidator_id,
        claim.claim_tag,
    ];
    verifier
        .verify(&claim.proof, Circuit::LiquidateClaim, &public_inputs)
        .map_err(|_| BlockProgramError::InvalidProof)?;
    Ok(())
}

pub fn canonical_liquidation_winners(
    entries: &[LiquidationAuctionEntryWitness],
) -> Vec<LiquidationWinnerWitness> {
    let mut ordered = entries.to_vec();
    ordered.sort_by_key(|entry| entry.claim_tag.to_bytes());
    ordered
        .iter()
        .filter_map(select_liquidation_winner)
        .collect()
}

pub fn liquidation_events_from_winners(
    winners: &[LiquidationWinnerWitness],
) -> Vec<CanonicalShieldedEvent> {
    winners
        .iter()
        .map(|winner| CanonicalShieldedEvent::LiquidationSettled {
            market_id: 0,
            winner_bond_commitment: winner.liquidator_id.to_bytes(),
            winning_bid: winner.bid_price,
        })
        .collect()
}

pub fn settle_liquidation_entries(
    entries: &[LiquidationAuctionEntryWitness],
) -> LiquidationSettlementWitness {
    let winners = canonical_liquidation_winners(entries);
    let events = liquidation_events_from_winners(&winners);
    LiquidationSettlementWitness { winners, events }
}

fn replay_liquidation_events(
    state: &ReplayShieldedState,
    verifier: &dyn crate::noir::Verifier,
    witness: &ShieldedTickWitness,
) -> Result<Vec<CanonicalShieldedEvent>, BlockProgramError> {
    let entries = witness.liquidation.auctions.clone();
    let liquidators = witness
        .liquidation
        .liquidators
        .iter()
        .map(|liquidator| liquidator.bond_commitment)
        .collect::<Vec<_>>();
    let mut valid_entries = Vec::new();

    for entry in entries {
        let mut has_valid_claim = false;
        for claim in &entry.claims {
            let context = LiquidationClaimValidationContext {
                root_is_recent: state.is_recent_root(&claim.anchor_root),
                liquidator_registered: liquidators.iter().any(|bond| bond == &claim.liquidator_id),
            };
            if validate_liquidation_claim_witness(claim, verifier, &context).is_ok() {
                has_valid_claim = true;
            }
        }
        if !has_valid_claim {
            continue;
        }
        valid_entries.push(entry);
    }

    Ok(settle_liquidation_entries(&valid_entries).events)
}

fn derive_shielded_event_root(
    block_number: u64,
    new_root: [u8; 32],
    drained_intent_count: u64,
    fba_events: &[CanonicalShieldedEvent],
    liquidation_events: &[CanonicalShieldedEvent],
) -> [u8; 32] {
    let events = build_shielded_tick_events(
        block_number,
        drained_intent_count,
        fba_events,
        liquidation_events,
        new_root,
    );
    shielded_event_root(&events)
}

pub fn build_shielded_tick_events(
    block_number: u64,
    drained_intent_count: u64,
    fba_events: &[CanonicalShieldedEvent],
    liquidation_events: &[CanonicalShieldedEvent],
    new_root: [u8; 32],
) -> Vec<CanonicalShieldedEvent> {
    let mut events = Vec::with_capacity(
        usize::from(drained_intent_count != 0) + fba_events.len() + liquidation_events.len() + 1,
    );
    if drained_intent_count != 0 {
        events.push(CanonicalShieldedEvent::MempoolBatchAdmitted {
            block_number,
            intent_count: drained_intent_count,
        });
    }
    events.extend_from_slice(fba_events);
    events.extend_from_slice(liquidation_events);
    events.push(CanonicalShieldedEvent::ShieldedRootAdvanced {
        block_number,
        new_root,
        notes_added: 0,
        nullifiers_added: 0,
    });
    events
}

pub fn shielded_event_root(events: &[CanonicalShieldedEvent]) -> [u8; 32] {
    if events.is_empty() {
        return [0u8; 32];
    }
    let mut encoded = Vec::new();
    for event in events {
        if let Ok(bytes) = bincode::serialize(event) {
            encoded.extend_from_slice(&bytes);
        }
    }
    let digest = Keccak256::digest(&encoded);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub fn hash_market_aggregates(aggregates: &[ShieldedMarketAggregateWitness]) -> [u8; 32] {
    let mut aggregates = aggregates.to_vec();
    aggregates.sort_by_key(|aggregate| aggregate.market_id);

    let mut buf = Vec::new();
    for aggregate in aggregates {
        buf.extend_from_slice(&aggregate.market_id.to_le_bytes());
        buf.extend_from_slice(&aggregate.mark_price.0);
        buf.extend_from_slice(&aggregate.long_open_interest.0);
        buf.extend_from_slice(&aggregate.short_open_interest.0);
        buf.extend_from_slice(&aggregate.last_clearing_price.0);
        buf.extend_from_slice(&aggregate.last_volume.0);
        buf.extend_from_slice(&aggregate.liquidatable_count.to_le_bytes());
    }

    let digest = Keccak256::digest(&buf);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub fn hash_liquidation_auction_stats(stats: &LiquidationAuctionStatsWitness) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(&stats.claims_received.to_le_bytes());
    buf.extend_from_slice(&stats.claims_rejected.to_le_bytes());
    buf.extend_from_slice(&stats.auctions_settled.to_le_bytes());
    buf.extend_from_slice(&stats.total_bounty_paid.0);
    buf.extend_from_slice(&stats.total_insurance_funded.0);

    let digest = Keccak256::digest(&buf);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn apply_block_tx(
    state: &mut ReplayShieldedState,
    transparent_balances: &mut HashMap<[u8; 20], U256Bytes>,
    verifier: &dyn crate::noir::Verifier,
    tx: &ShieldedBlockTx,
) -> Result<(), BlockProgramError> {
    match tx {
        ShieldedBlockTx::Transfer(tx) => {
            if !state.is_recent_root(&tx.anchor_root) {
                return Err(BlockProgramError::StaleAnchor);
            }
            let public_inputs = vec![
                tx.anchor_root,
                tx.nullifier,
                tx.output_commitment,
                Fr::ZERO,
            ];
            verifier
                .verify(&tx.proof, Circuit::Spend, &public_inputs)
                .map_err(|_| BlockProgramError::InvalidProof)?;
            if state.is_spent(&Nullifier(tx.nullifier)) {
                return Err(BlockProgramError::DoubleSpend);
            }
            state.spend(Nullifier(tx.nullifier))?;
            state.insert_note(NoteCommitment(tx.output_commitment));
            Ok(())
        }
        ShieldedBlockTx::Shield(tx) => {
            let Some(balance) = transparent_balances.get_mut(&tx.from) else {
                return Err(BlockProgramError::InsufficientTransparentBalance);
            };
            *balance = balance
                .checked_sub(tx.amount)
                .ok_or(BlockProgramError::InsufficientTransparentBalance)?;
            let public_inputs = vec![
                tx.output_commitment,
                Fr::ZERO,
                Fr::from_u64(tx.amount.low_u64()),
            ];
            verifier
                .verify(&tx.proof, Circuit::Output, &public_inputs)
                .map_err(|_| BlockProgramError::InvalidProof)?;
            state.insert_note(NoteCommitment(tx.output_commitment));
            Ok(())
        }
        ShieldedBlockTx::Unshield(tx) => {
            if !state.is_recent_root(&tx.anchor_root) {
                return Err(BlockProgramError::StaleAnchor);
            }
            let public_inputs = vec![
                tx.anchor_root,
                tx.nullifier,
                tx.change_commitment,
                Fr::from_u64(tx.amount.low_u64()),
            ];
            verifier
                .verify(&tx.proof, Circuit::Spend, &public_inputs)
                .map_err(|_| BlockProgramError::InvalidProof)?;
            if state.is_spent(&Nullifier(tx.nullifier)) {
                return Err(BlockProgramError::DoubleSpend);
            }
            state.spend(Nullifier(tx.nullifier))?;
            if tx.change_commitment != Fr::ZERO {
                state.insert_note(NoteCommitment(tx.change_commitment));
            }
            let balance = transparent_balances.entry(tx.to).or_default();
            *balance = balance.saturating_add(tx.amount);
            Ok(())
        }
        ShieldedBlockTx::LiquidationExecute(tx) => {
            if !state.is_recent_root(&tx.anchor_root) {
                return Err(BlockProgramError::StaleAnchor);
            }
            let public_inputs = vec![
                tx.anchor_root,
                tx.victim_nullifier,
                tx.bounty_commitment,
                tx.insurance_commitment,
                Fr::from_u64(tx.winning_bid.low_u64()),
                Fr::from_u64(tx.market_id),
                Fr::from_u64(tx.oracle_price.low_u64()),
            ];
            verifier
                .verify(&tx.proof, Circuit::LiquidateExecute, &public_inputs)
                .map_err(|_| BlockProgramError::InvalidProof)?;
            if state.is_spent(&Nullifier(tx.victim_nullifier)) {
                return Err(BlockProgramError::DoubleSpend);
            }
            state.spend(Nullifier(tx.victim_nullifier))?;
            state.insert_note(NoteCommitment(tx.bounty_commitment));
            state.insert_note(NoteCommitment(tx.insurance_commitment));
            Ok(())
        }
    }
}

#[derive(Clone, Debug)]
struct ReplayShieldedState {
    tree: MerkleTree,
    nullifiers: NullifierSet,
    recent_roots: VecDeque<Fr>,
}

#[derive(Clone, Debug, Default)]
struct ReplayOrderBook {
    bids: BTreeMap<U256Bytes, Vec<ReplayPendingIntent>>,
    asks: BTreeMap<U256Bytes, Vec<ReplayPendingIntent>>,
}

#[derive(Clone, Debug)]
struct ReplayPendingIntent {
    pending: PendingIntentWitness,
}

#[derive(Clone, Debug, Default)]
struct ReplayClearingResult {
    clearing_price: U256Bytes,
    matched_size: U256Bytes,
    fill_count: u64,
    fills: Vec<MarketFillWitness>,
}

#[derive(Clone, Debug, Default)]
struct ReplayShieldedOrders {
    initial_margin_bps: u64,
    price_tick: U256Bytes,
    size_lot: U256Bytes,
    markets: HashMap<u64, ReplayMarket>,
    aggregates: HashMap<u64, ShieldedMarketAggregateWitness>,
    books: HashMap<u64, ReplayOrderBook>,
}

#[derive(Clone, Debug)]
struct ReplayMarket {
    status: u8,
    oracle_price: U256Bytes,
}

impl ReplayOrderBook {
    fn from_witness(witness: &ShieldedOrderBookWitness) -> Self {
        Self {
            bids: replay_levels(&witness.bids),
            asks: replay_levels(&witness.asks),
        }
    }

    fn to_witness(&self, market_id: u64) -> ShieldedOrderBookWitness {
        ShieldedOrderBookWitness {
            market_id,
            bids: replay_pending_witnesses(&self.bids),
            asks: replay_pending_witnesses(&self.asks),
        }
    }
}

impl ReplayShieldedOrders {
    fn from_books(witness: &ShieldedOrdersTickWitness) -> Self {
        Self {
            initial_margin_bps: witness.initial_margin_bps,
            price_tick: witness.price_tick,
            size_lot: witness.size_lot,
            markets: witness
                .markets
                .iter()
                .map(|market| {
                    (
                        market.market_id,
                        ReplayMarket {
                            status: market.status,
                            oracle_price: market.oracle_price,
                        },
                    )
                })
                .collect(),
            aggregates: witness
                .aggregates
                .iter()
                .cloned()
                .map(|aggregate| (aggregate.market_id, aggregate))
                .collect(),
            books: witness
                .books
                .iter()
                .map(|book| (book.market_id, ReplayOrderBook::from_witness(book)))
                .collect(),
        }
    }

    fn from_admissions(witness: &ShieldedOrdersTickWitness) -> Self {
        Self {
            initial_margin_bps: witness.initial_margin_bps,
            price_tick: witness.price_tick,
            size_lot: witness.size_lot,
            markets: witness
                .markets
                .iter()
                .map(|market| {
                    (
                        market.market_id,
                        ReplayMarket {
                            status: market.status,
                            oracle_price: market.oracle_price,
                        },
                    )
                })
                .collect(),
            aggregates: witness
                .aggregates
                .iter()
                .cloned()
                .map(|mut aggregate| {
                    aggregate.bucketed_depth.clear();
                    (aggregate.market_id, aggregate)
                })
                .collect(),
            books: witness
                .markets
                .iter()
                .map(|market| (market.market_id, ReplayOrderBook::default()))
                .collect(),
        }
    }
}

fn replay_levels(intents: &[PendingIntentWitness]) -> BTreeMap<U256Bytes, Vec<ReplayPendingIntent>> {
    let mut levels = BTreeMap::<U256Bytes, Vec<ReplayPendingIntent>>::new();
    for intent in intents {
        levels.entry(intent.price).or_default().push(ReplayPendingIntent {
            pending: intent.clone(),
        });
    }
    for level in levels.values_mut() {
        level.sort_by_key(|intent| intent.pending.sequence);
    }
    levels
}

fn replay_pending_witnesses(
    levels: &BTreeMap<U256Bytes, Vec<ReplayPendingIntent>>,
) -> Vec<PendingIntentWitness> {
    levels
        .values()
        .flat_map(|level| level.iter())
        .map(|pending| pending.pending.clone())
        .collect()
}

fn apply_uniform_price_auction_witness(book: &ReplayOrderBook) -> (ReplayOrderBook, ReplayClearingResult) {
    if book.bids.is_empty() || book.asks.is_empty() {
        return (book.clone(), ReplayClearingResult::default());
    }

    let mut prices: Vec<U256Bytes> = book.bids.keys().chain(book.asks.keys()).copied().collect();
    prices.sort();
    prices.dedup();

    let mut best_price = U256Bytes::default();
    let mut best_volume = U256Bytes::default();

    for price in &prices {
        let cum_bids = book
            .bids
            .iter()
            .filter(|(level_price, _)| *level_price >= price)
            .flat_map(|(_, intents)| intents.iter())
            .fold(U256Bytes::default(), |acc, intent| acc.saturating_add(intent.pending.size));
        let cum_asks = book
            .asks
            .iter()
            .filter(|(level_price, _)| *level_price <= price)
            .flat_map(|(_, intents)| intents.iter())
            .fold(U256Bytes::default(), |acc, intent| acc.saturating_add(intent.pending.size));
        let matched = cum_bids.min(cum_asks);
        if matched > best_volume {
            best_volume = matched;
            best_price = *price;
        }
    }

    if best_volume.is_zero() {
        return (book.clone(), ReplayClearingResult::default());
    }

    let mut bids = book.bids.clone();
    let mut asks = book.asks.clone();
    let mut remaining_bid = best_volume;
    let mut remaining_ask = best_volume;
    let mut fill_count = 0u64;
    let mut fills = Vec::new();

    let mut bid_prices = bids.keys().copied().filter(|price| *price >= best_price).collect::<Vec<_>>();
    bid_prices.sort();
    bid_prices.reverse();
    for price in bid_prices {
        if remaining_bid.is_zero() {
            break;
        }
        if let Some(level) = bids.get_mut(&price) {
            for pending in level.iter_mut() {
                if remaining_bid.is_zero() {
                    break;
                }
                let fill_size = pending.pending.size.min(remaining_bid);
                if !fill_size.is_zero() {
                    fill_count = fill_count.saturating_add(1);
                    fills.push(MarketFillWitness {
                        market_id: pending.pending.market_id,
                        clearing_price: best_price,
                        size: fill_size,
                        side: pending.pending.side,
                        recipient: pending.pending.owner_pk,
                    });
                }
                pending.pending.size = pending.pending.size.checked_sub(fill_size).unwrap_or_default();
                remaining_bid = remaining_bid.checked_sub(fill_size).unwrap_or_default();
            }
        }
    }

    let mut ask_prices = asks.keys().copied().filter(|price| *price <= best_price).collect::<Vec<_>>();
    ask_prices.sort();
    for price in ask_prices {
        if remaining_ask.is_zero() {
            break;
        }
        if let Some(level) = asks.get_mut(&price) {
            for pending in level.iter_mut() {
                if remaining_ask.is_zero() {
                    break;
                }
                let fill_size = pending.pending.size.min(remaining_ask);
                if !fill_size.is_zero() {
                    fill_count = fill_count.saturating_add(1);
                    fills.push(MarketFillWitness {
                        market_id: pending.pending.market_id,
                        clearing_price: best_price,
                        size: fill_size,
                        side: pending.pending.side,
                        recipient: pending.pending.owner_pk,
                    });
                }
                pending.pending.size = pending.pending.size.checked_sub(fill_size).unwrap_or_default();
                remaining_ask = remaining_ask.checked_sub(fill_size).unwrap_or_default();
            }
        }
    }

    bids.retain(|_, level| {
        level.retain(|pending| !pending.pending.size.is_zero());
        !level.is_empty()
    });
    asks.retain(|_, level| {
        level.retain(|pending| !pending.pending.size.is_zero());
        !level.is_empty()
    });

    (
        ReplayOrderBook { bids, asks },
        ReplayClearingResult {
            clearing_price: best_price,
            matched_size: best_volume,
            fill_count,
            fills,
        },
    )
}

pub fn apply_market_tick_witness(
    mut aggregate: ShieldedMarketAggregateWitness,
    book: ShieldedOrderBookWitness,
) -> MarketTickTransitionWitness {
    let (next_book, result) = apply_uniform_price_auction_witness(&ReplayOrderBook::from_witness(&book));
    if !result.matched_size.is_zero() {
        aggregate.last_clearing_price = result.clearing_price;
        aggregate.mark_price = result.clearing_price;
        aggregate.last_volume = result.matched_size;
    }
    let event = (!result.matched_size.is_zero()).then_some(CanonicalShieldedEvent::FbaCleared {
        market_id: aggregate.market_id,
        clearing_price: result.clearing_price,
        matched_size: result.matched_size,
        intent_count: result.fill_count,
    });
    MarketTickTransitionWitness {
        aggregate: aggregate.clone(),
        book: next_book.to_witness(aggregate.market_id),
        event,
        fills: result.fills.clone(),
        fill_count: result.fill_count,
    }
}

impl ReplayShieldedState {
    fn restore(witness: &ShieldedStateWitness) -> Self {
        let mut tree = MerkleTree::new();
        for leaf in &witness.leaves {
            tree.insert(Fr::from_bytes_reduce(leaf));
        }

        let mut nullifiers = NullifierSet::new();
        for nullifier in &witness.nullifiers {
            nullifiers.insert(Nullifier(Fr::from_bytes_reduce(nullifier)));
        }

        let mut recent_roots = VecDeque::new();
        if witness.recent_roots.is_empty() {
            recent_roots.push_back(tree.root());
        } else {
            for root in &witness.recent_roots {
                recent_roots.push_back(Fr::from_bytes_reduce(root));
            }
        }

        Self {
            tree,
            nullifiers,
            recent_roots,
        }
    }

    fn current_root(&self) -> Fr {
        self.tree.root()
    }

    fn nullifier_root(&self) -> [u8; 32] {
        let mut buf = Vec::with_capacity(16);
        buf.extend_from_slice(&(self.nullifiers.len() as u64).to_le_bytes());
        let digest = Keccak256::digest(&buf);
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        out
    }

    fn is_recent_root(&self, root: &Fr) -> bool {
        self.recent_roots.iter().any(|candidate| candidate == root)
    }

    fn is_spent(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.contains(nullifier)
    }

    fn spend(&mut self, nullifier: Nullifier) -> Result<(), BlockProgramError> {
        if !self.nullifiers.insert(nullifier) {
            return Err(BlockProgramError::DoubleSpend);
        }
        Ok(())
    }

    fn insert_note(&mut self, commitment: NoteCommitment) {
        self.tree.insert(commitment.0);
        let root = self.tree.root();
        if self.recent_roots.back().copied() != Some(root) {
            self.recent_roots.push_back(root);
        }
    }
}

impl U256Bytes {
    pub fn from_u128(value: u128) -> Self {
        let mut out = [0u8; 32];
        out[..16].copy_from_slice(&value.to_le_bytes());
        Self(out)
    }

    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }

    pub fn low_u64(&self) -> u64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.0[..8]);
        u64::from_le_bytes(bytes)
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        if self < rhs {
            return None;
        }
        let mut out = [0u8; 32];
        let mut borrow = 0u16;
        for (index, byte) in out.iter_mut().enumerate() {
            let lhs = self.0[index] as u16;
            let rhs = rhs.0[index] as u16 + borrow;
            if lhs >= rhs {
                *byte = (lhs - rhs) as u8;
                borrow = 0;
            } else {
                *byte = ((lhs + 256) - rhs) as u8;
                borrow = 1;
            }
        }
        Some(Self(out))
    }

    pub fn saturating_add(self, rhs: Self) -> Self {
        let mut out = [0u8; 32];
        let mut carry = 0u16;
        for (index, byte) in out.iter_mut().enumerate() {
            let sum = self.0[index] as u16 + rhs.0[index] as u16 + carry;
            *byte = sum as u8;
            carry = sum >> 8;
        }
        Self(out)
    }

    pub fn min(self, rhs: Self) -> Self {
        if self <= rhs {
            self
        } else {
            rhs
        }
    }

    pub fn div_floor_u64(self, rhs: u64) -> u64 {
        if rhs == 0 {
            return 0;
        }
        self.low_u64() / rhs
    }

    pub fn div_ceil_u64(self, rhs: u64) -> u64 {
        if rhs == 0 {
            return 0;
        }
        self.low_u64().div_ceil(rhs)
    }
}

impl PartialOrd for U256Bytes {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for U256Bytes {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        for index in (0..32).rev() {
            match self.0[index].cmp(&other.0[index]) {
                std::cmp::Ordering::Equal => continue,
                ordering => return ordering,
            }
        }
        std::cmp::Ordering::Equal
    }
}

impl LiquidationAuctionStatsWitness {
    fn is_zero(&self) -> bool {
        self.claims_received == 0
            && self.claims_rejected == 0
            && self.auctions_settled == 0
            && self.total_bounty_paid.is_zero()
            && self.total_insurance_funded.is_zero()
    }
}

impl BlockProgramOutput {
    /// Field-element representation of the public output, used when a
    /// downstream circuit (e.g. a bridge verifier on another chain)
    /// needs to consume it via Noir.
    pub fn to_field_elements(&self) -> Vec<Fr> {
        vec![
            Fr::from_bytes_reduce(&self.prev_state_root),
            Fr::from_bytes_reduce(&self.new_state_root),
            Fr::from_bytes_reduce(&self.prev_nullifier_root),
            Fr::from_bytes_reduce(&self.new_nullifier_root),
            Fr::from_u64(self.block_number),
            Fr::from_bytes_reduce(&self.block_hash),
            Fr::from_bytes_reduce(&self.new_market_state_hash),
            Fr::from_bytes_reduce(&self.shielded_event_root),
            Fr::from_u64(self.tx_count),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_header(tx_count: u64, gas_used: u64) -> BlockHeaderWitness {
        let mut base_fee_be = [0u8; 32];
        base_fee_be[24..].copy_from_slice(&42u64.to_be_bytes());
        BlockHeaderWitness {
            chain_id: 7919,
            gas_limit: 30_000_000,
            gas_used,
            base_fee_be,
            coinbase: [0x22; 20],
            tx_count,
        }
    }

    #[test]
    fn output_round_trips_via_bincode() {
        let o = BlockProgramOutput {
            prev_state_root: [1u8; 32],
            new_state_root: [2u8; 32],
            prev_nullifier_root: [3u8; 32],
            new_nullifier_root: [4u8; 32],
            block_number: 100,
            block_hash: [5u8; 32],
            new_market_state_hash: [6u8; 32],
            shielded_event_root: [7u8; 32],
            tx_count: 12,
        };
        let bytes = bincode::serialize(&o).unwrap();
        let back: BlockProgramOutput = bincode::deserialize(&bytes).unwrap();
        assert_eq!(back.block_number, 100);
        assert_eq!(back.tx_count, 12);
    }

    #[test]
    fn to_field_elements_is_canonical_length() {
        let o = BlockProgramOutput {
            prev_state_root: [1u8; 32],
            new_state_root: [2u8; 32],
            prev_nullifier_root: [3u8; 32],
            new_nullifier_root: [4u8; 32],
            block_number: 100,
            block_hash: [5u8; 32],
            new_market_state_hash: [6u8; 32],
            shielded_event_root: [7u8; 32],
            tx_count: 12,
        };
        assert_eq!(o.to_field_elements().len(), 9);
    }

    #[test]
    fn execute_block_program_is_deterministic() {
        let empty_root = MerkleTree::new().root().to_bytes();
        let input = BlockProgramInput {
            prev_state_root: empty_root,
            prev_nullifier_root: [0u8; 32],
            block_number: 7,
            timestamp: 1_700_000_000,
            header: test_header(0, 0),
            txs: Vec::new(),
            prev_market_state: vec![9, 8, 7],
            prev_shielded_state: ShieldedStateWitness {
                leaves: Vec::new(),
                nullifiers: Vec::new(),
                recent_roots: vec![empty_root],
            },
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness::default(),
            expected_block_hash: derive_block_hash(7, &test_header(0, 0)),
            expected_market_state_hash: [8u8; 32],
        };

        let first = execute_block_program(&input).unwrap();
        let second = execute_block_program(&input).unwrap();

        assert_eq!(first.new_state_root, second.new_state_root);
        assert_eq!(first.new_nullifier_root, second.new_nullifier_root);
        assert_eq!(first.block_hash, second.block_hash);
        assert_eq!(first.new_market_state_hash, second.new_market_state_hash);
        assert_eq!(first.shielded_event_root, second.shielded_event_root);
    }

    #[test]
    fn execute_block_program_changes_when_witness_changes() {
        let mut input = BlockProgramInput {
            prev_state_root: MerkleTree::new().root().to_bytes(),
            prev_nullifier_root: [0u8; 32],
            block_number: 7,
            timestamp: 1_700_000_000,
            header: test_header(0, 0),
            txs: Vec::new(),
            prev_market_state: vec![9, 8, 7],
            prev_shielded_state: ShieldedStateWitness {
                leaves: Vec::new(),
                nullifiers: Vec::new(),
                recent_roots: vec![MerkleTree::new().root().to_bytes()],
            },
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness::default(),
            expected_block_hash: derive_block_hash(7, &test_header(0, 0)),
            expected_market_state_hash: [8u8; 32],
        };
        let first = execute_block_program(&input).unwrap();

        let transfer = ShieldedBlockTx::Transfer(ShieldedTransferBlockTx {
            anchor_root: Fr::from_bytes_reduce(&input.prev_state_root),
            nullifier: Fr::from_u64(9),
            output_commitment: Fr::from_u64(10),
            encrypted_output: Vec::new(),
            proof: crate::noir::MockVerifier::new().prove(
                Circuit::Spend,
                vec![
                    Fr::from_bytes_reduce(&input.prev_state_root),
                    Fr::from_u64(9),
                    Fr::from_u64(10),
                    Fr::ZERO,
                ],
            ),
        });
        input.txs.push(bincode::serialize(&transfer).unwrap());
        input.header.tx_count = 1;
        input.expected_block_hash = derive_block_hash(input.block_number, &input.header);
        let second = execute_block_program(&input).unwrap();

        assert_ne!(first.new_state_root, second.new_state_root);
        assert_ne!(first.new_nullifier_root, second.new_nullifier_root);
        assert_eq!(second.block_hash, derive_block_hash(input.block_number, &input.header));
        assert_eq!(second.new_market_state_hash, [8u8; 32]);
        assert_eq!(second.tx_count, 1);
    }

    #[test]
    fn execute_block_program_replays_shield_flow() {
        let proof = crate::noir::MockVerifier::new().prove(
            Circuit::Output,
            vec![Fr::from_u64(55), Fr::ZERO, Fr::from_u64(25)],
        );
        let tx = ShieldedBlockTx::Shield(ShieldBlockTx {
            from: [0x11; 20],
            amount: U256Bytes({
                let mut out = [0u8; 32];
                out[..8].copy_from_slice(&25u64.to_le_bytes());
                out
            }),
            output_commitment: Fr::from_u64(55),
            encrypted_output: Vec::new(),
            proof,
        });
        let input = BlockProgramInput {
            prev_state_root: MerkleTree::new().root().to_bytes(),
            prev_nullifier_root: [0u8; 32],
            block_number: 1,
            timestamp: 2,
            header: test_header(1, 0),
            txs: vec![bincode::serialize(&tx).unwrap()],
            prev_market_state: Vec::new(),
            prev_shielded_state: ShieldedStateWitness {
                leaves: Vec::new(),
                nullifiers: Vec::new(),
                recent_roots: vec![MerkleTree::new().root().to_bytes()],
            },
            transparent_balances: vec![TransparentBalanceEntry {
                address: [0x11; 20],
                balance: U256Bytes({
                    let mut out = [0u8; 32];
                    out[..8].copy_from_slice(&100u64.to_le_bytes());
                    out
                }),
            }],
            pre_tick_witness: ShieldedTickWitness::default(),
            expected_block_hash: derive_block_hash(1, &test_header(1, 0)),
            expected_market_state_hash: [0xbb; 32],
        };

        let output = execute_block_program(&input).unwrap();
        assert_eq!(output.block_hash, derive_block_hash(1, &test_header(1, 0)));
        assert_eq!(output.new_market_state_hash, [0xbb; 32]);
        assert_eq!(output.tx_count, 1);
    }

    #[test]
    fn execute_block_program_replays_fba_market_hash() {
        let anchor_root = MerkleTree::new().root().to_bytes();
        let side_hash = Poseidon::default().hash_two(&Fr::ZERO, &Fr::ZERO);
        let pre_tick_witness = ShieldedTickWitness {
            drained_intent_count: 0,
            decrypted_intents: vec![DecryptedIntentWitness {
                intent_id: [0x11; 32],
                plaintext: Vec::new(),
                order_admission: Some(OrderAdmissionWitness {
                    sequence: 0,
                    anchor_root: Fr::from_bytes_reduce(&anchor_root),
                    nullifier: Fr::ZERO,
                    new_commitment: Fr::ZERO,
                    market_id: 7,
                    side_hash,
                    price_band: 12,
                    size_band: 5,
                    oracle_price: u64_bytes(120),
                    imm_required: u64_bytes(30),
                    tif: 0,
                    proof: crate::noir::MockVerifier::new().prove(
                        Circuit::OrderPlace,
                        vec![
                            Fr::from_bytes_reduce(&anchor_root),
                            Fr::ZERO,
                            Fr::ZERO,
                            Fr::from_u64(7),
                            side_hash,
                            Fr::from_u64(12),
                            Fr::from_u64(5),
                            Fr::from_u64(120),
                            Fr::from_u64(30),
                        ],
                    ),
                    side: 0,
                    price: u64_bytes(120),
                    size: u64_bytes(5),
                    owner_pk: Fr::from_u64(1),
                    salt: Fr::ZERO,
                }),
            }],
            orders: ShieldedOrdersTickWitness {
                initial_margin_bps: 500,
                maintenance_margin_bps: 300,
                next_sequence: 2,
                insurance_fund: U256Bytes::default(),
                price_tick: u64_bytes(10),
                size_lot: u64_bytes(1),
                markets: vec![ShieldedMarketWitness {
                    market_id: 7,
                    tick_size: u64_bytes(10),
                    lot_size: u64_bytes(1),
                    last_price: U256Bytes::default(),
                    oracle_price: u64_bytes(120),
                    status: 0,
                }],
                aggregates: vec![ShieldedMarketAggregateWitness {
                    market_id: 7,
                    mark_price: U256Bytes::default(),
                    funding_rate_bps: 0,
                    long_open_interest: U256Bytes::default(),
                    short_open_interest: U256Bytes::default(),
                    last_clearing_price: U256Bytes::default(),
                    bucketed_depth: Vec::new(),
                    liquidatable_count: 0,
                    last_volume: U256Bytes::default(),
                }],
                books: vec![ShieldedOrderBookWitness {
                    market_id: 7,
                    bids: Vec::new(),
                    asks: vec![PendingIntentWitness {
                        sequence: 1,
                        side: 1,
                        price: u64_bytes(100),
                        size: u64_bytes(3),
                        owner_pk: Fr::from_u64(2),
                        tif: 0,
                        anchor_root: Fr::ZERO,
                        nullifier: Fr::ZERO,
                        new_commitment: Fr::ZERO,
                        market_id: 7,
                        side_hash: Fr::ZERO,
                        price_band: 10,
                        size_band: 3,
                        oracle_price: u64_bytes(100),
                        imm_required: u64_bytes(1),
                        encrypted_payload: Vec::new(),
                        proof: crate::noir::MockVerifier::new().prove(Circuit::OrderPlace, vec![Fr::ZERO; 9]),
                    }],
                }],
            },
            liquidation: LiquidationAuctionTickWitness::default(),
        };
        let mut state = ReplayShieldedState::restore(&ShieldedStateWitness {
            leaves: Vec::new(),
            nullifiers: Vec::new(),
            recent_roots: vec![anchor_root],
        });
        let verifier = default_verifier();
        let poseidon = Poseidon::default();
        let expected_market_state_hash = replay_tick_market_state(
            &mut state,
            &*verifier,
            &poseidon,
            &pre_tick_witness,
        )
        .unwrap()
        .0;
        let input = BlockProgramInput {
            prev_state_root: MerkleTree::new().root().to_bytes(),
            prev_nullifier_root: [0u8; 32],
            block_number: 9,
            timestamp: 0,
            header: test_header(0, 0),
            txs: Vec::new(),
            prev_market_state: Vec::new(),
            prev_shielded_state: ShieldedStateWitness {
                leaves: Vec::new(),
                nullifiers: Vec::new(),
                recent_roots: vec![anchor_root],
            },
            transparent_balances: Vec::new(),
            pre_tick_witness,
            expected_block_hash: derive_block_hash(9, &test_header(0, 0)),
            expected_market_state_hash,
        };

        let output = execute_block_program(&input).unwrap();
        assert_eq!(output.new_market_state_hash, expected_market_state_hash);
    }

    #[test]
    fn execute_block_program_derives_shielded_event_root() {
        let input = BlockProgramInput {
            prev_state_root: MerkleTree::new().root().to_bytes(),
            prev_nullifier_root: [0u8; 32],
            block_number: 5,
            timestamp: 0,
            header: test_header(0, 0),
            txs: Vec::new(),
            prev_market_state: Vec::new(),
            prev_shielded_state: ShieldedStateWitness {
                leaves: Vec::new(),
                nullifiers: Vec::new(),
                recent_roots: vec![MerkleTree::new().root().to_bytes()],
            },
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness {
                drained_intent_count: 2,
                ..Default::default()
            },
            expected_block_hash: derive_block_hash(5, &test_header(0, 0)),
            expected_market_state_hash: [0u8; 32],
        };
        let output = execute_block_program(&input).unwrap();
        assert_ne!(output.shielded_event_root, [0u8; 32]);
    }

    #[test]
    fn execute_block_program_replays_liquidation_events() {
        let anchor_root = MerkleTree::new().root().to_bytes();
        let liquidator_id = Fr::from_u64(7);
        let winning_bid = u64_bytes(25);
        let claim_tag = Fr::from_u64(9);
        let claim = LiquidationClaimWitness {
            anchor_root: Fr::from_bytes_reduce(&anchor_root),
            market_id: 3,
            oracle_price: u64_bytes(120),
            liquidator_id,
            claim_tag,
            encrypted_bid: Vec::new(),
            proof: crate::noir::MockVerifier::new().prove(
                Circuit::LiquidateClaim,
                vec![
                    Fr::from_bytes_reduce(&anchor_root),
                    Fr::from_u64(3),
                    Fr::from_u64(120),
                    liquidator_id,
                    claim_tag,
                ],
            ),
        };
        let input = BlockProgramInput {
            prev_state_root: anchor_root,
            prev_nullifier_root: [0u8; 32],
            block_number: 12,
            timestamp: 0,
            header: test_header(0, 0),
            txs: Vec::new(),
            prev_market_state: Vec::new(),
            prev_shielded_state: ShieldedStateWitness {
                leaves: Vec::new(),
                nullifiers: Vec::new(),
                recent_roots: vec![anchor_root],
            },
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness {
                liquidation: LiquidationAuctionTickWitness {
                    liquidators: vec![LiquidatorWitness {
                        bond_commitment: liquidator_id,
                        bond_amount: 100,
                        registered_at: 1,
                    }],
                    auctions: vec![LiquidationAuctionEntryWitness {
                        claim_tag,
                        claims: vec![claim],
                        bids: vec![LiquidationBidWitness {
                            claim_tag,
                            liquidator_id,
                            bid_price: winning_bid,
                        }],
                    }],
                    pending_revelation: Vec::new(),
                    insurance_fund: U256Bytes::default(),
                    stats: LiquidationAuctionStatsWitness::default(),
                },
                ..Default::default()
            },
            expected_block_hash: derive_block_hash(12, &test_header(0, 0)),
            expected_market_state_hash: hash_market_aggregates(&[]),
        };

        let output = execute_block_program(&input).unwrap();
        let expected_events = vec![CanonicalShieldedEvent::LiquidationSettled {
            market_id: 0,
            winner_bond_commitment: liquidator_id.to_bytes(),
            winning_bid,
        }];
        let expected_root = shielded_event_root(&build_shielded_tick_events(
            12,
            0,
            &[],
            &expected_events,
            anchor_root,
        ));

        assert_eq!(output.new_market_state_hash, hash_market_aggregates(&[]));
        assert_eq!(output.shielded_event_root, expected_root);
    }

    #[test]
    fn execute_block_program_rejects_market_hash_mismatch() {
        let input = BlockProgramInput {
            prev_state_root: MerkleTree::new().root().to_bytes(),
            prev_nullifier_root: [0u8; 32],
            block_number: 9,
            timestamp: 0,
            header: test_header(0, 0),
            txs: Vec::new(),
            prev_market_state: Vec::new(),
            prev_shielded_state: ShieldedStateWitness {
                leaves: Vec::new(),
                nullifiers: Vec::new(),
                recent_roots: vec![MerkleTree::new().root().to_bytes()],
            },
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness {
                orders: ShieldedOrdersTickWitness {
                    initial_margin_bps: 500,
                    maintenance_margin_bps: 300,
                    next_sequence: 0,
                    insurance_fund: U256Bytes::default(),
                    price_tick: u64_bytes(10),
                    size_lot: u64_bytes(1),
                    markets: vec![ShieldedMarketWitness {
                        market_id: 1,
                        tick_size: u64_bytes(10),
                        lot_size: u64_bytes(1),
                        last_price: U256Bytes::default(),
                        oracle_price: U256Bytes::default(),
                        status: 0,
                    }],
                    aggregates: vec![ShieldedMarketAggregateWitness {
                        market_id: 1,
                        mark_price: U256Bytes::default(),
                        funding_rate_bps: 0,
                        long_open_interest: U256Bytes::default(),
                        short_open_interest: U256Bytes::default(),
                        last_clearing_price: U256Bytes::default(),
                        bucketed_depth: Vec::new(),
                        liquidatable_count: 0,
                        last_volume: U256Bytes::default(),
                    }],
                    books: Vec::new(),
                },
                ..Default::default()
            },
            expected_block_hash: derive_block_hash(9, &test_header(0, 0)),
            expected_market_state_hash: [0xdd; 32],
        };

        assert!(matches!(
            execute_block_program(&input),
            Err(BlockProgramError::MarketStateHashMismatch)
        ));
    }

    #[test]
    fn execute_block_program_rejects_block_hash_mismatch() {
        let input = BlockProgramInput {
            prev_state_root: MerkleTree::new().root().to_bytes(),
            prev_nullifier_root: [0u8; 32],
            block_number: 9,
            timestamp: 0,
            header: test_header(0, 0),
            txs: Vec::new(),
            prev_market_state: Vec::new(),
            prev_shielded_state: ShieldedStateWitness::default(),
            transparent_balances: Vec::new(),
            pre_tick_witness: ShieldedTickWitness::default(),
            expected_block_hash: [0xdd; 32],
            expected_market_state_hash: [0u8; 32],
        };

        assert!(matches!(
            execute_block_program(&input),
            Err(BlockProgramError::BlockHashMismatch)
        ));
    }

    fn u64_bytes(value: u64) -> U256Bytes {
        let mut out = [0u8; 32];
        out[..8].copy_from_slice(&value.to_le_bytes());
        U256Bytes(out)
    }
}
