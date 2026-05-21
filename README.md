# Prime Chain

Privacy-first L1 blockchain with a native on-chain order matching engine
(PrimeOrders), EVM compatibility, and an Aztec-style account-level
privacy layer, built in Rust.

| Chain | ID | Purpose | Status |
|---|---|---|---|
| **Prime Chain mainnet** | `7919` (1000th prime) | Transparent EVM + CLOB | Live |
| **Privacy Testnet** | `7920` | Shielded EVM + shielded CLOB + sealed-bid liquidations | **Ready to bring up — `feat/zk-privacy`** |

> **Privacy redesign — testnet ready.** The privacy hard fork
> introduces shielded accounts, ZK-proved risk checks, sealed-bid
> liquidation auctions, threshold-encrypted mempool, and SP1 state
> proofs for light clients. All session-executable workstreams (A–D,
> H, K) are merged on `feat/zk-privacy`. Bring up your own privacy
> testnet with one command — see
> [`docs/runbooks/privacy-testnet-bootstrap.md`](docs/runbooks/privacy-testnet-bootstrap.md).
>
> **New here?** Start with [`docs/DEVELOPER_GUIDE.md`](docs/DEVELOPER_GUIDE.md)
> — it points you at the right workstream depending on what you want to ship.

## Features

### Transparent chain (live on 7919)

- EVM execution (revm) with block production and receipts
- PrimeOrders CLOB matching engine with margin checks and liquidation hooks
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
- **SP1 state-transition proofs** attached to every block (mock
  prover today; sp1up integration tracked in workstream E)
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
cargo run --bin prime-chain
```

### Run with config + RPC

```bash
cargo run --bin prime-chain -- --config path/to/config.json --rpc
```

### Tests

```bash
cargo test --workspace                                    # full suite (~5 min)
cargo test -p prime-zkp --features prover                 # cryptographic tests
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
crates/
├── core/                       — Engine, state, consensus, crypto
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
├── network/                    — P2P transport layer
├── rpc/                        — JSON-RPC + WebSocket server
│   ├── src/rpc_shielded.rs           — prime_submit*/prime_get* shielded methods
│   └── src/ws.rs                     — newShieldedRoot, newClearingPrice…
├── node/                       — CLI entrypoints
│   └── src/bin/
│       ├── prime-chain.rs            — main node binary
│       ├── genesis.rs                — generates validator keys + configs
│       ├── migrate_genesis.rs        — transparent → privacy migration
│       ├── faucet.rs                 — testnet faucet
│       └── loadtest.rs / stresstest.rs
└── zkp/                        — ZK primitives
    ├── src/poseidon.rs               — Poseidon-2 BN254 (Aztec-pinned)
    ├── src/pedersen.rs               — BN254 Pedersen commitment
    ├── src/bls_threshold.rs          — BLS12-381 threshold ElGamal
    ├── src/threshold.rs              — generic ThresholdElGamal trait
    └── params/poseidon-bn254.bin     — pinned hash parameters

contracts/                      — Solidity contracts (Foundry)
deploy/                         — Production deployment configs
testnet/                        — Testnet bring-up
├── configs/privacy/                  — 7-validator 5-of-7 configs
├── docker-compose.privacy.yml        — privacy testnet stack
└── scripts/                          — bootstrap, load, chaos

docs/
├── DEVELOPER_GUIDE.md          — START HERE for new contributors
├── STATUS.md                   — workstream progress tracker
├── ARCHITECTURE.md             — high-level design (transparent chain)
├── adr/                        — Architecture Decision Records (014–018)
├── security/
│   ├── cryptography-spec.md          — formal crypto spec for auditor
│   └── privacy-invariants.md         — CI-enforced rules
└── runbooks/
    ├── privacy-testnet-bootstrap.md  — bring up chain 7920
    └── zk-fork-activation.md         — mainnet hard-fork checklist

scripts/ci/                     — CI helpers (privacy-grep, etc)
sdk/, sdk-go/, sdk-python/      — TypeScript / Go / Python clients
docs-site/                      — Docusaurus (docs.primechain.xyz)
```

## Branch policy

Trunk-based with one long-lived integration branch (`feat/zk-privacy`) for
the privacy redesign. See [CONTRIBUTING.md](CONTRIBUTING.md). Retired
branches are preserved as `archive/*` tags and can be recovered any time.

## Ecosystem Repos

All ecosystem applications live in their own repositories:

| App | Repo | Description |
|-----|------|-------------|
| **PrimeTrade** | [prime-trade](https://github.com/PrimeNumbersLabs/prime-trade) | Professional CLOB trading terminal — perpetual futures with TradingView charts, 5 order types, TP/SL |
| **PrimeScan** | [primescan-explorer](https://github.com/PrimeNumbersLabs/primescan-explorer) | Block explorer — transactions, addresses, tokens, validators |
| **PrimeSwap V2** | [primeswap-v2](https://github.com/PrimeNumbersLabs/primeswap-v2) | Uniswap V2-style AMM DEX (React) |
| **PrimeSwap V3** | [primeswap-v3](https://github.com/PrimeNumbersLabs/primeswap-v3) | Concentrated liquidity DEX frontend |
| **PrimeSwap DEX** | [primeswap-dex](https://github.com/PrimeNumbersLabs/primeswap-dex) | Lightweight swap interface (vanilla JS) |
| **Validator Explorer** | [prime-chain-explorer](https://github.com/PrimeNumbersLabs/prime-chain-explorer) | Validator staking metrics and delegation UI |
| **Node Dashboard** | [primenodes-dashboard](https://github.com/PrimeNumbersLabs/primenodes-dashboard) | Validator monitoring and analytics |
| **Faucet** | [prime-faucet](https://github.com/PrimeNumbersLabs/prime-faucet) | Testnet PRIM token faucet |
| **Trading Bots** | [prime-bots](https://github.com/PrimeNumbersLabs/prime-bots) | Market maker, trader, and volume bots for CLOB testing |
| **SDK** | [prime-chain-sdk](https://github.com/PrimeNumbersLabs/prime-chain-sdk) | TypeScript SDK for JSON-RPC and PrimeOrders |

## Testnet

- **RPC:** `https://rpc.primechain.xyz` (or `http://46.225.30.187:8545`)
- **Explorer:** `http://46.225.30.187:4000`
- **PrimeTrade:** `http://46.225.30.187:4004`
- **PrimeSwap:** `http://46.225.30.187:4002`
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
- **[Public testnet](testnet/README.md)** — transparent chain 7919 + privacy chain 7920.

### Architecture + design

- **[Architecture overview](docs/ARCHITECTURE.md)**
- **[Whitepaper](docs/whitepaper.md)**
- **[Architecture Decision Records](docs/adr/)** — ADR-014 (shielded
  notes), ADR-015 (threshold mempool), ADR-016 (liquidation
  auctions), ADR-017 (SP1 state proofs), ADR-018 (privacy hard fork).

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

Proprietary — PrimeNumbers Labs
