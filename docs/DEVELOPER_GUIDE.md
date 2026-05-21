# Prime Chain — Developer Guide

**Audience:** Engineers landing on the Prime Chain repo for the first
time, or returning after a break.

This guide is the single entry point. It tells you:

1. What's in the repo and how it's laid out.
2. Where the active work is (`feat/zk-privacy` branch — privacy testnet).
3. How to build, test, lint, and run.
4. Which workstream owns which file.
5. Where to file PRs and what CI gates them.

If you only read one document, read this one. Everything else is a
deep dive linked from here.

---

## 1. What is Prime Chain?

A Rust-built EVM-compatible L1 with a native CLOB (PrimeOrders) that
is being upgraded from a transparent chain (chain ID 7919) to a
privacy-first chain (chain ID 7920) via a hard fork. The privacy fork:

- Moves trader-specific state (balances, positions, orders) behind a
  Poseidon-2 BN254 note commitment tree + nullifier set, à la Aztec.
- Replaces the public mempool with a BLS12-381 threshold-encrypted
  mempool to kill MEV ordering games.
- Runs frequent batch auctions per shielded market.
- Replaces public liquidation searches with sealed-bid auctions for
  bonded liquidators (victim identity stays private).
- Verifies block state transitions via SP1 succinct proofs for light
  clients and the Ethereum bridge.

**Why:** after October 10, 2025, large traders are unwilling to use
chains where their positions and proximity to liquidation are
publicly readable, because those signals get used to engineer
cascading liquidations. Prime Chain's privacy fork makes those
signals unobservable.

---

## 2. Repository layout

### Top level

```
crates/         — Rust workspace (the chain itself)
contracts/      — Solidity (Foundry)
sdk/            — TypeScript SDK
sdk-go/         — Go SDK
sdk-python/     — Python SDK
testnet/        — Testnet bring-up (compose files, configs, scripts)
deploy/         — Production deployment configs + monitoring
docs/           — All written design + runbook material
docs-site/      — Docusaurus public docs site
scripts/        — CI helpers, dev tooling
programs/       — SP1 program (state-transition proof body)
```

### Rust workspace at a glance

| Crate | Purpose | Key entry points |
|---|---|---|
| `prime-chain` (`crates/core`) | Engine, state, consensus, shielded subsystems | [`engine.rs`](../crates/core/src/engine.rs) |
| `prime-chain-network` (`crates/network`) | P2P transport | `p2p.rs` |
| `prime-chain-rpc` (`crates/rpc`) | JSON-RPC + WebSocket | [`rpc.rs`](../crates/rpc/src/rpc.rs), [`rpc_shielded.rs`](../crates/rpc/src/rpc_shielded.rs), [`ws.rs`](../crates/rpc/src/ws.rs) |
| `prime-chain-node` (`crates/node`) | Binaries — `prime-chain`, `genesis`, `migrate-genesis`, `faucet`, `loadtest`, `stresstest` | `src/bin/*.rs` |
| `prime-zkp` (`crates/zkp`) | ZK primitives — Poseidon, Pedersen, BLS threshold, merkle, nullifier, noir/sp1 stubs | `src/lib.rs` |

### Shielded subsystem map (where the privacy work lives)

| File | What it owns |
|---|---|
| `crates/core/src/shielded_state.rs` | Global note commitment tree + nullifier set + recent-roots ring |
| `crates/core/src/shielded_evm.rs` | Shielded EOA balances + transparent⇄shielded bridge + 0x7E tx envelope |
| `crates/core/src/shielded_orders.rs` | Shielded CLOB book + frequent batch auctions per market |
| `crates/core/src/liquidation_auction.rs` | Bonded liquidator registry + sealed-bid auctions |
| `crates/core/src/threshold_mempool.rs` | Encrypted intent queue, decryption-share collector |
| `crates/core/src/dkg.rs` | Pedersen-DKG epoch coordinator (5-of-7 on testnet) |
| `crates/core/src/shielded_persistence.rs` | redb-backed disk persistence for shielded subsystems |
| `crates/core/src/engine_snapshot.rs` | Versioned `PZS1` snapshot envelope with shielded extension |
| `crates/core/src/state_proof.rs` | SP1 state-transition proof glue |
| `crates/zkp/src/poseidon.rs` | Poseidon-2 BN254 (Aztec-pinned, params in `crates/zkp/params/`) |
| `crates/zkp/src/pedersen.rs` | BN254 Pedersen commitment + generators |
| `crates/zkp/src/bls_threshold.rs` | BLS12-381 threshold ElGamal (real impl behind `prover` feature) |
| `crates/zkp/src/threshold.rs` | Generic `ThresholdElGamal` trait + dummy impl |
| `crates/zkp/src/noir.rs` | Noir prover/verifier interface (mock today; nargo integration tracked in D5/D6) |
| `crates/zkp/src/sp1.rs` | SP1 state-transition proof I/O types |

