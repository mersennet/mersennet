# Mersennet — Executive Summary

> **Confidential** | Series A — $25M | March 2026

---

## Company Overview

**Mersennet** is a high-performance Layer 1 blockchain purpose-built for institutional finance. It is the only blockchain where smart contracts and a native central limit order book (CLOB) share a single atomic state — enabling Solidity contracts to trade on the order book within the same transaction.

This architectural breakthrough, delivered through CLOB precompile `0x0100`, unlocks compliant, auditable, and composable trading of tokenized real-world assets (RWA) and on-chain credit instruments at institutional speed.

**Category:** Institutional Trading Infrastructure
**Stage:** Pre-mainnet, testnet live (Chain ID 7919)
**Raising:** $25M Series A
**Core Innovation:** Atomic EVM ↔ CLOB composability (unique in market)

---

## The Opportunity

Traditional finance is in the early stages of tokenizing trillions of dollars in assets — bonds, real estate, commodities, funds, trade finance instruments, and credit products. Yet no existing blockchain provides the trading infrastructure these assets require: deterministic execution, native order matching, regulatory compliance, and institutional-grade performance.

### Market Size

| Market | 2024 | 2030 (Projected) | Growth |
|--------|------|-------------------|--------|
| RWA Tokenization | $310B | $16T | 50x |
| On-Chain Credit | ~$50B | $2.5T+ | 50x |
| **Combined TAM** | **$360B** | **$18.5T+** | **50x** |

Sources: BCG "Relevance of On-Chain Asset Tokenization," McKinsey, BIS Global Credit Report.

### Timing

Three forces are converging to create a window of 12–18 months:

1. **Regulatory clarity** — MiCA (EU), SEC tokenized securities guidance, Basel III credit rules are providing the framework institutions need.
2. **Institutional entry** — BlackRock ($500M+ BUIDL fund), Fidelity, Standard Chartered, and JPMorgan are actively tokenizing assets.
3. **Infrastructure gap** — Every major bank is building tokenization capability, but none have compliant trading infrastructure. The first chain to solve this captures the category.

---

## The Solution

Mersennet unifies three execution domains in a single blockchain:

```
State = (EVM + PrimeOrders + Bridge)
         │         │            │
   Smart Contracts  │       Cross-Chain
                    │
            Native Order Book
```

### Key Technical Differentiators

| Feature | Specification | Competitive Comparison |
|---------|--------------|----------------------|
| EVM TPS | 72,181 (Block-STM parallel) | Ethereum: 15, Solana: 4,000, Monad: 10,000 |
| CLOB Throughput | 2,484,170 ops/s | Hyperliquid: 200,000, dYdX: 100,000 |
| Finality | ~200ms (HotStuff-2 BFT) | Ethereum: 12,000ms, Solana: 400ms |
| EVM ↔ CLOB | **Atomic** (same transaction) | Hyperliquid: Async (seconds delay, stale reads) |
| MEV Protection | Frequent Batch Auctions (5M+ ops/s) | Most chains: None or partial |
| Compliance | Fully deterministic, auditable | Most chains: Probabilistic |

### What This Means

A smart contract on Mersennet can atomically: accept a tokenized bond deposit, place a limit order on the native order book, receive a fill, and use the proceeds as DeFi collateral — all in a single 200ms transaction. This is impossible on every other blockchain.

---

## Traction

### Partnerships (Active Relationships)

| Partner | Engagement | Value |
|---------|-----------|-------|
| **Standard Chartered** | RWA + credit trading pilot (Q1 2026), regulatory introductions to FCA/MAS/HKMA | $3–5M strategic investment discussions |
| **XDC Network** | Cross-chain trade finance integration, joint go-to-market | $2–3M ecosystem investment discussions |
| **Binance** | Exchange infrastructure evaluation, institutional credit markets | $5–7M strategic investment discussions |
| **Greg Kidd** | Strategic advisor (early Coinbase investor, Ripple, Square) | $1–2M via Hard Yaka fund |

### Technical Progress

- **Testnet live** on 4 Hetzner VPS validators (Chain ID 7919)
- **12 products built:** Explorer, DEX, lending, NFT marketplace, wallet (browser + mobile), prediction markets, liquid staking, validator dashboard, SDKs (Python, Go, TypeScript)
- **6 products deployed** to testnet with live UIs
- **7 smart contracts** verified and deployed (Multicall3, WPRIM, stablecoins, DEX factory/router)
- **3 liquidity pools** seeded (WPRIM/USDC, WPRIM/USDT, WPRIM/DAI)
- **Comprehensive documentation portal** (32+ pages, Docusaurus)
- **Core chain:** 44,000+ lines of Rust across a 6-crate workspace

---

## Business Model

### Revenue Streams

| Stream | Description | % of Revenue (Steady State) |
|--------|-------------|---------------------------|
| **Trading Fees** | 0.1–0.5% on RWA and credit trades | 60% |
| **Infrastructure Licensing** | White-label matching engine for institutions | 20% |
| **API Access** | Premium data feeds and trading APIs | 12% |
| **Data & Analytics** | Market data, compliance reporting, risk analytics | 8% |

### Financial Projections

