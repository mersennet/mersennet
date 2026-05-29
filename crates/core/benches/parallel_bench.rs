use prime_chain::engine::{Engine, Transaction};
use prime_chain::fba::BatchOrder;
use revm::primitives::{Address, Bytes, U256};
use std::time::Instant;

fn make_address(seed: u16) -> Address {
    let mut bytes = [0u8; 20];
    bytes[18] = (seed >> 8) as u8;
    bytes[19] = seed as u8;
    Address::from(bytes)
}

fn main() {
    println!("=== Parallel vs Sequential EVM Execution Benchmark ===\n");

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    println!("Available CPU cores: {}\n", cores);

    for batch_size in [100, 500, 1000, 1428] {
        let dir = tempfile::tempdir().unwrap();
        let mut eng = Engine::new_with_state(7919, dir.path().join("s1"));
        eng.add_validator(make_address(1), U256::from(1000u64))
            .unwrap();
        eng.set_token_economics(U256::from(0u64), U256::from(0u64), 1);

        // Fund unique senders (each sender only sends 1 tx for maximum parallelism)
        for i in 1..=(batch_size as u16 + 1) {
            eng.fund_account(make_address(i), U256::from(10_000_000_000u64), 0);
        }

        // Submit independent transactions (all different senders -> max parallelism)
        for i in 0..batch_size {
            let sender = make_address((i + 1) as u16);
            let receiver = make_address((i + batch_size + 2) as u16);
            let tx = Transaction {
                from: sender,
                to: Some(receiver),
                value: U256::from(1u64),
                data: Bytes::new(),
                gas_limit: 21_000,
                gas_price: U256::from(1u64),
                nonce: 0,
                chain_id: Some(7919),
                signature: None,
                tx_type: 0,
                shielded_payload: None,
            };
            let _ = eng.submit_tx_unsigned(tx);
        }

        // Sequential execution
        let dir2 = tempfile::tempdir().unwrap();
        let mut eng2 = Engine::new_with_state(7919, dir2.path().join("s2"));
        eng2.add_validator(make_address(1), U256::from(1000u64))
            .unwrap();
        eng2.set_token_economics(U256::from(0u64), U256::from(0u64), 1);
        for i in 1..=(batch_size as u16 + 1) {
            eng2.fund_account(make_address(i), U256::from(10_000_000_000u64), 0);
        }
        for i in 0..batch_size {
            let sender = make_address((i + 1) as u16);
            let receiver = make_address((i + batch_size + 2) as u16);
            let tx = Transaction {
                from: sender,
                to: Some(receiver),
                value: U256::from(1u64),
                data: Bytes::new(),
                gas_limit: 21_000,
                gas_price: U256::from(1u64),
                nonce: 0,
                chain_id: Some(7919),
                signature: None,
                tx_type: 0,
                shielded_payload: None,
            };
            let _ = eng2.submit_tx_unsigned(tx);
        }

        // Run sequential
        let seq_start = Instant::now();
        let seq_block = eng2.execute_block().unwrap();
        let seq_elapsed = seq_start.elapsed();
        let seq_tps = seq_block.transactions.len() as f64 / seq_elapsed.as_secs_f64();

        // Run parallel
        let par_start = Instant::now();
        let par_block = eng.execute_block_parallel().unwrap();
        let par_elapsed = par_start.elapsed();
        let par_tps = par_block.transactions.len() as f64 / par_elapsed.as_secs_f64();

        let speedup = seq_elapsed.as_secs_f64() / par_elapsed.as_secs_f64();

        println!(
            "batch={:5} | seq={:8.2}ms ({:8.0} TPS) | par={:8.2}ms ({:8.0} TPS) | speedup={:.2}x",
            batch_size,
            seq_elapsed.as_secs_f64() * 1000.0,
            seq_tps,
            par_elapsed.as_secs_f64() * 1000.0,
            par_tps,
            speedup,
        );
    }

    // Benchmark FBA
    println!("\n=== Frequent Batch Auction Benchmark ===\n");
    for batch_size in [100, 500, 1000, 5000, 10000] {
        let dir = tempfile::tempdir().unwrap();
        let mut eng = Engine::new_with_state(7919, dir.path().join("fba"));
        let market = eng.prime_orders_add_market("BTC/USD", U256::from(1u64), U256::from(1u64));

        let start = Instant::now();
        for i in 0..batch_size {
            let owner = make_address((i % 500 + 1) as u16);
            let side = if i % 2 == 0 {
                prime_chain::prime_orders::Side::Buy
            } else {
                prime_chain::prime_orders::Side::Sell
            };
            let price = if i % 2 == 0 {
                U256::from(100 + (i % 5) as u64)
            } else {
                U256::from(98 + (i % 5) as u64)
            };
            eng.submit_batch_order(BatchOrder {
                owner,
                market,
                side,
                price,
                size: U256::from(10u64),
                tif: prime_chain::prime_orders::TimeInForce::Gtc,
                sequence: 0,
            });
        }
        let submit_elapsed = start.elapsed();

        let exec_start = Instant::now();
        let results = eng.execute_batch_auctions();
        let exec_elapsed = exec_start.elapsed();

        let total_fills: usize = results.iter().map(|r| r.fills.len()).sum();
        let ops_per_sec = batch_size as f64 / (submit_elapsed + exec_elapsed).as_secs_f64();

        println!(
            "orders={:6} | submit={:8.2}ms | auction={:8.2}ms | fills={:5} | ops/s={:12.0}",
            batch_size,
            submit_elapsed.as_secs_f64() * 1000.0,
            exec_elapsed.as_secs_f64() * 1000.0,
            total_fills,
            ops_per_sec,
        );
    }

    // HotStuff-2 Consensus
    println!("\n=== HotStuff-2 Consensus Benchmark ===\n");
    {
        let dir = tempfile::tempdir().unwrap();
        let mut eng = Engine::new_with_state(7919, dir.path().join("hs2"));
        for i in 1..=10u16 {
            eng.add_validator(make_address(i), U256::from(100u64))
                .unwrap();
        }

        let start = Instant::now();
        let iterations = 1000;
        for i in 0..iterations {
            let hash = revm::primitives::keccak256(format!("block-{}", i).as_bytes());
            let _ = eng.consensus.run_hotstuff2_round(hash, i as u64);
        }
        let elapsed = start.elapsed();
        let rounds_per_sec = iterations as f64 / elapsed.as_secs_f64();
        let ms_per_round = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

        println!(
            "validators=10 | rounds={} | total={:.2}ms | per_round={:.3}ms | rounds/s={:.0}",
            iterations,
            elapsed.as_secs_f64() * 1000.0,
            ms_per_round,
            rounds_per_sec
        );
    }

    // ReDB vs Sled storage backend
    println!("\n=== ReDB vs Sled Storage Backend Benchmark ===\n");
    for batch_size in [100, 500, 1000] {
        for backend in ["sled", "redb"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(dir.path()).ok();
            let mut eng = Engine::new_with_backend(7919, dir.path(), backend);
            eng.add_validator(make_address(1), U256::from(1000u64))
                .unwrap();
            eng.set_token_economics(U256::from(0u64), U256::from(0u64), 1);
            for i in 1..=(batch_size as u16 + 1) {
                eng.fund_account(make_address(i), U256::from(10_000_000_000u64), 0);
            }
            for i in 0..batch_size {
                let tx = Transaction {
                    from: make_address((i % 255 + 1) as u16),
                    to: Some(make_address(((i + 128) % 255 + 1) as u16)),
                    value: U256::from(1u64),
                    data: Bytes::new(),
                    gas_limit: 21_000,
                    gas_price: U256::from(1u64),
                    nonce: (i / 255) as u64,
                    chain_id: Some(7919),
                    signature: None,
                    tx_type: 0,
                    shielded_payload: None,
                };
                let _ = eng.submit_tx_unsigned(tx);
            }
            let start = Instant::now();
            let block = eng.execute_block().unwrap();
            let elapsed = start.elapsed();
            let tps = block.transactions.len() as f64 / elapsed.as_secs_f64();
            println!(
                "backend={:5} | batch={:5} | exec={:8.2}ms | TPS={:10.0}",
                backend,
                batch_size,
                elapsed.as_secs_f64() * 1000.0,
                tps
            );
        }
    }

    // ZK MockProver prove/verify
    println!("\n=== ZK MockProver Benchmark ===\n");
    {
        use prime_chain::zk_proofs::{MockProver, StateProver};
        use prime_zkp::sp1::BlockProgramOutput;
        use revm::primitives::B256;

        let prover = MockProver::new();
        let prev_root = B256::from([1u8; 32]);
        let new_root = B256::from([2u8; 32]);
        let block_hash = B256::from([3u8; 32]);

        let iterations = 10_000;
        let start = Instant::now();
        for i in 0..iterations {
            let _ = prover.prove_public_output(&BlockProgramOutput {
                prev_state_root: prev_root.0,
                new_state_root: new_root.0,
                prev_nullifier_root: [0u8; 32],
                new_nullifier_root: [0u8; 32],
                block_number: i,
                block_hash: block_hash.0,
                new_market_state_hash: [0u8; 32],
                shielded_event_root: [0u8; 32],
                tx_count: 100,
            });
        }
        let elapsed = start.elapsed();
        let proves_per_sec = iterations as f64 / elapsed.as_secs_f64();
        let proof = prover
            .prove_public_output(&BlockProgramOutput {
                prev_state_root: prev_root.0,
                new_state_root: new_root.0,
                prev_nullifier_root: [0u8; 32],
                new_nullifier_root: [0u8; 32],
                block_number: 0,
                block_hash: block_hash.0,
                new_market_state_hash: [0u8; 32],
                shielded_event_root: [0u8; 32],
                tx_count: 100,
            })
            .unwrap();
        let verify_start = Instant::now();
        for _ in 0..iterations {
            let _ = prover.verify_proof(&proof);
        }
        let verify_elapsed = verify_start.elapsed();
        let verifies_per_sec = iterations as f64 / verify_elapsed.as_secs_f64();
        println!(
            "prove:  {} ops in {:.2}ms = {:.0} ops/s",
            iterations,
            elapsed.as_secs_f64() * 1000.0,
            proves_per_sec
        );
        println!(
            "verify: {} ops in {:.2}ms = {:.0} ops/s",
            iterations,
            verify_elapsed.as_secs_f64() * 1000.0,
            verifies_per_sec
        );
    }

    // Pipeline push/pop
    println!("\n=== Block Pipeline Benchmark ===\n");
    {
        use prime_chain::pipeline::{BlockPipeline, ExecutedBlock, PipelineConfig, StateDiff};
        use revm::primitives::B256;
        use std::time::Duration;

        let config = PipelineConfig {
            pipeline_depth: 10,
            ..PipelineConfig::default()
        };
        let mut pipeline = BlockPipeline::new(config);
        let zero_root = B256::ZERO;

        let iterations = 10_000;
        let start = Instant::now();
        for i in 0..iterations {
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
            if pipeline.is_full() {
                while pipeline.pop_for_consensus().is_some() {}
            }
        }
        let elapsed = start.elapsed();
        let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();
        println!(
            "push+pop: {} blocks in {:.2}ms = {:.0} ops/s",
            iterations,
            elapsed.as_secs_f64() * 1000.0,
            ops_per_sec
        );
    }
}
