//! CLOB precompile txs must emit domain events into the block.
//!
//! placeOrder / depositCollateral / cancelOrder executed via the 0x…0100
//! precompile inside `execute_block` must produce OrderSubmitted /
//! CollateralDeposited / Trade events in `block.domain_events` — that is what
//! the trade indexer, the explorer, and the WS trade feed consume.

use mersennet::engine::{Engine, Transaction};
use mersennet::events::{DomainEvent, MersennetOrdersEvent};
use mersennet::precompile_abi::{
    GAS_DEPOSIT_COLLATERAL, GAS_PLACE_ORDER, MERSENNET_ORDERS_PRECOMPILE,
    deposit_collateral_selector, encode_place_order, encode_u256,
};
use revm::primitives::{Address, Bytes, U256};
use std::sync::Mutex;
use tempfile::TempDir;

// The CLOB precompile uses a process-global context set per `execute_block`, so
// two tests in this binary running on separate threads clobber each other's
// order events. Serialize them behind one lock (recovering from poison so a
// failure in one test does not cascade into the other).
static ORDERS_CTX_LOCK: Mutex<()> = Mutex::new(());

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

#[test]
fn precompile_txs_emit_domain_events() {
    let _guard = ORDERS_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::new_with_state(1, dir.path());

    let maker = Address::from([0x21; 20]);
    let taker = Address::from([0x22; 20]);
    engine.fund_account(maker, U256::from(10_000_000u64), 0);
    engine.fund_account(taker, U256::from(10_000_000u64), 0);

    // market 1 exists (genesis-style seed)
    let market_id = engine.mersennet_orders_add_market("TST", U256::from(1), U256::from(1));

    // deposits (block 1)
    let mut dep = Vec::new();
    dep.extend_from_slice(&deposit_collateral_selector());
    dep.extend_from_slice(&encode_u256(U256::from(1_000_000u64)));
    engine
        .submit_tx_unsigned(tx(
            maker,
            0,
            21_000 + 36 * 16 + GAS_DEPOSIT_COLLATERAL * 2 + 30_000,
            Bytes::from(dep.clone()),
        ))
        .unwrap();
    engine
        .submit_tx_unsigned(tx(
            taker,
            0,
            21_000 + 36 * 16 + GAS_DEPOSIT_COLLATERAL * 2 + 30_000,
            Bytes::from(dep),
        ))
        .unwrap();
    let blk1 = engine.execute_block().unwrap();
    assert_eq!(blk1.transactions.len(), 2);
    assert!(blk1.receipts.iter().all(|r| r.success), "deposits succeed");
    let deposits = blk1
        .domain_events
        .iter()
        .filter(|e| {
            matches!(
                e,
                DomainEvent::MersennetOrders(MersennetOrdersEvent::CollateralDeposited { .. })
            )
        })
        .count();
    assert_eq!(deposits, 2, "deposit events must land in block.domain_events");

    // maker places a resting ask, taker crosses it (block 2)
    let ask = encode_place_order(market_id.0, false, U256::from(100), U256::from(5), 0);
    let bid = encode_place_order(market_id.0, true, U256::from(101), U256::from(5), 0);
    let gas = 21_000 + 164 * 16 + GAS_PLACE_ORDER * 2 + 30_000;
    engine
        .submit_tx_unsigned(tx(maker, 1, gas, Bytes::from(ask)))
        .unwrap();
    engine
        .submit_tx_unsigned(tx(taker, 1, gas, Bytes::from(bid)))
        .unwrap();
    let blk2 = engine.execute_block().unwrap();
    assert_eq!(blk2.transactions.len(), 2);
    assert!(blk2.receipts.iter().all(|r| r.success), "orders succeed");

    let submits = blk2
        .domain_events
        .iter()
        .filter(|e| {
            matches!(
                e,
                DomainEvent::MersennetOrders(MersennetOrdersEvent::OrderSubmitted { .. })
            )
        })
        .count();
    let trades = blk2
        .domain_events
        .iter()
        .filter(|e| {
            matches!(
                e,
                DomainEvent::MersennetOrders(MersennetOrdersEvent::Trade { .. })
            )
        })
        .count();
    assert_eq!(submits, 2, "both placeOrder events must be in the block");
    assert_eq!(trades, 1, "the crossing fill must emit a Trade event");
}

/// Followers receive blocks with domain_events stripped by the wire format.
/// Re-executing the txs on import must regenerate identical CLOB events so
/// the imported block serves the same fills as the producer's copy.
#[test]
fn imported_block_regenerates_precompile_events() {
    let _guard = ORDERS_CTX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let maker = Address::from([0x31; 20]);
    let taker = Address::from([0x32; 20]);

    let dir_p = TempDir::new().unwrap();
    let mut producer = Engine::new_with_state(1, dir_p.path());
    let dir_i = TempDir::new().unwrap();
    let mut importer = Engine::new_with_state(1, dir_i.path());

    for eng in [&mut producer, &mut importer] {
        eng.fund_account(maker, U256::from(10_000_000u64), 0);
        eng.fund_account(taker, U256::from(10_000_000u64), 0);
        let id = eng.mersennet_orders_add_market("TST", U256::from(1), U256::from(1));
        assert_eq!(id.0, 1);
        eng.mersennet_orders_deposit_collateral(maker, U256::from(1_000_000u64));
        eng.mersennet_orders_deposit_collateral(taker, U256::from(1_000_000u64));
    }

    let ask = encode_place_order(1, false, U256::from(100), U256::from(5), 0);
    let bid = encode_place_order(1, true, U256::from(101), U256::from(5), 0);
    let gas = 21_000 + 164 * 16 + GAS_PLACE_ORDER * 2 + 30_000;
    producer
        .submit_tx_unsigned(tx(maker, 0, gas, Bytes::from(ask)))
        .unwrap();
    producer
        .submit_tx_unsigned(tx(taker, 0, gas, Bytes::from(bid)))
        .unwrap();
    let mut block = producer.execute_block().unwrap();
    assert!(block.receipts.iter().all(|r| r.success));
    assert!(
        block
            .domain_events
            .iter()
            .any(|e| matches!(
                e,
                DomainEvent::MersennetOrders(MersennetOrdersEvent::Trade { .. })
            )),
        "producer block must carry the fill"
    );

    // Simulate the wire: domain_events are not transmitted.
    block.domain_events = Vec::new();
    let height = block.number;
    importer.import_block(block);
    assert_eq!(importer.block_number, height + 1, "import must apply");

    let imported = importer.block_by_number(height).expect("imported block");
    let trades = imported
        .domain_events
        .iter()
        .filter(|e| {
            matches!(
                e,
                DomainEvent::MersennetOrders(MersennetOrdersEvent::Trade { .. })
            )
        })
        .count();
    assert_eq!(
        trades, 1,
        "import must regenerate the Trade event from re-execution"
    );
}
