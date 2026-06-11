use mersennet::engine::Engine;
use mersennet::errors::MersennetOrdersError;
use mersennet::mersennet_orders::{Side, TimeInForce};
use revm::primitives::{Address, U256};
use tempfile::TempDir;

fn addr(byte: u8) -> Address {
    Address::from_slice(&[byte; 20])
}

fn setup_engine() -> (Engine, TempDir) {
    let dir = TempDir::new().expect("temp dir");
    let engine = Engine::new_with_state(1, dir.path());
    (engine, dir)
}

#[test]
fn insurance_fund_collects_fees_on_trades() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    engine.orders.state.insurance_contribution_rate_bps = 10; // 0.1%

    let maker = addr(0x11);
    let taker = addr(0x22);

    engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(10u64),
            TimeInForce::Gtc,
        )
        .unwrap();
    engine
        .mersennet_orders_submit_order(
            taker,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(10u64),
            TimeInForce::Ioc,
        )
        .unwrap();

    let fund = engine.orders.state.insurance_fund_balance();
    assert!(
        fund > U256::ZERO,
        "insurance fund should collect fees from trades, got {fund}"
    );
}

#[test]
fn insurance_fund_covers_liquidation_deficit() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    engine.mersennet_orders_set_margin_params(100, 0); // 1% initial
    engine.orders.state.insurance_contribution_rate_bps = 0;

    let trader = addr(0x33);
    let maker = addr(0x44);
    engine.mersennet_orders_deposit_collateral(trader, U256::from(10u64));
    engine.mersennet_orders_deposit_collateral(maker, U256::from(10u64));

    engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(2u64),
            TimeInForce::Gtc,
        )
        .unwrap();
    engine
        .mersennet_orders_submit_order(
            trader,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(2u64),
            TimeInForce::Ioc,
        )
        .unwrap();

    engine
        .orders
        .state
        .contribute_to_insurance(U256::from(500u64));
    let fund_before = engine.orders.state.insurance_fund_balance();

    engine.mersennet_orders_set_margin_params(100, 9_000);
    assert!(engine.mersennet_orders_is_liquidatable(trader));
    engine.mersennet_orders_liquidate(trader);

    let fund_after = engine.orders.state.insurance_fund_balance();
    assert!(
        fund_after <= fund_before,
        "insurance fund should be drawn down to cover deficit"
    );
}

#[test]
fn match_time_margin_rejects_when_insufficient() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    engine.mersennet_orders_set_margin_params(5_000, 2_500); // 50% initial, 25% maintenance

    let trader = addr(0x55);
    engine.mersennet_orders_deposit_collateral(trader, U256::from(1u64));

    let maker = addr(0x66);
    engine.mersennet_orders_deposit_collateral(maker, U256::from(100_000u64));
    engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(100u64),
            TimeInForce::Gtc,
        )
        .unwrap();

    let result = engine.mersennet_orders_submit_order(
        trader,
        market_id,
        Side::Buy,
        U256::from(100u64),
        U256::from(100u64),
        TimeInForce::Ioc,
    );
    assert!(
        result.is_err(),
        "order should be rejected due to insufficient initial margin"
    );
}

#[test]
fn vwap_entry_price_adding_to_position() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    let buyer = addr(0x77);
    let seller = addr(0x88);

    engine
        .mersennet_orders_submit_order(
            seller,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(10u64),
            TimeInForce::Gtc,
        )
        .unwrap();
    engine
        .mersennet_orders_submit_order(
            buyer,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(10u64),
            TimeInForce::Ioc,
        )
        .unwrap();

    engine
        .mersennet_orders_submit_order(
            seller,
            market_id,
            Side::Sell,
            U256::from(200u64),
            U256::from(10u64),
            TimeInForce::Gtc,
        )
        .unwrap();
    engine
        .mersennet_orders_submit_order(
            buyer,
            market_id,
            Side::Buy,
            U256::from(200u64),
            U256::from(10u64),
            TimeInForce::Ioc,
        )
        .unwrap();

    let positions = engine.orders.state.positions(buyer);
    let (_, pos) = positions
        .iter()
        .find(|(mid, _)| *mid == market_id)
        .expect("position exists");

    assert_eq!(pos.size, 20, "total position size");
    assert_eq!(
        pos.entry_price,
        U256::from(150u64),
        "VWAP of 10@100 + 10@200 = 150"
    );
}

