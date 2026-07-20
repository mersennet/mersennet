use crate::rpc_router;
use anyhow::{Result, anyhow};
use mersennet::bridge::BridgeDomain;
use mersennet::engine::{Block, Engine, LogEntry, Receipt, Transaction};
use mersennet::errors::RpcInputError;
use mersennet::events::{BridgeEvent, BridgeQueueKind, DomainEvent, MersennetOrdersEvent};
use mersennet::prometheus;
use revm::primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Response, Server};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Filter state for eth_newFilter / eth_getFilterChanges / eth_uninstallFilter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum FilterKind {
    Log(LogFilter),
    Block,
    PendingTx,
}

#[derive(Debug)]
struct InstalledFilter {
    kind: FilterKind,
    last_poll_block: u64,
}

type FilterStore = Arc<Mutex<FilterState>>;

#[derive(Debug)]
struct FilterState {
    filters: HashMap<u64, InstalledFilter>,
    next_id: u64,
}

impl FilterState {
    fn new() -> Self {
        Self {
            filters: HashMap::new(),
            next_id: 1,
        }
    }

    fn install(&mut self, kind: FilterKind, current_block: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.filters.insert(
            id,
            InstalledFilter {
                kind,
                last_poll_block: current_block,
            },
        );
        id
    }

    fn remove(&mut self, id: u64) -> bool {
        self.filters.remove(&id).is_some()
    }
}

// ---------------------------------------------------------------------------
// Per-IP throttle — this endpoint sits on the public internet.
// ---------------------------------------------------------------------------

/// Fixed-window per-IP request cap. Legit clients batch (a JSON-RPC batch is
/// one HTTP request), so anything above this rate is abuse or a stuck loop.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(1);
const RATE_LIMIT_MAX_PER_WINDOW: u32 = 100;
/// Bound tracker memory: drop all state rather than let a botnet grow the map.
const RATE_LIMIT_MAX_TRACKED_IPS: usize = 100_000;

/// Resolve the address to throttle on. Direct connections are keyed on the
/// socket peer. Loopback connections come from the local reverse proxy (Caddy
/// terminates TLS for rpc.mersennet.com on this host), so key on the client
/// address it forwards instead — otherwise every proxied request shares the
/// 127.0.0.1 bucket and one abuser starves all legitimate users.
fn throttle_ip(peer: IpAddr, cf_connecting_ip: Option<&str>, x_forwarded_for: Option<&str>) -> IpAddr {
    if !peer.is_loopback() {
        return peer;
    }
    // Behind our own loopback reverse proxy (Caddy) fronted by Cloudflare.
    // Prefer CF-Connecting-IP, which Cloudflare sets and overwrites (a client
    // cannot forge it through CF). For X-Forwarded-For, take the LAST hop —
    // the entry our own proxy appended — not the first, which is
    // client-controlled and would let an attacker rotate the per-IP bucket
    // with a spoofed header.
    if let Some(ip) = cf_connecting_ip.and_then(|v| v.trim().parse().ok()) {
        return ip;
    }
    if let Some(ip) = x_forwarded_for
        .and_then(|v| v.split(',').next_back())
        .and_then(|v| v.trim().parse().ok())
    {
        return ip;
    }
    peer
}

#[derive(Debug)]
struct RateLimiter {
    hits: HashMap<IpAddr, (Instant, u32)>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            hits: HashMap::new(),
        }
    }

    /// True if this IP may make another request right now.
    fn allow(&mut self, ip: IpAddr) -> bool {
        if self.hits.len() >= RATE_LIMIT_MAX_TRACKED_IPS {
            self.hits.clear();
        }
        let now = Instant::now();
        let entry = self.hits.entry(ip).or_insert((now, 0));
        if now.duration_since(entry.0) >= RATE_LIMIT_WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= RATE_LIMIT_MAX_PER_WINDOW
    }
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    #[serde(default, rename = "jsonrpc")]
    _jsonrpc: Option<String>,
    id: Value,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct RpcResponse<T> {
    jsonrpc: &'static str,
    id: Value,
    result: T,
}

