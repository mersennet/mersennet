//! Comprehensive integration tests for Prime Chain modules.
//!
//! Covers: ReDB backend, parallel execution, WebSocket, pipeline, ZK proofs,
//! Noise encryption, FBA, and commit-reveal.

use prime_chain::commit_reveal::CommitRevealError;
use prime_chain::engine::{Engine, Transaction};
use prime_chain::fba::BatchOrder;
use prime_chain::prime_orders::{Side, TimeInForce};
use prime_chain::ws::SubscriptionKind;
use revm::primitives::{keccak256, Address, B256, Bytes, U256};
use std::time::Duration;
use tempfile::tempdir;

fn make_address(seed: u8) -> Address {
    let mut bytes = [0u8; 20];
    bytes[19] = seed;
    Address::from(bytes)
}

// ---------------------------------------------------------------------------
// 1. Engine with ReDB backend tests
// ---------------------------------------------------------------------------

#[test]
fn redb_full_block_lifecycle() {
    let dir = tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path()).ok();
    let mut engine = Engine::new_with_backend(999, dir.path(), "redb");

    let alice = make_address(0x11);
    let bob = make_address(0x22);
    let validator = make_address(0x01);

    engine.fund_account(alice, U256::from(1_000_000u64), 0);
    engine.fund_account(bob, U256::from(500_000u64), 0);
    engine.add_validator(validator, U256::from(1000u64)).unwrap();
    engine.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

    engine
        .transfer(alice, bob, U256::from(100u64), 21_000, U256::from(1u64), 0)
        .expect("transfer");
    engine
        .transfer(bob, alice, U256::from(50u64), 21_000, U256::from(1u64), 0)
        .expect("transfer");

    let block = engine.execute_block().expect("execute block");
    assert_eq!(block.transactions.len(), 2);
    assert!(block.receipts.iter().all(|r| r.success));
    assert!(!block.state_root.is_zero());

    let block2 = engine.execute_block().expect("execute block 2");
    assert_eq!(block2.number, 2);

    let alice_bal = engine.get_balance(alice).unwrap();
    let bob_bal = engine.get_balance(bob).unwrap();
    assert!(alice_bal > U256::ZERO);
    assert!(bob_bal > U256::ZERO);
}

#[test]
fn redb_prime_orders_full_cycle() {
    let dir = tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path()).ok();
    let mut engine = Engine::new_with_backend(999, dir.path(), "redb");

    let maker = make_address(0x11);
    let taker = make_address(0x22);
    let validator = make_address(0x01);

    engine.add_validator(validator, U256::from(1000u64)).unwrap();
    engine.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

    let market_id = engine.prime_orders_add_market(
        "PRIME-PERP",
        U256::from(1u64),
        U256::from(1u64),
    );
    engine.prime_orders_deposit_collateral(maker, U256::from(10_000u64));
    engine.prime_orders_deposit_collateral(taker, U256::from(10_000u64));

    let outcome = engine
        .prime_orders_submit_order(
            maker,
            market_id,
            Side::Sell,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Gtc,
        )
        .expect("maker order");
    assert!(outcome.trades.is_empty());
    assert_eq!(outcome.remaining, U256::from(5u64));

    let outcome = engine
        .prime_orders_submit_order(
            taker,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(3u64),
            TimeInForce::Gtc,
        )
        .expect("taker order");
    assert_eq!(outcome.filled, U256::from(3u64));
    assert_eq!(outcome.trades.len(), 1);

    let book = engine.prime_orders_order_book(market_id).expect("order book");
    assert_eq!(book.asks[0].size, U256::from(2u64));

    let block = engine.execute_block().expect("block to persist");
    assert_eq!(block.number, 1);
}

