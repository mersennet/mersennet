use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use revm::db::InMemoryDB;
use revm::handler::register::EvmHandler;
use revm::precompile::Precompile;
use revm::primitives::{
    Address, Bytes, Env, KECCAK_EMPTY, PrecompileError, PrecompileErrors, PrecompileOutput,
    PrecompileResult, U256,
};
use revm::{ContextPrecompile, ContextStatefulPrecompileMut, Database, InnerEvmContext};

use crate::code_publication::CodePublicationRegistry;
use crate::events::{DomainEvent, MersennetOrdersEvent};
use crate::mersennet_orders::{MarketId, MersennetOrdersState, OrderId, Side, TimeInForce};
use crate::precompile_abi::*;
use crate::shielded_evm::{ShieldedEnvelope, ShieldedEvm};
use crate::zk_proofs::StateTransitionProof;

// ---------------------------------------------------------------------------
// Global MersennetOrders context — set before block execution, cleared after.
// ---------------------------------------------------------------------------

static MERSENNET_ORDERS_CTX: Lazy<Mutex<Option<Arc<Mutex<MersennetOrdersState>>>>> =
    Lazy::new(|| Mutex::new(None));

static TRANSPARENT_MERSENNET_ORDERS_ENABLED: AtomicBool = AtomicBool::new(true);

// Domain-event sink for CLOB mutations executed inside the precompile.
// The precompile has no access to the engine, so fills/cancels/deposits are
// buffered here and drained into `block.domain_events` after tx execution —
// on BOTH the produce and import paths (imports re-execute the same txs, so
// followers regenerate identical events deterministically). Recording is
// gated so read-only paths (eth_call / estimateGas, which run against a
// cloned state) never leak phantom events.
static MERSENNET_ORDERS_EVENTS: Lazy<Mutex<Vec<DomainEvent>>> = Lazy::new(|| Mutex::new(Vec::new()));
static MERSENNET_ORDERS_EVENTS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_orders_event_recording(enabled: bool) {
    MERSENNET_ORDERS_EVENTS_ENABLED.store(enabled, Ordering::SeqCst);
    if !enabled {
        MERSENNET_ORDERS_EVENTS.lock().unwrap().clear();
    }
}

pub fn drain_orders_events() -> Vec<DomainEvent> {
    std::mem::take(&mut *MERSENNET_ORDERS_EVENTS.lock().unwrap())
}

fn record_orders_event(event: MersennetOrdersEvent) {
    if MERSENNET_ORDERS_EVENTS_ENABLED.load(Ordering::SeqCst) {
        MERSENNET_ORDERS_EVENTS
            .lock()
            .unwrap()
            .push(DomainEvent::MersennetOrders(event));
    }
}

pub fn set_mersennet_orders_context(state: Arc<Mutex<MersennetOrdersState>>) {
    *MERSENNET_ORDERS_CTX.lock().unwrap() = Some(state);
}

pub fn clear_mersennet_orders_context() {
    *MERSENNET_ORDERS_CTX.lock().unwrap() = None;
}

pub fn set_transparent_mersennet_orders_enabled(enabled: bool) {
    TRANSPARENT_MERSENNET_ORDERS_ENABLED.store(enabled, Ordering::SeqCst);
}

pub fn transparent_mersennet_orders_enabled() -> bool {
    TRANSPARENT_MERSENNET_ORDERS_ENABLED.load(Ordering::SeqCst)
}

fn with_orders<F, R>(f: F) -> Result<R, PrecompileErrors>
where
    F: FnOnce(&mut MersennetOrdersState) -> R,
{
    let guard = MERSENNET_ORDERS_CTX
        .lock()
        .map_err(|_| PrecompileErrors::Fatal {
            msg: "mersennet orders context lock poisoned".into(),
        })?;
    let arc = guard.as_ref().ok_or_else(|| PrecompileErrors::Fatal {
        msg: "mersennet orders context not set".into(),
    })?;
    let mut state = arc.lock().map_err(|_| PrecompileErrors::Fatal {
        msg: "mersennet orders state lock poisoned".into(),
    })?;
    Ok(f(&mut state))
}

