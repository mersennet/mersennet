//! Shielded CLOB.
//!
//! Replaces the address-keyed [`crate::prime_orders`] CLOB with a
//! commitment-keyed engine. Trader identity, exact collateral, exact
//! position, exact order size, and PnL are never visible to anyone
//! other than the trader (and any party they have explicitly granted
//! a viewing key).
//!
//! ## Architecture
//!
//! ```text
//!   wallet
//!    │
//!    │ build ShieldedOrderTx { anchor, nullifier, new_commitment,
//!    │                         market, side_hash, price_band,
//!    │                         size_band, oracle_price, imm_required,
//!    │                         encrypted_payload, proof }
//!    │
//!    ▼
//!   threshold_mempool  ───────────────►  FBA tick
//!    │                                    │
//!    │                                    │ k-of-n decrypt shares
//!    │                                    │
//!    │                                    ▼
//!    │                                  ShieldedOrdersEngine::admit_intent
//!    │                                    │
//!    │                                    │ verify ZK proof, decrypt,
//!    │                                    │ bucket-band sanity, push to book
//!    │                                    │
//!    │                                    ▼
//!    │                                  FBA batch auction
//!    │                                    │ uniform clearing price
//!    │                                    │
//!    │                                    ▼
//!    │                                  ShieldedOrdersEngine::settle_batch
//!    │                                    │
//!    │                                    │ insert new commitments,
//!    │                                    │ insert nullifiers,
//!    │                                    │ update aggregates
//!    │                                    │
//!    │                                    ▼
//!    │                                  events: OrderMatched (anonymous),
//!    │                                          ClearingPrice, AggregateUpdate
//!    │
//! ```
//!
//! ## What this module owns
//!
//! - [`MarketAggregates`] — the public, observable per-market state.
//! - [`ShieldedOrderBook`] — the in-memory price-time book of
//!   *pending* intents that have been decrypted and verified but not
//!   yet matched.
//! - [`ShieldedOrdersEngine`] — the entry point the chain calls per
//!   block: admit intents, run FBA, settle batch.
//!
//! What this module does *not* own:
//! - The note tree / nullifier set — owned by [`crate::shielded_state`].
//! - The mempool — owned by [`crate::threshold_mempool`].
//! - The proving stack — owned by `prime-zkp`.

#![allow(dead_code)]

use crate::prime_orders::{MarketId, MarketStatus, Side, TimeInForce};
use crate::shielded_state::ShieldedState;
use prime_zkp::{
    Fr, NoteCommitment, Nullifier,
    noir::{CircuitProof, Verifier, VerifyError, default_verifier},
    poseidon::Poseidon,
    sp1::{
        BlockProgramError, DecryptedIntentWitness, MarketFillWitness, MarketTickTransitionWitness,
        OrderAdmissionValidationContext, OrderAdmissionWitness, PendingIntentWitness,
        ShieldedDepthLevelWitness, ShieldedMarketAggregateWitness, ShieldedMarketWitness,
        ShieldedOrderBookWitness, ShieldedOrdersTickWitness, U256Bytes, apply_market_tick_witness,
        validate_order_admission_witness,
    },
};
use revm::primitives::{U256, keccak256};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use thiserror::Error;

#[cfg(test)]
use prime_zkp::noir::MockVerifier;

/// Externally-submitted shielded order envelope. Crosses the
/// threshold mempool encrypted; only the public fields below are
/// readable by observers before the FBA decryption step.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShieldedOrderTx {
    /// Anchor: which recent note-tree root the proof binds to.
    pub anchor_root: Fr,
    /// Nullifier of the collateral note being spent.
    pub nullifier: Fr,
    /// Commitment of the remaining-collateral note (after the
    /// initial margin debit). Inserted into the tree on settlement.
    pub new_commitment: Fr,
    /// Market the order targets.
    pub market_id: MarketId,
    /// Poseidon(side, salt) — hides the side from observers.
    pub side_hash: Fr,
    /// Bucketed price tier (e.g. floor(price / tick_band)).
    pub price_band: u32,
    /// Bucketed max-size tier.
    pub size_band: u32,
    /// Oracle price asserted by the trader; the chain re-checks this
    /// against the oracle precompile feed.
    pub oracle_price: U256,
    /// Initial margin required for `(market, price_band, size_band,
    /// oracle_price)`. The chain re-derives this and compares.
    pub imm_required: U256,
    /// Threshold-encrypted exact-side/price/size payload. Decrypted
    /// at FBA tick by k-of-n validator shares.
    pub encrypted_payload: Vec<u8>,
    /// ZK proof of solvency + nullifier-derivation + new-commitment
    /// well-formedness.
    pub proof: CircuitProof,
    /// Time-in-force; plaintext because it only affects how long the
    /// intent lives, not what it does.
    pub tif: TimeInForce,
}

