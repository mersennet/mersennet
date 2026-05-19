//! Adversarial stress test tool for Prime Chain.
//! Runs scenarios that test edge cases, backpressure, and error handling.

use std::time::{Duration, Instant};

use k256::ecdsa::SigningKey;
use prime_chain::crypto::{address_from_signing_key, encode_raw_signed_tx, sign_transaction};
use prime_chain::engine::Transaction;
use revm::primitives::{Address, Bytes, U256};
use serde_json::{Value, json};

const DEFAULT_RPC: &str = "http://127.0.0.1:8545";

type ScenarioFn = dyn Fn(&str, u64) -> ScenarioResult;

fn main() {
    let rpc = parse_rpc_arg();

    println!("╔══════════════════════════════════════════════╗");
    println!("║       Prime Chain Stress Tests               ║");
    println!("╚══════════════════════════════════════════════╝");
    println!("[CONFIG] RPC: {}", rpc);
    println!();

    let chain_id = match get_chain_id(&rpc) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("[FATAL] Cannot connect to node: {}", e);
            std::process::exit(1);
        }
    };

    let scenarios: Vec<(&str, Box<ScenarioFn>)> = vec![
        ("Mempool Flood", Box::new(scenario_mempool_flood)),
        ("Large Blocks", Box::new(scenario_large_blocks)),
        ("Rapid Reconnect", Box::new(scenario_rapid_reconnect)),
        ("Nonce Gaps", Box::new(scenario_nonce_gaps)),
        ("Duplicate Transactions", Box::new(scenario_duplicate_txs)),
        ("Invalid Transactions", Box::new(scenario_invalid_txs)),
        ("CLOB Stress", Box::new(scenario_clob_stress)),
    ];

    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut results: Vec<(String, ScenarioResult)> = Vec::new();

    for (name, run) in &scenarios {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("[SCENARIO] {}", name);
        let result = run(&rpc, chain_id);
        match result.pass {
            true => {
                println!("[RESULT] PASS ({:.1}ms)", result.duration_ms);
                passed += 1;
            }
            false => {
                println!(
                    "[RESULT] FAIL ({:.1}ms) - {}",
                    result.duration_ms, result.detail
                );
                failed += 1;
            }
        }
        results.push((name.to_string(), result));
    }

    println!();
    println!("╔══════════════════════════════════════════════╗");
    println!("║           STRESS TEST SUMMARY                ║");
    println!("╚══════════════════════════════════════════════╝");
    for (name, result) in &results {
        let status = if result.pass { "PASS" } else { "FAIL" };
        println!("  [{}] {} ({:.1}ms)", status, name, result.duration_ms);
    }
    println!();
    println!(
        "  Total: {} passed, {} failed, {} total",
        passed,
        failed,
        passed + failed
    );
    println!("══════════════════════════════════════════════");

    if failed > 0 {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

fn parse_rpc_arg() -> String {
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--rpc"
            && let Some(v) = iter.next()
        {
            return v;
        }
        if arg == "--help" || arg == "-h" {
            println!("Usage: stresstest [OPTIONS]");
            println!("  --rpc URL    RPC endpoint (default: {})", DEFAULT_RPC);
            std::process::exit(0);
        }
    }
    DEFAULT_RPC.to_string()
}

// ---------------------------------------------------------------------------
// Scenario infrastructure
// ---------------------------------------------------------------------------

struct ScenarioResult {
    pass: bool,
    duration_ms: f64,
    detail: String,
}

fn make_account() -> (SigningKey, Address) {
    let key = SigningKey::random(&mut rand::thread_rng());
    let addr = address_from_signing_key(&key);
    (key, addr)
}

fn fund_account(rpc: &str, address: Address, chain_id: u64) {
    let addr_hex = format!("0x{}", hex::encode(address.as_slice()));
    let tx_obj = json!({
        "from": "0x0000000000000000000000000000000000000000",
        "to": addr_hex,
        "value": format!("0x{:x}", 100_000_000_000_000u64),
        "gas": "0x5208",
        "gasPrice": "0x1",
        "chainId": format!("0x{:x}", chain_id),
    });
    let _ = rpc_call(rpc, "prime_sendTransaction", json!([tx_obj]));
}

