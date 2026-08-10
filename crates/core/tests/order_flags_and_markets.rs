//! Tests for post-only, good-till-date expiry, permissionless market
//! creation, and multi-collateral margin added on top of the CLOB.

use mersennet::engine::Engine;
use mersennet::errors::MersennetOrdersError;
use mersennet::mersennet_orders::{CollateralAsset, Side, TimeInForce};
use revm::primitives::{Address, U256};
use tempfile::TempDir;

fn addr(byte: u8) -> Address {
    Address::from_slice(&[byte; 20])
}

fn setup() -> (Engine, TempDir) {
    let dir = TempDir::new().expect("temp dir");
    let engine = Engine::new_with_state(1, dir.path());
    (engine, dir)
}

#[test]
fn post_only_rests_when_it_does_not_cross() {
    let (mut engine, _dir) = setup();
    let m = engine.mersennet_orders_add_market("MRSN/USD", U256::from(1u64), U256::from(1u64));
    let state = &mut engine.orders.state;

    // Best ask at 105. A post-only bid at 100 does not cross → rests.
    state.place_order(addr(1), m, Side::Sell, U256::from(105u64), U256::from(5u64), TimeInForce::Gtc);
    let out = state
        .submit_order_ext(
            addr(2),
            m,
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Gtc,
            true,
            0,
        )
        .expect("post-only should rest");
    assert!(out.order_id.is_some());
    assert_eq!(out.filled, U256::ZERO);
    assert_eq!(out.remaining, U256::from(5u64));
}

#[test]
fn post_only_rejected_when_it_would_cross() {
    let (mut engine, _dir) = setup();
    let m = engine.mersennet_orders_add_market("MRSN/USD", U256::from(1u64), U256::from(1u64));
    let state = &mut engine.orders.state;

    // Best ask at 100. A post-only bid at 100 crosses → reject, no fill.
    state.place_order(addr(1), m, Side::Sell, U256::from(100u64), U256::from(5u64), TimeInForce::Gtc);
    let err = state
        .submit_order_ext(
            addr(2),
            m,
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Gtc,
            true,
            0,
        )
        .unwrap_err();
    assert!(matches!(err, MersennetOrdersError::PostOnlyWouldCross));
    // The crossing ask is untouched.
    let book = state.order_book(m).unwrap();
    assert_eq!(book.asks.len(), 1);
}

#[test]
fn post_only_requires_gtc() {
    let (mut engine, _dir) = setup();
    let m = engine.mersennet_orders_add_market("MRSN/USD", U256::from(1u64), U256::from(1u64));
    let err = engine
        .orders
        .state
        .submit_order_ext(
            addr(2),
            m,
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Ioc,
            true,
            0,
        )
        .unwrap_err();
    assert!(matches!(err, MersennetOrdersError::InvalidOrderFlags));
}

#[test]
fn gtd_order_expires_on_the_expiry_block() {
    let (mut engine, _dir) = setup();
    let m = engine.mersennet_orders_add_market("MRSN/USD", U256::from(1u64), U256::from(1u64));
    let state = &mut engine.orders.state;

    // Rest a GTD bid that expires at block 10.
    state
        .submit_order_ext(
            addr(2),
            m,
            Side::Buy,
            U256::from(90u64),
            U256::from(5u64),
            TimeInForce::Gtc,
            false,
            10,
        )
        .unwrap();
    assert_eq!(state.order_book(m).unwrap().bids.len(), 1);

    // Not yet expired at block 9.
    assert_eq!(state.expire_orders(9), 0);
    assert_eq!(state.order_book(m).unwrap().bids.len(), 1);

    // Expires at exactly block 10.
    assert_eq!(state.expire_orders(10), 1);
    assert!(state.order_book(m).unwrap().bids.is_empty());
    assert!(state.open_orders(addr(2)).is_empty());
}

#[test]
fn gtd_zero_never_expires() {
    let (mut engine, _dir) = setup();
    let m = engine.mersennet_orders_add_market("MRSN/USD", U256::from(1u64), U256::from(1u64));
    let state = &mut engine.orders.state;
    state.place_order(addr(2), m, Side::Buy, U256::from(90u64), U256::from(5u64), TimeInForce::Gtc);
    assert_eq!(state.expire_orders(1_000_000), 0);
    assert_eq!(state.order_book(m).unwrap().bids.len(), 1);
}

#[test]
fn create_market_checked_validates_and_dedupes() {
    let (mut engine, _dir) = setup();
    let state = &mut engine.orders.state;

    let id = state
        .create_market_checked("kBTC/USD".to_string(), U256::from(1u64), U256::from(1u64))
        .expect("valid market");
    assert_eq!(id.0, 1);

    // Duplicate symbol (case-insensitive) is rejected.
    let dup = state
        .create_market_checked("kbtc/usd".to_string(), U256::from(1u64), U256::from(1u64))
        .unwrap_err();
    assert!(matches!(dup, MersennetOrdersError::DuplicateMarket));

    // Zero tick/lot is rejected.
    let bad = state
        .create_market_checked("ETH/USD".to_string(), U256::ZERO, U256::from(1u64))
        .unwrap_err();
    assert!(matches!(bad, MersennetOrdersError::InvalidMarketParams));

    // Bad characters rejected.
    let bad2 = state
        .create_market_checked("BAD SYMBOL!".to_string(), U256::from(1u64), U256::from(1u64))
        .unwrap_err();
    assert!(matches!(bad2, MersennetOrdersError::InvalidMarketParams));
}

#[test]
fn token_collateral_counts_toward_initial_margin() {
    let (mut engine, _dir) = setup();
    let m = engine.mersennet_orders_add_market("MRSN/USD", U256::from(1u64), U256::from(1u64));
    let state = &mut engine.orders.state;
    state.set_margin_params(1_000, 500); // 10% initial

    let token = addr(0xAA);
    // 1 token = 1 unit of value, 80% haircut weight.
    state.register_collateral_asset(
        token,
        CollateralAsset {
            weight_bps: 8_000,
            value_num: U256::from(1u64),
            value_den: U256::from(1u64),
            balances_slot: U256::from(3u64),
        },
    );

    let trader = addr(0x33);
    // No native collateral, but 1000 tokens → weighted value 800.
    state.deposit_token_collateral(trader, token, U256::from(1_000u64)).unwrap();
    assert_eq!(state.token_margin_value(trader), U256::from(800u64));

    // Order notional 5000, initial margin 10% = 500 ≤ 800 → accepted.
    let out = state.submit_order_ext(
        trader,
        m,
        Side::Buy,
        U256::from(100u64),
        U256::from(50u64),
        TimeInForce::Gtc,
        false,
        0,
    );
    assert!(out.is_ok(), "token collateral should back the margin: {out:?}");
}

#[test]
fn token_collateral_withdraw_blocked_below_maintenance() {
    let (mut engine, _dir) = setup();
    let state = &mut engine.orders.state;
    let token = addr(0xAA);
    state.register_collateral_asset(
        token,
        CollateralAsset {
            weight_bps: 10_000,
            value_num: U256::from(1u64),
            value_den: U256::from(1u64),
            balances_slot: U256::from(3u64),
        },
    );
    let trader = addr(0x33);
    state.deposit_token_collateral(trader, token, U256::from(100u64)).unwrap();
    // No positions → withdrawal of the full balance is allowed.
    assert!(state.withdraw_token_collateral(trader, token, U256::from(100u64)).is_ok());
    // Over-withdraw fails.
    assert!(matches!(
        state.withdraw_token_collateral(trader, token, U256::from(1u64)).unwrap_err(),
        MersennetOrdersError::InsufficientEquity
    ));
}
