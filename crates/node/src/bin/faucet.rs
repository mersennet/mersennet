//! Testnet faucet for Prime Chain.
//! Serves a simple HTTP endpoint that funds accounts with test tokens.

use k256::ecdsa::SigningKey;
use prime_chain::crypto::{encode_raw_signed_tx, sign_transaction};
use prime_chain::engine::Transaction;
use revm::primitives::{Address, Bytes, U256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const DEFAULT_PORT: u16 = 8080;
const RATE_LIMIT_HOURS: u64 = 1;
const FAUCET_AMOUNT: &str = "1000000000000000000000"; // 1000 tokens (18 decimals)
const GAS_LIMIT: u64 = 21_000;

#[derive(Debug, Deserialize)]
struct FaucetRequest {
    address: String,
}

#[derive(Debug, Serialize)]
struct FaucetResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

struct RateLimit {
    last_request: Instant,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let rpc_url = args.rpc_url.clone();
    let port = args.port;

    let faucet_key = load_faucet_key(&args.private_key)?;
    let faucet_address = prime_chain::crypto::address_from_signing_key(&faucet_key);
    let chain_id = fetch_chain_id(&rpc_url)?;
    let rate_limit: Mutex<HashMap<String, RateLimit>> = Mutex::new(HashMap::new());

    let server = Server::http(format!("0.0.0.0:{}", port))
        .map_err(|e| format!("failed to bind: {}", e))?;
    println!("Faucet listening on http://0.0.0.0:{}", port);
    println!("RPC: {}", rpc_url);
    println!("Faucet address: 0x{}", hex::encode(faucet_address.as_slice()));

    for request in server.incoming_requests() {
        let rpc_url = rpc_url.clone();
        let faucet_key = faucet_key.clone();
        let rate_limit = &rate_limit;
        let _ = handle_request(request, rpc_url, faucet_key, chain_id, rate_limit);
    }

    Ok(())
}

fn handle_request(
    mut request: Request,
    rpc_url: String,
    faucet_key: SigningKey,
    chain_id: u64,
    rate_limit: &Mutex<HashMap<String, RateLimit>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = request.url().to_string();

    if request.method() == &Method::Get && (url == "/" || url == "/faucet") {
        let html = include_str!("faucet.html");
        let response = Response::from_string(html)
            .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
        request.respond(response)?;
        return Ok(());
    }

    if request.method() == &Method::Get && url == "/health" {
        let body = json!({ "status": "ok" }).to_string();
        let response = Response::from_string(body)
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
        request.respond(response)?;
        return Ok(());
    }

    if request.method() == &Method::Post && url == "/faucet" {
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body)?;
        let req: FaucetRequest = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(_) => {
                let resp = FaucetResponse {
                    success: false,
                    tx_hash: None,
                    error: Some("invalid JSON: expected {\"address\": \"0x...\"}".to_string()),
                };
                let response = Response::from_string(serde_json::to_string(&resp)?)
                    .with_status_code(400)
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
                request.respond(response)?;
                return Ok(());
            }
        };

        let address = match parse_address(&req.address) {
            Ok(a) => a,
            Err(e) => {
                let resp = FaucetResponse {
                    success: false,
                    tx_hash: None,
                    error: Some(format!("invalid address: {}", e)),
                };
                let response = Response::from_string(serde_json::to_string(&resp)?)
                    .with_status_code(400)
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
                request.respond(response)?;
                return Ok(());
            }
        };

        let addr_key = format!("0x{}", hex::encode(address.as_slice()));
        {
            let mut rl = rate_limit.lock().map_err(|_| "lock poisoned")?;
            if let Some(entry) = rl.get(&addr_key) {
                if entry.last_request.elapsed() < Duration::from_secs(RATE_LIMIT_HOURS * 3600) {
                    let resp = FaucetResponse {
                        success: false,
                        tx_hash: None,
                        error: Some(format!(
                            "rate limited: 1 request per address per {} hour(s)",
                            RATE_LIMIT_HOURS
                        )),
                    };
                    let response = Response::from_string(serde_json::to_string(&resp)?)
                        .with_status_code(429)
                        .with_header(
                            Header::from_bytes("Content-Type", "application/json").unwrap(),
                        );
                    request.respond(response)?;
                    return Ok(());
                }
            }
            rl.insert(addr_key.clone(), RateLimit { last_request: Instant::now() });
        }

        let nonce = fetch_nonce(&rpc_url, &prime_chain::crypto::address_from_signing_key(&faucet_key))?;
        let gas_price = fetch_gas_price(&rpc_url)?;

        let tx = Transaction {
            from: prime_chain::crypto::address_from_signing_key(&faucet_key),
            to: Some(address),
            value: U256::from_str_radix(FAUCET_AMOUNT.trim_start_matches("0x"), 10)
                .unwrap_or_else(|_| U256::from(1000u64) * U256::from(10u64).pow(U256::from(18))),
            data: Bytes::new(),
            gas_limit: GAS_LIMIT,
            gas_price,
            nonce,
            chain_id: Some(chain_id),
            signature: None,
        };

        let signed = sign_transaction(&tx, &faucet_key);
        let raw = encode_raw_signed_tx(&signed);
        let raw_hex = format!("0x{}", hex::encode(&raw));

        match send_raw_transaction(&rpc_url, &raw_hex) {
            Ok(tx_hash) => {
                let resp = FaucetResponse {
                    success: true,
                    tx_hash: Some(tx_hash),
                    error: None,
                };
                let response = Response::from_string(serde_json::to_string(&resp)?)
                    .with_header(
                        Header::from_bytes("Content-Type", "application/json").unwrap(),
                    );
                request.respond(response)?;
            }
            Err(e) => {
                let resp = FaucetResponse {
                    success: false,
                    tx_hash: None,
                    error: Some(e.to_string()),
                };
                let response = Response::from_string(serde_json::to_string(&resp)?)
                    .with_status_code(500)
                    .with_header(
                        Header::from_bytes("Content-Type", "application/json").unwrap(),
                    );
                request.respond(response)?;
            }
        }
        return Ok(());
    }

    let response = Response::from_string("Not Found").with_status_code(404);
    request.respond(response)?;
    Ok(())
}