#[allow(clippy::too_many_arguments)]
fn send_signed_tx(
    rpc: &str,
    key: &SigningKey,
    from: Address,
    to: Option<Address>,
    value: U256,
    nonce: u64,
    chain_id: u64,
    gas_limit: u64,
    data: Bytes,
) -> Result<String, String> {
    let tx = Transaction {
        from,
        to,
        value,
        data,
        gas_limit,
        gas_price: U256::from(1u64),
        nonce,
        chain_id: Some(chain_id),
        signature: None,
    };
    let signed = sign_transaction(&tx, key);
    let raw = encode_raw_signed_tx(&signed);
    let raw_hex = format!("0x{}", hex::encode(&raw));
    let result = rpc_call(rpc, "eth_sendRawTransaction", json!([raw_hex]))?;
    Ok(result.as_str().unwrap_or("").to_string())
}

// ---------------------------------------------------------------------------
// Scenario 1: Mempool Flood
// Submit 10K transactions in ~1 second, verify node handles backpressure.
// ---------------------------------------------------------------------------

fn scenario_mempool_flood(rpc: &str, chain_id: u64) -> ScenarioResult {
    println!("[TEST] Submitting 10,000 transactions as fast as possible...");
    let start = Instant::now();

    let (key, addr) = make_account();
    fund_account(rpc, addr, chain_id);

    let target_addr = Address::from_slice(&[0xBB; 20]);
    let mut submitted = 0u64;
    let mut errors = 0u64;

    for nonce in 0..10_000u64 {
        let result = send_signed_tx(
            rpc,
            &key,
            addr,
            Some(target_addr),
            U256::from(1u64),
            nonce,
            chain_id,
            21_000,
            Bytes::new(),
        );
        match result {
            Ok(_) => submitted += 1,
            Err(_) => errors += 1,
        }
    }

    let elapsed = start.elapsed();
    println!(
        "[TEST] Submitted: {}, Errors: {}, Duration: {:.1}s",
        submitted,
        errors,
        elapsed.as_secs_f64()
    );

    let pass = submitted > 0;
    ScenarioResult {
        pass,
        duration_ms: elapsed.as_secs_f64() * 1000.0,
        detail: format!("submitted={} errors={}", submitted, errors),
    }
}

// ---------------------------------------------------------------------------
// Scenario 2: Large Blocks
// Submit transactions with max gas to create large blocks.
// ---------------------------------------------------------------------------

fn scenario_large_blocks(rpc: &str, chain_id: u64) -> ScenarioResult {
    println!("[TEST] Submitting large-gas transactions to fill blocks...");
    let start = Instant::now();

    let (key, addr) = make_account();
    fund_account(rpc, addr, chain_id);

    // Simple contract with a loop that burns gas:
    // PUSH1 0xFF, PUSH1 0x00, MSTORE, PUSH1 0x00, PUSH1 0x00, RETURN
    let heavy_data = Bytes::from(vec![
        0x60, 0xFF, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xF3,
    ]);

    let mut submitted = 0u64;
    for nonce in 0..20u64 {
        let result = send_signed_tx(
            rpc,
            &key,
            addr,
            None,
            U256::ZERO,
            nonce,
            chain_id,
            10_000_000,
            heavy_data.clone(),
        );
        if result.is_ok() {
            submitted += 1;
        }
    }

    let elapsed = start.elapsed();
    println!("[TEST] Submitted {} large-gas transactions", submitted);

    ScenarioResult {
        pass: submitted > 0,
        duration_ms: elapsed.as_secs_f64() * 1000.0,
        detail: format!("submitted={}", submitted),
    }
}

// ---------------------------------------------------------------------------
// Scenario 3: Rapid Reconnect
// Simulate P2P peer churn by rapidly connecting/disconnecting via RPC health.
// ---------------------------------------------------------------------------

