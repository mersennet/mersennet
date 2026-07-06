//! Consensus-routed CLOB order flow over RPC.
//!
//! submitOrder returns {accepted, txHash}; the order tx mines in the next
//! block via the 0x…0100 precompile and only then rests in the book.
//!
//! This lives in its own test binary: block execution uses process-global
//! precompile contexts, so concurrent tests in a shared binary would clobber
//! each other (see collateral_backing.rs for the same constraint).

use mersennet::engine::Engine;
use mersennet_rpc::rpc_router::route;
use revm::primitives::{Address, U256};
use serde_json::json;
use tempfile::TempDir;

#[test]
fn rpc_mersennet_orders_submit_and_get_order_book() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::new_with_state(1, dir.path());

    // Genesis-style seeding (markets are not RPC-creatable).
    let market_id = engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1), U256::from(1));
    assert_eq!(market_id.0, 1);

    let maker = Address::from_slice(&[0x33; 20]);
    engine.fund_account(maker, U256::from(10_000_000u64), 0);
    engine.mersennet_orders_deposit_collateral(maker, U256::from(1_000_000u64));

    let order_params = json!([{
        "owner": format!("0x{}", hex::encode(maker.as_slice())),
        "market_id": 1,
        "side": "sell",
        "price": "0x64",
        "size": "0x5"
    }]);
    let order_result =
        route("mersennet_orders_submitOrder", order_params, &mut engine).expect("submit order");
    assert_eq!(
        order_result.get("accepted").and_then(|v| v.as_bool()),
        Some(true),
        "order must be accepted into the mempool"
    );

    // Mine the order tx.
    let block = engine.execute_block().expect("block");
    assert_eq!(block.transactions.len(), 1, "order tx mined");
    assert!(block.receipts[0].success, "placeOrder tx succeeds");

    // The fill/submit events must be in the block for the indexer.
    assert!(
        !block.domain_events.is_empty(),
        "placeOrder must emit domain events into the block"
    );

    let book_result = route("mersennet_orders_getOrderBook", json!(["0x1"]), &mut engine)
        .expect("get order book");
    let asks = book_result
        .get("asks")
        .and_then(|v| v.as_array())
        .expect("asks array");
    assert_eq!(asks.len(), 1, "one ask level after mining");
    assert_eq!(
        asks[0].get("price").and_then(|v| v.as_str()).unwrap_or(""),
        "0x64",
        "ask price = 100 = 0x64"
    );
}