#[derive(Debug, Serialize)]
struct RpcErrorResponse {
    jsonrpc: &'static str,
    id: Value,
    error: RpcError,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BlockDto {
    number: String,
    hash: String,
    parent_hash: String,
    nonce: String,
    sha3_uncles: String,
    logs_bloom: String,
    transactions_root: String,
    state_root: String,
    receipts_root: String,
    miner: String,
    proposer: String,
    difficulty: String,
    total_difficulty: String,
    extra_data: String,
    size: String,
    gas_limit: String,
    gas_used: String,
    #[serde(rename = "baseFeePerGas")]
    base_fee_per_gas: String,
    timestamp: String,
    transactions: Vec<Value>,
    uncles: Vec<Value>,
    mix_hash: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    domain_events: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TxDto {
    hash: String,
    from: String,
    to: Option<String>,
    value: String,
    nonce: String,
    gas: String,
    gas_price: String,
    input: String,
    block_hash: Option<String>,
    block_number: Option<String>,
    transaction_index: Option<String>,
    #[serde(rename = "type")]
    tx_type: String,
    v: String,
    r: String,
    s: String,
    chain_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptDto {
    transaction_hash: String,
    block_hash: String,
    block_number: String,
    transaction_index: String,
    from: String,
    to: Option<String>,
    gas_used: String,
    cumulative_gas_used: String,
    effective_gas_price: String,
    status: String,
    contract_address: Option<String>,
    logs_bloom: String,
    #[serde(rename = "type")]
    tx_type: String,
    logs: Vec<LogDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogDto {
    address: String,
    topics: Vec<String>,
    data: String,
    block_number: String,
    block_hash: String,
    transaction_hash: String,
    transaction_index: String,
    log_index: String,
    removed: bool,
}

#[derive(Debug, Deserialize)]
struct LogFilterInput {
    #[serde(rename = "fromBlock")]
    from_block: Option<Value>,
    #[serde(rename = "toBlock")]
    to_block: Option<Value>,
    address: Option<Value>,
    topics: Option<Vec<Value>>,
}

#[derive(Debug, Clone)]
struct LogFilter {
    from_block: u64,
    to_block: u64,
    addresses: Vec<Address>,
    topics: Vec<TopicFilter>,
}

#[derive(Debug, Clone)]
enum TopicFilter {
    Any,
    OneOf(Vec<B256>),
}

pub fn serve(engine: Arc<Mutex<Engine>>, addr: &str) -> Result<()> {
    let server = Server::http(addr).map_err(|err| anyhow!(err.to_string()))?;
    let filters: FilterStore = Arc::new(Mutex::new(FilterState::new()));
    let mut rate_limiter = RateLimiter::new();
    info!("RPC listening on http://{addr}");

    for request in server.incoming_requests() {
        // One malformed request must never take down the node (this process
        // is the public RPC for the whole stack): log the failure, drop the
        // connection, and keep serving.
        if let Err(err) = handle_request(request, &engine, &filters, &mut rate_limiter) {
            warn!("RPC request failed: {err:#}");
        }
    }

    Ok(())
}

fn handle_request(
    mut request: tiny_http::Request,
    engine: &Arc<Mutex<Engine>>,
    filters: &FilterStore,
    rate_limiter: &mut RateLimiter,
) -> Result<()> {
    // Per-IP throttle before any work: the whole stack hangs off this one
    // process, so a flood from a single source must not starve everyone else.
    if let Some(addr) = request.remote_addr() {
        let cf_ip = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("CF-Connecting-IP"))
            .map(|h| h.value.as_str().to_owned());
        let xff = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("X-Forwarded-For"))
            .map(|h| h.value.as_str().to_owned());
        let ip = throttle_ip(addr.ip(), cf_ip.as_deref(), xff.as_deref());
        if !rate_limiter.allow(ip) {
            let response = Response::from_string(error_body(
                Value::Null,
                -32005,
                "rate limit exceeded".to_string(),
            )?)
            .with_status_code(429)
            .with_header(
                Header::from_bytes("Content-Type", "application/json")
                    .map_err(|_| anyhow!("invalid header"))?,
            );
            request.respond(response)?;
            return Ok(());
        }
    }

    // Expose metrics on GET /metrics
    if request.method() == &Method::Get && request.url() == "/metrics" {
        if let Some(handle) = prometheus::handle() {
            metrics::increment_counter!("mersennet_rpc_requests", "method" => "metrics");
            let body = handle.render();
            let response = Response::from_string(body).with_header(
                Header::from_bytes("Content-Type", "text/plain; version=0.0.4")
                    .map_err(|_| anyhow!("invalid header"))?,
            );
            request.respond(response)?;
            return Ok(());
        } else {
            let response = Response::from_string("metrics not initialized").with_status_code(500);
            request.respond(response)?;
            return Ok(());
        }
    }

    if request.method() == &Method::Get && request.url() == "/health" {
        let engine = engine.lock().map_err(|_| anyhow!("engine lock poisoned"))?;
        let response = serde_json::json!({
            "status": "ok",
            "height": engine.latest_height(),
            "chain_id": engine.chain_id,
        });
        let response = Response::from_string(response.to_string()).with_header(
            Header::from_bytes("Content-Type", "application/json")
                .map_err(|_| anyhow!("invalid header"))?,
        );
        request.respond(response)?;
        return Ok(());
    }

    if request.method() == &Method::Options {
        let response = Response::from_string("")
            .with_status_code(204)
            .with_header(
                Header::from_bytes("Access-Control-Allow-Origin", "*")
                    .map_err(|_| anyhow!("invalid header"))?,
            )
            .with_header(
                Header::from_bytes("Access-Control-Allow-Methods", "POST, GET, OPTIONS")
                    .map_err(|_| anyhow!("invalid header"))?,
            )
            .with_header(
                Header::from_bytes("Access-Control-Allow-Headers", "Content-Type")
                    .map_err(|_| anyhow!("invalid header"))?,
            )
            .with_header(
                Header::from_bytes("Access-Control-Max-Age", "86400")
                    .map_err(|_| anyhow!("invalid header"))?,
            );
        request.respond(response)?;
        return Ok(());
    }

    if request.method() != &Method::Post {
        let response = Response::from_string("use POST").with_status_code(405);
        request.respond(response)?;
        return Ok(());
    }

    // Cap the body read — this endpoint is on the public internet.
    // Read raw bytes first: an invalid-UTF-8 body is a client error (-32700),
    // not an I/O error that would kill the accept loop.
    let mut raw = Vec::new();
    request
        .as_reader()
        .take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut raw)?;
    let response = match String::from_utf8(raw) {
        Ok(content) => handle_body(&content, engine, filters)?,
        Err(_) => error_body(
            Value::Null,
            -32700,
            "request body is not valid UTF-8".to_string(),
        )?,
    };

    let response = Response::from_string(response)
        .with_header(
            Header::from_bytes("Content-Type", "application/json")
                .map_err(|_| anyhow!("invalid header"))?,
        )
        .with_header(
            Header::from_bytes("Access-Control-Allow-Origin", "*")
                .map_err(|_| anyhow!("invalid header"))?,
        );
    request.respond(response)?;
    Ok(())
}

/// Public-endpoint limits: request bodies and batch fan-out are attacker-controlled.
const MAX_BODY_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BATCH_CALLS: usize = 100;

/// Turn a raw request body into a response body: a single JSON-RPC call, or a
/// JSON-RPC 2.0 batch (an array of calls answered by an array of responses in
/// the same order — ethers.js batches by default).
fn handle_body(
    content: &str,
    engine: &Arc<Mutex<Engine>>,
    filters: &FilterStore,
) -> Result<String> {
    if content.len() as u64 > MAX_BODY_BYTES {
        return error_body(
            Value::Null,
            -32600,
            format!("request body exceeds {MAX_BODY_BYTES} bytes"),
        );
    }
    match serde_json::from_str::<Value>(content) {
        Ok(Value::Array(calls)) => {
            if calls.is_empty() {
                error_body(Value::Null, -32600, "empty batch".to_string())
            } else if calls.len() > MAX_BATCH_CALLS {
                error_body(
                    Value::Null,
                    -32600,
                    format!("batch exceeds {MAX_BATCH_CALLS} calls"),
                )
            } else {
                let payloads = calls
                    .into_iter()
                    .map(|call| run_one_call(call, engine, filters))
                    .collect::<Result<Vec<String>>>()?;
                Ok(format!("[{}]", payloads.join(",")))
            }
        }
        Ok(single) => run_one_call(single, engine, filters),
        Err(err) => error_body(Value::Null, -32700, format!("invalid json: {err}")),
    }
}

fn error_body(id: Value, code: i64, message: String) -> Result<String> {
    Ok(serde_json::to_string(&RpcErrorResponse {
        jsonrpc: "2.0",
        id,
        error: RpcError {
            code,
            message,
            data: None,
        },
    })?)
}

/// Run a single already-JSON-parsed call and serialize its success or error
/// payload. Shared by the single-request path and each element of a batch.
fn run_one_call(
    raw: Value,
    engine: &Arc<Mutex<Engine>>,
    filters: &FilterStore,
) -> Result<String> {
    let call: RpcRequest = match serde_json::from_value(raw) {
        Ok(call) => call,
        Err(err) => return error_body(Value::Null, -32600, format!("invalid request: {err}")),
    };
    let method = call.method.clone();
    match dispatch(call, engine, filters) {
        Ok(payload) => Ok(payload),
        Err((id, error)) => {
            metrics::increment_counter!(
                "mersennet_rpc_errors",
                "code" => error.code.to_string(),
                "method" => method.clone()
            );
            tracing::warn!(method = %method, code = error.code, message = %error.message, "rpc request failed");
            Ok(serde_json::to_string(&RpcErrorResponse {
                jsonrpc: "2.0",
                id,
                error,
            })?)
        }
    }
}

fn dispatch(
    call: RpcRequest,
    engine: &Arc<Mutex<Engine>>,
    filters: &FilterStore,
) -> Result<String, (Value, RpcError)> {
    let id = call.id.clone();
    let start = std::time::Instant::now();
    // count requests per method
    metrics::increment_counter!("mersennet_rpc_requests", "method" => call.method.clone());

    let result = match call.method.as_str() {
        "mersennetId"
        | "eth_chainId"
        | "mersennet_blockNumber"
        | "eth_blockNumber"
        | "mersennet_getBalance"
        | "eth_getBalance"
        | "mersennet_getDomainEvents"
        | "mersennet_gasPrice"
        | "eth_gasPrice"
        | "mersennet_validators"
        | "mersennet_getCodeAttestation"
        | "mersennet_getCodeHash"
        | "mersennet_getCode"
        | "eth_getCode"
        | "mersennet_getStorageAt"
        | "eth_getStorageAt"
        | "mersennet_getTransactionCount"
        | "eth_getTransactionCount"
        | "mersennet_call"
        | "eth_call"
        | "eth_estimateGas"
        | "net_version"
        | "net_peerCount"
        | "net_listening"
        | "web3_clientVersion"
        | "txpool_status" => {
            let params = call.params.unwrap_or(Value::Null);
            let mut engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match rpc_router::route(call.method.as_str(), params, &mut engine) {
                Ok(value) => value,
                Err(err) => return Err((id.clone(), rpc_error_with_code(err.code, err.message))),
            }
        }
        "mersennet_getBlockByNumber" | "eth_getBlockByNumber" => {
            let params = call.params.unwrap_or(Value::Null);
            let (number, include_txs) = parse_block_params(params, engine, id.clone())
                .map_err(|(id, message)| (id, rpc_error_invalid_params(message)))?;
            let engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            if include_txs {
                require_transparent_tx_metadata_access_enabled(&engine)
                    .map_err(|err| (id.clone(), err))?;
            }
            let block = engine.block_by_number(number);
            match block {
                Some(block) => serde_json::to_value(block_to_dto(block, include_txs))
                    .map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?,
                None => Value::Null,
            }
        }
        "mersennet_getTransactionReceipt" | "eth_getTransactionReceipt" => {
            let params = call.params.unwrap_or(Value::Null);
            let tx_hash = parse_hash_param(params)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let mut engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            require_transparent_tx_metadata_access_enabled(&engine)
                .map_err(|err| (id.clone(), err))?;
            let receipt = find_receipt(&mut engine, tx_hash);
            match receipt {
                Some(dto) => serde_json::to_value(dto)
                    .map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?,
                None => Value::Null,
            }
        }
        "mersennet_getTransactionByHash" | "eth_getTransactionByHash" => {
            let params = call.params.unwrap_or(Value::Null);
            let tx_hash = parse_hash_param(params)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            require_transparent_tx_metadata_access_enabled(&engine)
                .map_err(|err| (id.clone(), err))?;
            let result = find_transaction(&engine, tx_hash);
            match result {
                Some((tx, block, idx)) => serde_json::to_value(tx_to_dto_with_block(
                    tx,
                    hex_b256(block.hash),
                    hex_u64(block.number),
                    idx,
                ))
                .map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?,
                None => Value::Null,
            }
        }
        "eth_sendRawTransaction" => {
            let params = call.params.unwrap_or(Value::Null);
            let raw_hex = parse_raw_tx_param(params)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let signed = mersennet::crypto::decode_raw_signed_tx(&raw_hex)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let mut engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let tx_hash = tx_hash(&signed.tx);
            // Signature was already verified during decode_raw_signed_tx
            // (which recovers `from` from the signature). Using submit_tx_unsigned
            // avoids re-hashing with the wrong signing scheme for Ethereum-format txs.
            engine.submit_tx_unsigned(signed.tx).map_err(|err| {
                let data = json!({ "reason": err.code() });
                (
                    id.clone(),
                    rpc_error_with_data(-32005, format!("tx rejected: {}", err), data),
                )
            })?;
            // Keep the raw envelope so the p2p relay can gossip it verbatim;
            // peers re-verify the Ethereum signature from the raw bytes.
            engine.cache_raw_tx(tx_hash, raw_hex);
            Value::String(hex_b256(tx_hash))
        }
        "mersennet_sendTransaction" | "eth_sendTransaction" => {
            // Disabled on the public RPC: this signs with a node-held key /
            // trusts a caller-supplied `from`, so exposing it lets anyone
            // execute transactions as any account. Wallets must sign locally
            // and submit via eth_sendRawTransaction.
            return Err((
                id.clone(),
                rpc_error_with_data(
                    -32601,
                    "eth_sendTransaction is disabled; sign the transaction in your wallet and submit it via eth_sendRawTransaction".to_string(),
                    Value::Null,
                ),
            ));
        }
        "mersennet_getLogs" | "eth_getLogs" => {
            let params = call.params.unwrap_or(Value::Null);
            {
                let engine = engine
                    .lock()
                    .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                require_transparent_event_access_enabled(&engine)
                    .map_err(|err| (id.clone(), err))?;
            }
            let filter = parse_log_filter(params, engine, id.clone())
                .map_err(|(id, message)| (id, rpc_error_invalid_params(message)))?;
            let engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let logs = collect_logs(&engine, &filter);
            serde_json::to_value(logs)
                .map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?
        }
        "eth_getBlockByHash" => {
            let params = call.params.unwrap_or(Value::Null);
            let array = match params {
                Value::Array(values) => values,
                _ => return Err((id.clone(), rpc_error_invalid_params("invalid params"))),
            };
            let hash = match array.first() {
                Some(Value::String(value)) => parse_hash(value)
                    .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?,
                _ => return Err((id.clone(), rpc_error_invalid_params("block hash required"))),
            };
            let include_txs = match array.get(1) {
                Some(Value::Bool(value)) => *value,
                _ => false,
            };
            let engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            if include_txs {
                require_transparent_tx_metadata_access_enabled(&engine)
                    .map_err(|err| (id.clone(), err))?;
            }
            match engine.block_by_hash(hash) {
                Some(block) => serde_json::to_value(block_to_dto(block, include_txs))
                    .map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?,
                None => Value::Null,
            }
        }
        "eth_feeHistory" => {
            let params = call.params.unwrap_or(Value::Null);
            let engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let latest = engine.latest_height();
            let array = match params {
                Value::Array(v) => v,
                _ => vec![],
            };
            // Clamp blockCount: this is attacker-controlled and each block is a
            // lookup into the in-memory chain window done while holding the
            // engine lock (which also serializes block production and every
            // other RPC). An unclamped huge count is a whole-node DoS.
            // Standard clients never request more than 1024.
            const MAX_FEE_HISTORY_BLOCKS: u64 = 1024;
            let block_count = match array.first() {
                Some(Value::String(s)) => parse_hex_u64(s).unwrap_or(1),
                Some(Value::Number(n)) => n.as_u64().unwrap_or(1),
                _ => 1,
            }
            .clamp(1, MAX_FEE_HISTORY_BLOCKS);
            let newest = match array.get(1) {
                Some(Value::String(s)) if s == "latest" || s == "pending" => latest,
                Some(Value::String(s)) => parse_hex_u64(s).unwrap_or(latest),
                _ => latest,
            };
            let oldest = newest.saturating_sub(block_count.saturating_sub(1));
            let base_fees: Vec<String> = (oldest..=newest + 1)
                .map(|n| {
                    engine
                        .block_by_number(n)
                        .map(|b| hex_u256(b.base_fee))
                        .unwrap_or_else(|| "0x1".to_string())
                })
                .collect();
            let gas_ratios: Vec<Vec<f64>> = (oldest..=newest)
                .map(|n| {
                    engine
                        .block_by_number(n)
                        .map(|b| {
                            if b.gas_limit > 0 {
                                vec![b.gas_used as f64 / b.gas_limit as f64]
                            } else {
                                vec![0.0]
                            }
                        })
                        .unwrap_or_else(|| vec![0.0])
                })
                .collect();
            json!({
                "oldestBlock": hex_u64(oldest),
                "baseFeePerGas": base_fees,
                "gasUsedRatio": gas_ratios.into_iter().map(|v| v[0]).collect::<Vec<f64>>(),
                "reward": []
            })
        }
        "eth_maxPriorityFeePerGas" => Value::String("0x0".to_string()),
        "eth_accounts" => Value::Array(vec![]),
        "eth_mining" => Value::Bool(false),
        "eth_syncing" => Value::Bool(false),
        "eth_getUncleCountByBlockNumber" | "eth_getUncleCountByBlockHash" => {
            Value::String("0x0".to_string())
        }
        "eth_protocolVersion" => Value::String("0x41".to_string()),
        "eth_newFilter" => {
            let params = call.params.unwrap_or(Value::Null);
            {
                let eng = engine
                    .lock()
                    .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                require_transparent_event_access_enabled(&eng).map_err(|err| (id.clone(), err))?;
            }
            let filter = parse_log_filter(params, engine, id.clone())
                .map_err(|(id, message)| (id, rpc_error_invalid_params(message)))?;
            let latest = {
                let eng = engine
                    .lock()
                    .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                eng.latest_height()
            };
            let mut fs = filters
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter_id = fs.install(FilterKind::Log(filter), latest);
            Value::String(hex_u64(filter_id))
        }
        "eth_newBlockFilter" => {
            let latest = {
                let eng = engine
                    .lock()
                    .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                eng.latest_height()
            };
            let mut fs = filters
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter_id = fs.install(FilterKind::Block, latest);
            Value::String(hex_u64(filter_id))
        }
        "eth_newPendingTransactionFilter" => {
            {
                let eng = engine
                    .lock()
                    .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                require_transparent_pending_tx_access_enabled(&eng)
                    .map_err(|err| (id.clone(), err))?;
            }
            let latest = {
                let eng = engine
                    .lock()
                    .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                eng.latest_height()
            };
            let mut fs = filters
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter_id = fs.install(FilterKind::PendingTx, latest);
            Value::String(hex_u64(filter_id))
        }
        "eth_getFilterChanges" => {
            let params = call.params.unwrap_or(Value::Null);
            let array = match params {
                Value::Array(v) => v,
                _ => vec![],
            };
            let filter_id = match array.first() {
                Some(Value::String(s)) => parse_hex_u64(s)
                    .map_err(|e| (id.clone(), rpc_error_invalid_params(e.to_string())))?,
                _ => return Err((id.clone(), rpc_error_invalid_params("filter id required"))),
            };
            let mut fs = filters
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter = fs
                .filters
                .get_mut(&filter_id)
                .ok_or_else(|| (id.clone(), rpc_error_invalid_params("filter not found")))?;
            let engine_guard = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match &filter.kind {
                FilterKind::Log(_) => require_transparent_event_access_enabled(&engine_guard)
                    .map_err(|err| (id.clone(), err))?,
                FilterKind::PendingTx => {
                    require_transparent_pending_tx_access_enabled(&engine_guard)
                        .map_err(|err| (id.clone(), err))?
                }
                FilterKind::Block => {}
            }
            let latest = engine_guard.latest_height();
            let from_block = filter.last_poll_block + 1;

            let result = match &filter.kind {
                FilterKind::Log(log_filter) => {
                    let mut f = log_filter.clone();
                    f.from_block = from_block;
                    f.to_block = latest;
                    let logs = collect_logs(&engine_guard, &f);
                    serde_json::to_value(logs)
                        .map_err(|e| (id.clone(), rpc_error_internal(e.to_string())))?
                }
                FilterKind::Block => {
                    let hashes: Vec<Value> = (from_block..=latest)
                        .filter_map(|n| engine_guard.block_by_number(n))
                        .map(|b| Value::String(hex_b256(b.hash)))
                        .collect();
                    Value::Array(hashes)
                }
                FilterKind::PendingTx => Value::Array(vec![]),
            };
            filter.last_poll_block = latest;
            result
        }
        "eth_getFilterLogs" => {
            let params = call.params.unwrap_or(Value::Null);
            let array = match params {
                Value::Array(v) => v,
                _ => vec![],
            };
            let filter_id = match array.first() {
                Some(Value::String(s)) => parse_hex_u64(s)
                    .map_err(|e| (id.clone(), rpc_error_invalid_params(e.to_string())))?,
                _ => return Err((id.clone(), rpc_error_invalid_params("filter id required"))),
            };
            let fs = filters
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter = fs
                .filters
                .get(&filter_id)
                .ok_or_else(|| (id.clone(), rpc_error_invalid_params("filter not found")))?;
            match &filter.kind {
                FilterKind::Log(log_filter) => {
                    let engine_guard = engine
                        .lock()
                        .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                    require_transparent_event_access_enabled(&engine_guard)
                        .map_err(|err| (id.clone(), err))?;
                    let logs = collect_logs(&engine_guard, log_filter);
                    serde_json::to_value(logs)
                        .map_err(|e| (id.clone(), rpc_error_internal(e.to_string())))?
                }
                _ => Value::Array(vec![]),
            }
        }
        "eth_uninstallFilter" => {
            let params = call.params.unwrap_or(Value::Null);
            let array = match params {
                Value::Array(v) => v,
                _ => vec![],
            };
            let filter_id = match array.first() {
                Some(Value::String(s)) => parse_hex_u64(s)
                    .map_err(|e| (id.clone(), rpc_error_invalid_params(e.to_string())))?,
                _ => return Err((id.clone(), rpc_error_invalid_params("filter id required"))),
            };
            let mut fs = filters
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            Value::Bool(fs.remove(filter_id))
        }
        "mersennet_orders_addMarket"
        | "mersennet_orders_submitOrder"
        | "mersennet_orders_cancelOrder"
        | "mersennet_orders_getOrderBook"
        | "mersennet_orders_getOpenOrders"
        | "mersennet_orders_setMarginParams"
        | "mersennet_orders_depositCollateral"
        | "mersennet_orders_isLiquidatable"
        | "mersennet_orders_liquidate" => {
            let params = call.params.unwrap_or(Value::Null);
            let mut engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match rpc_router::route(call.method.as_str(), params, &mut engine) {
                Ok(value) => value,
                Err(err) => return Err((id.clone(), rpc_error_with_code(err.code, err.message))),
            }
        }
        "mersennet_bridge_enqueueOrdersToEvm"
        | "mersennet_bridge_enqueueEvmToOrders"
        | "mersennet_bridge_dequeueOrdersToEvm"
        | "mersennet_bridge_dequeueEvmToOrders" => {
            let params = call.params.unwrap_or(Value::Null);
            let mut engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match rpc_router::route(call.method.as_str(), params, &mut engine) {
                Ok(value) => value,
                Err(err) => return Err((id.clone(), rpc_error_with_code(err.code, err.message))),
            }
        }
        _ => {
            // Fall through to the comprehensive router, which also
            // dispatches shielded methods (mersennet_getShielded*,
            // mersennet_*StateProof, mersennet_submitShield*, etc.).
            // Genuinely unknown methods return -32601 from the router.
            let params = call.params.unwrap_or(Value::Null);
            let mut engine = engine
                .lock()
                .map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match rpc_router::route(call.method.as_str(), params, &mut engine) {
                Ok(value) => value,
                Err(err) => return Err((id.clone(), rpc_error_with_code(err.code, err.message))),
            }
        }
    };

    // record request duration
    let dur = start.elapsed().as_secs_f64();
    metrics::histogram!("mersennet_rpc_duration_seconds", dur, "method" => call.method.clone());

    serde_json::to_string(&RpcResponse {
        jsonrpc: "2.0",
        id,
        result,
    })
    .map_err(|err| (call.id, rpc_error_internal(err.to_string())))
}

fn rpc_error_internal(message: impl Into<String>) -> RpcError {
    RpcError {
        code: -32000,
        message: message.into(),
        data: None,
    }
}

fn rpc_error_invalid_params(message: impl Into<String>) -> RpcError {
    RpcError {
        code: -32602,
        message: message.into(),
        data: None,
    }
}

fn rpc_error_with_code(code: i64, message: impl Into<String>) -> RpcError {
    RpcError {
        code,
        message: message.into(),
        data: None,
    }
}

fn rpc_error_with_data(code: i64, message: impl Into<String>, data: Value) -> RpcError {
    RpcError {
        code,
        message: message.into(),
        data: Some(data),
    }
}

fn require_transparent_event_access_enabled(engine: &Engine) -> Result<(), RpcError> {
    if engine.privacy_mode_activated() {
        Err(rpc_error_with_code(
            -32605,
            "transparent event/log RPC disabled after privacy activation",
        ))
    } else {
        Ok(())
    }
}

fn require_transparent_pending_tx_access_enabled(engine: &Engine) -> Result<(), RpcError> {
    if engine.privacy_mode_activated() {
        Err(rpc_error_with_code(
            -32605,
            "transparent pending-transaction RPC disabled after privacy activation",
        ))
    } else {
        Ok(())
    }
}

fn require_transparent_tx_metadata_access_enabled(engine: &Engine) -> Result<(), RpcError> {
    if engine.privacy_mode_activated() {
        Err(rpc_error_with_code(
            -32605,
            "transparent transaction/receipt metadata RPC disabled after privacy activation",
        ))
    } else {
        Ok(())
    }
}

fn parse_block_params(
    params: Value,
    engine: &Arc<Mutex<Engine>>,
    id: Value,
) -> Result<(u64, bool), (Value, String)> {
    let array = match params {
        Value::Array(values) => values,
        Value::Null => Vec::new(),
        _ => return Err((id, "invalid params".to_string())),
    };

    let number = match array.first() {
        Some(Value::String(value)) if value == "latest" => {
            let engine = engine
                .lock()
                .map_err(|_| (id.clone(), "engine lock poisoned".to_string()))?;
            engine.latest_height()
        }
        Some(Value::String(value)) => {
            parse_hex_u64(value).map_err(|err| (id.clone(), err.to_string()))?
        }
        Some(Value::Number(value)) => value
            .as_u64()
            .ok_or((id.clone(), "invalid block number".to_string()))?,
        None => {
            let engine = engine
                .lock()
                .map_err(|_| (id.clone(), "engine lock poisoned".to_string()))?;
            engine.latest_height()
        }
        _ => return Err((id, "invalid block number".to_string())),
    };

    let include_txs = match array.get(1) {
        Some(Value::Bool(value)) => *value,
        _ => false,
    };

    Ok((number, include_txs))
}

fn parse_raw_tx_param(params: Value) -> Result<Vec<u8>, String> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err("invalid params".to_string()),
    };
    let hex_str = match array.first() {
        Some(Value::String(value)) => value.as_str(),
        _ => return Err("raw tx hex required".to_string()),
    };
    let stripped = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(stripped).map_err(|e| format!("invalid hex: {}", e))
}

