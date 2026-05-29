# Prime Chain Security Audit Preparation

## Version: 7.0
## Date: March 2026

## Audit Scope

### Critical Path (Must Audit)

1. **EVM Execution Engine** (`crates/core/src/engine.rs`) — 1,520 lines
   - State transition correctness
   - Gas accounting
   - Parallel execution safety (Block-STM)
   - Chain ID validation, nonce handling, signature verification

2. **CLOB Matching Engine** (`crates/core/src/prime_orders.rs`) — 800 lines
   - Price-time priority enforcement
   - Conservation of value
   - Margin calculation correctness
   - Position management, liquidation, ADL (Auto-Deleveraging)

3. **Consensus** (`crates/core/src/hotstuff2.rs`) — 760 lines
   - BFT safety (no two conflicting commits)
   - Liveness (progress under partial synchrony)
   - Slashing correctness
   - Quorum certificate validation, timeout handling

4. **State Persistence** (`crates/core/src/state.rs` — 1,039 lines, `state_redb.rs` — 814 lines)
   - ACID compliance
   - Merkle tree correctness (`MerkleTree::compute_root`, `compute_proof`)
   - Crash recovery
   - Multi-domain state (EVM, PrimeOrders, Bridge) snapshot/restore

### High Priority

5. **CLOB Precompile** (`crates/core/src/precompiles.rs` — 291 lines, `precompile_abi.rs`)
   - ABI encoding correctness
   - Gas metering
   - Context injection (`PRIME_ORDERS_CTX`) and lock safety

6. **FBA Auctions** (`crates/core/src/fba.rs` — 391 lines)
   - Uniform price correctness
   - MEV resistance
   - Pro-rata allocation when oversubscribed

7. **Commit-Reveal** (`crates/core/src/commit_reveal.rs` — 129 lines)
   - Binding property (revealed tx matches committed hash)
   - Timing attacks (commit window expiry)
   - Duplicate commitment / already-revealed handling

8. **Cross-Chain Bridge** (`crates/core/src/cross_chain.rs` — 501 lines, `bridge.rs` — 84 lines)
   - Value conservation
   - Proof verification
   - Deposit/withdrawal status transitions

9. **Account Abstraction** (`crates/core/src/account_abstraction.rs` — 407 lines)
   - EntryPoint validation
   - Nonce handling
   - UserOperation verification

### Medium Priority

10. **P2P Networking** (`crates/network/`, `crates/core/src/network.rs` — 68 lines)
    - Noise encryption
    - Peer authentication

11. **RPC Server** (`crates/rpc/src/rpc.rs` — 999 lines, `rpc_router.rs` — 808 lines)
    - Input validation
    - DoS protection
    - WebSocket handling

12. **Mempool** (`crates/core/src/mempool.rs` — 512 lines)
    - Ordering fairness
    - Spam resistance
    - Per-sender queue limits

13. **Encrypted Mempool** (`crates/core/src/encrypted_mempool.rs` — 282 lines)
    - Threshold security
    - Key rotation

14. **Privacy state-proof path** (`crates/core/src/state_proof.rs`, `crates/core/src/zk_sp1.rs`, `programs/state-transition/`, `programs/state-transition-host/`)
   - Public-values contract matches the documented `BlockProgramOutput`
   - Host/program/request wiring preserves `prev/new` nullifier roots and market-state hash
   - Vkey pin procedure and release artifact provenance
   - Real-SP1 cut-over boundary versus deterministic mock path

## Known Risks