fn parse_address(s: &str) -> Result<Address, String> {
    let stripped = s.trim().strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(stripped).map_err(|e| e.to_string())?;
    if bytes.len() != 20 {
        return Err("address must be 20 bytes".to_string());
    }
    Ok(Address::from_slice(&bytes))
}

fn load_faucet_key(path: &str) -> Result<SigningKey, Box<dyn std::error::Error>> {
    let data = std::fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&data)?;
    let hex_str = json
        .get("private_key")
        .or_else(|| json.get("privateKey"))
        .and_then(|v| v.as_str())
        .ok_or("missing private_key in JSON")?;
    let bytes = hex::decode(hex_str.trim_start_matches("0x"))?;
    Ok(SigningKey::from_slice(&bytes)?)
}

fn rpc_request(rpc_url: &str, method: &str, params: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    let client = ureq::Agent::new();
    let resp = client
        .post(rpc_url)
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())?;
    let body_str = resp.into_string()?;
    let json: Value = serde_json::from_str(&body_str)?;
    if let Some(err) = json.get("error") {
        return Err(format!("RPC error: {}", err).into());
    }
    json.get("result")
        .cloned()
        .ok_or_else(|| "no result in RPC response".into())
}

fn fetch_chain_id(rpc_url: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let result = rpc_request(rpc_url, "eth_chainId", json!([]))?;
    let hex_str = result.as_str().ok_or("chainId not string")?;
    let n = u64::from_str_radix(hex_str.trim_start_matches("0x"), 16)?;
    Ok(n)
}

fn fetch_nonce(rpc_url: &str, address: &Address) -> Result<u64, Box<dyn std::error::Error>> {
    let addr_hex = format!("0x{}", hex::encode(address.as_slice()));
    let result = rpc_request(rpc_url, "eth_getTransactionCount", json!([addr_hex, "latest"]))?;
    let hex_str = result.as_str().ok_or("nonce not string")?;
    Ok(u64::from_str_radix(hex_str.trim_start_matches("0x"), 16)?)
}

fn fetch_gas_price(rpc_url: &str) -> Result<U256, Box<dyn std::error::Error>> {
    let result = rpc_request(rpc_url, "eth_gasPrice", json!([]))?;
    let hex_str = result.as_str().ok_or("gasPrice not string")?;
    Ok(U256::from_str_radix(hex_str.trim_start_matches("0x"), 16)?)
}

fn send_raw_transaction(rpc_url: &str, raw_hex: &str) -> Result<String, Box<dyn std::error::Error>> {
    let result = rpc_request(rpc_url, "eth_sendRawTransaction", json!([raw_hex]))?;
    Ok(result.as_str().ok_or("tx hash not string")?.to_string())
}

struct Args {
    port: u16,
    rpc_url: String,
    private_key: String,
}

fn parse_args() -> Args {
    let mut port = DEFAULT_PORT;
    let mut rpc_url = "http://localhost:8545".to_string();
    let mut private_key = "faucet-key.json".to_string();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                if let Some(p) = args.next() {
                    port = p.parse().unwrap_or(DEFAULT_PORT);
                }
            }
            "--rpc-url" => {
                if let Some(u) = args.next() {
                    rpc_url = u;
                }
            }
            "--private-key" => {
                if let Some(k) = args.next() {
                    private_key = k;
                }
            }
            "--help" | "-h" => {
                println!("Usage: faucet [OPTIONS]");
                println!("  --port PORT         Listen port (default: {})", DEFAULT_PORT);
                println!("  --rpc-url URL       Prime Chain RPC URL (default: http://localhost:8545)");
                println!("  --private-key PATH  Path to faucet key JSON (default: faucet-key.json)");
                std::process::exit(0);
            }
            _ => {}
        }
    }

    Args {
        port,
        rpc_url,
        private_key,
    }
}