fn parse_hash_param(params: Value) -> Result<B256, RpcInputError> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcInputError::InvalidParams),
    };
    let hash = match array.first() {
        Some(Value::String(value)) => parse_hash(value)?,
        _ => return Err(RpcInputError::HashRequired),
    };
    Ok(hash)
}

fn parse_log_filter(
    params: Value,
    engine: &Arc<Mutex<Engine>>,
    id: Value,
) -> Result<LogFilter, (Value, String)> {
    let array = match params {
        Value::Array(values) => values,
        Value::Null => Vec::new(),
        _ => return Err((id, "invalid params".to_string())),
    };
    let raw = array.first().cloned().unwrap_or(Value::Null);
    let input: LogFilterInput = if raw.is_null() {
        LogFilterInput {
            from_block: None,
            to_block: None,
            address: None,
            topics: None,
        }
    } else {
        serde_json::from_value(raw).map_err(|err| {
            (
                id.clone(),
                RpcInputError::InvalidLogFilter(err.to_string()).to_string(),
            )
        })?
    };

    let latest = {
        let engine = engine
            .lock()
            .map_err(|_| (id.clone(), "engine lock poisoned".to_string()))?;
        engine.latest_height()
    };

    let from_block =
        parse_block_tag(input.from_block, latest).map_err(|err| (id.clone(), err.to_string()))?;
    let to_block =
        parse_block_tag(input.to_block, latest).map_err(|err| (id.clone(), err.to_string()))?;
    let addresses =
        parse_filter_addresses(input.address).map_err(|err| (id.clone(), err.to_string()))?;
    let topics = parse_filter_topics(input.topics).map_err(|err| (id.clone(), err.to_string()))?;

    Ok(LogFilter {
        from_block,
        to_block,
        addresses,
        topics,
    })
}