#[test]
fn vwap_reducing_position_realizes_pnl() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    let buyer = addr(0x99);
    let seller = addr(0xAA);

    engine
        .mersennet_orders_submit_order(
            seller,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(10u64),
            TimeInForce::Gtc,
        )
        .unwrap();
    engine
        .mersennet_orders_submit_order(
            buyer,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(10u64),
            TimeInForce::Ioc,
        )
        .unwrap();

    let buyer2 = addr(0xBB);
    engine
        .mersennet_orders_submit_order(
            buyer2,
            market_id,
            Side::Buy,
            U256::from(150u64),
            U256::from(5u64),
            TimeInForce::Gtc,
        )
        .unwrap();
    engine
        .mersennet_orders_submit_order(
            buyer,
            market_id,
            Side::Sell,
            U256::from(150u64),
            U256::from(5u64),
            TimeInForce::Ioc,
        )
        .unwrap();

    let positions = engine.orders.state.positions(buyer);
    let (_, pos) = positions
        .iter()
        .find(|(mid, _)| *mid == market_id)
        .expect("position exists");

    assert_eq!(pos.size, 5, "remaining long size after partial close");
    assert_eq!(
        pos.entry_price,
        U256::from(100u64),
        "entry price unchanged on reduction"
    );
    assert!(
        pos.realized_pnl > 0,
        "closing at 150 with entry 100 should realize profit, got {}",
        pos.realized_pnl
    );
}

#[test]
fn market_halt_rejects_orders() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    engine.orders.state.halt_market(market_id);

    let trader = addr(0xCC);
    let result = engine.mersennet_orders_submit_order(
        trader,
        market_id,
        Side::Buy,
        U256::from(100u64),
        U256::from(1u64),
        TimeInForce::Gtc,
    );
    assert!(matches!(result, Err(MersennetOrdersError::MarketHalted)));
}

#[test]
fn collateral_withdrawal_rejected_when_equity_insufficient() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    engine.mersennet_orders_set_margin_params(100, 500); // 1% initial, 5% maintenance

    let trader = addr(0xDD);
    let maker = addr(0xEE);
    engine.mersennet_orders_deposit_collateral(trader, U256::from(100u64));
    engine.mersennet_orders_deposit_collateral(maker, U256::from(100u64));

    engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(10u64),
            TimeInForce::Gtc,
        )
        .unwrap();
    engine
        .mersennet_orders_submit_order(
            trader,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(10u64),
            TimeInForce::Ioc,
        )
        .unwrap();

    let result = engine
        .orders
        .state
        .withdraw_collateral(trader, U256::from(100u64));
    assert!(
        result.is_err(),
        "withdrawing all collateral with open position should fail"
    );
}

#[test]
fn fok_all_or_nothing() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    let maker = addr(0x11);
    engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(3u64),
            TimeInForce::Gtc,
        )
        .unwrap();

    let taker = addr(0x22);
    let result = engine.mersennet_orders_submit_order(
        taker,
        market_id,
        Side::Buy,
        U256::from(100u64),
        U256::from(5u64),
        TimeInForce::Fok,
    );
    assert!(
        result.is_err(),
        "FOK should fail when full fill not available"
    );

    let book = engine.mersennet_orders_order_book(market_id).unwrap();
    assert_eq!(
        book.asks[0].size,
        U256::from(3u64),
        "maker order untouched after FOK reject"
    );

    let result = engine.mersennet_orders_submit_order(
        taker,
        market_id,
        Side::Buy,
        U256::from(100u64),
        U256::from(3u64),
        TimeInForce::Fok,
    );
    assert!(
        result.is_ok(),
        "FOK should succeed when exact liquidity available"
    );
    assert_eq!(result.unwrap().filled, U256::from(3u64));
}

#[test]
fn ioc_partial_fill_remainder_cancelled() {
    let (mut engine, _dir) = setup_engine();
    let market_id =
        engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));

    let maker = addr(0x33);
    engine
        .mersennet_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(3u64),
            TimeInForce::Gtc,
        )
        .unwrap();

    let taker = addr(0x44);
    let outcome = engine
        .mersennet_orders_submit_order(
            taker,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Ioc,
        )
        .unwrap();

    assert_eq!(outcome.filled, U256::from(3u64));
    assert_eq!(outcome.remaining, U256::from(2u64));
    assert!(
        outcome.order_id.is_none(),
        "IOC remainder should not be resting on the book"
    );

    let book = engine.mersennet_orders_order_book(market_id).unwrap();
    assert!(book.bids.is_empty(), "no taker orders on the book");
    assert!(book.asks.is_empty(), "maker fully consumed");
}
