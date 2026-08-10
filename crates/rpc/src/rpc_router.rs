use mersennet::bridge::{BridgeDomain, BridgeMessage};
use mersennet::engine::Engine;
use mersennet::errors::MersennetOrdersError;
use mersennet::events::{
    BridgeEvent, BridgeQueueKind, DomainEvent, DomainEventRecord, MersennetOrdersEvent,
};
use mersennet::mersennet_orders::{Order, OrderBookView, OrderOutcome, Side, TimeInForce};
use revm::primitives::{Address, B256, Bytes, U256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub type RpcResult<T> = Result<T, RpcError>;

#[derive(Debug, Clone)]
pub struct RpcError {
    #[allow(dead_code)]
    pub code: i64,
    pub message: String,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Genesis-funded system address used as the caller for consensus-routed
/// CLOB operations that are not owner-scoped (cancelOrder). Must be
/// funded in the genesis config.
fn system_clob_address() -> Address {
    let mut a = [0u8; 20];
    a[19] = 0x01;
    Address::from(a)
}

fn require_transparent_mersennet_orders_enabled(engine: &Engine) -> RpcResult<()> {
    if engine.privacy_mode_activated() {
        Err(RpcError::new(
            -32605,
            "transparent MersennetOrders RPC disabled after privacy activation",
        ))
    } else {
        Ok(())
    }
}

/// Gate for the state-MUTATING `mersennet_orders_*` methods. These act for an
/// arbitrary `owner`/`caller` with no signature, so they're only allowed when
/// the operator opts in (testnet seeding). Disabled on mainnet, where orders
/// must arrive as signed txs to the CLOB precompile. Read-only queries don't
/// call this.
fn require_unsigned_orders_rpc_allowed(engine: &Engine) -> RpcResult<()> {
    if engine.allow_unsigned_orders_rpc() {
        Ok(())
    } else {
        Err(RpcError::new(
            -32604,
            "unsigned mersennet_orders_* mutations are disabled on this node; submit a signed transaction to the CLOB precompile (0x…0100)",
        ))
    }
}

fn require_transparent_account_state_enabled(engine: &Engine) -> RpcResult<()> {
    if engine.privacy_mode_activated() {
        Err(RpcError::new(
            -32605,
            "transparent account-state RPC disabled after privacy activation",
        ))
    } else {
        Ok(())
    }
}

fn require_transparent_simulation_enabled(engine: &Engine) -> RpcResult<()> {
    if engine.privacy_mode_activated() {
        Err(RpcError::new(
            -32605,
            "transparent EVM simulation RPC disabled after privacy activation",
        ))
    } else {
        Ok(())
    }
}

fn require_transparent_contract_state_enabled(engine: &Engine) -> RpcResult<()> {
    if engine.privacy_mode_activated() {
        Err(RpcError::new(
            -32605,
            "transparent contract-state RPC disabled after privacy activation",
        ))
    } else {
        Ok(())
    }
}

pub fn route(call: &str, params: Value, engine: &mut Engine) -> RpcResult<Value> {
    // Try the Phase 6 shielded-mode dispatcher first. Returns
    // `Ok(None)` if the method is not a shielded method; the main
    // match below then takes over. Returns an `Err`-shaped value when
    // the method is structurally valid but the privacy hard fork has
    // not yet activated, or when params are malformed.
    match crate::rpc_shielded::try_dispatch(call, params.clone(), engine) {
        Ok(Some(v)) => return Ok(v),
        Err(e) => return Err(RpcError::new(e.code, e.message)),
        Ok(None) => {}
    }
    match call {
        "mersennetId" | "eth_chainId" => Ok(Value::String(hex_u64(engine.chain_id))),
        "mersennet_blockNumber" | "eth_blockNumber" => {
            Ok(Value::String(hex_u64(engine.latest_height())))
        }
        "mersennet_getBalance" | "eth_getBalance" => {
            require_transparent_account_state_enabled(engine)?;
            let (address, _) = parse_balance_params(params)?;
            let balance = engine
                .get_balance(address)
                .map_err(|err| RpcError::new(-32000, err.to_string()))?;
            Ok(Value::String(hex_u256(balance)))
        }
        "mersennet_getDomainEvents" => {
            let filter = parse_domain_event_filter(params, engine)?;
            let records = engine.domain_events_in_range(
                filter.from_block,
                filter.to_block,
                filter.domain.as_deref(),
                filter.kind.as_deref(),
            );
            let privacy_active = engine.privacy_mode_activated();
            let events: Vec<Value> = records
                .into_iter()
                .filter(|record| !hide_post_privacy_sensitive_domain_event(record, engine))
                .map(|record| domain_event_record_to_value(record, privacy_active))
                .collect();
            Ok(Value::Array(events))
        }
        "mersennet_orders_addMarket" => {
            // Disabled: this mutated only the receiving node's state (markets
            // are not consensus objects via RPC), so a market added here
            // existed on one node and nowhere else — every order for it then
            // reverted on the rest of the network. Markets are seeded
            // deterministically from genesis config on every node instead.
            Err(RpcError::new(
                -32601,
                "mersennet_orders_addMarket is disabled: markets are seeded from genesis config \
                 (adding one via RPC would only mutate this node and fork CLOB state)"
                    .to_string(),
            ))
        }
        "mersennet_orders_submitOrder" => {
            require_transparent_mersennet_orders_enabled(engine)?;
            require_unsigned_orders_rpc_allowed(engine)?;
            let input = parse_mersennet_order_input(params)?;
            let owner = parse_address(&input.owner)?;
            let side = parse_side(&input.side)?;
            let price = parse_hex_u256(&input.price)?;
            let size = parse_hex_u256(&input.size)?;
            let tif = parse_tif(input.tif.as_deref())?;
            // Route through consensus: build an unsigned placeOrder
            // precompile tx and submit it to the mempool. It gossips to
            // the leader and executes deterministically on every node,
            // so the order book is consensus-consistent (the old direct
            // state mutation only touched the receiving node).
            let is_buy = matches!(side, Side::Buy);
            let tif_byte = match tif {
                TimeInForce::Gtc => 0u8,
                TimeInForce::Ioc => 1u8,
                TimeInForce::Fok => 2u8,
            };
            let calldata = mersennet::precompile_abi::encode_place_order(
                input.market_id,
                is_buy,
                price,
                size,
                tif_byte,
            );
            let tx_hash = engine
                .submit_orders_call(owner, calldata, mersennet::precompile_abi::GAS_PLACE_ORDER)
                .map_err(|e| RpcError::new(-32005, format!("order rejected: {}", e.code())))?;
            Ok(json!({ "accepted": true, "txHash": hex_b256(tx_hash) }))
        }
        "mersennet_orders_cancelOrder" => {
            require_transparent_mersennet_orders_enabled(engine)?;
            require_unsigned_orders_rpc_allowed(engine)?;
            let order_id = parse_order_id(params)?;
            // cancelOrder(uint256) is not owner-scoped in the precompile,
            // so route it as a tx from the genesis-funded system CLOB
            // address (0x…01). It gossips to the leader and cancels in a
            // block deterministically, keeping the book consensus-
            // consistent.
            let calldata = mersennet::precompile_abi::encode_cancel_order(order_id);
            let tx_hash = engine
                .submit_orders_call(
                    system_clob_address(),
                    calldata,
                    mersennet::precompile_abi::GAS_CANCEL_ORDER,
                )
                .map_err(|e| RpcError::new(-32005, format!("cancel rejected: {}", e.code())))?;
            Ok(json!({ "accepted": true, "txHash": hex_b256(tx_hash) }))
        }
        "mersennet_orders_getOrderBook" => {
            require_transparent_mersennet_orders_enabled(engine)?;
            let market_id = parse_market_id(params)?;
            let book = engine
                .mersennet_orders_order_book(mersennet::mersennet_orders::MarketId(market_id));
            match book {
                Some(book) => Ok(serde_json::to_value(order_book_to_dto(book))
                    .map_err(|err| RpcError::new(-32000, err.to_string()))?),
                None => Ok(Value::Null),
            }
        }
        "mersennet_orders_getOpenOrders" => {
            require_transparent_mersennet_orders_enabled(engine)?;
            let owner = parse_owner_param(params)?;
            let orders = engine.mersennet_orders_open_orders(owner);
            let dtos: Vec<MersennetOrderDto> = orders.into_iter().map(order_to_dto).collect();
            Ok(serde_json::to_value(dtos).map_err(|err| RpcError::new(-32000, err.to_string()))?)
        }
        "mersennet_orders_setMarginParams" => {
            // Disabled: direct local mutation — would fork CLOB margin state
            // across nodes (same class of bug as addMarket).
            Err(RpcError::new(
                -32601,
                "mersennet_orders_setMarginParams is disabled: margin params are consensus state \
                 and cannot be mutated via RPC on a single node"
                    .to_string(),
            ))
        }
        "mersennet_orders_depositCollateral" => {
            require_transparent_mersennet_orders_enabled(engine)?;
            require_unsigned_orders_rpc_allowed(engine)?;
            let (owner, amount) = parse_collateral_input(params)?;
            // Route through consensus as a precompile depositCollateral
            // tx from the owner (debits their native MRSN into escrow —
            // 1:1 backed). Deterministic across nodes.
            let calldata = mersennet::precompile_abi::encode_deposit_collateral(amount);
            let tx_hash = engine
                .submit_orders_call(
                    owner,
                    calldata,
                    mersennet::precompile_abi::GAS_DEPOSIT_COLLATERAL,
                )
                .map_err(|e| RpcError::new(-32005, format!("deposit rejected: {}", e.code())))?;
            Ok(json!({ "accepted": true, "txHash": hex_b256(tx_hash) }))
        }
        "mersennet_orders_isLiquidatable" => {
            require_transparent_mersennet_orders_enabled(engine)?;
            let owner = parse_owner_param(params)?;
            Ok(Value::Bool(engine.mersennet_orders_is_liquidatable(owner)))
        }
        "mersennet_orders_liquidate" => {
            // Disabled: direct local mutation — liquidation must happen
            // deterministically in consensus, not on one node via RPC.
            Err(RpcError::new(
                -32601,
                "mersennet_orders_liquidate is disabled: liquidations are consensus state \
                 transitions and cannot be triggered via RPC on a single node"
                    .to_string(),
            ))
        }
        "mersennet_bridge_enqueueOrdersToEvm" => {
            let payload = parse_payload(params)?;
            let msg = engine.bridge_enqueue_orders_to_evm(payload);
            Ok(serde_json::to_value(bridge_message_to_dto(msg))
                .map_err(|err| RpcError::new(-32000, err.to_string()))?)
        }
        "mersennet_bridge_enqueueEvmToOrders" => {
            let payload = parse_payload(params)?;
            let msg = engine.bridge_enqueue_evm_to_orders(payload);
            Ok(serde_json::to_value(bridge_message_to_dto(msg))
                .map_err(|err| RpcError::new(-32000, err.to_string()))?)
        }
        "mersennet_bridge_dequeueOrdersToEvm" => {
            let msg = engine.bridge_dequeue_orders_to_evm();
            match msg {
                Some(msg) => Ok(serde_json::to_value(bridge_message_to_dto(msg))
                    .map_err(|err| RpcError::new(-32000, err.to_string()))?),
                None => Ok(Value::Null),
            }
        }
        "mersennet_bridge_dequeueEvmToOrders" => {
            let msg = engine.bridge_dequeue_evm_to_orders();
            match msg {
                Some(msg) => Ok(serde_json::to_value(bridge_message_to_dto(msg))
                    .map_err(|err| RpcError::new(-32000, err.to_string()))?),
                None => Ok(Value::Null),
            }
        }
        "mersennet_gasPrice" | "eth_gasPrice" => Ok(Value::String(hex_u256(engine.base_fee))),
        "mersennet_validators" => {
            let validators: Vec<Value> = engine
                .consensus
                .validators()
                .iter()
                .map(|v| {
                    json!({
                        "address": hex_address(v.address),
                        "stake": hex_u256(v.stake),
                    })
                })
                .collect();
            Ok(Value::Array(validators))
        }
        "mersennet_staking_getValidators" => {
            let staking = &engine.orders.state.staking;
            let validators: Vec<Value> = engine
                .consensus
                .validators()
                .iter()
                .map(|v| {
                    let pool = staking.pools.get(&v.address);
                    json!({
                        "address": hex_address(v.address),
                        "selfStake": hex_u256(v.stake),
                        "delegatedTotal": hex_u256(
                            pool.map(|p| p.delegated_total).unwrap_or_default()
                        ),
                        "commissionBps": pool.map(|p| p.commission_bps).unwrap_or_default(),
                    })
                })
                .collect();
            Ok(Value::Array(validators))
        }
        "mersennet_staking_getDelegation" => {
            let (delegator, validator) = parse_two_addresses(params)?;
            let staking = &engine.orders.state.staking;
            let delegation = staking.delegations.get(&(delegator, validator));
            Ok(json!({
                "amount": hex_u256(delegation.map(|d| d.amount).unwrap_or_default()),
                "pendingRewards": hex_u256(staking.pending_rewards(delegator, validator)),
            }))
        }
        "mersennet_staking_getUnbonding" => {
            let (delegator, _) = parse_balance_params(params)?;
            let staking = &engine.orders.state.staking;
            let entries: Vec<Value> = staking
                .unbondings
                .get(&delegator)
                .map(|list| {
                    list.iter()
                        .map(|e| {
                            json!({
                                "validator": hex_address(e.validator),
                                "amount": hex_u256(e.amount),
                                "unlockAtBlock": hex_u64(e.unlock_at),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(Value::Array(entries))
        }
        "mersennet_orders_getCollateralAssets" => {
            let mut assets: Vec<(&Address, &mersennet::mersennet_orders::CollateralAsset)> =
                engine.orders.state.collateral_assets.iter().collect();
            assets.sort_by_key(|(addr, _)| **addr);
            let assets: Vec<Value> = assets
                .into_iter()
                .map(|(token, asset)| {
                    json!({
                        "token": hex_address(*token),
                        "weightBps": asset.weight_bps,
                        "valueNum": hex_u256(asset.value_num),
                        "valueDen": hex_u256(asset.value_den),
                        "balancesSlot": hex_u256(asset.balances_slot),
                    })
                })
                .collect();
            Ok(Value::Array(assets))
        }
        "mersennet_orders_getTokenCollateral" => {
            require_transparent_mersennet_orders_enabled(engine)?;
            let (owner, token) = parse_two_addresses(params)?;
            let balance = engine
                .orders
                .state
                .accounts
                .get(&owner)
                .and_then(|a| a.token_collateral.get(&token).copied())
                .unwrap_or_default();
            Ok(Value::String(hex_u256(balance)))
        }
        "mersennet_getCodeAttestation" => {
            let (address, _) = parse_balance_params(params)?;
            match engine.published_code_attestation(address) {
                Some(attestation) => Ok(json!({
                    "contract": hex_address(address),
                    "deployer": hex_address(attestation.deployer),
                    "codeHash": hex_b256(attestation.code_hash),
                    "metadataUri": attestation.metadata_uri,
                    "publishedAtBlock": hex_u64(attestation.published_at_block),
                })),
                None => Ok(Value::Null),
            }
        }
        "mersennet_getCodeHash" => {
            let (address, _) = parse_balance_params(params)?;
            match engine.published_code_attestation(address) {
                Some(attestation) => Ok(Value::String(hex_b256(attestation.code_hash))),
                None => Ok(Value::Null),
            }
        }
        "mersennet_getCode" | "eth_getCode" => {
            require_transparent_contract_state_enabled(engine)?;
            let (address, _) = parse_balance_params(params)?;
            let code = engine
                .get_code(address)
                .map_err(|err| RpcError::new(-32000, err.to_string()))?;
            Ok(Value::String(format!("0x{}", hex::encode(&code))))
        }
        "mersennet_getStorageAt" | "eth_getStorageAt" => {
            require_transparent_contract_state_enabled(engine)?;
            let array = match params {
                Value::Array(values) => values,
                _ => return Err(RpcError::new(-32602, "invalid params")),
            };
            let address = match array.first() {
                Some(Value::String(value)) => parse_address(value)?,
                _ => return Err(RpcError::new(-32602, "address required")),
            };
            let slot = match array.get(1) {
                Some(Value::String(value)) => parse_hex_u256(value)?,
                _ => return Err(RpcError::new(-32602, "slot required")),
            };
            let value = engine
                .get_storage_at(address, slot)
                .map_err(|err| RpcError::new(-32000, err.to_string()))?;
            Ok(Value::String(hex_u256(value)))
        }
        "mersennet_getTransactionCount" | "eth_getTransactionCount" => {
            require_transparent_account_state_enabled(engine)?;
            let (address, _) = parse_balance_params(params)?;
            let nonce = engine
                .get_account_nonce(address)
                .map_err(|err| RpcError::new(-32000, err.to_string()))?;
            Ok(Value::String(hex_u64(nonce)))
        }
        "mersennet_call" | "eth_call" => {
            require_transparent_simulation_enabled(engine)?;
            let input = parse_call_input(params)?;
            let to = input
                .to
                .ok_or_else(|| RpcError::new(-32602, "to address required for eth_call"))?;
            let output = engine
                .call_contract(input.from, to, input.data, input.gas_limit, input.value)
                .map_err(|err| RpcError::new(-32000, err.to_string()))?;
            Ok(Value::String(format!("0x{}", hex::encode(&output))))
        }
        "eth_estimateGas" => {
            require_transparent_simulation_enabled(engine)?;
            let input = parse_call_input(params)?;
            if input.data.is_empty() && input.to.is_some() {
                Ok(Value::String(hex_u64(21000)))
            } else {
                let exec = engine
                    .simulate_call(
                        input.from,
                        input.to,
                        input.data,
                        input.gas_limit,
                        input.value,
                    )
                    .map_err(|err| RpcError::new(-32000, err.to_string()))?;
                Ok(Value::String(hex_u64(exec.gas_used)))
            }
        }
        "net_version" => Ok(Value::String(engine.chain_id.to_string())),
        "net_peerCount" => {
            let count = engine.peer_count.load(std::sync::atomic::Ordering::Relaxed);
            Ok(Value::String(format!("0x{:x}", count)))
        }
        "net_listening" => Ok(Value::Bool(true)),
        "web3_clientVersion" => Ok(Value::String("Mersennet/0.1.0".to_string())),
        "txpool_status" => {
            let pending = engine.mempool_pending_count();
            let queued = engine.mempool_queued_count();
            Ok(json!({
                "pending": format!("0x{:x}", pending),
                "queued": format!("0x{:x}", queued)
            }))
        }
        _ => Err(RpcError::new(-32601, format!("method not found: {call}"))),
    }
}

#[derive(Debug, Deserialize)]
struct MersennetOrderInput {
    owner: String,
    market_id: u64,
    side: String,
    price: String,
    size: String,
    tif: Option<String>,
}

// Scaffolding for the order-mutation / admin RPCs (submitOrder result shape,
// addMarket, setMarginParams) that are hard-disabled on the public router and
// return -32601. Kept intact so they can be wired on a permissioned deployment
// without rebuilding the DTOs; `#[allow(dead_code)]` keeps `-D warnings` green.
#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct MersennetOrderResultDto {
    order_id: Option<String>,
    filled: String,
    remaining: String,
    trades: Vec<TradeDto>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct TradeDto {
    taker: String,
    maker: String,
    market_id: String,
    side: String,
    price: String,
    size: String,
}

#[derive(Debug, Serialize)]
struct OrderBookDto {
    bids: Vec<OrderBookLevelDto>,
    asks: Vec<OrderBookLevelDto>,
}

#[derive(Debug, Serialize)]
struct OrderBookLevelDto {
    price: String,
    size: String,
}

#[derive(Debug, Serialize)]
struct MersennetOrderDto {
    id: String,
    owner: String,
    market_id: String,
    side: String,
    price: String,
    size: String,
    tif: String,
}

#[derive(Debug, Serialize)]
struct BridgeMessageDto {
    nonce: String,
    from: String,
    to: String,
    payload: String,
}

#[derive(Debug, Serialize)]
struct DomainEventDto {
    block_number: String,
    event_index: String,
    domain: String,
    kind: String,
    data: Value,
}

#[derive(Debug, Deserialize)]
struct DomainEventFilterInput {
    #[serde(rename = "fromBlock")]
    from_block: Option<Value>,
    #[serde(rename = "toBlock")]
    to_block: Option<Value>,
    domain: Option<String>,
    kind: Option<String>,
}

struct DomainEventFilter {
    from_block: u64,
    to_block: u64,
    domain: Option<String>,
    kind: Option<String>,
}

fn parse_owner_param(params: Value) -> RpcResult<Address> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    match array.first() {
        Some(Value::String(value)) => parse_address(value),
        _ => Err(RpcError::new(-32602, "owner required")),
    }
}

fn parse_balance_params(params: Value) -> RpcResult<(Address, Value)> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };

    let address = match array.first() {
        Some(Value::String(value)) => parse_address(value)?,
        _ => return Err(RpcError::new(-32602, "address required")),
    };

    let block = array.get(1).cloned().unwrap_or(Value::Null);
    Ok((address, block))
}

// Parser for the disabled addMarket RPC — see the DTO note above.
#[allow(dead_code)]
fn parse_market_input(params: Value) -> RpcResult<(String, U256, U256)> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let symbol = match array.first() {
        Some(Value::String(value)) => value.clone(),
        _ => return Err(RpcError::new(-32602, "symbol required")),
    };
    let tick_size = match array.get(1) {
        Some(Value::String(value)) => parse_hex_u256(value)?,
        _ => return Err(RpcError::new(-32602, "tick_size required")),
    };
    let lot_size = match array.get(2) {
        Some(Value::String(value)) => parse_hex_u256(value)?,
        _ => return Err(RpcError::new(-32602, "lot_size required")),
    };
    Ok((symbol, tick_size, lot_size))
}

fn parse_mersennet_order_input(params: Value) -> RpcResult<MersennetOrderInput> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let obj = array
        .first()
        .ok_or_else(|| RpcError::new(-32602, "order object required"))?
        .clone();
    serde_json::from_value(obj).map_err(|err| RpcError::new(-32602, err.to_string()))
}

fn parse_two_addresses(params: Value) -> RpcResult<(Address, Address)> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let first = match array.first() {
        Some(Value::String(value)) => parse_address(value)?,
        _ => return Err(RpcError::new(-32602, "first address required")),
    };
    let second = match array.get(1) {
        Some(Value::String(value)) => parse_address(value)?,
        _ => return Err(RpcError::new(-32602, "second address required")),
    };
    Ok((first, second))
}

fn parse_order_id(params: Value) -> RpcResult<u64> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    match array.first() {
        Some(Value::String(value)) => parse_hex_u64(value),
        Some(Value::Number(value)) => value
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "invalid order id")),
        _ => Err(RpcError::new(-32602, "order id required")),
    }
}

