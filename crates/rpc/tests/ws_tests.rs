use prime_chain_rpc::ws::{SubscriptionKind, WsSubscriptionManager};

#[test]
fn ws_subscription_manager_lifecycle() {
    let mut manager = WsSubscriptionManager::new();

    let (id_heads, rx_heads) = manager.subscribe(SubscriptionKind::NewHeads);
    let (id_trades, rx_trades) =
        manager.subscribe(SubscriptionKind::PrimeOrdersTrades { market: None });

    assert_eq!(manager.active_count(), 2);

    let block_payload = serde_json::json!({ "number": "0x1", "hash": "0x123" });
    manager.notify_new_block(&block_payload);

    let msg_heads = rx_heads.recv().expect("NewHeads should receive");
    assert!(msg_heads.contains("0x1"));

    let trade_result = rx_trades.try_recv();
    assert!(
        trade_result.is_err() || matches!(trade_result, Err(std::sync::mpsc::TryRecvError::Empty))
    );

    let removed = manager.unsubscribe(id_heads);
    assert!(removed);
    assert_eq!(manager.active_count(), 1);

    let removed = manager.unsubscribe(id_trades);
    assert!(removed);
    assert_eq!(manager.active_count(), 0);
}