// ---------------------------------------------------------------------------
// Handler register — plugs the precompile into an EVM builder.
// ---------------------------------------------------------------------------

/// Stateful wrapper so the orders precompile receives `&mut InnerEvmContext`
/// (journaled state + db) and can move native MRSN — depositCollateral debits
/// the caller into the escrow at this precompile's address; withdrawCollateral
/// pays it back. Collateral is therefore 1:1 backed by native MRSN.
#[derive(Clone)]
struct MersennetOrdersPrecompile;

impl ContextStatefulPrecompileMut<InMemoryDB> for MersennetOrdersPrecompile {
    fn call_mut(
        &mut self,
        bytes: &Bytes,
        gas_limit: u64,
        evmctx: &mut InnerEvmContext<InMemoryDB>,
    ) -> PrecompileResult {
        mersennet_orders_precompile(bytes, gas_limit, evmctx)
    }
}

#[allow(clippy::arc_with_non_send_sync)]
pub fn register_mersennet_orders_precompile(handler: &mut EvmHandler<'_, (), InMemoryDB>) {
    let prev_load = handler.pre_execution.load_precompiles.clone();
    handler.pre_execution.load_precompiles = Arc::new(move || {
        let mut precompiles = prev_load();
        precompiles.extend([(
            MERSENNET_ORDERS_PRECOMPILE,
            ContextPrecompile::ContextStatefulMut(Box::new(MersennetOrdersPrecompile)),
        )]);
        precompiles
    });
}

// ---------------------------------------------------------------------------
// Shielded precompiles — Phase 4 of the privacy redesign.
//
// 0x0200 → shieldedTransfer(bytes) — bincode(ShieldedTransferTx)
// 0x0201 → shield(uint256,bytes) / unshield(bytes) bridge
// 0x0300 → verifyStateProof(bytes) — SP1 light-client gate
//
// All three share the global ShieldedEvm context. See ADR-014,
// ADR-017, ADR-018.
// ---------------------------------------------------------------------------

static SHIELDED_EVM_CTX: Lazy<Mutex<Option<Arc<Mutex<ShieldedEvm>>>>> =
    Lazy::new(|| Mutex::new(None));

pub fn set_shielded_evm_context(evm: Arc<Mutex<ShieldedEvm>>) {
    *SHIELDED_EVM_CTX.lock().unwrap() = Some(evm);
}

pub fn clear_shielded_evm_context() {
    *SHIELDED_EVM_CTX.lock().unwrap() = None;
}

#[derive(Debug)]
struct CodePublicationContext {
    registry: Arc<Mutex<CodePublicationRegistry>>,
    db: InMemoryDB,
    block_number: u64,
}

static CODE_PUBLICATION_CTX: Lazy<Mutex<Option<Arc<Mutex<CodePublicationContext>>>>> =
    Lazy::new(|| Mutex::new(None));

pub fn set_code_publication_context(
    registry: Arc<Mutex<CodePublicationRegistry>>,
    db: InMemoryDB,
    block_number: u64,
) {
    *CODE_PUBLICATION_CTX.lock().unwrap() = Some(Arc::new(Mutex::new(CodePublicationContext {
        registry,
        db,
        block_number,
    })));
}

pub fn clear_code_publication_context() {
    *CODE_PUBLICATION_CTX.lock().unwrap() = None;
}

fn with_code_publication<F, R>(f: F) -> Result<R, PrecompileErrors>
where
    F: FnOnce(&mut CodePublicationContext) -> Result<R, PrecompileErrors>,
{
    let guard = CODE_PUBLICATION_CTX
        .lock()
        .map_err(|_| PrecompileErrors::Fatal {
            msg: "code publication context lock poisoned".into(),
        })?;
    let arc = guard.as_ref().ok_or_else(|| PrecompileErrors::Fatal {
        msg: "code publication context not set".into(),
    })?;
    let mut state = arc.lock().map_err(|_| PrecompileErrors::Fatal {
        msg: "code publication state lock poisoned".into(),
    })?;
    f(&mut state)
}

