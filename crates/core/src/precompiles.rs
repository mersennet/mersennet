use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use revm::db::InMemoryDB;
use revm::handler::register::EvmHandler;
use revm::precompile::Precompile;
use revm::primitives::{
    Address, B256, Bytes, Env, KECCAK_EMPTY, PrecompileError, PrecompileErrors, PrecompileOutput,
    PrecompileResult, U256, keccak256,
};
use revm::{ContextPrecompile, ContextStatefulPrecompileMut, InnerEvmContext};

use crate::code_publication::CodePublicationRegistry;
use crate::errors::MersennetOrdersError;
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
static MERSENNET_ORDERS_EVENTS: Lazy<Mutex<Vec<DomainEvent>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
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

/// Delegated staking precompile (0x…0400). Shares the orders shared-context
/// (the staking ledger lives inside `MersennetOrdersState`), and escrows
/// delegated principal + delegator rewards at its own address.
#[derive(Clone)]
struct StakingPrecompile;

impl ContextStatefulPrecompileMut<InMemoryDB> for StakingPrecompile {
    fn call_mut(
        &mut self,
        bytes: &Bytes,
        gas_limit: u64,
        evmctx: &mut InnerEvmContext<InMemoryDB>,
    ) -> PrecompileResult {
        staking_precompile(bytes, gas_limit, evmctx)
    }
}

