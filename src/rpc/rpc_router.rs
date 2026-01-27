use crate::bridge::{BridgeDomain, BridgeMessage};
use crate::engine::Engine;
use crate::events::{BridgeEvent, BridgeQueueKind, DomainEvent, DomainEventRecord, PrimeOrdersEvent};
use crate::errors::PrimeOrdersError;
use crate::prime_orders::{Order, OrderBookView, OrderOutcome, Side, TimeInForce};
use revm::primitives::{Address, Bytes, U256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub type RpcResult<T> = Result<T, RpcError>;

#[derive(Debug, Clone)]
pub struct RpcError {
    #[allow(dead_code)]
    pub code: i64,
    pub message: String,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

pub fn route(call: &str, params: Value, engine: &mut Engine) -> RpcResult<Value> {
    match call {
        "prime_chainId" => Ok(Value::String(hex_u64(engine.chain_id))),
        "prime_blockNumber" => Ok(Value::String(hex_u64(engine.latest_height()))),
        "prime_getBalance" => {
            let (address, _) = parse_balance_params(params)?;
            let balance = engine
                .get_balance(address)
                .map_err(|err| RpcError::new(-32000, err.to_string()))?;
            Ok(Value::String(hex_u256(balance)))
        }
        "prime_getDomainEvents" => {
            let filter = parse_domain_event_filter(params, engine)?;
            let records = engine.domain_events_in_range(
                filter.from_block,
                filter.to_block,
                filter.domain.as_deref(),
                filter.kind.as_deref(),
            );
            let events: Vec<Value> = records
                .into_iter()
                .map(domain_event_record_to_value)
                .collect();
            Ok(Value::Array(events))
        }
        "primeorders_addMarket" => {
            let (symbol, tick_size, lot_size) = parse_market_input(params)?;
            let market_id = engine.prime_orders_add_market(symbol, tick_size, lot_size);
            Ok(Value::String(hex_u64(market_id.0)))
        }
        "primeorders_submitOrder" => {
            let input = parse_prime_order_input(params)?;
            let owner = parse_address(&input.owner)?;
            let side = parse_side(&input.side)?;
            let price = parse_hex_u256(&input.price)?;
            let size = parse_hex_u256(&input.size)?;
            let tif = parse_tif(input.tif.as_deref())?;
            let outcome = engine
                .prime_orders_submit_order(
                    owner,
                    crate::prime_orders::MarketId(input.market_id),
                    side,
                    price,
                    size,
                    tif,
                )
                .map_err(map_prime_orders_error)?;
            Ok(serde_json::to_value(order_outcome_to_dto(outcome))
                .map_err(|err| RpcError::new(-32000, err.to_string()))?)
        }
        "primeorders_cancelOrder" => {
            let order_id = parse_order_id(params)?;
            let cancelled = engine
                .prime_orders_cancel_order(crate::prime_orders::OrderId(order_id))
                .is_some();
            Ok(Value::Bool(cancelled))
        }
        "primeorders_getOrderBook" => {
            let market_id = parse_market_id(params)?;
            let book = engine.prime_orders_order_book(crate::prime_orders::MarketId(market_id));
            match book {
                Some(book) => Ok(serde_json::to_value(order_book_to_dto(book))
                    .map_err(|err| RpcError::new(-32000, err.to_string()))?),
                None => Ok(Value::Null),
            }
        }
        "primeorders_getOpenOrders" => {
            let owner = parse_owner_param(params)?;
            let orders = engine.prime_orders_open_orders(owner);
            let dtos: Vec<PrimeOrderDto> = orders.into_iter().map(order_to_dto).collect();
            Ok(serde_json::to_value(dtos).map_err(|err| RpcError::new(-32000, err.to_string()))?)
        }
        "primeorders_setMarginParams" => {
            let (initial_bps, maintenance_bps) = parse_margin_params(params)?;
            engine.prime_orders_set_margin_params(initial_bps, maintenance_bps);
            Ok(Value::Bool(true))
        }
        "primeorders_depositCollateral" => {
            let (owner, amount) = parse_collateral_input(params)?;
            engine.prime_orders_deposit_collateral(owner, amount);
            Ok(Value::Bool(true))
        }
        "primeorders_isLiquidatable" => {
            let owner = parse_owner_param(params)?;
            Ok(Value::Bool(engine.prime_orders_is_liquidatable(owner)))
        }
        "primeorders_liquidate" => {
            let owner = parse_owner_param(params)?;
            Ok(Value::Bool(engine.prime_orders_liquidate(owner)))
        }
        "primebridge_enqueueOrdersToEvm" => {
            let payload = parse_payload(params)?;
            let msg = engine.bridge_enqueue_orders_to_evm(payload);
            Ok(serde_json::to_value(bridge_message_to_dto(msg))
                .map_err(|err| RpcError::new(-32000, err.to_string()))?)
        }
        "primebridge_enqueueEvmToOrders" => {
            let payload = parse_payload(params)?;
            let msg = engine.bridge_enqueue_evm_to_orders(payload);
            Ok(serde_json::to_value(bridge_message_to_dto(msg))
                .map_err(|err| RpcError::new(-32000, err.to_string()))?)
        }
        "primebridge_dequeueOrdersToEvm" => {
            let msg = engine.bridge_dequeue_orders_to_evm();
            match msg {
                Some(msg) => Ok(serde_json::to_value(bridge_message_to_dto(msg))
                    .map_err(|err| RpcError::new(-32000, err.to_string()))?),
                None => Ok(Value::Null),
            }
        }
        "primebridge_dequeueEvmToOrders" => {
            let msg = engine.bridge_dequeue_evm_to_orders();
            match msg {
                Some(msg) => Ok(serde_json::to_value(bridge_message_to_dto(msg))
                    .map_err(|err| RpcError::new(-32000, err.to_string()))?),
                None => Ok(Value::Null),
            }
        }
        _ => Err(RpcError::new(-32601, format!("method not found: {call}"))),
    }
}

#[derive(Debug, Deserialize)]
struct PrimeOrderInput {
    owner: String,
    market_id: u64,
    side: String,
    price: String,
    size: String,
    tif: Option<String>,
}

#[derive(Debug, Serialize)]
struct PrimeOrderResultDto {
    order_id: Option<String>,
    filled: String,
    remaining: String,
    trades: Vec<TradeDto>,
}

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
struct PrimeOrderDto {
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
    match array.get(0) {
        Some(Value::String(value)) => parse_address(value),
        _ => Err(RpcError::new(-32602, "owner required")),
    }
}

fn parse_balance_params(params: Value) -> RpcResult<(Address, Value)> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };

    let address = match array.get(0) {
        Some(Value::String(value)) => parse_address(value)?,
        _ => return Err(RpcError::new(-32602, "address required")),
    };

    let block = array.get(1).cloned().unwrap_or(Value::Null);
    Ok((address, block))
}