fn parse_market_id(params: Value) -> RpcResult<u64> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    match array.first() {
        Some(Value::String(value)) => parse_hex_u64(value),
        Some(Value::Number(value)) => value
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "invalid market id")),
        _ => Err(RpcError::new(-32602, "market id required")),
    }
}

// Parser for the disabled setMarginParams RPC — see the DTO note above.
#[allow(dead_code)]
fn parse_margin_params(params: Value) -> RpcResult<(u64, u64)> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let initial = match array.first() {
        Some(Value::Number(value)) => value
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "invalid initial_bps"))?,
        Some(Value::String(value)) => parse_hex_u64(value)?,
        _ => return Err(RpcError::new(-32602, "initial_bps required")),
    };
    let maintenance = match array.get(1) {
        Some(Value::Number(value)) => value
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "invalid maintenance_bps"))?,
        Some(Value::String(value)) => parse_hex_u64(value)?,
        _ => return Err(RpcError::new(-32602, "maintenance_bps required")),
    };
    Ok((initial, maintenance))
}

fn parse_collateral_input(params: Value) -> RpcResult<(Address, U256)> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let owner = match array.first() {
        Some(Value::String(value)) => parse_address(value)?,
        _ => return Err(RpcError::new(-32602, "owner required")),
    };
    let amount = match array.get(1) {
        Some(Value::String(value)) => parse_hex_u256(value)?,
        Some(Value::Number(value)) => U256::from(
            value
                .as_u64()
                .ok_or_else(|| RpcError::new(-32602, "invalid amount"))?,
        ),
        _ => return Err(RpcError::new(-32602, "amount required")),
    };
    Ok((owner, amount))
}

