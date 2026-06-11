use mersennet::engine::Engine;
use mersennet::events::{DomainEvent, MersennetOrdersEvent};
use mersennet::mersennet_orders::{Side, TimeInForce};
use revm::primitives::{Address, U256};
use tempfile::TempDir;

#[test]
fn mersennet_orders_limit_matching_and_book() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut engine = Engine::new_with_state(1, temp_dir.path());
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    let maker = Address::from_slice(&[0x11; 20]);
    let taker = Address::from_slice(&[0x22; 20]);

    let outcome = engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Gtc,
        )
        .expect("maker order accepted");
    assert!(outcome.trades.is_empty());
    assert_eq!(outcome.remaining, U256::from(5u64));

    let outcome = engine
        .mersennet_orders_submit_order(
            taker,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(3u64),
            TimeInForce::Gtc,
        )
        .expect("taker order accepted");
    assert_eq!(outcome.filled, U256::from(3u64));
    assert_eq!(outcome.trades.len(), 1);

    let book = engine
        .mersennet_orders_order_book(market_id)
        .expect("order book exists");
    assert!(book.bids.is_empty());
    assert_eq!(book.asks.len(), 1);
    assert_eq!(book.asks[0].price, U256::from(100u64));
    assert_eq!(book.asks[0].size, U256::from(2u64));

    // IOC: partial fill, remainder discarded
    let outcome = engine
        .mersennet_orders_submit_order(
            taker,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Ioc,
        )
        .expect("ioc order accepted");
    assert_eq!(outcome.filled, U256::from(2u64));
    assert_eq!(outcome.remaining, U256::from(3u64));

    let book = engine
        .mersennet_orders_order_book(market_id)
        .expect("order book exists");
    assert!(book.asks.is_empty());

    // FOK: fails if not fully fillable
    let outcome = engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(101u64),
            U256::from(1u64),
            TimeInForce::Gtc,
        )
        .expect("maker order accepted");
    assert_eq!(outcome.remaining, U256::from(1u64));

    let fok = engine.mersennet_orders_submit_order(
        taker,
        market_id,
        Side::Buy,
        U256::from(100u64),
        U256::from(2u64),
        TimeInForce::Fok,
    );
    assert!(fok.is_err());
}

#[test]
fn mersennet_orders_margin_enforced_and_liquidation() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut engine = Engine::new_with_state(1, temp_dir.path());
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    let trader = Address::from_slice(&[0x33; 20]);

    engine.mersennet_orders_set_margin_params(100, 0); // 1% initial, maintenance disabled

    engine.mersennet_orders_deposit_collateral(trader, U256::from(10u64));
    let maker = Address::from_slice(&[0x44; 20]);
    engine.mersennet_orders_deposit_collateral(maker, U256::from(10u64));

    let _ = engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(2u64),
            TimeInForce::Gtc,
        )
        .expect("maker order accepted");

    let _ = engine
        .mersennet_orders_submit_order(
            trader,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(2u64),
            TimeInForce::Ioc,
        )
        .expect("taker order accepted");

    assert!(!engine.mersennet_orders_is_liquidatable(trader));

    // Tighten maintenance margin after position exists
    engine.mersennet_orders_set_margin_params(100, 9_000);
    assert!(engine.mersennet_orders_is_liquidatable(trader));
    assert!(engine.mersennet_orders_liquidate(trader));
}

#[test]
fn mersennet_orders_domain_events_embedded_in_block() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut engine = Engine::new_with_state(1, temp_dir.path());
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    let trader = Address::from_slice(&[0x55; 20]);
    let _ = engine
        .mersennet_orders_submit_order(
            trader,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(1u64),
            TimeInForce::Gtc,
        )
        .expect("order accepted");

    let block = engine.execute_block().expect("block executed");
    assert!(!block.domain_events.is_empty());

    let has_market = block.domain_events.iter().any(|event| matches!(
        event,
        DomainEvent::MersennetOrders(MersennetOrdersEvent::MarketAdded { market_id: id, .. }) if *id == market_id
    ));
    assert!(has_market, "market added event missing");

    let has_submit = block.domain_events.iter().any(|event| matches!(
        event,
        DomainEvent::MersennetOrders(MersennetOrdersEvent::OrderSubmitted { owner, .. }) if *owner == trader
    ));
    assert!(has_submit, "order submitted event missing");
}

