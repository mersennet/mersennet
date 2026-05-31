//! Re-export of the revm-free state-transition proof types.
//!
//! The implementation moved to the `prime-state-proof` crate so the SP1
//! state-transition host can depend on it without pulling the `revm`
//! execution stack (and therefore `c-kzg` 1.x), which is what blocked
//! enabling `sp1-sdk/network`. The public path `prime_chain::zk_proofs::*`
//! is preserved here unchanged.

pub use prime_state_proof::zk_proofs::*;