fn parse_payload(params: Value) -> RpcResult<Bytes> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    match array.first() {
        Some(Value::String(value)) => parse_hex_bytes(value),
        _ => Err(RpcError::new(-32602, "payload required")),
    }
}

fn parse_domain_event_filter(params: Value, engine: &Engine) -> RpcResult<DomainEventFilter> {
    let latest = engine.latest_height();
    let raw = match params {
        Value::Array(values) => values.first().cloned().unwrap_or(Value::Null),
        Value::Null => Value::Null,
        value => value,
    };
    let input: DomainEventFilterInput = if raw.is_null() {
        DomainEventFilterInput {
            from_block: None,
            to_block: None,
            domain: None,
            kind: None,
        }
    } else {
        serde_json::from_value(raw).map_err(|err| RpcError::new(-32602, err.to_string()))?
    };

    let from_block = match input.from_block {
        Some(value) => parse_block_bound(&value, latest, 0)?,
        None => 0,
    };
    let to_block = match input.to_block {
        Some(value) => parse_block_bound(&value, latest, latest)?,
        None => latest,
    };

    Ok(DomainEventFilter {
        from_block,
        to_block,
        domain: input.domain,
        kind: input.kind,
    })
}

fn parse_block_bound(value: &Value, latest: u64, fallback: u64) -> RpcResult<u64> {
    match value {
        Value::Null => Ok(fallback),
        Value::String(tag) if tag == "latest" => Ok(latest),
        Value::String(tag) => parse_hex_u64(tag),
        Value::Number(num) => num
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "invalid block number")),
        _ => Err(RpcError::new(-32602, "invalid block number")),
    }
}

