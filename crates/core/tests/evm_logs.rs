//! Regression test: EVM event logs emitted during execution must appear in the
//! transaction receipt (so `eth_getTransactionReceipt`, `eth_getLogs`, and the
//! explorer's token-transfer indexer can see them).
//!
//! On the live testnet a successful ERC-20 `transfer` produced a receipt with
//! an empty `logs` array and a zero `logsBloom`, so no token transfers were
//! ever indexed. This drives a minimal LOG-emitting contract through the real
//! `execute_block` path and asserts the log survives into the receipt.

use mersennet::engine::{Engine, Transaction};
use revm::primitives::{Address, Bytes, U256};
use tempfile::TempDir;

fn tx(from: Address, to: Option<Address>, nonce: u64, data: Bytes) -> Transaction {
    Transaction {
        from,
        to,
        value: U256::ZERO,
        data,
        gas_limit: 300_000,
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
fn emitted_evm_logs_land_in_the_receipt() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::new_with_state(1, dir.path());
    let alice = Address::from([0x11; 20]);
    engine.fund_account(alice, U256::from(1_000_000_000u64), 0);

    // Runtime bytecode: LOG1 with topic 0xAA..AA over 32 bytes of memory, then STOP.
    //   PUSH1 0x20 PUSH1 0x00 MSTORE-less: we just LOG1 mem[0..0] with one topic.
    //   PUSH32 <topic> PUSH1 0x00 PUSH1 0x00 LOG1 STOP
    // mem[0..0] is empty data; the topic is what the indexer keys on.
    let mut runtime = vec![0x7f]; // PUSH32
    runtime.extend_from_slice(&[0xAA; 32]); // topic
    runtime.extend_from_slice(&[
        0x60, 0x00, // PUSH1 0x00  (length)
        0x60, 0x00, // PUSH1 0x00  (offset)
        0xa1, // LOG1
        0x00, // STOP
    ]);
    let runtime_len = runtime.len();

    // Init code: CODECOPY the runtime out of the init payload and RETURN it.
    //   PUSH1 <len> PUSH1 0x0c PUSH1 0x00 CODECOPY PUSH1 <len> PUSH1 0x00 RETURN
    // The init prologue below is exactly 12 (0x0c) bytes, so runtime starts at 0x0c.
    let mut init = vec![
        0x60,
        runtime_len as u8, // PUSH1 len
        0x60,
        0x0c, // PUSH1 0x0c (runtime offset within this code)
        0x60,
        0x00, // PUSH1 0x00 (dest offset in memory)
        0x39, // CODECOPY
        0x60,
        runtime_len as u8, // PUSH1 len
        0x60,
        0x00, // PUSH1 0x00
        0xf3, // RETURN
    ];
    init.extend_from_slice(&runtime);

    // Deploy.
    engine
        .submit_tx_unsigned(tx(alice, None, 0, Bytes::from(init)))
        .expect("submit deploy");
    let deploy_blk = engine.execute_block().expect("deploy block");
    assert_eq!(deploy_blk.receipts.len(), 1, "deploy tx included");
    assert!(deploy_blk.receipts[0].success, "deploy succeeds");
    let contract = deploy_blk.receipts[0]
        .created_address
        .expect("contract address returned");

    // Call the contract — its runtime emits LOG1.
    engine
        .submit_tx_unsigned(tx(alice, Some(contract), 1, Bytes::new()))
        .expect("submit call");
    let call_blk = engine.execute_block().expect("call block");
    assert_eq!(call_blk.receipts.len(), 1, "call tx included");
    let receipt = &call_blk.receipts[0];
    assert!(receipt.success, "call succeeds");

    assert_eq!(
        receipt.logs.len(),
        1,
        "the emitted LOG1 must be recorded in the receipt (was empty on the live chain)"
    );
    let log = &receipt.logs[0];
    assert_eq!(log.address, contract, "log carries the emitting contract address");
    assert_eq!(log.topics.len(), 1, "LOG1 has exactly one topic");
    assert_eq!(log.topics[0].0, [0xAA; 32], "topic round-trips intact");
}

/// The live bug lived on the IMPORT path: the gossip wire form (`WireReceipt`)
/// drops logs, so a follower / public RPC node that ingests a block over the
/// network ends up with empty receipt logs. `apply_imported_block` now rebuilds
/// receipts from local re-execution. This reproduces the wire-stripping and
/// asserts the importer reconstructs the logs.
#[test]
fn imported_block_reconstructs_stripped_receipt_logs() {
    let dir_p = TempDir::new().unwrap();
    let dir_i = TempDir::new().unwrap();
    let mut producer = Engine::new_with_state(1, dir_p.path());
    let mut importer = Engine::new_with_state(1, dir_i.path());
    let alice = Address::from([0x11; 20]);
    producer.fund_account(alice, U256::from(1_000_000_000u64), 0);
    importer.fund_account(alice, U256::from(1_000_000_000u64), 0);

    // Same LOG1 contract as above.
    let mut runtime = vec![0x7f];
    runtime.extend_from_slice(&[0xAA; 32]);
    runtime.extend_from_slice(&[0x60, 0x00, 0x60, 0x00, 0xa1, 0x00]);
    let runtime_len = runtime.len();
    let mut init = vec![
        0x60, runtime_len as u8, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, runtime_len as u8, 0x60,
        0x00, 0xf3,
    ];
    init.extend_from_slice(&runtime);

    producer
        .submit_tx_unsigned(tx(alice, None, 0, Bytes::from(init)))
        .expect("deploy");
    let deploy_blk = producer.execute_block().expect("deploy block");
    let contract = deploy_blk.receipts[0].created_address.expect("addr");

    producer
        .submit_tx_unsigned(tx(alice, Some(contract), 1, Bytes::new()))
        .expect("call");
    let call_blk = producer.execute_block().expect("call block");
    assert_eq!(call_blk.receipts[0].logs.len(), 1, "producer has the log");

    // Simulate the gossip wire round-trip, which strips receipt logs.
    let strip = |mut b: mersennet::engine::Block| {
        for r in &mut b.receipts {
            r.logs.clear();
        }
        b
    };
    importer.import_block(strip(deploy_blk));
    importer.import_block(strip(call_blk.clone()));

    // The importer must have reconstructed the log locally despite the wire
    // form carrying none.
    let imported = importer
        .chain
        .iter()
        .find(|b| b.number == call_blk.number)
        .expect("call block imported");
    assert_eq!(
        imported.receipts[0].logs.len(),
        1,
        "importer reconstructs the stripped log (this is the live-chain bug)"
    );
    assert_eq!(imported.receipts[0].logs[0].topics[0].0, [0xAA; 32]);
}
