use prime_chain::bridge::BridgeDomain;
use prime_chain::engine::{Block, Engine, LogEntry, Receipt, Transaction};
use prime_chain::events::{BridgeEvent, BridgeQueueKind, DomainEvent, PrimeOrdersEvent};
use prime_chain::errors::RpcInputError;
use crate::rpc_router;
use anyhow::{anyhow, Result};
use revm::primitives::{Address, B256, Bytes, U256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Response, Server};
use prime_chain::prometheus;
use tracing::info;

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
        Self { filters: HashMap::new(), next_id: 1 }
    }

    fn install(&mut self, kind: FilterKind, current_block: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.filters.insert(id, InstalledFilter { kind, last_poll_block: current_block });
        id
    }

    fn remove(&mut self, id: u64) -> bool {
        self.filters.remove(&id).is_some()
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TxInput {
    from: String,
    to: Option<String>,
    value: Option<String>,
    data: Option<String>,
    gas: Option<String>,
    #[serde(alias = "gas_price")]
    gas_price: Option<String>,
    nonce: Option<String>,
    #[serde(alias = "chain_id")]
    chain_id: Option<u64>,
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
    info!("RPC listening on http://{addr}");

    for request in server.incoming_requests() {
        handle_request(request, &engine, &filters)?;
    }

    Ok(())
}

fn handle_request(mut request: tiny_http::Request, engine: &Arc<Mutex<Engine>>, filters: &FilterStore) -> Result<()> {
    // Expose metrics on GET /metrics
    if request.method() == &Method::Get && request.url() == "/metrics" {
        if let Some(handle) = prometheus::handle() {
            metrics::increment_counter!("prime_chain_rpc_requests", "method" => "metrics");
            let body = handle.render();
            let response = Response::from_string(body).with_header(
                Header::from_bytes("Content-Type", "text/plain; version=0.0.4").map_err(|_| anyhow!("invalid header"))?,
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

    let mut content = String::new();
    request.as_reader().read_to_string(&mut content)?;
    let parsed: Result<RpcRequest, _> = serde_json::from_str(&content);
    let response = match parsed {
        Ok(call) => {
            let method = call.method.clone();
            match dispatch(call, engine, filters) {
                Ok(payload) => payload,
                Err((id, error)) => {
                    metrics::increment_counter!(
                        "prime_chain_rpc_errors",
                        "code" => error.code.to_string(),
                        "method" => method.clone()
                    );
                    tracing::warn!(method = %method, code = error.code, message = %error.message, "rpc request failed");
                    serde_json::to_string(&RpcErrorResponse {
                        jsonrpc: "2.0",
                        id,
                        error,
                    })?
                }
            }
        }
        Err(err) => serde_json::to_string(&RpcErrorResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            error: RpcError {
                code: -32700,
                message: format!("invalid json: {err}"),
                data: None,
            },
        })?,
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

fn dispatch(call: RpcRequest, engine: &Arc<Mutex<Engine>>, filters: &FilterStore) -> Result<String, (Value, RpcError)> {
    let id = call.id.clone();
    let start = std::time::Instant::now();
    // count requests per method
    metrics::increment_counter!("prime_chain_rpc_requests", "method" => call.method.clone());

    let result = match call.method.as_str() {
        "prime_chainId" | "eth_chainId"
        | "prime_blockNumber" | "eth_blockNumber"
        | "prime_getBalance" | "eth_getBalance"
        | "prime_getDomainEvents"
        | "prime_gasPrice" | "eth_gasPrice"
        | "prime_validators"
        | "prime_getCode" | "eth_getCode"
        | "prime_getStorageAt" | "eth_getStorageAt"
        | "prime_getTransactionCount" | "eth_getTransactionCount"
        | "prime_call" | "eth_call"
        | "eth_estimateGas"
        | "net_version" | "web3_clientVersion" => {
            let params = call.params.unwrap_or(Value::Null);
            let mut engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match rpc_router::route(call.method.as_str(), params, &mut engine) {
                Ok(value) => value,
                Err(err) => return Err((id.clone(), rpc_error_with_code(err.code, err.message))),
            }
        }
        "prime_getBlockByNumber" | "eth_getBlockByNumber" => {
            let params = call.params.unwrap_or(Value::Null);
            let (number, include_txs) = parse_block_params(params, engine, id.clone())
                .map_err(|(id, message)| (id, rpc_error_invalid_params(message)))?;
            let engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let block = engine.block_by_number(number);
            match block {
                Some(block) => serde_json::to_value(block_to_dto(block, include_txs))
                    .map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?,
                None => Value::Null,
            }
        }
        "prime_getTransactionReceipt" | "eth_getTransactionReceipt" => {
            let params = call.params.unwrap_or(Value::Null);
            let tx_hash = parse_hash_param(params)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let mut engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let receipt = find_receipt(&mut engine, tx_hash);
            match receipt {
                Some(dto) => serde_json::to_value(dto)
                    .map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?,
                None => Value::Null,
            }
        }
        "prime_getTransactionByHash" | "eth_getTransactionByHash" => {
            let params = call.params.unwrap_or(Value::Null);
            let tx_hash = parse_hash_param(params)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let result = find_transaction(&engine, tx_hash);
            match result {
                Some((tx, block, idx)) => serde_json::to_value(tx_to_dto_with_block(
                    tx,
                    hex_b256(block.hash),
                    hex_u64(block.number),
                    idx,
                )).map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?,
                None => Value::Null,
            }
        }
        "eth_sendRawTransaction" => {
            let params = call.params.unwrap_or(Value::Null);
            let raw_hex = parse_raw_tx_param(params)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let signed = prime_chain::crypto::decode_raw_signed_tx(&raw_hex)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let mut engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let tx_hash = tx_hash(&signed.tx);
            engine
                .submit_tx(signed.tx)
                .map_err(|err| {
                    let data = json!({ "reason": err.code() });
                    (id.clone(), rpc_error_with_data(-32005, format!("tx rejected: {}", err), data))
                })?;
            Value::String(hex_b256(tx_hash))
        }
        "prime_sendTransaction" | "eth_sendTransaction" => {
            let params = call.params.unwrap_or(Value::Null);
            let tx = parse_tx_input(params)
                .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?;
            let mut engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let tx_hash = tx_hash(&tx);
            engine
                .submit_tx_unsigned(tx)
                .map_err(|err| {
                    let data = json!({
                        "reason": err.code(),
                    });
                    (
                        id.clone(),
                        rpc_error_with_data(-32005, format!("tx rejected: {}", err), data),
                    )
                })?;
            Value::String(hex_b256(tx_hash))
        }
        "prime_getLogs" | "eth_getLogs" => {
            let params = call.params.unwrap_or(Value::Null);
            let filter = parse_log_filter(params, engine, id.clone())
                .map_err(|(id, message)| (id, rpc_error_invalid_params(message)))?;
            let engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
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
            let hash = match array.get(0) {
                Some(Value::String(value)) => parse_hash(value)
                    .map_err(|err| (id.clone(), rpc_error_invalid_params(err.to_string())))?,
                _ => return Err((id.clone(), rpc_error_invalid_params("block hash required"))),
            };
            let include_txs = match array.get(1) {
                Some(Value::Bool(value)) => *value,
                _ => false,
            };
            let engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match engine.block_by_hash(hash) {
                Some(block) => serde_json::to_value(block_to_dto(block, include_txs))
                    .map_err(|err| (id.clone(), rpc_error_internal(err.to_string())))?,
                None => Value::Null,
            }
        }
        "eth_feeHistory" => {
            let params = call.params.unwrap_or(Value::Null);
            let engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let latest = engine.latest_height();
            let array = match params { Value::Array(v) => v, _ => vec![] };
            let block_count = match array.get(0) {
                Some(Value::String(s)) => parse_hex_u64(s).unwrap_or(1),
                Some(Value::Number(n)) => n.as_u64().unwrap_or(1),
                _ => 1,
            };
            let newest = match array.get(1) {
                Some(Value::String(s)) if s == "latest" || s == "pending" => latest,
                Some(Value::String(s)) => parse_hex_u64(s).unwrap_or(latest),
                _ => latest,
            };
            let oldest = newest.saturating_sub(block_count.saturating_sub(1));
            let base_fees: Vec<String> = (oldest..=newest + 1)
                .map(|n| {
                    engine.block_by_number(n)
                        .map(|b| hex_u256(b.base_fee))
                        .unwrap_or_else(|| "0x1".to_string())
                })
                .collect();
            let gas_ratios: Vec<Vec<f64>> = (oldest..=newest)
                .map(|n| {
                    engine.block_by_number(n)
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
        "eth_maxPriorityFeePerGas" => {
            Value::String("0x0".to_string())
        }
        "eth_accounts" => {
            Value::Array(vec![])
        }
        "eth_mining" => {
            Value::Bool(false)
        }
        "eth_syncing" => {
            Value::Bool(false)
        }
        "eth_getUncleCountByBlockNumber" | "eth_getUncleCountByBlockHash" => {
            Value::String("0x0".to_string())
        }
        "eth_protocolVersion" => {
            Value::String("0x41".to_string())
        }
        "eth_newFilter" => {
            let params = call.params.unwrap_or(Value::Null);
            let filter = parse_log_filter(params, engine, id.clone())
                .map_err(|(id, message)| (id, rpc_error_invalid_params(message)))?;
            let latest = {
                let eng = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                eng.latest_height()
            };
            let mut fs = filters.lock().map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter_id = fs.install(FilterKind::Log(filter), latest);
            Value::String(hex_u64(filter_id))
        }
        "eth_newBlockFilter" => {
            let latest = {
                let eng = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                eng.latest_height()
            };
            let mut fs = filters.lock().map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter_id = fs.install(FilterKind::Block, latest);
            Value::String(hex_u64(filter_id))
        }
        "eth_newPendingTransactionFilter" => {
            let latest = {
                let eng = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                eng.latest_height()
            };
            let mut fs = filters.lock().map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter_id = fs.install(FilterKind::PendingTx, latest);
            Value::String(hex_u64(filter_id))
        }
        "eth_getFilterChanges" => {
            let params = call.params.unwrap_or(Value::Null);
            let array = match params { Value::Array(v) => v, _ => vec![] };
            let filter_id = match array.get(0) {
                Some(Value::String(s)) => parse_hex_u64(s).map_err(|e| (id.clone(), rpc_error_invalid_params(e.to_string())))?,
                _ => return Err((id.clone(), rpc_error_invalid_params("filter id required"))),
            };
            let mut fs = filters.lock().map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter = fs.filters.get_mut(&filter_id)
                .ok_or_else(|| (id.clone(), rpc_error_invalid_params("filter not found")))?;
            let engine_guard = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            let latest = engine_guard.latest_height();
            let from_block = filter.last_poll_block + 1;

            let result = match &filter.kind {
                FilterKind::Log(log_filter) => {
                    let mut f = log_filter.clone();
                    f.from_block = from_block;
                    f.to_block = latest;
                    let logs = collect_logs(&engine_guard, &f);
                    serde_json::to_value(logs).map_err(|e| (id.clone(), rpc_error_internal(e.to_string())))?
                }
                FilterKind::Block => {
                    let hashes: Vec<Value> = (from_block..=latest)
                        .filter_map(|n| engine_guard.block_by_number(n))
                        .map(|b| Value::String(hex_b256(b.hash)))
                        .collect();
                    Value::Array(hashes)
                }
                FilterKind::PendingTx => {
                    Value::Array(vec![])
                }
            };
            filter.last_poll_block = latest;
            result
        }
        "eth_getFilterLogs" => {
            let params = call.params.unwrap_or(Value::Null);
            let array = match params { Value::Array(v) => v, _ => vec![] };
            let filter_id = match array.get(0) {
                Some(Value::String(s)) => parse_hex_u64(s).map_err(|e| (id.clone(), rpc_error_invalid_params(e.to_string())))?,
                _ => return Err((id.clone(), rpc_error_invalid_params("filter id required"))),
            };
            let fs = filters.lock().map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            let filter = fs.filters.get(&filter_id)
                .ok_or_else(|| (id.clone(), rpc_error_invalid_params("filter not found")))?;
            match &filter.kind {
                FilterKind::Log(log_filter) => {
                    let engine_guard = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
                    let logs = collect_logs(&engine_guard, log_filter);
                    serde_json::to_value(logs).map_err(|e| (id.clone(), rpc_error_internal(e.to_string())))?
                }
                _ => Value::Array(vec![]),
            }
        }
        "eth_uninstallFilter" => {
            let params = call.params.unwrap_or(Value::Null);
            let array = match params { Value::Array(v) => v, _ => vec![] };
            let filter_id = match array.get(0) {
                Some(Value::String(s)) => parse_hex_u64(s).map_err(|e| (id.clone(), rpc_error_invalid_params(e.to_string())))?,
                _ => return Err((id.clone(), rpc_error_invalid_params("filter id required"))),
            };
            let mut fs = filters.lock().map_err(|_| (id.clone(), rpc_error_internal("filter lock poisoned")))?;
            Value::Bool(fs.remove(filter_id))
        }
        "primeorders_addMarket" | "primeorders_submitOrder" | "primeorders_cancelOrder" | "primeorders_getOrderBook" | "primeorders_getOpenOrders" | "primeorders_setMarginParams" | "primeorders_depositCollateral" | "primeorders_isLiquidatable" | "primeorders_liquidate" => {
            let params = call.params.unwrap_or(Value::Null);
            let mut engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match rpc_router::route(call.method.as_str(), params, &mut engine) {
                Ok(value) => value,
                Err(err) => return Err((id.clone(), rpc_error_with_code(err.code, err.message))),
            }
        }
        "primebridge_enqueueOrdersToEvm" | "primebridge_enqueueEvmToOrders" | "primebridge_dequeueOrdersToEvm" | "primebridge_dequeueEvmToOrders" => {
            let params = call.params.unwrap_or(Value::Null);
            let mut engine = engine.lock().map_err(|_| (id.clone(), rpc_error_internal("engine lock poisoned")))?;
            match rpc_router::route(call.method.as_str(), params, &mut engine) {
                Ok(value) => value,
                Err(err) => return Err((id.clone(), rpc_error_with_code(err.code, err.message))),
            }
        }
        _ => {
            return Err((id, rpc_error_with_code(-32601, format!("method not found: {}", call.method))));
        }
    };

    // record request duration
    let dur = start.elapsed().as_secs_f64();
    metrics::histogram!("prime_chain_rpc_duration_seconds", dur, "method" => call.method.clone());

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

    let number = match array.get(0) {
        Some(Value::String(value)) if value == "latest" => {
            let engine = engine.lock().map_err(|_| (id.clone(), "engine lock poisoned".to_string()))?;
            engine.latest_height()
        }
        Some(Value::String(value)) => {
            parse_hex_u64(value).map_err(|err| (id.clone(), err.to_string()))?
        }
        Some(Value::Number(value)) => value.as_u64().ok_or((id.clone(), "invalid block number".to_string()))?,
        None => {
            let engine = engine.lock().map_err(|_| (id.clone(), "engine lock poisoned".to_string()))?;
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
    let hex_str = match array.get(0) {
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
    let hash = match array.get(0) {
        Some(Value::String(value)) => parse_hash(value)?,
        _ => return Err(RpcInputError::HashRequired),
    };
    Ok(hash)
}

fn parse_tx_input(params: Value) -> Result<Transaction, RpcInputError> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcInputError::InvalidParams),
    };
    let obj = array
        .get(0)
        .ok_or(RpcInputError::TransactionRequired)?
        .clone();
    let input: TxInput = serde_json::from_value(obj)
        .map_err(|err| RpcInputError::InvalidTransaction(err.to_string()))?;

    let from = parse_address(&input.from)?;
    let to = match input.to {
        Some(value) => Some(parse_address(&value)?),
        None => None,
    };
    let value = input
        .value
        .as_deref()
        .map(parse_hex_u256)
        .transpose()?
        .unwrap_or_else(|| U256::ZERO);
    let gas_limit = input
        .gas
        .as_deref()
        .map(parse_hex_u64)
        .transpose()?
        .unwrap_or(21_000);
    let gas_price = input
        .gas_price
        .as_deref()
        .map(parse_hex_u256)
        .transpose()?
        .unwrap_or_else(|| U256::ZERO);
    let nonce = input
        .nonce
        .as_deref()
        .map(parse_hex_u64)
        .transpose()?
        .unwrap_or(0);
    let data = input
        .data
        .as_deref()
        .map(parse_hex_bytes)
        .transpose()?
        .unwrap_or_else(Bytes::new);

    Ok(Transaction {
        from,
        to,
        value,
        data,
        gas_limit,
        gas_price,
        nonce,
        chain_id: input.chain_id,
        signature: None,
    })
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
    let raw = array.get(0).cloned().unwrap_or(Value::Null);
    let input: LogFilterInput = if raw.is_null() {
        LogFilterInput {
            from_block: None,
            to_block: None,
            address: None,
            topics: None,
        }
    } else {
        serde_json::from_value(raw)
            .map_err(|err| (id.clone(), RpcInputError::InvalidLogFilter(err.to_string()).to_string()))?
    };

    let latest = {
        let engine = engine.lock().map_err(|_| (id.clone(), "engine lock poisoned".to_string()))?;
        engine.latest_height()
    };

    let from_block = parse_block_tag(input.from_block, latest)
        .map_err(|err| (id.clone(), err.to_string()))?;
    let to_block = parse_block_tag(input.to_block, latest)
        .map_err(|err| (id.clone(), err.to_string()))?;
    let addresses = parse_filter_addresses(input.address)
        .map_err(|err| (id.clone(), err.to_string()))?;
    let topics = parse_filter_topics(input.topics)
        .map_err(|err| (id.clone(), err.to_string()))?;

    Ok(LogFilter {
        from_block,
        to_block,
        addresses,
        topics,
    })
}

fn parse_block_tag(value: Option<Value>, latest: u64) -> Result<u64, RpcInputError> {
    let Some(value) = value else { return Ok(latest); };
    match value {
        Value::String(tag) if tag == "latest" => Ok(latest),
        Value::String(hex) => parse_hex_u64(&hex),
        Value::Number(num) => num.as_u64().ok_or(RpcInputError::InvalidBlockTag),
        Value::Null => Ok(latest),
        _ => Err(RpcInputError::InvalidBlockTag),
    }
}

fn parse_filter_addresses(value: Option<Value>) -> Result<Vec<Address>, RpcInputError> {
    let Some(value) = value else { return Ok(Vec::new()); };
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
    let Some(values) = value else { return Ok(Vec::new()); };
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
                        Value::Null => {},
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

fn find_transaction<'a>(engine: &'a Engine, hash: B256) -> Option<(&'a Transaction, &'a Block, u64)> {
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
    let block_hash = hex_b256(block.hash);
    let block_number = hex_u64(block.number);

    let transactions = if include_txs {
        block
            .transactions
            .iter()
            .enumerate()
            .map(|(i, tx)| {
                serde_json::to_value(tx_to_dto_with_block(
                    tx, block_hash.clone(), block_number.clone(), i as u64,
                )).unwrap_or(Value::Null)
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
        parent_hash: if block.number > 0 {
            hex_b256(B256::from(U256::from(block.number - 1)))
        } else {
            hex_b256(B256::ZERO)
        },
        nonce: "0x0000000000000000".to_string(),
        sha3_uncles: hex_b256(B256::ZERO),
        logs_bloom: format!("0x{}", "0".repeat(512)),
        transactions_root: hex_b256(block.state_root),
        state_root: hex_b256(block.state_root),
        receipts_root: hex_b256(B256::ZERO),
        miner: hex_address(block.coinbase),
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
            .map(domain_event_to_value)
            .collect(),
    }
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

fn domain_event_to_value(event: &DomainEvent) -> Value {
    match event {
        DomainEvent::PrimeOrders(evt) => json!({
            "domain": "primeorders",
            "kind": evt.kind(),
            "data": prime_orders_event_data(evt),
        }),
        DomainEvent::Bridge(evt) => json!({
            "domain": "bridge",
            "kind": evt.kind(),
            "data": bridge_event_data(evt),
        }),
    }
}

fn prime_orders_event_data(event: &PrimeOrdersEvent) -> Value {
    match event {
        PrimeOrdersEvent::MarketAdded { market_id, symbol, tick_size, lot_size } => json!({
            "market_id": hex_u64(market_id.0),
            "symbol": symbol,
            "tick_size": hex_u256(*tick_size),
            "lot_size": hex_u256(*lot_size),
        }),
        PrimeOrdersEvent::OrderSubmitted {
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
                prime_chain::prime_orders::Side::Buy => "buy",
                prime_chain::prime_orders::Side::Sell => "sell",
            },
            "price": hex_u256(*price),
            "size": hex_u256(*size),
            "tif": match tif {
                prime_chain::prime_orders::TimeInForce::Gtc => "gtc",
                prime_chain::prime_orders::TimeInForce::Ioc => "ioc",
                prime_chain::prime_orders::TimeInForce::Fok => "fok",
            },
            "filled": hex_u256(*filled),
            "remaining": hex_u256(*remaining),
        }),
        PrimeOrdersEvent::OrderCancelled { order_id, owner, market_id } => json!({
            "order_id": hex_u64(order_id.0),
            "owner": hex_address(*owner),
            "market_id": hex_u64(market_id.0),
        }),
        PrimeOrdersEvent::Trade { taker, maker, market_id, side, price, size } => json!({
            "taker": hex_address(*taker),
            "maker": hex_address(*maker),
            "market_id": hex_u64(market_id.0),
            "side": match side {
                prime_chain::prime_orders::Side::Buy => "buy",
                prime_chain::prime_orders::Side::Sell => "sell",
            },
            "price": hex_u256(*price),
            "size": hex_u256(*size),
        }),
        PrimeOrdersEvent::MarginParamsUpdated { initial_bps, maintenance_bps } => json!({
            "initial_bps": initial_bps,
            "maintenance_bps": maintenance_bps,
        }),
        PrimeOrdersEvent::CollateralDeposited { owner, amount } => json!({
            "owner": hex_address(*owner),
            "amount": hex_u256(*amount),
        }),
        PrimeOrdersEvent::Liquidation { owner, liquidated } => json!({
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
        BridgeDomain::PrimeOrders => "primeorders",
        BridgeDomain::PrimeEvm => "primeevm",
    }
}

fn receipt_to_dto(block: &Block, tx: &Transaction, receipt: &Receipt, index: u64, hash: B256) -> ReceiptDto {
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
        status: if receipt.success { "0x1".to_string() } else { "0x0".to_string() },
        contract_address: receipt.created_address.map(hex_address),
        logs_bloom: format!("0x{}", "0".repeat(512)),
        tx_type: "0x0".to_string(),
        logs,
    }
}

fn log_to_dto(block: &Block, tx_hash: B256, tx_index: u64, log_index: u64, log: &LogEntry) -> LogDto {
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
        for (tx_index, (tx, receipt)) in block.transactions.iter().zip(&block.receipts).enumerate() {
            let tx_hash = tx_hash(tx);
            for (log_index, log) in receipt.logs.iter().enumerate() {
                if !filter.addresses.is_empty()
                    && !filter.addresses.iter().any(|addr| *addr == log.address)
                {
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

fn tx_hash(tx: &Transaction) -> B256 {
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
    u64::from_str_radix(stripped, 16)
        .map_err(|err| RpcInputError::InvalidHex(err.to_string()))
}

fn parse_hex_u256(input: &str) -> Result<U256, RpcInputError> {
    let stripped = input.strip_prefix("0x").unwrap_or(input);
    U256::from_str_radix(stripped, 16)
        .map_err(|err| RpcInputError::InvalidHex(err.to_string()))
}

fn parse_hex_bytes(input: &str) -> Result<Bytes, RpcInputError> {
    let stripped = input.strip_prefix("0x").unwrap_or(input);
    let bytes = hex::decode(stripped).map_err(|err| RpcInputError::HexDecode(err.to_string()))?;
    Ok(Bytes::from(bytes))
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