fn parse_side(value: &str) -> RpcResult<Side> {
    match value.to_lowercase().as_str() {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        _ => Err(RpcError::new(-32602, "invalid side")),
    }
}

fn parse_tif(value: Option<&str>) -> RpcResult<TimeInForce> {
    match value.map(|v| v.to_lowercase()) {
        None => Ok(TimeInForce::Gtc),
        Some(v) if v == "gtc" => Ok(TimeInForce::Gtc),
        Some(v) if v == "ioc" => Ok(TimeInForce::Ioc),
        Some(v) if v == "fok" => Ok(TimeInForce::Fok),
        Some(_) => Err(RpcError::new(-32602, "invalid tif")),
    }
}

fn parse_hex_u64(input: &str) -> RpcResult<u64> {
    let stripped = input.strip_prefix("0x").unwrap_or(input);
    u64::from_str_radix(stripped, 16).map_err(|err| RpcError::new(-32602, err.to_string()))
}

fn parse_hex_u256(input: &str) -> RpcResult<U256> {
    let stripped = input.strip_prefix("0x").unwrap_or(input);
    U256::from_str_radix(stripped, 16).map_err(|err| RpcError::new(-32602, err.to_string()))
}

fn parse_address(value: &str) -> RpcResult<Address> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(stripped).map_err(|err| RpcError::new(-32602, err.to_string()))?;
    if bytes.len() != 20 {
        return Err(RpcError::new(-32602, "invalid address length"));
    }
    Ok(Address::from_slice(&bytes))
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