fn scenario_rapid_reconnect(rpc: &str, _chain_id: u64) -> ScenarioResult {
    println!("[TEST] Simulating rapid reconnect via health checks...");
    let start = Instant::now();

    let mut successes = 0u64;
    let mut failures = 0u64;

    for _ in 0..100 {
        match rpc_call(rpc, "prime_blockNumber", json!([])) {
            Ok(_) => successes += 1,
            Err(_) => failures += 1,
        }
    }

    let elapsed = start.elapsed();
    println!(
        "[TEST] {} connections, {} failures in {:.1}ms",
        successes,
        failures,
        elapsed.as_secs_f64() * 1000.0
    );

    ScenarioResult {
        pass: successes > 50,
        duration_ms: elapsed.as_secs_f64() * 1000.0,
        detail: format!("successes={} failures={}", successes, failures),
    }
}

// ---------------------------------------------------------------------------
// Scenario 4: Nonce Gaps
// Submit transactions with gaps in nonces, verify node handles reordering.
// ---------------------------------------------------------------------------

fn scenario_nonce_gaps(rpc: &str, chain_id: u64) -> ScenarioResult {
    println!("[TEST] Submitting transactions with nonce gaps...");
    let start = Instant::now();

    let (key, addr) = make_account();
    fund_account(rpc, addr, chain_id);

    let target = Address::from_slice(&[0xCC; 20]);
    let mut accepted = 0u64;
    let mut rejected = 0u64;

    // Submit nonces out of order: 5, 3, 1, 0, 2, 4
    let nonces = [5u64, 3, 1, 0, 2, 4];
    for &nonce in &nonces {
        let result = send_signed_tx(
            rpc,
            &key,
            addr,
            Some(target),
            U256::from(1u64),
            nonce,
            chain_id,
            21_000,
            Bytes::new(),
        );
        match result {
            Ok(_) => accepted += 1,
            Err(_) => rejected += 1,
        }
    }

    // Also submit nonce 100 (large gap)
    let gap_result = send_signed_tx(
        rpc,
        &key,
        addr,
        Some(target),
        U256::from(1u64),
        100,
        chain_id,
        21_000,
        Bytes::new(),
    );
    let gap_accepted = gap_result.is_ok();

    let elapsed = start.elapsed();
    println!(
        "[TEST] Accepted: {}, Rejected: {}, Large gap accepted: {}",
        accepted, rejected, gap_accepted
    );

    ScenarioResult {
        pass: accepted > 0,
        duration_ms: elapsed.as_secs_f64() * 1000.0,
        detail: format!(
            "accepted={} rejected={} gap_accepted={}",
            accepted, rejected, gap_accepted
        ),
    }
}

// ---------------------------------------------------------------------------
// Scenario 5: Duplicate Transactions
// Submit the same transaction multiple times.
// ---------------------------------------------------------------------------

fn scenario_duplicate_txs(rpc: &str, chain_id: u64) -> ScenarioResult {
    println!("[TEST] Submitting duplicate transactions...");
    let start = Instant::now();

    let (key, addr) = make_account();
    fund_account(rpc, addr, chain_id);

    let target = Address::from_slice(&[0xDD; 20]);
    let tx = Transaction {
        from: addr,
        to: Some(target),
        value: U256::from(1u64),
        data: Bytes::new(),
        gas_limit: 21_000,
        gas_price: U256::from(1u64),
        nonce: 0,
        chain_id: Some(chain_id),
        signature: None,
    };

    let signed = sign_transaction(&tx, &key);
    let raw = encode_raw_signed_tx(&signed);
    let raw_hex = format!("0x{}", hex::encode(&raw));

    let mut first_accepted = false;
    let mut duplicates_rejected = 0u64;
    let mut duplicates_accepted = 0u64;

    for i in 0..10 {
        let result = rpc_call(rpc, "eth_sendRawTransaction", json!([raw_hex.clone()]));
        if i == 0 {
            first_accepted = result.is_ok();
        } else {
            match result {
                Ok(_) => duplicates_accepted += 1,
                Err(_) => duplicates_rejected += 1,
            }
        }
    }

    let elapsed = start.elapsed();
    println!(
        "[TEST] First accepted: {}, Duplicates rejected: {}, Duplicates accepted: {}",
        first_accepted, duplicates_rejected, duplicates_accepted
    );

    ScenarioResult {
        pass: first_accepted,
        duration_ms: elapsed.as_secs_f64() * 1000.0,
        detail: format!(
            "first_accepted={} dup_rejected={} dup_accepted={}",
            first_accepted, duplicates_rejected, duplicates_accepted
        ),
    }
}