| Risk | Location | Description |
|------|----------|-------------|
| `unwrap()` / `expect()` usage | engine.rs (32), prime_orders.rs (14), state.rs (6), precompiles.rs (7), hotstuff2.rs (7), consensus.rs (9), parallel.rs (6), fba.rs (8), state_redb.rs (4) | Potential panic on unexpected states; should be replaced with proper error handling |
| Lock poisoning | precompiles.rs, engine.rs | `Mutex` lock poisoning can propagate; consider `Mutex::get_mut` or poisoning recovery |
| Bridge queue overflow | bridge.rs | `set_max_len` drops oldest messages when limit exceeded; no explicit value conservation check |
| Commit-reveal window | commit_reveal.rs | Fixed 2-block window; timing-sensitive for MEV; expiry may be exploitable |
| Merkle tree odd-length leaves | state.rs | Odd number of leaves handled by promoting last leaf; verify proof correctness |
| FBA price discovery | fba.rs | Clearing price maximization; edge cases when buy/sell volumes are asymmetric |
| Parallel execution conflicts | parallel.rs, engine.rs | Block-STM validation; ensure no lost updates or incorrect rollbacks |
| Cross-chain proof verification | cross_chain.rs | Proof validation logic; ensure no forged proofs accepted |
| SP1 proving path not engine-parity complete yet | state_proof.rs, zk_sp1.rs, programs/state-transition/, programs/state-transition-host/ | The zkVM path now consumes canonical witness-bearing `BlockProgramInput` and replays the shielded tx + tick path, including order admission, liquidation settle, and `shielded_event_root`. The remaining cut-over boundary is final header/public-output hardening, replacing mirrored replay code with extracted engine-parity transition core, release-grade prove/verify transcript capture, and network proving integration. |

## Test Coverage Summary

| Component | Test Files | Test Count (approx) |
|-----------|------------|---------------------|
| Core integration | `integration_tests.rs`, `integration_block.rs` | 11+ |
| Prime orders | `prime_orders_integration.rs`, `prime_orders_advanced.rs` | 13+ |
| Consensus | `consensus_tests.rs`, `consensus_sim.rs` | 10+ |
| State | `state_tests.rs` | 7+ |
| Bridge | `bridge_tests.rs` | 5+ |
| Crypto | `crypto_tests.rs` | 4+ |
| Mempool fuzz | `fuzz_mempool.rs` | 1 |
| RPC | `rpc_tests.rs`, `ws_tests.rs` | 7+ |
| Network | `noise_tests.rs` | 2+ |

**Total:** ~60+ unit/integration tests across `crates/core/tests/`, `crates/rpc/tests/`, `crates/network/tests/`.

**Formal verification:** `crates/core/src/formal_verification.rs` provides invariant checking and property-based test generators.

## Privacy-Fork Audit Package

Before scheduling external privacy-fork audits, attach the following artifacts to the engagement packet:

- Working packet document: `docs/security/privacy-fork-audit-packet.md`

- Exact SP1 proving artifact set: program ELF provenance, pinned `PRIME_SP1_VKEY_HASH`, and one prove/verify transcript.
- Output-contract references: `crates/zkp/src/sp1.rs`, `crates/core/src/state_proof.rs`, `crates/core/src/zk_sp1.rs`, `programs/state-transition/src/main.rs`, `programs/state-transition-host/src/main.rs`.
- Current limitation note: the repo now re-executes canonical witness-bearing `BlockProgramInput` and enforces the public-output boundary, but it still needs final header/public-output hardening, extraction of shared engine-parity transition logic, release-grade prove/verify transcript capture, and network prover integration before the SP1 path should be treated as release-complete.
- CI evidence for the host/program path: node `--features prover,sp1`, standalone program check, standalone host test.

## Static Analysis

### Clippy

```bash
cargo clippy --workspace --offline
```

**Result:** 105 warnings (as of March 2026)

Representative categories:
- `mixed_case_hex_literals` (account_abstraction.rs)
- `manual_is_multiple_of` (formal_verification.rs)
- `needless_borrow` (crypto/mod.rs)
- `needless_range_loop` (encrypted_mempool.rs)
- `collapsible_if` (consensus.rs, bridge.rs, engine.rs)
- `derivable_impls` (config.rs)
- `too_many_arguments` (engine.rs)
- `map_or` simplification (mempool.rs)

Recommendation: Address Critical/High-priority audit items first; resolve clippy warnings in batches.

### Cargo Audit

`cargo-audit` is not installed by default. To check for known vulnerabilities:

```bash
cargo install cargo-audit
cargo audit
```

## Formal Verification Module

The `formal_verification` module (`crates/core/src/formal_verification.rs`) provides:

- **InvariantChecker:** Verifies conservation of value, price-time priority, order book consistency, nonce monotonicity, FBA uniform price
- **PropertyTestGenerator:** Generates random and adversarial orders/transfers for property-based testing

Use `BlockReport` to feed block-level data into `InvariantChecker::check_all()` for continuous invariant validation during testing or audit runs.
