//! Testnet faucet for Mersennet.
//! Serves a simple HTTP endpoint that funds accounts with test tokens.

use k256::ecdsa::SigningKey;
use mersennet::crypto::{encode_raw_signed_tx, sign_transaction};
use mersennet::engine::Transaction;
use revm::primitives::{Address, Bytes, U256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const DEFAULT_PORT: u16 = 8080;
const RATE_LIMIT_HOURS: u64 = 1;
const FAUCET_AMOUNT: &str = "1000000000000000000000"; // 1000 tokens (18 decimals)
const GAS_LIMIT: u64 = 21_000;
const TOKEN_GAS_LIMIT: u64 = 200_000;
const TOKEN_AMOUNT_6: U256 = U256::from_limbs([10_000_000_000u64, 0, 0, 0]); // 10,000 @ 6 decimals
const TOKEN_AMOUNT_18: U256 = U256::from_limbs([1_864_712_049_423_024_128u64, 542u64, 0, 0]); // 10,000 @ 18 decimals (1e22)

const MOCK_USDC: &str = "0x2e06b6e7479ddf54b46458b5a61f302d962957ea";
const MOCK_USDT: &str = "0x7cfd9b3e373c3f4fd34aed80d7ae083fc0b20eb7";
const MOCK_DAI: &str = "0x4359446ffb3e262294923ec61f35769ce62fa5ad";

#[derive(Debug, Deserialize)]
struct FaucetRequest {
    address: String,
}

#[derive(Debug, Deserialize)]
struct ClaimTokenRequest {
    address: String,
    token: String,
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
    let faucet_address = mersennet::crypto::address_from_signing_key(&faucet_key);
    let chain_id = fetch_chain_id(&rpc_url)?;
    let rate_limit: Mutex<HashMap<String, RateLimit>> = Mutex::new(HashMap::new());
    // Locally tracked next nonce for the faucet account. Re-reading "latest"
    // alone races when multiple txs (drip + token claims) are still unmined, so
    // we hand out strictly increasing nonces via reserve_nonces().
    let nonce_state: Mutex<u64> = Mutex::new(0);

    let server =
        Server::http(format!("0.0.0.0:{}", port)).map_err(|e| format!("failed to bind: {}", e))?;
    println!("Faucet listening on http://0.0.0.0:{}", port);
    println!("RPC: {}", rpc_url);
    println!(
        "Faucet address: 0x{}",
        hex::encode(faucet_address.as_slice())
    );

    for request in server.incoming_requests() {
        let rpc_url = rpc_url.clone();
        let faucet_key = faucet_key.clone();
        let rate_limit = &rate_limit;
        let nonce_state = &nonce_state;
        let _ = handle_request(
            request,
            rpc_url,
            faucet_key,
            chain_id,
            rate_limit,
            nonce_state,
        );
    }

    Ok(())
}

fn handle_request(
    mut request: Request,
    rpc_url: String,
    faucet_key: SigningKey,
    chain_id: u64,
    rate_limit: &Mutex<HashMap<String, RateLimit>>,
    nonce_state: &Mutex<u64>,
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
            if let Some(entry) = rl.get(&addr_key)
                && entry.last_request.elapsed() < Duration::from_secs(RATE_LIMIT_HOURS * 3600)
            {
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
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
                request.respond(response)?;
                return Ok(());
            }
            rl.insert(
                addr_key.clone(),
                RateLimit {
                    last_request: Instant::now(),
                },
            );
        }

        let nonce = reserve_nonces(
            nonce_state,
            &rpc_url,
            &mersennet::crypto::address_from_signing_key(&faucet_key),
            1,
        )?;
        let gas_price = fetch_gas_price(&rpc_url)?;

        let tx = Transaction {
            from: mersennet::crypto::address_from_signing_key(&faucet_key),
            to: Some(address),
            value: U256::from_str_radix(FAUCET_AMOUNT.trim_start_matches("0x"), 10)
                .unwrap_or_else(|_| U256::from(1000u64) * U256::from(10u64).pow(U256::from(18))),
            data: Bytes::new(),
            gas_limit: GAS_LIMIT,
            gas_price,
            nonce,
            chain_id: Some(chain_id),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
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
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
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
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
                request.respond(response)?;
            }
        }
        return Ok(());
    }

    if request.method() == &Method::Post && url == "/claim-token" {
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body)?;

        let resp = handle_claim_token(&body, &rpc_url, &faucet_key, chain_id, nonce_state);
        let status = if resp.success { 200 } else { 500 };
        let response = Response::from_string(serde_json::to_string(&resp)?)
            .with_status_code(status)
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
        request.respond(response)?;
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

fn rpc_request(
    rpc_url: &str,
    method: &str,
    params: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
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
    let result = rpc_request(
        rpc_url,
        "eth_getTransactionCount",
        json!([addr_hex, "latest"]),
    )?;
    let hex_str = result.as_str().ok_or("nonce not string")?;
    Ok(u64::from_str_radix(hex_str.trim_start_matches("0x"), 16)?)
}

/// Reserve `count` consecutive nonces for the faucet account, returning the
/// first. Uses `max(locally tracked, chain "latest")` so it self-heals after
/// restarts/failures while staying ahead of still-unmined txs.
fn reserve_nonces(
    state: &Mutex<u64>,
    rpc_url: &str,
    address: &Address,
    count: u64,
) -> Result<u64, Box<dyn std::error::Error>> {
    let chain = fetch_nonce(rpc_url, address)?;
    let mut g = state.lock().map_err(|_| "nonce lock poisoned")?;
    let start = (*g).max(chain);
    *g = start + count;
    Ok(start)
}

