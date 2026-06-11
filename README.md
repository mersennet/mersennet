# Mersennet

Privacy-first L1 blockchain with a native on-chain order matching engine
(MersennetOrders), EVM compatibility, and an Aztec-style account-level
privacy layer, built in Rust.

| Chain | ID | Purpose | Status |
|---|---|---|---|
| **Public testnet** | `131071` (Mersenne prime 2^17 − 1, default chain ID) | Transparent EVM + CLOB | Live |
| **Privacy testnet** | `7920` | Shielded EVM + shielded CLOB + sealed-bid liquidations | **Ready to bring up — `feat/zk-privacy`** |
| **Mersennet mainnet** | `8191` (Mersenne prime 2^13 − 1) | Mainnet genesis ([`mainnet/genesis.json`](mainnet/genesis.json)) — MRSN token, 1B max supply, 10 MRSN/block initial reward | Pre-launch — see [`mainnet/launch-checklist.md`](mainnet/launch-checklist.md) |

> **Privacy redesign — testnet ready.** The privacy hard fork
> introduces shielded accounts, ZK-proved risk checks, sealed-bid
> liquidation auctions, threshold-encrypted mempool, and SP1 state
> proofs for light clients. Workstreams A–F (in-repo), G1–G3 (bridge
> contracts), H, and K are merged on `feat/zk-privacy`; the E5 Groth16
> verifying key, external audits (I), and governance activation (J)
> remain. Bring up your own privacy testnet with one command — see
> [`docs/runbooks/privacy-testnet-bootstrap.md`](docs/runbooks/privacy-testnet-bootstrap.md).
>
> **New here?** Start with [`docs/DEVELOPER_GUIDE.md`](docs/DEVELOPER_GUIDE.md)
> — it points you at the right workstream depending on what you want to ship.

## Features

### Transparent chain (live on testnet 131071)

- EVM execution (revm) with block production and receipts
- MersennetOrders CLOB matching engine with margin checks and liquidation hooks
- Snapshot export/import with TCP chunked sync + hash verification
- Structured JSON-RPC + WebSocket subscriptions, metrics, health endpoint
- Node identity + peer-store persistence
- HotStuff-2 consensus with slashing and unbonding

### Privacy testnet (7920, `feat/zk-privacy`)

- **Shielded note commitment tree** (Poseidon-2 BN254) + global nullifier
  set + 64-block recent-roots ring for client-side proving
- **Threshold-encrypted mempool** (BLS12-381 KEM/DEM + ChaCha20-Poly1305)
- **5-of-7 distributed key generation** coordinator (Pedersen-VSS,
  epoch-rotated)
- **Frequent batch auctions** per shielded market — uniform-price
  clearing, MEV-resistant
- **Sealed-bid liquidation auctions** — bonded liquidators only,
  victim identity never revealed
- **Shielded EVM bridge** — `0x7E` tx type for shielded transfers /
  shield / unshield / shielded orders
- **SP1 state-transition proofs** attached to every block
  (deterministic mock by default; real SP1 path via the
  `scripts/zk/` adapters with a pinned verifying key — workstream E)
- **Migration tool** — deterministically mirrors every transparent EOA
  into one shielded note at the activation height
- **Versioned snapshot envelopes** (`PZS1`) with chain-id check and
  forward-compatible shielded extension
- **CI privacy invariants** — grep guards enforce that no
  address-keyed state, address fields, or address-labeled metrics
  leak into shielded code paths

## Quick Start

### Build

```bash
cargo build --workspace                                   # default features
cargo build --workspace --features prover                 # real BN254/BLS crypto
```

### Run (devnet demo)

```bash
cargo run --bin mersennet
```

### Run with config + RPC

```bash
cargo run --bin mersennet -- --config path/to/config.json --rpc
```

### Tests

```bash
cargo test --workspace                                    # full suite, 241 tests (~5 min)
cargo test -p mersennet-zkp --features prover                 # cryptographic tests
bash scripts/ci/check-privacy-invariants.sh               # CI K2 privacy grep
```

### Bring up the privacy testnet (chain 7920)

