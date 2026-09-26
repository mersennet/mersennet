//! Fee floor and fee split (consensus switch `fee_floor_height`).
//!
//! Before the switch the base fee can sit at 1 wei and everything it
//! collects is burned: one account put 530k reverting transactions a day on
//! the chain for free (20–25 Sep 2026). From the switch the base fee never
//! falls below `min_base_fee_wei`, a transaction priced under it is not
//! includable, a block declaring a base fee under the floor is invalid, and
//! the base fee a block collects is split treasury / proposer / burn by the
//! configured shares — identically on the producer and every importer.

use mersennet::engine::Engine;
use revm::primitives::{Address, U256};
use tempfile::tempdir;

const FLOOR: u64 = 1_000_000_000; // 1 gwei
const TREASURY_BPS: u64 = 5_000;
const PROPOSER_BPS: u64 = 2_500;

const CAROL: [u8; 20] = [0x33; 20];

fn engine_with_floor(
    alice: Address,
    treasury: Address,
    switch: u64,
) -> (Engine, tempfile::TempDir) {
    let dir = tempdir().expect("temp dir");
    let mut engine = Engine::new_with_state(131_071, dir.path());
    engine.fund_account(alice, U256::from(10u64).pow(U256::from(24u64)), 0);
    engine.fund_account(
        Address::from_slice(&CAROL),
        U256::from(10u64).pow(U256::from(24u64)),
        0,
    );
    engine.set_fee_floor_params(switch, FLOOR, TREASURY_BPS, PROPOSER_BPS, Some(treasury));
    (engine, dir)
}

#[test]
fn base_fee_floor_and_split_apply_from_the_switch_on_producer_and_importer() {
    let alice = Address::from_slice(&[0x11; 20]);
    let bob = Address::from_slice(&[0x22; 20]);
    let treasury = Address::from_slice(&[0x77; 20]);

    // Switch at height 2: block 1 is legacy, block 2 is the first floored block.
    let (mut producer, _pd) = engine_with_floor(alice, treasury, 2);
    let (mut importer, _id) = engine_with_floor(alice, treasury, 2);
    let proposer = producer.coinbase;

    // ---- Block 1 (before the switch): 1-wei gas is fine, everything burned ----
    producer
        .transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            0,
        )
        .expect("legacy transfer");
    let b1 = producer.execute_block().expect("block 1");
    assert_eq!(b1.transactions.len(), 1);
    assert_eq!(b1.base_fee, U256::from(1u64), "legacy base fee");
    assert_eq!(
        producer.get_balance(treasury).unwrap(),
        U256::ZERO,
        "no treasury share before the switch"
    );
    importer.import_block(b1.clone());
    assert_eq!(importer.latest_height(), 1);

    // ---- Block 2 (the switch): the floor is in force ----
    assert!(
        producer.base_fee_floor_at(2) == U256::from(FLOOR),
        "floor is active from the switch height"
    );
    // The next block is the switch block: a 1-wei transaction is refused at
    // admission already (it could never be included); alice pays the floor.
    let carol = Address::from_slice(&CAROL);
    assert!(
        producer
            .transfer(
                carol,
                bob,
                U256::from(1_000u64),
                21_000,
                U256::from(1u64),
                0
            )
            .is_err(),
        "a transaction under the floor is refused once the next block is floored"
    );
    producer
        .transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(FLOOR),
            1,
        )
        .expect("floored transfer");
    let b2 = producer.execute_block().expect("block 2");
    assert_eq!(b2.number, 2);
    assert_eq!(
        b2.base_fee,
        U256::from(FLOOR),
        "the switch block already charges the floor"
    );
    assert!(
        b2.transactions
            .iter()
            .all(|t| t.gas_price >= U256::from(FLOOR)),
        "no transaction under the floor is included"
    );
    assert_eq!(
        b2.transactions.len(),
        1,
        "only the floored transfer is included"
    );
    assert_eq!(b2.transactions[0].from, alice);
    // A transaction at the floor from carol is admitted and included later.
    producer
        .transfer(
            carol,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(FLOOR),
            0,
        )
        .expect("a transaction at the floor is admitted");

    let collected = U256::from(FLOOR) * U256::from(b2.gas_used);
    let (t_share, p_share, burned) = producer.fee_split(2, b2.base_fee, b2.gas_used);
    assert_eq!(
        t_share,
        collected * U256::from(TREASURY_BPS) / U256::from(10_000u64)
    );
    assert_eq!(
        p_share,
        collected * U256::from(PROPOSER_BPS) / U256::from(10_000u64)
    );
    assert_eq!(
        t_share + p_share + burned,
        collected,
        "the split is exhaustive"
    );
    assert_eq!(
        producer.get_balance(treasury).unwrap(),
        t_share,
        "treasury receives its share of the base fee"
    );
    let proposer_balance = producer.get_balance(proposer).unwrap();
    assert!(
        proposer_balance >= p_share,
        "the proposer's recipient receives its share (got {proposer_balance}, share {p_share})"
    );

    // The importer converges: same treasury credit, same state root.
    importer.import_block(b2.clone());
    assert_eq!(importer.latest_height(), 2, "block 2 imported");
    assert_eq!(
        importer.get_balance(treasury).unwrap(),
        t_share,
        "importer credits the same treasury share"
    );
    assert_eq!(
        importer.get_balance(proposer).unwrap(),
        proposer_balance,
        "importer credits the same proposer share"
    );
    assert_eq!(
        importer.get_balance(alice).unwrap(),
        producer.get_balance(alice).unwrap()
    );

    // ---- A block priced below the floor is rejected by importers ----
    let (mut cheap_producer, _cd) = engine_with_floor(alice, treasury, 2);
    // Same first block so the chains link…
    cheap_producer
        .transfer(
            alice,
            bob,
            U256::from(1_000u64),
            21_000,
            U256::from(1u64),
            0,
        )
        .expect("legacy transfer");
    let _ = cheap_producer
        .execute_block()
        .expect("block 1 on the cheap chain");
    let (mut strict_importer, _sd) = engine_with_floor(alice, treasury, 2);
    strict_importer.import_block(b1.clone());
    // …then forge block 2 with a 1-wei base fee.
    let mut forged = b2.clone();
    forged.base_fee = U256::from(1u64);
    strict_importer.import_block(forged);
    assert_eq!(
        strict_importer.latest_height(),
        1,
        "a block under the floor is not applied"
    );

    // ---- The floor persists: the next base fee never dips below it ----
    let b3 = producer.execute_block().expect("block 3");
    assert!(
        b3.base_fee >= U256::from(FLOOR),
        "light blocks do not pull the base fee under the floor"
    );
    assert_eq!(
        b3.transactions.len(),
        1,
        "carol's floored transfer is included"
    );
    let b4 = producer.execute_block().expect("empty block 4");
    assert!(
        b4.base_fee >= U256::from(FLOOR),
        "empty blocks do not pull the base fee under the floor"
    );
}