fn parse_block_tag(value: Option<Value>, latest: u64) -> Result<u64, RpcInputError> {
    let Some(value) = value else {
        return Ok(latest);
    };
    match value {
        Value::String(tag) if tag == "latest" => Ok(latest),
        Value::String(hex) => parse_hex_u64(&hex),
        Value::Number(num) => num.as_u64().ok_or(RpcInputError::InvalidBlockTag),
        Value::Null => Ok(latest),
        _ => Err(RpcInputError::InvalidBlockTag),
    }
}

fn parse_filter_addresses(value: Option<Value>) -> Result<Vec<Address>, RpcInputError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match value {
        Value::String(addr) => Ok(vec![parse_address(&addr)?]),
        Value::Array(values) => values
            .into_iter()
            .map(|item| match item {
                Value::String(addr) => parse_address(&addr),
                _ => Err(RpcInputError::InvalidAddress),
            })
            .collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err(RpcInputError::InvalidAddress),
    }
}

fn parse_filter_topics(value: Option<Vec<Value>>) -> Result<Vec<TopicFilter>, RpcInputError> {
    let Some(values) = value else {
        return Ok(Vec::new());
    };
    let mut topics = Vec::new();
    for item in values {
        match item {
            Value::Null => topics.push(TopicFilter::Any),
            Value::String(topic) => {
                let hash = parse_hash(&topic)?;
                topics.push(TopicFilter::OneOf(vec![hash]));
            }
            Value::Array(inner) => {
                let mut hashes = Vec::new();
                for value in inner {
                    match value {
                        Value::String(topic) => hashes.push(parse_hash(&topic)?),
                        Value::Null => {}
                        _ => return Err(RpcInputError::InvalidTopic),
                    }
                }
                topics.push(TopicFilter::OneOf(hashes));
            }
            _ => return Err(RpcInputError::InvalidTopic),
        }
    }
    Ok(topics)
}