#[allow(clippy::arc_with_non_send_sync)]
pub fn register_mersennet_orders_precompile(handler: &mut EvmHandler<'_, (), InMemoryDB>) {
    let prev_load = handler.pre_execution.load_precompiles.clone();
    handler.pre_execution.load_precompiles = Arc::new(move || {
        let mut precompiles = prev_load();
        precompiles.extend([
            (
                MERSENNET_ORDERS_PRECOMPILE,
                ContextPrecompile::ContextStatefulMut(Box::new(MersennetOrdersPrecompile)),
            ),
            (
                STAKING_PRECOMPILE,
                ContextPrecompile::ContextStatefulMut(Box::new(StakingPrecompile)),
            ),
        ]);
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
    /// Pre-tx `address -> code_hash` snapshot. The publication precompile
    /// only ever reads code hashes, so snapshotting just those (instead of
    /// cloning the entire `InMemoryDB` per transaction) keeps identical
    /// semantics at a fraction of the cost.
    code_hashes: HashMap<Address, B256>,
    block_number: u64,
}

static CODE_PUBLICATION_CTX: Lazy<Mutex<Option<Arc<Mutex<CodePublicationContext>>>>> =
    Lazy::new(|| Mutex::new(None));

pub fn set_code_publication_context(
    registry: Arc<Mutex<CodePublicationRegistry>>,
    code_hashes: HashMap<Address, B256>,
    block_number: u64,
) {
    *CODE_PUBLICATION_CTX.lock().unwrap() = Some(Arc::new(Mutex::new(CodePublicationContext {
        registry,
        code_hashes,
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
                .code_hashes
                .get(&contract)
                .copied()
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
    } else if sel == place_order_ext_selector() {
        handle_place_order_ext(input, gas_limit, caller)
    } else if sel == create_market_selector() {
        handle_create_market(input, gas_limit, caller, evmctx)
    } else if sel == cancel_order_selector() {
        handle_cancel_order(input, gas_limit, caller)
    } else if sel == deposit_collateral_selector() {
        handle_deposit_collateral(input, gas_limit, caller, evmctx)
    } else if sel == withdraw_collateral_selector() {
        handle_withdraw_collateral(input, gas_limit, caller, evmctx)
    } else if sel == deposit_collateral_multi_selector() {
        handle_deposit_collateral_multi(input, gas_limit, caller, evmctx)
    } else if sel == withdraw_collateral_multi_selector() {
        handle_withdraw_collateral_multi(input, gas_limit, caller, evmctx)
    } else if sel == get_collateral_multi_selector() {
        handle_get_collateral_multi(input, gas_limit)
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
// placeOrderExt(uint64 marketId, bool isBuy, uint256 price, uint256 size,
//               uint8 tif, uint8 flags, uint64 expireAtBlock)
//   -> (uint256 orderId, uint256 filled, uint256 remaining)
// flags bit 0 = post-only. expireAtBlock 0 = never (plain GTC).
// ---------------------------------------------------------------------------

fn handle_place_order_ext(input: &Bytes, gas_limit: u64, caller: Address) -> PrecompileResult {
    check_gas(gas_limit, GAS_PLACE_ORDER)?;

    let market_id_w =
        read_word(input, 0).ok_or_else(|| PrecompileError::other("missing marketId"))?;
    let is_buy_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing isBuy"))?;
    let price_w = read_word(input, 2).ok_or_else(|| PrecompileError::other("missing price"))?;
    let size_w = read_word(input, 3).ok_or_else(|| PrecompileError::other("missing size"))?;
    let tif_w = read_word(input, 4).ok_or_else(|| PrecompileError::other("missing tif"))?;
    let flags_w = read_word(input, 5).ok_or_else(|| PrecompileError::other("missing flags"))?;
    let expire_w =
        read_word(input, 6).ok_or_else(|| PrecompileError::other("missing expireAtBlock"))?;

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
    let flags = decode_u8(flags_w);
    if flags & !0x01 != 0 {
        return Err(PrecompileError::other("unknown order flags").into());
    }
    let post_only = flags & 0x01 != 0;
    let expire_at = decode_u64(expire_w);

    let outcome = with_orders(|state| {
        state.submit_order_ext(
            caller, market_id, side, price, size, tif, post_only, expire_at,
        )
    })?;
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
// createMarket(bytes32 symbol, uint256 tickSize, uint256 lotSize)
//   -> (uint64 marketId)
// Permissionless listing. Charges CREATE_MARKET_FEE_WEI native MRSN from the
// caller into the precompile escrow and credits the CLOB insurance fund.
// ---------------------------------------------------------------------------

fn handle_create_market(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_CREATE_MARKET)?;

    let symbol_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing symbol"))?;
    let tick_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing tickSize"))?;
    let lot_w = read_word(input, 2).ok_or_else(|| PrecompileError::other("missing lotSize"))?;

    let symbol_len = symbol_w.iter().position(|b| *b == 0).unwrap_or(32);
    let symbol = std::str::from_utf8(&symbol_w[..symbol_len])
        .map_err(|_| PrecompileError::other("symbol must be ASCII"))?
        .to_string();
    let tick_size = decode_u256(tick_w);
    let lot_size = decode_u256(lot_w);

    // Listing fee: escrow first (journaled — a frame revert undoes it),
    // create the market only if payment succeeded.
    let fee = U256::from(CREATE_MARKET_FEE_WEI);
    match evmctx
        .journaled_state
        .transfer(&caller, &MERSENNET_ORDERS_PRECOMPILE, fee, &mut evmctx.db)
        .map_err(|_| PrecompileError::other("createMarket: state error"))?
    {
        None => {}
        Some(_) => {
            return Err(PrecompileError::other(
                "insufficient MRSN balance for the market listing fee",
            )
            .into());
        }
    }

    let created = with_orders(|state| {
        state
            .create_market_checked(symbol.clone(), tick_size, lot_size)
            .inspect(|_| {
                state.insurance_fund = state.insurance_fund.saturating_add(fee);
            })
    })?;
    let market_id = match created {
        Ok(id) => id,
        Err(e) => {
            // Refund the escrowed fee before surfacing the error — the frame
            // may not revert if a wrapping contract swallows the failure.
            let _ = evmctx.journaled_state.transfer(
                &MERSENNET_ORDERS_PRECOMPILE,
                &caller,
                fee,
                &mut evmctx.db,
            );
            return Err(PrecompileError::other(e.to_string()).into());
        }
    };

    record_orders_event(MersennetOrdersEvent::MarketAdded {
        market_id,
        symbol,
        tick_size,
        lot_size,
    });

    let mut out = [0u8; 32];
    out[24..32].copy_from_slice(&market_id.0.to_be_bytes());
    Ok(PrecompileOutput::new(
        GAS_CREATE_MARKET,
        Bytes::from(out.to_vec()),
    ))
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
// Multi-asset collateral: move ERC-20 balances into/out of the precompile
// escrow by writing the token's `balanceOf` mapping slots directly (no EVM
// sub-call). Storage key for `mapping(address=>uint256)` at slot `s` is
// keccak256(pad32(holder) ++ pad32(s)).
// ---------------------------------------------------------------------------

fn erc20_balance_slot(holder: Address, mapping_slot: U256) -> U256 {
    let mut buf = [0u8; 64];
    buf[12..32].copy_from_slice(holder.as_slice());
    buf[32..64].copy_from_slice(&mapping_slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(buf).0)
}

fn erc20_move(
    evmctx: &mut InnerEvmContext<InMemoryDB>,
    token: Address,
    mapping_slot: U256,
    from: Address,
    to: Address,
    amount: U256,
) -> Result<bool, PrecompileErrors> {
    // Warm the token account so sload/sstore can assume presence, and mark it
    // touched: revm's journal finalize drops untouched accounts at commit, so
    // without this the sstores below would be silently discarded.
    evmctx
        .journaled_state
        .load_account(token, &mut evmctx.db)
        .map_err(|_| PrecompileError::other("erc20: token account load failed"))?;
    evmctx.journaled_state.touch(&token);

    let from_key = erc20_balance_slot(from, mapping_slot);
    let (from_bal, _) = evmctx
        .journaled_state
        .sload(token, from_key, &mut evmctx.db)
        .map_err(|_| PrecompileError::other("erc20: sload failed"))?;
    if from_bal < amount {
        return Ok(false);
    }
    let to_key = erc20_balance_slot(to, mapping_slot);
    let (to_bal, _) = evmctx
        .journaled_state
        .sload(token, to_key, &mut evmctx.db)
        .map_err(|_| PrecompileError::other("erc20: sload failed"))?;

    evmctx
        .journaled_state
        .sstore(token, from_key, from_bal - amount, &mut evmctx.db)
        .map_err(|_| PrecompileError::other("erc20: sstore failed"))?;
    evmctx
        .journaled_state
        .sstore(token, to_key, to_bal.saturating_add(amount), &mut evmctx.db)
        .map_err(|_| PrecompileError::other("erc20: sstore failed"))?;
    Ok(true)
}

/// depositTokenCollateral(address token, uint256 amount) -> (bool)
fn handle_deposit_collateral_multi(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_DEPOSIT_COLLATERAL_MULTI)?;
    let token_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing token"))?;
    let amt_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing amount"))?;
    let token = decode_address(token_w);
    let amount = decode_u256(amt_w);

    let slot = with_orders(|state| state.collateral_assets.get(&token).map(|a| a.balances_slot))?;
    let Some(mapping_slot) = slot else {
        return Err(PrecompileError::other("token is not a registered collateral asset").into());
    };

    let moved = erc20_move(
        evmctx,
        token,
        mapping_slot,
        caller,
        MERSENNET_ORDERS_PRECOMPILE,
        amount,
    )?;
    if !moved {
        return Err(PrecompileError::other("insufficient token balance for deposit").into());
    }

    with_orders(|state| state.deposit_token_collateral(caller, token, amount))?
        .map_err(|e| PrecompileError::other(e.message()))?;

    Ok(PrecompileOutput::new(
        GAS_DEPOSIT_COLLATERAL_MULTI,
        Bytes::from(encode_bool(true).to_vec()),
    ))
}

/// withdrawTokenCollateral(address token, uint256 amount) -> (bool)
fn handle_withdraw_collateral_multi(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_WITHDRAW_COLLATERAL_MULTI)?;
    let token_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing token"))?;
    let amt_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing amount"))?;
    let token = decode_address(token_w);
    let amount = decode_u256(amt_w);

    // Decrement the CLOB-side balance first (enforces maintenance margin);
    // only then release the escrowed tokens.
    let slot = with_orders(|state| {
        let slot = state.collateral_assets.get(&token).map(|a| a.balances_slot);
        match slot {
            Some(s) => state
                .withdraw_token_collateral(caller, token, amount)
                .map(|_| s),
            None => Err(MersennetOrdersError::UnknownCollateralAsset),
        }
    })?;
    let mapping_slot = match slot {
        Ok(s) => s,
        Err(e) => {
            return Ok(PrecompileOutput::new(
                GAS_WITHDRAW_COLLATERAL_MULTI,
                Bytes::from({
                    let _ = e;
                    encode_bool(false).to_vec()
                }),
            ));
        }
    };

    let moved = erc20_move(
        evmctx,
        token,
        mapping_slot,
        MERSENNET_ORDERS_PRECOMPILE,
        caller,
        amount,
    )?;
    if !moved {
        // Roll the CLOB-side decrement back so ledgers stay consistent.
        let _ = with_orders(|state| state.deposit_token_collateral(caller, token, amount));
        return Err(PrecompileError::other("withdraw: escrow underfunded").into());
    }

    Ok(PrecompileOutput::new(
        GAS_WITHDRAW_COLLATERAL_MULTI,
        Bytes::from(encode_bool(true).to_vec()),
    ))
}

/// getTokenCollateral(address account, address token) -> (uint256 amount)
fn handle_get_collateral_multi(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    check_gas(gas_limit, GAS_GET_COLLATERAL_MULTI)?;
    let acct_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing account"))?;
    let token_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing token"))?;
    let account = decode_address(acct_w);
    let token = decode_address(token_w);

    let amount = with_orders(|state| {
        state
            .accounts
            .get(&account)
            .and_then(|a| a.token_collateral.get(&token))
            .copied()
            .unwrap_or(U256::ZERO)
    })?;

    Ok(PrecompileOutput::new(
        GAS_GET_COLLATERAL_MULTI,
        Bytes::from(encode_u256(amount).to_vec()),
    ))
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

// ═══════════════════════════════════════════════════════════════════════════
// Delegated staking precompile (0x…0400)
// ═══════════════════════════════════════════════════════════════════════════

fn staking_precompile(
    input: &Bytes,
    gas_limit: u64,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    if input.len() < 4 {
        return Err(PrecompileError::other("input too short for function selector").into());
    }
    let caller = evmctx.env.tx.caller;
    if evmctx.env.tx.value != U256::ZERO {
        return Err(PrecompileError::other(
            "staking precompile is non-payable; the amount is in calldata",
        )
        .into());
    }
    let current_block = evmctx.env.block.number.saturating_to::<u64>();
    let sel = [input[0], input[1], input[2], input[3]];

    if sel == delegate_selector() {
        handle_delegate(input, gas_limit, caller, evmctx)
    } else if sel == undelegate_selector() {
        handle_undelegate(input, gas_limit, caller, current_block)
    } else if sel == claim_rewards_selector() {
        handle_claim_rewards(input, gas_limit, caller, evmctx)
    } else if sel == withdraw_unbonded_selector() {
        handle_withdraw_unbonded(gas_limit, caller, current_block, evmctx)
    } else if sel == get_delegation_selector() {
        handle_get_delegation(input, gas_limit)
    } else if sel == get_validator_staking_selector() {
        handle_get_validator_staking(input, gas_limit)
    } else if sel == get_unbonding_selector() {
        handle_get_unbonding(input, gas_limit, current_block)
    } else if sel == register_validator_selector() {
        handle_register_validator(input, gas_limit, caller, current_block, evmctx)
    } else if sel == add_self_stake_selector() {
        handle_add_self_stake(input, gas_limit, caller, evmctx)
    } else if sel == unregister_validator_selector() {
        handle_unregister_validator(input, gas_limit, caller)
    } else if sel == rotate_validator_key_selector() {
        handle_rotate_validator_key(input, gas_limit, caller)
    } else {
        Err(PrecompileError::other("unknown function selector").into())
    }
}

// ─── Open validator set ─────────────────────────────────────────────────────

fn verify_registration_proof(
    operator: Address,
    identity: Address,
    proof: &[u8],
) -> Result<(), PrecompileErrors> {
    let msg = crate::crypto::validator_registration_message(operator, identity);
    let signer = crate::crypto::recover_eip191(msg.as_bytes(), proof)
        .map_err(|e| PrecompileError::other(format!("invalid identity proof: {e}")))?;
    if signer != identity {
        return Err(PrecompileError::other("identity proof was not signed by the node key").into());
    }
    Ok(())
}

/// registerValidator(address identity, uint256 selfStake, uint256 commissionBps, bytes proof)
/// The caller becomes the operator; `selfStake` MRSN is escrowed from its
/// balance; `proof` is the node key's signature over the registration message
/// (the node prints it via `whoami` / `mersennet_nodeIdentity`).
fn handle_register_validator(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    current_block: u64,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_REGISTER_VALIDATOR)?;
    let identity = decode_address(
        read_word(input, 0).ok_or_else(|| PrecompileError::other("missing identity"))?,
    );
    let amount = decode_u256(
        read_word(input, 1).ok_or_else(|| PrecompileError::other("missing selfStake"))?,
    );
    let commission = decode_u256(
        read_word(input, 2).ok_or_else(|| PrecompileError::other("missing commissionBps"))?,
    );
    let proof = read_bytes_arg(input, 3).ok_or_else(|| PrecompileError::other("missing proof"))?;
    let commission_bps: u64 = commission.try_into().unwrap_or(u64::MAX);
    if commission_bps > 10_000 {
        return Err(PrecompileError::other("commissionBps must be <= 10000").into());
    }
    verify_registration_proof(caller, identity, &proof)?;

    // Escrow the self-stake (journaled — a frame revert undoes it).
    match evmctx
        .journaled_state
        .transfer(&caller, &STAKING_PRECOMPILE, amount, &mut evmctx.db)
        .map_err(|_| PrecompileError::other("registerValidator: state error"))?
    {
        None => {}
        Some(_) => {
            return Err(
                PrecompileError::other("insufficient MRSN balance for the self-stake").into(),
            );
        }
    }
    let result = with_orders(|state| {
        state
            .staking
            .register_validator(caller, identity, amount, commission_bps, current_block)
    })?;
    if let Err(e) = result {
        let _ =
            evmctx
                .journaled_state
                .transfer(&STAKING_PRECOMPILE, &caller, amount, &mut evmctx.db);
        return Err(PrecompileError::other(e.message()).into());
    }
    Ok(PrecompileOutput::new(
        GAS_REGISTER_VALIDATOR,
        Bytes::from(encode_bool(true).to_vec()),
    ))
}

/// addSelfStake(address identity, uint256 amount)
fn handle_add_self_stake(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_VALIDATOR_ADMIN)?;
    let identity = decode_address(
        read_word(input, 0).ok_or_else(|| PrecompileError::other("missing identity"))?,
    );
    let amount =
        decode_u256(read_word(input, 1).ok_or_else(|| PrecompileError::other("missing amount"))?);
    match evmctx
        .journaled_state
        .transfer(&caller, &STAKING_PRECOMPILE, amount, &mut evmctx.db)
        .map_err(|_| PrecompileError::other("addSelfStake: state error"))?
    {
        None => {}
        Some(_) => return Err(PrecompileError::other("insufficient MRSN balance").into()),
    }
    let result = with_orders(|state| state.staking.add_self_stake(caller, identity, amount))?;
    if let Err(e) = result {
        let _ =
            evmctx
                .journaled_state
                .transfer(&STAKING_PRECOMPILE, &caller, amount, &mut evmctx.db);
        return Err(PrecompileError::other(e.message()).into());
    }
    Ok(PrecompileOutput::new(
        GAS_VALIDATOR_ADMIN,
        Bytes::from(encode_bool(true).to_vec()),
    ))
}

/// unregisterValidator(address identity) — leaves at the next epoch; the
/// self-stake then unbonds and is collected with withdrawUnbonded().
fn handle_unregister_validator(input: &Bytes, gas_limit: u64, caller: Address) -> PrecompileResult {
    check_gas(gas_limit, GAS_VALIDATOR_ADMIN)?;
    let identity = decode_address(
        read_word(input, 0).ok_or_else(|| PrecompileError::other("missing identity"))?,
    );
    let result = with_orders(|state| state.staking.unregister_validator(caller, identity))?;
    result.map_err(|e| PrecompileError::other(e.message()))?;
    Ok(PrecompileOutput::new(
        GAS_VALIDATOR_ADMIN,
        Bytes::from(encode_bool(true).to_vec()),
    ))
}

/// rotateValidatorKey(address identity, address newIdentity, bytes proof) —
/// proof is signed by the NEW node key; takes effect at the next epoch.
fn handle_rotate_validator_key(input: &Bytes, gas_limit: u64, caller: Address) -> PrecompileResult {
    check_gas(gas_limit, GAS_VALIDATOR_ADMIN)?;
    let identity = decode_address(
        read_word(input, 0).ok_or_else(|| PrecompileError::other("missing identity"))?,
    );
    let new_identity = decode_address(
        read_word(input, 1).ok_or_else(|| PrecompileError::other("missing newIdentity"))?,
    );
    let proof = read_bytes_arg(input, 2).ok_or_else(|| PrecompileError::other("missing proof"))?;
    verify_registration_proof(caller, new_identity, &proof)?;
    let result = with_orders(|state| {
        state
            .staking
            .rotate_identity(caller, identity, new_identity)
    })?;
    result.map_err(|e| PrecompileError::other(e.message()))?;
    Ok(PrecompileOutput::new(
        GAS_VALIDATOR_ADMIN,
        Bytes::from(encode_bool(true).to_vec()),
    ))
}

/// delegate(address validator, uint256 amount)
fn handle_delegate(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_DELEGATE)?;
    let val_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing validator"))?;
    let amt_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing amount"))?;
    let validator = decode_address(val_w);
    let amount = decode_u256(amt_w);

    // Escrow the principal first (journaled — a frame revert undoes it).
    match evmctx
        .journaled_state
        .transfer(&caller, &STAKING_PRECOMPILE, amount, &mut evmctx.db)
        .map_err(|_| PrecompileError::other("delegate: state error"))?
    {
        None => {}
        Some(_) => {
            return Err(PrecompileError::other("insufficient MRSN balance to delegate").into());
        }
    }

    let result = with_orders(|state| state.staking.delegate(caller, validator, amount))?;
    if let Err(e) = result {
        // Refund the escrow before surfacing the error.
        let _ =
            evmctx
                .journaled_state
                .transfer(&STAKING_PRECOMPILE, &caller, amount, &mut evmctx.db);
        return Err(PrecompileError::other(e.message()).into());
    }

    Ok(PrecompileOutput::new(
        GAS_DELEGATE,
        Bytes::from(encode_bool(true).to_vec()),
    ))
}

/// undelegate(address validator, uint256 amount) -> (uint256 unlockAtBlock)
fn handle_undelegate(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    current_block: u64,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_UNDELEGATE)?;
    let val_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing validator"))?;
    let amt_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing amount"))?;
    let validator = decode_address(val_w);
    let amount = decode_u256(amt_w);

    let result = with_orders(|state| {
        state
            .staking
            .undelegate(caller, validator, amount, current_block)
    })?;
    let unlock_at = result.map_err(|e| PrecompileError::other(e.message()))?;

    Ok(PrecompileOutput::new(
        GAS_UNDELEGATE,
        Bytes::from(encode_u256(U256::from(unlock_at)).to_vec()),
    ))
}

/// claimRewards(address validator) -> (uint256 paid)
fn handle_claim_rewards(
    input: &Bytes,
    gas_limit: u64,
    caller: Address,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_CLAIM_REWARDS)?;
    let val_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing validator"))?;
    let validator = decode_address(val_w);

    let payout = with_orders(|state| state.staking.claim_rewards(caller, validator))?;
    if !payout.is_zero() {
        match evmctx
            .journaled_state
            .transfer(&STAKING_PRECOMPILE, &caller, payout, &mut evmctx.db)
            .map_err(|_| PrecompileError::other("claimRewards: state error"))?
        {
            None => {}
            Some(_) => {
                // Escrow underfunded — roll the accrual back and fail loudly.
                let _ = with_orders(|state| {
                    if let Some(d) = state.staking.delegations.get_mut(&(caller, validator)) {
                        d.pending_rewards = d.pending_rewards.saturating_add(payout);
                    }
                });
                return Err(
                    PrecompileError::other("claimRewards: staking escrow underfunded").into(),
                );
            }
        }
    }

    Ok(PrecompileOutput::new(
        GAS_CLAIM_REWARDS,
        Bytes::from(encode_u256(payout).to_vec()),
    ))
}

/// withdrawUnbonded() -> (uint256 paid)
fn handle_withdraw_unbonded(
    gas_limit: u64,
    caller: Address,
    current_block: u64,
    evmctx: &mut InnerEvmContext<InMemoryDB>,
) -> PrecompileResult {
    check_gas(gas_limit, GAS_WITHDRAW_UNBONDED)?;

    let payout = with_orders(|state| state.staking.withdraw_unbonded(caller, current_block))?;
    if !payout.is_zero() {
        match evmctx
            .journaled_state
            .transfer(&STAKING_PRECOMPILE, &caller, payout, &mut evmctx.db)
            .map_err(|_| PrecompileError::other("withdrawUnbonded: state error"))?
        {
            None => {}
            Some(_) => {
                return Err(
                    PrecompileError::other("withdrawUnbonded: staking escrow underfunded").into(),
                );
            }
        }
    }

    Ok(PrecompileOutput::new(
        GAS_WITHDRAW_UNBONDED,
        Bytes::from(encode_u256(payout).to_vec()),
    ))
}

/// getDelegation(address delegator, address validator)
///   -> (uint256 amount, uint256 pendingRewards)
fn handle_get_delegation(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    check_gas(gas_limit, GAS_STAKING_VIEW)?;
    let del_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing delegator"))?;
    let val_w = read_word(input, 1).ok_or_else(|| PrecompileError::other("missing validator"))?;
    let delegator = decode_address(del_w);
    let validator = decode_address(val_w);

    let (amount, pending) = with_orders(|state| {
        let amount = state
            .staking
            .delegations
            .get(&(delegator, validator))
            .map(|d| d.amount)
            .unwrap_or(U256::ZERO);
        let pending = state.staking.pending_rewards(delegator, validator);
        (amount, pending)
    })?;

    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&encode_u256(amount));
    out.extend_from_slice(&encode_u256(pending));
    Ok(PrecompileOutput::new(GAS_STAKING_VIEW, Bytes::from(out)))
}

/// getValidatorStaking(address validator)
///   -> (uint256 delegatedTotal, uint256 commissionBps)
fn handle_get_validator_staking(input: &Bytes, gas_limit: u64) -> PrecompileResult {
    check_gas(gas_limit, GAS_STAKING_VIEW)?;
    let val_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing validator"))?;
    let validator = decode_address(val_w);

    let (total, commission) = with_orders(|state| {
        state
            .staking
            .pools
            .get(&validator)
            .map(|p| (p.delegated_total, p.commission_bps))
            .unwrap_or((U256::ZERO, 0))
    })?;

    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&encode_u256(total));
    out.extend_from_slice(&encode_u256(U256::from(commission)));
    Ok(PrecompileOutput::new(GAS_STAKING_VIEW, Bytes::from(out)))
}

