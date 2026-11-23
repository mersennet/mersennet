//! `eth_call` / `eth_estimateGas` report a revert as a JSON-RPC error, the way
//! geth does: code 3, `execution reverted` (plus the `Error(string)` reason
//! when there is one) and the revert bytes as `data`. A halt (out of gas,
//! invalid opcode) is a -32000 error. Successful calls, and precompile calls
//! that answer `false` or a value, are unchanged. The legacy alias
//! `mersennet_call` keeps answering a revert with its bytes as the result.
//!
//! The order-book and staking precompiles read a process-wide context that
//! every simulation installs and clears, so the tests in this binary take a
//! lock and run one at a time.

use mersennet::engine::Engine;
use mersennet::precompile_abi::{
    MERSENNET_ORDERS_PRECOMPILE, STAKING_PRECOMPILE, encode_u256, get_delegation_selector,
    place_order_selector, withdraw_collateral_selector,
};
use mersennet::precompiles::encode_error_string;
use mersennet_rpc::rpc_router::{RpcError, route};
use revm::primitives::{Address, Bytes, U256};
use serde_json::{Value, json};
use std::sync::Mutex;
use tempfile::TempDir;

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn addr(byte: u8) -> Address {
    Address::from_slice(&[byte; 20])
}

fn hex_addr(address: Address) -> String {
    format!("0x{}", hex::encode(address.as_slice()))
}