fn fetch_gas_price(rpc_url: &str) -> Result<U256, Box<dyn std::error::Error>> {
    let result = rpc_request(rpc_url, "eth_gasPrice", json!([]))?;
    let hex_str = result.as_str().ok_or("gasPrice not string")?;
    Ok(U256::from_str_radix(hex_str.trim_start_matches("0x"), 16)?)
}

fn send_raw_transaction(
    rpc_url: &str,
    raw_hex: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let result = rpc_request(rpc_url, "eth_sendRawTransaction", json!([raw_hex]))?;
    Ok(result.as_str().ok_or("tx hash not string")?.to_string())
}

fn handle_claim_token(
    body: &str,
    rpc_url: &str,
    faucet_key: &SigningKey,
    chain_id: u64,
    nonce_state: &Mutex<u64>,
) -> FaucetResponse {
    let req: ClaimTokenRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(_) => {
            return FaucetResponse {
                success: false,
                tx_hash: None,
                error: Some("expected {\"address\":\"0x...\",\"token\":\"usdc|usdt|dai\"}".into()),
            };
        }
    };

    let user_address = match parse_address(&req.address) {
        Ok(a) => a,
        Err(e) => {
            return FaucetResponse {
                success: false,
                tx_hash: None,
                error: Some(format!("invalid address: {}", e)),
            };
        }
    };

    // (contract, transfer amount) — faucet() mints 10,000 tokens scaled to the
    // token's own decimals, so the transfer must match (6 dec for USDC/USDT,
    // 18 dec for DAI) or DAI would only deliver dust.
    let (token_contract, transfer_amount) = match req.token.to_lowercase().as_str() {
        "usdc" => (parse_address(MOCK_USDC).unwrap(), TOKEN_AMOUNT_6),
        "usdt" => (parse_address(MOCK_USDT).unwrap(), TOKEN_AMOUNT_6),
        "dai" => (parse_address(MOCK_DAI).unwrap(), TOKEN_AMOUNT_18),
        _ => {
            return FaucetResponse {
                success: false,
                tx_hash: None,
                error: Some("unknown token: use usdc, usdt, or dai".into()),
            };
        }
    };

    let faucet_address = mersennet::crypto::address_from_signing_key(faucet_key);
    let gas_price = match fetch_gas_price(rpc_url) {
        Ok(p) => p,
        Err(e) => {
            return FaucetResponse {
                success: false,
                tx_hash: None,
                error: Some(e.to_string()),
            };
        }
    };
    // Reserve two consecutive nonces (mint + transfer) so rapid back-to-back
    // claims can't collide on the shared faucet account.
    let nonce = match reserve_nonces(nonce_state, rpc_url, &faucet_address, 2) {
        Ok(n) => n,
        Err(e) => {
            return FaucetResponse {
                success: false,
                tx_hash: None,
                error: Some(e.to_string()),
            };
        }
    };

    // Step 1: Call faucet() on the token contract (mints to faucet address)
    let mint_tx = Transaction {
        from: faucet_address,
        to: Some(token_contract),
        value: U256::ZERO,
        data: Bytes::from(hex::decode("de5f72fd").unwrap()),
        gas_limit: TOKEN_GAS_LIMIT,
        gas_price,
        nonce,
        chain_id: Some(chain_id),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
        hash: None,
    };
    let signed_mint = sign_transaction(&mint_tx, faucet_key);
    let raw_mint = format!("0x{}", hex::encode(encode_raw_signed_tx(&signed_mint)));
    if let Err(e) = send_raw_transaction(rpc_url, &raw_mint) {
        return FaucetResponse {
            success: false,
            tx_hash: None,
            error: Some(format!("mint failed: {}", e)),
        };
    }

    // Step 2: Transfer tokens to user
    let transfer_data = encode_transfer(user_address, transfer_amount);
    let transfer_tx = Transaction {
        from: faucet_address,
        to: Some(token_contract),
        value: U256::ZERO,
        data: Bytes::from(transfer_data),
        gas_limit: TOKEN_GAS_LIMIT,
        gas_price,
        nonce: nonce + 1,
        chain_id: Some(chain_id),
        signature: None,
        tx_type: 0,
        shielded_payload: None,
        hash: None,
    };
    let signed_transfer = sign_transaction(&transfer_tx, faucet_key);
    let raw_transfer = format!("0x{}", hex::encode(encode_raw_signed_tx(&signed_transfer)));
    match send_raw_transaction(rpc_url, &raw_transfer) {
        Ok(tx_hash) => FaucetResponse {
            success: true,
            tx_hash: Some(tx_hash),
            error: None,
        },
        Err(e) => FaucetResponse {
            success: false,
            tx_hash: None,
            error: Some(format!("transfer failed: {}", e)),
        },
    }
}

fn encode_transfer(to: Address, amount: U256) -> Vec<u8> {
    // transfer(address,uint256) = 0xa9059cbb
    let mut data = vec![0xa9, 0x05, 0x9c, 0xbb];
    // address padded to 32 bytes
    data.extend_from_slice(&[0u8; 12]);
    data.extend_from_slice(to.as_slice());
    // uint256 amount
    data.extend_from_slice(&amount.to_be_bytes::<32>());
    data
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
                println!(
                    "  --port PORT         Listen port (default: {})",
                    DEFAULT_PORT
                );
                println!(
                    "  --rpc-url URL       Mersennet RPC URL (default: http://localhost:8545)"
                );
                println!(
                    "  --private-key PATH  Path to faucet key JSON (default: faucet-key.json)"
                );
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