| Metric | Year 1 | Year 2 | Year 3 | Year 4 |
|--------|--------|--------|--------|--------|
| Institutional Partners | 3 | 12 | 30 | 50+ |
| Total Value Assets (TVA) | $500M | $5B | $25B | $100B |
| Daily Trading Volume | $5M | $50M | $250M | $1B |
| **Total Revenue** | **$8.5M** | **$85M** | **$425M** | **$1.2B** |
| Gross Margin | 85% | 88% | 92% | 94% |
| Team Size | 15 | 35 | 60 | 100 |

### Comparable Valuations

| Project | Raise | FDV at Raise | Revenue | Key Difference from Mersennet |
|---------|-------|-------------|---------|-------------------------------|
| Hyperliquid | TGE | $4.5B | ~$500M ann. | Async EVM ↔ CLOB, crypto-only |
| Monad | $225M Series A | $3.0B | Pre-revenue | No CLOB, no institutional focus |
| Berachain | $69M Series A | $1.5B | Pre-revenue | No CLOB, DeFi-focused |
| dYdX | $65M Series C | $2.0B | ~$200M ann. | App-chain, no EVM |

Mersennet at $25M Series A represents early-stage pricing for technology that combines the best attributes of Hyperliquid (native CLOB) and Monad (parallel EVM) with an institutional focus none of them offer.

---

## Tokenomics

| Parameter | Value |
|-----------|-------|
| Token | PRIM |
| Max Supply | 1,000,000,000 (1B) |
| Block Rewards | 70% (700M) — Bitcoin-style halving every 35M blocks (~1.1 years at 1 s blocks) |
| Ecosystem & Grants | 10% (100M) — 5-year linear |
| Foundation Reserve | 10% (100M) — 1-year cliff + 4-year linear |
| Team & Contributors | 5% (50M) — 1-year cliff + 3-year linear |
| Sales | 5% (50M) — 6-month cliff + 18-month linear |

**Notable:** 10% combined team + investor allocation is the lowest of any major L1 launched in the last three years. 70% goes to validators, ensuring network security and decentralization.

---

## The Ask

### $25 Million Series A

| Category | Amount | Use |
|----------|--------|-----|
| Engineering | $8.75M (35%) | Mainnet development, security audits, bridge, SDKs, developer tools |
| Business Development | $7.0M (28%) | Institutional partnerships, regulatory engagement, pilot programs |
| Operations & Team | $5.0M (20%) | Key hires (CTO, Head of BD, VP Eng, GC), infrastructure, compliance |
| Ecosystem & Grants | $3.0M (12%) | Developer grants, hackathons, integration partnerships |
| Reserves | $1.25M (5%) | Contingency, strategic opportunities |

**Runway:** 24+ months at full burn rate.

### Milestones Unlocked by This Raise

| Milestone | Target Date | Success Metric |
|-----------|-------------|----------------|
| Standard Chartered live pilot | Q1 2026 | $100M tokenized bond trading |
| 3+ institutional partners | Q2 2026 | Signed agreements |
| Security audit passed | Q3 2026 | Clean audit from tier-1 firm |
| Public testnet (15+ validators) | Q3 2026 | Community participation |
| Mainnet launch | Q4 2026 | Chain ID 13370, institutional trading live |
| $500M+ TVA | Q4 2026 | Assets trading on Mersennet |

---

## Investment Thesis

1. **Unique technology moat** — Only L1 with atomic EVM ↔ CLOB composability. Requires consensus-level integration, creating a 2+ year engineering barrier to entry.

2. **Largest TAM in crypto** — $18.5T+ institutional RWA + credit market, growing 50x by 2030. Not speculative — driven by regulatory frameworks and institutional capital flows.

3. **Active institutional relationships** — Standard Chartered, XDC Network, Binance, and Greg Kidd are not pitch targets — they are active partners with defined pilot timelines.

4. **Built, not theoretical** — Testnet live with 12 products, 72K TPS measured, comprehensive docs. This is execution, not a whitepaper.

5. **Category-defining window** — The "institutional trading L1" category will be captured in the next 12–18 months. First mover with institutional partnerships wins.

---

## Risk Factors & Mitigations

| Risk | Mitigation |
|------|-----------|
| Regulatory uncertainty | Early FCA/MAS/SEC engagement via Standard Chartered regulatory relationships |
| Technical execution | Testnet live, benchmarks proven, incremental path to mainnet |
| Competition from larger chains | Consensus-level CLOB cannot be retrofitted — requires new L1 |
| Market timing (RWA slower than expected) | Multiple use cases (RWA + credit + DeFi), EVM compatibility as fallback |
| Security vulnerabilities | Planned tier-1 audit Q3 2026, bug bounty program, formal verification |
| Key person risk | $25M enables hiring of 15-person core team, reducing single-point-of-failure |

---

## Contact

**Mersennet**
The Institutional Trading Layer for Real-World Assets + On-Chain Credit Markets

- **Docs:** docs.primechain.network
- **GitHub:** PrimeNumbersLabs
- **Testnet Explorer:** Chain ID 7919
- **Email:** [founders@primechain.network]

---

*This document is confidential and for qualified investors only. It does not constitute an offer to sell or solicitation to buy any securities. Investment in digital assets carries significant risk including potential loss of principal.*
