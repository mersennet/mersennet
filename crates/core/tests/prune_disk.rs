//! Does pruning give disk space back? sled keeps a value that outgrows its
//! page inline limit in a blob file (on the public node: 388,625 files, 102 GB
//! of block history in `state/blobs`, ~12 blocks per file). This stores a few
//! thousand blocks of the live size (~22 KB as stored), prunes all but the
//! last few hundred and measures the directory before, right after, and after
//! more writes (sled reclaims segments as it writes).
//!
//! Slow and about disk sizes, so not in the default run:
//! `cargo test -p mersennet --test prune_disk -- --ignored --nocapture`

use mersennet::engine::{Block, Engine, Transaction};
use mersennet::state::PersistentState;
use revm::primitives::{Address, B256, Bytes, U256};

fn dir_bytes(path: &std::path::Path) -> u64 {
    let mut total = 0;
    for entry in std::fs::read_dir(path).unwrap().flatten() {
        let md = entry.metadata().unwrap();
        total += if md.is_dir() {
            dir_bytes(&entry.path())
        } else {
            md.len()
        };
    }
    total
}

fn blob_files(path: &std::path::Path) -> usize {
    std::fs::read_dir(path.join("blobs"))
        .map(|d| d.count())
        .unwrap_or(0)
}

/// One produced block with ~40 transactions, so it serializes to ~22 KB.
fn template() -> Block {
    let dir = tempfile::tempdir().unwrap();
    let mut e = Engine::new_with_state(131_071, dir.path());
    let v = Address::from_slice(&[0x44; 20]);
    e.add_validator(v, U256::from(1_000_000u64)).unwrap();
    e.set_local_validator(v);
    let from = Address::from_slice(&[0x11; 20]);
    e.fund_account(from, U256::from(10u64).pow(U256::from(24u64)), 0);
    for nonce in 0..40u64 {
        e.submit_tx_unsigned(Transaction {
            from,
            to: Some(Address::from_slice(&[0x22; 20])),
            value: U256::from(1u64),
            data: Bytes::from(vec![0xab; 128]),
            gas_limit: 60_000,
            gas_price: U256::from(1u64),
            nonce,
            chain_id: Some(131_071),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
            raw: None,
        })
        .unwrap();
    }
    e.execute_block().unwrap()
}

#[test]
#[ignore]
fn pruning_returns_blob_space_to_the_disk() {
    let base = template();
    let size = serde_json::to_vec(&base).unwrap().len();
    println!(
        "block as stored: {size} bytes, {} txs",
        base.transactions.len()
    );

    let dir = tempfile::tempdir().unwrap();
    let st = PersistentState::open_with_cache(dir.path(), 64 * 1024 * 1024).unwrap();
    let total: u64 = 6_000;
    let keep: u64 = 300;
    let store = |from: u64, to: u64| {
        for n in from..to {
            let mut b = base.clone();
            b.number = n;
            b.hash = B256::from(U256::from(n + 1));
            st.store_block(&b).unwrap();
        }
        st.flush().unwrap();
    };
    store(0, total);
    let before = dir_bytes(dir.path());
    println!(
        "stored {total}: {:.1} MB, {} blob files",
        before as f64 / 1e6,
        blob_files(dir.path())
    );

    let mut floor = 0;
    while floor < total - keep {
        let to = (floor + 256).min(total - keep);
        st.prune_blocks(floor, to).unwrap();
        floor = to;
    }
    st.flush().unwrap();
    let after = dir_bytes(dir.path());
    println!(
        "pruned to {floor}: {:.1} MB, {} blob files",
        after as f64 / 1e6,
        blob_files(dir.path())
    );
    assert_eq!(st.load_block(floor - 1).unwrap().map(|b| b.number), None);
    assert_eq!(st.load_block(floor).unwrap().map(|b| b.number), Some(floor));

    // sled frees segments as new writes come in: a few hundred more blocks.
    store(total, total + 600);
    let later = dir_bytes(dir.path());
    println!(
        "after 600 more: {:.1} MB, {} blob files",
        later as f64 / 1e6,
        blob_files(dir.path())
    );
    drop(st);
    let st = PersistentState::open_with_cache(dir.path(), 64 * 1024 * 1024).unwrap();
    st.flush().unwrap();
    let reopened = dir_bytes(dir.path());
    println!(
        "after reopen: {:.1} MB, {} blob files",
        reopened as f64 / 1e6,
        blob_files(dir.path())
    );
    assert!(
        reopened < before / 2,
        "pruning {} of {} blocks must free most of the space ({before} -> {reopened} bytes)",
        total - keep,
        total
    );
}