### Where the testnet stack lives

```
testnet/
├── configs/
│   ├── validator-{1..3}.json              — transparent testnet (chain 7919)
│   └── privacy/
│       ├── validator-{1..7}.json          — privacy testnet (chain 7920, 5-of-7)
│       └── rpc-node.json
├── docker-compose.testnet.yml             — transparent stack
├── docker-compose.privacy.yml             — privacy stack
├── prometheus.privacy.yml
└── scripts/
    ├── bootstrap-privacy-genesis.sh       — one-command genesis
    ├── privacy-load.sh                    — synthetic load generator
    └── chaos-kill-validator.sh            — kill-validator chaos drill
```

---

## 3. Branches

| Branch | Purpose |
|---|---|
| `main` | Production trunk for transparent chain (7919) |
| `feat/zk-privacy` | **Where the privacy testnet work lives.** All current development happens here. |
| `feat/*`, `fix/*`, `docs/*`, `chore/*` | Short-lived, one PR each |
| `archive/*` (tags) | Read-only snapshots of retired work |

When you clone, you almost certainly want:

```bash
git checkout feat/zk-privacy
```

The session-executable subset of the privacy work (workstreams
A, B, C, D1–D4, D7, H, K1–K2) is merged. See
[`STATUS.md`](STATUS.md) for what's left.

---

## 4. Build, test, run

### Prerequisites

