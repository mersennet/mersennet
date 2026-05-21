//! Shielded-mode RPC methods (Phase 6 of the privacy redesign).
//!
//! These are dispatched from
//! [`crate::rpc_router::route`](super::rpc_router) when the method
//! name matches one of the shielded-method strings in
//! [`is_shielded_method`].
//!
//! ## Wire format
//!
//! The opaque ZK payloads (proofs, encrypted ciphertexts, note
//! commitments, etc.) are encoded as **bincode-then-0x-hex** strings
//! in JSON. This avoids a per-struct JSON schema explosion and lets
//! the SDK + chain agree on a single canonical format. The structs
//! themselves derive `Serialize`/`Deserialize`, so SDK users that
//! prefer a typed schema can opt into JSON encoding by including
//! `?format=json` (handled by the SDK layer, not here).
//!
//! ## Gating
//!
//! Mutation methods return RPC error `-32605` ("disabled until
//! privacy hard fork activates") when
//! [`prime_chain::engine::Engine::privacy_mode_activated`] is
//! `false`. Read-only methods (root, balance count, latest proof)
//! always return a structured response — pre-fork they describe the
//! initial empty state.

use prime_chain::engine::Engine;
use prime_chain::shielded_evm::{ShieldedEnvelope, ShieldedTransferTx, ShieldTx, UnshieldTx};
use prime_chain::shielded_orders::ShieldedOrderTx;
use prime_chain::liquidation_auction::{LiquidationClaim, LiquidationExecute};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ShieldedRpcError {
    pub code: i64,
    pub message: String,
}

impl ShieldedRpcError {
    pub fn into_value(self) -> Value {
        json!({ "code": self.code, "message": self.message })
    }
}

pub type ShieldedRouteResult = Result<Option<Value>, ShieldedRpcError>;

const ERR_DISABLED: i64 = -32605;
const ERR_INVALID_PARAMS: i64 = -32602;
const ERR_INTERNAL: i64 = -32000;

/// Try to dispatch a shielded-mode RPC. Returns `Ok(None)` if the
/// method name does not match any shielded method.
pub fn try_dispatch(
    method: &str,
    params: Value,
    engine: &mut Engine,
) -> ShieldedRouteResult {
    if !is_shielded_method(method) {
        return Ok(None);
    }

    let active = engine.privacy_mode_activated();
    match method {
        // ─────────── Read-only methods (always available) ───────────
        "prime_getShieldedRoot" => Ok(Some(read_shielded_root(engine))),
        "prime_getShieldedBalance" => Ok(Some(read_shielded_balance(engine))),
        "prime_getShieldedNotes" => Ok(Some(read_shielded_notes(engine))),
        "prime_getStateProof" => Ok(Some(read_state_proof(engine, &params))),
        "prime_getLatestStateProof" => Ok(Some(read_latest_state_proof(engine))),
        "prime_verifyStateProof" => Ok(Some(verify_state_proof(params)?)),
        "prime_getShieldedMarketAggregates" => {
            Ok(Some(read_market_aggregates(engine)))
        }

        // ─────────── Mutation methods (gated on activation) ─────────
        "prime_submitShieldedTransfer" => {
            require_active(active)?;
            submit_shielded(engine, params, |t| ShieldedEnvelope::Transfer(t))
        }
        "prime_submitShield" => {
            require_active(active)?;
            submit_shield_inner(engine, params)
        }
        "prime_submitUnshield" => {
            require_active(active)?;
            submit_unshield(engine, params)
        }
        "prime_submitShieldedOrder" => {
            require_active(active)?;
            submit_shielded_order(engine, params)
        }
        "prime_submitLiquidationClaim" => {
            require_active(active)?;
            submit_liquidation_claim(engine, params)
        }
        "prime_submitLiquidationExecute" => {
            require_active(active)?;
            submit_liquidation_execute(engine, params)
        }
        "prime_registerLiquidator" => {
            require_active(active)?;
            register_liquidator(engine, params)
        }

        // ─────────── View-key methods (post-fork only) ──────────────
        "prime_viewGrantToken" | "prime_viewRevokeToken" => {
            require_active(active)?;
            Err(ShieldedRpcError {
                code: ERR_DISABLED,
                message: format!("{method} requires viewing-key infrastructure (Workstream H)"),
            })
        }

        _ => Ok(None),
    }
}

