//! contributeInsurance and the listing fee, through real transactions to the
//! MersennetOrders precompile (0x…0100). One test function: the precompile
//! context is process-global, so scenarios in one binary must run in sequence.

use mersennet::engine::{Engine, Transaction};
use mersennet::precompile_abi::{
    MERSENNET_ORDERS_PRECOMPILE, contribute_insurance_selector, create_market_selector, encode_u256,
};
use revm::primitives::{Address, Bytes, U256};
use tempfile::TempDir;

const MRSN: u128 = 1_000_000_000_000_000_000;

fn tx(from: Address, nonce: u64, data: Bytes) -> Transaction {
    Transaction {
        from,
        to: Some(MERSENNET_ORDERS_PRECOMPILE),
        value: U256::ZERO,
        data,
        gas_limit: 700_000,
        gas_price: U256::from(1u64),
        nonce,
        chain_id: Some(1),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
        hash: None,
        raw: None,
    }
}

fn contribute(units: u64) -> Bytes {
    let mut v = contribute_insurance_selector().to_vec();
    v.extend_from_slice(&encode_u256(U256::from(units)));
    Bytes::from(v)
}

fn create_market(symbol: &str) -> Bytes {
    let mut v = create_market_selector().to_vec();
    let mut word = [0u8; 32];
    word[..symbol.len()].copy_from_slice(symbol.as_bytes());
    v.extend_from_slice(&word);
    v.extend_from_slice(&encode_u256(U256::from(1u64)));
    v.extend_from_slice(&encode_u256(U256::from(1u64)));
    Bytes::from(v)
}

/// One block with `t`; returns whether its receipt succeeded.
fn run(engine: &mut Engine, t: Transaction) -> bool {
    engine.submit_tx_unsigned(t).expect("submit");
    let block = engine.execute_block().expect("block");
    assert_eq!(block.transactions.len(), 1);
    block.receipts[0].success
}

#[test]
fn contributions_pay_down_bad_debt_then_fill_the_fund_from_the_switch() {
    let escrow = MERSENNET_ORDERS_PRECOMPILE;
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::new_with_state(1, dir.path());
    let alice = Address::from([0x11; 20]);
    engine.fund_account(alice, U256::from(1_000 * MRSN), 0);
    // Settlement live with collateral in MRSN units, 300 units of bad debt.
    engine.orders.state.settlement_height = 1;
    engine.orders.state.units_migrated = true;
    engine.orders.state.bad_debt = U256::from(300u64);

    // Before the switch the call does not exist: nothing moves.
    assert!(!run(&mut engine, tx(alice, 0, contribute(500))));
    assert_eq!(engine.get_balance(escrow).unwrap(), U256::ZERO);
    assert_eq!(engine.orders.state.bad_debt, U256::from(300u64));

    let next = engine.latest_height() + 1;
    engine.set_mark_price_height(Some(next));

    // 500 units: 300 pay the bad debt, 200 join the fund; 500 MRSN escrowed.
    assert!(run(&mut engine, tx(alice, 1, contribute(500))));
    assert_eq!(engine.get_balance(escrow).unwrap(), U256::from(500 * MRSN));
    assert_eq!(engine.orders.state.bad_debt, U256::ZERO);
    assert_eq!(engine.orders.state.insurance_fund, U256::from(200u64));

    // More than the caller holds: refused, nothing changes.
    assert!(!run(&mut engine, tx(alice, 2, contribute(10_000))));
    assert_eq!(engine.orders.state.insurance_fund, U256::from(200u64));

    // A listing fee (100 MRSN) is credited as 100 units from the switch.
    assert!(run(&mut engine, tx(alice, 3, create_market("NEW/USD"))));
    assert_eq!(engine.orders.state.insurance_fund, U256::from(300u64));
    assert_eq!(engine.get_balance(escrow).unwrap(), U256::from(600 * MRSN));
}