fn find_receipt(engine: &mut Engine, hash: B256) -> Option<ReceiptDto> {
    for block in &engine.chain {
        for (index, (tx, receipt)) in block.transactions.iter().zip(&block.receipts).enumerate() {
            if tx_hash(tx) == hash {
                return Some(receipt_to_dto(block, tx, receipt, index as u64, hash));
            }
        }
    }
    None
}

fn find_transaction(engine: &Engine, hash: B256) -> Option<(&Transaction, &Block, u64)> {
    for block in &engine.chain {
        for (index, tx) in block.transactions.iter().enumerate() {
            if tx_hash(tx) == hash {
                return Some((tx, block, index as u64));
            }
        }
    }
    None
}

fn block_to_dto(block: &Block, include_txs: bool) -> BlockDto {
    block_to_dto_with_privacy(block, include_txs, crate::ws::privacy_mode_activated())
}

fn block_to_dto_with_privacy(block: &Block, include_txs: bool, privacy_active: bool) -> BlockDto {
    let block_hash = hex_b256(block.hash);
    let block_number = hex_u64(block.number);

    let transactions = if include_txs {
        block
            .transactions
            .iter()
            .enumerate()
            .map(|(i, tx)| {
                serde_json::to_value(tx_to_dto_with_block(
                    tx,
                    block_hash.clone(),
                    block_number.clone(),
                    i as u64,
                ))
                .unwrap_or(Value::Null)
            })
            .collect()
    } else {
        block
            .transactions
            .iter()
            .map(|tx| Value::String(hex_b256(tx_hash(tx))))
            .collect()
    };

    let timestamp = block.timestamp;

    BlockDto {
        number: block_number.clone(),
        hash: block_hash,
        // Real hash-linked parent (was a placeholder derived from the
        // block number). Genesis carries a zero parent.
        parent_hash: hex_b256(block.parent_hash),
        nonce: "0x0000000000000000".to_string(),
        sha3_uncles: hex_b256(B256::ZERO),
        logs_bloom: block_logs_bloom_hex(block),
        transactions_root: hex_b256(block.hash),
        state_root: hex_b256(block.state_root),
        receipts_root: hex_b256(B256::ZERO),
        miner: hex_address(block.coinbase),
        proposer: hex_address(block.proposer),
        difficulty: "0x0".to_string(),
        total_difficulty: "0x0".to_string(),
        extra_data: "0x".to_string(),
        size: hex_u64(0),
        gas_limit: hex_u64(block.gas_limit),
        gas_used: hex_u64(block.gas_used),
        base_fee_per_gas: hex_u256(block.base_fee),
        timestamp: hex_u64(timestamp),
        transactions,
        uncles: vec![],
        mix_hash: hex_b256(B256::ZERO),
        domain_events: block
            .domain_events
            .iter()
            .filter(|event| block_domain_event_is_visible(event, privacy_active))
            .map(|event| domain_event_to_value(event, privacy_active))
            .collect(),
    }
}

// Pre-fork, all CLOB events are public. A non-zero shielded_state_root is NOT
// a privacy signal any more — proof-only mode gives every block one — so
// visibility keys on the actual privacy activation flag.
fn block_domain_event_is_visible(event: &DomainEvent, privacy_active: bool) -> bool {
    if !privacy_active {
        return true;
    }

    event.is_privacy_safe_after_activation()
}

#[allow(dead_code)]
fn tx_to_dto(tx: &Transaction) -> TxDto {
    tx_to_dto_in_block(tx, None, None, 0)
}

fn tx_to_dto_with_block(
    tx: &Transaction,
    block_hash: String,
    block_number: String,
    tx_index: u64,
) -> TxDto {
    tx_to_dto_in_block(tx, Some(block_hash), Some(block_number), tx_index)
}

fn tx_to_dto_in_block(
    tx: &Transaction,
    block_hash: Option<String>,
    block_number: Option<String>,
    tx_index: u64,
) -> TxDto {
    let (v, r, s) = match &tx.signature {
        Some((r, s, v)) => (hex_u64(*v), hex_u256(*r), hex_u256(*s)),
        None => ("0x0".to_string(), "0x0".to_string(), "0x0".to_string()),
    };
    TxDto {
        hash: hex_b256(tx_hash(tx)),
        from: hex_address(tx.from),
        to: tx.to.map(hex_address),
        value: hex_u256(tx.value),
        nonce: hex_u64(tx.nonce),
        gas: hex_u64(tx.gas_limit),
        gas_price: hex_u256(tx.gas_price),
        input: format!("0x{}", hex::encode(&tx.data)),
        block_hash,
        block_number,
        transaction_index: Some(hex_u64(tx_index)),
        tx_type: "0x0".to_string(),
        v,
        r,
        s,
        chain_id: tx.chain_id.map(hex_u64),
    }
}

fn domain_event_to_value(event: &DomainEvent, privacy_active: bool) -> Value {
    match event {
        DomainEvent::MersennetOrders(evt) => json!({
            "domain": "mersennet_orders",
            "kind": evt.kind(),
            "data": mersennet_orders_event_data(evt, privacy_active),
        }),
        DomainEvent::Bridge(evt) => json!({
            "domain": "bridge",
            "kind": evt.kind(),
            "data": bridge_event_data(evt),
        }),
        DomainEvent::Shielded(evt) => json!({
            "domain": "shielded",
            "kind": evt.kind(),
            // Shielded events serialize cleanly via serde — no
            // address fields, no leakage. CI privacy-grep (K2)
            // enforces this.
            "data": serde_json::to_value(evt).unwrap_or(Value::Null),
        }),
    }
}