- Rust 1.79+ (stable). Install with [rustup](https://rustup.rs/).
- Docker 24+ + Docker Compose v2 (for the testnet).
- Foundry (for Solidity bridge work — workstream G).
- Optional, for the real ZK pipeline (workstreams D5/D6/E):
  - `nargo` (Noir 0.30+)
  - `sp1up` + the SP1 toolchain
  - `barretenberg-sys` dependencies (libc++, cmake)

### Build

```bash
cargo build --workspace                    # default features, ~3 min cold
cargo build --workspace --release          # ~7 min cold
cargo build --workspace --features prover  # real BN254 + BLS crypto
```

### Test

```bash
cargo test --workspace --lib --tests       # 286+ tests, ~5 min
cargo test -p prime-zkp --features prover  # 43 cryptographic tests
cargo test -p prime-chain --test privacy_migration_e2e  # migration E2E
```

### Lint + format

```bash
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/ci/check-privacy-invariants.sh
```

### Run a single-node devnet

```bash
cargo run --bin prime-chain -- --rpc
# JSON-RPC at http://127.0.0.1:8545
# Metrics at http://127.0.0.1:8545/metrics
```

### Bring up a multi-validator privacy testnet

```bash
cd testnet
./scripts/bootstrap-privacy-genesis.sh
docker compose -f docker-compose.privacy.yml up -d --build
```

Then:

- Grafana dashboard: http://localhost:3001 (admin / `${GRAFANA_ADMIN_PASSWORD:-changeme}`)
- RPC observer: http://localhost:8545
- Faucet: http://localhost:8081

Tear down with `docker compose -f docker-compose.privacy.yml down -v`.

---

## 5. RPC + WebSocket — what's available

### Transparent RPC (works on both 7919 and 7920)

Standard Ethereum-style methods plus `prime_*` extensions:

| Method | Purpose |
|---|---|
| `eth_blockNumber`, `eth_getBlockByNumber`, `eth_call`, `eth_sendRawTransaction`, … | Standard EVM |
| `prime_getChainConfig` | Chain ID + activation heights + feature flags |
| `prime_getMarkets`, `prime_getOrderBook`, `prime_submitOrder` | PrimeOrders CLOB |
| `prime_getBridgeQueue` | Bridge state |

### Shielded RPC (gated on privacy activation — chain 7920)

All payloads are opaque `bincode-then-0x-hex` blobs. Wallets build
them locally with the SDK.

| Method | Purpose |
|---|---|
| `prime_submitShieldedTransfer` | Shielded P2P transfer |
| `prime_submitShield`, `prime_submitUnshield` | Transparent ⇄ shielded bridge |
| `prime_submitShieldedOrder` | Shielded order book intent |
| `prime_submitLiquidationClaim`, `prime_submitLiquidationExecute` | Sealed-bid auction flow |
| `prime_registerLiquidator` | Bond a liquidator |
| `prime_getShieldedRoot` | Current note tree root |
| `prime_getShieldedBalance` | Wallet-side balance lookup (viewing-key auth) |
| `prime_getShieldedMarketAggregates` | Market-level public stats |
| `prime_getStateProof(blockNumber?)` | SP1 state proof for a specific block (or latest) |
| `prime_getLatestStateProof` | Convenience alias for latest |
| `prime_verifyStateProof` | Verify a serialized state proof |

Full reference: [`shielded-rpc.md`](shielded-rpc.md).

### WebSocket subscriptions

Standard `eth_subscribe`:

- `newHeads`, `newPendingTransactions`, `logs`

Prime-specific (`prime_subscribe`):

- `PrimeOrdersTrades(marketId?)`, `PrimeOrdersBook(marketId)`, `BatchAuctionResults(marketId?)`

Privacy-mode subscriptions (added in C3):

- `newShieldedRoot` — fires every time the note tree advances
- `newClearingPrice(marketId?)` — fires after each FBA tick
- `newAuctionSettled(marketId?)` — fires when a sealed-bid auction settles
- `newStateProof` — fires when a fresh SP1 proof is attached

All shielded WS payloads are address-free by construction; CI K2
enforces this at the source.

---

## 6. CI gates

Every PR runs the following GitHub Actions jobs (see
`.github/workflows/ci.yml`):

| Job | Purpose |
|---|---|
| `cargo check (default)` | Fast type-check |
| `cargo test (default)` | Full test suite, default features |
| `cargo check (--features prover)` | Type-check the real-crypto lane |
| `cargo test (--features prover)` | Run the 43 cryptographic tests |
| `clippy` | `-D warnings` |
| `fmt` | `cargo fmt --check` |
| `sdk_typecheck` | `tsc --noEmit` over `sdk/` |
| `contracts` | `forge build --sizes && forge test -vvv` |
| `audit` | `cargo audit` |
| `privacy_invariants` | `scripts/ci/check-privacy-invariants.sh` |

If `privacy_invariants` fails, it almost certainly means an address
field or `HashMap<Address, ...>` slipped into a shielded code path.
The fix is either:

1. Move the state to a commitment-keyed map.
2. Or, if the boundary is legitimate (migration, bridge), add
   `// privacy-allow: <reason>` on the line.

See [`privacy-invariants.md`](security/privacy-invariants.md) for the
full rule set.

---

## 7. Where to start contributing

| If you want to … | Look at | Workstream |
|---|---|---|
| Add a new shielded RPC method | `crates/rpc/src/rpc_shielded.rs` | C |
| Add a new WS subscription | `crates/rpc/src/ws.rs` + node binary dispatch | C |
| Wire real Noir circuits | `crates/zkp/src/noir.rs` + `crates/zkp/circuits/` | D5, D6 |
| Plug in `sp1up` for real proofs | `crates/core/src/state_proof.rs` + `programs/state-transition/` | E |
| Build the wallet UI | `sdk/` (TS bindings) + external prime-trade repo | F |
| Write the Solidity bridge | `contracts/src/bridge/` (placeholder dir) | G |
| Improve testnet ops | `testnet/scripts/`, `deploy/monitoring/` | H |
| Tighten CI | `.github/workflows/ci.yml`, `scripts/ci/` | K |

For each workstream, the canonical reference for what's done and
what's left is [`STATUS.md`](STATUS.md).

---

## 8. Useful references

- **Architecture overview** — [`ARCHITECTURE.md`](ARCHITECTURE.md)
- **Whitepaper** — [`whitepaper.md`](whitepaper.md)
- **ADR index** — [`adr/`](adr/)
  - ADR-014: Shielded notes
  - ADR-015: Threshold-encrypted mempool
  - ADR-016: Liquidation auctions
  - ADR-017: SP1 state proofs
  - ADR-018: Privacy hard fork
- **Cryptography spec** (for the auditor) — [`security/cryptography-spec.md`](security/cryptography-spec.md)
- **Privacy invariants** — [`security/privacy-invariants.md`](security/privacy-invariants.md)
- **Privacy testnet runbook** — [`runbooks/privacy-testnet-bootstrap.md`](runbooks/privacy-testnet-bootstrap.md)
- **Mainnet activation runbook** — [`runbooks/zk-fork-activation.md`](runbooks/zk-fork-activation.md)
- **Workstream status tracker** — [`STATUS.md`](STATUS.md)
- **Contributing guide** — [`../CONTRIBUTING.md`](../CONTRIBUTING.md)
- **Security policy** — [`SECURITY.md`](SECURITY.md)

---

## 9. Getting help

- File a discussion on GitHub for design questions.
- File an issue with a minimal reproduction for bugs.
- Do **not** file public issues for security vulnerabilities — see
  [`SECURITY.md`](SECURITY.md) for the private disclosure channel.