fn require_active(active: bool) -> Result<(), ShieldedRpcError> {
    if active {
        Ok(())
    } else {
        Err(ShieldedRpcError {
            code: ERR_DISABLED,
            message: "shielded methods are disabled until the privacy hard fork activates"
                .to_string(),
        })
    }
}

fn is_shielded_method(method: &str) -> bool {
    matches!(
        method,
        "prime_getShieldedRoot"
            | "prime_getShieldedBalance"
            | "prime_getShieldedNotes"
            | "prime_getShieldedMarketAggregates"
            | "prime_submitShieldedTransfer"
            | "prime_submitShield"
            | "prime_submitUnshield"
            | "prime_submitShieldedOrder"
            | "prime_submitLiquidationClaim"
            | "prime_submitLiquidationExecute"
            | "prime_registerLiquidator"
            | "prime_getStateProof"
            | "prime_getLatestStateProof"
            | "prime_verifyStateProof"
            | "prime_viewGrantToken"
            | "prime_viewRevokeToken"
    )
}

// ---------------------------------------------------------------------------
// Read-only handlers
// ---------------------------------------------------------------------------

fn read_shielded_root(engine: &Engine) -> Value {
    let root = engine.shielded_evm.state.current_root().to_bytes();
    json!({
        "shieldedStateRoot": hex_bytes(&root),
        "blockNumber": engine.latest_height(),
        "noteCount": engine.shielded_evm.state.note_count(),
        "nullifierCount": engine.shielded_evm.state.nullifier_count(),
    })
}

fn read_shielded_balance(engine: &Engine) -> Value {
    json!({
        "totalNoteCount": engine.shielded_evm.state.note_count(),
        "totalNullifierCount": engine.shielded_evm.state.nullifier_count(),
        "transparentEoaCount": engine.shielded_evm.transparent_balances.len(),
    })
}

fn read_shielded_notes(engine: &Engine) -> Value {
    // We expose only the aggregate count. Decryption to per-account
    // notes requires viewing keys, which are not handled server-side
    // (the SDK does it locally).
    json!({
        "noteCount": engine.shielded_evm.state.note_count(),
        "currentRoot": hex_bytes(&engine.shielded_evm.state.current_root().to_bytes()),
    })
}

fn read_latest_state_proof(engine: &Engine) -> Value {
    match engine.chain.last().and_then(|b| b.state_proof.clone()) {
        Some(proof) => state_proof_to_value(&proof),
        None => json!({ "blockHeight": 0, "proof": Value::Null }),
    }
}

/// `prime_getStateProof(blockNumberOrTag)` — return the SP1
/// state-transition proof attached to the block at the requested
/// height. The `blockNumber` parameter may be:
///
/// - Omitted / `null` / `"latest"` → behaves like
///   `prime_getLatestStateProof`.
/// - A hex string `"0x..."` or a decimal u64 → looks up the block
///   by exact height.
///
/// Output JSON shape (matches `read_latest_state_proof`):
///
/// ```json
/// {
///   "blockHeight": ...,
///   "prevStateRoot": "0x..",
///   "newStateRoot":  "0x..",
///   "blockHash":     "0x..",
///   "txCount": ...,
///   "proofBincodeHex": "0x..",
///   "proofType": "..."
/// }
/// ```
fn read_state_proof(engine: &Engine, params: &Value) -> Value {
    // Extract the first positional arg.
    let target_height: Option<u64> = params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| {
            if v.is_null() {
                return None;
            }
            if let Some(s) = v.as_str() {
                if s == "latest" {
                    return None;
                }
                let stripped = s.strip_prefix("0x").unwrap_or(s);
                return u64::from_str_radix(stripped, 16).ok();
            }
            v.as_u64()
        });

    let block_opt = match target_height {
        None => engine.chain.last(),
        Some(h) => engine.chain.iter().find(|b| b.number == h),
    };

    match block_opt.and_then(|b| b.state_proof.clone()) {
        Some(proof) => state_proof_to_value(&proof),
        None => json!({
            "blockHeight": target_height.unwrap_or(0),
            "proof": Value::Null,
            "reason": "no state proof at this block (privacy mode inactive or block not found)",
        }),
    }
}

