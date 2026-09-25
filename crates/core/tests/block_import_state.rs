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
fn chain_resumes_at_persisted_height_after_restart() {
    let alice = Address::from_slice(&[0x77; 20]);
    let bob = Address::from_slice(&[0x88; 20]);
    let dir = tempdir().expect("temp dir");

    let last_height;
    let bob_balance;
    {
        let mut engine = Engine::new_with_state(131_071, dir.path());
        engine.fund_account(alice, U256::from(2_000_000u64), 0);
        engine
            .transfer(
                alice,
                bob,
                U256::from(1_000u64),
                21_000,
                U256::from(1u64),
                0,
            )
            .expect("transfer");
        let _b1 = engine.execute_block().expect("block 1");
        let _b2 = engine.execute_block().expect("block 2");
        last_height = engine.latest_height();
        bob_balance = engine.get_balance(bob).expect("bob");
        assert!(last_height >= 2, "should have produced at least 2 blocks");
    }

    // Reopen the same datadir. Without height restoration the chain
    // resets to genesis (block_number = 1) while balances load from
    // disk — a corrupt resume. It must instead pick up where it left
    // off.
    let mut reopened = Engine::new_with_state(131_071, dir.path());
    assert_eq!(
        reopened.latest_height(),
        last_height,
        "height must persist across restart"
    );
    assert_eq!(
        reopened.block_number,
        last_height + 1,
        "next block must follow the persisted height, not reset to genesis"
    );
    assert_eq!(
        reopened.get_balance(bob).expect("bob"),
        bob_balance,
        "balances must persist across restart"
    );
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

/// Remove the block record at `height` from a closed state store, leaving
/// `latest_height` untouched — the on-disk shape a hard kill between
/// `record_height` and `store_block` produces (no marker on redb; on sled the
/// interrupted-commit marker normally catches it, so the marker is not set
/// here to exercise the resume path itself).
fn drop_block_record(dir: &std::path::Path, backend: &str, height: u64) {
    match backend {
        "sled" => {
            let db = sled::open(dir).expect("open sled");
            let blocks = db.open_tree("blocks").expect("blocks tree");
            blocks.remove(height.to_be_bytes()).expect("remove");
            db.flush().expect("flush");
        }
        "redb" => {
            use redb::TableDefinition;
            const BLOCKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blocks");
            let db = redb::Database::open(dir.join("mersennet.redb")).expect("open redb");
            let txn = db.begin_write().expect("write txn");
            {
                let mut table = txn.open_table(BLOCKS).expect("blocks table");
                table
                    .remove(height.to_be_bytes().as_slice())
                    .expect("remove");
            }
            txn.commit().expect("commit");
        }
        other => panic!("unknown backend {other}"),
    }
}

/// Field report (f67d812): "persisted_height=487, next_block=488, then
/// FORK DETECTED at 488 (parent hash mismatch) forever, also after restarts".
/// The state was committed at 487 but the block record for 487 was missing,
/// so the in-memory head was 486, block 488 could never chain onto it, and
/// block 487 arriving from a peer was refused as "already applied". The node
/// must (a) say so at resume and (b) accept the missing head from block-sync
/// without re-executing it, then continue normally.
#[test]
fn resume_with_missing_head_block_record_heals_from_block_sync() {
    for backend in ["sled", "redb"] {
        let alice = Address::from_slice(&[0x31; 20]);
        let bob = Address::from_slice(&[0x32; 20]);
        let dir = tempdir().expect("temp dir");

        // Produce three blocks; each commits state and stores its record.
        let (produced, bob_after_3) = {
            let mut engine = Engine::new_with_backend(131_071, dir.path(), backend);
            engine.fund_account(alice, U256::from(5_000_000u64), 0);
            let mut produced = Vec::new();
            for nonce in 0..3u64 {
                engine
                    .transfer(
                        alice,
                        bob,
                        U256::from(1_000u64),
                        21_000,
                        U256::from(1u64),
                        nonce,
                    )
                    .expect("transfer");
                produced.push(engine.execute_block().expect("block"));
            }
            assert_eq!(produced[2].number, 3);
            (produced, engine.get_balance(bob).expect("bob"))
        };
        let b3 = produced[2].clone();

        // The crash shape: latest_height = 3, no record for block 3.
        drop_block_record(dir.path(), backend, 3);

        let mut reopened = Engine::new_with_backend(131_071, dir.path(), backend);
        assert_eq!(
            reopened.block_number, 4,
            "[{backend}] state is committed at 3, so 4 is next"
        );
        assert_eq!(
            reopened.latest_height(),
            2,
            "[{backend}] the on-disk head is 2 (record for 3 missing)"
        );
        assert_eq!(
            reopened.get_balance(bob).expect("bob"),
            bob_after_3,
            "[{backend}] the committed state already includes block 3"
        );

        // A wrong block for the gap is refused (nothing changes)…
        let mut wrong = b3.clone();
        wrong.parent_hash = revm::primitives::B256::repeat_byte(0xAA);
        reopened.import_block(wrong);
        assert_eq!(
            reopened.latest_height(),
            2,
            "[{backend}] a block that does not chain onto the head is not slotted in"
        );

        // …the real block 3 from a peer fills the gap without re-execution.
        reopened.import_block(b3.clone());
        assert_eq!(
            reopened.latest_height(),
            3,
            "[{backend}] the missing head record is filled"
        );
        assert_eq!(reopened.block_number, 4);
        assert_eq!(
            reopened.get_balance(bob).expect("bob"),
            bob_after_3,
            "[{backend}] filling the record must not re-apply block 3"
        );
        assert_eq!(
            reopened.get_block(3).map(|b| b.hash),
            Some(b3.hash),
            "[{backend}] the record is on disk again"
        );

        // And block 4, produced by a peer that holds the identical history
        // (imported, not re-produced: block hashes carry the wall-clock
        // timestamp), applies as usual.
        let b4 = {
            let peer_dir = tempdir().expect("peer dir");
            let mut peer = Engine::new_with_backend(131_071, peer_dir.path(), backend);
            peer.fund_account(alice, U256::from(5_000_000u64), 0);
            for b in &produced {
                peer.import_block(b.clone());
            }
            assert_eq!(
                peer.latest_height(),
                3,
                "[{backend}] peer holds the same three blocks"
            );
            peer.transfer(
                alice,
                bob,
                U256::from(1_000u64),
                21_000,
                U256::from(1u64),
                3,
            )
            .expect("transfer");
            peer.execute_block().expect("block 4")
        };
        reopened.import_block(b4);
        assert_eq!(
            reopened.latest_height(),
            4,
            "[{backend}] block 4 chains onto the filled head"
        );
        assert_eq!(
            reopened.get_balance(bob).expect("bob"),
            bob_after_3 + U256::from(1_000u64),
            "[{backend}] block 4 executed once"
        );
    }
}
