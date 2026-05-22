//! Prime Chain ZK primitives.
//!
//! This crate is the shared cryptographic surface between the chain
//! ([`prime-chain` core](../prime_chain/index.html)) and the Noir/SP1
//! proving stacks.
//!
//! Layout:
//!
//! - [`field`] — BN254 base-field arithmetic (subset used by the rest
//!   of the crate). Implemented as a `Fr` newtype around `[u64; 4]`
//!   little-endian limbs, with constant-time add/sub/mul/inv. Enough
//!   for hashing and Merkle trees; full curve operations are deferred
//!   to the proving backend.
//! - [`poseidon`] — Poseidon-2 permutation over `Fr`, 5-arity, sized
//!   for BN254. The same parameters used by Aztec / Noir, so circuit
//!   and Rust hash agree at the boundary.
//! - [`pedersen`] — Pedersen commitment over `Fr` (placeholder using
//!   Poseidon until the proving backend is wired; behind the
//!   `prover` feature this becomes a real EC commitment).
//! - [`merkle`] — Sparse Poseidon Merkle tree of fixed depth (32) used
//!   for the note commitment set.
//! - [`nullifier`] — Sparse nullifier set with bloom-filter fast-path
//!   for double-spend rejection.
//! - [`note`] — Note schema, commitment derivation, encrypted
//!   ciphertext envelope.
//! - [`threshold`] — BLS12-381 threshold ElGamal scaffolding for the
//!   encrypted mempool. Provides the trait + dummy in-memory provider
//!   used by tests; the real `blstrs`-backed implementation is gated
//!   behind the `prover` feature on `prime-chain` to avoid pulling
//!   pairing-friendly curve libs into every workspace build.
//! - [`noir`] — Noir circuit proof envelope + verifier trait. Mock
//!   prover used when `prover` is off; Barretenberg bindings when on.
//! - [`sp1`] — SP1 program input/output envelope shared with the
//!   `crates/core/src/zk_sp1.rs` runtime path.
//!
//! ## Invariants
//!
//! 1. Every public type that crosses the proof boundary uses
//!    canonical little-endian byte serialization. No `Display`/`Debug`
//!    representation is ever fed to a hash function.
//! 2. Proof verification never panics. Invalid inputs return
//!    `Err(VerifyError::...)`.
//! 3. Nullifier insertion is the *only* commit-point for spend
//!    finality. Insertion is atomic with the containing block.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod bls_threshold;
pub mod field;
pub mod merkle;
pub mod noir;
pub mod note;
pub mod nullifier;
pub mod pedersen;
pub mod poseidon;
pub mod sp1;
pub mod threshold;

pub use field::Fr;
pub use merkle::{MERKLE_DEPTH, MerkleProof, MerkleTree};
pub use noir::{Circuit, CircuitProof, Verifier, VerifyError};
pub use note::{EncryptedNote, Note, NoteCommitment, ViewingKey};
pub use nullifier::{Nullifier, NullifierSet};
pub use threshold::{
    DecryptionShare, EncryptedPayload, KeyShare, ThresholdCiphertext, ThresholdElGamal,
    ThresholdError,
};
