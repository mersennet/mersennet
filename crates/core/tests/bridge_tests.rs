use prime_chain::bridge::{BridgeDomain, BridgeQueue};
use prime_chain::engine::Engine;
use prime_chain::events::{BridgeEvent, BridgeQueueKind, DomainEvent};
use revm::primitives::Bytes;
use tempfile::TempDir;

#[test]
fn enqueue_dequeue_fifo_ordering() {
    let mut queue = BridgeQueue::new();
    let msg_a = queue.push(
        BridgeDomain::PrimeOrders,
        BridgeDomain::PrimeEvm,
        Bytes::from(vec![1, 2, 3]),
    );
    let msg_b = queue.push(
        BridgeDomain::PrimeOrders,
        BridgeDomain::PrimeEvm,
        Bytes::from(vec![4, 5, 6]),
    );
    let msg_c = queue.push(
        BridgeDomain::PrimeOrders,
        BridgeDomain::PrimeEvm,
        Bytes::from(vec![7, 8, 9]),
    );

    let out_a = queue.pop().expect("first message");
    let out_b = queue.pop().expect("second message");
    let out_c = queue.pop().expect("third message");
    assert!(queue.pop().is_none(), "queue should be empty");

    assert_eq!(out_a.nonce, msg_a.nonce);
    assert_eq!(out_b.nonce, msg_b.nonce);
    assert_eq!(out_c.nonce, msg_c.nonce);
    assert_eq!(out_a.payload.as_ref(), &[1, 2, 3]);
    assert_eq!(out_b.payload.as_ref(), &[4, 5, 6]);
    assert_eq!(out_c.payload.as_ref(), &[7, 8, 9]);
}

#[test]
fn nonce_sequencing() {
    let mut queue = BridgeQueue::new();
    let msg1 = queue.push(BridgeDomain::PrimeEvm, BridgeDomain::PrimeOrders, Bytes::from(vec![1]));
    let msg2 = queue.push(BridgeDomain::PrimeEvm, BridgeDomain::PrimeOrders, Bytes::from(vec![2]));
    let msg3 = queue.push(BridgeDomain::PrimeEvm, BridgeDomain::PrimeOrders, Bytes::from(vec![3]));

    assert_eq!(msg1.nonce, 1);
    assert_eq!(msg2.nonce, 2);
    assert_eq!(msg3.nonce, 3);
    assert!(msg2.nonce > msg1.nonce);
    assert!(msg3.nonce > msg2.nonce);
}

#[test]
fn queue_limits_fifo_eviction() {
    let mut queue = BridgeQueue::new();
    queue.set_max_len(Some(2));

    let _msg1 = queue.push(BridgeDomain::PrimeOrders, BridgeDomain::PrimeEvm, Bytes::from(vec![1]));
    let msg2 = queue.push(BridgeDomain::PrimeOrders, BridgeDomain::PrimeEvm, Bytes::from(vec![2]));
    let msg3 = queue.push(BridgeDomain::PrimeOrders, BridgeDomain::PrimeEvm, Bytes::from(vec![3]));

    assert_eq!(queue.len(), 2, "queue should not exceed max_len");

    let out_first = queue.pop().expect("first remaining");
    let out_second = queue.pop().expect("second remaining");

    assert_eq!(out_first.nonce, msg2.nonce, "oldest message evicted");
    assert_eq!(out_second.nonce, msg3.nonce);
}

#[test]
fn bridge_messages_included_in_block() {
    let dir = TempDir::new().expect("temp dir");
    let mut engine = Engine::new_with_state(1, dir.path());

    let payload_a = Bytes::from(vec![10, 20, 30]);
    let payload_b = Bytes::from(vec![40, 50]);
    engine.bridge_enqueue_orders_to_evm(payload_a.clone());
    engine.bridge_enqueue_evm_to_orders(payload_b.clone());

    let block = engine.execute_block().expect("block executed");

    assert_eq!(block.bridge_orders_to_evm.len(), 1);
    assert_eq!(block.bridge_evm_to_orders.len(), 1);
    assert_eq!(block.bridge_orders_to_evm[0].payload.as_ref(), payload_a.as_ref());
    assert_eq!(block.bridge_evm_to_orders[0].payload.as_ref(), payload_b.as_ref());
}

#[test]
fn bridge_events_emitted_correctly() {
    let dir = TempDir::new().expect("temp dir");
    let mut engine = Engine::new_with_state(1, dir.path());

    let payload = Bytes::from(vec![0xAA, 0xBB]);
    engine.bridge_enqueue_orders_to_evm(payload);

    let block = engine.execute_block().expect("block executed");

    let bridge_enqueued = block.domain_events.iter().any(|evt| {
        matches!(
            evt,
            DomainEvent::Bridge(BridgeEvent::Enqueued {
                queue: BridgeQueueKind::OrdersToEvm,
                ..
            })
        )
    });
    assert!(bridge_enqueued, "bridge enqueue event should be in block domain events");
}
