//! Shielded-mode RPC methods (Phase 6 of the privacy redesign).
//!
//! These are dispatched from `crates/rpc/src/rpc_router.rs::route`
//! when the method name starts with `prime_shielded_*` or `prime_view_*`.
//!
//! Until the privacy hard fork activates, these methods return a
//! structured RPC error with code `-32605` ("method valid but
//! disabled in the current chain mode") and a message pointing
//! callers at the activation height.

use serde_json::{json, Value};

/// Returned RPC error type, mirroring the shape `rpc_router` already
/// returns elsewhere. We keep this module self-contained to avoid
/// circular re-exports.
#[derive(Debug, Clone)]
pub struct ShieldedRpcError {
    pub code: i64,
    pub message: String,
}

impl ShieldedRpcError {
    pub fn into_value(self) -> Value {
        json!({
            "code": self.code,
            "message": self.message,
        })
    }
}

/// Result of dispatching a shielded RPC. `Ok(None)` means the method
/// is not a shielded method; `Ok(Some(...))` is the result; `Err` is
/// a typed error the caller wraps in JSON-RPC error format.
pub type ShieldedRouteResult = Result<Option<Value>, ShieldedRpcError>;

/// Try to dispatch a shielded-mode RPC. Returns `Ok(None)` if the
/// method name does not match any shielded method.
pub fn try_dispatch(method: &str, _params: Value) -> ShieldedRouteResult {
    if !is_shielded_method(method) {
        return Ok(None);
    }
    // Pre-fork: every shielded method is structurally valid but the
    // hard fork has not yet activated.
    Err(ShieldedRpcError {
        code: -32605,
        message: format!(
            "{} is a shielded-mode RPC method and is disabled until the privacy hard fork activates. See docs/internal/zk-privacy-plan.md.",
            method
        ),
    })
}

fn is_shielded_method(method: &str) -> bool {
    matches!(
        method,
        "prime_getShieldedBalance"
        | "prime_getShieldedNotes"
        | "prime_getShieldedPositions"
        | "prime_submitShieldedTransfer"
        | "prime_submitShieldedOrder"
        | "prime_submitShieldClaim"
        | "prime_submitUnshield"
        | "prime_submitLiquidationClaim"
        | "prime_submitLiquidationExecute"
        | "prime_registerLiquidator"
        | "prime_getStateProof"
        | "prime_verifyStateProof"
        | "prime_viewGrantToken"
        | "prime_viewRevokeToken"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_method_returns_ok_none() {
        let r = try_dispatch("eth_blockNumber", json!([])).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn shielded_methods_return_disabled_error_pre_fork() {
        for m in [
            "prime_getShieldedBalance",
            "prime_submitShieldedOrder",
            "prime_getStateProof",
            "prime_viewGrantToken",
        ] {
            let err = try_dispatch(m, json!([])).unwrap_err();
            assert_eq!(err.code, -32605);
            assert!(err.message.contains(m));
        }
    }
}
