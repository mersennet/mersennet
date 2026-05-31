//! Re-export of the revm-free SP1 proof glue.
//!
//! The implementation moved to the `prime-state-proof` crate (see
//! `zk_proofs` for the rationale). The public path `prime_chain::zk_sp1::*`
//! is preserved here unchanged. The CLI adapter backend is gated by
//! prime-chain's `sp1` feature, which forwards to `prime-state-proof/sp1`.

pub use prime_state_proof::zk_sp1::*;