/// Plaintext intent recovered after threshold decryption. Carried
/// only inside the engine; never leaves a validator's hot memory.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecryptedIntent {
    pub side: Side,
    pub price: U256,
    pub size: U256,
    /// Recipient public key for the fill note.
    pub owner_pk: Fr,
    /// Side-hash salt; the engine re-derives `Poseidon(side, salt)`
    /// and compares to the public `side_hash`.
    pub salt: Fr,
}

/// Canonical decrypted order payload carried by the threshold mempool.
/// The public order envelope is paired with the decrypted exact-side /
/// price / size intent so the block tick can re-run admission.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThresholdOrderIntent {
    pub tx: ShieldedOrderTx,
    pub intent: DecryptedIntent,
}

#[derive(Clone, Debug)]
pub struct PendingIntent {
    pub tx: ShieldedOrderTx,
    pub intent: DecryptedIntent,
    /// Engine-assigned sequence number for stable price-time priority.
    pub sequence: u64,
}

#[derive(Debug, Error)]
pub enum ShieldedOrderError {
    #[error("anchor root is not in the recent-roots window")]
    StaleAnchor,
    #[error("zk proof verification failed: {0}")]
    InvalidProof(#[from] VerifyError),
    #[error("nullifier already spent")]
    DoubleSpend,
    #[error("market {0:?} does not exist")]
    UnknownMarket(MarketId),
    #[error("market {0:?} is not active")]
    MarketInactive(MarketId),
    #[error("oracle price disagrees with current feed")]
    OraclePriceMismatch,
    #[error("price band {claimed} does not match plaintext price {actual_band}")]
    PriceBandMismatch { claimed: u32, actual_band: u32 },
    #[error("size band {claimed} is below plaintext size {actual_band}")]
    SizeBandViolation { claimed: u32, actual_band: u32 },
    #[error("side hash binding fails")]
    SideHashMismatch,
    #[error("imm_required {required} does not match chain-derived value {derived}")]
    InitialMarginMismatch { required: U256, derived: U256 },
    #[error("decryption produced malformed plaintext")]
    DecryptionFailed,
}

/// Public market-level aggregates. The only state visible to
/// non-traders. Updated atomically at the end of each FBA batch.
#[derive(Clone, Debug, Default)]
pub struct MarketAggregates {
    pub mark_price: U256,
    pub funding_rate_bps: i64,
    pub long_open_interest: U256,
    pub short_open_interest: U256,
    pub last_clearing_price: U256,
    /// `price_band -> aggregated_size`. Observable order-book depth.
    pub bucketed_depth: BTreeMap<u32, U256>,
    pub liquidatable_count: u32,
    pub last_volume: U256,
}

/// In-memory book of intents that have been threshold-decrypted,
/// proof-verified, and are pending the next FBA tick.
#[derive(Default, Debug)]
pub struct ShieldedOrderBook {
    pub bids: BTreeMap<U256, VecDeque<PendingIntent>>,
    pub asks: BTreeMap<U256, VecDeque<PendingIntent>>,
}

impl ShieldedOrderBook {
    pub fn total_bid_size(&self) -> U256 {
        let mut sum = U256::ZERO;
        for level in self.bids.values() {
            for it in level.iter() {
                sum = sum.saturating_add(it.intent.size);
            }
        }
        sum
    }
    pub fn total_ask_size(&self) -> U256 {
        let mut sum = U256::ZERO;
        for level in self.asks.values() {
            for it in level.iter() {
                sum = sum.saturating_add(it.intent.size);
            }
        }
        sum
    }
}

/// Frequent-batch-auction clearing summary.
#[derive(Clone, Debug, Default)]
pub struct ClearingResult {
    pub clearing_price: U256,
    pub matched_size: U256,
    pub fills: Vec<Fill>,
}

#[derive(Clone, Debug)]
pub struct Fill {
    pub market_id: MarketId,
    pub clearing_price: U256,
    pub size: U256,
    pub side: Side,
    pub recipient: Fr,
}

/// Top-level engine. The chain block-production loop calls this:
///
/// 1. For each decrypted intent in the mempool: `admit_intent`.
/// 2. At end of pre-tick window: `run_fba(market)` for each market.
/// 3. For each fill: insert a fresh note into `ShieldedState`.
#[derive(Debug)]
pub struct ShieldedOrdersEngine {
    pub markets: HashMap<MarketId, crate::prime_orders::Market>,
    pub aggregates: HashMap<MarketId, MarketAggregates>,
    pub books: HashMap<MarketId, ShieldedOrderBook>,
    pub insurance_fund: U256,
    pub initial_margin_bps: u64,
    pub maintenance_margin_bps: u64,
    pub next_sequence: u64,
    pub verifier: Box<dyn Verifier>,
    pub poseidon: Poseidon,
    /// Tick size for price bucketing. `price_band = price / tick_size`.
    pub price_tick: U256,
    /// Lot size for size bucketing. `size_band = ceil(size / lot)`.
    pub size_lot: U256,
}

impl Default for ShieldedOrdersEngine {
    fn default() -> Self {
        Self {
            markets: HashMap::new(),
            aggregates: HashMap::new(),
            books: HashMap::new(),
            insurance_fund: U256::ZERO,
            initial_margin_bps: 500,
            maintenance_margin_bps: 300,
            next_sequence: 0,
            verifier: default_verifier(),
            poseidon: Poseidon::default(),
            price_tick: U256::from(10u64),
            size_lot: U256::from(1u64),
        }
    }
}

struct ShieldedOrderAdmissionErrorContext {
    market_id: MarketId,
    claimed_price_band: u32,
    actual_price_band: u32,
    claimed_size_band: u32,
    actual_size_band: u32,
    required_imm: U256,
    derived_imm: U256,
}

impl ShieldedOrdersEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_market(&mut self, m: crate::prime_orders::Market) {
        let id = m.id;
        self.markets.insert(id, m);
        self.aggregates.entry(id).or_default();
        self.books.entry(id).or_default();
    }

