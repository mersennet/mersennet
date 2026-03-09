# Prime Chain — Investor Data Room

> **Confidential** | Series A — $25M | March 2026
> Access granted to: [Investor Name] on [Date]

---

## Data Room Structure

This data room contains all materials required for due diligence on Prime Chain's $25M Series A round. Documents are organized by category with security classification.

```
📁 PRIME CHAIN DATA ROOM
│
├── 📁 1. FUNDRAISING MATERIALS
│   ├── 📄 Pitch Deck (16 slides)                    ← PITCH_DECK_25M.md
│   ├── 📄 One-Pager                                 ← ONE_PAGER_25M.md
│   ├── 📄 Executive Summary (3 pages)               ← EXECUTIVE_SUMMARY_25M.md
│   ├── 📄 Financial Model & Unit Economics           ← FINANCIAL_MODEL_25M.md
│   ├── 📄 Term Sheet Template                        ← TERM_SHEET_TEMPLATE.md
│   └── 📄 Investor FAQ                              ← INVESTOR_FAQ.md
│
├── 📁 2. TECHNICAL DOCUMENTATION
│   ├── 📄 Whitepaper v7.0                            ← docs/whitepaper.md
│   ├── 📄 Technical Reference                        ← docs/TECHNICAL_REFERENCE.md
│   ├── 📄 Architecture Decision Records              ← docs/ARCHITECTURE.md
│   ├── 📄 Competitive Analysis                       ← docs/competitive-analysis.md
│   ├── 📄 Security Audit Scope                       ← SECURITY_AUDIT.md
│   └── 📄 Security Policy                            ← docs/SECURITY.md
│
├── 📁 3. TOKENOMICS & ECONOMICS
│   ├── 📄 Tokenomics (full spec)                     ← docs/TOKENOMICS.md
│   ├── 📄 Emission Schedule & Halving Curves         ← (in Tokenomics)
│   ├── 📄 Validator Economics                        ← (in Tokenomics)
│   └── 📄 Slashing Mechanics                        ← (in Tokenomics)
│
├── 📁 4. STRATEGY & POSITIONING
│   ├── 📄 Strategic Positioning Document             ← docs/STRATEGIC_POSITIONING.md
│   ├── 📄 Go-to-Market Strategy                     ← (in Strategic Positioning)
│   ├── 📄 Fundraising Strategy                      ← (in Strategic Positioning)
│   └── 📄 Competitive Moat Analysis                 ← (in Strategic Positioning)
│
├── 📁 5. ROADMAPS
│   ├── 📄 Ecosystem Roadmap                          ← docs/ECOSYSTEM_ROADMAP.md
│   ├── 📄 Infrastructure Roadmap (4 quarters)        ← docs/INFRASTRUCTURE_ROADMAP.md
│   ├── 📄 Mainnet Launch Checklist                   ← mainnet/launch-checklist.md
│   └── 📄 Validator Requirements                     ← mainnet/validator-requirements.md
│
├── 📁 6. PRODUCT & ECOSYSTEM
│   ├── 📄 Live Products (testnet)
│   │   ├── PrimeScan Explorer                       → http://46.225.30.187/
│   │   ├── PrimeSwap DEX                            → http://46.225.30.187:4000
│   │   ├── PrimeNodes Dashboard                     → http://46.225.30.187:4001
│   │   ├── Faucet                                   → http://46.225.30.187:8080
│   │   ├── Documentation Portal                     → http://46.225.30.187:3001
│   │   └── Grafana Monitoring                       → http://46.225.30.187:3000
│   │
│   ├── 📄 Built Products (ready to deploy)
│   │   ├── PrimeFi (Lending/Borrowing)              ← primefi-contracts/
│   │   ├── Primeport (NFT Marketplace)              ← primeport-ui/
│   │   ├── PrimeXDC Wallet (Browser + Mobile)       ← primexdc-wallet/
│   │   ├── Prediction Markets                       ← xdc-markets/
│   │   ├── Liquid Staking                           ← liquid-staking-contracts/
│   │   └── DeFi Vaults                              ← prime-xdc-vaults/
│   │
│   └── 📄 SDKs
│       ├── Python SDK                               ← sdk-python/
│       ├── Go SDK                                   ← sdk-go/
│       └── TypeScript SDK                           ← sdk/
│
├── 📁 7. TEAM & ORGANIZATION
│   ├── 📄 Team Handbook                              ← docs/TEAM.md
│   ├── 📄 Contributing Guidelines                    ← CONTRIBUTING.md
│   └── 📄 GitHub Organization: PrimeNumbersLabs
│
├── 📁 8. CODEBASE (available on request)
│   ├── 📄 Core Chain (Rust)                          ← crates/ (28 modules, 10,600+ LOC)
│   ├── 📄 Smart Contracts (Solidity)                 ← contracts/ (Foundry, 28 tests)
│   ├── 📄 Explorer                                   ← explorer/
│   ├── 📄 DEX                                        ← dex/
│   ├── 📄 Validator Explorer                         ← validator-explorer/
│   ├── 📄 Documentation Site                         ← docs-site/ (Docusaurus)
│   └── 📄 Deployment Configs                         ← deploy/
│
└── 📁 9. LEGAL (to be prepared)
    ├── 📄 Token Legal Opinion
    ├── 📄 SAFT / SAFE Agreement
    ├── 📄 Corporate Structure
    ├── 📄 Regulatory Analysis (FCA, MAS, SEC)
    └── 📄 IP Assignment
```