fn state_proof_to_value(proof: &prime_chain::zk_proofs::StateTransitionProof) -> Value {
    match bincode::serialize(proof) {
        Ok(bytes) => json!({
            "blockHeight": proof.block_height,
            "prevStateRoot": hex_bytes(proof.prev_state_root.as_slice()),
            "newStateRoot": hex_bytes(proof.new_state_root.as_slice()),
            "blockHash": hex_bytes(proof.block_hash.as_slice()),
            "txCount": proof.tx_count,
            "proofBincodeHex": hex_bytes(&bytes),
            "proofType": format!("{:?}", proof.proof_type),
        }),
        Err(e) => json!({ "error": format!("serialize: {e}") }),
    }
}

fn verify_state_proof(params: Value) -> Result<Value, ShieldedRpcError> {
    let payload = decode_bincode_hex_param(&params, "proofBincodeHex")?;
    let proof: prime_chain::zk_proofs::StateTransitionProof = bincode::deserialize(&payload)
        .map_err(|e| invalid_params(format!("verifyStateProof: bad bincode: {e}")))?;
    let ok = prime_chain::state_proof::verify_block_proof(&proof);
    Ok(json!({ "valid": ok }))
}

fn read_market_aggregates(engine: &Engine) -> Value {
    let mut markets = Vec::new();
    for (mid, agg) in engine.shielded_orders.aggregates.iter() {
        markets.push(json!({
            "marketId": mid.0,
            "markPrice": hex_u256(agg.mark_price),
            "longOpenInterest": hex_u256(agg.long_open_interest),
            "shortOpenInterest": hex_u256(agg.short_open_interest),
            "lastClearingPrice": hex_u256(agg.last_clearing_price),
            "lastVolume": hex_u256(agg.last_volume),
            "liquidatableCount": agg.liquidatable_count,
        }));
    }
    json!({ "markets": markets })
}

// ---------------------------------------------------------------------------
// Mutation handlers
// ---------------------------------------------------------------------------

fn submit_shielded<F>(
    engine: &mut Engine,
    params: Value,
    wrap: F,
) -> ShieldedRouteResult
where
    F: FnOnce(ShieldedTransferTx) -> ShieldedEnvelope,
{
    let payload = decode_bincode_hex_param(&params, "envelopeBincodeHex")?;
    let tx: ShieldedTransferTx = bincode::deserialize(&payload)
        .map_err(|e| invalid_params(format!("invalid envelope bincode: {e}")))?;
    let envelope = wrap(tx);
    apply_envelope_directly(engine, envelope, "shieldedTransfer")
}

fn submit_shield_inner(engine: &mut Engine, params: Value) -> ShieldedRouteResult {
    let payload = decode_bincode_hex_param(&params, "envelopeBincodeHex")?;
    let tx: ShieldTx = bincode::deserialize(&payload)
        .map_err(|e| invalid_params(format!("invalid shield envelope: {e}")))?;
    apply_envelope_directly(engine, ShieldedEnvelope::Shield(tx), "shield")
}

fn submit_unshield(engine: &mut Engine, params: Value) -> ShieldedRouteResult {
    let payload = decode_bincode_hex_param(&params, "envelopeBincodeHex")?;
    let tx: UnshieldTx = bincode::deserialize(&payload)
        .map_err(|e| invalid_params(format!("invalid unshield envelope: {e}")))?;
    apply_envelope_directly(engine, ShieldedEnvelope::Unshield(tx), "unshield")
}

fn submit_shielded_order(engine: &mut Engine, params: Value) -> ShieldedRouteResult {
    let payload = decode_bincode_hex_param(&params, "envelopeBincodeHex")?;
    let tx: ShieldedOrderTx = bincode::deserialize(&payload)
        .map_err(|e| invalid_params(format!("invalid order envelope: {e}")))?;
    apply_envelope_directly(
        engine,
        ShieldedEnvelope::Order(Box::new(tx)),
        "shieldedOrder",
    )
}

