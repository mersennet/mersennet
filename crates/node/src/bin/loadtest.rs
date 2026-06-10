//! Load test tool for Mersennet.
//! Generates transactions at configurable rates against a running node.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use k256::ecdsa::SigningKey;
use prime_chain::crypto::{address_from_signing_key, encode_raw_signed_tx, sign_transaction};
use prime_chain::engine::Transaction;
use revm::primitives::{Address, Bytes, U256};
use serde_json::{Value, json};

const DEFAULT_RPC: &str = "http://127.0.0.1:8545";
const DEFAULT_RATE: u64 = 100;
const DEFAULT_DURATION: u64 = 60;
const DEFAULT_ACCOUNTS: usize = 10;
const DEFAULT_MODE: &str = "transfer";

fn main() {
    let args = parse_args();

    print_banner(&args);

    let chain_id = match setup_phase(&args.rpc) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("[FATAL] Setup failed: {}", e);
            std::process::exit(1);
        }
    };

    let accounts = generate_accounts(args.accounts);
    println!("[SETUP] Generated {} test accounts", accounts.len());

    if let Err(e) = fund_accounts(&args.rpc, &accounts, chain_id) {
        eprintln!("[WARN] Account funding: {}", e);
    }

    let stats = run_load_test(&args, &accounts, chain_id);
    print_report(&stats, &args);
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct Args {
    rpc: String,
    rate: u64,
    duration: u64,
    accounts: usize,
    mode: String,
}

fn parse_args() -> Args {
    let mut args = Args {
        rpc: DEFAULT_RPC.to_string(),
        rate: DEFAULT_RATE,
        duration: DEFAULT_DURATION,
        accounts: DEFAULT_ACCOUNTS,
        mode: DEFAULT_MODE.to_string(),
    };

    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--rpc" => {
                if let Some(v) = iter.next() {
                    args.rpc = v;
                }
            }
            "--rate" => {
                if let Some(v) = iter.next() {
                    args.rate = v.parse().unwrap_or(DEFAULT_RATE);
                }
            }
            "--duration" => {
                if let Some(v) = iter.next() {
                    args.duration = v.parse().unwrap_or(DEFAULT_DURATION);
                }
            }
            "--accounts" => {
                if let Some(v) = iter.next() {
                    args.accounts = v.parse().unwrap_or(DEFAULT_ACCOUNTS);
                }
            }
            "--mode" => {
                if let Some(v) = iter.next() {
                    args.mode = v;
                }
            }
            "--help" | "-h" => {
                println!("Usage: loadtest [OPTIONS]");
                println!(
                    "  --rpc URL          RPC endpoint (default: {})",
                    DEFAULT_RPC
                );
                println!(
                    "  --rate TPS         Target TPS (default: {})",
                    DEFAULT_RATE
                );
                println!(
                    "  --duration SECS    Test duration (default: {})",
                    DEFAULT_DURATION
                );
                println!(
                    "  --accounts N       Test accounts (default: {})",
                    DEFAULT_ACCOUNTS
                );
                println!(
                    "  --mode MODE        transfer|clob|mixed (default: {})",
                    DEFAULT_MODE
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }

    args
}

// ---------------------------------------------------------------------------
// Banner & setup
// ---------------------------------------------------------------------------

fn print_banner(args: &Args) {
    println!("╔══════════════════════════════════════════════╗");
    println!("║          Mersennet Load Test               ║");
    println!("╚══════════════════════════════════════════════╝");
    println!("[CONFIG] RPC:      {}", args.rpc);
    println!("[CONFIG] Rate:     {} TPS", args.rate);
    println!("[CONFIG] Duration: {}s", args.duration);
    println!("[CONFIG] Accounts: {}", args.accounts);
    println!("[CONFIG] Mode:     {}", args.mode);
    println!();
}

fn setup_phase(rpc: &str) -> Result<u64, String> {
    println!("[SETUP] Connecting to {}", rpc);

    let block_num = rpc_call(rpc, "prime_blockNumber", json!([]))?;
    let block_str = block_num.as_str().unwrap_or("0x0");
    let height = u64::from_str_radix(block_str.trim_start_matches("0x"), 16).unwrap_or(0);
    println!("[SETUP] Chain is running at block {}", height);

    let chain_id_val = rpc_call(rpc, "eth_chainId", json!([]))?;
    let chain_id_str = chain_id_val.as_str().unwrap_or("0x0");
    let chain_id = u64::from_str_radix(chain_id_str.trim_start_matches("0x"), 16).unwrap_or(0);
    println!("[SETUP] Chain ID: {}", chain_id);

    Ok(chain_id)
}

// ---------------------------------------------------------------------------
// Account management
// ---------------------------------------------------------------------------

struct TestAccount {
    key: SigningKey,
    address: Address,
    nonce: AtomicU64,
}

fn generate_accounts(n: usize) -> Vec<Arc<TestAccount>> {
    (0..n)
        .map(|_| {
            let key = SigningKey::random(&mut rand::thread_rng());
            let address = address_from_signing_key(&key);
            Arc::new(TestAccount {
                key,
                address,
                nonce: AtomicU64::new(0),
            })
        })
        .collect()
}

