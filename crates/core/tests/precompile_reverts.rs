//! Business errors of the stateful precompiles become reasoned reverts from
//! `revert_reasons_height`.
//!
//! Before the switch a refused CLOB call is a precompile halt: the receipt is
//! a failure with empty output and the whole gas limit is consumed (300k per
//! refused order — 530k of them a day between 20 and 25 Sep 2026, with no
//! reason a wallet could show). From the switch the same call is an ordinary
//! revert: `Error(string)` output carrying the precompile's message, and only
//! the call's base gas charged on top of the intrinsic cost.
//!
//! One test function on purpose: the orders precompile uses a process-global
//! context set per `execute_block`, so parallel tests in one binary would
//! clobber each other.

use mersennet::engine::{Engine, Transaction};
use mersennet::precompile_abi::{
    MERSENNET_ORDERS_PRECOMPILE, deposit_collateral_selector, encode_u256, place_order_selector,
};
use mersennet::precompiles::{GAS_PRECOMPILE_REVERT, encode_error_string};
use revm::primitives::{Address, Bytes, U256};
use tempfile::TempDir;

fn place_order_calldata(market: u64, is_buy: bool, price: u64, size: u64) -> Bytes {
    let mut v = Vec::with_capacity(4 + 5 * 32);
    v.extend_from_slice(&place_order_selector());
    for word in [market, u64::from(is_buy), price, size, 0] {
        v.extend_from_slice(&encode_u256(U256::from(word)));
    }
    Bytes::from(v)
}

fn tx(from: Address, nonce: u64, gas_limit: u64, data: Bytes) -> Transaction {
    Transaction {
        from,
        to: Some(MERSENNET_ORDERS_PRECOMPILE),
        value: U256::ZERO,
        data,
        gas_limit,
        gas_price: U256::from(1u64),
        nonce,
        chain_id: Some(1),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
        hash: None,
    }
}

fn decode_error_string(out: &[u8]) -> Option<String> {
    if out.len() < 4 + 64 || out[..4] != [0x08, 0xc3, 0x79, 0xa0] {
        return None;
    }
    let len = U256::from_be_slice(&out[4 + 32..4 + 64]).to::<usize>();
    String::from_utf8(out[4 + 64..4 + 64 + len].to_vec()).ok()
}

#[test]
fn precompile_business_errors_revert_with_a_reason_from_the_switch() {
    const GAS_LIMIT: u64 = 300_000;
    let alice = Address::from([0x11; 20]);
    // Market 99 does not exist in a fresh engine: "unknown market" is the
    // first check placeOrder fails, whatever the account holds.
    let refused = place_order_calldata(99, true, 100, 1);

    // ---- Legacy: a halt that burns everything and says nothing ----
    {
        let dir = TempDir::new().unwrap();
        let mut engine = Engine::new_with_state(1, dir.path());
        engine.fund_account(alice, U256::from(10u64).pow(U256::from(24u64)), 0);
        engine
            .submit_tx_unsigned(tx(alice, 0, GAS_LIMIT, refused.clone()))
            .expect("submit");
        let blk = engine.execute_block().expect("block");
        let r = &blk.receipts[0];
        assert!(!r.success, "the refused order fails");
        assert!(r.output.is_empty(), "legacy halt carries no output");
        assert_eq!(
            r.gas_used, GAS_LIMIT,
            "legacy halt burns the whole gas limit"
        );
    }

    // ---- From the switch: a revert with the reason, base gas only ----
    {
        let dir = TempDir::new().unwrap();
        let mut engine = Engine::new_with_state(1, dir.path());
        engine.set_revert_reasons_height(1);
        engine.fund_account(alice, U256::from(10u64).pow(U256::from(24u64)), 0);
        let before = engine.get_balance(alice).unwrap();

        engine
            .submit_tx_unsigned(tx(alice, 0, GAS_LIMIT, refused.clone()))
            .expect("submit");
        let blk = engine.execute_block().expect("block");
        assert!(blk.number >= 1, "switch height 1 is active");
        let r = &blk.receipts[0];
        assert!(!r.success, "the refused order still fails");
        let reason = decode_error_string(&r.output).expect("Error(string) revert data");
        assert!(
            reason.contains("unknown market"),
            "the precompile's message is the revert reason, got {reason:?}"
        );
        assert_eq!(
            r.output,
            encode_error_string(&reason),
            "canonical Error(string) encoding"
        );
        // Intrinsic (21k + calldata) + the flat precompile revert charge, nothing more.
        let intrinsic_max = 21_000 + 16 * refused.len() as u64;
        assert!(
            r.gas_used <= intrinsic_max + GAS_PRECOMPILE_REVERT,
            "only base gas is charged: used {} (limit {GAS_LIMIT})",
            r.gas_used
        );
        assert!(
            r.gas_used > GAS_PRECOMPILE_REVERT,
            "the flat charge is applied"
        );
        assert_eq!(
            engine.get_balance(alice).unwrap(),
            before - U256::from(r.gas_used),
            "the unused gas is refunded to the sender"
        );

        // A call the precompile accepts is untouched by the wrapper.
        let mut dep = Vec::with_capacity(36);
        dep.extend_from_slice(&deposit_collateral_selector());
        dep.extend_from_slice(&encode_u256(U256::from(5u64)));
        engine
            .submit_tx_unsigned(tx(alice, 1, GAS_LIMIT, Bytes::from(dep)))
            .expect("submit deposit");
        let blk = engine.execute_block().expect("deposit block");
        assert!(
            blk.receipts[0].success,
            "a valid deposit still succeeds after the switch"
        );

        // eth_call surfaces the same reason for wallets and terminals.
        let sim = engine
            .simulate_call(
                alice,
                Some(MERSENNET_ORDERS_PRECOMPILE),
                refused.clone(),
                GAS_LIMIT,
                U256::ZERO,
            )
            .expect("simulate_call answers");
        assert!(!sim.success, "simulation reports the refusal");
        assert!(
            decode_error_string(sim.output.as_ref())
                .map(|m| m.contains("unknown market"))
                .unwrap_or(false),
            "simulation returns the Error(string) too, got {:?}",
            sim.output
        );
    }
}