fn submit_liquidation_claim(engine: &mut Engine, params: Value) -> ShieldedRouteResult {
    let payload = decode_bincode_hex_param(&params, "claimBincodeHex")?;
    let claim: LiquidationClaim = bincode::deserialize(&payload)
        .map_err(|e| invalid_params(format!("invalid claim: {e}")))?;
    // Split-borrow workaround: `submit_claim` reads `&ShieldedState`
    // while mutating the auction. We can't pass `engine.shielded_evm.state`
    // through a method call on `engine.liquidation_auction` because
    // both come off the same `engine` borrow. The fix is to grab the
    // state's current root + recent-roots check inline.
    let state_ref = &engine.shielded_evm.state;
    let recent = state_ref.is_recent_root(&claim.anchor_root);
    if !recent {
        return Err(ShieldedRpcError {
            code: ERR_INTERNAL,
            message: "submit_claim: anchor root is stale".into(),
        });
    }
    // Reborrow split: now use a fresh borrow for the mutable side.
    let dummy_state = prime_chain::shielded_state::ShieldedState::restore(
        &engine.shielded_evm.state.snapshot(),
    );
    let tag = engine
        .liquidation_auction
        .submit_claim(&dummy_state, claim)
        .map_err(|e| ShieldedRpcError {
            code: ERR_INTERNAL,
            message: format!("submit_claim: {e:?}"),
        })?;
    Ok(Some(json!({ "claimTag": hex_bytes(&tag.to_bytes()) })))
}

fn submit_liquidation_execute(engine: &mut Engine, params: Value) -> ShieldedRouteResult {
    let payload = decode_bincode_hex_param(&params, "executeBincodeHex")?;
    let exec: LiquidationExecute = bincode::deserialize(&payload)
        .map_err(|e| invalid_params(format!("invalid execute: {e}")))?;
    apply_envelope_directly(
        engine,
        ShieldedEnvelope::LiquidationExecute(Box::new(exec)),
        "liquidationExecute",
    )
}

fn register_liquidator(engine: &mut Engine, params: Value) -> ShieldedRouteResult {
    // params = [{ bondCommitmentHex: "0x..", bondAmount: "0x.." }]
    let arr = params
        .as_array()
        .ok_or_else(|| invalid_params("expected single-element array"))?;
    let obj = arr
        .first()
        .ok_or_else(|| invalid_params("missing object"))?;
    let bond_hex = obj
        .get("bondCommitmentHex")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("missing bondCommitmentHex"))?;
    let bond_amount_hex = obj
        .get("bondAmount")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("missing bondAmount"))?;

    let bond_bytes = decode_hex(bond_hex)?;
    if bond_bytes.len() != 32 {
        return Err(invalid_params("bondCommitmentHex must be 32 bytes"));
    }
    let mut arr32 = [0u8; 32];
    arr32.copy_from_slice(&bond_bytes);
    let bond_commitment = prime_zkp::Fr::from_bytes_reduce(&arr32);
    let amount: u128 = u128::from_str_radix(bond_amount_hex.trim_start_matches("0x"), 16)
        .map_err(|e| invalid_params(format!("bad bondAmount: {e}")))?;

    let ok = engine
        .liquidation_auction
        .register(bond_commitment, amount, engine.latest_height());
    Ok(Some(json!({ "registered": ok })))
}

/// Build a synthetic 0x7E transaction, push it through the engine's
/// `apply_shielded_tx` dispatcher, and return the receipt-shaped JSON
/// response.
fn apply_envelope_directly(
    engine: &mut Engine,
    envelope: ShieldedEnvelope,
    method_label: &str,
) -> ShieldedRouteResult {
    let mut tx = prime_chain::engine::Transaction::default();
    tx.tx_type = prime_chain::shielded_evm::SHIELDED_TX_TYPE;
    tx.shielded_payload = Some(envelope);
    let execution = engine.apply_shielded_tx(&tx);
    Ok(Some(json!({
        "method": method_label,
        "success": execution.success,
        "gasUsed": execution.gas_used,
        "logs": execution.logs.len(),
    })))
}