#[test]
fn redb_state_persistence_across_restart() {
    let dir = tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path()).ok();
    let path = dir.path();

    {
        let mut engine = Engine::new_with_backend(999, path, "redb");
        let alice = make_address(0x11);
        let bob = make_address(0x22);
        let validator = make_address(0x01);

        engine.fund_account(alice, U256::from(1_000_000u64), 0);
        engine.fund_account(bob, U256::from(500_000u64), 0);
        engine.add_validator(validator, U256::from(1000u64)).unwrap();
        engine.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

        engine
            .transfer(alice, bob, U256::from(100u64), 21_000, U256::from(1u64), 0)
            .expect("transfer");

        let block = engine.execute_block().expect("execute block");
        assert_eq!(block.number, 1);
    }

    let mut engine = Engine::new_with_backend(999, &path, "redb");
    let alice = make_address(0x11);
    let bob = make_address(0x22);

    let alice_bal = engine.get_balance(alice).unwrap();
    let bob_bal = engine.get_balance(bob).unwrap();
    assert!(alice_bal > U256::ZERO, "Alice balance should persist");
    assert!(bob_bal > U256::ZERO, "Bob balance should persist");
}

// ---------------------------------------------------------------------------
// 2. Parallel execution tests
// ---------------------------------------------------------------------------

#[test]
fn parallel_execution_matches_sequential() {
    let dir = tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path()).ok();
    let mut engine = Engine::new_with_backend(999, dir.path(), "redb");

    let validator = make_address(0x01);
    engine.fund_account(validator, U256::from(1000u64), 0);
    engine.add_validator(validator, U256::from(1000u64)).unwrap();
    engine.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

    for i in 1..=10u8 {
        engine.fund_account(make_address(i), U256::from(10_000_000u64), 0);
    }

    for i in 1..=10u8 {
        let sender = make_address(i);
        let receiver = make_address((i % 10 + 1) as u8);
        let tx = Transaction {
            from: sender,
            to: Some(receiver),
            value: U256::from(100u64),
            data: Bytes::new(),
            gas_limit: 21_000,
            gas_price: U256::from(1u64),
            nonce: 0,
            chain_id: Some(999),
            signature: None,
        };
        engine.submit_tx(tx).expect("submit");
    }

    let par_block = engine.execute_block_parallel().expect("parallel block");
    let par_state_root = par_block.state_root;

    let dir2 = tempdir().expect("temp dir 2");
    std::fs::create_dir_all(dir2.path()).ok();
    let mut engine2 = Engine::new_with_backend(999, dir2.path(), "redb");
    engine2.fund_account(validator, U256::from(1000u64), 0);
    engine2.add_validator(validator, U256::from(1000u64)).unwrap();
    engine2.set_token_economics(U256::from(0u64), U256::from(0u64), 1);
    for i in 1..=10u8 {
        engine2.fund_account(make_address(i), U256::from(10_000_000u64), 0);
    }
    for i in 1..=10u8 {
        let sender = make_address(i);
        let receiver = make_address((i % 10 + 1) as u8);
        let tx = Transaction {
            from: sender,
            to: Some(receiver),
            value: U256::from(100u64),
            data: Bytes::new(),
            gas_limit: 21_000,
            gas_price: U256::from(1u64),
            nonce: 0,
            chain_id: Some(999),
            signature: None,
        };
        engine2.submit_tx(tx).expect("submit");
    }
    let seq_block = engine2.execute_block().expect("sequential block");

    assert_eq!(par_block.transactions.len(), seq_block.transactions.len());
    assert_eq!(par_state_root, seq_block.state_root);
}

// ---------------------------------------------------------------------------
// 3. WebSocket subscription tests
// ---------------------------------------------------------------------------

#[test]
fn ws_subscription_manager_lifecycle() {
    use prime_chain::ws::WsSubscriptionManager;

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
    assert!(trade_result.is_err() || matches!(trade_result, Err(std::sync::mpsc::TryRecvError::Empty)));

    let removed = manager.unsubscribe(id_heads);
    assert!(removed);
    assert_eq!(manager.active_count(), 1);

    let removed = manager.unsubscribe(id_trades);
    assert!(removed);
    assert_eq!(manager.active_count(), 0);
}

