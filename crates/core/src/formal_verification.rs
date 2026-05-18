//! Formal verification module for Prime Chain matching engine correctness.
//!
//! Provides invariant checking and property-based test generators for security audits.

use revm::primitives::{Address, U256};
use std::collections::HashMap;

/// Formally verifiable invariants for the matching engine.
#[allow(dead_code)]
pub struct InvariantChecker {
    violations: Vec<InvariantViolation>,
    checks_run: u64,
    checks_passed: u64,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct InvariantViolation {
    pub invariant: Invariant,
    pub description: String,
    pub block_height: u64,
    pub severity: Severity,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Invariant {
    ConservationOfValue,          // Sum of all balances never changes (except minting)
    PriceTimePriority,            // Orders execute in price-time priority
    OrderBookConsistency,         // Best bid < best ask (no crossed book)
    NonNegativeBalances,          // No account has negative balance
    NonceMono,                    // Nonces are strictly monotonic per account
    StateRootDeterminism,         // Same inputs produce same state root
    GasAccountingCorrectness,     // Gas used <= gas limit, fees calculated correctly
    DoubleSpendPrevention,        // No transaction double-spent
    ValidatorSetConsistency,      // Sum of stakes matches validator set
    FBAUniformPrice,              // All fills in a batch at the same clearing price
    CommitRevealBinding,          // Revealed tx matches committed hash
    BridgeValueConservation,      // Bridge doesn't create or destroy value
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Critical,  // Must halt chain
    High,      // Must fix immediately
    Medium,    // Should fix soon
    Low,       // Informational
}

#[allow(dead_code)]
pub struct BlockReport {
    pub height: u64,
    pub balances_before: HashMap<Address, U256>,
    pub balances_after: HashMap<Address, U256>,
    pub minted: U256,
    pub burned: U256,
    pub fills: Vec<(u64, u64, u64)>,
    pub best_bid: u64,
    pub best_ask: u64,
    pub nonces: HashMap<Address, Vec<u64>>,
    pub fill_prices: Vec<u64>,
}

#[allow(dead_code)]
pub struct VerificationStats {
    pub total_checks: u64,
    pub passed: u64,
    pub failed: u64,
    pub violations: usize,
    pub critical_violations: usize,
}

impl InvariantChecker {
    /// Create a new invariant checker.
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
            checks_run: 0,
            checks_passed: 0,
        }
    }

    /// Check conservation of value: sum(balances_after) = sum(balances_before) + minted - burned.
    pub fn check_conservation_of_value(
        balances_before: &HashMap<Address, U256>,
        balances_after: &HashMap<Address, U256>,
        minted: U256,
        burned: U256,
    ) -> bool {
        let sum_before: U256 = balances_before.values().copied().sum();
        let sum_after: U256 = balances_after.values().copied().sum();
        sum_after == sum_before.saturating_add(minted).saturating_sub(burned)
    }

    /// Check price-time priority: fills are ordered by (price desc for buys, time asc, order_id asc).
    /// Tuples are (price, time, order_id). For buys: higher price first, then earlier time, then lower order_id.
    pub fn check_price_time_priority(fills: &[(u64, u64, u64)]) -> bool {
        for i in 1..fills.len() {
            let (p_prev, t_prev, id_prev) = fills[i - 1];
            let (p_curr, t_curr, id_curr) = fills[i];
            if p_curr > p_prev {
                return false;
            }
            if p_curr == p_prev && t_curr < t_prev {
                return false;
            }
            if p_curr == p_prev && t_curr == t_prev && id_curr < id_prev {
                return false;
            }
        }
        true
    }

    /// Check order book consistency: best bid < best ask (no crossed book).
    pub fn check_order_book_consistency(best_bid: u64, best_ask: u64) -> bool {
        best_bid < best_ask
    }

    /// Check nonce monotonicity: nonces per account are strictly increasing.
    pub fn check_nonce_monotonicity(nonces: &HashMap<Address, Vec<u64>>) -> bool {
        for seq in nonces.values() {
            for i in 1..seq.len() {
                if seq[i] <= seq[i - 1] {
                    return false;
                }
            }
        }
        true
    }

    /// Check FBA uniform price: all fills in a batch at the same clearing price.
    pub fn check_fba_uniform_price(fill_prices: &[u64]) -> bool {
        if fill_prices.len() <= 1 {
            return true;
        }
        let first = fill_prices[0];
        fill_prices.iter().all(|&p| p == first)
    }

    /// Run all applicable checks on a block report and collect violations.
    pub fn check_all(&mut self, block_height: u64, report: &BlockReport) -> Vec<InvariantViolation> {
        self.violations.clear();

        // Conservation of value
        self.checks_run += 1;
        if Self::check_conservation_of_value(
            &report.balances_before,
            &report.balances_after,
            report.minted,
            report.burned,
        ) {
            self.checks_passed += 1;
        } else {
            self.violations.push(InvariantViolation {
                invariant: Invariant::ConservationOfValue,
                description: "Sum of balances changed incorrectly (minted/burned mismatch)".to_string(),
                block_height,
                severity: Severity::Critical,
            });
        }

        // Price-time priority
        self.checks_run += 1;
        if Self::check_price_time_priority(&report.fills) {
            self.checks_passed += 1;
        } else {
            self.violations.push(InvariantViolation {
                invariant: Invariant::PriceTimePriority,
                description: "Fills violated price-time priority ordering".to_string(),
                block_height,
                severity: Severity::High,
            });
        }

        // Order book consistency
        self.checks_run += 1;
        if Self::check_order_book_consistency(report.best_bid, report.best_ask) {
            self.checks_passed += 1;
        } else {
            self.violations.push(InvariantViolation {
                invariant: Invariant::OrderBookConsistency,
                description: "Crossed book: best_bid >= best_ask".to_string(),
                block_height,
                severity: Severity::High,
            });
        }

        // Nonce monotonicity
        self.checks_run += 1;
        if Self::check_nonce_monotonicity(&report.nonces) {
            self.checks_passed += 1;
        } else {
            self.violations.push(InvariantViolation {
                invariant: Invariant::NonceMono,
                description: "Nonces not strictly monotonic per account".to_string(),
                block_height,
                severity: Severity::Critical,
            });
        }

        // FBA uniform price
        self.checks_run += 1;
        if Self::check_fba_uniform_price(&report.fill_prices) {
            self.checks_passed += 1;
        } else {
            self.violations.push(InvariantViolation {
                invariant: Invariant::FBAUniformPrice,
                description: "FBA fills at different prices (non-uniform)".to_string(),
                block_height,
                severity: Severity::High,
            });
        }

        self.violations.clone()
    }