fn mersennet_orders_event_data(event: &MersennetOrdersEvent, privacy_active: bool) -> Value {
    // Pre-fork the transparent CLOB is public by design — only redact once
    // the privacy hard fork has actually activated.
    if privacy_active && !event.is_privacy_safe_after_activation() {
        return json!({
            "redacted": true,
            "reason": "privacy_mode_sensitive_event",
            "kind": event.kind(),
        });
    }

    match event {
        MersennetOrdersEvent::MarketAdded {
            market_id,
            symbol,
            tick_size,
            lot_size,
        } => json!({
            "market_id": hex_u64(market_id.0),
            "symbol": symbol,
            "tick_size": hex_u256(*tick_size),
            "lot_size": hex_u256(*lot_size),
        }),
        MersennetOrdersEvent::OrderSubmitted {
            order_id,
            owner,
            market_id,
            side,
            price,
            size,
            tif,
            filled,
            remaining,
        } => json!({
            "order_id": order_id.map(|id| hex_u64(id.0)),
            "owner": hex_address(*owner),
            "market_id": hex_u64(market_id.0),
            "side": match side {
                mersennet::mersennet_orders::Side::Buy => "buy",
                mersennet::mersennet_orders::Side::Sell => "sell",
            },
            "price": hex_u256(*price),
            "size": hex_u256(*size),
            "tif": match tif {
                mersennet::mersennet_orders::TimeInForce::Gtc => "gtc",
                mersennet::mersennet_orders::TimeInForce::Ioc => "ioc",
                mersennet::mersennet_orders::TimeInForce::Fok => "fok",
            },
            "filled": hex_u256(*filled),
            "remaining": hex_u256(*remaining),
        }),
        MersennetOrdersEvent::OrderCancelled {
            order_id,
            owner,
            market_id,
        } => json!({
            "order_id": hex_u64(order_id.0),
            "owner": hex_address(*owner),
            "market_id": hex_u64(market_id.0),
        }),
        MersennetOrdersEvent::Trade {
            taker,
            maker,
            market_id,
            side,
            price,
            size,
        } => json!({
            "taker": hex_address(*taker),
            "maker": hex_address(*maker),
            "market_id": hex_u64(market_id.0),
            "side": match side {
                mersennet::mersennet_orders::Side::Buy => "buy",
                mersennet::mersennet_orders::Side::Sell => "sell",
            },
            "price": hex_u256(*price),
            "size": hex_u256(*size),
        }),
        MersennetOrdersEvent::MarginParamsUpdated {
            initial_bps,
            maintenance_bps,
        } => json!({
            "initial_bps": initial_bps,
            "maintenance_bps": maintenance_bps,
        }),
        MersennetOrdersEvent::CollateralDeposited { owner, amount } => json!({
            "owner": hex_address(*owner),
            "amount": hex_u256(*amount),
        }),
        MersennetOrdersEvent::Liquidation { owner, liquidated } => json!({
            "owner": hex_address(*owner),
            "liquidated": liquidated,
        }),
    }
}

fn bridge_event_data(event: &BridgeEvent) -> Value {
    match event {
        BridgeEvent::Enqueued { queue, message } => json!({
            "queue": bridge_queue_kind_to_str(queue),
            "nonce": hex_u64(message.nonce),
            "from": bridge_domain_to_str(&message.from),
            "to": bridge_domain_to_str(&message.to),
            "payload": format!("0x{}", hex::encode(message.payload.as_ref())),
        }),
        BridgeEvent::Dequeued { queue, message } => json!({
            "queue": bridge_queue_kind_to_str(queue),
            "nonce": hex_u64(message.nonce),
            "from": bridge_domain_to_str(&message.from),
            "to": bridge_domain_to_str(&message.to),
            "payload": format!("0x{}", hex::encode(message.payload.as_ref())),
        }),
    }
}

fn bridge_queue_kind_to_str(queue: &BridgeQueueKind) -> &'static str {
    match queue {
        BridgeQueueKind::OrdersToEvm => "orders_to_evm",
        BridgeQueueKind::EvmToOrders => "evm_to_orders",
    }
}

fn bridge_domain_to_str(domain: &BridgeDomain) -> &'static str {
    match domain {
        BridgeDomain::MersennetOrders => "mersennet_orders",
        BridgeDomain::MersennetEvm => "mersennet_evm",
    }
}

fn receipt_to_dto(
    block: &Block,
    tx: &Transaction,
    receipt: &Receipt,
    index: u64,
    hash: B256,
) -> ReceiptDto {
    let logs = receipt
        .logs
        .iter()
        .enumerate()
        .map(|(log_index, log)| log_to_dto(block, hash, index, log_index as u64, log))
        .collect();
    ReceiptDto {
        transaction_hash: hex_b256(hash),
        block_hash: hex_b256(block.hash),
        block_number: hex_u64(block.number),
        transaction_index: hex_u64(index),
        from: hex_address(tx.from),
        to: tx.to.map(hex_address),
        gas_used: hex_u64(receipt.gas_used),
        cumulative_gas_used: hex_u64(receipt.gas_used),
        effective_gas_price: hex_u256(tx.gas_price),
        status: if receipt.success {
            "0x1".to_string()
        } else {
            "0x0".to_string()
        },
        contract_address: receipt.created_address.map(hex_address),
        logs_bloom: logs_bloom_hex(&receipt.logs),
        tx_type: "0x0".to_string(),
        logs,
    }
}

fn log_to_dto(
    block: &Block,
    tx_hash: B256,
    tx_index: u64,
    log_index: u64,
    log: &LogEntry,
) -> LogDto {
    LogDto {
        address: hex_address(log.address),
        topics: log.topics.iter().map(|topic| hex_b256(*topic)).collect(),
        data: format!("0x{}", hex::encode(&log.data)),
        block_number: hex_u64(block.number),
        block_hash: hex_b256(block.hash),
        transaction_hash: hex_b256(tx_hash),
        transaction_index: hex_u64(tx_index),
        log_index: hex_u64(log_index),
        removed: false,
    }
}

fn collect_logs(engine: &Engine, filter: &LogFilter) -> Vec<LogDto> {
    let mut logs = Vec::new();
    for block in &engine.chain {
        if block.number < filter.from_block || block.number > filter.to_block {
            continue;
        }
        for (tx_index, (tx, receipt)) in block.transactions.iter().zip(&block.receipts).enumerate()
        {
            let tx_hash = tx_hash(tx);
            for (log_index, log) in receipt.logs.iter().enumerate() {
                if !filter.addresses.is_empty() && !filter.addresses.contains(&log.address) {
                    continue;
                }
                if !topics_match(&log.topics, &filter.topics) {
                    continue;
                }
                logs.push(log_to_dto(
                    block,
                    tx_hash,
                    tx_index as u64,
                    log_index as u64,
                    log,
                ));
            }
        }
    }
    logs
}