fn with_shielded_evm<F, R>(f: F) -> Result<R, PrecompileErrors>
where
    F: FnOnce(&mut ShieldedEvm) -> R,
{
    let guard = SHIELDED_EVM_CTX
        .lock()
        .map_err(|_| PrecompileErrors::Fatal {
            msg: "shielded evm context lock poisoned".into(),
        })?;
    let arc = guard.as_ref().ok_or_else(|| PrecompileErrors::Fatal {
        msg: "shielded evm context not set (call set_shielded_evm_context first)".into(),
    })?;
    let mut state = arc.lock().map_err(|_| PrecompileErrors::Fatal {
        msg: "shielded evm state lock poisoned".into(),
    })?;
    Ok(f(&mut state))
}

#[allow(clippy::arc_with_non_send_sync)]
pub fn register_shielded_precompiles(handler: &mut EvmHandler<'_, (), InMemoryDB>) {
    let prev_load = handler.pre_execution.load_precompiles.clone();
    handler.pre_execution.load_precompiles = Arc::new(move || {
        let mut precompiles = prev_load();
        precompiles.extend([
            (
                SHIELDED_TRANSFER_PRECOMPILE,
                ContextPrecompile::Ordinary(Precompile::Env(shielded_transfer_precompile)),
            ),
            (
                SHIELD_BRIDGE_PRECOMPILE,
                ContextPrecompile::Ordinary(Precompile::Env(shield_bridge_precompile)),
            ),
            (
                STATE_PROOF_VERIFIER_PRECOMPILE,
                ContextPrecompile::Ordinary(Precompile::Env(state_proof_verifier_precompile)),
            ),
        ]);
        precompiles
    });
}

#[allow(clippy::arc_with_non_send_sync)]
pub fn register_code_publication_precompile(handler: &mut EvmHandler<'_, (), InMemoryDB>) {
    let prev_load = handler.pre_execution.load_precompiles.clone();
    handler.pre_execution.load_precompiles = Arc::new(move || {
        let mut precompiles = prev_load();
        precompiles.extend([(
            CODE_PUBLICATION_PRECOMPILE,
            ContextPrecompile::Ordinary(Precompile::Env(code_publication_precompile)),
        )]);
        precompiles
    });
}

// ---------------------------------------------------------------------------
// 0x0200 — shieldedTransfer(bytes)
// Input: 4-byte selector || ABI-encoded bytes(payload)
//        payload = bincode(ShieldedEnvelope::Transfer(ShieldedTransferTx))
// Output: 32-byte bool (success).
// ---------------------------------------------------------------------------

fn shielded_transfer_precompile(input: &Bytes, gas_limit: u64, _env: &Env) -> PrecompileResult {
    check_gas(gas_limit, GAS_SHIELDED_TRANSFER)?;
    let payload = abi_decode_bytes(input)?;
    let envelope: ShieldedEnvelope = bincode::deserialize(&payload)
        .map_err(|e| PrecompileError::other(format!("shieldedTransfer: invalid bincode: {e}")))?;

    let success = match envelope {
        ShieldedEnvelope::Transfer(t) => {
            with_shielded_evm(|evm| evm.apply_shielded_transfer(&t))?.is_ok()
        }
        _ => return Err(PrecompileError::other("shieldedTransfer: wrong envelope variant").into()),
    };

    Ok(PrecompileOutput::new(
        GAS_SHIELDED_TRANSFER,
        Bytes::from(encode_bool(success).to_vec()),
    ))
}

// ---------------------------------------------------------------------------
// 0x0201 — shield(uint256 amount, bytes payload) / unshield(bytes payload)
// Dispatches on the 4-byte selector.
// ---------------------------------------------------------------------------