#[allow(dead_code)]
fn order_outcome_to_dto(outcome: OrderOutcome) -> MersennetOrderResultDto {
    MersennetOrderResultDto {
        order_id: outcome.order_id.map(|id| hex_u64(id.0)),
        filled: hex_u256(outcome.filled),
        remaining: hex_u256(outcome.remaining),
        trades: outcome.trades.into_iter().map(trade_to_dto).collect(),
    }
}

#[allow(dead_code)]
fn trade_to_dto(trade: mersennet::mersennet_orders::Trade) -> TradeDto {
    TradeDto {
        taker: hex_address(trade.taker),
        maker: hex_address(trade.maker),
        market_id: hex_u64(trade.market.0),
        side: match trade.side {
            Side::Buy => "buy".to_string(),
            Side::Sell => "sell".to_string(),
        },
        price: hex_u256(trade.price),
        size: hex_u256(trade.size),
    }
}

fn order_book_to_dto(book: OrderBookView) -> OrderBookDto {
    OrderBookDto {
        bids: book
            .bids
            .into_iter()
            .map(|level| OrderBookLevelDto {
                price: hex_u256(level.price),
                size: hex_u256(level.size),
            })
            .collect(),
        asks: book
            .asks
            .into_iter()
            .map(|level| OrderBookLevelDto {
                price: hex_u256(level.price),
                size: hex_u256(level.size),
            })
            .collect(),
    }
}