fn topics_match(log_topics: &[B256], filters: &[TopicFilter]) -> bool {
    for (index, filter) in filters.iter().enumerate() {
        let Some(topic) = log_topics.get(index) else {
            return false;
        };
        match filter {
            TopicFilter::Any => {}
            TopicFilter::OneOf(options) => {
                if !options.iter().any(|value| value == topic) {
                    return false;
                }
            }
        }
    }
    true
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;
    use revm::primitives::Bytes;
    use mersennet::engine::Engine;
    use mersennet::events::{DomainEvent, MersennetOrdersEvent};
    use mersennet::mersennet_orders::{Side, TimeInForce};
    use tempfile::TempDir;

    #[test]
    fn logs_bloom_empty_is_zero() {
        assert_eq!(compute_logs_bloom(&[]), [0u8; 256]);
    }

    #[test]
    fn throttle_ip_uses_forwarded_headers_only_for_loopback_peers() {
        let lo: IpAddr = "127.0.0.1".parse().unwrap();
        let direct: IpAddr = "198.51.100.4".parse().unwrap();
        let client: IpAddr = "203.0.113.9".parse().unwrap();
        // Direct peers are never overridden by (spoofable) headers.
        assert_eq!(throttle_ip(direct, Some("203.0.113.9"), None), direct);
        // Loopback = local reverse proxy: trust CF-Connecting-IP first…
        assert_eq!(throttle_ip(lo, Some("203.0.113.9"), Some("192.0.2.1")), client);
        // …then the LAST X-Forwarded-For hop (the one our own proxy appended;
        // the first entry is client-controlled and spoofable).
        assert_eq!(
            throttle_ip(lo, None, Some("198.51.100.4, 203.0.113.9")),
            client
        );
        // …and fall back to the peer when neither is present or parseable.
        assert_eq!(throttle_ip(lo, Some("garbage"), None), lo);
    }

    #[test]
    fn rate_limiter_caps_per_ip_per_window() {
        let mut limiter = RateLimiter::new();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        for _ in 0..RATE_LIMIT_MAX_PER_WINDOW {
            assert!(limiter.allow(ip), "requests within the cap must pass");
        }
        assert!(!limiter.allow(ip), "request beyond the cap must be denied");
        // A different source IP is unaffected.
        let other: IpAddr = "203.0.113.8".parse().unwrap();
        assert!(limiter.allow(other));
    }

    #[test]
    fn logs_bloom_matches_geth_bloom9_for_zero_address() {
        // keccak256(20 zero bytes) = 0x5380c7b7…, whose byte pairs 0/2/4 select
        // bloom bits 896, 1665, 1975 per go-ethereum's bloom9. This pins both the
        // bit math and the big-endian orientation, so external clients that trust
        // logsBloom agree.
        let log = LogEntry {
            address: Address::ZERO,
            topics: vec![],
            data: Bytes::new(),
        };
        let bloom = compute_logs_bloom(std::slice::from_ref(&log));
        let bit_set = |bit: usize| bloom[256 - 1 - bit / 8] & (1u8 << (bit % 8)) != 0;
        for bit in [896usize, 1665, 1975] {
            assert!(bit_set(bit), "expected bloom bit {bit} to be set");
        }
        let popcount: u32 = bloom.iter().map(|b| b.count_ones()).sum();
        assert_eq!(popcount, 3, "a single zero-address log sets exactly 3 bits");
    }

    #[test]
    fn block_dto_hides_sensitive_mersennet_orders_events_after_privacy_activation() {
        let temp_dir = TempDir::new().expect("temp dir");
        let mut engine = Engine::new_with_state(1, temp_dir.path());
        engine.activate_privacy_mode();

        let market_id =
            engine.mersennet_orders_add_market("MRSN-PERP", U256::from(1u64), U256::from(1u64));
        let trader = Address::from_slice(&[0x44; 20]);
        engine.mersennet_orders_deposit_collateral(trader, U256::from(10u64));
        let _ = engine
            .mersennet_orders_submit_order(
                trader,
                market_id,
                Side::Buy,
                U256::from(100u64),
                U256::from(1u64),
                TimeInForce::Gtc,
            )
            .expect("order accepted");

        let block = engine.execute_block().expect("block executed");
        let dto = block_to_dto(&block, false);

        assert!(
            dto.domain_events.iter().any(|event| {
                event.get("kind").and_then(|v| v.as_str()) == Some("market_added")
            })
        );
        assert!(dto.domain_events.iter().all(|event| {
            !matches!(
                event.get("kind").and_then(|v| v.as_str()),
                Some("order_submitted")
                    | Some("order_cancelled")
                    | Some("trade")
                    | Some("collateral_deposited")
                    | Some("liquidation")
            )
        }));

        assert!(block.domain_events.iter().all(|event| !matches!(
            event,
            DomainEvent::MersennetOrders(
                MersennetOrdersEvent::OrderSubmitted { .. }
                    | MersennetOrdersEvent::OrderCancelled { .. }
                    | MersennetOrdersEvent::Trade { .. }
                    | MersennetOrdersEvent::CollateralDeposited { .. }
                    | MersennetOrdersEvent::Liquidation { .. }
            )
        )));
    }

    fn test_request(method: &str, params: Value) -> RpcRequest {
        RpcRequest {
            _jsonrpc: Some("2.0".to_string()),
            id: Value::from(1),
            method: method.to_string(),
            params: Some(params),
        }
    }

    fn test_engine() -> (TempDir, Arc<Mutex<Engine>>, FilterStore) {
        let temp_dir = TempDir::new().expect("temp dir");
        let engine = Arc::new(Mutex::new(Engine::new_with_state(1, temp_dir.path())));
        let filters: FilterStore = Arc::new(Mutex::new(FilterState::new()));
        (temp_dir, engine, filters)
    }

    #[test]
    fn handle_body_single_call_returns_object() {
        let (_dir, engine, filters) = test_engine();
        let body = r#"{"jsonrpc":"2.0","id":7,"method":"eth_chainId","params":[]}"#;
        let out = handle_body(body, &engine, &filters).expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["id"], Value::from(7));
        assert_eq!(v["result"], Value::from("0x1"));
    }

    #[test]
    fn handle_body_batch_returns_array_in_order() {
        let (_dir, engine, filters) = test_engine();
        let body = r#"[
            {"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]},
            {"jsonrpc":"2.0","id":2,"method":"eth_blockNumber","params":[]},
            {"jsonrpc":"2.0","id":3,"method":"no_such_method","params":[]}
        ]"#;
        let out = handle_body(body, &engine, &filters).expect("response");
        let v: Value = serde_json::from_str(&out).expect("json array");
        let arr = v.as_array().expect("array response");
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["id"], Value::from(1));
        assert_eq!(arr[0]["result"], Value::from("0x1"));
        assert_eq!(arr[1]["id"], Value::from(2));
        assert!(arr[1]["result"].is_string());
        // Per-call failures answer in place without failing the batch.
        assert_eq!(arr[2]["id"], Value::from(3));
        assert_eq!(arr[2]["error"]["code"], Value::from(-32601));
    }

    #[test]
    fn handle_body_empty_batch_is_invalid_request() {
        let (_dir, engine, filters) = test_engine();
        let out = handle_body("[]", &engine, &filters).expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["error"]["code"], Value::from(-32600));
    }

    #[test]
    fn handle_body_oversized_batch_rejected() {
        let (_dir, engine, filters) = test_engine();
        let call = r#"{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}"#;
        let body = format!("[{}]", vec![call; MAX_BATCH_CALLS + 1].join(","));
        let out = handle_body(&body, &engine, &filters).expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["error"]["code"], Value::from(-32600));
    }

    #[test]
    fn handle_body_malformed_batch_element_answers_in_place() {
        let (_dir, engine, filters) = test_engine();
        let body = r#"[{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}, 42]"#;
        let out = handle_body(body, &engine, &filters).expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["result"], Value::from("0x1"));
        assert_eq!(arr[1]["error"]["code"], Value::from(-32600));
    }

    #[test]
    fn handle_body_garbage_is_parse_error() {
        let (_dir, engine, filters) = test_engine();
        let out = handle_body("not json at all", &engine, &filters).expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["error"]["code"], Value::from(-32700));
    }

    #[test]
    fn handle_body_oversized_body_rejected() {
        let (_dir, engine, filters) = test_engine();
        let body = "x".repeat((MAX_BODY_BYTES + 1) as usize);
        let out = handle_body(&body, &engine, &filters).expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["error"]["code"], Value::from(-32600));
    }

    #[test]
    fn dispatch_rejects_transparent_log_and_pending_filters_after_privacy_activation() {
        let temp_dir = TempDir::new().expect("temp dir");
        let mut engine = Engine::new_with_state(1, temp_dir.path());
        engine.activate_privacy_mode();

        let engine = Arc::new(Mutex::new(engine));
        let filters: FilterStore = Arc::new(Mutex::new(FilterState::new()));

        let log_err = dispatch(test_request("eth_getLogs", json!([{}])), &engine, &filters)
            .expect_err("eth_getLogs should be disabled");
        assert_eq!(log_err.1.code, -32605);
        assert!(log_err.1.message.contains("event/log RPC disabled"));

        let new_filter_err = dispatch(
            test_request("eth_newFilter", json!([{}])),
            &engine,
            &filters,
        )
        .expect_err("eth_newFilter should be disabled");
        assert_eq!(new_filter_err.1.code, -32605);
        assert!(new_filter_err.1.message.contains("event/log RPC disabled"));

        let pending_err = dispatch(
            test_request("eth_newPendingTransactionFilter", json!([])),
            &engine,
            &filters,
        )
        .expect_err("pending tx filter should be disabled");
        assert_eq!(pending_err.1.code, -32605);
        assert!(
            pending_err
                .1
                .message
                .contains("pending-transaction RPC disabled")
        );
    }

    #[test]
    fn dispatch_rejects_existing_sensitive_filters_after_privacy_activation() {
        let temp_dir = TempDir::new().expect("temp dir");
        let engine = Arc::new(Mutex::new(Engine::new_with_state(1, temp_dir.path())));
        let filters: FilterStore = Arc::new(Mutex::new(FilterState::new()));

        let log_filter_id = {
            let mut fs = filters.lock().expect("filter lock");
            fs.install(
                FilterKind::Log(LogFilter {
                    from_block: 0,
                    to_block: 0,
                    addresses: Vec::new(),
                    topics: Vec::new(),
                }),
                0,
            )
        };
        let pending_filter_id = {
            let mut fs = filters.lock().expect("filter lock");
            fs.install(FilterKind::PendingTx, 0)
        };

        engine.lock().expect("engine lock").activate_privacy_mode();

        let log_changes_err = dispatch(
            test_request(
                "eth_getFilterChanges",
                json!([format!("0x{log_filter_id:x}")]),
            ),
            &engine,
            &filters,
        )
        .expect_err("log filter changes should be disabled");
        assert_eq!(log_changes_err.1.code, -32605);
        assert!(log_changes_err.1.message.contains("event/log RPC disabled"));

        let log_logs_err = dispatch(
            test_request("eth_getFilterLogs", json!([format!("0x{log_filter_id:x}")])),
            &engine,
            &filters,
        )
        .expect_err("log filter logs should be disabled");
        assert_eq!(log_logs_err.1.code, -32605);
        assert!(log_logs_err.1.message.contains("event/log RPC disabled"));

        let pending_changes_err = dispatch(
            test_request(
                "eth_getFilterChanges",
                json!([format!("0x{pending_filter_id:x}")]),
            ),
            &engine,
            &filters,
        )
        .expect_err("pending filter changes should be disabled");
        assert_eq!(pending_changes_err.1.code, -32605);
        assert!(
            pending_changes_err
                .1
                .message
                .contains("pending-transaction RPC disabled")
        );
    }

    #[test]
    fn dispatch_rejects_transaction_metadata_methods_after_privacy_activation() {
        let temp_dir = TempDir::new().expect("temp dir");
        let mut eng = Engine::new_with_state(1, temp_dir.path());
        let from = Address::from_slice(&[0x11; 20]);
        let to = Address::from_slice(&[0x22; 20]);
        eng.fund_account(from, U256::from(1_000_000u64), 0);
        eng.submit_tx_unsigned(Transaction {
            from,
            to: Some(to),
            value: U256::from(1u64),
            data: Bytes::new(),
            gas_limit: 21_000,
            gas_price: U256::from(1u64),
            nonce: 0,
            chain_id: Some(1),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
        })
        .expect("tx accepted");
        let block = eng.execute_block().expect("block executed");
        let hash = tx_hash(&block.transactions[0]);
        eng.activate_privacy_mode();

        let engine = Arc::new(Mutex::new(eng));
        let filters: FilterStore = Arc::new(Mutex::new(FilterState::new()));

        let tx_err = dispatch(
            test_request("eth_getTransactionByHash", json!([hex_b256(hash)])),
            &engine,
            &filters,
        )
        .expect_err("tx metadata should be disabled");
        assert_eq!(tx_err.1.code, -32605);
        assert!(
            tx_err
                .1
                .message
                .contains("transaction/receipt metadata RPC disabled")
        );

        let receipt_err = dispatch(
            test_request("eth_getTransactionReceipt", json!([hex_b256(hash)])),
            &engine,
            &filters,
        )
        .expect_err("receipt metadata should be disabled");
        assert_eq!(receipt_err.1.code, -32605);
        assert!(
            receipt_err
                .1
                .message
                .contains("transaction/receipt metadata RPC disabled")
        );

        let block_err = dispatch(
            test_request("eth_getBlockByNumber", json!(["latest", true])),
            &engine,
            &filters,
        )
        .expect_err("full block tx metadata should be disabled");
        assert_eq!(block_err.1.code, -32605);
        assert!(
            block_err
                .1
                .message
                .contains("transaction/receipt metadata RPC disabled")
        );

        let block_ok = dispatch(
            test_request("eth_getBlockByNumber", json!(["latest", false])),
            &engine,
            &filters,
        )
        .expect("header-only block lookup should remain enabled");
        assert!(block_ok.contains(&hex_b256(hash)));
    }
}