fn shield_bridge_precompile(input: &Bytes, gas_limit: u64, _env: &Env) -> PrecompileResult {
    if input.len() < 4 {
        return Err(PrecompileError::other("shield_bridge: input too short").into());
    }
    let sel = [input[0], input[1], input[2], input[3]];

    if sel == shield_selector() {
        check_gas(gas_limit, GAS_SHIELD)?;
        // shield(uint256,bytes) — amount lives in word 0, the
        // bytes-offset in word 1, then the bytes blob. We only
        // consume the bytes blob; the amount must match the inner
        // ShieldTx (the inner amount is the source of truth, so we
        // reject any mismatch).
        let amount_word =
            read_word(input, 0).ok_or_else(|| PrecompileError::other("shield: missing amount"))?;
        let amount = decode_u256(amount_word);
        let payload = abi_decode_bytes_at(input, 1)?;
        let envelope: ShieldedEnvelope = bincode::deserialize(&payload)
            .map_err(|e| PrecompileError::other(format!("shield: invalid bincode: {e}")))?;
        let tx = match envelope {
            ShieldedEnvelope::Shield(t) => t,
            _ => return Err(PrecompileError::other("shield: wrong envelope variant").into()),
        };
        if tx.amount != amount {
            return Err(PrecompileError::other("shield: amount mismatch with envelope").into());
        }
        let ok = with_shielded_evm(|evm| evm.apply_shield(&tx))?.is_ok();
        return Ok(PrecompileOutput::new(
            GAS_SHIELD,
            Bytes::from(encode_bool(ok).to_vec()),
        ));
    }

    if sel == unshield_selector() {
        check_gas(gas_limit, GAS_UNSHIELD)?;
        let payload = abi_decode_bytes(input)?;
        let envelope: ShieldedEnvelope = bincode::deserialize(&payload)
            .map_err(|e| PrecompileError::other(format!("unshield: invalid bincode: {e}")))?;
        let tx = match envelope {
            ShieldedEnvelope::Unshield(t) => t,
            _ => return Err(PrecompileError::other("unshield: wrong envelope variant").into()),
        };
        let ok = with_shielded_evm(|evm| evm.apply_unshield(&tx))?.is_ok();
        return Ok(PrecompileOutput::new(
            GAS_UNSHIELD,
            Bytes::from(encode_bool(ok).to_vec()),
        ));
    }

    Err(PrecompileError::other("shield_bridge: unknown selector").into())
}

// ---------------------------------------------------------------------------
// 0x0300 — verifyStateProof(bytes proof_bincode) → (bool)
// Light-client / bridge gate. Verifies a StateTransitionProof
// produced by `state_proof::prove_block`.
// ---------------------------------------------------------------------------

fn state_proof_verifier_precompile(input: &Bytes, gas_limit: u64, _env: &Env) -> PrecompileResult {
    check_gas(gas_limit, GAS_STATE_PROOF_VERIFY)?;
    let payload = abi_decode_bytes(input)?;
    let proof: StateTransitionProof = bincode::deserialize(&payload)
        .map_err(|e| PrecompileError::other(format!("verifyStateProof: invalid bincode: {e}")))?;
    let ok = crate::state_proof::verify_block_proof(&proof);
    Ok(PrecompileOutput::new(
        GAS_STATE_PROOF_VERIFY,
        Bytes::from(encode_bool(ok).to_vec()),
    ))
}