fn fund_accounts(rpc: &str, accounts: &[Arc<TestAccount>], chain_id: u64) -> Result<(), String> {
    println!(
        "[SETUP] Funding {} accounts via prime_sendTransaction...",
        accounts.len()
    );
    for account in accounts {
        let addr_hex = format!("0x{}", hex::encode(account.address.as_slice()));
        let tx_obj = json!({
            "from": "0x0000000000000000000000000000000000000000",
            "to": addr_hex,
            "value": format!("0x{:x}", 10_000_000_000_000u64),
            "gas": "0x5208",
            "gasPrice": "0x1",
            "chainId": format!("0x{:x}", chain_id),
        });
        let _ = rpc_call(rpc, "prime_sendTransaction", json!([tx_obj]));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Load generation
// ---------------------------------------------------------------------------

struct LoadStats {
    submitted: u64,
    confirmed: u64,
    failed: u64,
    latencies_ms: Vec<u64>,
    start: Instant,
    end: Instant,
    blocks_start: u64,
    blocks_end: u64,
}

fn run_load_test(args: &Args, accounts: &[Arc<TestAccount>], chain_id: u64) -> LoadStats {
    let start_block = get_block_number(&args.rpc);
    let start = Instant::now();
    let duration = Duration::from_secs(args.duration);
    let interval = 1_000_000u64
        .checked_div(args.rate)
        .map(Duration::from_micros)
        .unwrap_or(Duration::from_secs(1));

    let submitted = Arc::new(AtomicU64::new(0));
    let confirmed = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let latencies: Arc<std::sync::Mutex<Vec<u64>>> = Arc::new(std::sync::Mutex::new(Vec::new()));

    let progress_submitted = submitted.clone();
    let progress_confirmed = confirmed.clone();
    let progress_failed = failed.clone();
    let progress_start = start;
    let progress_duration = duration;
    let progress_handle = thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(5));
            let elapsed = progress_start.elapsed();
            if elapsed >= progress_duration {
                break;
            }
            let sub = progress_submitted.load(Ordering::Relaxed);
            let conf = progress_confirmed.load(Ordering::Relaxed);
            let fail = progress_failed.load(Ordering::Relaxed);
            let actual_tps = if elapsed.as_secs() > 0 {
                sub / elapsed.as_secs()
            } else {
                sub
            };
            println!(
                "[PROGRESS] {:.0}s elapsed | submitted={} confirmed={} failed={} actual_tps={}",
                elapsed.as_secs_f64(),
                sub,
                conf,
                fail,
                actual_tps
            );
        }
    });

    println!(
        "[LOAD] Starting {} mode for {}s at {} TPS",
        args.mode, args.duration, args.rate
    );

    let mut tx_count = 0u64;
    while start.elapsed() < duration {
        let tick = Instant::now();

        let account_idx = (tx_count as usize) % accounts.len();
        let account = &accounts[account_idx];
        let nonce = account.nonce.fetch_add(1, Ordering::Relaxed);

        let result = match args.mode.as_str() {
            "transfer" => send_transfer(&args.rpc, account, accounts, nonce, chain_id),
            "clob" => send_clob_order(&args.rpc, account, nonce, chain_id, tx_count),
            "mixed" => {
                let r = tx_count % 10;
                if r < 7 {
                    send_transfer(&args.rpc, account, accounts, nonce, chain_id)
                } else if r < 9 {
                    send_clob_order(&args.rpc, account, nonce, chain_id, tx_count)
                } else {
                    send_contract_deploy(&args.rpc, account, nonce, chain_id)
                }
            }
            _ => send_transfer(&args.rpc, account, accounts, nonce, chain_id),
        };

        let latency = tick.elapsed().as_millis() as u64;
        submitted.fetch_add(1, Ordering::Relaxed);

        match result {
            Ok(_) => {
                confirmed.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut lats) = latencies.lock() {
                    lats.push(latency);
                }
            }
            Err(_) => {
                failed.fetch_add(1, Ordering::Relaxed);
            }
        }

        tx_count += 1;

        let elapsed_tick = tick.elapsed();
        if elapsed_tick < interval {
            thread::sleep(interval - elapsed_tick);
        }
    }

    let end = Instant::now();
    let _ = progress_handle.join();

    let end_block = get_block_number(&args.rpc);
    let final_latencies = latencies.lock().unwrap().clone();

    LoadStats {
        submitted: submitted.load(Ordering::Relaxed),
        confirmed: confirmed.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        latencies_ms: final_latencies,
        start,
        end,
        blocks_start: start_block,
        blocks_end: end_block,
    }
}