fn hex_bytes(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// 12-byte prologue that copies the code after it into memory and ends with
/// `last` over it: `0xf3` (RETURN) or `0xfd` (REVERT).
fn copy_tail_then(last: u8, tail: &[u8]) -> Vec<u8> {
    let n = u16::try_from(tail.len()).expect("short tail");
    let [hi, lo] = n.to_be_bytes();
    // PUSH2 n, DUP1, PUSH1 12, PUSH1 0, CODECOPY, PUSH1 0, RETURN|REVERT
    let mut code = vec![
        0x61, hi, lo, 0x80, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x00, last,
    ];
    code.extend_from_slice(tail);
    code
}

/// Init code that deploys `runtime`.
fn creation(runtime: &[u8]) -> Bytes {
    Bytes::from(copy_tail_then(0xf3, runtime))
}

/// Runtime code that always reverts with `payload`.
fn reverting_with(payload: &[u8]) -> Vec<u8> {
    copy_tail_then(0xfd, payload)
}

/// `Error(string)` revert data, as Solidity's `require(false, msg)` encodes it.
fn error_string(msg: &str) -> Vec<u8> {
    encode_error_string(msg).to_vec()
}

struct Fixture {
    engine: Engine,
    _dir: TempDir,
    caller: Address,
    returns_42: Address,
    reverts_with_reason: Address,
    reverts_custom: Address,
    reverts_bare: Address,
    invalid_opcode: Address,
    loops_forever: Address,
}

const CUSTOM_ERROR: [u8; 36] = {
    // error Unauthorized(uint256 code) with code = 7: selector + one word.
    let mut b = [0u8; 36];
    b[0] = 0xde;
    b[1] = 0xad;
    b[2] = 0xbe;
    b[3] = 0xef;
    b[35] = 7;
    b
};

fn fixture() -> Fixture {
    let dir = TempDir::new().expect("temp dir");
    let mut engine = Engine::new_with_state(131071, dir.path());
    let deployer = addr(0x66);
    let caller = addr(0x77);
    engine.fund_account(deployer, U256::from(10u64).pow(U256::from(21u64)), 0);
    engine.fund_account(caller, U256::from(10u64).pow(U256::from(21u64)), 0);

    // PUSH1 42, PUSH1 0, MSTORE, PUSH1 32, PUSH1 0, RETURN
    let returns_42 = vec![0x60, 0x2a, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3];
    let runtimes: Vec<Vec<u8>> = vec![
        returns_42,
        reverting_with(&error_string("not enough balance")),
        reverting_with(&CUSTOM_ERROR),
        // PUSH1 0, PUSH1 0, REVERT
        vec![0x60, 0x00, 0x60, 0x00, 0xfd],
        // INVALID
        vec![0xfe],
        // JUMPDEST, PUSH1 0, JUMP
        vec![0x5b, 0x60, 0x00, 0x56],
    ];
    for (nonce, runtime) in runtimes.iter().enumerate() {
        engine
            .deploy_contract(
                deployer,
                creation(runtime),
                1_000_000,
                U256::from(1u64),
                nonce as u64,
                U256::ZERO,
            )
            .expect("deploy accepted");
    }
    let block = engine.execute_block().expect("deploy block");
    let created: Vec<Address> = block
        .receipts
        .iter()
        .map(|r| {
            assert!(r.success, "deployment succeeds");
            r.created_address.expect("contract address")
        })
        .collect();
    assert_eq!(created.len(), runtimes.len());
    for (address, runtime) in created.iter().zip(&runtimes) {
        assert_eq!(
            engine.get_code(*address).expect("code").to_vec(),
            *runtime,
            "runtime code deployed"
        );
    }
    Fixture {
        engine,
        _dir: dir,
        caller,
        returns_42: created[0],
        reverts_with_reason: created[1],
        reverts_custom: created[2],
        reverts_bare: created[3],
        invalid_opcode: created[4],
        loops_forever: created[5],
    }
}

fn call_params(from: Address, to: Address, data: &[u8], gas: Option<u64>) -> Value {
    let mut obj = json!({
        "from": hex_addr(from),
        "to": hex_addr(to),
        "data": hex_bytes(data),
    });
    if let Some(g) = gas {
        obj["gas"] = Value::String(format!("0x{g:x}"));
    }
    json!([obj, "latest"])
}

fn call(f: &mut Fixture, method: &str, to: Address, data: &[u8]) -> Result<Value, RpcError> {
    route(method, call_params(f.caller, to, data, None), &mut f.engine)
}

fn assert_revert(err: &RpcError, message: &str, data: &[u8]) {
    assert_eq!(err.code, 3, "a revert is JSON-RPC error 3: {err:?}");
    assert_eq!(err.message, message);
    assert_eq!(
        err.data,
        Some(Value::String(hex_bytes(data))),
        "the revert bytes travel as error.data"
    );
}

#[test]
fn eth_call_reports_an_error_string_revert_with_its_reason_and_bytes() {
    let _serial = serial();
    let mut f = fixture();
    let target = f.reverts_with_reason;
    let err = call(&mut f, "eth_call", target, &[]).expect_err("a revert is an error");
    assert_revert(
        &err,
        "execution reverted: not enough balance",
        &error_string("not enough balance"),
    );
}

#[test]
fn eth_call_reports_a_custom_error_revert_with_its_bytes() {
    let _serial = serial();
    let mut f = fixture();
    let target = f.reverts_custom;
    let err = call(&mut f, "eth_call", target, &[]).expect_err("a revert is an error");
    assert_revert(&err, "execution reverted", &CUSTOM_ERROR);

    let target = f.reverts_bare;
    let err = call(&mut f, "eth_call", target, &[]).expect_err("a bare revert is an error");
    assert_revert(&err, "execution reverted", &[]);
}

#[test]
fn eth_call_reports_a_halt_as_a_server_error() {
    let _serial = serial();
    let mut f = fixture();
    let target = f.invalid_opcode;
    let err = call(&mut f, "eth_call", target, &[]).expect_err("a halt is an error");
    assert_eq!(err.code, -32000);
    assert_eq!(err.message, "invalid opcode: INVALID");
    assert_eq!(err.data, None);

    let params = call_params(f.caller, f.loops_forever, &[], Some(100_000));
    let err = route("eth_call", params, &mut f.engine).expect_err("out of gas is an error");
    assert_eq!(err.code, -32000);
    assert_eq!(err.message, "out of gas");

    // Gas below the intrinsic cost is refused before execution; the node
    // keeps answering afterwards (both methods).
    for method in ["eth_call", "mersennet_call"] {
        let params = call_params(f.caller, f.returns_42, &[], Some(1));
        let err = route(method, params, &mut f.engine).expect_err("gas below intrinsic");
        assert_eq!(err.code, -32000, "{method}: {err:?}");
        let target = f.returns_42;
        call(&mut f, method, target, &[]).expect("still answers");
    }
}

#[test]
fn eth_call_and_estimate_gas_of_a_successful_call_are_unchanged() {
    let _serial = serial();
    let mut f = fixture();
    let target = f.returns_42;
    let out = call(&mut f, "eth_call", target, &[]).expect("success");
    assert_eq!(out, Value::String(format!("0x{}2a", "0".repeat(62))));
    let gas = call(&mut f, "eth_estimateGas", target, &[0x01]).expect("success");
    let gas = u64::from_str_radix(gas.as_str().expect("hex").trim_start_matches("0x"), 16)
        .expect("hex quantity");
    assert!(
        gas > 21_000 && gas < 30_000,
        "intrinsic gas plus a few opcodes, got {gas}"
    );
    // Same answer from the legacy alias.
    let out = call(&mut f, "mersennet_call", target, &[]).expect("success");
    assert_eq!(out, Value::String(format!("0x{}2a", "0".repeat(62))));
}

#[test]
fn eth_estimate_gas_reports_reverts_and_halts_as_errors() {
    let _serial = serial();
    let mut f = fixture();
    // Non-empty calldata: an empty call to an address is priced at 21,000
    // without running it.
    let target = f.reverts_with_reason;
    let err = call(&mut f, "eth_estimateGas", target, &[0x01]).expect_err("revert");
    assert_revert(
        &err,
        "execution reverted: not enough balance",
        &error_string("not enough balance"),
    );

    let target = f.reverts_custom;
    let err = call(&mut f, "eth_estimateGas", target, &[0x01]).expect_err("revert");
    assert_revert(&err, "execution reverted", &CUSTOM_ERROR);

    let target = f.invalid_opcode;
    let err = call(&mut f, "eth_estimateGas", target, &[0x01]).expect_err("halt");
    assert_eq!(err.code, -32000);
    assert_eq!(err.message, "invalid opcode: INVALID");

    let params = call_params(f.caller, f.loops_forever, &[0x01], Some(100_000));
    let err = route("eth_estimateGas", params, &mut f.engine).expect_err("out of gas");
    assert_eq!(err.code, -32000);
    assert_eq!(err.message, "gas required exceeds allowance (100000)");
}

#[test]
fn mersennet_call_keeps_answering_a_revert_with_its_bytes() {
    let _serial = serial();
    let mut f = fixture();
    let target = f.reverts_with_reason;
    let out = call(&mut f, "mersennet_call", target, &[]).expect("legacy result");
    assert_eq!(
        out,
        Value::String(hex_bytes(&error_string("not enough balance")))
    );
    let target = f.invalid_opcode;
    let out = call(&mut f, "mersennet_call", target, &[]).expect("legacy result");
    assert_eq!(
        out,
        Value::String("0x".into()),
        "a halt answers empty output"
    );
}

#[test]
fn precompile_answers_and_refusals() {
    let _serial = serial();
    let mut f = fixture();
    f.engine.set_revert_reasons_height(1);

    // withdrawCollateral with nothing deposited answers `false` in a
    // successful call: a result, not an error.
    let mut withdraw = withdraw_collateral_selector().to_vec();
    withdraw.extend_from_slice(&encode_u256(U256::from(5u64)));
    let out = call(&mut f, "eth_call", MERSENNET_ORDERS_PRECOMPILE, &withdraw)
        .expect("false is a result");
    assert_eq!(out, Value::String(format!("0x{}", "0".repeat(64))));
    call(
        &mut f,
        "eth_estimateGas",
        MERSENNET_ORDERS_PRECOMPILE,
        &withdraw,
    )
    .expect("a call answering false has a gas estimate");

    // A staking view answers its values.
    let mut delegation = get_delegation_selector().to_vec();
    delegation.extend_from_slice(&[0u8; 12]);
    delegation.extend_from_slice(f.caller.as_slice());
    delegation.extend_from_slice(&[0u8; 12]);
    delegation.extend_from_slice(addr(0x55).as_slice());
    let out =
        call(&mut f, "eth_call", STAKING_PRECOMPILE, &delegation).expect("a view is a result");
    assert_eq!(out, Value::String(format!("0x{}", "0".repeat(128))));

    // An order on a market that does not exist is refused with a reason.
    let mut order = place_order_selector().to_vec();
    for word in [99u64, 1, 100, 1, 0] {
        order.extend_from_slice(&encode_u256(U256::from(word)));
    }
    let err = call(&mut f, "eth_call", MERSENNET_ORDERS_PRECOMPILE, &order)
        .expect_err("a refused order is a revert");
    assert_revert(
        &err,
        "execution reverted: unknown market",
        &error_string("unknown market"),
    );
    let err = call(
        &mut f,
        "eth_estimateGas",
        MERSENNET_ORDERS_PRECOMPILE,
        &order,
    )
    .expect_err("a refused order has no gas estimate");
    assert_revert(
        &err,
        "execution reverted: unknown market",
        &error_string("unknown market"),
    );
    let out = call(
        &mut f,
        "mersennet_call",
        MERSENNET_ORDERS_PRECOMPILE,
        &order,
    )
    .expect("legacy result");
    assert_eq!(
        out,
        Value::String(hex_bytes(&error_string("unknown market")))
    );
}