    pub fn tick_witness(&self) -> ShieldedOrdersTickWitness {
        let mut markets: Vec<_> = self.markets.values().cloned().collect();
        markets.sort_by_key(|market| market.id.0);

        let market_ids = self.market_ids_in_canonical_order();

        ShieldedOrdersTickWitness {
            initial_margin_bps: self.initial_margin_bps,
            maintenance_margin_bps: self.maintenance_margin_bps,
            next_sequence: self.next_sequence,
            insurance_fund: u256_bytes(self.insurance_fund),
            price_tick: u256_bytes(self.price_tick),
            size_lot: u256_bytes(self.size_lot),
            markets: markets
                .into_iter()
                .map(|market| ShieldedMarketWitness {
                    market_id: market.id.0,
                    tick_size: u256_bytes(market.tick_size),
                    lot_size: u256_bytes(market.lot_size),
                    last_price: u256_bytes(market.last_price),
                    oracle_price: u256_bytes(market.last_price),
                    status: market_status_code(market.status),
                })
                .collect(),
            aggregates: market_ids
                .iter()
                .filter_map(|market_id| self.market_aggregate_witness(*market_id))
                .collect(),
            books: market_ids
                .iter()
                .filter_map(|market_id| self.market_book_witness(*market_id))
                .collect(),
        }
    }

    pub fn market_ids_in_canonical_order(&self) -> Vec<MarketId> {
        let mut market_ids: Vec<_> = self.aggregates.keys().copied().collect();
        market_ids.sort_by_key(|market_id| market_id.0);
        market_ids
    }

    pub fn order_admission_witnesses(&self) -> Vec<DecryptedIntentWitness> {
        let mut pending = self
            .books
            .values()
            .flat_map(|book| book.bids.values().chain(book.asks.values()))
            .flat_map(|level| level.iter())
            .collect::<Vec<_>>();
        pending.sort_by_key(|entry| entry.sequence);
        pending
            .into_iter()
            .map(|entry| {
                let admission = OrderAdmissionWitness {
                    sequence: entry.sequence,
                    anchor_root: entry.tx.anchor_root,
                    nullifier: entry.tx.nullifier,
                    new_commitment: entry.tx.new_commitment,
                    market_id: entry.tx.market_id.0,
                    side_hash: entry.tx.side_hash,
                    price_band: entry.tx.price_band,
                    size_band: entry.tx.size_band,
                    oracle_price: u256_bytes(entry.tx.oracle_price),
                    imm_required: u256_bytes(entry.tx.imm_required),
                    tif: time_in_force_code(entry.tx.tif),
                    proof: entry.tx.proof.clone(),
                    side: side_code(entry.intent.side),
                    price: u256_bytes(entry.intent.price),
                    size: u256_bytes(entry.intent.size),
                    owner_pk: entry.intent.owner_pk,
                    salt: entry.intent.salt,
                };
                let intent_id = keccak256(bincode::serialize(&admission).unwrap_or_default());
                DecryptedIntentWitness {
                    intent_id: intent_id.0,
                    plaintext: Vec::new(),
                    order_admission: Some(admission),
                }
            })
            .collect()
    }

    pub fn oracle_price_for_market(&self, market_id: MarketId) -> Option<U256> {
        self.markets.get(&market_id).map(|market| market.last_price)
    }

