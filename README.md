# Prime Chain

Prime Chain is a Rust blockchain prototype that combines an EVM execution engine with a PrimeOrders matching engine, domain events, and a lightweight P2P and RPC stack.

## Features

- EVM execution (revm) with block production and receipts
- PrimeOrders matching engine with margin checks and liquidation hooks
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
- Health endpoint: GET /health
- Metrics endpoint: GET /metrics

## Snapshot Sync (TCP)

- Listen mode: --snapshot-listen <addr>
- Fetch mode: --snapshot-fetch <addr>
- Optional: --snapshot-out <path>

## Project Layout

- src/core — engine, state, orders, consensus, bridge, events
- src/network — net transport + p2p
- src/rpc — JSON-RPC server + router
- src/config — app config and parsing helpers
- src/identity — node key management
- src/governance — governance logic
- src/prometheus — metrics exporter
- src/bin/prime-chain.rs — CLI entrypoint

## Docs

- docs/prime-orders-evm.md
- docs/whitepaper.md