// ---------------------------------------------------------------------------
// 4. Pipeline tests
// ---------------------------------------------------------------------------

#[test]
fn pipeline_buffer_and_drain() {
    use prime_chain::pipeline::{BlockPipeline, ExecutedBlock, PipelineConfig, StateDiff};

    let mut config = PipelineConfig::default();
    config.pipeline_depth = 2;

    let mut pipeline = BlockPipeline::new(config);

    let zero_root = B256::ZERO;
    for i in 1..=3u64 {
        let block = ExecutedBlock {
            height: i,
            transactions: vec![],
            receipts: vec![],
            gas_used: 0,
            execution_time: Duration::from_millis(1),
            state_diff: StateDiff {
                dirty_accounts: vec![],
                new_state_root: zero_root,
            },
        };
        pipeline.push_executed(block);
    }

    assert!(pipeline.is_full());

    let b1 = pipeline.pop_for_consensus().expect("block 1");
    assert_eq!(b1.height, 1);

    let b2 = pipeline.pop_for_consensus().expect("block 2");
    assert_eq!(b2.height, 2);

    let b3 = pipeline.pop_for_consensus().expect("block 3");
    assert_eq!(b3.height, 3);

    let empty = pipeline.pop_for_consensus();
    assert!(empty.is_none());
}

// ---------------------------------------------------------------------------
// 5. ZK proof tests
// ---------------------------------------------------------------------------

#[test]
fn mock_prover_roundtrip() {
    use prime_chain::zk_proofs::{MockProver, StateProver};

    let prover = MockProver::new();
    let prev_root = B256::from([1u8; 32]);
    let new_root = B256::from([2u8; 32]);
    let block_hash = B256::from([3u8; 32]);

    let proof = prover
        .prove_state_transition(prev_root, new_root, 1, block_hash, 5)
        .expect("prove");

    let result = prover.verify_proof(&proof).expect("verify");
    assert!(result.valid);

    let mut tampered = proof.clone();
    tampered.proof_data[0] = tampered.proof_data[0].wrapping_add(1);
    let tampered_result = prover.verify_proof(&tampered).expect("verify tampered");
    assert!(!tampered_result.valid);
}

#[test]
fn checkpoint_store_chain_verification() {
    use prime_chain::zk_proofs::{
        CheckpointStore, MockProver, ProofCheckpoint, StateProver,
    };

    let prover = MockProver::new();
    let mut store = CheckpointStore::new(100);

    let mut prev_root = B256::from([0u8; 32]);
    for h in 1..=5u64 {
        let new_root = B256::from([h as u8; 32]);
        let block_hash = keccak256(&h.to_be_bytes());
        let proof = prover
            .prove_state_transition(prev_root, new_root, h, block_hash.into(), 1)
            .expect("prove");
        store.add(ProofCheckpoint {
            height: h,
            state_root: new_root,
            proof: proof.clone(),
            timestamp: 0,
        });
        prev_root = new_root;
    }

    assert!(store.verify_chain(&prover).expect("verify chain"));

    let bad_proof = prover
        .prove_state_transition(B256::ZERO, B256::ZERO, 99, B256::ZERO, 0)
        .expect("bad proof");
    store.add(ProofCheckpoint {
        height: 99,
        state_root: B256::ZERO,
        proof: bad_proof,
        timestamp: 0,
    });

    assert!(!store.verify_chain(&prover).expect("verify should fail"));
}

#[test]
fn batch_proof_aggregation() {
    use prime_chain::zk_proofs::{BatchProofAggregator, MockProver, StateProver};

    let prover = MockProver::new();
    let mut aggregator = BatchProofAggregator::new(3);

    let mut prev_root = B256::from([0u8; 32]);
    for h in 1..=3u64 {
        let new_root = B256::from([h as u8; 32]);
        let block_hash = keccak256(&h.to_be_bytes());
        let proof = prover
            .prove_state_transition(prev_root, new_root, h, block_hash.into(), 1)
            .expect("prove");
        aggregator.add_proof(proof);
        prev_root = new_root;
    }

    assert!(aggregator.should_aggregate());
    let aggregated = aggregator.aggregate().expect("aggregate");
    assert_eq!(aggregated.prev_state_root, B256::from([0u8; 32]));
    assert_eq!(aggregated.new_state_root, B256::from([3u8; 32]));
}