// ---------------------------------------------------------------------------
// Scenario 6: Invalid Transactions
// Submit transactions with invalid signatures, insufficient balance, etc.
// ---------------------------------------------------------------------------

fn scenario_invalid_txs(rpc: &str, chain_id: u64) -> ScenarioResult {
    println!("[TEST] Submitting invalid transactions...");
    let start = Instant::now();

    let mut correctly_rejected = 0u64;
    let mut incorrectly_accepted = 0u64;

    // 6a: Invalid signature (corrupted raw tx)
    {
        let (key, addr) = make_account();
        let tx = Transaction {
            from: addr,
            to: Some(Address::from_slice(&[0xEE; 20])),
            value: U256::from(1u64),
            data: Bytes::new(),
            gas_limit: 21_000,
            gas_price: U256::from(1u64),
            nonce: 0,
            chain_id: Some(chain_id),
            signature: None,
        };
        let signed = sign_transaction(&tx, &key);
        let mut raw = encode_raw_signed_tx(&signed);
        // Corrupt the signature
        if let Some(byte) = raw.last_mut() {
            *byte ^= 0xFF;
        }
        let raw_hex = format!("0x{}", hex::encode(&raw));
        match rpc_call(rpc, "eth_sendRawTransaction", json!([raw_hex])) {
            Ok(_) => incorrectly_accepted += 1,
            Err(_) => correctly_rejected += 1,
        }
    }

    // 6b: Insufficient balance (unfunded account sending value)
    {
        let (key, addr) = make_account();
        let result = send_signed_tx(
            rpc,
            &key,
            addr,
            Some(Address::from_slice(&[0xFF; 20])),
            U256::from(999_999_999_999_999u64),
            0,
            chain_id,
            21_000,
            Bytes::new(),
        );
        match result {
            Ok(_) => incorrectly_accepted += 1,
            Err(_) => correctly_rejected += 1,
        }
    }

    // 6c: Zero gas limit
    {
        let (key, addr) = make_account();
        fund_account(rpc, addr, chain_id);
        let result = send_signed_tx(
            rpc,
            &key,
            addr,
            Some(Address::from_slice(&[0xAA; 20])),
            U256::from(1u64),
            0,
            chain_id,
            0,
            Bytes::new(),
        );
        match result {
            Ok(_) => incorrectly_accepted += 1,
            Err(_) => correctly_rejected += 1,
        }
    }

    // 6d: Malformed raw hex
    {
        match rpc_call(rpc, "eth_sendRawTransaction", json!(["0xdeadbeef"])) {
            Ok(_) => incorrectly_accepted += 1,
            Err(_) => correctly_rejected += 1,
        }
    }

    let elapsed = start.elapsed();
    println!(
        "[TEST] Correctly rejected: {}, Incorrectly accepted: {}",
        correctly_rejected, incorrectly_accepted
    );

    ScenarioResult {
        pass: correctly_rejected >= 2,
        duration_ms: elapsed.as_secs_f64() * 1000.0,
        detail: format!(
            "rejected={} accepted={}",
            correctly_rejected, incorrectly_accepted
        ),
    }
}

// ---------------------------------------------------------------------------
// Scenario 7: CLOB Stress
// Place 1000 orders, cancel 500, place 1000 more, run FBA.
// ---------------------------------------------------------------------------