// ---------------------------------------------------------------------------
// JSON / hex helpers
// ---------------------------------------------------------------------------

fn invalid_params(msg: impl Into<String>) -> ShieldedRpcError {
    ShieldedRpcError {
        code: ERR_INVALID_PARAMS,
        message: msg.into(),
    }
}

fn decode_bincode_hex_param(params: &Value, key: &str) -> Result<Vec<u8>, ShieldedRpcError> {
    // Accept either `[{ key: "0x.." }]` or `["0x.."]`.
    if let Some(arr) = params.as_array() {
        if let Some(first) = arr.first() {
            if let Some(s) = first.as_str() {
                return decode_hex(s);
            }
            if let Some(s) = first.get(key).and_then(Value::as_str) {
                return decode_hex(s);
            }
        }
    }
    Err(invalid_params(format!("missing or malformed `{key}`")))
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ShieldedRpcError> {
    let trimmed = s.trim_start_matches("0x");
    hex::decode(trimmed).map_err(|e| invalid_params(format!("bad hex: {e}")))
}

fn hex_bytes(b: &[u8]) -> String {
    format!("0x{}", hex::encode(b))
}

fn hex_u256(v: revm::primitives::U256) -> String {
    let bytes = v.to_be_bytes::<32>();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(31);
    format!("0x{}", hex::encode(&bytes[start..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn fresh_engine() -> Engine {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("rpc");
        std::fs::create_dir_all(&sub).unwrap();
        let mut e = Engine::new_with_backend(7919, sub, "redb");
        e.set_token_economics(
            revm::primitives::U256::ZERO,
            revm::primitives::U256::ZERO,
            1,
        );
        std::mem::forget(dir); // leak the temp dir for the rest of the test
        e
    }

    #[test]
    fn unknown_method_returns_ok_none() {
        let mut e = fresh_engine();
        let r = try_dispatch("eth_blockNumber", json!([]), &mut e).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn read_methods_work_pre_fork() {
        let mut e = fresh_engine();
        let root = try_dispatch("prime_getShieldedRoot", json!([]), &mut e)
            .unwrap()
            .unwrap();
        assert!(root.get("shieldedStateRoot").is_some());
        assert_eq!(root["noteCount"], 0);

        let bal = try_dispatch("prime_getShieldedBalance", json!([]), &mut e)
            .unwrap()
            .unwrap();
        assert_eq!(bal["totalNoteCount"], 0);
    }

    #[test]
    fn mutation_methods_are_disabled_pre_fork() {
        let mut e = fresh_engine();
        let err = try_dispatch(
            "prime_submitShieldedTransfer",
            json!([{ "envelopeBincodeHex": "0x" }]),
            &mut e,
        )
        .unwrap_err();
        assert_eq!(err.code, ERR_DISABLED);
    }

    #[test]
    fn mutation_methods_allowed_post_fork() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        // Empty hex payload — we expect an invalid-bincode error,
        // NOT a disabled error, proving the gate is open.
        let err = try_dispatch(
            "prime_submitShieldedTransfer",
            json!([{ "envelopeBincodeHex": "0x" }]),
            &mut e,
        )
        .unwrap_err();
        assert_eq!(err.code, ERR_INVALID_PARAMS);
    }

    #[test]
    fn register_liquidator_post_fork() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        // bondAmount must be ≥ MIN_LIQUIDATOR_BOND (10_000 * 1e18).
        // We use 20_000 * 1e18 = 0x43c33c1937564800000 to be safe.
        let resp = try_dispatch(
            "prime_registerLiquidator",
            json!([{
                "bondCommitmentHex": format!("0x{}", "00".repeat(32)),
                "bondAmount": "0x43c33c1937564800000",
            }]),
            &mut e,
        )
        .unwrap()
        .unwrap();
        assert_eq!(resp["registered"], true);
    }
}
