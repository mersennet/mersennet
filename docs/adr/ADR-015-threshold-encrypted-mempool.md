# ADR-015: Threshold-Encrypted Mempool

**Status:** Accepted (scaffold landed; production BLS pending Phase 1.3.x)
**Date:** 2026-05-21

## Context

A shielded transaction model (ADR-014) protects per-account state at
rest, but the network observer can still time-correlate intents
against fills, and a malicious validator can preview cleartext
intents before block-production. Both attacks let a sophisticated
operator reconstruct who is trading what.

## Decision

Every shielded intent is sealed under a per-epoch threshold ElGamal
key, decrypted in lockstep by `k`-of-`n` validators only at the FBA
batch boundary. Until that boundary, no party — including individual
validators — can read the plaintext.

The trait surface is in `crates/zkp/src/threshold.rs` and the chain
adapter is `crates/core/src/threshold_mempool.rs`. Two backends
implement the trait:

- `DummyThreshold` (default, in-tree, used in tests + devnet):
  XOR-share aggregation. Not secure on its own; faithful API
  placeholder.
- `BlsThreshold` (`crates/zkp/src/bls_threshold.rs`, gated behind
  the `prover` feature on `mersennet-zkp`): real BLS12-381 KEM/DEM with
  Pedersen DKG, k-of-n decryption shares via Lagrange interpolation,
  ChaCha20-Poly1305 symmetric core.

Validators run a Pedersen DKG ceremony once per epoch (target: 1
epoch = 1 hour = ~18 000 blocks at 200ms target). The ceremony is
embedded as an extra round in HotStuff-2 consensus.

## Consequences

**Positive.**
- Front-running by validators is impossible (they cannot read
  intents before the FBA boundary).
- Sandwich attacks by network observers are impossible (the only
  cleartext that crosses the mempool is the public preamble:
  market id, side hash, price band, size band, gas/fee).
- Wrong decryption shares are detectable (a recovered plaintext
  whose ChaCha20-Poly1305 tag does not verify is fatal evidence; the
  share's signer is slashable).

**Negative.**
- Per-block decryption ceremony adds 1-RTT to block production. We
  pipeline this with `t-1` block production to hide the latency.
- DKG faults can halt the chain. Mitigation: at-most-`f` faulty
  validators tolerated; emergency fallback to transparent mode behind
  `FEATURE_ENCRYPTED_MEMPOOL` requires governance vote.

## Phase 1.3.x work plan

1. Add `blstrs = "0.7"` and `group = "0.13"` under the `prover`
   feature on `mersennet-zkp`.
2. Implement Pedersen-DKG (`blstrs` G1 / G2).
3. Wire the DKG ceremony into HotStuff-2.
4. Slash on wrong-share evidence.
