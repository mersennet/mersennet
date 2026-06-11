use mersennet::engine::{Engine, Transaction};
use revm::primitives::{Address, Bytes, U256};
use std::time::Instant;

fn make_address(seed: u8) -> Address {
    let mut bytes = [0u8; 20];
    bytes[19] = seed;
    Address::from(bytes)
}

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new_with_state(131071, dir.path().join("state"));

    // Fund 256 accounts with plenty of balance
    let funder = make_address(0);
    engine.fund_account(funder, U256::from(u128::MAX), 0);
    for i in 1..=255u8 {
        engine.fund_account(make_address(i), U256::from(10_000_000_000u64), 0);
    }

    // Add a validator so blocks finalize
    engine
        .add_validator(make_address(1), U256::from(1000u64))
        .unwrap();
    engine.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

    // ─── Benchmark 1: Simple ETH transfers ───
    println!("=== Benchmark: Simple ETH Transfers ===");
    for batch_size in [100, 500, 1000, 2000, 5000] {
        let dir2 = tempfile::tempdir().unwrap();
        let mut eng = Engine::new_with_state(131071, dir2.path().join("state"));
        eng.add_validator(make_address(1), U256::from(1000u64))
            .unwrap();
        eng.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

        // Fund senders
        for i in 0..batch_size {
            let sender = make_address((i % 255 + 1) as u8);
            eng.fund_account(sender, U256::from(10_000_000_000u64), 0);
        }

        // Submit transactions
        let submit_start = Instant::now();
        for i in 0..batch_size {
            let sender_idx = (i % 255 + 1) as u8;
            let sender = make_address(sender_idx);
            let nonce = (i / 255) as u64;
            let receiver = make_address(((i + 128) % 255 + 1) as u8);
            let tx = Transaction {
                from: sender,
                to: Some(receiver),
                value: U256::from(1u64),
                data: Bytes::new(),
                gas_limit: 21_000,
                gas_price: U256::from(1u64),
                nonce,
                chain_id: Some(131071),
                signature: None,
                tx_type: 0,
                shielded_payload: None,
            };
            let _ = eng.submit_tx_unsigned(tx);
        }
        let submit_elapsed = submit_start.elapsed();

        // Execute block
        let exec_start = Instant::now();
        let block = eng.execute_block().unwrap();
        let exec_elapsed = exec_start.elapsed();

        let tx_count = block.transactions.len();
        let tps = if exec_elapsed.as_secs_f64() > 0.0 {
            tx_count as f64 / exec_elapsed.as_secs_f64()
        } else {
            f64::INFINITY
        };

        println!(
            "  batch={:5} | submitted={:5} | executed={:5} | submit={:8.2}ms | exec={:8.2}ms | TPS={:10.0} | gas_used={}",
            batch_size,
            batch_size,
            tx_count,
            submit_elapsed.as_secs_f64() * 1000.0,
            exec_elapsed.as_secs_f64() * 1000.0,
            tps,
            block.gas_used,
        );
    }

    // ─── Benchmark 2: Contract deployment ───
    println!("\n=== Benchmark: Contract Deployments ===");
    let creation_bytecode = Bytes::from_static(&[
        0x60, 0x0a, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x0a, 0x60, 0x00, 0xf3, 0x60, 0x2a, 0x60,
        0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
    ]);

    for batch_size in [50, 100, 500, 1000] {
        let dir2 = tempfile::tempdir().unwrap();
        let mut eng = Engine::new_with_state(131071, dir2.path().join("state"));
        eng.add_validator(make_address(1), U256::from(1000u64))
            .unwrap();
        eng.set_token_economics(U256::from(0u64), U256::from(0u64), 1);
        eng.set_fee_market_params(300_000_000, 2, 8);

        for i in 0..batch_size {
            let sender = make_address((i % 255 + 1) as u8);
            eng.fund_account(sender, U256::from(10_000_000_000u64), 0);
        }

        for i in 0..batch_size {
            let sender_idx = (i % 255 + 1) as u8;
            let sender = make_address(sender_idx);
            let nonce = (i / 255) as u64;
            let tx = Transaction {
                from: sender,
                to: None,
                value: U256::ZERO,
                data: creation_bytecode.clone(),
                gas_limit: 200_000,
                gas_price: U256::from(1u64),
                nonce,
                chain_id: Some(131071),
                signature: None,
                tx_type: 0,
                shielded_payload: None,
            };
            let _ = eng.submit_tx_unsigned(tx);
        }

        let exec_start = Instant::now();
        let block = eng.execute_block().unwrap();
        let exec_elapsed = exec_start.elapsed();

        let tx_count = block.transactions.len();
        let tps = if exec_elapsed.as_secs_f64() > 0.0 {
            tx_count as f64 / exec_elapsed.as_secs_f64()
        } else {
            f64::INFINITY
        };

        println!(
            "  batch={:5} | executed={:5} | exec={:8.2}ms | TPS={:10.0} | gas_used={}",
            batch_size,
            tx_count,
            exec_elapsed.as_secs_f64() * 1000.0,
            tps,
            block.gas_used,
        );
    }

    // ─── Benchmark 3: MersennetOrders matching ───
    println!("\n=== Benchmark: MersennetOrders Order Matching ===");
    for batch_size in [100, 500, 1000, 5000, 10000] {
        let dir2 = tempfile::tempdir().unwrap();
        let mut eng = Engine::new_with_state(131071, dir2.path().join("state"));
        eng.add_validator(make_address(1), U256::from(1000u64))
            .unwrap();
        eng.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

        let market = eng.mersennet_orders_add_market("BTC/USDC", U256::from(1u64), U256::from(1u64));

        // Place maker orders (sells at price 100)
        let start = Instant::now();
        for i in 0..batch_size {
            let maker = make_address((i % 200 + 50) as u8);
            let _ = eng.mersennet_orders_submit_order(
                maker,
                market,
                mersennet::mersennet_orders::Side::Sell,
                U256::from(100u64),
                U256::from(1u64),
                mersennet::mersennet_orders::TimeInForce::Gtc,
            );
        }
        let maker_elapsed = start.elapsed();

        // Place taker orders (buys at price 100) - these will match
        let start = Instant::now();
        for i in 0..batch_size {
            let taker = make_address((i % 50 + 1) as u8);
            let _ = eng.mersennet_orders_submit_order(
                taker,
                market,
                mersennet::mersennet_orders::Side::Buy,
                U256::from(100u64),
                U256::from(1u64),
                mersennet::mersennet_orders::TimeInForce::Ioc,
            );
        }
        let taker_elapsed = start.elapsed();

        let total_orders = batch_size * 2;
        let total_elapsed = maker_elapsed + taker_elapsed;
        let ops = if total_elapsed.as_secs_f64() > 0.0 {
            total_orders as f64 / total_elapsed.as_secs_f64()
        } else {
            f64::INFINITY
        };
        let match_ops = if taker_elapsed.as_secs_f64() > 0.0 {
            batch_size as f64 / taker_elapsed.as_secs_f64()
        } else {
            f64::INFINITY
        };

        println!(
            "  orders={:5} | makers={:8.2}ms | takers+match={:8.2}ms | total_ops/s={:10.0} | match_ops/s={:10.0}",
            total_orders,
            maker_elapsed.as_secs_f64() * 1000.0,
            taker_elapsed.as_secs_f64() * 1000.0,
            ops,
            match_ops,
        );
    }

    // ─── Benchmark 4: Mixed workload (realistic) ───
    println!("\n=== Benchmark: Mixed Workload (transfers + orders in one block) ===");
    {
        let dir2 = tempfile::tempdir().unwrap();
        let mut eng = Engine::new_with_state(131071, dir2.path().join("state"));
        eng.add_validator(make_address(1), U256::from(1000u64))
            .unwrap();
        eng.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

        let market = eng.mersennet_orders_add_market("ETH/USDC", U256::from(1u64), U256::from(1u64));

        // Fund accounts
        for i in 1..=255u8 {
            eng.fund_account(make_address(i), U256::from(10_000_000_000u64), 0);
        }

        // Submit 1000 transfers
        for i in 0..1000u64 {
            let sender = make_address((i % 255 + 1) as u8);
            let nonce = i / 255;
            let tx = Transaction {
                from: sender,
                to: Some(make_address(((i + 100) % 255 + 1) as u8)),
                value: U256::from(1u64),
                data: Bytes::new(),
                gas_limit: 21_000,
                gas_price: U256::from(1u64),
                nonce,
                chain_id: Some(131071),
                signature: None,
                tx_type: 0,
                shielded_payload: None,
            };
            let _ = eng.submit_tx_unsigned(tx);
        }

        // Submit 500 maker + 500 taker orders
        for i in 0..500u64 {
            let maker = make_address((i % 200 + 50) as u8);
            let _ = eng.mersennet_orders_submit_order(
                maker,
                market,
                mersennet::mersennet_orders::Side::Sell,
                U256::from(100u64),
                U256::from(1u64),
                mersennet::mersennet_orders::TimeInForce::Gtc,
            );
        }
        for i in 0..500u64 {
            let taker = make_address((i % 50 + 1) as u8);
            let _ = eng.mersennet_orders_submit_order(
                taker,
                market,
                mersennet::mersennet_orders::Side::Buy,
                U256::from(100u64),
                U256::from(1u64),
                mersennet::mersennet_orders::TimeInForce::Ioc,
            );
        }

        let start = Instant::now();
        let block = eng.execute_block().unwrap();
        let elapsed = start.elapsed();

        let evm_tps = if elapsed.as_secs_f64() > 0.0 {
            block.transactions.len() as f64 / elapsed.as_secs_f64()
        } else {
            f64::INFINITY
        };

        println!(
            "  EVM txs={} | Orders=1000 | Block exec={:.2}ms | EVM TPS={:.0} | Gas used={}",
            block.transactions.len(),
            elapsed.as_secs_f64() * 1000.0,
            evm_tps,
            block.gas_used,
        );
    }

    // ─── Summary ───
    println!("\n=== Theoretical Max (1s block time) ===");
    println!("  If block execution takes Xms, then TPS = (executed_txs / X) * 1000");
    println!("  Remaining time budget: 1000ms - exec_ms - consensus_ms - network_ms");
}
