use mersennet::bridge::{BridgeDomain, BridgeQueue};
use mersennet::mersennet_orders::MersennetOrdersState;
use mersennet::state::PersistentState;
use mersennet::state_redb::RedbState;
use mersennet::state_trait::StateBackend;
use revm::Database;
use revm::db::InMemoryDB;
use revm::primitives::Bytes;
use revm::primitives::{AccountInfo, Address, B256, Bytecode, KECCAK_EMPTY, U256};
use tempfile::tempdir;

#[test]
fn persistent_state_roundtrip() {
    let dir = tempdir().expect("temp dir");
    let state = PersistentState::open(dir.path()).expect("open state");

    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x11; 20]);
    let info = AccountInfo::new(U256::from(1234u64), 7, KECCAK_EMPTY, Bytecode::new());
    db.insert_account_info(address, info);
    let slot = U256::from(9u64);
    let value = U256::from(4242u64);
    db.insert_account_storage(address, slot, value)
        .expect("storage");

    let root_before = state
        .commit_state(
            &db,
            &MersennetOrdersState::new(),
            &BridgeQueue::new(),
            &BridgeQueue::new(),
            1,
        )
        .expect("commit");

    let mut db2 = InMemoryDB::default();
    state.load_into_db(&mut db2).expect("load");
    let info2 = db2.basic(address).expect("basic").expect("account");
    assert_eq!(info2.balance, U256::from(1234u64));
    assert_eq!(info2.nonce, 7);
    let stored = db2.storage(address, slot).expect("storage");
    assert_eq!(stored, value);

    let root_after = state.compute_state_root();
    assert_eq!(root_before, root_after);
    assert_ne!(root_after, B256::ZERO);
}

#[test]
fn mersennet_orders_and_bridge_persistence() {
    let dir = tempdir().expect("temp dir");
    let state = PersistentState::open(dir.path()).expect("open state");

    let db = InMemoryDB::default();

    let mut mersennet_orders = MersennetOrdersState::new();
    mersennet_orders.set_margin_params(0, 0);
    let market_id = mersennet_orders.add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));
    let owner = Address::from_slice(&[0x22; 20]);
    let _order_id = mersennet_orders.place_order(
        owner,
        market_id,
        mersennet::mersennet_orders::Side::Buy,
        U256::from(100u64),
        U256::from(2u64),
        mersennet::mersennet_orders::TimeInForce::Gtc,
    );

    let mut orders_to_evm = BridgeQueue::new();
    let mut evm_to_orders = BridgeQueue::new();
    let payload = Bytes::from(vec![1, 2, 3, 4]);
    let msg = orders_to_evm.push(
        BridgeDomain::MersennetOrders,
        BridgeDomain::MersennetEvm,
        payload.clone(),
    );
    let _ = evm_to_orders.push(
        BridgeDomain::MersennetEvm,
        BridgeDomain::MersennetOrders,
        Bytes::from(vec![9]),
    );

    let root_before = state
        .commit_state(&db, &mersennet_orders, &orders_to_evm, &evm_to_orders, 1)
        .expect("commit");

    let mut loaded_orders = MersennetOrdersState::new();
    state
        .load_mersennet_orders(&mut loaded_orders)
        .expect("load mersennet orders");
    let mut loaded_orders_to_evm = BridgeQueue::new();
    let mut loaded_evm_to_orders = BridgeQueue::new();
    state
        .load_bridge_queues(&mut loaded_orders_to_evm, &mut loaded_evm_to_orders)
        .expect("load bridge queues");

    assert_eq!(loaded_orders.markets.len(), 1);
    assert_eq!(loaded_orders.orders.len(), 1);
    assert_eq!(loaded_orders.books.len(), 1);

    let loaded_msg = loaded_orders_to_evm.pop().expect("bridge message");
    assert_eq!(loaded_msg.nonce, msg.nonce);
    assert_eq!(loaded_msg.payload.as_ref(), payload.as_ref());

    let root_after = state.compute_state_root();
    assert_eq!(root_before, root_after);
    assert_ne!(root_after, B256::ZERO);
}

