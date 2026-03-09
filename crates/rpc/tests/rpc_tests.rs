use prime_chain::engine::Engine;
use prime_chain_rpc::rpc_router::route;
use revm::primitives::{Address, Bytes, U256};
use serde_json::{json, Value};
use tempfile::TempDir;

fn setup_engine(chain_id: u64) -> (Engine, TempDir) {
    let dir = TempDir::new().expect("temp dir");
    let engine = Engine::new_with_state(chain_id, dir.path());
    (engine, dir)
}

fn addr(byte: u8) -> Address {
    Address::from_slice(&[byte; 20])
}

fn hex_addr(address: Address) -> String {
    format!("0x{}", hex::encode(address.as_slice()))
}

#[test]
fn rpc_prime_chain_id() {
    let (mut engine, _dir) = setup_engine(7919);
    let result = route("prime_chainId", Value::Null, &mut engine).expect("rpc ok");
    assert_eq!(result, Value::String("0x1eef".to_string()));
}

#[test]
fn rpc_prime_block_number() {
    let (mut engine, _dir) = setup_engine(1);
    let result = route("prime_blockNumber", Value::Null, &mut engine).expect("rpc ok");
    assert_eq!(result, Value::String("0x0".to_string()), "no blocks produced yet");

    engine.execute_block().expect("block 1");
    let result = route("prime_blockNumber", Value::Null, &mut engine).expect("rpc ok");
    assert_eq!(result, Value::String("0x1".to_string()), "one block produced");
}

#[test]
fn rpc_prime_get_balance() {
    let (mut engine, _dir) = setup_engine(1);
    let alice = addr(0x11);
    engine.fund_account(alice, U256::from(5_000u64), 0);

    let params = json!([hex_addr(alice), "latest"]);
    let result = route("prime_getBalance", params, &mut engine).expect("rpc ok");
    assert_eq!(result, Value::String("0x1388".to_string()), "5000 = 0x1388");
}

#[test]
fn rpc_prime_send_transaction_adds_to_mempool() {
    let (mut engine, _dir) = setup_engine(7919);
    let alice = addr(0x11);
    let bob = addr(0x22);
    engine.fund_account(alice, U256::from(1_000_000u64), 0);

    let tx = prime_chain::engine::Transaction {
        from: alice,
        to: Some(bob),
        value: U256::from(100u64),
        data: Bytes::new(),
        gas_limit: 21_000,
        gas_price: U256::from(1u64),
        nonce: 0,
        chain_id: Some(7919),
        signature: None,
    };

    assert!(engine.mempool_is_empty(), "mempool starts empty");
    engine.submit_tx_unsigned(tx).expect("tx accepted");
    assert!(!engine.mempool_is_empty(), "tx should be in mempool");
}

#[test]
fn rpc_primeorders_add_market() {
    let (mut engine, _dir) = setup_engine(1);
    let params = json!(["PRIME-PERP", "0x1", "0x1"]);
    let result = route("primeorders_addMarket", params, &mut engine).expect("rpc ok");
    let market_id_str = result.as_str().expect("string result");
    assert!(market_id_str.starts_with("0x"), "market id should be hex");
}

#[test]
fn rpc_primeorders_submit_and_get_order_book() {
    let (mut engine, _dir) = setup_engine(1);
    let market_params = json!(["PRIME-PERP", "0x1", "0x1"]);
    let market_result = route("primeorders_addMarket", market_params, &mut engine).expect("add market");
    let market_id_str = market_result.as_str().expect("market id string");

    let maker = addr(0x33);
    let order_params = json!([{
        "owner": hex_addr(maker),
        "market_id": 1,
        "side": "sell",
        "price": "0x64",
        "size": "0x5"
    }]);
    let order_result = route("primeorders_submitOrder", order_params, &mut engine).expect("submit order");
    let remaining = order_result.get("remaining").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        remaining == "0x5" || remaining == "0x05",
        "no matching buy orders, full size resting, got {remaining}"
    );

    let book_params = json!([market_id_str]);
    let book_result = route("primeorders_getOrderBook", book_params, &mut engine).expect("get order book");
    let asks = book_result.get("asks").and_then(|v| v.as_array()).expect("asks array");
    assert_eq!(asks.len(), 1, "one ask level");
    assert_eq!(
        asks[0].get("price").and_then(|v| v.as_str()).unwrap_or(""),
        "0x64",
        "ask price = 100 = 0x64"
    );
    let ask_size = asks[0].get("size").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        ask_size == "0x5" || ask_size == "0x05",
        "ask size = 5, got {ask_size}"
    );

    let taker = addr(0x44);
    let taker_order = json!([{
        "owner": hex_addr(taker),
        "market_id": 1,
        "side": "buy",
        "price": "0x64",
        "size": "0x3"
    }]);
    let taker_result = route("primeorders_submitOrder", taker_order, &mut engine).expect("taker order");
    let filled = taker_result.get("filled").and_then(|v| v.as_str()).unwrap_or("");
    assert!(filled == "0x3" || filled == "0x03", "taker filled 3, got {filled}");

    let trades = taker_result.get("trades").and_then(|v| v.as_array()).expect("trades array");
    assert_eq!(trades.len(), 1);

    let book_params2 = json!([market_id_str]);
    let book_after = route("primeorders_getOrderBook", book_params2, &mut engine).expect("book after");
    let asks_after = book_after.get("asks").and_then(|v| v.as_array()).expect("asks after");
    assert_eq!(asks_after.len(), 1, "still one ask level");
    let remaining_size = asks_after[0].get("size").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        remaining_size == "0x2" || remaining_size == "0x02",
        "remaining ask size = 2, got {remaining_size}"
    );
}
