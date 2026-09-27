//! Testnet faucet for Mersennet.
//! Serves a simple HTTP endpoint that funds accounts with test tokens.

use k256::ecdsa::SigningKey;
use mersennet::crypto::{encode_raw_signed_tx, sign_transaction};
use mersennet::engine::Transaction;
use revm::primitives::{Address, Bytes, U256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const DEFAULT_PORT: u16 = 8080;
const RATE_LIMIT_HOURS: u64 = 1;
/// Native drips one IP may take in 24 h, whatever the addresses. The hourly
/// address+IP window alone was farmable by rotating addresses from one host
/// (and every farmed MRSN turns into trading points). Three a day leaves a
/// shared office/NAT usable and caps a single host at ~3,000 MRSN/day.
const IP_DAILY_CAP: usize = 3;
const DAY: Duration = Duration::from_secs(24 * 3600);
/// Cloudflare Turnstile. Set TURNSTILE_SECRET (+ TURNSTILE_SITE_KEY for the
/// page) in the service environment to require a passed challenge on every
/// drip; unset = no challenge (current behaviour).
const TURNSTILE_VERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
// 1,000 MRSN (the validator bond / trading collateral) plus 1 MRSN for gas: the
// staking precompile pulls the bond from the balance after gas, so a wallet
// holding exactly 1,000 could never bond 1,000.
const FAUCET_AMOUNT: &str = "1001000000000000000000"; // 1001 tokens (18 decimals)
const GAS_LIMIT: u64 = 21_000;
const TOKEN_GAS_LIMIT: u64 = 200_000;
/// Requests are tiny JSON bodies; cap the read so the public endpoint can't be
/// fed an unbounded stream.
const MAX_BODY_BYTES: u64 = 16 * 1024;
/// Worker threads pulling from the shared server — a single-threaded loop
/// serialized every drip behind sequential RPC round-trips.
const WORKER_THREADS: usize = 8;
/// Shared, purgeable rate-limit ledger: composite key -> last grant time.
type RateLedger = Mutex<HashMap<String, Instant>>;
/// Per-key grant timestamps inside a rolling window (the IP daily cap).
type CountLedger = Mutex<HashMap<String, Vec<Instant>>>;

/// Turnstile settings from the environment; `None` = challenge disabled.
struct Turnstile {
    secret: String,
    site_key: Option<String>,
}
fn turnstile_from_env() -> Option<Turnstile> {
    let secret = std::env::var("TURNSTILE_SECRET")
        .ok()
        .filter(|s| !s.trim().is_empty())?;
    let site_key = std::env::var("TURNSTILE_SITE_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty());
    Some(Turnstile { secret, site_key })
}

/// Server-side check of a Turnstile token (siteverify). A network failure
/// counts as a failed challenge: the faucet stays closed rather than open.
fn turnstile_passes(ts: &Turnstile, token: Option<&str>, ip: &str) -> bool {
    let token = match token.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => t,
        None => return false,
    };
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .build();
    let resp = agent.post(TURNSTILE_VERIFY_URL).send_form(&[
        ("secret", ts.secret.as_str()),
        ("response", token),
        ("remoteip", ip),
    ]);
    let body = match resp
        .map_err(|e| e.to_string())
        .and_then(|r| r.into_string().map_err(|e| e.to_string()))
    {
        Ok(body) => body,
        Err(e) => {
            eprintln!("turnstile: siteverify failed: {e}");
            return false;
        }
    };
    serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| v.get("success").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// Allow at most `cap` grants for `key` inside the trailing `window`; stamps
/// the grant on success. Entries fall out of the map as they age.
fn count_cap_grant(
    ledger: &CountLedger,
    key: &str,
    window: Duration,
    cap: usize,
) -> Result<(), ()> {
    let mut map = ledger.lock().map_err(|_| ())?;
    let now = Instant::now();
    map.retain(|_, v| {
        v.retain(|t| now.duration_since(*t) < window);
        !v.is_empty()
    });
    let entry = map.entry(key.to_string()).or_default();
    if entry.len() >= cap {
        return Err(());
    }
    entry.push(now);
    Ok(())
}

/// `/health` reports "low" below this so an operator gets warned while there
/// is still runway: 500,000 MRSN ≈ 500 drips ≈ a busy day (was 50,000 — fifty
/// drips, an hour's notice at launch traffic). Top-up: deploy/faucet-topup.md.
fn low_balance_threshold() -> U256 {
    U256::from(500_000u64) * U256::from(10u64).pow(U256::from(18))
}
const TOKEN_AMOUNT_6: U256 = U256::from_limbs([10_000_000_000u64, 0, 0, 0]); // 10,000 @ 6 decimals
const TOKEN_AMOUNT_18: U256 = U256::from_limbs([1_864_712_049_423_024_128u64, 542u64, 0, 0]); // 10,000 @ 18 decimals (1e22)

// Redeployed on the 2026-07-06 chain reset (the old addresses died with the
// wiped chain state). Deployer 0x4f42B196A9BfcB2094fBDDFB5635A57D5421cCf7.
const MOCK_USDC: &str = "0xa44b23d1d0c0133da71dece399d5d5ade6dd22d1";
const MOCK_USDT: &str = "0x3923578a19d0e9b35cef08b7eba0cb6d4b9c28f6";
const MOCK_DAI: &str = "0x27942c2cee3e0e02377d01bfe6e74cefc9a9fd45";

#[derive(Debug, Deserialize)]
struct FaucetRequest {
    address: String,
    /// Turnstile response token from the page's widget (required when the
    /// service runs with TURNSTILE_SECRET).
    #[serde(default, rename = "turnstileToken", alias = "cf-turnstile-response")]
    turnstile_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaimTokenRequest {
    address: String,
    token: String,
    #[serde(default, rename = "turnstileToken", alias = "cf-turnstile-response")]
    turnstile_token: Option<String>,
}

#[derive(Debug, Serialize)]
struct FaucetResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl FaucetResponse {
    fn ok(tx_hash: String) -> Self {
        Self {
            success: true,
            tx_hash: Some(tx_hash),
            error: None,
        }
    }
    fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            tx_hash: None,
            error: Some(message.into()),
        }
    }
}

/// Client IP for per-IP limiting. Behind Caddy the real client is the first
/// entry of `X-Forwarded-For`; fall back to the socket peer for direct hits.
fn client_ip(request: &Request) -> String {
    // Prefer CF-Connecting-IP: Cloudflare sets and overwrites it with the real
    // client, so it can't be forged through CF. This is the per-IP limit's
    // trusted source.
    for h in request.headers() {
        if h.field.equiv("CF-Connecting-IP") {
            let v = h.value.as_str().trim();
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    // Fallback: the LAST X-Forwarded-For hop (appended by our own proxy). The
    // FIRST entry is client-controlled, so using it would let an attacker
    // rotate the per-IP bucket with a spoofed header and drain the faucet.
    for h in request.headers() {
        if h.field.equiv("X-Forwarded-For")
            && let Some(last) = h.value.as_str().split(',').next_back()
            && !last.trim().is_empty()
        {
            return last.trim().to_string();
        }
    }
    request
        .remote_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Grant if every key is outside the window, then stamp all keys with now.
/// Returns Err(()) if any key is still rate-limited. Opportunistically purges
/// entries older than 2× the window so the map can't grow without bound.
fn rate_limit_grant(ledger: &RateLedger, keys: &[String], window: Duration) -> Result<(), ()> {
    let mut map = ledger.lock().map_err(|_| ())?;
    let now = Instant::now();
    map.retain(|_, t| now.duration_since(*t) < window * 2);
    if keys
        .iter()
        .any(|k| map.get(k).is_some_and(|t| now.duration_since(*t) < window))
    {
        return Err(());
    }
    for k in keys {
        map.insert(k.clone(), now);
    }
    Ok(())
}

fn json_response(status: u16, resp: &FaucetResponse) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_string(resp).unwrap_or_else(|_| "{}".to_string());
    Response::from_string(body)
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
}

fn read_body_capped(request: &mut Request) -> String {
    let mut body = String::new();
    let _ = request
        .as_reader()
        .take(MAX_BODY_BYTES)
        .read_to_string(&mut body);
    body
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let rpc_url = args.rpc_url.clone();
    let port = args.port;

    let faucet_key = load_faucet_key(&args.private_key)?;
    let faucet_address = mersennet::crypto::address_from_signing_key(&faucet_key);
    let chain_id = fetch_chain_id(&rpc_url)?;
    let rate_limit: Arc<RateLedger> = Arc::new(Mutex::new(HashMap::new()));
    let daily_ip: Arc<CountLedger> = Arc::new(Mutex::new(HashMap::new()));
    let turnstile: Arc<Option<Turnstile>> = Arc::new(turnstile_from_env());
    match turnstile.as_ref() {
        Some(t) => println!(
            "Turnstile: on (site key {})",
            if t.site_key.is_some() {
                "set"
            } else {
                "MISSING — the page cannot render the widget"
            }
        ),
        None => println!(
            "Turnstile: off (set TURNSTILE_SECRET and TURNSTILE_SITE_KEY to require a challenge)"
        ),
    }
    println!(
        "Limits: 1 drip per address per {RATE_LIMIT_HOURS} h, {IP_DAILY_CAP} drips per IP per 24 h"
    );
    // Locally tracked next nonce for the faucet account. Re-reading "latest"
    // alone races when multiple txs (drip + token claims) are still unmined, so
    // we hand out strictly increasing nonces via reserve_nonces().
    let nonce_state: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    let server = Arc::new(
        Server::http(format!("0.0.0.0:{}", port)).map_err(|e| format!("failed to bind: {}", e))?,
    );
    println!("Faucet listening on http://0.0.0.0:{}", port);
    println!("RPC: {}", rpc_url);
    println!("Workers: {}", WORKER_THREADS);
    println!(
        "Faucet address: 0x{}",
        hex::encode(faucet_address.as_slice())
    );

    // A small worker pool: each thread pulls requests from the shared server so
    // one slow drip (several sequential RPC round-trips) can't stall the queue.
    let mut handles = Vec::new();
    for _ in 0..WORKER_THREADS {
        let server = Arc::clone(&server);
        let rpc_url = rpc_url.clone();
        let faucet_key = faucet_key.clone();
        let rate_limit = Arc::clone(&rate_limit);
        let daily_ip = Arc::clone(&daily_ip);
        let turnstile = Arc::clone(&turnstile);
        let nonce_state = Arc::clone(&nonce_state);
        handles.push(std::thread::spawn(move || {
            while let Ok(request) = server.recv() {
                let _ = handle_request(
                    request,
                    &rpc_url,
                    &faucet_key,
                    chain_id,
                    &rate_limit,
                    &daily_ip,
                    turnstile.as_ref().as_ref(),
                    &nonce_state,
                );
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_request(
    mut request: Request,
    rpc_url: &str,
    faucet_key: &SigningKey,
    chain_id: u64,
    rate_limit: &RateLedger,
    daily_ip: &CountLedger,
    turnstile: Option<&Turnstile>,
    nonce_state: &Mutex<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = request.url().to_string();
    let ip = client_ip(&request);

    // What the page needs to render itself: the Turnstile site key (null =
    // no challenge) and the limits it should explain to the user.
    if request.method() == &Method::Get && url == "/config" {
        let body = json!({
            "turnstileSiteKey": turnstile.and_then(|t| t.site_key.clone()),
            "dripMrsn": 1001,
            "addressWindowHours": RATE_LIMIT_HOURS,
            "ipDailyCap": IP_DAILY_CAP,
        })
        .to_string();
        let response = Response::from_string(body)
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
        request.respond(response)?;
        return Ok(());
    }

    if request.method() == &Method::Get && (url == "/" || url == "/faucet") {
        let html = include_str!("faucet.html");
        let response = Response::from_string(html)
            .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
        request.respond(response)?;
        return Ok(());
    }

    if request.method() == &Method::Get && url == "/health" {
        let faucet_address = mersennet::crypto::address_from_signing_key(faucet_key);
        let body = match fetch_balance(rpc_url, &faucet_address) {
            Ok(bal) => json!({
                "status": if bal >= low_balance_threshold() { "ok" } else { "low" },
                "faucetAddress": format!("0x{}", hex::encode(faucet_address.as_slice())),
                "faucetBalanceWei": bal.to_string(),
                "dripsLeft": (bal / (U256::from(1001u64) * U256::from(10u64).pow(U256::from(18)))).to_string(),
                "turnstile": turnstile.is_some(),
                "ipDailyCap": IP_DAILY_CAP,
            }),
            // RPC unreachable: report degraded rather than a bare "ok".
            Err(e) => json!({ "status": "degraded", "error": e.to_string() }),
        }
        .to_string();
        let response = Response::from_string(body)
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
        request.respond(response)?;
        return Ok(());
    }

    if request.method() == &Method::Post && url == "/faucet" {
        let body = read_body_capped(&mut request);
        let req: FaucetRequest = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(_) => {
                request.respond(json_response(
                    400,
                    &FaucetResponse::err("invalid JSON: expected {\"address\": \"0x...\"}"),
                ))?;
                return Ok(());
            }
        };

        let address = match parse_address(&req.address) {
            Ok(a) => a,
            Err(e) => {
                request.respond(json_response(
                    400,
                    &FaucetResponse::err(format!("invalid address: {}", e)),
                ))?;
                return Ok(());
            }
        };

        if let Some(ts) = turnstile
            && !turnstile_passes(ts, req.turnstile_token.as_deref(), &ip)
        {
            request.respond(json_response(
                403,
                &FaucetResponse::err("challenge failed — reload the page and try again"),
            ))?;
            return Ok(());
        }

        let addr_key = format!("native:{}", hex::encode(address.as_slice()));
        let ip_key = format!("native-ip:{}", ip);
        let window = Duration::from_secs(RATE_LIMIT_HOURS * 3600);
        if rate_limit_grant(rate_limit, &[addr_key, ip_key], window).is_err() {
            request.respond(json_response(
                429,
                &FaucetResponse::err(format!(
                    "rate limited: 1 native drip per address and per IP per {} hour(s)",
                    RATE_LIMIT_HOURS
                )),
            ))?;
            return Ok(());
        }
        if count_cap_grant(daily_ip, &format!("native-ip-day:{ip}"), DAY, IP_DAILY_CAP).is_err() {
            request.respond(json_response(
                429,
                &FaucetResponse::err(format!(
                    "rate limited: {IP_DAILY_CAP} native drips per IP per 24 hours — that is {} MRSN a day, plenty for testing",
                    IP_DAILY_CAP * 1001
                )),
            ))?;
            return Ok(());
        }

        let faucet_address = mersennet::crypto::address_from_signing_key(faucet_key);
        let nonce = reserve_nonces(nonce_state, rpc_url, &faucet_address, 1)?;
        let gas_price = fetch_gas_price(rpc_url)?;

        let tx = Transaction {
            from: faucet_address,
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

        let signed = sign_transaction(&tx, faucet_key);
        let raw_hex = format!("0x{}", hex::encode(encode_raw_signed_tx(&signed)));

        match send_raw_transaction(rpc_url, &raw_hex) {
            Ok(tx_hash) => request.respond(json_response(200, &FaucetResponse::ok(tx_hash)))?,
            Err(e) => request.respond(json_response(500, &FaucetResponse::err(e.to_string())))?,
        }
        return Ok(());
    }

    if request.method() == &Method::Post && url == "/claim-token" {
        let body = read_body_capped(&mut request);
        // Parse first so the rate-limit key can be scoped to (address, token) —
        // this endpoint previously had NO limit at all.
        let req: ClaimTokenRequest = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(_) => {
                request.respond(json_response(
                    400,
                    &FaucetResponse::err(
                        "expected {\"address\":\"0x...\",\"token\":\"usdc|usdt|dai\"}",
                    ),
                ))?;
                return Ok(());
            }
        };
        if let Some(ts) = turnstile
            && !turnstile_passes(ts, req.turnstile_token.as_deref(), &ip)
        {
            request.respond(json_response(
                403,
                &FaucetResponse::err("challenge failed — reload the page and try again"),
            ))?;
            return Ok(());
        }
        let token = req.token.to_lowercase();
        let addr_norm = req.address.trim().to_lowercase();
        let addr_key = format!("token:{}:{}", token, addr_norm);
        let ip_key = format!("token-ip:{}:{}", token, ip);
        let window = Duration::from_secs(RATE_LIMIT_HOURS * 3600);
        if rate_limit_grant(rate_limit, &[addr_key, ip_key], window).is_err() {
            request.respond(json_response(
                429,
                &FaucetResponse::err(format!(
                    "rate limited: 1 {} claim per address and per IP per {} hour(s)",
                    token, RATE_LIMIT_HOURS
                )),
            ))?;
            return Ok(());
        }

        let resp = handle_claim_token(&req, rpc_url, faucet_key, chain_id, nonce_state);
        let status = if resp.success { 200 } else { 500 };
        request.respond(json_response(status, &resp))?;
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

fn fetch_balance(rpc_url: &str, address: &Address) -> Result<U256, Box<dyn std::error::Error>> {
    let addr_hex = format!("0x{}", hex::encode(address.as_slice()));
    let result = rpc_request(rpc_url, "eth_getBalance", json!([addr_hex, "latest"]))?;
    let hex_str = result.as_str().ok_or("balance not string")?;
    Ok(U256::from_str_radix(hex_str.trim_start_matches("0x"), 16)?)
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
    req: &ClaimTokenRequest,
    rpc_url: &str,
    faucet_key: &SigningKey,
    chain_id: u64,
    nonce_state: &Mutex<u64>,
) -> FaucetResponse {
    let user_address = match parse_address(&req.address) {
        Ok(a) => a,
        Err(e) => return FaucetResponse::err(format!("invalid address: {}", e)),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_blocks_repeat_within_window() {
        let ledger: RateLedger = Mutex::new(HashMap::new());
        let window = Duration::from_secs(3600);
        let keys = vec!["native:aa".to_string(), "native-ip:1.2.3.4".to_string()];
        // First grant succeeds.
        assert!(rate_limit_grant(&ledger, &keys, window).is_ok());
        // Immediate repeat on the same keys is blocked.
        assert!(rate_limit_grant(&ledger, &keys, window).is_err());
    }

    #[test]
    fn rate_limit_blocks_when_only_ip_matches() {
        let ledger: RateLedger = Mutex::new(HashMap::new());
        let window = Duration::from_secs(3600);
        // Address A from IP X.
        assert!(
            rate_limit_grant(
                &ledger,
                &["native:a".to_string(), "native-ip:9.9.9.9".to_string()],
                window
            )
            .is_ok()
        );
        // Different address B but the SAME IP — must be blocked (per-IP limit).
        assert!(
            rate_limit_grant(
                &ledger,
                &["native:b".to_string(), "native-ip:9.9.9.9".to_string()],
                window
            )
            .is_err()
        );
    }

    #[test]
    fn rate_limit_independent_keys_are_allowed() {
        let ledger: RateLedger = Mutex::new(HashMap::new());
        let window = Duration::from_secs(3600);
        // Native drip and a USDC token claim use disjoint key namespaces, so a
        // user can do both — but each is limited within its own namespace.
        assert!(
            rate_limit_grant(
                &ledger,
                &["native:a".to_string(), "native-ip:1.1.1.1".to_string()],
                window
            )
            .is_ok()
        );
        assert!(
            rate_limit_grant(
                &ledger,
                &[
                    "token:usdc:a".to_string(),
                    "token-ip:usdc:1.1.1.1".to_string()
                ],
                window
            )
            .is_ok()
        );
        assert!(
            rate_limit_grant(
                &ledger,
                &[
                    "token:usdt:a".to_string(),
                    "token-ip:usdt:1.1.1.1".to_string()
                ],
                window
            )
            .is_ok()
        );
    }

    #[test]
    fn rate_limit_grants_again_after_window() {
        let ledger: RateLedger = Mutex::new(HashMap::new());
        let window = Duration::from_millis(50);
        let keys = vec!["native:z".to_string()];
        assert!(rate_limit_grant(&ledger, &keys, window).is_ok());
        assert!(rate_limit_grant(&ledger, &keys, window).is_err());
        std::thread::sleep(Duration::from_millis(60));
        // Window elapsed — allowed again (and the stale entry is purged).
        assert!(rate_limit_grant(&ledger, &keys, window).is_ok());
    }

    #[test]
    fn low_balance_threshold_is_50k_ether() {
        let expected = U256::from(50_000u64) * U256::from(10u64).pow(U256::from(18));
        assert_eq!(low_balance_threshold(), expected);
        assert_eq!(expected.to_string(), "50000000000000000000000");
    }
}