#[test]
fn staking_and_collateral_and_order_flags_persist() {
    use mersennet::mersennet_orders::{CollateralAsset, Side, TimeInForce};

    let dir = tempdir().expect("temp dir");
    let state = PersistentState::open(dir.path()).expect("open state");
    let db = InMemoryDB::default();

    let mut orders = MersennetOrdersState::new();
    orders.set_margin_params(0, 0);
    let m = orders.add_market("MRSN/USD", U256::from(1u64), U256::from(1u64));

    // A GTD + post-only-shaped resting order.
    orders.place_order_ext(
        Address::from_slice(&[0x22; 20]),
        m,
        Side::Buy,
        U256::from(90u64),
        U256::from(3u64),
        TimeInForce::Gtc,
        true,
        4242,
    );

    // Delegated staking.
    let val = Address::from_slice(&[0x01; 20]);
    orders.staking.ensure_pool(val);
    orders
        .staking
        .delegate(Address::from_slice(&[0x02; 20]), val, U256::from(500u64))
        .unwrap();
    orders.staking.on_reward(val, U256::from(100u64), U256::from(500u64));

    // A registered collateral asset + balance.
    let token = Address::from_slice(&[0xAA; 20]);
    orders.register_collateral_asset(
        token,
        CollateralAsset {
            weight_bps: 8_000,
            value_num: U256::from(1u64),
            value_den: U256::from(1u64),
            balances_slot: U256::from(3u64),
        },
    );
    orders
        .deposit_token_collateral(Address::from_slice(&[0x02; 20]), token, U256::from(1_000u64))
        .unwrap();

    state
        .commit_state(&db, &orders, &BridgeQueue::new(), &BridgeQueue::new(), 1)
        .expect("commit");

    // Assert both backends round-trip the new fields identically.
    let dir2 = tempdir().expect("temp dir");
    let redb = RedbState::open(dir2.path()).expect("open redb");
    redb.commit_state(&db, &orders, &BridgeQueue::new(), &BridgeQueue::new(), 1)
        .expect("redb commit");

    for (label, loaded) in [
        ("sled", {
            let mut s = MersennetOrdersState::new();
            state.load_mersennet_orders(&mut s).expect("sled load");
            s
        }),
        ("redb", {
            let mut s = MersennetOrdersState::new();
            redb.load_mersennet_orders(&mut s).expect("redb load");
            s
        }),
    ] {
        let ord = loaded
            .orders
            .values()
            .next()
            .unwrap_or_else(|| panic!("{label}: order persisted"));
        assert!(ord.post_only, "{label}: post_only");
        assert_eq!(ord.expire_at, 4242, "{label}: expire_at");
        assert_eq!(
            loaded.staking.delegated_total(val),
            U256::from(500u64),
            "{label}: delegated total"
        );
        assert_eq!(
            loaded
                .staking
                .pending_rewards(Address::from_slice(&[0x02; 20]), val),
            U256::from(45u64),
            "{label}: pending rewards"
        );
        assert_eq!(loaded.collateral_assets.len(), 1, "{label}: assets");
        assert_eq!(
            loaded.token_margin_value(Address::from_slice(&[0x02; 20])),
            U256::from(800u64),
            "{label}: token margin value"
        );
    }
}

#[test]
fn bridge_queue_persistence_roundtrip() {
    let dir = tempdir().expect("temp dir");
    let state = PersistentState::open(dir.path()).expect("open state");

    let db = InMemoryDB::default();
    let empty_orders = MersennetOrdersState::new();

    let mut orders_to_evm = BridgeQueue::new();
    let mut evm_to_orders = BridgeQueue::new();
    let payload_a = Bytes::from(vec![10, 11, 12]);
    let payload_b = Bytes::from(vec![200, 201]);
    let msg_a = orders_to_evm.push(
        BridgeDomain::MersennetOrders,
        BridgeDomain::MersennetEvm,
        payload_a.clone(),
    );
    let msg_b = evm_to_orders.push(
        BridgeDomain::MersennetEvm,
        BridgeDomain::MersennetOrders,
        payload_b.clone(),
    );

    state
        .commit_state(&db, &empty_orders, &orders_to_evm, &evm_to_orders, 1)
        .expect("commit");

    let mut loaded_orders_to_evm = BridgeQueue::new();
    let mut loaded_evm_to_orders = BridgeQueue::new();
    state
        .load_bridge_queues(&mut loaded_orders_to_evm, &mut loaded_evm_to_orders)
        .expect("load bridge queues");

    let loaded_a = loaded_orders_to_evm.pop().expect("bridge msg a");
    let loaded_b = loaded_evm_to_orders.pop().expect("bridge msg b");

    assert_eq!(loaded_a.nonce, msg_a.nonce);
    assert_eq!(loaded_a.payload.as_ref(), payload_a.as_ref());
    assert_eq!(loaded_b.nonce, msg_b.nonce);
    assert_eq!(loaded_b.payload.as_ref(), payload_b.as_ref());
}

// ==================== ReDB Backend Tests ====================

