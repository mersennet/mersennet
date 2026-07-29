//! End-to-end test that CLOB collateral is backed by native MRSN.
//!
//! A real transaction to the MersennetOrders precompile (0x…0100) must move
//! native MRSN: depositCollateral debits the caller into the precompile's escrow
//! address, and withdrawCollateral pays it back. This exercises the actual EVM
//! precompile path (revm) inside `execute_block`, confirming the stateful
//! precompile wiring, the journaled transfer, and that `transact_commit` flushes
//! the escrow balance into committed state.
//!
//! Both scenarios live in ONE test function on purpose: the orders precompile
//! uses a process-global context set per `execute_block`, so concurrent tests in
//! the same binary would clobber each other. Running sequentially avoids that.
//!
//! base_fee defaults to 1, so txs use gas_price 1; the escrow assertions are
//! gas-independent (gas is burned, never touches 0x…0100) and alice's net loss
//! over the round-trip equals exactly the gas burned (read from the receipts).

use mersennet::engine::{Engine, Transaction};
use mersennet::formal_verification::Invariant;
use mersennet::precompile_abi::{
    MERSENNET_ORDERS_PRECOMPILE, deposit_collateral_selector, encode_u256,
    withdraw_collateral_selector,
};
use revm::primitives::{Address, Bytes, U256};
use tempfile::TempDir;

fn calldata(selector: [u8; 4], amount: U256) -> Bytes {
    let mut v = Vec::with_capacity(36);
    v.extend_from_slice(&selector);
    v.extend_from_slice(&encode_u256(amount));
    Bytes::from(v)
}

fn precompile_tx(from: Address, nonce: u64, gas_limit: u64, data: Bytes) -> Transaction {
    Transaction {
        from,
        to: Some(MERSENNET_ORDERS_PRECOMPILE),
        value: U256::ZERO,
        data,
        gas_limit,
        gas_price: U256::from(1u64), // >= base_fee (1) so the tx is includable
        nonce,
        chain_id: Some(1),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
        hash: None,
    }
}

fn has_conservation_violation(engine: &Engine) -> bool {
    engine
        .invariant_checker
        .violations()
        .iter()
        .any(|v| v.invariant == Invariant::ConservationOfValue)
}

#[test]
fn collateral_is_backed_by_native_mrsn() {
    let escrow = MERSENNET_ORDERS_PRECOMPILE;

    // ---- Scenario 1: deposit + withdraw round-trip moves native MRSN ----
    {
        let dir = TempDir::new().unwrap();
        let mut engine = Engine::new_with_state(1, dir.path());
        let alice = Address::from([0x11; 20]);
        let initial = U256::from(1_000_000u64);
        let amount = U256::from(250_000u64);
        engine.fund_account(alice, initial, 0);

        assert_eq!(
            engine.get_balance(escrow).unwrap(),
            U256::ZERO,
            "escrow starts empty"
        );

        // deposit: caller -> escrow
        engine
            .submit_tx_unsigned(precompile_tx(
                alice,
                0,
                200_000,
                calldata(deposit_collateral_selector(), amount),
            ))
            .expect("submit deposit");
        let dblk = engine.execute_block().expect("deposit block");
        assert_eq!(dblk.transactions.len(), 1, "deposit tx must be included");
        assert!(dblk.receipts[0].success, "deposit tx must succeed");
        let gas0 = U256::from(dblk.receipts[0].gas_used);

        assert_eq!(
            engine.get_balance(escrow).unwrap(),
            amount,
            "deposited MRSN is escrowed at the precompile (collateral is backed)"
        );
        assert_eq!(
            engine.get_balance(alice).unwrap(),
            initial - amount - gas0,
            "caller debited by exactly the deposit + gas burned"
        );
        assert!(
            !has_conservation_violation(&engine),
            "deposit conserves supply"
        );

        // withdraw: escrow -> caller (no positions, so margin allows it)
        engine
            .submit_tx_unsigned(precompile_tx(
                alice,
                1,
                200_000,
                calldata(withdraw_collateral_selector(), amount),
            ))
            .expect("submit withdraw");
        let wblk = engine.execute_block().expect("withdraw block");
        assert_eq!(wblk.transactions.len(), 1, "withdraw tx must be included");
        assert!(wblk.receipts[0].success, "withdraw tx must succeed");
        let gas1 = U256::from(wblk.receipts[0].gas_used);

        assert_eq!(
            engine.get_balance(escrow).unwrap(),
            U256::ZERO,
            "escrow is drained when collateral is withdrawn"
        );
        assert_eq!(
            engine.get_balance(alice).unwrap(),
            initial - gas0 - gas1,
            "caller recovers the full collateral; only gas is spent over the round-trip"
        );
        assert!(
            !has_conservation_violation(&engine),
            "withdraw conserves supply"
        );
    }

    // ---- Scenario 2: a deposit the caller can't cover reverts (no minting) ----
    {
        let dir = TempDir::new().unwrap();
        let mut engine = Engine::new_with_state(1, dir.path());
        let bob = Address::from([0x22; 20]);
        // Enough to afford gas (gas_limit * gas_price = 60_000) but far less than
        // the attempted deposit, so the escrow transfer in the precompile fails.
        engine.fund_account(bob, U256::from(70_000u64), 0);

        engine
            .submit_tx_unsigned(precompile_tx(
                bob,
                0,
                60_000,
                calldata(deposit_collateral_selector(), U256::from(500_000u64)),
            ))
            .expect("submit deposit");
        let blk = engine.execute_block().expect("block");

        assert_eq!(blk.transactions.len(), 1, "tx is included (it can pay gas)");
        assert!(
            !blk.receipts[0].success,
            "an unbackable deposit must revert"
        );
        assert_eq!(
            engine.get_balance(escrow).unwrap(),
            U256::ZERO,
            "a reverted deposit escrows nothing — no collateral conjured from nothing"
        );
    }
}
