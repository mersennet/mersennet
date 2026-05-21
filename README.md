# Prime Chain

Privacy-first L1 blockchain with a native on-chain order matching engine
(PrimeOrders) and EVM compatibility, built in Rust.

**Chain ID:** 7919 (1000th prime number)

> **In progress: ZK privacy redesign.** The chain is being upgraded from a
> transparent EVM+CLOB L1 to a privacy-first L1 with shielded accounts,
> ZK-proved risk checks, sealed-bid liquidation auctions, and SP1 state
> proofs for light clients. See
> [docs/internal/zk-privacy-plan.md](docs/internal/zk-privacy-plan.md) and
> [CONTRIBUTING.md](CONTRIBUTING.md). Active integration branch:
> `feat/zk-privacy`.

## Features

- EVM execution (revm) with block production and receipts
- PrimeOrders CLOB matching engine with margin checks and liquidation hooks
- Threshold-encrypted mempool + commit-reveal + frequent batch auctions
  (Phase 1 work to wire real cryptography is in progress)
- ZK state-transition proofs scaffolded for SP1; mock prover today, real
  prover in Phase 5
- Domain events indexed per block and queryable via RPC
- Snapshot export/import with TCP chunked sync + hash verification
- Structured RPC errors, metrics, and health endpoint
- Node identity persistence and peer store persistence

## Quick Start

### Build

```bash
cargo build
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
cargo test
```

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
├── core/       — Engine, state, orders, consensus, crypto, shielded state
├── network/    — P2P transport layer
├── rpc/        — JSON-RPC + WebSocket server
├── node/       — CLI entrypoint and faucet
└── zkp/        — ZK primitives: Noir circuits, prover, verifier, params
                  (added in Phase 1 of the privacy redesign)

contracts/      — Solidity contracts (Foundry)
├── src/dex/         — PrimeSwap factory, pair, router
├── src/foundation/  — WPRIM, MockERC20, Multicall3
├── src/primeorders/ — AtomicArbitrage, SmartContractMM, VaultStrategy
└── src/interfaces/  — IPrimeOrders

deploy/         — Testnet deployment configs
docs/           — Whitepaper, ADRs, fundraising, roadmaps
docs/internal/  — Internal plans (ZK privacy plan, etc.)
docs-site/      — Docusaurus documentation (docs.primechain.xyz)
sdk/            — TypeScript SDK
sdk-go/         — Go SDK
sdk-python/     — Python SDK
testnet/        — Testnet configuration
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

- [Whitepaper](docs/whitepaper.md)
- [Ecosystem Roadmap](docs/ECOSYSTEM_ROADMAP.md)
- [Infrastructure Roadmap](docs/INFRASTRUCTURE_ROADMAP.md)
- [Team](docs/TEAM.md)
- [Fundraising Materials](docs/fundraising/)

## License

Proprietary — PrimeNumbers Labs
