//! Regression tests for state application on block import.
//!
//! `execute_block` (the producer path) applies transaction state
//! changes, but historically `import_block` only stored the block and
//! bumped the height — so non-producing nodes never applied state and
//! every node's balances diverged. These tests pin the fixed behavior:
//! importing a block re-executes its transactions so a non-producing
//! node converges on the producer's state, including across
//! out-of-order delivery.

use mersennet::engine::Engine;
use revm::primitives::{Address, U256};
use tempfile::tempdir;

fn funded_engine(alice: Address) -> (Engine, tempfile::TempDir) {
    let dir = tempdir().expect("temp dir");
    let mut engine = Engine::new_with_state(131_071, dir.path());
    engine.fund_account(alice, U256::from(2_000_000u64), 0);
    (engine, dir)
}

#[test]
fn imported_block_applies_state_on_nonproducing_node() {
    let alice = Address::from_slice(&[0x11; 20]);
    let bob = Address::from_slice(&[0x22; 20]);

    let (mut producer, _pd) = funded_engine(alice);
    let (mut importer, _id) = funded_engine(alice);

    producer
        .transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            0,
        )
        .expect("transfer");
    let block = producer.execute_block().expect("produce block");
    assert_eq!(block.transactions.len(), 1);

    // The non-producing node has not seen the transaction yet.
    assert_eq!(importer.get_balance(bob).expect("bob"), U256::ZERO);

    importer.import_block(block.clone());

    // After import the importer must match the producer exactly.
    assert_eq!(
        importer.get_balance(bob).expect("bob"),
        producer.get_balance(bob).expect("bob"),
        "recipient balance must converge with the producer"
    );
    assert_eq!(
        importer.get_balance(bob).expect("bob"),
        U256::from(1_000u64)
    );
    assert_eq!(
        importer.get_balance(alice).expect("alice"),
        producer.get_balance(alice).expect("alice"),
        "sender balance must converge with the producer"
    );
    assert_eq!(importer.block_number, block.number + 1);
}

#[test]
fn out_of_order_blocks_apply_in_sequence() {
    let alice = Address::from_slice(&[0x33; 20]);
    let bob = Address::from_slice(&[0x44; 20]);

    let (mut producer, _pd) = funded_engine(alice);
    let (mut importer, _id) = funded_engine(alice);

    producer
        .transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            0,
        )
        .expect("transfer 1");
    let block1 = producer.execute_block().expect("block 1");

    producer
        .transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            1,
        )
        .expect("transfer 2");
    let block2 = producer.execute_block().expect("block 2");

    assert_eq!(block2.number, block1.number + 1);

    // Deliver out of order: block2 arrives first and must be buffered,
    // not applied, until block1 fills the gap.
    importer.import_block(block2.clone());
    assert_eq!(
        importer.get_balance(bob).expect("bob"),
        U256::ZERO,
        "future block must not apply before its predecessor"
    );
    assert_eq!(importer.block_number, block1.number);

    importer.import_block(block1.clone());

    // Both blocks now drained in order.
    assert_eq!(
        importer.get_balance(bob).expect("bob"),
        U256::from(2_000u64)
    );
    assert_eq!(importer.block_number, block2.number + 1);
}

#[test]
fn duplicate_import_is_ignored() {
    let alice = Address::from_slice(&[0x55; 20]);
    let bob = Address::from_slice(&[0x66; 20]);

    let (mut producer, _pd) = funded_engine(alice);
    let (mut importer, _id) = funded_engine(alice);

    producer
        .transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            0,
        )
        .expect("transfer");
    let block = producer.execute_block().expect("block");

    importer.import_block(block.clone());
    let bob_after_first = importer.get_balance(bob).expect("bob");

    // Re-delivering the same block must not double-apply.
    importer.import_block(block.clone());
    assert_eq!(importer.get_balance(bob).expect("bob"), bob_after_first);
    assert_eq!(
        importer.get_balance(bob).expect("bob"),
        U256::from(1_000u64)
    );
}
