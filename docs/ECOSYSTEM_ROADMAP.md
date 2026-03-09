# Prime Chain Ecosystem Roadmap

> **Master Plan:** This document covers the ecosystem layer (apps, contracts, integrations). For the full infrastructure roadmap covering core chain, CI/CD, security, audits, and mainnet launch, see **[Infrastructure Roadmap](INFRASTRUCTURE_ROADMAP.md)**.

## Current State: What We Have

### Live on Testnet (Chain ID 7919)

| Service | URL | Status |
|---------|-----|--------|
| 4-Validator Network | 4 Hetzner VPS nodes | Running |
| PrimeScan Explorer | http://46.225.30.187/ | Live |
| JSON-RPC | http://46.225.30.187:8545 | Live |
| WebSocket RPC | ws://46.225.30.187:8546 | Live |
| Faucet | http://46.225.30.187:8080 | Live |
| PrimeSwap DEX | http://46.225.30.187:4000 | Live |
| PrimeNodes Dashboard | http://46.225.30.187:4001 | Live |
| Grafana Monitoring | http://46.225.30.187:3000 | Live |
| Documentation Portal | http://46.225.30.187:3001 | Live |

### Deployed Contracts (Testnet)

| Contract | Address | Description |
|----------|---------|-------------|
| Multicall3 | `0x973ee1bf0907287d1eb8a144d88b34f515c83f29` | Batch RPC calls |
| WPRIM | `0x079bf1207b51acda83e2e8178344f62a883f8479` | Wrapped PRIM (ERC-20) |
| MockUSDC | `0xb22f77d89122e9e3784bfd3eee9616273f38238d` | Testnet USDC (6 decimals) |
| MockUSDT | `0x877feca38919acd7aaf7cb81f100e0454aa95c17` | Testnet USDT (6 decimals) |
| MockDAI | `0xb88d63a65691effbf4b6808325b1588912c15cf4` | Testnet DAI (18 decimals) |
| PrimeSwapFactory | `0x63f7a64db6d2b965189b8b48b7435668021f6b17` | DEX pair factory |
| PrimeSwapRouter | `0x9f337f433e71ce969b991511f1dcd3d0622116bb` | DEX swap router |

### Liquidity Pools (Seeded)

| Pool | Reserves | Implied Price |
|------|----------|---------------|
| WPRIM/USDC | 10,000 WPRIM + 10,000 USDC | 1 PRIM = 1 USDC |
| WPRIM/USDT | 10,000 WPRIM + 10,000 USDT | 1 PRIM = 1 USDT |
| WPRIM/DAI | 10,000 WPRIM + 10,000 DAI | 1 PRIM = 1 DAI |

### Built Locally (Not Yet Deployed to Prime Chain)

| App | Type | Repo | Stack | State |
|-----|------|------|-------|-------|
| PrimeXDC Wallet | Browser Extension | `primexdc-wallet/` | React, ethers, Chrome MV3 | Built |
| PrimeXDC Mobile | Mobile Wallet | `primexdc-mobile/` | React Native | APK built |
| PrimeFi Contracts | Lending/Borrowing | `primefi-contracts/` | Solidity, Hardhat, Aave-style | Contracts ready |
| PrimeFi UI v2 | Lending Frontend | `primefi-ui-v2/` | React 19, Vite, wagmi | Built (dist/) |
| PrimeFi Liquidator | Liquidation Bot | `primefi-liquidator/` | Node.js | Ready |
| PrimeFi Omni | Cross-chain Lending | `primefi-omni/` | Hardhat, LayerZero v2 | Contracts ready |
| Primeport UI | NFT Marketplace | `primeport-ui/` | Next.js 15, Seaport | Built (.next/) |
| Primeport Server | NFT Backend | `primeport-project/` | NestJS, Prisma, GraphQL | Ready |
| xdc-markets | Prediction Markets | `xdc-markets/` | Express, Vite, Prisma | Docker-ready |
| Liquid Staking | LST Contracts | `liquid-staking-contracts/` | Hardhat, Solidity | Contracts ready |
| Staking UI v2 | Staking Frontend | `primestaking-ui-v2/` | Next.js 15, wagmi | Built |
| Masternode Dashboard | Validator Mgmt | `xdc-masternode-dashboard/` | Express, Vite, React | Deployed elsewhere |
| PrimeRoll Casino | AI Agent Casino | `primeroll-agent-casino/` | NestJS, multi-SDK | Ready |
| Vault Contracts | DeFi Vaults | `prime-xdc-vaults/` | Hardhat, Solidity | Ready |
| PRFI NFT Contracts | Omnichain NFTs | `prfi-nft-contracts/` | Hardhat, LayerZero | Ready |
| SDKs | Developer SDKs | `prime-chain/sdk*` | JS, Go, Python | In repo |

---

## Completed Items

### P0 — Critical

| # | Need | Status |
|---|------|--------|
| 1 | Developer Documentation Portal (Docusaurus) | **DONE** — http://46.225.30.187:3001 |
| 2 | Network Configuration Page | **DONE** — in explorer + docs |
| 3 | Wallet Compatibility (MetaMask + one-click add) | **DONE** — documented in docs |
| 4 | Smart Contract Deployment Guide (Hardhat/Foundry) | **DONE** — in developer docs |
| 5 | ERC-20 Token Deployer (MockERC20 with faucet) | **DONE** — USDC, USDT, DAI deployed |
| 6 | WebSocket RPC | **DONE** — events wired into block producer |