fn order_to_dto(order: Order) -> MersennetOrderDto {
    MersennetOrderDto {
        id: hex_u64(order.id.0),
        owner: hex_address(order.owner),
        market_id: hex_u64(order.market.0),
        side: match order.side {
            Side::Buy => "buy".to_string(),
            Side::Sell => "sell".to_string(),
        },
        price: hex_u256(order.price),
        size: hex_u256(order.size),
        tif: match order.tif {
            TimeInForce::Gtc => "gtc".to_string(),
            TimeInForce::Ioc => "ioc".to_string(),
            TimeInForce::Fok => "fok".to_string(),
        },
    }
}

fn domain_event_record_to_value(record: DomainEventRecord, privacy_active: bool) -> Value {
    let (domain, kind, data) = domain_event_parts(&record.event, privacy_active);
    let dto = DomainEventDto {
        block_number: hex_u64(record.block_number),
        event_index: hex_u64(record.event_index),
        domain: domain.to_string(),
        kind: kind.to_string(),
        data,
    };
    serde_json::to_value(dto).unwrap_or(Value::Null)
}

fn hide_post_privacy_sensitive_domain_event(record: &DomainEventRecord, engine: &Engine) -> bool {
    if !engine.privacy_mode_activated() {
        return false;
    }

    if record.event.is_privacy_safe_after_activation() {
        return false;
    }

    let activation_height = engine.privacy_activation_height.unwrap_or(0);
    record.block_number >= activation_height
}

