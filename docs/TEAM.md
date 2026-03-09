# Prime Chain — Team Handbook

> Internal reference for the Prime Chain development team. Everything you need to build, deploy, and operate the Prime Chain ecosystem.

**Last Updated:** March 2026

---

## Table of Contents

1. [Quick Reference](#quick-reference)
2. [Architecture Overview](#architecture-overview)
3. [Repository Map](#repository-map)
4. [Live Services](#live-services)
5. [Deployed Contracts](#deployed-contracts)
6. [Local Development](#local-development)
7. [Deployment Guide](#deployment-guide)
8. [App-by-App Guide](#app-by-app-guide)
9. [Roadmaps](#roadmaps)
10. [Branding](#branding)
11. [Key Decisions Log](#key-decisions-log)

---

## Quick Reference

| Parameter | Value |
|-----------|-------|
| **Chain ID** | `7919` (`0x1EEF` hex) |
| **Currency** | PRIM (18 decimals) |
| **Block Time** | ~1 second |
| **Max Supply** | 1,000,000,000 PRIM |
| **Consensus** | DPoS (HotStuff-2 BFT design) |
| **EVM** | Shanghai spec via `revm` |
| **Native Precompile** | PrimeOrders CLOB at `0x0100` |
| **Validators** | 4 nodes on Hetzner VPS |
| **Primary Server** | `46.225.30.187` (public-facing) |
| **Secondary Server** | `46.225.183.192` (validator-2, RPC backend) |
| **GitHub Org** | [PrimeNumbersLabs](https://github.com/PrimeNumbersLabs) |

---

## Architecture Overview

```
                           ┌─────────────────────────────────────┐
                           │         PUBLIC ENDPOINTS             │
                           │     (46.225.30.187 via Nginx)        │
                           ├─────────────────────────────────────┤
                           │                                     │
     :80   Block Explorer  │   Nginx reverse proxy + rate limit  │
     :3001 Documentation   │                                     │
     :4000 PrimeSwap DEX   │   Static files: /var/www/           │
     :4001 Validator Dash  │     explorer/  dex/  validators/    │
     :8080 Faucet          │     docs/ (Docusaurus build)        │
     :8545 JSON-RPC        │                                     │
     :8546 WebSocket       │   Proxied to node :8545/:8546       │
     :3000 Grafana         │                                     │
     :3002 Uptime Kuma     │                                     │
     :9090 Prometheus      │                                     │
     :9093 Alertmanager    │                                     │
                           └───────────┬─────────────────────────┘
                                       │
                    ┌──────────────────┼──────────────────┐
                    │                  │                  │
              ┌─────▼─────┐    ┌──────▼──────┐    ┌──────▼──────┐
              │ Validator 1│    │ Validator 2 │    │Validator 3/4│
              │  :8545 RPC │    │  :8545 RPC  │    │  :8545 RPC  │
              │  :30303 P2P│    │  :30303 P2P │    │  :30303 P2P │
              └────────────┘    └─────────────┘    └─────────────┘
                  46.225.30.187   46.225.183.192     (additional)
```

### Core Rust Crates

| Crate | Path | Purpose |
|-------|------|---------|
| `prime-core` | `crates/core/` | Engine, consensus, EVM (revm), PrimeOrders, state, mempool |
| `prime-rpc` | `crates/rpc/` | JSON-RPC + WebSocket server, filter APIs |
| `prime-network` | `crates/network/` | P2P gossip, block sync, Noise encryption |
| `prime-node` | `crates/node/` | Node binary, genesis tool, faucet binary |

---

## Repository Map

### Main Monorepo: `prime-chain`

```
prime-chain/
├── crates/                    # Rust blockchain core
│   ├── core/                  #   Engine, EVM, consensus, PrimeOrders
│   ├── rpc/                   #   JSON-RPC + WebSocket
│   ├── network/               #   P2P networking
│   └── node/                  #   Binaries (prime-chain, faucet, genesis)
├── contracts/                 # Solidity smart contracts (Foundry)
│   ├── src/foundation/        #   Multicall3, WPRIM, MockERC20
│   ├── src/dex/               #   PrimeSwap (UniV2 fork)
│   ├── src/primeorders/       #   CLOB strategy examples
│   ├── src/interfaces/        #   IPrimeOrders.sol
│   ├── test/                  #   Foundry test suite
│   └── script/                #   Deploy + seed scripts
├── explorer/                  # PrimeScan block explorer (vanilla JS SPA)
├── dex/                       # PrimeSwap DEX frontend (vanilla JS SPA)
├── validator-explorer/        # PrimeNodes validator dashboard (vanilla JS SPA)
├── docs-site/                 # Documentation portal (Docusaurus)
├── docs/                      # Internal docs, roadmaps, whitepaper
│   ├── TEAM.md                #   ← You are here
│   ├── INFRASTRUCTURE_ROADMAP.md
│   ├── ECOSYSTEM_ROADMAP.md
│   ├── TECHNICAL_REFERENCE.md
│   ├── TOKENOMICS.md
│   ├── whitepaper.md
│   └── ...
├── deploy/                    # Deployment configs
│   ├── monitoring/            #   Prometheus, Grafana, Alertmanager, Uptime Kuma
│   └── ...
├── sdk/                       # TypeScript SDK (placeholder)
├── sdk-python/                # Python SDK
├── sdk-go/                    # Go SDK
├── .github/workflows/         # CI/CD pipelines
│   ├── ci.yml                 #   Rust check/test/clippy/fmt
│   ├── docker.yml             #   Docker image builds
│   ├── integration.yml        #   RPC conformance tests
│   └── release.yml            #   Release automation
├── Dockerfile                 # Node Docker image
├── Dockerfile.faucet          # Faucet Docker image
├── docker-compose.yml         # Local multi-node testnet
├── Cargo.toml                 # Workspace manifest
└── README.md                  # Public-facing README
```

### Related Repos (PrimeNumbersLabs org)

| Repo | Purpose | Status |
|------|---------|--------|
| [`prime-chain`](https://github.com/PrimeNumbersLabs/prime-chain) | Core blockchain + all testnet apps (monorepo) | **Active** — main development |
| [`primescan-explorer`](https://github.com/PrimeNumbersLabs/primescan-explorer) | Block explorer (standalone) | Synced from monorepo `explorer/` |
| [`primeswap-dex`](https://github.com/PrimeNumbersLabs/primeswap-dex) | DEX frontend (standalone) | Synced from monorepo `dex/` |
| [`primenodes-dashboard`](https://github.com/PrimeNumbersLabs/primenodes-dashboard) | Validator dashboard (standalone) | Synced from monorepo `validator-explorer/` |
| [`prime-chain-explorer`](https://github.com/PrimeNumbersLabs/prime-chain-explorer) | Block explorer (legacy standalone) | Synced from monorepo |
| [`prime-chain-sdk`](https://github.com/PrimeNumbersLabs/prime-chain-sdk) | TypeScript SDK | Synced from monorepo `sdk/` |
| `primefi-omni` | Cross-chain lending (LayerZero v2) | Built, not on Prime Chain yet |
| `primefi-contracts-v3` | Aave v3–style lending contracts | Built, not deployed |
| `primefi-ui-v3` | Lending frontend | Built |
| `primexdc-wallet` | Browser extension wallet | Built, needs chain ID update |
| `primexdc-mobile` | React Native mobile wallet | Built |
| `Primeport-v2` | NFT marketplace v2 | Built |
| `primeport-ui` | NFT marketplace frontend | Built |
| `primeport-server` | NFT backend (NestJS) | Built |
| `xdc-markets` | Prediction markets | Built |
| `xdc-masternode-app` | Validator management app | Built |
| `liquid-staking-contracts` | LST contracts | Built |
| `primestaking-ui-v2` | Staking frontend | Built |
| `prime-numbers-labs-logos` | Brand logos and assets | **Public** |

---

## Live Services

All services run on `46.225.30.187` unless noted.

| # | Service | Port | URL | Stack | Source |
|---|---------|------|-----|-------|--------|
| 1 | **JSON-RPC** | 8545 | http://46.225.30.187:8545 | Rust (prime-chain binary) | `crates/rpc/` |
| 2 | **WebSocket** | 8546 | ws://46.225.30.187:8546 | Rust (prime-chain binary) | `crates/node/` |
| 3 | **Block Explorer** | 80 | http://46.225.30.187/ | Vanilla JS SPA + Nginx | `explorer/` |
| 4 | **Faucet** | 8080 | http://46.225.30.187:8080 | Rust binary (embedded HTML) | `crates/node/src/bin/faucet.html` |
| 5 | **PrimeSwap DEX** | 4000 | http://46.225.30.187:4000 | Vanilla JS SPA + Nginx | `dex/` |
| 6 | **Validator Dashboard** | 4001 | http://46.225.30.187:4001 | Vanilla JS SPA + Nginx | `validator-explorer/` |
| 7 | **Documentation** | 3001 | http://46.225.30.187:3001 | Docusaurus (static build) | `docs-site/` |
| 8 | **Grafana** | 3000 | http://46.225.30.187:3000 | Docker container | `deploy/monitoring/` |
| 9 | **Prometheus** | 9090 | http://46.225.30.187:9090 | Docker container | `deploy/monitoring/` |
| 10 | **Alertmanager** | 9093 | http://46.225.30.187:9093 | Docker container | `deploy/monitoring/` |
| 11 | **Uptime Kuma** | 3002 | http://46.225.30.187:3002 | Docker container | `deploy/monitoring/` |

### Server Layout

```
/opt/prime-chain/
├── bin/
│   ├── prime-chain          # Node binary
│   └── faucet               # Faucet binary
├── keys/
│   ├── validator-key.json   # Validator signing key
│   └── faucet-key.json      # Faucet funding key
└── data/                    # Blockchain state

/var/www/
├── explorer/                # Block explorer static files
├── dex/                     # DEX static files
├── validators/              # Validator dashboard static files
└── docs/                    # Documentation build output
```

---

## Deployed Contracts

| Contract | Address | Purpose |
|----------|---------|---------|
| **Multicall3** | `0x973ee1bf0907287d1eb8a144d88b34f515c83f29` | Batch RPC calls |
| **WPRIM** | `0x079bf1207b51acda83e2e8178344f62a883f8479` | Wrapped PRIM (ERC-20) |
| **MockUSDC** | `0xb22f77d89122e9e3784bfd3eee9616273f38238d` | 6 decimals |
| **MockUSDT** | `0x877feca38919acd7aaf7cb81f100e0454aa95c17` | 6 decimals |
| **MockDAI** | `0xb88d63a65691effbf4b6808325b1588912c15cf4` | 18 decimals |
| **PrimeSwapFactory** | `0x63f7a64db6d2b965189b8b48b7435668021f6b17` | DEX pair factory |
| **PrimeSwapRouter** | `0x9f337f433e71ce969b991511f1dcd3d0622116bb` | DEX swap router |
| **PrimeOrders** | `0x0000000000000000000000000000000000000100` | Native CLOB precompile |

---

## Local Development

### Prerequisites

- Rust 1.75+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Node.js 18+ (for docs-site, contract scripts)
- Foundry (`curl -L https://foundry.paradigm.xyz | bash && foundryup`)

### Build the chain

```bash
cargo build --release
```

### Run a local 4-node testnet

```bash
# Generate genesis config
cargo run --bin genesis -- --validators 4 --output ./testnet

# Run validator 1 (in separate terminals for each)
cargo run --release --bin prime-chain -- --config testnet/validator-1.json
cargo run --release --bin prime-chain -- --config testnet/validator-2.json
cargo run --release --bin prime-chain -- --config testnet/validator-3.json
cargo run --release --bin prime-chain -- --config testnet/validator-4.json
```

Or use Docker Compose:
```bash
docker-compose up -d
```

### Run tests

```bash
# Rust unit + integration tests
cargo test

# Contract tests (Foundry)
cd contracts && forge test -vv

# Run benchmarks
cargo bench --bench tps_bench
cargo bench --bench parallel_bench
```

### Build & serve documentation locally

```bash
cd docs-site
npm install
npm run start   # Dev server at localhost:3000
npm run build   # Production build to build/
```

### Deploy contracts to local node

```bash
cd contracts
DEPLOYER_KEY=<private-key> node script/deploy.cjs
DEPLOYER_KEY=<private-key> node script/seed-liquidity.cjs
```

---

## Deployment Guide

### Deploy frontend apps

```bash
# Explorer
scp explorer/{index.html,style.css,app.js} root@46.225.30.187:/var/www/explorer/

# DEX
scp dex/{index.html,style.css,app.js} root@46.225.30.187:/var/www/dex/

# Validator Dashboard
scp validator-explorer/{index.html,style.css,app.js} root@46.225.30.187:/var/www/validators/

# Documentation
cd docs-site && npm run build
rsync -az --delete build/ root@46.225.30.187:/var/www/docs/
```

### Deploy node binary

```bash
cargo build --release
scp target/release/prime-chain root@46.225.30.187:/tmp/prime-chain-new

ssh root@46.225.30.187
  kill $(pgrep prime-chain)
  cp /tmp/prime-chain-new /opt/prime-chain/bin/prime-chain
  nohup /opt/prime-chain/bin/prime-chain --config /opt/prime-chain/config.json > /var/log/prime-chain.log 2>&1 &
```

### Deploy faucet

```bash
cargo build --release --bin faucet
scp target/release/faucet root@46.225.30.187:/tmp/faucet-new

ssh root@46.225.30.187
  kill $(pgrep faucet)
  cp /tmp/faucet-new /opt/prime-chain/bin/faucet
  nohup /opt/prime-chain/bin/faucet --port 8080 \
    --rpc-url http://46.225.183.192:8545 \
    --private-key /opt/prime-chain/keys/faucet-key.json \
    > /var/log/faucet.log 2>&1 &
```

### Monitoring stack

```bash
cd deploy/monitoring
docker-compose up -d
```

---

## App-by-App Guide

### 1. Block Explorer (`explorer/`)

- **Stack:** Vanilla JS SPA, no build step
- **Files:** `index.html`, `style.css`, `app.js`
- **RPC calls:** `eth_blockNumber`, `eth_getBlockByNumber`, `eth_getTransactionByHash`, `eth_getTransactionReceipt`, `eth_getBalance`, `eth_getCode`, `eth_gasPrice`
- **Features:** Dashboard with live blocks/txs, block detail, tx detail, address page, validator list, network info, search
- **Config:** `RPC_URL` and `CHAIN_ID` at top of `app.js`
- **Deploy:** `scp` to `/var/www/explorer/`

### 2. PrimeSwap DEX (`dex/`)

- **Stack:** Vanilla JS SPA, ethers.js v6 from CDN
- **Files:** `index.html`, `style.css`, `app.js`
- **Features:** Token swap, add/remove liquidity, pool info, pair analytics
- **Contracts:** PrimeSwapRouter, PrimeSwapFactory, WPRIM
- **Config:** Contract addresses and RPC at top of `app.js`
- **Deploy:** `scp` to `/var/www/dex/`

### 3. Validator Dashboard (`validator-explorer/`)

- **Stack:** Vanilla JS SPA, canvas charts
- **Files:** `index.html`, `style.css`, `app.js`
- **Features:** Validator list, stake distribution, block production charts, uptime, individual validator detail
- **Config:** RPC URL at top of `app.js`
- **Deploy:** `scp` to `/var/www/validators/`

### 4. Faucet (`crates/node/src/bin/faucet.html`)

- **Stack:** HTML embedded in Rust binary
- **Features:** Dispense PRIM + mock tokens (USDC/USDT/DAI), MetaMask connect, auto-add network
- **Deploy:** Requires `cargo build --release --bin faucet` then binary deploy (see above)
- **Rate limit:** 1 request per address per period (server-side)

### 5. Documentation Portal (`docs-site/`)

- **Stack:** Docusaurus (React/MDX), Node.js
- **Content:** 32 pages across Getting Started, Developers, Validators, Architecture, Ecosystem, Resources
- **Build:** `npm run build` → static files in `build/`
- **Deploy:** `rsync` to `/var/www/docs/`

### 6. Smart Contracts (`contracts/`)

- **Stack:** Foundry (forge, cast)
- **Test:** `forge test -vv` (28 tests across WPRIM, MockERC20, PrimeSwap)
- **Deploy:** `DEPLOYER_KEY=... node script/deploy.cjs`
- **Seed liquidity:** `DEPLOYER_KEY=... node script/seed-liquidity.cjs`

---

## Roadmaps

| Document | Scope | Path |
|----------|-------|------|
| **Infrastructure Roadmap** | 4-quarter plan: chain hardening → production → public testnet → mainnet | [`docs/INFRASTRUCTURE_ROADMAP.md`](INFRASTRUCTURE_ROADMAP.md) |
| **Ecosystem Roadmap** | Apps, contracts, integrations, competitive analysis | [`docs/ECOSYSTEM_ROADMAP.md`](ECOSYSTEM_ROADMAP.md) |
| **Whitepaper v7** | Full technical specification | [`docs/whitepaper.md`](whitepaper.md) |
| **Technical Reference** | Implementation details for all subsystems | [`docs/TECHNICAL_REFERENCE.md`](TECHNICAL_REFERENCE.md) |
| **Tokenomics** | Supply schedule, allocation, halving model | [`docs/TOKENOMICS.md`](TOKENOMICS.md) |
| **Security Audit** | Audit scope, findings, remediation tracking | [`SECURITY_AUDIT.md`](../SECURITY_AUDIT.md) |

### Roadmap Summary

| Quarter | Theme | Key Deliverables |
|---------|-------|------------------|
| **Q1** (Months 1-3) | Foundation & Hardening | Block timestamps, RPC compatibility (camelCase, filters, feeHistory), CI/CD pipelines, monitoring stack, contract tests, security fixes |
| **Q2** (Months 4-6) | Production Readiness | Subgraph/indexer, testnet bridge, SDK publishing, configurable endpoints, security audit scope |
| **Q3** (Months 7-9) | Public Testnet | External audit, governance, multi-sig, validator onboarding program, load testing |
| **Q4** (Months 10-12) | Mainnet Launch | Chain ID 13370, genesis ceremony, mainnet bridge, launch comms |

---

## Branding

| Element | Value |
|---------|-------|
| **Primary Violet** | `#4901FF` |
| **Light Violet** | `#6d2fff` |
| **Accent Cyan** | `#00FFF9` |
| **Pink** | `#FF00C0` |
| **Background** | `#0b0b12` |
| **Card Background** | `#111122` |
| **UI Font** | Sora (Google Fonts) |
| **Code Font** | JetBrains Mono (Google Fonts) |
| **Gradients** | Always Violet → Pink → Cyan (3-stop) |
| **Logo** | Violet swirl SVG in `docs-site/static/img/logo.svg` |
| **Logo repo** | `PrimeNumbersLabs/prime-numbers-labs-logos` |

---

## Key Decisions Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-03 | Chain ID `7919` (testnet), `13370` (mainnet) | Unique, not used by any known chain |
| 2026-03 | PRIM ticker | Not used by any tracked token |
| 2026-03 | Monorepo structure | Explorer, DEX, validator dashboard kept in main repo for ease of development |
| 2026-03 | Vanilla JS for frontends | No build step required, instant deploy via SCP |
| 2026-03 | Embedded faucet HTML | Single binary deployment, no static file dependency |
| 2026-03 | redb over sled | ACID-compliant, pure Rust, better write performance |
| 2026-03 | Block reward halving at 35M blocks | ~2.22 years per halving, 99% emitted by year 13 |
| 2026-03 | 70/10/10/5/5 allocation | Block rewards dominant, reasonable team/ecosystem/foundation split |
| 2026-03 | Nginx rate limiting | 50 req/s general, 5 req/s for sendRawTransaction, 5 WS connections per IP |
| 2026-03 | `DEPLOYER_KEY` env var | Removed hardcoded private keys from scripts |

---

## Useful Commands

```bash
# Check chain status
curl -s -X POST -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' \
  http://46.225.30.187:8545

# Check gas price
curl -s -X POST -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_gasPrice","params":[]}' \
  http://46.225.30.187:8545

# Check validator count
curl -s -X POST -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"prime_validators","params":[]}' \
  http://46.225.30.187:8545

# SSH to primary server
ssh root@46.225.30.187

# Check all processes
ssh root@46.225.30.187 'ps aux | grep -E "prime-chain|faucet|nginx|docker" | grep -v grep'

# View node logs
ssh root@46.225.30.187 'tail -50 /var/log/prime-chain.log'

# View faucet logs
ssh root@46.225.30.187 'tail -50 /var/log/faucet.log'
```