fn code_publication_precompile(input: &Bytes, gas_limit: u64, env: &Env) -> PrecompileResult {
    if input.len() < 4 {
        return Err(PrecompileError::other("code publication: input too short").into());
    }

    let sel = [input[0], input[1], input[2], input[3]];
    let caller = env.tx.caller;

    if sel == publish_code_hash_selector() {
        check_gas(gas_limit, GAS_CODE_PUBLICATION_UPDATE)?;
        let contract_word = read_word(input, 0)
            .ok_or_else(|| PrecompileError::other("publishCodeHash: missing contract"))?;
        let contract = decode_address(contract_word);
        let metadata = abi_decode_bytes_at(input, 1)?;
        let metadata_uri =
            if metadata.is_empty() {
                None
            } else {
                Some(String::from_utf8(metadata).map_err(|_| {
                    PrecompileError::other("publishCodeHash: invalid UTF-8 metadata")
                })?)
            };

        with_code_publication(|ctx| {
            let code_hash = ctx
                .db
                .basic(contract)
                .map_err(|e| {
                    PrecompileError::other(format!("publishCodeHash: db read failed: {e}"))
                })?
                .map(|info| info.code_hash)
                .unwrap_or(KECCAK_EMPTY);
            ctx.registry
                .lock()
                .map_err(|_| PrecompileErrors::Fatal {
                    msg: "code publication registry mutex poisoned".into(),
                })?
                .publish(caller, contract, code_hash, metadata_uri, ctx.block_number)
                .map_err(|e| PrecompileError::other(e.to_string()).into())
        })?;

        return Ok(PrecompileOutput::new(
            GAS_CODE_PUBLICATION_UPDATE,
            Bytes::from(encode_bool(true).to_vec()),
        ));
    }

    if sel == revoke_code_hash_selector() {
        check_gas(gas_limit, GAS_CODE_PUBLICATION_UPDATE)?;
        let contract_word = read_word(input, 0)
            .ok_or_else(|| PrecompileError::other("revokeCodeHash: missing contract"))?;
        let contract = decode_address(contract_word);

        with_code_publication(|ctx| {
            ctx.registry
                .lock()
                .map_err(|_| PrecompileErrors::Fatal {
                    msg: "code publication registry mutex poisoned".into(),
                })?
                .revoke(caller, contract)
                .map_err(|e| PrecompileError::other(e.to_string()).into())
        })?;

        return Ok(PrecompileOutput::new(
            GAS_CODE_PUBLICATION_UPDATE,
            Bytes::from(encode_bool(true).to_vec()),
        ));
    }

    Err(PrecompileError::other("code publication: unknown selector").into())
}

/// Decode `bytes` argument that lives in slot 0 (head = offset @ 0,
/// then length || data). For minimal-shape calls we treat the input
/// as just `selector || raw_payload` and skip the 4-byte selector.
fn abi_decode_bytes(input: &Bytes) -> Result<Vec<u8>, PrecompileErrors> {
    if input.len() < 4 {
        return Err(PrecompileError::other("abi_decode_bytes: input < 4").into());
    }
    abi_decode_bytes_at(input, 0)
}

/// Decode `bytes` at parameter slot `slot` (0-indexed after the
/// 4-byte selector).
fn abi_decode_bytes_at(input: &Bytes, slot: usize) -> Result<Vec<u8>, PrecompileErrors> {
    if input.len() < 4 {
        return Err(PrecompileError::other("abi: input < 4").into());
    }
    let head_word =
        read_word(input, slot).ok_or_else(|| PrecompileError::other("abi: missing head word"))?;
    // Offset is relative to the start of the parameter area
    // (i.e. byte 4 of the input).
    let offset_u = U256::from_be_bytes(head_word);
    let offset: usize = offset_u
        .try_into()
        .map_err(|_| PrecompileError::other("abi: offset too large"))?;
    let start = 4usize
        .checked_add(offset)
        .ok_or_else(|| PrecompileError::other("abi: bad offset"))?;
    if input.len() < start + 32 {
        return Err(PrecompileError::other("abi: truncated len word").into());
    }
    let mut len_bytes = [0u8; 32];
    len_bytes.copy_from_slice(&input[start..start + 32]);
    let len: usize = U256::from_be_bytes(len_bytes)
        .try_into()
        .map_err(|_| PrecompileError::other("abi: length too large"))?;
    let data_start = start + 32;
    if input.len() < data_start + len {
        return Err(PrecompileError::other("abi: truncated data").into());
    }
    Ok(input[data_start..data_start + len].to_vec())
}

// ---------------------------------------------------------------------------
// Main precompile entry-point (dispatches on function selector).
// ---------------------------------------------------------------------------