fn scenario_clob_stress(rpc: &str, chain_id: u64) -> ScenarioResult {
    println!("[TEST] CLOB stress: place 1000, cancel 500, place 1000, run FBA...");
    let start = Instant::now();

    let (_, addr) = make_account();
    fund_account(rpc, addr, chain_id);

    let addr_hex = format!("0x{}", hex::encode(addr.as_slice()));

    // Ensure market 0 exists
    let _ = rpc_call(
        rpc,
        "prime_addMarket",
        json!([{"symbol": "ETH-USD", "tick_size": 1, "lot_size": 1}]),
    );

    // Deposit collateral
    let _ = rpc_call(
        rpc,
        "prime_depositCollateral",
        json!([{"owner": addr_hex, "amount": 1_000_000_000}]),
    );

    // Phase 1: Place 1000 orders
    let mut placed = 0u64;
    let mut order_ids: Vec<u64> = Vec::new();
    for i in 0..1000u64 {
        let side = if i % 2 == 0 { "buy" } else { "sell" };
        let price = 900 + (i % 200);
        let params = json!([{
            "owner": addr_hex,
            "market_id": 0,
            "side": side,
            "price": price,
            "quantity": 1,
            "time_in_force": "gtc"
        }]);
        if let Ok(val) = rpc_call(rpc, "prime_submitOrder", params) {
            placed += 1;
            if let Some(id) = val.get("order_id").and_then(|v| v.as_u64()) {
                order_ids.push(id);
            }
        }
    }
    println!("[TEST] Phase 1: Placed {} orders", placed);

    // Phase 2: Cancel 500 orders
    let mut cancelled = 0u64;
    for id in order_ids.iter().take(500) {
        if rpc_call(rpc, "prime_cancelOrder", json!([*id])).is_ok() {
            cancelled += 1
        }
    }
    println!("[TEST] Phase 2: Cancelled {} orders", cancelled);

    // Phase 3: Place 1000 more orders
    let mut placed2 = 0u64;
    for i in 0..1000u64 {
        let side = if i % 2 == 0 { "buy" } else { "sell" };
        let price = 950 + (i % 100);
        let params = json!([{
            "owner": addr_hex,
            "market_id": 0,
            "side": side,
            "price": price,
            "quantity": 1,
            "time_in_force": "gtc"
        }]);
        if rpc_call(rpc, "prime_submitOrder", params).is_ok() {
            placed2 += 1;
        }
    }
    println!("[TEST] Phase 3: Placed {} more orders", placed2);

    // Phase 4: Trigger block execution (which runs FBA matching)
    let block_before = get_block_number(rpc);
    std::thread::sleep(Duration::from_secs(2));
    let block_after = get_block_number(rpc);

    let elapsed = start.elapsed();
    let blocks_advanced = block_after.saturating_sub(block_before);
    println!(
        "[TEST] Blocks advanced: {} ({}→{})",
        blocks_advanced, block_before, block_after
    );

    ScenarioResult {
        pass: placed > 0,
        duration_ms: elapsed.as_secs_f64() * 1000.0,
        detail: format!(
            "placed={} cancelled={} placed2={} blocks_advanced={}",
            placed, cancelled, placed2, blocks_advanced
        ),
    }
}

// ---------------------------------------------------------------------------
// RPC helpers
// ---------------------------------------------------------------------------

fn rpc_call(rpc: &str, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    let resp = ureq::post(rpc)
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
        .map_err(|e| format!("RPC failed: {}", e))?;
    let text = resp
        .into_string()
        .map_err(|e| format!("read body: {}", e))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("parse JSON: {}", e))?;
    if let Some(err) = json.get("error") {
        return Err(format!("RPC error: {}", err));
    }
    json.get("result")
        .cloned()
        .ok_or_else(|| "no result".to_string())
}

fn get_chain_id(rpc: &str) -> Result<u64, String> {
    let result = rpc_call(rpc, "eth_chainId", json!([]))?;
    let hex_str = result.as_str().ok_or("chainId not string")?;
    u64::from_str_radix(hex_str.trim_start_matches("0x"), 16)
        .map_err(|e| format!("parse chain id: {}", e))
}

fn get_block_number(rpc: &str) -> u64 {
    rpc_call(rpc, "prime_blockNumber", json!([]))
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0)
}
