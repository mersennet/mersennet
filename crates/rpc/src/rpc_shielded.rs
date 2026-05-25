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
use prime_chain::liquidation_auction::{LiquidationClaim, LiquidationExecute};
use prime_chain::shielded_evm::{ShieldTx, ShieldedEnvelope, ShieldedTransferTx, UnshieldTx};
use prime_chain::shielded_evm::{ViewingGrantScope, ViewingGrantToken};
use prime_chain::shielded_orders::ShieldedOrderTx;
use revm::primitives::keccak256;
use serde_json::{Value, json};

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
const ERR_FORBIDDEN: i64 = -32604;

/// Try to dispatch a shielded-mode RPC. Returns `Ok(None)` if the
/// method name does not match any shielded method.
pub fn try_dispatch(method: &str, params: Value, engine: &mut Engine) -> ShieldedRouteResult {
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
        "prime_getShieldedMarketAggregates" => Ok(Some(read_market_aggregates(engine))),

        // ─────────── Mutation methods (gated on activation) ─────────
        "prime_submitShieldedTransfer" => {
            require_active(active)?;
            submit_shielded(engine, params, ShieldedEnvelope::Transfer)
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
        "prime_viewGrantToken" => {
            require_active(active)?;
            grant_view_token(engine, params)
        }
        "prime_viewRevokeToken" => {
            require_active(active)?;
            revoke_view_token(engine, params)
        }
        "prime_viewPortfolioDigest" => {
            require_active(active)?;
            read_view_portfolio_digest(engine, params)
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
            | "prime_viewPortfolioDigest"
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
    let target_height: Option<u64> = params.as_array().and_then(|arr| arr.first()).and_then(|v| {
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

fn read_view_portfolio_digest(engine: &Engine, params: Value) -> ShieldedRouteResult {
    let obj = first_param_object(&params)?;
    let grant_id = decode_fixed_hex_field(obj, "grantIdHex", 32)?;
    let token = engine
        .shielded_evm
        .viewing_grants
        .get(&grant_id)
        .ok_or_else(|| invalid_params("unknown grantIdHex"))?;

    if !token.verify_signature() {
        return Err(forbidden("grant token signature verification failed"));
    }
    if engine.shielded_evm.is_viewing_grant_revoked(&grant_id) {
        return Err(forbidden("grant token has been revoked"));
    }
    if !token.is_active_at(engine.latest_height()) {
        return Err(forbidden("grant token is outside its validity window"));
    }
    if !token.has_scope(&ViewingGrantScope::PortfolioDigestExport) {
        return Err(forbidden(
            "grant token is missing exports:portfolio_digest scope",
        ));
    }

    let root = engine.shielded_evm.state.current_root().to_bytes();
    let note_count = engine.shielded_evm.state.note_count() as u64;
    let nullifier_count = engine.shielded_evm.state.nullifier_count() as u64;
    let block_number = engine.latest_height();
    let digest = derive_portfolio_digest(token, &root, block_number, note_count, nullifier_count);

    Ok(Some(json!({
        "grantId": hex_bytes(&grant_id),
        "grantorCommitment": hex_bytes(&token.grantor_commitment),
        "blockNumber": block_number,
        "shieldedStateRoot": hex_bytes(&root),
        "noteCount": note_count,
        "nullifierCount": nullifier_count,
        "capabilitiesHash": hex_bytes(&token.capabilities_hash),
        "portfolioDigest": hex_bytes(&digest),
        "signatureVerified": true,
    })))
}

// ---------------------------------------------------------------------------
// Mutation handlers
// ---------------------------------------------------------------------------

fn submit_shielded<F>(engine: &mut Engine, params: Value, wrap: F) -> ShieldedRouteResult
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
    let dummy_state =
        prime_chain::shielded_state::ShieldedState::restore(&engine.shielded_evm.state.snapshot());
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

fn grant_view_token(engine: &mut Engine, params: Value) -> ShieldedRouteResult {
    let obj = first_param_object(&params)?;
    let grantor_commitment = decode_fixed_hex_field(obj, "grantorCommitmentHex", 32)?;
    let grantor_sig_pubkey = decode_hex_field(obj, "grantorSigPubkeyHex")?;
    let grantee_pubkey = decode_hex_field(obj, "granteePubkeyHex")?;
    if grantor_sig_pubkey.is_empty() {
        return Err(invalid_params("grantorSigPubkeyHex must not be empty"));
    }
    if grantee_pubkey.is_empty() {
        return Err(invalid_params("granteePubkeyHex must not be empty"));
    }

    let scopes = parse_viewing_scopes(obj)?;
    if scopes.is_empty() {
        return Err(invalid_params("scopes must not be empty"));
    }

    let start_block = parse_u64_field(obj, "startBlock")?.unwrap_or_else(|| engine.latest_height());
    let end_block = parse_u64_field(obj, "endBlock")?
        .ok_or_else(|| invalid_params("missing endBlock"))?;
    if end_block < start_block {
        return Err(invalid_params("endBlock must be >= startBlock"));
    }

    let capabilities_hash = match obj.get("capabilitiesHashHex").and_then(Value::as_str) {
        Some(value) => decode_fixed_hex(value, 32)?,
        None => derive_capabilities_hash(&scopes, start_block, end_block),
    };
    let signature = decode_hex_field(obj, "signatureHex")?;
    let grant_id = match obj.get("grantIdHex").and_then(Value::as_str) {
        Some(value) => decode_fixed_hex(value, 32)?,
        None => derive_grant_id(
            engine.chain_id,
            &grantor_commitment,
            &grantor_sig_pubkey,
            &grantee_pubkey,
            &capabilities_hash,
            start_block,
            end_block,
        ),
    };

    let token = ViewingGrantToken {
        version: 1,
        chain_id: engine.chain_id,
        grant_id,
        grantor_commitment,
        grantor_sig_pubkey,
        grantee_pubkey,
        scopes,
        start_block,
        end_block,
        capabilities_hash,
        signature,
    };
    let signature_verified = token.verify_signature();
    if !signature_verified {
        return Err(invalid_params("grant signature verification failed"));
    }
    let stored = engine.shielded_evm.register_viewing_grant(token.clone());

    Ok(Some(json!({
        "grantToken": viewing_grant_to_value(&token),
        "stored": stored,
        "signatureVerified": signature_verified,
    })))
}

fn revoke_view_token(engine: &mut Engine, params: Value) -> ShieldedRouteResult {
    let obj = first_param_object(&params)?;
    let grant_id = decode_fixed_hex_field(obj, "grantIdHex", 32)?;
    let already_revoked = engine.shielded_evm.is_viewing_grant_revoked(&grant_id);
    let revoked_at_block = engine.latest_height();
    let revoked = engine
        .shielded_evm
        .revoke_viewing_grant(grant_id, revoked_at_block);

    Ok(Some(json!({
        "grantId": hex_bytes(&grant_id),
        "revoked": revoked,
        "alreadyRevoked": already_revoked,
        "revokedAtBlock": if revoked || already_revoked { json!(revoked_at_block) } else { Value::Null },
    })))
}

/// Build a synthetic 0x7E transaction, push it through the engine's
/// `apply_shielded_tx` dispatcher, and return the receipt-shaped JSON
/// response.
fn apply_envelope_directly(
    engine: &mut Engine,
    envelope: ShieldedEnvelope,
    method_label: &str,
) -> ShieldedRouteResult {
    let tx = prime_chain::engine::Transaction {
        tx_type: prime_chain::shielded_evm::SHIELDED_TX_TYPE,
        shielded_payload: Some(envelope),
        ..Default::default()
    };
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

fn forbidden(msg: impl Into<String>) -> ShieldedRpcError {
    ShieldedRpcError {
        code: ERR_FORBIDDEN,
        message: msg.into(),
    }
}

fn first_param_object<'a>(params: &'a Value) -> Result<&'a Value, ShieldedRpcError> {
    let arr = params
        .as_array()
        .ok_or_else(|| invalid_params("expected single-element array"))?;
    arr.first()
        .ok_or_else(|| invalid_params("missing object"))
}

fn decode_hex_field(obj: &Value, key: &str) -> Result<Vec<u8>, ShieldedRpcError> {
    let value = obj
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params(format!("missing {key}")))?;
    decode_hex(value)
}

fn decode_fixed_hex_field(obj: &Value, key: &str, expected_len: usize) -> Result<[u8; 32], ShieldedRpcError> {
    let value = obj
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params(format!("missing {key}")))?;
    decode_fixed_hex(value, expected_len)
}

fn decode_fixed_hex(s: &str, expected_len: usize) -> Result<[u8; 32], ShieldedRpcError> {
    let bytes = decode_hex(s)?;
    if bytes.len() != expected_len {
        return Err(invalid_params(format!("expected {expected_len} bytes")));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn parse_u64_field(obj: &Value, key: &str) -> Result<Option<u64>, ShieldedRpcError> {
    let Some(value) = obj.get(key) else {
        return Ok(None);
    };
    if let Some(number) = value.as_u64() {
        return Ok(Some(number));
    }
    if let Some(text) = value.as_str() {
        let stripped = text.strip_prefix("0x").unwrap_or(text);
        return u64::from_str_radix(stripped, 16)
            .map(Some)
            .map_err(|e| invalid_params(format!("bad {key}: {e}")));
    }
    Err(invalid_params(format!("bad {key}")))
}

fn parse_viewing_scopes(obj: &Value) -> Result<Vec<ViewingGrantScope>, ShieldedRpcError> {
    let scopes = obj
        .get("scopes")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_params("missing scopes"))?;
    scopes
        .iter()
        .map(|scope| {
            let value = scope
                .as_str()
                .ok_or_else(|| invalid_params("scope entries must be strings"))?;
            parse_viewing_scope(value)
        })
        .collect()
}

fn parse_viewing_scope(scope: &str) -> Result<ViewingGrantScope, ShieldedRpcError> {
    match scope {
        "notes:read" => Ok(ViewingGrantScope::NotesRead),
        "balances:read" => Ok(ViewingGrantScope::BalancesRead),
        "positions:read" => Ok(ViewingGrantScope::PositionsRead),
        "orders:read" => Ok(ViewingGrantScope::OrdersRead),
        "liquidations:read" => Ok(ViewingGrantScope::LiquidationsRead),
        "exports:portfolio_digest" => Ok(ViewingGrantScope::PortfolioDigestExport),
        _ => Err(invalid_params(format!("unsupported scope: {scope}"))),
    }
}

fn derive_capabilities_hash(scopes: &[ViewingGrantScope], start_block: u64, end_block: u64) -> [u8; 32] {
    let mut payload = Vec::new();
    for scope in scopes {
        payload.extend_from_slice(scope.as_str().as_bytes());
        payload.push(0);
    }
    payload.extend_from_slice(&start_block.to_le_bytes());
    payload.extend_from_slice(&end_block.to_le_bytes());
    let digest = revm::primitives::keccak256(payload);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

fn derive_grant_id(
    chain_id: u64,
    grantor_commitment: &[u8; 32],
    grantor_sig_pubkey: &[u8],
    grantee_pubkey: &[u8],
    capabilities_hash: &[u8; 32],
    start_block: u64,
    end_block: u64,
) -> [u8; 32] {
    let mut payload = Vec::new();
    payload.extend_from_slice(&chain_id.to_le_bytes());
    payload.extend_from_slice(grantor_commitment);
    payload.extend_from_slice(grantor_sig_pubkey);
    payload.extend_from_slice(grantee_pubkey);
    payload.extend_from_slice(capabilities_hash);
    payload.extend_from_slice(&start_block.to_le_bytes());
    payload.extend_from_slice(&end_block.to_le_bytes());
    let digest = revm::primitives::keccak256(payload);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

fn viewing_grant_to_value(token: &ViewingGrantToken) -> Value {
    json!({
        "version": token.version,
        "chainId": token.chain_id,
        "grantId": hex_bytes(&token.grant_id),
        "grantorCommitment": hex_bytes(&token.grantor_commitment),
        "grantorSigPubkey": hex_bytes(&token.grantor_sig_pubkey),
        "granteePubkey": hex_bytes(&token.grantee_pubkey),
        "scopes": token.scopes.iter().map(ViewingGrantScope::as_str).collect::<Vec<_>>(),
        "startBlock": token.start_block,
        "endBlock": token.end_block,
        "capabilitiesHash": hex_bytes(&token.capabilities_hash),
        "signature": hex_bytes(&token.signature),
    })
}

fn derive_portfolio_digest(
    token: &ViewingGrantToken,
    shielded_state_root: &[u8; 32],
    block_number: u64,
    note_count: u64,
    nullifier_count: u64,
) -> [u8; 32] {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"PRIME_VIEW_PORTFOLIO_DIGEST_V1");
    payload.extend_from_slice(&token.grant_id);
    payload.extend_from_slice(&token.grantor_commitment);
    payload.extend_from_slice(&token.capabilities_hash);
    payload.extend_from_slice(shielded_state_root);
    payload.extend_from_slice(&block_number.to_le_bytes());
    payload.extend_from_slice(&note_count.to_le_bytes());
    payload.extend_from_slice(&nullifier_count.to_le_bytes());
    let digest = keccak256(payload);
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

fn decode_bincode_hex_param(params: &Value, key: &str) -> Result<Vec<u8>, ShieldedRpcError> {
    // Accept either `[{ key: "0x.." }]` or `["0x.."]`.
    if let Some(arr) = params.as_array()
        && let Some(first) = arr.first()
    {
        if let Some(s) = first.as_str() {
            return decode_hex(s);
        }
        if let Some(s) = first.get(key).and_then(Value::as_str) {
            return decode_hex(s);
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
    use k256::ecdsa::signature::hazmat::PrehashSigner;
    use k256::ecdsa::{Signature, SigningKey};
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

    fn sign_viewing_grant(
        signing_key: &SigningKey,
        chain_id: u64,
        grantor_commitment: [u8; 32],
        grantee_pubkey: Vec<u8>,
        scopes: Vec<ViewingGrantScope>,
        start_block: u64,
        end_block: u64,
    ) -> (String, String, String) {
        let grantor_sig_pubkey = signing_key
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec();
        let capabilities_hash = derive_capabilities_hash(&scopes, start_block, end_block);
        let grant_id = derive_grant_id(
            chain_id,
            &grantor_commitment,
            &grantor_sig_pubkey,
            &grantee_pubkey,
            &capabilities_hash,
            start_block,
            end_block,
        );
        let token = ViewingGrantToken {
            version: 1,
            chain_id,
            grant_id,
            grantor_commitment,
            grantor_sig_pubkey: grantor_sig_pubkey.clone(),
            grantee_pubkey,
            scopes,
            start_block,
            end_block,
            capabilities_hash,
            signature: Vec::new(),
        };
        let digest = token.signing_digest();
        let signature: Signature = signing_key.sign_prehash(&digest).unwrap();
        (
            format!("0x{}", hex::encode(grantor_sig_pubkey)),
            format!("0x{}", hex::encode(grant_id)),
            format!("0x{}", hex::encode(signature.to_bytes())),
        )
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

    #[test]
    fn view_grant_token_registers_post_fork() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        let signing_key = SigningKey::from_bytes((&[7u8; 32]).into()).unwrap();
        let (grantor_sig_pubkey_hex, _, signature_hex) = sign_viewing_grant(
            &signing_key,
            e.chain_id,
            [0x11; 32],
            vec![0x22; 33],
            vec![ViewingGrantScope::BalancesRead, ViewingGrantScope::OrdersRead],
            5,
            10,
        );

        let resp = try_dispatch(
            "prime_viewGrantToken",
            json!([{
                "grantorCommitmentHex": format!("0x{}", "11".repeat(32)),
                "grantorSigPubkeyHex": grantor_sig_pubkey_hex,
                "granteePubkeyHex": format!("0x{}", "22".repeat(33)),
                "scopes": ["balances:read", "orders:read"],
                "startBlock": "0x5",
                "endBlock": "0xa",
                "signatureHex": signature_hex,
            }]),
            &mut e,
        )
        .unwrap()
        .unwrap();

        assert_eq!(resp["stored"], true);
        assert_eq!(resp["signatureVerified"], true);
        assert_eq!(resp["grantToken"]["startBlock"], 5);
        assert_eq!(resp["grantToken"]["endBlock"], 10);
        assert_eq!(e.shielded_evm.viewing_grants.len(), 1);
    }

    #[test]
    fn view_grant_token_can_be_revoked_post_fork() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        let signing_key = SigningKey::from_bytes((&[9u8; 32]).into()).unwrap();
        let (grantor_sig_pubkey_hex, _, signature_hex) = sign_viewing_grant(
            &signing_key,
            e.chain_id,
            [0x44; 32],
            vec![0x55; 33],
            vec![ViewingGrantScope::NotesRead],
            e.latest_height(),
            20,
        );

        let grant = try_dispatch(
            "prime_viewGrantToken",
            json!([{
                "grantorCommitmentHex": format!("0x{}", "44".repeat(32)),
                "grantorSigPubkeyHex": grantor_sig_pubkey_hex,
                "granteePubkeyHex": format!("0x{}", "55".repeat(33)),
                "scopes": ["notes:read"],
                "endBlock": "0x14",
                "signatureHex": signature_hex,
            }]),
            &mut e,
        )
        .unwrap()
        .unwrap();
        let grant_id = grant["grantToken"]["grantId"].as_str().unwrap().to_string();

        let revoke = try_dispatch(
            "prime_viewRevokeToken",
            json!([{
                "grantIdHex": grant_id,
            }]),
            &mut e,
        )
        .unwrap()
        .unwrap();

        assert_eq!(revoke["revoked"], true);
        assert_eq!(e.shielded_evm.viewing_grant_revocations.len(), 1);
    }

    #[test]
    fn view_portfolio_digest_requires_scope_and_valid_grant() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        let signing_key = SigningKey::from_bytes((&[11u8; 32]).into()).unwrap();
        let (grantor_sig_pubkey_hex, grant_id_hex, signature_hex) = sign_viewing_grant(
            &signing_key,
            e.chain_id,
            [0x66; 32],
            vec![0x77; 33],
            vec![ViewingGrantScope::PortfolioDigestExport],
            e.latest_height(),
            e.latest_height() + 5,
        );

        try_dispatch(
            "prime_viewGrantToken",
            json!([{
                "grantorCommitmentHex": format!("0x{}", "66".repeat(32)),
                "grantorSigPubkeyHex": grantor_sig_pubkey_hex,
                "granteePubkeyHex": format!("0x{}", "77".repeat(33)),
                "scopes": ["exports:portfolio_digest"],
                "startBlock": format!("0x{:x}", e.latest_height()),
                "endBlock": format!("0x{:x}", e.latest_height() + 5),
                "grantIdHex": grant_id_hex,
                "signatureHex": signature_hex,
            }]),
            &mut e,
        )
        .unwrap();

        let digest = try_dispatch(
            "prime_viewPortfolioDigest",
            json!([{
                "grantIdHex": grant_id_hex,
            }]),
            &mut e,
        )
        .unwrap()
        .unwrap();

        assert_eq!(digest["signatureVerified"], true);
        assert!(digest.get("portfolioDigest").is_some());
    }

    #[test]
    fn view_grant_token_rejects_invalid_signature() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        let signing_key = SigningKey::from_bytes((&[13u8; 32]).into()).unwrap();
        let (grantor_sig_pubkey_hex, _, mut signature_hex) = sign_viewing_grant(
            &signing_key,
            e.chain_id,
            [0x88; 32],
            vec![0x99; 33],
            vec![ViewingGrantScope::BalancesRead],
            1,
            9,
        );
        signature_hex.replace_range(signature_hex.len() - 2.., "00");

        let err = try_dispatch(
            "prime_viewGrantToken",
            json!([{
                "grantorCommitmentHex": format!("0x{}", "88".repeat(32)),
                "grantorSigPubkeyHex": grantor_sig_pubkey_hex,
                "granteePubkeyHex": format!("0x{}", "99".repeat(33)),
                "scopes": ["balances:read"],
                "startBlock": "0x1",
                "endBlock": "0x9",
                "signatureHex": signature_hex,
            }]),
            &mut e,
        )
        .unwrap_err();

        assert_eq!(err.code, ERR_INVALID_PARAMS);
    }

    #[test]
    fn view_portfolio_digest_rejects_revoked_grant() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        let signing_key = SigningKey::from_bytes((&[15u8; 32]).into()).unwrap();
        let (grantor_sig_pubkey_hex, grant_id_hex, signature_hex) = sign_viewing_grant(
            &signing_key,
            e.chain_id,
            [0xAA; 32],
            vec![0xBB; 33],
            vec![ViewingGrantScope::PortfolioDigestExport],
            e.latest_height(),
            e.latest_height() + 5,
        );

        try_dispatch(
            "prime_viewGrantToken",
            json!([{
                "grantorCommitmentHex": format!("0x{}", "aa".repeat(32)),
                "grantorSigPubkeyHex": grantor_sig_pubkey_hex,
                "granteePubkeyHex": format!("0x{}", "bb".repeat(33)),
                "scopes": ["exports:portfolio_digest"],
                "startBlock": format!("0x{:x}", e.latest_height()),
                "endBlock": format!("0x{:x}", e.latest_height() + 5),
                "grantIdHex": grant_id_hex.clone(),
                "signatureHex": signature_hex,
            }]),
            &mut e,
        )
        .unwrap();

        try_dispatch(
            "prime_viewRevokeToken",
            json!([{ "grantIdHex": grant_id_hex.clone() }]),
            &mut e,
        )
        .unwrap();

        let err = try_dispatch(
            "prime_viewPortfolioDigest",
            json!([{ "grantIdHex": grant_id_hex }]),
            &mut e,
        )
        .unwrap_err();

        assert_eq!(err.code, ERR_FORBIDDEN);
        assert!(err.message.contains("revoked"));
    }

    #[test]
    fn view_portfolio_digest_rejects_expired_grant() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        let signing_key = SigningKey::from_bytes((&[17u8; 32]).into()).unwrap();
        let current = e.latest_height();
        let (grantor_sig_pubkey_hex, grant_id_hex, signature_hex) = sign_viewing_grant(
            &signing_key,
            e.chain_id,
            [0xCC; 32],
            vec![0xDD; 33],
            vec![ViewingGrantScope::PortfolioDigestExport],
            current.saturating_sub(3),
            current.saturating_sub(1),
        );

        try_dispatch(
            "prime_viewGrantToken",
            json!([{
                "grantorCommitmentHex": format!("0x{}", "cc".repeat(32)),
                "grantorSigPubkeyHex": grantor_sig_pubkey_hex,
                "granteePubkeyHex": format!("0x{}", "dd".repeat(33)),
                "scopes": ["exports:portfolio_digest"],
                "startBlock": format!("0x{:x}", current.saturating_sub(3)),
                "endBlock": format!("0x{:x}", current.saturating_sub(1)),
                "grantIdHex": grant_id_hex.clone(),
                "signatureHex": signature_hex,
            }]),
            &mut e,
        )
        .unwrap();

        let expired_height = current.saturating_add(1);
        let expired_block = prime_chain::engine::Block {
            number: expired_height,
            ..Default::default()
        };
        e.chain.push(expired_block);

        let err = try_dispatch(
            "prime_viewPortfolioDigest",
            json!([{ "grantIdHex": grant_id_hex }]),
            &mut e,
        )
        .unwrap_err();

        assert_eq!(err.code, ERR_FORBIDDEN);
        assert!(err.message.contains("validity window"));
    }

    #[test]
    fn view_portfolio_digest_rejects_wrong_scope() {
        let mut e = fresh_engine();
        e.activate_privacy_mode();
        let signing_key = SigningKey::from_bytes((&[19u8; 32]).into()).unwrap();
        let (grantor_sig_pubkey_hex, grant_id_hex, signature_hex) = sign_viewing_grant(
            &signing_key,
            e.chain_id,
            [0xEE; 32],
            vec![0xFF; 33],
            vec![ViewingGrantScope::BalancesRead],
            e.latest_height(),
            e.latest_height() + 5,
        );

        try_dispatch(
            "prime_viewGrantToken",
            json!([{
                "grantorCommitmentHex": format!("0x{}", "ee".repeat(32)),
                "grantorSigPubkeyHex": grantor_sig_pubkey_hex,
                "granteePubkeyHex": format!("0x{}", "ff".repeat(33)),
                "scopes": ["balances:read"],
                "startBlock": format!("0x{:x}", e.latest_height()),
                "endBlock": format!("0x{:x}", e.latest_height() + 5),
                "grantIdHex": grant_id_hex.clone(),
                "signatureHex": signature_hex,
            }]),
            &mut e,
        )
        .unwrap();

        let err = try_dispatch(
            "prime_viewPortfolioDigest",
            json!([{ "grantIdHex": grant_id_hex }]),
            &mut e,
        )
        .unwrap_err();

        assert_eq!(err.code, ERR_FORBIDDEN);
        assert!(err.message.contains("missing exports:portfolio_digest scope"));
    }
}