fn parse_market_input(params: Value) -> RpcResult<(String, U256, U256)> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let symbol = match array.get(0) {
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

fn parse_prime_order_input(params: Value) -> RpcResult<PrimeOrderInput> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let obj = array
        .get(0)
        .ok_or_else(|| RpcError::new(-32602, "order object required"))?
        .clone();
    serde_json::from_value(obj).map_err(|err| RpcError::new(-32602, err.to_string()))
}

fn parse_order_id(params: Value) -> RpcResult<u64> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    match array.get(0) {
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
    match array.get(0) {
        Some(Value::String(value)) => parse_hex_u64(value),
        Some(Value::Number(value)) => value
            .as_u64()
            .ok_or_else(|| RpcError::new(-32602, "invalid market id")),
        _ => Err(RpcError::new(-32602, "market id required")),
    }
}

fn parse_margin_params(params: Value) -> RpcResult<(u64, u64)> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    let initial = match array.get(0) {
        Some(Value::Number(value)) => value.as_u64().ok_or_else(|| RpcError::new(-32602, "invalid initial_bps"))?,
        Some(Value::String(value)) => parse_hex_u64(value)?,
        _ => return Err(RpcError::new(-32602, "initial_bps required")),
    };
    let maintenance = match array.get(1) {
        Some(Value::Number(value)) => value.as_u64().ok_or_else(|| RpcError::new(-32602, "invalid maintenance_bps"))?,
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
    let owner = match array.get(0) {
        Some(Value::String(value)) => parse_address(value)?,
        _ => return Err(RpcError::new(-32602, "owner required")),
    };
    let amount = match array.get(1) {
        Some(Value::String(value)) => parse_hex_u256(value)?,
        Some(Value::Number(value)) => U256::from(value.as_u64().ok_or_else(|| RpcError::new(-32602, "invalid amount"))?),
        _ => return Err(RpcError::new(-32602, "amount required")),
    };
    Ok((owner, amount))
}