fn send_transfer(
    rpc: &str,
    from: &TestAccount,
    accounts: &[Arc<TestAccount>],
    nonce: u64,
    chain_id: u64,
) -> Result<String, String> {
    let to_idx = (nonce as usize + 1) % accounts.len();
    let to_addr = accounts[to_idx].address;

    let tx = Transaction {
        from: from.address,
        to: Some(to_addr),
        value: U256::from(1u64),
        data: Bytes::new(),
        gas_limit: 21_000,
        gas_price: U256::from(1u64),
        nonce,
        chain_id: Some(chain_id),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
    };

    let signed = sign_transaction(&tx, &from.key);
    let raw = encode_raw_signed_tx(&signed);
    let raw_hex = format!("0x{}", hex::encode(&raw));
    let result = rpc_call(rpc, "eth_sendRawTransaction", json!([raw_hex]))?;
    Ok(result.as_str().unwrap_or("").to_string())
}

fn send_clob_order(
    rpc: &str,
    account: &TestAccount,
    _nonce: u64,
    _chain_id: u64,
    seq: u64,
) -> Result<String, String> {
    let addr_hex = format!("0x{}", hex::encode(account.address.as_slice()));
    let side = if seq.is_multiple_of(2) { "buy" } else { "sell" };
    let price = 1000 + (seq % 100);
    let quantity = 1 + (seq % 10);

    let params = json!([{
        "owner": addr_hex,
        "market_id": 0,
        "side": side,
        "price": price,
        "quantity": quantity,
        "time_in_force": "gtc"
    }]);

    let result = rpc_call(rpc, "prime_submitOrder", params)?;
    Ok(result.to_string())
}

fn send_contract_deploy(
    rpc: &str,
    from: &TestAccount,
    nonce: u64,
    chain_id: u64,
) -> Result<String, String> {
    let bytecode = vec![
        0x60, 0x0a, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x0a, 0x60, 0x00, 0xf3, 0x60, 0x2a, 0x60,
        0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
    ];

    let tx = Transaction {
        from: from.address,
        to: None,
        value: U256::ZERO,
        data: Bytes::from(bytecode),
        gas_limit: 1_000_000,
        gas_price: U256::from(1u64),
        nonce,
        chain_id: Some(chain_id),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
    };

    let signed = sign_transaction(&tx, &from.key);
    let raw = encode_raw_signed_tx(&signed);
    let raw_hex = format!("0x{}", hex::encode(&raw));
    let result = rpc_call(rpc, "eth_sendRawTransaction", json!([raw_hex]))?;
    Ok(result.as_str().unwrap_or("").to_string())
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

fn print_report(stats: &LoadStats, args: &Args) {
    let duration = stats.end.duration_since(stats.start);
    let actual_tps = if duration.as_secs() > 0 {
        stats.submitted as f64 / duration.as_secs_f64()
    } else {
        stats.submitted as f64
    };
    let confirmation_rate = if stats.submitted > 0 {
        (stats.confirmed as f64 / stats.submitted as f64) * 100.0
    } else {
        0.0
    };

    let (p50, p95, p99) = percentiles(&stats.latencies_ms);
    let blocks_produced = stats.blocks_end.saturating_sub(stats.blocks_start);
    let block_rate = if duration.as_secs() > 0 {
        blocks_produced as f64 / duration.as_secs_f64()
    } else {
        0.0
    };

    println!();
    println!("╔══════════════════════════════════════════════╗");
    println!("║            LOAD TEST RESULTS                 ║");
    println!("╚══════════════════════════════════════════════╝");
    println!("  Mode:                {}", args.mode);
    println!("  Duration:            {:.1}s", duration.as_secs_f64());
    println!("  Target TPS:          {}", args.rate);
    println!("  Actual TPS:          {:.1}", actual_tps);
    println!();
    println!("  Submitted:           {}", stats.submitted);
    println!("  Confirmed:           {}", stats.confirmed);
    println!("  Failed:              {}", stats.failed);
    println!("  Confirmation Rate:   {:.1}%", confirmation_rate);
    println!();
    println!("  Latency P50:         {}ms", p50);
    println!("  Latency P95:         {}ms", p95);
    println!("  Latency P99:         {}ms", p99);
    println!();
    println!("  Blocks Produced:     {}", blocks_produced);
    println!("  Block Rate:          {:.2} blocks/s", block_rate);
    println!("══════════════════════════════════════════════");
}

fn percentiles(latencies: &[u64]) -> (u64, u64, u64) {
    if latencies.is_empty() {
        return (0, 0, 0);
    }
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let len = sorted.len();
    let p50 = sorted[len * 50 / 100];
    let p95 = sorted[(len * 95 / 100).min(len - 1)];
    let p99 = sorted[(len * 99 / 100).min(len - 1)];
    (p50, p95, p99)
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
        .map_err(|e| format!("RPC request failed: {}", e))?;
    let text = resp
        .into_string()
        .map_err(|e| format!("read body: {}", e))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("parse JSON: {}", e))?;
    if let Some(err) = json.get("error") {
        return Err(format!("RPC error: {}", err));
    }
    json.get("result")
        .cloned()
        .ok_or_else(|| "no result in response".to_string())
}

fn get_block_number(rpc: &str) -> u64 {
    rpc_call(rpc, "prime_blockNumber", json!([]))
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0)
}