fn mersennet_orders_precompile(
    input: &Bytes,
    gas_limit: u64,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    if !transparent_mersennet_orders_enabled() {
        return Err(PrecompileError::other(
            "transparent MersennetOrders precompile disabled after privacy activation",
        )
        .into());
    }

    if input.len() < 4 {
        return Err(PrecompileError::other("input too short for function selector").into());
    }

    let caller = evmctx.env.tx.caller;

    // Non-payable: the deposit amount is taken from calldata and debited
    // explicitly, so any attached value would be escrowed without crediting
    // anyone. Reject it (the frame revert returns the value to the caller).
    if evmctx.env.tx.value != U256::ZERO {
        return Err(PrecompileError::other(
            "MersennetOrders precompile is non-payable; send value 0 (the deposit amount is in calldata)",
        )
        .into());
    }

    let sel = [input[0], input[1], input[2], input[3]];

    if sel == place_order_selector() {
        handle_place_order(input, gas_limit, caller)
    } else if sel == cancel_order_selector() {
        handle_cancel_order(input, gas_limit, caller)
    } else if sel == deposit_collateral_selector() {
        handle_deposit_collateral(input, gas_limit, caller, evmctx)
    } else if sel == withdraw_collateral_selector() {
        handle_withdraw_collateral(input, gas_limit, caller, evmctx)
    } else if sel == get_position_selector() {
        handle_get_position(input, gas_limit, caller)
    } else if sel == get_collateral_selector() {
        handle_get_collateral(input, gas_limit, caller)
    } else if sel == is_liquidatable_selector() {
        handle_is_liquidatable(input, gas_limit)
    } else if sel == get_best_bid_ask_selector() {
        handle_get_best_bid_ask(input, gas_limit)
    } else {
        Err(PrecompileError::other("unknown function selector").into())
    }
}

// ---------------------------------------------------------------------------
// Gas guard
// ---------------------------------------------------------------------------