// ---------------------------------------------------------------------------
// 6. Noise encryption tests
// ---------------------------------------------------------------------------

#[test]
fn noise_keypair_generation() {
    use prime_chain::noise::NoiseKeypair;

    let kp = NoiseKeypair::generate();
    assert_eq!(kp.private_key.len(), 32);
    assert_eq!(kp.public_key.len(), 32);
    assert_ne!(kp.private_key, kp.public_key);
}

#[test]
fn noise_keypair_from_bytes() {
    use prime_chain::noise::NoiseKeypair;

    let kp1 = NoiseKeypair::generate();
    let kp2 = NoiseKeypair::from_bytes(&kp1.private_key).unwrap();
    assert_eq!(kp1.public_key, kp2.public_key);
}

// ---------------------------------------------------------------------------
// 7. FBA + Commit-Reveal integration
// ---------------------------------------------------------------------------

#[test]
fn fba_engine_full_auction_cycle() {
    let dir = tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path()).ok();
    let mut engine = Engine::new_with_backend(999, dir.path(), "redb");

    let market_id = engine.prime_orders_add_market(
        "BTC/USD",
        U256::from(1u64),
        U256::from(1u64),
    );

    let buyer = make_address(0x11);
    let seller = make_address(0x22);
    engine.prime_orders_deposit_collateral(buyer, U256::from(10_000u64));
    engine.prime_orders_deposit_collateral(seller, U256::from(10_000u64));

    engine.submit_batch_order(BatchOrder {
        owner: buyer,
        market: market_id,
        side: Side::Buy,
        price: U256::from(100u64),
        size: U256::from(5u64),
        tif: TimeInForce::Gtc,
        sequence: 0,
    });
    engine.submit_batch_order(BatchOrder {
        owner: seller,
        market: market_id,
        side: Side::Sell,
        price: U256::from(100u64),
        size: U256::from(5u64),
        tif: TimeInForce::Gtc,
        sequence: 0,
    });

    let results = engine.execute_batch_auctions();
    assert!(!results.is_empty());
    assert!(results[0].matched_volume > U256::ZERO);
    assert!(!results[0].fills.is_empty());
}

#[test]
fn commit_reveal_full_cycle() {
    use prime_chain::commit_reveal::{CommitRevealPool, TxCommitment, TxReveal};

    let mut pool = CommitRevealPool::new(10);
    pool.current_block = 0;

    let tx_data = Bytes::from_static(b"encrypted_tx_payload");
    let salt = B256::from([42u8; 32]);
    let mut commit_input = Vec::new();
    commit_input.extend_from_slice(tx_data.as_ref());
    commit_input.extend_from_slice(salt.as_slice());
    let commitment_hash = keccak256(&commit_input);

    let commitment = TxCommitment {
        commitment_hash,
        sender: make_address(0x11),
        block_number: 0,
    };
    pool.commit(commitment).expect("commit");

    let wrong_salt = B256::from([99u8; 32]);
    let wrong_reveal = TxReveal {
        commitment_hash,
        encrypted_tx: tx_data.clone(),
        salt: wrong_salt,
    };
    let err = pool.reveal(wrong_reveal).unwrap_err();
    assert!(matches!(err, CommitRevealError::InvalidReveal));

    let correct_reveal = TxReveal {
        commitment_hash,
        encrypted_tx: tx_data.clone(),
        salt,
    };
    let revealed = pool.reveal(correct_reveal).expect("reveal");
    assert_eq!(revealed, tx_data);

    let drained = pool.drain_revealed();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0], tx_data);
}