fn domain_event_parts(
    event: &DomainEvent,
    privacy_active: bool,
) -> (&'static str, &'static str, Value) {
    match event {
        DomainEvent::MersennetOrders(event) => (
            "mersennet_orders",
            event.kind(),
            mersennet_orders_event_to_value(event, privacy_active),
        ),
        DomainEvent::Bridge(event) => ("bridge", event.kind(), bridge_event_to_value(event)),
        DomainEvent::Shielded(event) => ("shielded", event.kind(), shielded_event_to_value(event)),
    }
}

fn shielded_event_to_value(event: &mersennet::events::ShieldedEvent) -> Value {
    use mersennet::events::ShieldedEvent;
    match event {
        ShieldedEvent::FbaCleared {
            market_id,
            clearing_price,
            matched_size,
            intent_count,
        } => json!({
            "market_id": hex_u64(market_id.0),
            "clearing_price": hex_u256(*clearing_price),
            "matched_size": hex_u256(*matched_size),
            "intent_count": hex_u64(*intent_count),
        }),
        ShieldedEvent::MempoolBatchAdmitted {
            block_number,
            intent_count,
        } => json!({
            "block_number": hex_u64(*block_number),
            "intent_count": hex_u64(*intent_count),
        }),
        ShieldedEvent::LiquidationSettled {
            market_id,
            winner_bond_commitment,
            winning_bid,
        } => json!({
            "market_id": hex_u64(market_id.0),
            "winner_bond_commitment": format!("0x{}", hex::encode(winner_bond_commitment)),
            "winning_bid": hex_u256(*winning_bid),
        }),
        ShieldedEvent::ShieldedRootAdvanced {
            block_number,
            new_root,
            notes_added,
            nullifiers_added,
        } => json!({
            "block_number": hex_u64(*block_number),
            "new_root": format!("0x{}", hex::encode(new_root)),
            "notes_added": hex_u64(*notes_added),
            "nullifiers_added": hex_u64(*nullifiers_added),
        }),
    }
}