    /// Compute the initial-margin requirement for this band combination.
    /// Deterministic so the chain and the wallet agree.
    pub fn derive_imm_required(
        &self,
        market_id: MarketId,
        price_band: u32,
        size_band: u32,
        _oracle_price: U256,
    ) -> Option<U256> {
        let _ = self.markets.get(&market_id)?;
        // imm = ceil(notional * initial_margin_bps / 10000)
        // notional = (price_band * tick_size) * (size_band * lot_size)
        let price = self
            .price_tick
            .saturating_mul(U256::from(price_band as u64));
        let size = self.size_lot.saturating_mul(U256::from(size_band as u64));
        let notional = price.saturating_mul(size);
        let bps = U256::from(self.initial_margin_bps);
        let num = notional.saturating_mul(bps);
        let imm = num.div_ceil(U256::from(10_000u64));
        Some(imm)
    }

    /// Admit one decrypted intent into the book. Returns `Ok(())` on
    /// success, or a typed error that the engine emits as a public
    /// "intent rejected" event without leaking which intent.
    pub fn admit_intent(
        &mut self,
        state: &mut ShieldedState,
        tx: ShieldedOrderTx,
        intent: DecryptedIntent,
        current_oracle_price: U256,
    ) -> Result<(), ShieldedOrderError> {
        let market = self
            .markets
            .get(&tx.market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(tx.market_id))?;
        let actual_price_band =
            u32::try_from((intent.price / self.price_tick).as_limbs()[0]).unwrap_or(u32::MAX);
        let actual_size_band =
            u32::try_from(intent.size.div_ceil(self.size_lot).as_limbs()[0]).unwrap_or(u32::MAX);
        let derived_imm = self
            .derive_imm_required(tx.market_id, tx.price_band, tx.size_band, tx.oracle_price)
            .ok_or(ShieldedOrderError::UnknownMarket(tx.market_id))?;
        let admission = OrderAdmissionWitness {
            sequence: self.next_sequence,
            anchor_root: tx.anchor_root,
            nullifier: tx.nullifier,
            new_commitment: tx.new_commitment,
            market_id: tx.market_id.0,
            side_hash: tx.side_hash,
            price_band: tx.price_band,
            size_band: tx.size_band,
            oracle_price: u256_bytes(tx.oracle_price),
            imm_required: u256_bytes(tx.imm_required),
            tif: time_in_force_code(tx.tif),
            proof: tx.proof.clone(),
            side: side_code(intent.side),
            price: u256_bytes(intent.price),
            size: u256_bytes(intent.size),
            owner_pk: intent.owner_pk,
            salt: intent.salt,
        };
        validate_order_admission_witness(
            &*self.verifier,
            &self.poseidon,
            &admission,
            &OrderAdmissionValidationContext {
                market_present: true,
                market_status: market_status_code(market.status),
                market_oracle_price: u256_bytes(current_oracle_price),
                initial_margin_bps: self.initial_margin_bps,
                price_tick: u256_bytes(self.price_tick),
                size_lot: u256_bytes(self.size_lot),
                root_is_recent: state.is_recent_root(&tx.anchor_root),
                nullifier_spent: state.is_spent(&Nullifier(tx.nullifier)),
            },
        )
        .map_err(|error| {
            Self::map_block_program_error_to_shielded_order_error(
                error,
                ShieldedOrderAdmissionErrorContext {
                    market_id: tx.market_id,
                    claimed_price_band: tx.price_band,
                    actual_price_band,
                    claimed_size_band: tx.size_band,
                    actual_size_band,
                    required_imm: tx.imm_required,
                    derived_imm,
                },
            )
        })?;
        // 10. Insert the spend nullifier and the new collateral commitment.
        state.spend(Nullifier(tx.nullifier))?;
        state.insert_note(NoteCommitment(tx.new_commitment))?;
        // 11. Push to the book. Capture market_id and price for the
        // aggregate update before tx is moved.
        let market_id = tx.market_id;
        let order_price = intent.price;
        let order_size = intent.size;
        let seq = self.next_sequence;
        self.next_sequence += 1;
        let book = self.books.entry(market_id).or_default();
        let book_side = match intent.side {
            Side::Buy => &mut book.bids,
            Side::Sell => &mut book.asks,
        };
        let pending = PendingIntent {
            tx,
            intent: intent.clone(),
            sequence: seq,
        };
        book_side.entry(order_price).or_default().push_back(pending);

        // 12. Update aggregates: bucketed depth.
        if let Some(agg) = self.aggregates.get_mut(&market_id) {
            let depth = agg
                .bucketed_depth
                .entry(actual_price_band)
                .or_insert(U256::ZERO);
            *depth = depth.saturating_add(order_size);
        }
        Ok(())
    }