fn check_gas(gas_limit: u64, required: u64) -> Result<(), PrecompileErrors> {
    if gas_limit < required {
        Err(PrecompileError::OutOfGas.into())
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// placeOrder(uint64 marketId, bool isBuy, uint256 price, uint256 size, uint8 tif)
//   -> (uint256 orderId, uint256 filled, uint256 remaining)
// ---------------------------------------------------------------------------

fn handle_place_order(input: &Bytes, gas_limit: u64, caller: Address) -> PrecompileResult {
    check_gas(gas_limit, GAS_PLACE_ORDER)?;

    let market_id_w =
        read_word(input, 0).ok_or_else(|| PrecompileError::other("missing marketId"))?;
    let is_buy_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing isBuy"))?;
    let price_w = read_word(input, 2).ok_or_else(|| PrecompileError::other("missing price"))?;
    let size_w = read_word(input, 3).ok_or_else(|| PrecompileError::other("missing size"))?;
    let tif_w = read_word(input, 4).ok_or_else(|| PrecompileError::other("missing tif"))?;

    let market_id = MarketId(decode_u64(market_id_w));
    let side = if decode_bool(is_buy_w) {
        Side::Buy
    } else {
        Side::Sell
    };
    let price = decode_u256(price_w);
    let size = decode_u256(size_w);
    let tif = match decode_u8(tif_w) {
        0 => TimeInForce::Gtc,
        1 => TimeInForce::Ioc,
        2 => TimeInForce::Fok,
        _ => return Err(PrecompileError::other("invalid TimeInForce value").into()),
    };

    let outcome =
        with_orders(|state| state.submit_order(caller, market_id, side, price, size, tif))?;

    let outcome = outcome.map_err(|e| PrecompileError::other(e.to_string()))?;

    record_orders_event(MersennetOrdersEvent::OrderSubmitted {
        order_id: outcome.order_id,
        owner: caller,
        market_id,
        side,
        price,
        size,
        tif,
        filled: outcome.filled,
        remaining: outcome.remaining,
    });
    for trade in &outcome.trades {
        record_orders_event(MersennetOrdersEvent::Trade {
            taker: trade.taker,
            maker: trade.maker,
            market_id: trade.market,
            side: trade.side,
            price: trade.price,
            size: trade.size,
        });
    }

    let order_id_val = outcome
        .order_id
        .map(|id| U256::from(id.0))
        .unwrap_or(U256::ZERO);

    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(&encode_u256(order_id_val));
    out.extend_from_slice(&encode_u256(outcome.filled));
    out.extend_from_slice(&encode_u256(outcome.remaining));

    Ok(PrecompileOutput::new(GAS_PLACE_ORDER, Bytes::from(out)))
}

// ---------------------------------------------------------------------------
// cancelOrder(uint256 orderId) -> (bool success)
// ---------------------------------------------------------------------------

fn handle_cancel_order(input: &Bytes, gas_limit: u64, caller: Address) -> PrecompileResult {
    check_gas(gas_limit, GAS_CANCEL_ORDER)?;

    let id_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing orderId"))?;
    let order_id = OrderId(decode_u256(id_w).as_limbs()[0]);

    // Ownership is enforced here: a caller can only cancel its own resting
    // orders. Cancelling by iterating IDs across other accounts (order-book
    // griefing / manipulation) is rejected with "caller does not own".
    let cancelled = with_orders(|state| state.cancel_order_owned(order_id, caller))?
        .map_err(|e| PrecompileError::other(e.to_string()))?;
    let success = cancelled.is_some();
    if let Some(order) = cancelled {
        record_orders_event(MersennetOrdersEvent::OrderCancelled {
            order_id: order.id,
            owner: order.owner,
            market_id: order.market,
        });
    }

    Ok(PrecompileOutput::new(
        GAS_CANCEL_ORDER,
        Bytes::from(encode_bool(success).to_vec()),
    ))
}

// ---------------------------------------------------------------------------
// depositCollateral(uint256 amount) -> (bool success)
// ---------------------------------------------------------------------------

fn handle_deposit_collateral(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_DEPOSIT_COLLATERAL)?;

    let amt_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing amount"))?;
    let amount = decode_u256(amt_w);

    // Escrow `amount` native MRSN from the caller into this precompile's address
    // FIRST (journaled — a frame revert undoes it). Only on success do we credit
    // the orders-side collateral, so the two ledgers never desync.
    match evmctx
        .journaled_state
        .transfer(
            &caller,
            &MERSENNET_ORDERS_PRECOMPILE,
            amount,
            &mut evmctx.db,
        )
        .map_err(|_| PrecompileError::other("collateral deposit: state error"))?
    {
        None => {}
        Some(_) => {
            return Err(
                PrecompileError::other("insufficient MRSN balance for collateral deposit").into(),
            );
        }
    }

    with_orders(|state| state.deposit_collateral(caller, amount))?;
    record_orders_event(MersennetOrdersEvent::CollateralDeposited {
        owner: caller,
        amount,
    });

    Ok(PrecompileOutput::new(
        GAS_DEPOSIT_COLLATERAL,
        Bytes::from(encode_bool(true).to_vec()),
    ))
}

// ---------------------------------------------------------------------------
// withdrawCollateral(uint256 amount) -> (bool success)
// ---------------------------------------------------------------------------

fn handle_withdraw_collateral(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_WITHDRAW_COLLATERAL)?;

    let amt_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing amount"))?;
    let amount = decode_u256(amt_w);

    // Validate + decrement orders-side collateral first; it only succeeds if the
    // account stays above maintenance margin, and leaves collateral untouched on
    // failure. Return false (not an error) for an ineligible withdrawal.
    let withdraw_result = with_orders(|state| state.withdraw_collateral(caller, amount))?;
    if withdraw_result.is_err() {
        return Ok(PrecompileOutput::new(
            GAS_WITHDRAW_COLLATERAL,
            Bytes::from(encode_bool(false).to_vec()),
        ));
    }

    // Pay the native MRSN back out of the escrow. Under the deposit invariant the
    // escrow always covers it; if it somehow doesn't, roll the orders-side
    // decrement back so the two ledgers stay consistent.
    match evmctx
        .journaled_state
        .transfer(
            &MERSENNET_ORDERS_PRECOMPILE,
            &caller,
            amount,
            &mut evmctx.db,
        )
        .map_err(|_| PrecompileError::other("collateral withdrawal: state error"))?
    {
        None => Ok(PrecompileOutput::new(
            GAS_WITHDRAW_COLLATERAL,
            Bytes::from(encode_bool(true).to_vec()),
        )),
        Some(_) => {
            let _ = with_orders(|state| state.deposit_collateral(caller, amount));
            Err(PrecompileError::other("collateral withdrawal: escrow underfunded").into())
        }
    }
}