    /// Return all recorded violations.
    pub fn violations(&self) -> &[InvariantViolation] {
        &self.violations
    }

    /// Return verification statistics.
    pub fn stats(&self) -> VerificationStats {
        let failed = self.checks_run.saturating_sub(self.checks_passed);
        let critical_violations = self
            .violations
            .iter()
            .filter(|v| v.severity == Severity::Critical)
            .count();
        VerificationStats {
            total_checks: self.checks_run,
            passed: self.checks_passed,
            failed,
            violations: self.violations.len(),
            critical_violations,
        }
    }
}

impl std::fmt::Debug for InvariantChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InvariantChecker")
            .field("checks_run", &self.checks_run)
            .field("violations", &self.violations.len())
            .finish()
    }
}

impl Default for InvariantChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Property-Based Test Generators
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct PropertyTestGenerator {
    rng_seed: u64,
}

#[allow(dead_code)]
pub struct TestOrder {
    pub market_id: u64,
    pub side: String,
    pub price: u64,
    pub amount: u64,
}

#[allow(dead_code)]
pub struct TestTransfer {
    pub from: Address,
    pub to: Address,
    pub amount: U256,
}

impl PropertyTestGenerator {
    /// Create a new property test generator with the given seed.
    pub fn new(seed: u64) -> Self {
        Self { rng_seed: seed }
    }

    /// Simple LCG for deterministic pseudo-random generation.
    fn next_u64(&mut self) -> u64 {
        self.rng_seed = self.rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.rng_seed
    }

    /// Generate random orders for property-based testing.
    pub fn generate_random_orders(&mut self, count: usize) -> Vec<TestOrder> {
        let mut orders = Vec::with_capacity(count);
        for _ in 0..count {
            let side = if self.next_u64() % 2 == 0 { "Buy" } else { "Sell" };
            orders.push(TestOrder {
                market_id: self.next_u64() % 16,
                side: side.to_string(),
                price: 1 + (self.next_u64() % 1_000_000),
                amount: 1 + (self.next_u64() % 10_000),
            });
        }
        orders
    }

    /// Generate random transfers for property-based testing.
    pub fn generate_random_transfers(&mut self, count: usize) -> Vec<TestTransfer> {
        let mut transfers = Vec::with_capacity(count);
        let zero = Address::ZERO;
        for _ in 0..count {
            let from = Address::from_slice(&self.next_u64().to_be_bytes().repeat(4)[..20]);
            let to = Address::from_slice(&self.next_u64().to_be_bytes().repeat(4)[..20]);
            let amount = U256::from(self.next_u64() % 1_000_000);
            if from != to && from != zero {
                transfers.push(TestTransfer { from, to, amount });
            }
        }
        transfers
    }

    /// Generate adversarial orders (edge cases: max price, zero size, etc.).
    pub fn generate_adversarial_orders(&mut self, count: usize) -> Vec<TestOrder> {
        let mut orders = Vec::with_capacity(count);
        let edge_cases = [
            (0u64, 1u64, "Buy"),   // zero price
            (u64::MAX, 1, "Sell"), // max price
            (100, 0, "Buy"),       // zero size
            (100, u64::MAX, "Sell"), // max size
            (1, 1, "Buy"),         // min values
        ];
        for i in 0..count {
            let (price, amount, side) = edge_cases[i % edge_cases.len()];
            orders.push(TestOrder {
                market_id: self.next_u64() % 4,
                side: side.to_string(),
                price,
                amount,
            });
        }
        orders
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conservation_of_value() {
        let mut before = HashMap::new();
        before.insert(Address::ZERO, U256::from(100));
        let mut after = HashMap::new();
        after.insert(Address::ZERO, U256::from(150));
        assert!(InvariantChecker::check_conservation_of_value(
            &before,
            &after,
            U256::from(50),
            U256::ZERO
        ));
    }

    #[test]
    fn test_order_book_consistency() {
        assert!(InvariantChecker::check_order_book_consistency(100, 101));
        assert!(!InvariantChecker::check_order_book_consistency(101, 100));
    }

    #[test]
    fn test_fba_uniform_price() {
        assert!(InvariantChecker::check_fba_uniform_price(&[100, 100, 100]));
        assert!(!InvariantChecker::check_fba_uniform_price(&[100, 101]));
    }

    #[test]
    fn test_property_generator() {
        let mut generator = PropertyTestGenerator::new(42);
        let orders = generator.generate_random_orders(10);
        assert_eq!(orders.len(), 10);
        let transfers = generator.generate_random_transfers(5);
        assert!(transfers.len() <= 5);
    }
}