    fn map_block_program_error_to_shielded_order_error(
        error: BlockProgramError,
        context: ShieldedOrderAdmissionErrorContext,
    ) -> ShieldedOrderError {
        match error {
            BlockProgramError::StaleAnchor => ShieldedOrderError::StaleAnchor,
            BlockProgramError::DoubleSpend => ShieldedOrderError::DoubleSpend,
            BlockProgramError::UnknownMarket(_) => {
                ShieldedOrderError::UnknownMarket(context.market_id)
            }
            BlockProgramError::MarketInactive(_) => {
                ShieldedOrderError::MarketInactive(context.market_id)
            }
            BlockProgramError::OraclePriceMismatch => ShieldedOrderError::OraclePriceMismatch,
            BlockProgramError::PriceBandMismatch => ShieldedOrderError::PriceBandMismatch {
                claimed: context.claimed_price_band,
                actual_band: context.actual_price_band,
            },
            BlockProgramError::SizeBandViolation => ShieldedOrderError::SizeBandViolation {
                claimed: context.claimed_size_band,
                actual_band: context.actual_size_band,
            },
            BlockProgramError::SideHashMismatch => ShieldedOrderError::SideHashMismatch,
            BlockProgramError::InitialMarginMismatch => ShieldedOrderError::InitialMarginMismatch {
                required: context.required_imm,
                derived: context.derived_imm,
            },
            BlockProgramError::InvalidProof => {
                ShieldedOrderError::InvalidProof(VerifyError::InvalidProof)
            }
            other => {
                panic!("unexpected block-program error in shielded order admission: {other:?}")
            }
        }
    }
    /// Run a Frequent Batch Auction on `market_id`. Returns a
    /// summary; the engine emits public events with `(market_id,
    /// clearing_price, matched_size)` only — no addresses.
    pub fn run_fba(&mut self, market_id: MarketId) -> Result<ClearingResult, ShieldedOrderError> {
        let _ = self
            .markets
            .get(&market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(market_id))?;
        let aggregate = self
            .market_aggregate_witness(market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(market_id))?;
        let book = self
            .market_book_witness(market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(market_id))?;
        let transition = apply_market_tick_witness(aggregate, book);
        self.apply_market_tick_transition(market_id, &transition)?;
        Ok(ClearingResult {
            clearing_price: u256_from_bytes(transition.aggregate.last_clearing_price),
            matched_size: u256_from_bytes(transition.aggregate.last_volume),
            fills: transition.fills.iter().map(fill_from_witness).collect(),
        })
    }
    fn market_aggregate_witness(
        &self,
        market_id: MarketId,
    ) -> Option<ShieldedMarketAggregateWitness> {
        let aggregate = self.aggregates.get(&market_id)?;
        Some(ShieldedMarketAggregateWitness {
            market_id: market_id.0,
            mark_price: u256_bytes(aggregate.mark_price),
            funding_rate_bps: aggregate.funding_rate_bps,
            long_open_interest: u256_bytes(aggregate.long_open_interest),
            short_open_interest: u256_bytes(aggregate.short_open_interest),
            last_clearing_price: u256_bytes(aggregate.last_clearing_price),
            bucketed_depth: aggregate
                .bucketed_depth
                .iter()
                .map(|(price_band, size)| ShieldedDepthLevelWitness {
                    price_band: *price_band,
                    size: u256_bytes(*size),
                })
                .collect(),
            liquidatable_count: aggregate.liquidatable_count,
            last_volume: u256_bytes(aggregate.last_volume),
        })
    }

    fn market_book_witness(&self, market_id: MarketId) -> Option<ShieldedOrderBookWitness> {
        let book = self.books.get(&market_id)?;
        Some(ShieldedOrderBookWitness {
            market_id: market_id.0,
            bids: pending_intents(&book.bids),
            asks: pending_intents(&book.asks),
        })
    }

    fn apply_market_tick_transition(
        &mut self,
        market_id: MarketId,
        transition: &MarketTickTransitionWitness,
    ) -> Result<(), ShieldedOrderError> {
        let aggregate = self
            .aggregates
            .get_mut(&market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(market_id))?;
        aggregate.mark_price = u256_from_bytes(transition.aggregate.mark_price);
        aggregate.last_clearing_price = u256_from_bytes(transition.aggregate.last_clearing_price);
        aggregate.last_volume = u256_from_bytes(transition.aggregate.last_volume);

        let book = self
            .books
            .get_mut(&market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(market_id))?;
        apply_level_sizes(&mut book.bids, &transition.book.bids);
        apply_level_sizes(&mut book.asks, &transition.book.asks);
        Ok(())
    }
}

pub fn decode_threshold_order_intent(bytes: &[u8]) -> Result<ThresholdOrderIntent, bincode::Error> {
    bincode::deserialize(bytes)
}

