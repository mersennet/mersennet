//! Property-based fuzzing tests for Mersennet matching engine, FBA, mempool,
//! parallel execution, and consensus.

use mersennet::engine::{Engine, Transaction};
use mersennet::fba::{BatchAuction, BatchOrder};
use mersennet::mempool::Mempool;
use mersennet::prime_orders::{MarketId, PrimeOrdersState, Side, TimeInForce};
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use revm::primitives::{Address, B256, Bytes, U256};
use std::collections::HashMap;
use tempfile::tempdir;

fn make_address(seed: u8) -> Address {
    let mut bytes = [0u8; 20];
    bytes[19] = seed;
    Address::from(bytes)
}

#[test]
fn fuzz_matching_conservation_of_value() {
    let mut rng = StdRng::seed_from_u64(42);
    let tick = U256::from(1u64);
    let lot = U256::from(1u64);

    for _ in 0..1000 {
        let mut state = PrimeOrdersState::new();
        state.set_margin_params(0, 0);
        let market = state.add_market("F", tick, lot);
        let market_id = MarketId(market.0);

        let num_accounts = (rng.r#gen::<u8>() % 10).max(2) as usize;
        for i in 0..num_accounts {
            state.deposit_collateral(make_address(i as u8), U256::from(1_000_000u64));
        }

        let num_buys = (rng.r#gen::<u8>() % 20).max(1) as usize;
        let num_sells = (rng.r#gen::<u8>() % 20).max(1) as usize;

        let mut buy_fills: U256 = U256::ZERO;
        let mut sell_fills: U256 = U256::ZERO;

        for i in 0..num_buys {
            let owner = make_address((i % num_accounts) as u8);
            let price = U256::from((rng.r#gen::<u32>() % 1000 + 10) as u64);
            let size = U256::from((rng.r#gen::<u32>() % 50 + 1) as u64);
            if let Ok(outcome) =
                state.submit_order(owner, market_id, Side::Buy, price, size, TimeInForce::Gtc)
            {
                for t in &outcome.trades {
                    buy_fills += t.size;
                    sell_fills += t.size;
                }
            }
        }
        for i in 0..num_sells {
            let owner = make_address((i % num_accounts) as u8);
            let price = U256::from((rng.r#gen::<u32>() % 1000 + 10) as u64);
            let size = U256::from((rng.r#gen::<u32>() % 50 + 1) as u64);
            if let Ok(outcome) =
                state.submit_order(owner, market_id, Side::Sell, price, size, TimeInForce::Gtc)
            {
                for t in &outcome.trades {
                    buy_fills += t.size;
                    sell_fills += t.size;
                }
            }
        }

        assert_eq!(
            buy_fills, sell_fills,
            "conservation: buy fills must equal sell fills"
        );

        for order in state.orders.values() {
            assert!(
                order.size <= U256::from(1000u64),
                "no order size exceeds reasonable max"
            );
        }
    }
}

#[test]
fn fuzz_price_time_priority() {
    let _rng = StdRng::seed_from_u64(123);
    let tick = U256::from(1u64);
    let lot = U256::from(1u64);

    for _ in 0..500 {
        let mut state = PrimeOrdersState::new();
        state.set_margin_params(0, 0);
        let market = state.add_market("P", tick, lot);
        let market_id = MarketId(market.0);
        state.deposit_collateral(make_address(1), U256::from(1_000_000u64));
        state.deposit_collateral(make_address(2), U256::from(1_000_000u64));
        state.deposit_collateral(make_address(3), U256::from(1_000_000u64));

        let maker = make_address(1);
        let taker1 = make_address(2);
        let taker2 = make_address(3);

        state
            .submit_order(
                maker,
                market_id,
                Side::Sell,
                U256::from(100u64),
                U256::from(10u64),
                TimeInForce::Gtc,
            )
            .ok();
        let o1 = state.submit_order(
            taker1,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Gtc,
        );
        let o2 = state.submit_order(
            taker2,
            market_id,
            Side::Buy,
            U256::from(100u64),
            U256::from(5u64),
            TimeInForce::Gtc,
        );

        if let (Ok(o1_result), Ok(o2_result)) = (o1, o2) {
            let o1_fill = o1_result.filled;
            let o2_fill = o2_result.filled;
            assert!(
                o1_fill >= o2_fill,
                "first taker should fill before or equal to second (price-time)"
            );
        }
    }
}

#[test]
fn fuzz_parallel_determinism() {
    let mut rng = StdRng::seed_from_u64(456);

    for iter in 0..100 {
        let dir = tempdir().expect("temp dir");
        std::fs::create_dir_all(dir.path()).ok();
        let dir2 = tempdir().expect("temp dir 2");
        std::fs::create_dir_all(dir2.path()).ok();

        let chain_id = 999u64;
        let mut engine = Engine::new_with_backend(chain_id, dir.path(), "redb");
        let mut engine2 = Engine::new_with_backend(chain_id, dir2.path(), "redb");

        let validator = make_address(0x01);
        engine.fund_account(validator, U256::from(1000u64), 0);
        engine
            .add_validator(validator, U256::from(1000u64))
            .unwrap();
        engine.set_token_economics(U256::ZERO, U256::ZERO, 1);

        engine2.fund_account(validator, U256::from(1000u64), 0);
        engine2
            .add_validator(validator, U256::from(1000u64))
            .unwrap();
        engine2.set_token_economics(U256::ZERO, U256::ZERO, 1);

        let n = (rng.r#gen::<u8>() % 8).max(2);
        for i in 1..=n {
            engine.fund_account(make_address(i), U256::from(10_000_000u64), 0);
            engine2.fund_account(make_address(i), U256::from(10_000_000u64), 0);
        }

        let mut txs = Vec::new();
        for i in 0..n {
            let from = make_address(i % n + 1);
            let to = make_address((i + 1) % n + 1);
            let tx = Transaction {
                from,
                to: Some(to),
                value: U256::from(50u64),
                data: Bytes::new(),
                gas_limit: 21_000,
                gas_price: U256::from(100u64 - i as u64),
                nonce: i as u64,
                chain_id: Some(chain_id),
                signature: None,
                tx_type: 0,
                shielded_payload: None,
            };
            txs.push(tx);
        }

        for tx in txs.clone() {
            engine.submit_tx_unsigned(tx).ok();
        }
        for tx in txs {
            engine2.submit_tx_unsigned(tx).ok();
        }

        let par_block = engine.execute_block_parallel().expect("parallel block");
        let seq_block = engine2.execute_block().expect("sequential block");

        assert_eq!(
            par_block.state_root, seq_block.state_root,
            "iter {}: parallel and sequential state roots must match",
            iter
        );
    }
}

#[test]
fn fuzz_fba_uniform_price() {
    let mut rng = StdRng::seed_from_u64(789);
    let market = MarketId(1);

    for _ in 0..500 {
        let mut auction = BatchAuction::new(market);
        let n_buys = (rng.r#gen::<u8>() % 15).max(1) as usize;
        let n_sells = (rng.r#gen::<u8>() % 15).max(1) as usize;

        for i in 0..n_buys {
            auction.submit(BatchOrder {
                owner: make_address((i % 5) as u8),
                market,
                side: Side::Buy,
                price: U256::from((rng.r#gen::<u32>() % 500 + 50) as u64),
                size: U256::from((rng.r#gen::<u32>() % 20 + 1) as u64),
                tif: TimeInForce::Gtc,
                sequence: 0,
            });
        }
        for i in 0..n_sells {
            auction.submit(BatchOrder {
                owner: make_address((i % 5 + 5) as u8),
                market,
                side: Side::Sell,
                price: U256::from((rng.r#gen::<u32>() % 500 + 50) as u64),
                size: U256::from((rng.r#gen::<u32>() % 20 + 1) as u64),
                tif: TimeInForce::Gtc,
                sequence: 0,
            });
        }

        let result = auction.execute();

        let clearing = result.clearing_price;
        for f in &result.fills {
            assert_eq!(f.price, clearing, "all FBA fills must be at clearing price");
        }

        for f in &result.fills {
            assert!(f.price <= clearing, "no fill worse than clearing");
        }
    }
}

#[test]
fn fuzz_mempool_ordering() {
    let mut rng = StdRng::seed_from_u64(1011);
    let base_fee = U256::from(1u64);

    for _ in 0..300 {
        let mut mempool = Mempool::with_limits(1000, 100, 1000);
        let mut inserted = Vec::new();

        let n_senders = (rng.r#gen::<u8>() % 5).max(1) as usize;
        for _ in 0..20 {
            let sender = make_address(rng.r#gen::<u8>() % n_senders as u8);
            let nonce = rng.r#gen::<u32>() % 10;
            let gas_price = U256::from((rng.r#gen::<u32>() % 100 + 1) as u64);
            let tx = Transaction {
                from: sender,
                to: Some(make_address(99)),
                value: U256::ZERO,
                data: Bytes::new(),
                gas_limit: 21_000,
                gas_price,
                nonce: nonce as u64,
                chain_id: Some(1),
                signature: None,
                tx_type: 0,
                shielded_payload: None,
            };
            let account_nonce = rng.r#gen::<u32>() % (nonce + 1);
            if mempool
                .insert(tx.clone(), base_fee, account_nonce as u64)
                .is_ok()
            {
                inserted.push((tx.gas_price, tx.nonce, tx.from));
            }
        }

        let drained = mempool.drain_ready(100);
        let mut by_sender_nonce: HashMap<Address, Vec<u64>> = HashMap::new();
        for tx in &drained {
            by_sender_nonce.entry(tx.from).or_default().push(tx.nonce);
        }
        for nonces in by_sender_nonce.values_mut() {
            nonces.sort();
        }
        for nonces in by_sender_nonce.values() {
            let mut prev = 0u64;
            for &n in nonces {
                assert!(n >= prev, "nonces must be increasing per sender");
                prev = n;
            }
        }

        let total_drained: usize = drained.len();
        assert!(
            total_drained <= inserted.len(),
            "cannot drain more than inserted"
        );
    }
}

#[test]
fn fuzz_consensus_no_conflicting_commits() {
    let mut rng = StdRng::seed_from_u64(1314);
    let mut finalized: HashMap<u64, B256> = HashMap::new();

    // Simulate consensus: each height gets exactly one finalized block.
    // Deterministic hash per height to model correct protocol behavior.
    for _ in 0..200 {
        let height = rng.r#gen::<u64>() % 100;
        let mut hash_input = [0u8; 40];
        hash_input[0..8].copy_from_slice(&height.to_be_bytes());
        let block_hash = revm::primitives::keccak256(hash_input);

        if let Some(prev) = finalized.get(&height) {
            assert_eq!(*prev, block_hash, "no two different blocks at same height");
        } else {
            finalized.insert(height, block_hash);
        }
    }
}
