//! Regression test: depositTokenCollateral / withdrawTokenCollateral move real
//! ERC-20 storage through the EVM commit path.
//!
//! The precompile mutates the token's `balanceOf` mapping with raw
//! `journaled_state.sstore` calls. revm's journal `finalize()` drops accounts
//! that are not marked *touched*, so without an explicit `touch(&token)` the
//! storage writes were silently discarded at commit: deposits credited CLOB
//! collateral without ever debiting the token (found live on testnet, fixed in
//! `erc20_move`). This test drives a real transaction through `execute_block`
//! and asserts the token balances actually changed in committed state.

use mersennet::engine::{Engine, Transaction};
use mersennet::precompile_abi::{
    MERSENNET_ORDERS_PRECOMPILE, deposit_collateral_multi_selector, encode_u256,
    withdraw_collateral_multi_selector,
};
use revm::primitives::{Address, Bytes, U256, keccak256};

fn balance_slot(holder: Address, mapping_slot: U256) -> U256 {
    let mut buf = [0u8; 64];
    buf[12..32].copy_from_slice(holder.as_slice());
    buf[32..64].copy_from_slice(&mapping_slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(buf).0)
}

fn calldata(selector: [u8; 4], token: Address, amount: U256) -> Bytes {
    let mut v = Vec::with_capacity(68);
    v.extend_from_slice(&selector);
    let mut addr_word = [0u8; 32];
    addr_word[12..].copy_from_slice(token.as_slice());
    v.extend_from_slice(&addr_word);
    v.extend_from_slice(&encode_u256(amount));
    Bytes::from(v)
}

fn precompile_tx(from: Address, nonce: u64, data: Bytes) -> Transaction {
    Transaction {
        from,
        to: Some(MERSENNET_ORDERS_PRECOMPILE),
        value: U256::ZERO,
        data,
        gas_limit: 200_000,
        gas_price: U256::from(1u64),
        nonce,
        chain_id: Some(1),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
        hash: None,
    }
}

#[test]
fn token_collateral_moves_erc20_storage_through_commit() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut engine = Engine::new_with_state(1, dir.path());

    let alice = Address::from([0x22; 20]);
    let token = Address::from([0x77; 20]);
    let mapping_slot = U256::from(3u64);
    engine.fund_account(alice, U256::from(10_000_000u64), 0);

    // Register the token and seed alice's ERC-20 balance directly in storage
    // (the precompile never executes token code, only its storage).
    engine.register_collateral_asset(token, 9_000, U256::from(1u64), U256::from(1u64), mapping_slot);
    let alice_slot = balance_slot(alice, mapping_slot);
    engine
        .evm
        .db
        .insert_account_storage(token, alice_slot, U256::from(1_000u64))
        .unwrap();

    // Deposit 600: alice's token balance must drop, escrow's must rise, and
    // the CLOB must credit token collateral.
    engine
        .submit_tx_unsigned(precompile_tx(
            alice,
            0,
            calldata(deposit_collateral_multi_selector(), token, U256::from(600u64)),
        ))
        .unwrap();
    engine.execute_block().unwrap();

    let escrow_slot = balance_slot(MERSENNET_ORDERS_PRECOMPILE, mapping_slot);
    assert_eq!(
        engine.get_storage_at(token, alice_slot).unwrap(),
        U256::from(400u64),
        "deposit must debit the caller's ERC-20 balance in committed state"
    );
    assert_eq!(
        engine.get_storage_at(token, escrow_slot).unwrap(),
        U256::from(600u64),
        "deposit must credit the escrow's ERC-20 balance in committed state"
    );
    assert_eq!(
        engine
            .orders
            .state
            .accounts
            .get(&alice)
            .and_then(|a| a.token_collateral.get(&token).copied())
            .unwrap_or_default(),
        U256::from(600u64),
        "CLOB token collateral credited"
    );

    // Withdraw 250 back.
    engine
        .submit_tx_unsigned(precompile_tx(
            alice,
            1,
            calldata(withdraw_collateral_multi_selector(), token, U256::from(250u64)),
        ))
        .unwrap();
    engine.execute_block().unwrap();

    assert_eq!(
        engine.get_storage_at(token, alice_slot).unwrap(),
        U256::from(650u64),
        "withdraw must return tokens to the caller"
    );
    assert_eq!(
        engine.get_storage_at(token, escrow_slot).unwrap(),
        U256::from(350u64),
        "withdraw must debit the escrow"
    );
    assert_eq!(
        engine
            .orders
            .state
            .accounts
            .get(&alice)
            .and_then(|a| a.token_collateral.get(&token).copied())
            .unwrap_or_default(),
        U256::from(350u64),
        "CLOB token collateral decremented"
    );
}