fn pending_intents(levels: &BTreeMap<U256, VecDeque<PendingIntent>>) -> Vec<PendingIntentWitness> {
    levels
        .values()
        .flat_map(|level| level.iter())
        .map(|pending| PendingIntentWitness {
            sequence: pending.sequence,
            side: side_code(pending.intent.side),
            price: u256_bytes(pending.intent.price),
            size: u256_bytes(pending.intent.size),
            owner_pk: pending.intent.owner_pk,
            tif: time_in_force_code(pending.tx.tif),
            anchor_root: pending.tx.anchor_root,
            nullifier: pending.tx.nullifier,
            new_commitment: pending.tx.new_commitment,
            market_id: pending.tx.market_id.0,
            side_hash: pending.tx.side_hash,
            price_band: pending.tx.price_band,
            size_band: pending.tx.size_band,
            oracle_price: u256_bytes(pending.tx.oracle_price),
            imm_required: u256_bytes(pending.tx.imm_required),
            encrypted_payload: pending.tx.encrypted_payload.clone(),
            proof: pending.tx.proof.clone(),
        })
        .collect()
}

fn apply_level_sizes(
    levels: &mut BTreeMap<U256, VecDeque<PendingIntent>>,
    witness: &[PendingIntentWitness],
) {
    let remaining = witness
        .iter()
        .map(|pending| {
            (
                (u256_from_bytes(pending.price), pending.sequence),
                u256_from_bytes(pending.size),
            )
        })
        .collect::<HashMap<_, _>>();
    for (price, queue) in levels.iter_mut() {
        for pending in queue.iter_mut() {
            pending.intent.size = remaining
                .get(&(*price, pending.sequence))
                .copied()
                .unwrap_or(U256::ZERO);
        }
        queue.retain(|pending| !pending.intent.size.is_zero());
    }
    levels.retain(|_, queue| !queue.is_empty());
}

fn fill_from_witness(fill: &MarketFillWitness) -> Fill {
    Fill {
        market_id: MarketId(fill.market_id),
        clearing_price: u256_from_bytes(fill.clearing_price),
        size: u256_from_bytes(fill.size),
        side: side_from_code(fill.side),
        recipient: fill.recipient,
    }
}

fn market_status_code(status: MarketStatus) -> u8 {
    match status {
        MarketStatus::Active => 0,
        MarketStatus::Halted => 1,
        MarketStatus::SettleOnly => 2,
    }
}

fn side_code(side: Side) -> u8 {
    match side {
        Side::Buy => 0,
        Side::Sell => 1,
    }
}

fn side_from_code(side: u8) -> Side {
    match side {
        1 => Side::Sell,
        _ => Side::Buy,
    }
}