```bash
cd testnet
./scripts/bootstrap-privacy-genesis.sh                    # keys + genesis
docker compose -f docker-compose.privacy.yml up -d --build
```

Then visit:

- Grafana — http://localhost:3001 (privacy dashboard auto-loaded)
- RPC — http://localhost:8545
- Faucet — http://localhost:8081

Full runbook: [`docs/runbooks/privacy-testnet-bootstrap.md`](docs/runbooks/privacy-testnet-bootstrap.md).

## RPC

- JSON-RPC served at the configured address (default: 127.0.0.1:8545)
- Health endpoint: `GET /health`
- Metrics endpoint: `GET /metrics`

## Snapshot Sync (TCP)

- Listen mode: `--snapshot-listen <addr>`
- Fetch mode: `--snapshot-fetch <addr>`
- Optional: `--snapshot-out <path>`

## Project Layout

```
crates/                         — Rust workspace (6 crates)
├── core/                       — Engine, state, consensus, crypto (package `mersennet`)
│   └── src/
│       ├── engine.rs                 — block production
│       ├── engine_snapshot.rs        — PZS1 versioned snapshot envelope
│       ├── shielded_state.rs         — note tree + nullifier set
│       ├── shielded_evm.rs           — shielded balances + 0x7E tx type
│       ├── shielded_orders.rs        — shielded CLOB + FBA
│       ├── liquidation_auction.rs    — sealed-bid auction
│       ├── threshold_mempool.rs      — encrypted intent queue
│       ├── dkg.rs                    — Pedersen-DKG coordinator
│       ├── shielded_persistence.rs   — redb-backed shielded storage
│       └── state_proof.rs            — SP1 state-transition proof glue
├── network/                    — P2P transport layer (`mersennet-network`)
├── rpc/                        — JSON-RPC + WebSocket server (`mersennet-rpc`)
│   ├── src/rpc_shielded.rs           — mersennet_submit*/mersennet_get* shielded methods
│   └── src/ws.rs                     — newShieldedRoot, newClearingPrice…
├── node/                       — CLI entrypoints (`mersennet-node`)
│   └── src/bin/
│       ├── mersennet.rs            — main node binary
│       ├── genesis.rs                — generates validator keys + configs
│       ├── migrate_genesis.rs        — transparent → privacy migration (`migrate-genesis`)
│       ├── faucet.rs                 — testnet faucet
│       └── loadtest.rs / stresstest.rs
├── state-proof/                — proof envelope types shared with the SP1 host (`mersennet-state-proof`)
└── zkp/                        — ZK primitives (`mersennet-zkp`)
    ├── src/poseidon.rs               — Poseidon-2 BN254 (Aztec-pinned)
    ├── src/pedersen.rs               — BN254 Pedersen commitment
    ├── src/bls_threshold.rs          — BLS12-381 threshold ElGamal
    ├── src/threshold.rs              — generic ThresholdElGamal trait
    └── params/poseidon-bn254.bin     — pinned hash parameters

programs/                       — SP1 RISC-V zkVM (built outside the workspace)
├── state-transition/                 — state-transition program
└── state-transition-host/            — host runner (mock / real-SP1 / network)

deploy/                         — Hetzner VPS testnet deployment (systemd + scripts)
testnet/                        — Dockerized testnets
├── configs/privacy/                  — 7-validator 5-of-7 configs
├── docker-compose.testnet.yml        — transparent testnet stack (131071)
├── docker-compose.privacy.yml        — privacy testnet stack (7920)
└── scripts/                          — bootstrap, load, chaos
mainnet/                        — Mainnet genesis (8191), compose stack, launch checklist
monitoring/                     — Prometheus + Grafana dashboards
scripts/ci/, scripts/zk/        — CI helpers (privacy-grep) + ZK prover adapters
```

## Branch policy

Trunk-based with one long-lived integration branch (`feat/zk-privacy`) for
the privacy redesign. See [CONTRIBUTING.md](CONTRIBUTING.md). Retired
branches are preserved as `archive/*` tags and can be recovered any time.

## Ecosystem Repos

