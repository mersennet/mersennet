//! revm-free state-transition proof envelopes for Mersennet.
//!
//! These types used to live in `mersennet` (`crates/core`), which pulls
//! the full `revm` execution stack and therefore `c-kzg` 1.x. The SP1 host
//! needs only the proof I/O types, so they were extracted here against
//! `alloy-primitives` directly. This lets `programs/state-transition-host`
//! depend on `mersennet-state-proof` + `mersennet-zkp` (both revm-free) and enable
//! `sp1-sdk/network` (which pulls `c-kzg` 2.x via Alloy 1.0) without the
//! native `ckzg` link collision.
//!
//! `mersennet` re-exports these modules so existing `mersennet::zk_proofs`
//! / `mersennet::zk_sp1` paths keep working unchanged.

pub mod zk_proofs;
pub mod zk_sp1;