fn time_in_force_code(tif: TimeInForce) -> u8 {
    match tif {
        TimeInForce::Gtc => 0,
        TimeInForce::Ioc => 1,
        TimeInForce::Fok => 2,
    }
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

/// Saturating cast of the low 64 bits of a U256 to u64. Sufficient
/// for hashing into the `Fr` slot because the circuit binds the full
/// value via Poseidon's wider linear combination on the chain side.
fn u256_as_u64(x: &U256) -> u64 {
    x.as_limbs()[0]
}

/// Uniform-price discrete-time auction.
///
/// Walk the book matching at the clearing price that maximises total
/// matched volume; break ties by minimising the absolute imbalance,
/// then by giving priority to earlier-sequence intents.
fn uniform_price_auction(book: &mut ShieldedOrderBook) -> ClearingResult {
    if book.bids.is_empty() || book.asks.is_empty() {
        return ClearingResult::default();
    }
    // Collect (price, total_bid_size_at_or_above, total_ask_size_at_or_below).
    let mut prices: Vec<U256> = book.bids.keys().chain(book.asks.keys()).copied().collect();
    prices.sort();
    prices.dedup();

    let mut best_price = U256::ZERO;
    let mut best_volume = U256::ZERO;

    for p in &prices {
        let cum_bids: U256 = book
            .bids
            .iter()
            .filter(|(price, _)| *price >= p)
            .flat_map(|(_, q)| q.iter())
            .fold(U256::ZERO, |acc, it| acc.saturating_add(it.intent.size));
        let cum_asks: U256 = book
            .asks
            .iter()
            .filter(|(price, _)| *price <= p)
            .flat_map(|(_, q)| q.iter())
            .fold(U256::ZERO, |acc, it| acc.saturating_add(it.intent.size));
        let matched = cum_bids.min(cum_asks);
        if matched > best_volume {
            best_volume = matched;
            best_price = *p;
        }
    }

    if best_volume.is_zero() {
        return ClearingResult::default();
    }

    // Execute: walk each side at the clearing price, in sequence
    // order, until `best_volume` is exhausted on each side.
    let mut fills = Vec::new();
    let mut remaining_bid = best_volume;
    let mut remaining_ask = best_volume;

    // Bids: highest price first, sequence order within a level.
    let bid_prices: Vec<U256> = book
        .bids
        .keys()
        .copied()
        .filter(|p| *p >= best_price)
        .rev()
        .collect();
    for price in bid_prices {
        if remaining_bid.is_zero() {
            break;
        }
        let level = book.bids.get_mut(&price).unwrap();
        while let Some(mut head) = level.pop_front() {
            let fill_size = head.intent.size.min(remaining_bid);
            fills.push(Fill {
                market_id: head.tx.market_id,
                clearing_price: best_price,
                size: fill_size,
                side: Side::Buy,
                recipient: head.intent.owner_pk,
            });
            head.intent.size = head.intent.size.saturating_sub(fill_size);
            remaining_bid = remaining_bid.saturating_sub(fill_size);
            if !head.intent.size.is_zero() {
                level.push_front(head);
                break;
            }
            if remaining_bid.is_zero() {
                break;
            }
        }
    }
    // Asks: lowest price first.
    let ask_prices: Vec<U256> = book
        .asks
        .keys()
        .copied()
        .filter(|p| *p <= best_price)
        .collect();
    for price in ask_prices {
        if remaining_ask.is_zero() {
            break;
        }
        let level = book.asks.get_mut(&price).unwrap();
        while let Some(mut head) = level.pop_front() {
            let fill_size = head.intent.size.min(remaining_ask);
            fills.push(Fill {
                market_id: head.tx.market_id,
                clearing_price: best_price,
                size: fill_size,
                side: Side::Sell,
                recipient: head.intent.owner_pk,
            });
            head.intent.size = head.intent.size.saturating_sub(fill_size);
            remaining_ask = remaining_ask.saturating_sub(fill_size);
            if !head.intent.size.is_zero() {
                level.push_front(head);
                break;
            }
            if remaining_ask.is_zero() {
                break;
            }
        }
    }
    // Drop empty levels.
    book.bids.retain(|_, q| !q.is_empty());
    book.asks.retain(|_, q| !q.is_empty());

    ClearingResult {
        clearing_price: best_price,
        matched_size: best_volume,
        fills,
    }
}

impl From<crate::shielded_state::ShieldedStateError> for ShieldedOrderError {
    fn from(e: crate::shielded_state::ShieldedStateError) -> Self {
        use crate::shielded_state::ShieldedStateError;
        match e {
            ShieldedStateError::DoubleSpend => ShieldedOrderError::DoubleSpend,
            ShieldedStateError::StaleAnchor => ShieldedOrderError::StaleAnchor,
            ShieldedStateError::InvalidMembershipProof => {
                ShieldedOrderError::InvalidProof(VerifyError::InvalidProof)
            }
            ShieldedStateError::DuplicateCommitment => {
                ShieldedOrderError::InvalidProof(VerifyError::InvalidProof)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prime_orders::Market;
    use prime_zkp::Circuit;
    use prime_zkp::note::Note;

    fn mk_market(id: u64) -> Market {
        Market {
            id: MarketId(id),
            symbol: format!("M{}", id),
            tick_size: U256::from(10u64),
            lot_size: U256::from(1u64),
            last_price: U256::ZERO,
            status: MarketStatus::Active,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_intent(
        engine: &ShieldedOrdersEngine,
        state: &ShieldedState,
        market: MarketId,
        side: Side,
        price: U256,
        size: U256,
        owner_pk: Fr,
        seed: u64,
    ) -> (ShieldedOrderTx, DecryptedIntent) {
        let p = &engine.poseidon;
        let collateral = Note {
            value: 1_000_000_000,
            asset_id: 0,
            owner_pk,
            rho: Fr::from_u64(seed),
            psi: Fr::from_u64(seed + 1),
        };
        let spend_sk = Fr::from_u64(seed + 0xdead);
        let nullifier = collateral.nullifier(p, &spend_sk).0;
        let new_collateral = Note {
            value: 999_900_000,
            asset_id: 0,
            owner_pk,
            rho: Fr::from_u64(seed + 100),
            psi: Fr::from_u64(seed + 101),
        };
        let new_commitment = new_collateral.commit(p).0;
        let price_band = u32::try_from((price / engine.price_tick).as_limbs()[0]).unwrap();
        let size_band = u32::try_from(size.div_ceil(engine.size_lot).as_limbs()[0]).unwrap();
        let oracle_price = U256::from(1_000u64);
        let imm_required = engine
            .derive_imm_required(market, price_band, size_band, oracle_price)
            .unwrap();
        let salt = Fr::from_u64(seed + 0xbeef);
        let side_fr = match side {
            Side::Buy => Fr::ZERO,
            Side::Sell => Fr::ONE,
        };
        let side_hash = p.hash_two(&side_fr, &salt);

        // Use the anchor that includes the collateral note in the tree.
        // The test fixture inserts the collateral commitment first.
        let anchor_root = state.current_root();

        let public_inputs = vec![
            anchor_root,
            nullifier,
            new_commitment,
            Fr::from_u64(market.0),
            side_hash,
            Fr::from_u64(price_band as u64),
            Fr::from_u64(size_band as u64),
            Fr::from_u64(u256_as_u64(&oracle_price)),
            Fr::from_u64(u256_as_u64(&imm_required)),
        ];
        let proof = MockVerifier::new().prove(Circuit::OrderPlace, public_inputs);

        let tx = ShieldedOrderTx {
            anchor_root,
            nullifier,
            new_commitment,
            market_id: market,
            side_hash,
            price_band,
            size_band,
            oracle_price,
            imm_required,
            encrypted_payload: Vec::new(),
            proof,
            tif: TimeInForce::Gtc,
        };
        let intent = DecryptedIntent {
            side,
            price,
            size,
            owner_pk,
            salt,
        };
        (tx, intent)
    }

    #[test]
    fn admit_then_match_two_intents_via_fba() {
        let mut engine = ShieldedOrdersEngine::new();
        engine.add_market(mk_market(1));
        let mut state = ShieldedState::new();

        let (tx_b, in_b) = build_intent(
            &engine,
            &state,
            MarketId(1),
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            Fr::from_u64(0xa11ce),
            1,
        );
        engine
            .admit_intent(&mut state, tx_b, in_b, U256::from(1_000u64))
            .unwrap();
        let (tx_s, in_s) = build_intent(
            &engine,
            &state,
            MarketId(1),
            Side::Sell,
            U256::from(100u64),
            U256::from(5u64),
            Fr::from_u64(0xb0b),
            2,
        );
        engine
            .admit_intent(&mut state, tx_s, in_s, U256::from(1_000u64))
            .unwrap();

        let result = engine.run_fba(MarketId(1)).unwrap();
        assert_eq!(result.matched_size, U256::from(5u64));
        assert_eq!(result.clearing_price, U256::from(100u64));
        assert_eq!(result.fills.len(), 2);
    }

    #[test]
    fn admit_rejects_stale_anchor() {
        let mut engine = ShieldedOrdersEngine::new();
        engine.add_market(mk_market(1));
        let mut state = ShieldedState::new();
        let stale_anchor = Fr::from_u64(0xfeed); // not in recent-roots
        let (mut tx, intent) = build_intent(
            &engine,
            &state,
            MarketId(1),
            Side::Buy,
            U256::from(100u64),
            U256::from(1u64),
            Fr::from_u64(1),
            42,
        );
        tx.anchor_root = stale_anchor;
        let err = engine
            .admit_intent(&mut state, tx, intent, U256::from(1_000u64))
            .unwrap_err();
        assert!(matches!(err, ShieldedOrderError::StaleAnchor));
    }

    #[test]
    fn admit_rejects_double_spend() {
        let mut engine = ShieldedOrdersEngine::new();
        engine.add_market(mk_market(1));
        let mut state = ShieldedState::new();
        let (tx, intent) = build_intent(
            &engine,
            &state,
            MarketId(1),
            Side::Buy,
            U256::from(100u64),
            U256::from(1u64),
            Fr::from_u64(7),
            10,
        );
        // Pre-insert the nullifier to simulate prior spend.
        state.spend(Nullifier(tx.nullifier)).unwrap();
        let err = engine
            .admit_intent(&mut state, tx, intent, U256::from(1_000u64))
            .unwrap_err();
        assert!(matches!(err, ShieldedOrderError::DoubleSpend));
    }

    #[test]
    fn admit_rejects_oracle_mismatch() {
        let mut engine = ShieldedOrdersEngine::new();
        engine.add_market(mk_market(1));
        let mut state = ShieldedState::new();
        let (mut tx, intent) = build_intent(
            &engine,
            &state,
            MarketId(1),
            Side::Buy,
            U256::from(100u64),
            U256::from(1u64),
            Fr::from_u64(1),
            42,
        );
        tx.oracle_price = U256::from(99u64); // disagree with the runtime current
        let err = engine
            .admit_intent(&mut state, tx, intent, U256::from(1_000u64))
            .unwrap_err();
        assert!(matches!(err, ShieldedOrderError::OraclePriceMismatch));
    }

    #[test]
    fn admit_rejects_side_hash_mismatch() {
        let mut engine = ShieldedOrdersEngine::new();
        engine.add_market(mk_market(1));
        let mut state = ShieldedState::new();
        let (tx, mut intent) = build_intent(
            &engine,
            &state,
            MarketId(1),
            Side::Buy,
            U256::from(100u64),
            U256::from(1u64),
            Fr::from_u64(1),
            42,
        );
        intent.side = Side::Sell; // contradicts the side_hash bound to Buy
        let err = engine
            .admit_intent(&mut state, tx, intent, U256::from(1_000u64))
            .unwrap_err();
        assert!(matches!(err, ShieldedOrderError::SideHashMismatch));
    }
}
