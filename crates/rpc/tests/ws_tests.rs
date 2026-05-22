use prime_chain_rpc::ws::{SubscriptionKind, WsSubscriptionManager, set_privacy_mode_activated};

#[test]
fn ws_subscription_manager_lifecycle() {
    set_privacy_mode_activated(false);
    let mut manager = WsSubscriptionManager::new();

    let (id_heads, rx_heads) = manager
        .subscribe(SubscriptionKind::NewHeads)
        .expect("subscribe newHeads");
    let (id_trades, rx_trades) = manager
        .subscribe(SubscriptionKind::PrimeOrdersTrades { market: None })
        .expect("subscribe trades");

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

    set_privacy_mode_activated(true);
    assert!(
        manager
            .subscribe(SubscriptionKind::PrimeOrdersBook { market: 1 })
            .is_err()
    );
    manager
        .subscribe(SubscriptionKind::NewShieldedRoot)
        .expect("shielded subscription should remain enabled");
    set_privacy_mode_activated(false);
}