### P1 — Important

| # | Need | Status |
|---|------|--------|
| 7 | DEX / AMM (PrimeSwap) | **DONE** — Factory + Router deployed |
| 8 | Wrapped PRIM (WPRIM) | **DONE** — deployed |
| 9 | Stablecoin Mocks | **DONE** — USDC, USDT, DAI deployed |
| 10 | Multicall3 Contract | **DONE** — deployed |
| 11 | Contract Verification | **DONE** — verified contracts shown in explorer |
| 12 | Ecosystem Landing Page | **DONE** — in docs portal |
| 13 | Liquidity Pools | **DONE** — WPRIM/USDC, WPRIM/USDT, WPRIM/DAI seeded |
| 14 | Whitepaper in Docs | **DONE** — /whitepaper route on docs portal |
| 15 | GitHub Organization | **DONE** — README, CONTRIBUTING, templates, branch cleanup |
| 16 | PrimeSwap DEX Frontend | **DONE** — http://46.225.30.187:4000 |
| 17 | PrimeNodes Validator Dashboard | **DONE** — http://46.225.30.187:4001 |
| 18 | Brand Rebrand (PNL Violet/Cyan) | **DONE** — Explorer, Faucet, Docs, DEX, Validators |
| 19 | Comprehensive Node Architecture Docs | **DONE** — Node architecture, consensus deep-dive, config reference |
| 20 | Full-text Search in Docs | **DONE** — Local search plugin |

---

## Remaining

### Phase 2: App Integration (User-driven)

These require pointing existing apps to chain 7919:

1. PrimeFi — deploy lending contracts, configure markets with WPRIM + mock stablecoins
2. Primeport — deploy Seaport, point UI + NestJS backend to testnet RPC
3. xdc-markets — deploy prediction market contracts, point frontend to testnet
4. PrimeXDC Wallet — update chain ID to 7919, add default RPC/explorer URLs
5. Staking UI — point to validator set on testnet
6. Liquid Staking — deploy LST contracts, connect staking UI

### Phase 3: Advanced Infrastructure

> These items are now tracked in the **[Infrastructure Roadmap](INFRASTRUCTURE_ROADMAP.md)** with specific quarterly timelines and deliverables.

| # | Need | Priority | Infra Roadmap Quarter |
|---|------|----------|----------------------|
| 1 | Subgraph / Indexer | P1 | Q2 |
| 2 | Bridge (testnet <-> Sepolia) | P2 | Q2 |
| 3 | Multi-sig Wallet (Safe-style) | P2 | Q3 |
| 4 | Name Service (.prime domains) | P2 | Post-mainnet |
| 5 | Account Abstraction (ERC-4337) | P2 | Post-mainnet |
| 6 | Governance Portal | P2 | Q3 |
| 7 | Bug Bounty / Grants program | P2 | Q3 |
| 8 | Status Page / uptime monitoring | P2 | Q1 |

---

## Documentation Portal

**Live at:** http://46.225.30.187:3001

### Content

| Section | Pages |
|---------|-------|
| **Getting Started** | Overview, Network Info, Wallet Setup, Faucet, First Transaction |
| **Developers** | Hardhat Deploy, Foundry Deploy, ERC-20 Guide, NFT Guide, DeFi Integration, RPC Overview, RPC Methods (incl. PrimeOrders + Bridge), JS SDK, Python SDK |
| **Validators** | Overview, Run a Node, Staking, Monitoring |
| **Architecture** | Consensus, Node Architecture, EVM Compatibility, PrimeOrders, Tokenomics |
| **Ecosystem** | PrimeSwap, PrimeFi, Primeport, Wallet |
| **Resources** | Deployed Contracts, Brand Assets, FAQ |
| **Whitepaper** | Full v7.0 technical whitepaper |

---

## Competitive Reference

| Feature | Ethereum | Polygon | Arbitrum | Base | **Prime Chain** |
|---------|----------|---------|----------|------|----------------|
| Explorer | Etherscan | Polygonscan | Arbiscan | Basescan | **Live** |
| Faucet | Multiple | Multiple | Alchemy | Coinbase | **Live** |
| Wallet | MetaMask+ | MetaMask+ | MetaMask+ | Coinbase | **Built (PrimeXDC)** |
| DEX + UI | Uniswap | QuickSwap | Camelot | Aerodrome | **Live (PrimeSwap)** |
| Lending | Aave/Compound | Aave | Aave | Moonwell | **Built (PrimeFi)** |
| NFT Market | OpenSea | OpenSea | OpenSea | OpenSea | **Built (Primeport)** |
| Bridge | Multiple | PoS Bridge | Arbitrum Bridge | Base Bridge | Missing |
| Docs Portal | ethereum.org | docs.polygon | docs.arbitrum | docs.base | **Live** |
| Staking UI | Lido, etc. | Polygon Staking | N/A | N/A | **Built** |
| Subgraph | The Graph | The Graph | The Graph | The Graph | Missing |
| Multi-sig | Safe | Safe | Safe | Safe | Missing |
| Contract Verification | Etherscan | Polygonscan | Arbiscan | Basescan | **Live** |
| WebSocket RPC | Yes | Yes | Yes | Yes | **Live** |
| Liquidity Pools | Yes | Yes | Yes | Yes | **Live** |
| Whitepaper | Yes | Yes | Yes | Yes | **Live** |
| GitHub Org | Yes | Yes | Yes | Yes | **Done** |
