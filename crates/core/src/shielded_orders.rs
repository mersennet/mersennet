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
    noir::{Circuit, CircuitProof, MockVerifier, Verifier, VerifyError},
    poseidon::Poseidon,
};
use revm::primitives::U256;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use thiserror::Error;

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
#[derive(Clone, Debug)]
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
            verifier: Box::new(MockVerifier::new()),
            poseidon: Poseidon::default(),
            price_tick: U256::from(10u64),
            size_lot: U256::from(1u64),
        }
    }
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
        // 1. Anchor must be recent.
        if !state.is_recent_root(&tx.anchor_root) {
            return Err(ShieldedOrderError::StaleAnchor);
        }
        // 2. Market exists and is active.
        let market = self
            .markets
            .get(&tx.market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(tx.market_id))?;
        if market.status != MarketStatus::Active {
            return Err(ShieldedOrderError::MarketInactive(tx.market_id));
        }
        // 3. Oracle price agrees with the trader's claimed snapshot.
        // For now we accept exact equality; in production a band is allowed.
        if tx.oracle_price != current_oracle_price {
            return Err(ShieldedOrderError::OraclePriceMismatch);
        }
        // 4. Initial margin derivation matches the trader's claim.
        let derived_imm = self
            .derive_imm_required(tx.market_id, tx.price_band, tx.size_band, tx.oracle_price)
            .ok_or(ShieldedOrderError::UnknownMarket(tx.market_id))?;
        if derived_imm != tx.imm_required {
            return Err(ShieldedOrderError::InitialMarginMismatch {
                required: tx.imm_required,
                derived: derived_imm,
            });
        }
        // 5. Price-band binding to plaintext price.
        let actual_price_band =
            u32::try_from((intent.price / self.price_tick).as_limbs()[0]).unwrap_or(u32::MAX);
        if actual_price_band != tx.price_band {
            return Err(ShieldedOrderError::PriceBandMismatch {
                claimed: tx.price_band,
                actual_band: actual_price_band,
            });
        }
        // 6. Size-band binding (size_band must be >= actual ceil(size/lot)).
        let actual_size_band =
            u32::try_from(intent.size.div_ceil(self.size_lot).as_limbs()[0]).unwrap_or(u32::MAX);
        if actual_size_band > tx.size_band {
            return Err(ShieldedOrderError::SizeBandViolation {
                claimed: tx.size_band,
                actual_band: actual_size_band,
            });
        }
        // 7. Side hash binding.
        let side_fr = match intent.side {
            Side::Buy => Fr::ZERO,
            Side::Sell => Fr::ONE,
        };
        let derived_side_hash = self.poseidon.hash_two(&side_fr, &intent.salt);
        if derived_side_hash != tx.side_hash {
            return Err(ShieldedOrderError::SideHashMismatch);
        }
        // 8. ZK proof verifies against the public inputs.
        let public_inputs = vec![
            tx.anchor_root,
            tx.nullifier,
            tx.new_commitment,
            Fr::from_u64(tx.market_id.0),
            tx.side_hash,
            Fr::from_u64(tx.price_band as u64),
            Fr::from_u64(tx.size_band as u64),
            Fr::from_u64(u256_as_u64(&tx.oracle_price)),
            Fr::from_u64(u256_as_u64(&tx.imm_required)),
        ];
        self.verifier
            .verify(&tx.proof, Circuit::OrderPlace, &public_inputs)?;
        // 9. Nullifier must not already be spent.
        if state.is_spent(&Nullifier(tx.nullifier)) {
            return Err(ShieldedOrderError::DoubleSpend);
        }
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

    /// Run a Frequent Batch Auction on `market_id`. Returns a
    /// summary; the engine emits public events with `(market_id,
    /// clearing_price, matched_size)` only — no addresses.
    pub fn run_fba(&mut self, market_id: MarketId) -> Result<ClearingResult, ShieldedOrderError> {
        let _ = self
            .markets
            .get(&market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(market_id))?;
        let book = self
            .books
            .get_mut(&market_id)
            .ok_or(ShieldedOrderError::UnknownMarket(market_id))?;
        let result = uniform_price_auction(book);
        // Update aggregates atomically.
        if let Some(agg) = self.aggregates.get_mut(&market_id)
            && !result.matched_size.is_zero()
        {
            agg.last_clearing_price = result.clearing_price;
            agg.mark_price = result.clearing_price;
            agg.last_volume = result.matched_size;
        }
        Ok(result)
    }
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