/// getUnbonding(address delegator)
///   -> (uint256 totalUnbonding, uint256 withdrawableNow, uint256 nextUnlockBlock)
fn handle_get_unbonding(input: &Bytes, gas_limit: u64, current_block: u64) -> PrecompileResult {
    check_gas(gas_limit, GAS_STAKING_VIEW)?;
    let del_w = read_word(input, 0).ok_or_else(|| PrecompileError::other("missing delegator"))?;
    let delegator = decode_address(del_w);

    let (total, withdrawable, next_unlock) = with_orders(|state| {
        let mut total = U256::ZERO;
        let mut withdrawable = U256::ZERO;
        let mut next_unlock = 0u64;
        if let Some(entries) = state.staking.unbondings.get(&delegator) {
            for e in entries {
                total = total.saturating_add(e.amount);
                if e.unlock_at <= current_block {
                    withdrawable = withdrawable.saturating_add(e.amount);
                } else if next_unlock == 0 || e.unlock_at < next_unlock {
                    next_unlock = e.unlock_at;
                }
            }
        }
        (total, withdrawable, next_unlock)
    })?;

    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(&encode_u256(total));
    out.extend_from_slice(&encode_u256(withdrawable));
    out.extend_from_slice(&encode_u256(U256::from(next_unlock)));
    Ok(PrecompileOutput::new(GAS_STAKING_VIEW, Bytes::from(out)))
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