#[test]
fn mersennet_orders_deterministic_matching_across_nodes() {
    let temp_a = TempDir::new().expect("temp dir");
    let temp_b = TempDir::new().expect("temp dir");
    let mut engine_a = Engine::new_with_state(1, temp_a.path());
    let mut engine_b = Engine::new_with_state(1, temp_b.path());

    let market_a =
        engine_a.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));
    let market_b =
        engine_b.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));
    assert_eq!(market_a.0, market_b.0);

    let maker = Address::from_slice(&[0x66; 20]);
    let taker = Address::from_slice(&[0x77; 20]);

    let orders = vec![
        (
            maker,
            Side::Sell,
            U256::from(101u64),
            U256::from(2u64),
            TimeInForce::Gtc,
        ),
        (
            maker,
            Side::Sell,
            U256::from(100u64),
            U256::from(1u64),
            TimeInForce::Gtc,
        ),
        (
            taker,
            Side::Buy,
            U256::from(101u64),
            U256::from(2u64),
            TimeInForce::Gtc,
        ),
        (
            taker,
            Side::Buy,
            U256::from(99u64),
            U256::from(1u64),
            TimeInForce::Ioc,
        ),
    ];

    let mut outcomes_a = Vec::new();
    let mut outcomes_b = Vec::new();
    for (owner, side, price, size, tif) in orders {
        let out_a = engine_a
            .mersennet_orders_submit_order(owner, market_a, side, price, size, tif)
            .expect("order accepted");
        let out_b = engine_b
            .mersennet_orders_submit_order(owner, market_b, side, price, size, tif)
            .expect("order accepted");
        assert_eq!(out_a.filled, out_b.filled);
        assert_eq!(out_a.remaining, out_b.remaining);
        assert_eq!(out_a.trades.len(), out_b.trades.len());
        outcomes_a.push(out_a);
        outcomes_b.push(out_b);
    }

    let book_a = engine_a
        .mersennet_orders_order_book(market_a)
        .expect("book A");
    let book_b = engine_b
        .mersennet_orders_order_book(market_b)
        .expect("book B");
    assert_eq!(book_a.bids.len(), book_b.bids.len());
    assert_eq!(book_a.asks.len(), book_b.asks.len());
    for (a, b) in book_a.bids.iter().zip(book_b.bids.iter()) {
        assert_eq!(a.price, b.price);
        assert_eq!(a.size, b.size);
    }
    for (a, b) in book_a.asks.iter().zip(book_b.asks.iter()) {
        assert_eq!(a.price, b.price);
        assert_eq!(a.size, b.size);
    }
}

#[test]
fn mersennet_orders_sensitive_domain_events_suppressed_after_privacy_activation() {
    let temp_dir = TempDir::new().expect("temp dir");
    let mut engine = Engine::new_with_state(1, temp_dir.path());
    engine.activate_privacy_mode();

    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));
    let trader = Address::from_slice(&[0x88; 20]);

    engine.mersennet_orders_deposit_collateral(trader, U256::from(10u64));
    let _ = engine
        .mersennet_orders_submit_order(
            trader,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(1u64),
            TimeInForce::Gtc,
        )
        .expect("order accepted");

    let block = engine.execute_block().expect("block executed");

    assert!(block.domain_events.iter().any(|event| matches!(
        event,
        DomainEvent::MersennetOrders(MersennetOrdersEvent::MarketAdded { market_id: id, .. }) if *id == market_id
    )));

    assert!(block.domain_events.iter().all(|event| !matches!(
        event,
        DomainEvent::MersennetOrders(
            MersennetOrdersEvent::OrderSubmitted { .. }
                | MersennetOrdersEvent::OrderCancelled { .. }
                | MersennetOrdersEvent::Trade { .. }
                | MersennetOrdersEvent::CollateralDeposited { .. }
                | MersennetOrdersEvent::Liquidation { .. }
        )
    )));
}