fn mersennet_orders_event_to_value(event: &MersennetOrdersEvent, privacy_active: bool) -> Value {
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
                Side::Buy => "buy",
                Side::Sell => "sell",
            },
            "price": hex_u256(*price),
            "size": hex_u256(*size),
            "tif": match tif {
                TimeInForce::Gtc => "gtc",
                TimeInForce::Ioc => "ioc",
                TimeInForce::Fok => "fok",
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
                Side::Buy => "buy",
                Side::Sell => "sell",
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

fn bridge_event_to_value(event: &BridgeEvent) -> Value {
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

// Error mapper for the disabled order-mutation RPCs — see the DTO note above.
#[allow(dead_code)]
fn map_mersennet_orders_error(err: MersennetOrdersError) -> RpcError {
    let code = match err {
        MersennetOrdersError::UnknownMarket => -32010,
        MersennetOrdersError::InvalidSize => -32011,
        MersennetOrdersError::FokNotFillable => -32012,
        MersennetOrdersError::InsufficientCollateral => -32013,
        MersennetOrdersError::InsufficientEquity => -32014,
        MersennetOrdersError::MarketHalted => -32015,
        MersennetOrdersError::WithdrawalExceedsEquity => -32016,
        MersennetOrdersError::NotOrderOwner => -32017,
        MersennetOrdersError::PostOnlyWouldCross => -32018,
        MersennetOrdersError::InvalidOrderFlags => -32019,
        MersennetOrdersError::DuplicateMarket => -32020,
        MersennetOrdersError::InvalidMarketParams => -32021,
        MersennetOrdersError::UnknownCollateralAsset => -32022,
    };
    RpcError::new(code, err.message())
}

fn bridge_message_to_dto(msg: BridgeMessage) -> BridgeMessageDto {
    BridgeMessageDto {
        nonce: hex_u64(msg.nonce),
        from: bridge_domain_to_str(&msg.from).to_string(),
        to: bridge_domain_to_str(&msg.to).to_string(),
        payload: format!("0x{}", hex::encode(msg.payload.as_ref())),
    }
}

fn bridge_domain_to_str(domain: &BridgeDomain) -> &'static str {
    match domain {
        BridgeDomain::MersennetOrders => "mersennet_orders",
        BridgeDomain::MersennetEvm => "mersennet_evm",
    }
}

struct CallInput {
    from: Address,
    to: Option<Address>,
    data: Bytes,
    gas_limit: u64,
    value: U256,
}

fn parse_call_input(params: Value) -> RpcResult<CallInput> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let obj = array
        .first()
        .ok_or_else(|| RpcError::new(-32602, "call object required"))?
        .clone();

    #[derive(Deserialize)]
    struct RawCallInput {
        from: Option<String>,
        to: Option<String>,
        data: Option<String>,
        input: Option<String>,
        value: Option<String>,
        gas: Option<String>,
    }

    let raw: RawCallInput =
        serde_json::from_value(obj).map_err(|err| RpcError::new(-32602, err.to_string()))?;

    let from = match raw.from {
        Some(addr) => parse_address(&addr)?,
        None => Address::ZERO,
    };
    let to = match raw.to {
        Some(addr) => Some(parse_address(&addr)?),
        None => None,
    };
    let data_str = raw.data.or(raw.input);
    let data = match data_str {
        Some(d) => parse_hex_bytes(&d)?,
        None => Bytes::new(),
    };
    let value = match raw.value {
        Some(v) => parse_hex_u256(&v)?,
        None => U256::ZERO,
    };
    let gas_limit = match raw.gas {
        Some(g) => parse_hex_u64(&g)?,
        None => 30_000_000,
    };

    Ok(CallInput {
        from,
        to,
        data,
        gas_limit,
        value,
    })
}

fn parse_hex_bytes(input: &str) -> RpcResult<Bytes> {
    let stripped = input.strip_prefix("0x").unwrap_or(input);
    let bytes = hex::decode(stripped).map_err(|err| RpcError::new(-32602, err.to_string()))?;
    Ok(Bytes::from(bytes))
}
