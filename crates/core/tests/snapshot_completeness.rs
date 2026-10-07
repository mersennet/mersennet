//! A state snapshot must carry everything the state root covers, or a
//! chain that has used price scales, agent grants, bad debt or the open
//! validator set cannot be restored from it ("snapshot state root
//! mismatch").

use mersennet::engine::Engine;
use mersennet::mersennet_orders::MarketId;
use revm::primitives::{Address, U256};

fn round_trip(backend: &str) {
    let dir = tempfile::tempdir().unwrap();
    let validator = Address::repeat_byte(0x44);
    let mut source = Engine::new_with_backend(131071, dir.path().join("source"), backend);
    source.fund_account(Address::repeat_byte(0x11), U256::from(5u64), 0);
    source.orders.state.price_scales.insert(MarketId(1), 100);
    source.orders.state.bad_debt = U256::from(7u64);
    source
        .orders
        .state
        .staking
        .seed_genesis(&[(validator, U256::from(1_000_000u64))], 1);
    let path = dir.path().join("snapshot.bin");
    source.export_state_snapshot(&path).unwrap();

    let mut restored = Engine::new_with_backend(131071, dir.path().join("restored"), backend);
    restored
        .import_state_snapshot(&path)
        .unwrap_or_else(|e| panic!("{backend}: {e:#}"));
    let state = &restored.orders.state;
    assert_eq!(
        state.price_scales.get(&MarketId(1)),
        Some(&100),
        "{backend}"
    );
    assert_eq!(state.bad_debt, U256::from(7u64), "{backend}");
    assert!(state.staking.registry.contains_key(&validator), "{backend}");
}

#[test]
fn a_sled_snapshot_carries_everything_the_state_root_covers() {
    round_trip("sled");
}

#[test]
fn a_redb_snapshot_carries_everything_the_state_root_covers() {
    round_trip("redb");
}