This repository contains **only the blockchain**: the Rust node, the SP1
programs, and deployment/monitoring for running networks. Everything else
lives in its own repository under [github.com/mersennet](https://github.com/mersennet):

| Repo | Contents |
|------|----------|
| [docs](https://github.com/mersennet/docs) | docs.mersennet.com (Astro Starlight) + internal engineering docs, ADRs, runbooks, whitepaper |
| [website](https://github.com/mersennet/website) | mersennet.com landing site (Next.js) |
| [trade](https://github.com/mersennet/trade) | Mersennet Trade — perpetuals terminal on the native order book |
| [explorer](https://github.com/mersennet/explorer) | Block explorer |
| [validator-dashboard](https://github.com/mersennet/validator-dashboard) | Validator monitoring & analytics |
| [faucet](https://github.com/mersennet/faucet) | Testnet MRSN faucet |
| [contracts](https://github.com/mersennet/contracts) | Solidity contracts — Groth16 bridge, WMRSN, MersennetOrders examples (Foundry) |
| [sdk-ts](https://github.com/mersennet/sdk-ts) | TypeScript SDK (`@mersennet/sdk`) |
| [sdk-go](https://github.com/mersennet/sdk-go) | Go SDK |
| [sdk-python](https://github.com/mersennet/sdk-python) | Python SDK (`mersennet-sdk`) |
| [brand](https://github.com/mersennet/brand) | Logo masters, media kit, brand guidelines |

## Testnet

- **RPC:** `http://46.225.30.187:8545` (rpc.mersennet.com at launch)
- **Explorer:** `http://46.225.30.187:4000`
- **Mersennet Trade:** `http://46.225.30.187:4004`
- **MersennetSwap:** `http://46.225.30.187:4002`
- **Faucet:** `http://46.225.30.187:4005`
- **Docs:** `http://46.225.30.187:3001`

## Documentation

### For contributors

- **[Developer Guide](docs/DEVELOPER_GUIDE.md)** — one-stop entry: how
  to build, where the code lives, which workstream is which.
- **[Workstream Status](docs/STATUS.md)** — current progress on every
  numbered workstream (A–K).
- **[Contributing Guide](CONTRIBUTING.md)** — branching, commits, CI
  gates, privacy ground rules.
- **[Shielded JSON-RPC + WS reference](docs/shielded-rpc.md)** — every
  shielded RPC method and WS subscription, with payload shapes and
  error codes.
- **[ADR index](docs/adr/)** — Architecture Decision Records.

### For operators

- **[Privacy testnet bootstrap](docs/runbooks/privacy-testnet-bootstrap.md)**
  — bring up chain 7920 from a fresh host.
- **[ZK fork activation runbook](docs/runbooks/zk-fork-activation.md)** —
  mainnet hard-fork checklist (T-8w through T+24h).
- **[Public testnet](testnet/README.md)** — transparent chain 131071 + privacy chain 7920.

### Architecture + design

- **[Architecture overview](docs/ARCHITECTURE.md)**
- **[Whitepaper](docs/whitepaper.md)**
- **[Architecture Decision Records](docs/adr/)** — ADR-014 (shielded
  notes), ADR-015 (threshold mempool), ADR-016 (liquidation
  auctions), ADR-017 (SP1 state proofs), ADR-018 (privacy hard fork),
  ADR-019 (selective disclosure / viewing keys).

### Security

- **[Cryptography specification](docs/security/cryptography-spec.md)** —
  formal protocol document for the auditor.
- **[Privacy invariants](docs/security/privacy-invariants.md)** —
  CI-enforced rules (no `HashMap<Address>` in shielded modules, no
  address fields in events, no address-labeled metrics).
- **[Security policy](docs/SECURITY.md)** — vulnerability disclosure.

### Ecosystem + business

- [Ecosystem Roadmap](docs/ECOSYSTEM_ROADMAP.md)
- [Infrastructure Roadmap](docs/INFRASTRUCTURE_ROADMAP.md)
- [Team](docs/TEAM.md)
- [Fundraising Materials](docs/fundraising/)

## License

Proprietary — MersennetNumbers Labs