---

## Document Summary Table

| # | Document | Type | Pages | Classification |
|---|----------|------|-------|---------------|
| 1 | Pitch Deck | Fundraising | 16 slides | Confidential |
| 2 | One-Pager | Fundraising | 1 page | Confidential |
| 3 | Executive Summary | Fundraising | 3 pages | Confidential |
| 4 | Financial Model | Fundraising | 10 pages | Highly Confidential |
| 5 | Term Sheet | Legal | 3 pages | Highly Confidential |
| 6 | Investor FAQ | Fundraising | 5 pages | Confidential |
| 7 | Whitepaper v7.0 | Technical | ~50 pages | Public |
| 8 | Technical Reference | Technical | ~30 pages | Public |
| 9 | Tokenomics | Economics | 8 pages | Public |
| 10 | Strategic Positioning | Strategy | ~20 pages | Confidential |
| 11 | Ecosystem Roadmap | Roadmap | 3 pages | Public |
| 12 | Infrastructure Roadmap | Roadmap | 6 pages | Public |
| 13 | Competitive Analysis | Strategy | 5 pages | Confidential |
| 14 | Security Policy | Technical | 2 pages | Public |
| 15 | Team Handbook | Operations | 6 pages | Internal |

---

## Due Diligence Checklist

### Technical Due Diligence

- [ ] Review whitepaper v7.0 (architecture, consensus, PrimeOrders)
- [ ] Review technical reference (implementation details)
- [ ] Test live testnet (Chain ID 7919, RPC: http://46.225.30.187:8545)
- [ ] Review codebase (Rust core — 28 modules, 10,600+ LOC)
- [ ] Verify performance claims (72K TPS, 2.4M CLOB ops/s, ~200ms finality)
- [ ] Review smart contracts (Foundry, 28 tests)
- [ ] Evaluate CLOB precompile (0x0100) atomic composability
- [ ] Assess security posture (SECURITY.md, SECURITY_AUDIT.md)
- [ ] Review architecture decisions (13 ADRs in ARCHITECTURE.md)

### Business Due Diligence

- [ ] Review financial model and assumptions
- [ ] Validate market size claims (BCG, McKinsey sources)
- [ ] Assess partnership strength (Standard Chartered, XDC, Binance, Greg Kidd)
- [ ] Review competitive positioning vs. Hyperliquid, Monad, dYdX, Sei
- [ ] Evaluate go-to-market strategy
- [ ] Review team and hiring plan
- [ ] Assess regulatory strategy (FCA, MAS, SEC approach)

### Legal Due Diligence

- [ ] Token legal opinion
- [ ] Corporate structure review
- [ ] IP ownership confirmation
- [ ] Regulatory compliance assessment
- [ ] SAFT/SAFE agreement review
- [ ] Team equity/token agreements

### Ecosystem Due Diligence

- [ ] Test deployed products (explorer, DEX, validator dashboard)
- [ ] Review built products (lending, NFT marketplace, wallet)
- [ ] Evaluate developer documentation (docs portal)
- [ ] Assess SDK quality (Python, Go, TypeScript)
- [ ] Review deployment infrastructure (Hetzner VPS, Docker)

---

## Live Demo Access

| Resource | URL | Credentials |
|----------|-----|-------------|
| Testnet RPC | http://46.225.30.187:8545 | Public |
| WebSocket RPC | ws://46.225.30.187:8546 | Public |
| Block Explorer | http://46.225.30.187/ | Public |
| DEX (PrimeSwap) | http://46.225.30.187:4000 | Public |
| Validator Dashboard | http://46.225.30.187:4001 | Public |
| Faucet | http://46.225.30.187:8080 | Public |
| Documentation | http://46.225.30.187:3001 | Public |
| Grafana Monitoring | http://46.225.30.187:3000 | Public |

### Quick Start (Try the Testnet)

1. Add Prime Chain to MetaMask: Chain ID `7919`, RPC `http://46.225.30.187:8545`
2. Get testnet PRIM from the faucet: http://46.225.30.187:8080
3. Trade on PrimeSwap: http://46.225.30.187:4000
4. View your transactions: http://46.225.30.187/

---

## Contact & Access

**Primary Contact:** [founders@primechain.network]
**GitHub:** github.com/PrimeNumbersLabs
**Docs:** docs.primechain.network

For additional materials or questions not covered in this data room, please contact the team directly.

---

*All materials in this data room are confidential and proprietary. Distribution without written consent is prohibited. This data room does not constitute an offer to sell or a solicitation of an offer to buy any securities.*
