//! Regression tests for the conservation-of-value invariant.
//!
//! The per-block check was fed empty balance maps, so it evaluated
//! `0 == 0 + minted - burned` and fired a false `Critical` violation on
//! every block that minted a reward. These tests pin the fixed
//! behavior: the block's actual net balance delta is reconciled against
//! the minted reward minus the burned base fee, so honest blocks report
//! no violation.

use mersennet::engine::Engine;
use mersennet::formal_verification::Invariant;
use revm::primitives::{Address, U256};
use tempfile::tempdir;

fn has_conservation_violation(engine: &Engine) -> bool {
    engine
        .invariant_checker
        .violations()
        .iter()
        .any(|v| v.invariant == Invariant::ConservationOfValue)
}

/// An engine with a staked validator so each block mints a real reward
/// (the case that triggered the false positive).
fn minting_engine(dir: &std::path::Path) -> Engine {
    let mut engine = Engine::new_with_state(131_071, dir);
    let validator = Address::from_slice(&[0xAA; 20]);
    engine
        .add_validator(validator, U256::from(1_000_000u64))
        .expect("add validator");
    // Mainnet tokenomics: 2^89-1 cap, 2^61-1 reward/block (~2.3 MRSN), no halving
    // within the test horizon (interval = 5th perfect number).
    engine.set_token_economics(
        U256::from(2u128.pow(89) - 1),
        U256::from(2u128.pow(61) - 1),
        33_550_336,
    );
    engine
}

#[test]
fn empty_minting_blocks_conserve_value() {
    let dir = tempdir().unwrap();
    let mut engine = minting_engine(dir.path());
    for _ in 0..5 {
        engine.execute_block().expect("block");
        assert!(
            !has_conservation_violation(&engine),
            "an empty block that mints a reward must not violate conservation"
        );
    }
}

#[test]
fn transfer_block_conserves_value() {
    let dir = tempdir().unwrap();
    let mut engine = minting_engine(dir.path());
    let alice = Address::from_slice(&[0x11; 20]);
    let bob = Address::from_slice(&[0x22; 20]);
    engine.fund_account(alice, U256::from(2_000_000u64), 0);

    engine
        .transfer(alice, bob, U256::from(1_000u64), 21_000, U256::from(1u64), 0)
        .expect("transfer");
    engine.execute_block().expect("block");

    assert!(
        !has_conservation_violation(&engine),
        "a value transfer plus minted reward must conserve value"
    );
    assert_eq!(engine.get_balance(bob).expect("bob"), U256::from(1_000u64));
}