fn parse_payload(params: Value) -> RpcResult<Bytes> {
    let array = match params {
        Value::Array(values) => values,
        _ => return Err(RpcError::new(-32602, "invalid params")),
    };
    match array.get(0) {
        Some(Value::String(value)) => parse_hex_bytes(value),
        _ => Err(RpcError::new(-32602, "payload required")),
    }
}

fn parse_domain_event_filter(params: Value, engine: &Engine) -> RpcResult<DomainEventFilter> {
    let latest = engine.latest_height();
    let raw = match params {
        Value::Array(values) => values.get(0).cloned().unwrap_or(Value::Null),
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
        Value::Number(num) => num.as_u64().ok_or_else(|| RpcError::new(-32602, "invalid block number")),
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

fn order_outcome_to_dto(outcome: OrderOutcome) -> PrimeOrderResultDto {
    PrimeOrderResultDto {
        order_id: outcome.order_id.map(|id| hex_u64(id.0)),
        filled: hex_u256(outcome.filled),
        remaining: hex_u256(outcome.remaining),
        trades: outcome.trades.into_iter().map(trade_to_dto).collect(),
    }
}

fn trade_to_dto(trade: crate::prime_orders::Trade) -> TradeDto {
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

fn order_to_dto(order: Order) -> PrimeOrderDto {
    PrimeOrderDto {
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

fn domain_event_record_to_value(record: DomainEventRecord) -> Value {
    let (domain, kind, data) = domain_event_parts(&record.event);
    let dto = DomainEventDto {
        block_number: hex_u64(record.block_number),
        event_index: hex_u64(record.event_index),
        domain: domain.to_string(),
        kind: kind.to_string(),
        data,
    };
    serde_json::to_value(dto).unwrap_or(Value::Null)
}

fn domain_event_parts(event: &DomainEvent) -> (&'static str, &'static str, Value) {
    match event {
        DomainEvent::PrimeOrders(event) => (
            "primeorders",
            event.kind(),
            prime_orders_event_to_value(event),
        ),
        DomainEvent::Bridge(event) => (
            "bridge",
            event.kind(),
            bridge_event_to_value(event),
        ),
    }
}

fn prime_orders_event_to_value(event: &PrimeOrdersEvent) -> Value {
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
                Side::Buy => "buy",
                Side::Sell => "sell",
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

fn map_prime_orders_error(err: PrimeOrdersError) -> RpcError {
    let code = match err {
        PrimeOrdersError::UnknownMarket => -32010,
        PrimeOrdersError::InvalidSize => -32011,
        PrimeOrdersError::FokNotFillable => -32012,
        PrimeOrdersError::InsufficientCollateral => -32013,
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
        BridgeDomain::PrimeOrders => "primeorders",
        BridgeDomain::PrimeEvm => "primeevm",
    }
}

fn parse_hex_bytes(input: &str) -> RpcResult<Bytes> {
    let stripped = input.strip_prefix("0x").unwrap_or(input);
    let bytes = hex::decode(stripped).map_err(|err| RpcError::new(-32602, err.to_string()))?;
    Ok(Bytes::from(bytes))
}