// ---------------------------------------------------------------------------
// getPosition(uint64 marketId) -> (int128 size, uint256 entryPrice)
// ---------------------------------------------------------------------------

fn handle_get_position(input: &Bytes, gas_limit: u64, caller: Address) -> PrecompileResult {
    check_gas(gas_limit, GAS_GET_POSITION)?;

    let mid_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing marketId"))?;
    let market_id = MarketId(decode_u64(mid_w));

    let (size, entry_price) = with_orders(|state| {
        state
            .accounts
            .get(&caller)
            .and_then(|a| a.positions.get(&market_id))
            .map(|p| (p.size, p.entry_price))
            .unwrap_or((0, U256::ZERO))
    })?;

    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&encode_i128(size));
    out.extend_from_slice(&encode_u256(entry_price));

    Ok(PrecompileOutput::new(GAS_GET_POSITION, Bytes::from(out)))
}

// ---------------------------------------------------------------------------
// getCollateral() -> (uint256 collateral)
// ---------------------------------------------------------------------------

fn handle_get_collateral(_input: &Bytes, gas_limit: u64, caller: Address) -> PrecompileResult {
    check_gas(gas_limit, GAS_GET_COLLATERAL)?;

    let collateral = with_orders(|state| {
        state
            .accounts
            .get(&caller)
            .map(|a| a.collateral)
            .unwrap_or(U256::ZERO)
    })?;

    Ok(PrecompileOutput::new(
        GAS_GET_COLLATERAL,
        Bytes::from(encode_u256(collateral).to_vec()),
    ))
}

// ---------------------------------------------------------------------------
// isLiquidatable(address account) -> (bool)
// ---------------------------------------------------------------------------

fn handle_is_liquidatable(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    check_gas(gas_limit, GAS_IS_LIQUIDATABLE)?;

    let addr_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing account"))?;
    let account = decode_address(addr_w);

    let liquidatable = with_orders(|state| state.is_liquidatable(account))?;

    Ok(PrecompileOutput::new(
        GAS_IS_LIQUIDATABLE,
        Bytes::from(encode_bool(liquidatable).to_vec()),
    ))
}

// ---------------------------------------------------------------------------
// getBestBidAsk(uint64 marketId) -> (uint256 bestBid, uint256 bestAsk)
// ---------------------------------------------------------------------------

fn handle_get_best_bid_ask(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    check_gas(gas_limit, GAS_GET_BEST_BID_ASK)?;

    let mid_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing marketId"))?;
    let market_id = MarketId(decode_u64(mid_w));

    let (best_bid, best_ask) = with_orders(|state| match state.order_book(market_id) {
        Some(view) => {
            let bid = view.bids.last().map(|l| l.price).unwrap_or(U256::ZERO);
            let ask = view.asks.first().map(|l| l.price).unwrap_or(U256::ZERO);
            (bid, ask)
        }
        None => (U256::ZERO, U256::ZERO),
    })?;

    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&encode_u256(best_bid));
    out.extend_from_slice(&encode_u256(best_ask));

    Ok(PrecompileOutput::new(
        GAS_GET_BEST_BID_ASK,
        Bytes::from(out),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_mersennet_orders_precompile_rejects_when_disabled() {
        set_transparent_mersennet_orders_enabled(false);

        let input = Bytes::from(get_collateral_selector().to_vec());
        let mut evmctx = InnerEvmContext::new_with_env(InMemoryDB::default(), Box::default());
        let err = mersennet_orders_precompile(&input, GAS_GET_COLLATERAL, &mut evmctx)
            .expect_err("transparent precompile should be disabled");

        assert!(
            format!("{err:?}").contains("disabled after privacy activation"),
            "unexpected error: {err:?}"
        );

        set_transparent_mersennet_orders_enabled(true);
    }
}