#[test]
fn redb_state_roundtrip() {
    let dir = tempdir().expect("temp dir");
    let state = RedbState::open(dir.path()).expect("open redb state");

    let mut db = InMemoryDB::default();
    let address = Address::from_slice(&[0x11; 20]);
    let info = AccountInfo::new(U256::from(1234u64), 7, KECCAK_EMPTY, Bytecode::new());
    db.insert_account_info(address, info);
    let slot = U256::from(9u64);
    let value = U256::from(4242u64);
    db.insert_account_storage(address, slot, value)
        .expect("storage");

    let root_before = state
        .commit_state(
            &db,
            &MersennetOrdersState::new(),
            &BridgeQueue::new(),
            &BridgeQueue::new(),
            1,
        )
        .expect("commit");

    let mut db2 = InMemoryDB::default();
    state.load_into_db(&mut db2).expect("load");
    let info2 = db2.basic(address).expect("basic").expect("account");
    assert_eq!(info2.balance, U256::from(1234u64));
    assert_eq!(info2.nonce, 7);
    let stored = db2.storage(address, slot).expect("storage");
    assert_eq!(stored, value);

    let root_after = state.compute_state_root();
    assert_eq!(root_before, root_after);
    assert_ne!(root_after, B256::ZERO);
}

#[test]
fn redb_mersennet_orders_persistence() {
    let dir = tempdir().expect("temp dir");
    let state = RedbState::open(dir.path()).expect("open redb state");

    let db = InMemoryDB::default();

    let mut mersennet_orders = MersennetOrdersState::new();
    mersennet_orders.set_margin_params(0, 0);
    let market_id = mersennet_orders.add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));
    let owner = Address::from_slice(&[0x22; 20]);
    let _order_id = mersennet_orders.place_order(
        owner,
        market_id,
        mersennet::mersennet_orders::Side::Buy,
        U256::from(100u64),
        U256::from(2u64),
        mersennet::mersennet_orders::TimeInForce::Gtc,
    );

    let mut orders_to_evm = BridgeQueue::new();
    let mut evm_to_orders = BridgeQueue::new();
    let payload = Bytes::from(vec![1, 2, 3, 4]);
    let msg = orders_to_evm.push(
        BridgeDomain::MersennetOrders,
        BridgeDomain::MersennetEvm,
        payload.clone(),
    );
    let _ = evm_to_orders.push(
        BridgeDomain::MersennetEvm,
        BridgeDomain::MersennetOrders,
        Bytes::from(vec![9]),
    );

    let root_before = state
        .commit_state(&db, &mersennet_orders, &orders_to_evm, &evm_to_orders, 1)
        .expect("commit");

    let mut loaded_orders = MersennetOrdersState::new();
    state
        .load_mersennet_orders(&mut loaded_orders)
        .expect("load mersennet orders");
    let mut loaded_orders_to_evm = BridgeQueue::new();
    let mut loaded_evm_to_orders = BridgeQueue::new();
    state
        .load_bridge_queues(&mut loaded_orders_to_evm, &mut loaded_evm_to_orders)
        .expect("load bridge queues");

    assert_eq!(loaded_orders.markets.len(), 1);
    assert_eq!(loaded_orders.orders.len(), 1);
    assert_eq!(loaded_orders.books.len(), 1);

    let loaded_msg = loaded_orders_to_evm.pop().expect("bridge message");
    assert_eq!(loaded_msg.nonce, msg.nonce);
    assert_eq!(loaded_msg.payload.as_ref(), payload.as_ref());

    let root_after = state.compute_state_root();
    assert_eq!(root_before, root_after);
    assert_ne!(root_after, B256::ZERO);
}

#[test]
fn redb_block_storage_via_engine() {
    use mersennet::engine::Engine;
    let dir = tempdir().expect("temp dir");
    let mut engine = Engine::new_with_backend(131071, dir.path(), "redb");

    let alice = Address::from_slice(&[0xCC; 20]);
    engine.fund_account(alice, U256::from(10_000_000u64), 0);
    engine
        .add_validator(alice, U256::from(100u64))
        .expect("add validator");

    let block = engine.execute_block().expect("execute empty block");
    assert_eq!(block.number, 1);

    let loaded = engine
        .evm
        .state
        .load_block(1)
        .expect("load")
        .expect("block exists");
    assert_eq!(loaded.number, 1);
    assert_eq!(loaded.chain_id, 131071);
}

#[test]
fn redb_engine_integration() {
    use mersennet::engine::Engine;
    let dir = tempdir().expect("temp dir");
    let mut engine = Engine::new_with_backend(131071, dir.path(), "redb");

    let alice = Address::from_slice(&[0xAA; 20]);
    let bob = Address::from_slice(&[0xBB; 20]);
    engine.fund_account(alice, U256::from(10_000_000u64), 0);
    engine
        .add_validator(alice, U256::from(100u64))
        .expect("add validator");

    engine
        .transfer(alice, bob, U256::from(1000u64), 21000, U256::from(1u64), 0)
        .expect("transfer");
    let block = engine.execute_block().expect("execute block");

    assert_eq!(block.number, 1);
    assert_eq!(block.transactions.len(), 1);
    assert!(block.receipts[0].success);
    assert_ne!(block.state_root, B256::ZERO);

    let bob_balance = engine.get_balance(bob).expect("balance");
    assert_eq!(bob_balance, U256::from(1000u64));
}