fn tx_hash(tx: &Transaction) -> B256 {
    // Prefer the canonical hash captured at decode time (keccak256 of the
    // raw RLP envelope) so MetaMask/ethers-submitted txs are found by the
    // exact hash the wallet computed.
    if let Some(h) = tx.hash {
        return h;
    }
    let mut payload = Vec::new();
    payload.extend_from_slice(tx.from.as_slice());
    payload.push(if tx.to.is_some() { 1 } else { 0 });
    if let Some(to) = tx.to {
        payload.extend_from_slice(to.as_slice());
    }
    payload.extend_from_slice(&tx.value.to_be_bytes::<32>());
    payload.extend_from_slice(&tx.gas_price.to_be_bytes::<32>());
    payload.extend_from_slice(&tx.gas_limit.to_be_bytes());
    payload.extend_from_slice(&tx.nonce.to_be_bytes());
    payload.extend_from_slice(tx.data.as_ref());
    payload.extend_from_slice(&tx.chain_id.unwrap_or_default().to_be_bytes());
    revm::primitives::keccak256(payload)
}

fn parse_hex_u64(input: &str) -> Result<u64, RpcInputError> {
    let stripped = input.strip_prefix("0x").unwrap_or(input);
    u64::from_str_radix(stripped, 16).map_err(|err| RpcInputError::InvalidHex(err.to_string()))
}

fn parse_address(value: &str) -> Result<Address, RpcInputError> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(stripped).map_err(|err| RpcInputError::HexDecode(err.to_string()))?;
    if bytes.len() != 20 {
        return Err(RpcInputError::InvalidAddressLength);
    }
    Ok(Address::from_slice(&bytes))
}

fn parse_hash(value: &str) -> Result<B256, RpcInputError> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(stripped).map_err(|err| RpcInputError::HexDecode(err.to_string()))?;
    if bytes.len() != 32 {
        return Err(RpcInputError::InvalidHashLength);
    }
    Ok(B256::from_slice(&bytes))
}

fn hex_u64(value: u64) -> String {
    format!("0x{:x}", value)
}

fn hex_u256(value: U256) -> String {
    let bytes = value.to_be_bytes::<32>();
    let mut leading = 0;
    while leading < bytes.len() && bytes[leading] == 0 {
        leading += 1;
    }
    if leading == bytes.len() {
        return "0x0".to_string();
    }
    format!("0x{}", hex::encode(&bytes[leading..]))
}

fn hex_address(address: Address) -> String {
    format!("0x{}", hex::encode(address.as_slice()))
}

fn hex_b256(hash: B256) -> String {
    format!("0x{}", hex::encode(hash.as_slice()))
}

/// Ethereum 2048-bit logs bloom (go-ethereum `bloom9`): for a log's address and
/// each topic, keccak-hash it and set three bits chosen from byte pairs 0/2/4 of
/// the hash. Returns the 256-byte bloom as a `0x`-prefixed 512-hex string.
fn compute_logs_bloom(logs: &[LogEntry]) -> [u8; 256] {
    let mut bloom = [0u8; 256];
    let mut add = |bytes: &[u8]| {
        let hash = revm::primitives::keccak256(bytes);
        for i in [0usize, 2, 4] {
            let bit = (((hash[i] as usize) << 8) | hash[i + 1] as usize) & 0x7ff;
            // Bit `bit` counted from the low end of the 2048-bit big-endian array.
            bloom[256 - 1 - bit / 8] |= 1u8 << (bit % 8);
        }
    };
    for log in logs {
        add(log.address.as_slice());
        for topic in &log.topics {
            add(topic.as_slice());
        }
    }
    bloom
}

fn logs_bloom_hex(logs: &[LogEntry]) -> String {
    format!("0x{}", hex::encode(compute_logs_bloom(logs)))
}

/// OR-fold the per-receipt blooms into one block-level bloom.
fn block_logs_bloom_hex(block: &Block) -> String {
    let mut bloom = [0u8; 256];
    for receipt in &block.receipts {
        let rb = compute_logs_bloom(&receipt.logs);
        for (b, r) in bloom.iter_mut().zip(rb.iter()) {
            *b |= *r;
        }
    }
    format!("0x{}", hex::encode(bloom))
}
