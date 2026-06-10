# Mersennet — Investor FAQ

> **Confidential** | Series A — $25M | March 2026

---

## Technology

### Q: What is Mersennet in one sentence?

**A:** Mersennet is the only Layer 1 blockchain where Solidity smart contracts can trade on a native central limit order book (CLOB) in the same transaction — enabling institutional-grade, compliant trading of tokenized real-world assets and credit instruments.

---

### Q: What makes the atomic EVM ↔ CLOB composability unique?

**A:** On Mersennet, a smart contract can atomically (in a single transaction):

1. Receive a tokenized bond deposit
2. Place a limit order on the native order book
3. Get the fill result back
4. Use those proceeds as collateral in a DeFi lending protocol

This happens in ~200ms, in one transaction, with deterministic execution. No other blockchain can do this.

**Hyperliquid** has a CLOB and HyperEVM, but they are **async** — the CoreWriter is delayed by seconds, and EVM reads of order book state are stale by at least one block. Smart contracts cannot atomically trade on the order book.

**Why it matters:** Atomic composability means DeFi protocols can be built directly on top of institutional order books. This is how you bridge TradFi and DeFi in a single atomic operation.

---

### Q: How do your performance numbers compare?

```
                     EVM TPS       CLOB ops/s     Finality
   ─────────────────────────────────────────────────────────
   Ethereum           15            N/A            12,000ms
   Solana            4,000          N/A              400ms
   Monad            10,000          N/A            ~1,000ms
   Hyperliquid       ~500         200,000            200ms
   dYdX              N/A          100,000           ~1,000ms
   ─────────────────────────────────────────────────────────
   MERSENNET      72,181       2,484,170            200ms
```

- **EVM:** 72,181 TPS via Block-STM parallel execution (4.8x Monad, 18x Solana)
- **CLOB:** 2,484,170 ops/s (12.4x Hyperliquid, 24.8x dYdX)
- **FBA:** 5,053,782 ops/s (MEV-resistant matching)
- **Finality:** ~200ms (HotStuff-2 BFT) — comparable to Hyperliquid, 60x faster than Ethereum

These are measured benchmarks, not theoretical maximums.

---

### Q: What is the tech stack?

| Layer | Technology |
|-------|-----------|
| Core blockchain | Rust 2024 edition (10,600+ LOC, 28 modules) |
| EVM | revm v12 (Shanghai spec) |
| Consensus | HotStuff-2 BFT (DPoS) |
| CLOB | Native PrimeOrders engine with FBA matching |
| Storage | sled 0.34 (dev), redb (production) |
| Networking | Custom UDP gossip, TCP sync, Noise encryption |
| Cryptography | secp256k1 ECDSA (k256), keccak256 |
| RPC | JSON-RPC 2.0 + WebSocket subscriptions |
| Contracts | Solidity 0.8.20+, Foundry |
| Docs | Docusaurus 3.9, React 19 |

---

### Q: Is the code open source?

**A:** The codebase lives in the PrimeNumbersLabs GitHub organization. Core protocol code and documentation are available. Smart contracts are verified on-chain. We plan full open-sourcing around mainnet launch.

---

### Q: What about security?

**A:** Security is addressed at multiple levels:

1. **Architecture:** Deterministic execution means every state transition is reproducible and auditable
2. **MEV Protection:** Frequent Batch Auctions (FBA) eliminate front-running and sandwich attacks
3. **Testing:** 241 Rust tests passing, Foundry test suite for contracts
4. **Audit Plan:** Tier-1 security audit scheduled for Q3 2026 (pre-mainnet)
5. **Bug Bounty:** Program planned for post-audit
6. **Slashing:** Double-sign (5%), timeout (1%), escalating penalties up to 10%

---

## Market & Competition

### Q: Why is the $18.5T TAM credible?

**A:** The TAM is derived from third-party research, not our estimates:

| Source | Estimate | Scope |
|--------|----------|-------|
| BCG + ADDX | $16T by 2030 | RWA tokenization |
| McKinsey | $5T by 2030 | Tokenized securities |
| BIS | $2.5T+ by 2030 | On-chain credit |
| Citi | $5T by 2030 | Asset tokenization |
| World Economic Forum | 10% of global GDP tokenized by 2027 | Broad tokenization |

Our assumption: Mersennet captures **0.14% of TAM** in Year 3 ($25B TVA out of $18.5T). That's conservative — it requires only 30 institutional partners with ~$833M TVA each.

---

### Q: Why not just use Ethereum or an L2?

**A:**

| Limitation | Ethereum | L2s (Arbitrum, etc.) | Mersennet |
|-----------|----------|---------------------|-------------|
| Finality | 12 seconds | 1–2 seconds | **200ms** |
| Order Matching | Smart contract (MEV-prone) | Smart contract (MEV-prone) | **Native CLOB (MEV-free)** |
| Compliance | Probabilistic execution | Probabilistic | **Deterministic, auditable** |
| Risk Management | Build from scratch | Build from scratch | **Built-in (margin, liquidation)** |
| Gas Cost | $10–100/tx | $0.10–1/tx | **<$0.01/tx** |

Ethereum is the "internet computer." Mersennet is the "institutional exchange." Different products for different markets.

---

### Q: How is Mersennet different from Hyperliquid?

**A:** This is the most important competitive question. Three critical differences:

1. **Atomic vs. Async:** Mersennet's EVM ↔ CLOB is atomic (same transaction). Hyperliquid's is async — CoreWriter is delayed by seconds, reads are stale by 1 block. This means DeFi protocols on Hyperliquid cannot atomically interact with the order book.

2. **12x Faster CLOB:** 2.4M ops/s vs. 200K ops/s. More throughput = more concurrent markets, more institutional users.

3. **Institutional Focus vs. Crypto Perps:** Hyperliquid focuses on crypto perpetual futures. Mersennet targets $18.5T in tokenized RWA and credit — a fundamentally larger market.

**Analogy:** Hyperliquid is Binance Futures on-chain. Mersennet is the NYSE/NASDAQ for tokenized real-world assets.

---

### Q: What if Ethereum adds native order matching?

**A:** Extremely unlikely for three reasons:

1. **Consensus-level change:** Adding a CLOB to Ethereum would require modifying the consensus layer — the most conservative part of any blockchain. Ethereum's roadmap is focused on scaling (sharding, rollups), not adding new execution domains.

2. **Community governance:** Ethereum's governance process takes years for even minor changes. Adding a fundamentally new execution domain would face enormous resistance.

3. **Design philosophy:** Ethereum's philosophy is "minimal viable base layer." Adding application-specific features contradicts this principle.

The risk of Ethereum adding native order matching in the next 5 years is effectively zero.

---

## Business & Partnerships

### Q: How real are the partnerships?

**A:** The partnerships represent active relationships with defined engagement paths:

| Partner | Relationship Level | Evidence |
|---------|-------------------|----------|
| **Standard Chartered** | Direct VP relationship | RWA pilot scoped for Q1 2026, regulatory introductions discussed |
| **XDC Network** | Core team relationship | Cross-chain integration planned, ecosystem fund investment |
| **Binance** | Leadership relationship | Exchange infrastructure evaluation, strategic investment discussions |
| **Greg Kidd** | Advisory relationship | Early Coinbase investor, Hard Yaka fund |

These are not cold outreach targets. These are active relationships with specific engagement timelines and deliverables.

---

### Q: What's the revenue model?

**A:** Four revenue streams, all driven by the volume of tokenized assets on the network:

| Stream | Model | Year 3 Target |
|--------|-------|---------------|
| **Trading Fees** | 0.1–0.5% per trade | $182.5M |
| **Infrastructure Licensing** | Annual license for white-label matching engine | $85M |
| **API Access** | Monthly subscription for premium APIs | $75M |
| **Data & Analytics** | Market data, compliance reporting | $82.5M |

**Key metric:** 99.99% gross margin per trade (revenue: ~$100/trade, marginal cost: ~$0.01/trade). Infrastructure businesses have extraordinary unit economics.

---

### Q: Why $25M? Why not raise more or less?

**A:** $25M is calibrated to three factors:

1. **Runway:** 24+ months at full burn, reaching breakeven by month 14–16
2. **Milestones:** Sufficient to achieve mainnet launch, security audit, 10+ institutional partners, and $5B+ TVA pipeline
3. **Dilution:** At suggested FDV of $150M–$250M, this represents 10–17% dilution — reasonable for Series A

Raising less would risk under-funding business development (the partnerships are the moat). Raising more at this stage would be unnecessarily dilutive before proving mainnet revenue.

---

### Q: What's the path to breakeven?

**A:**

```
    Revenue vs. OpEx (Monthly)
    
    $8M   ┤                                              ╱ Revenue
          │                                         ╱──╱
    $6M   ┤                                   ╱──╱
          │                              ╱──╱
    $4M   ┤                         ╱──╱
          │                    ╱──╱
    $2M   ┤               ╱──╱
          │          ╱──╱               ════════════════  OpEx
    $1M   ┤     ╱──╱               ════════════════
          │╱──╱           ═════════
          ╱══════════════
    $0    └──┬────┬────┬────┬────┬────┬────┬────┬────
           M1   M3   M6   M9   M12  M15  M18  M21  M24
                                  ▲
                          BREAKEVEN (Month 14–16)
```

- **Months 1–6:** $750K/month burn (team building, infrastructure)
- **Months 7–12:** $1.17M/month burn (growing team, pilot programs)
- **Month 14–16:** Revenue from trading fees + licensing crosses OpEx
- **Month 18+:** Cash-flow positive, scaling with revenue

---

## Tokenomics

### Q: Why only 5% for sales?

**A:** Mersennet has the lowest combined insider allocation (10% team + sales) of any major L1 launched in the last three years:

| Chain | Team | Investors | Combined Insider |
|-------|------|-----------|-----------------|
| Celestia | 17.6% | 35.6% | **53.2%** |
| Aptos | 19% | 49% | **68%** |
| Sui | 14% | 48% | **62%** |
| Avalanche | 20% | 4% | **24%** |
| **PRIM** | **5%** | **5%** | **10%** |

70% goes to block rewards (validators), ensuring network security and genuine decentralization. The 5% sales allocation is sufficient for a $25M raise while maintaining an investor-friendly supply structure.

---

### Q: What's the token utility?

**A:** PRIM serves four functions:

1. **Staking:** Validators stake PRIM to participate in consensus and earn block rewards
2. **Gas:** Transaction fees paid in PRIM (EIP-1559 base fee model)
3. **Governance:** Token-weighted voting on protocol parameters
4. **Collateral:** Can be used as collateral in PrimeFi lending and PrimeOrders margin

---

### Q: What's the expected initial APY for stakers?

**A:** At genesis with 4 validators (1M PRIM staked each):

```
Annual blocks:     ~15,768,000 (at 2s block time)
Reward per block:  10 PRIM
Per validator:     2.5 PRIM/block
Annual yield:      39,420,000 PRIM per validator
APY:               ~3,942%
```

High initial APY incentivizes early staking. As more validators join and total stake increases, APY decreases proportionally. Halving every ~2.2 years further reduces emissions over time.

---

## Risk & Mitigation

### Q: What are the biggest risks?

| Risk | Severity | Probability | Mitigation |
|------|----------|------------|------------|
| **Regulatory** | High | Medium | Early FCA/MAS/SEC engagement via Standard Chartered |
| **Technical execution** | Medium | Low | Testnet live, benchmarks proven, incremental path |
| **Partnership failure** | High | Low | Multiple partners (no single dependency) |
| **Competition** | Medium | Medium | 2+ year consensus-level moat |
| **Market timing** | Medium | Medium | Multiple use cases, EVM as fallback |
| **Security** | High | Low | Planned audit, bug bounty, deterministic design |
| **Key person** | Medium | Medium | $25M enables 15-person team |

---

### Q: What if RWA tokenization is slower than expected?

**A:** Mersennet has three fallback positions:

1. **Credit markets:** $2.5T+ TAM independent of RWA timeline
2. **DeFi:** Full EVM compatibility means standard DeFi use cases work immediately
3. **AI Trading:** Deterministic matching is ideal for AI trading agents (hot market)

The native CLOB + EVM combination has value even if RWA tokenization takes longer. The chain is not dependent on any single use case.

---

### Q: How do you handle regulatory risk?

**A:** Five-layer regulatory strategy:

1. **Compliance by design:** Deterministic execution means every trade is auditable — regulators can independently verify execution fairness
2. **Institutional partners:** Standard Chartered provides FCA, MAS, and HKMA introductions
3. **Early engagement:** Proactive regulatory dialogue, not reactive compliance
4. **Flexible governance:** Protocol parameters can be updated without hard forks
5. **Legal structure:** Foundation structure with clear jurisdictional strategy

---

## Investment Mechanics

### Q: What is the investment instrument?

**A:** SAFT (Simple Agreement for Future Tokens). Investors receive the right to PRIM tokens at Token Generation Event (TGE), subject to vesting. The SAFT is a widely used instrument in crypto fundraising with established legal precedent.

---

### Q: What are the vesting terms?

**A:** 6-month cliff + 18-month linear vesting (24 months total). No tokens unlock at TGE. First unlock at month 7. Team vests over 48 months (longer than investors), ensuring alignment.

---

### Q: What governance rights do investors have?

**A:** 
- Standard token voting (1 token = 1 vote)
- Board observer seat for investments ≥$5M
- Protective provisions (consent for supply changes, material tokenomics changes, large related-party transactions)
- Pro-rata rights in future rounds
- Anti-dilution protection (weighted average)
- MFN (most favored nation) clause

---

### Q: What are the expected exit paths?

**A:**

1. **TGE + Exchange Listing:** PRIM listed on major exchanges at mainnet (Q4 2026). Post-vesting liquidity via public markets.
2. **Secondary Sales:** OTC sales to institutional buyers (with issuer consent during vesting).
3. **Strategic Acquisition:** Enterprise blockchain or exchange acquiring the technology platform.
4. **Token Appreciation:** Protocol revenue growth drives token value (comparable: Hyperliquid $4.5B FDV with $500M annual fees).

---

### Q: What comparable returns should I expect?

| Comparable | Entry → Peak | Timeline | Notes |
|-----------|-------------|----------|-------|
| Hyperliquid | Community → $4.5B FDV | 1 year | Async CLOB, crypto-only |
| Solana | $20M raise → $77B FDV | 3 years | General L1 |
| Avalanche | $60M raise → $30B FDV | 2 years | General L1 |
| Arbitrum | Private → $10B FDV | 2 years | L2, no CLOB |

**Conservative estimate:** 8–13x at $2B FDV (Year 3)
**Base case:** 26–43x at $5B+ FDV (Year 3)
**Optimistic:** 72–120x at $10B+ FDV (Year 3)

---

## Next Steps

### For Interested Investors

1. **Sign NDA** — Access full data room
2. **Technical deep-dive** — Live demo of testnet + architecture walkthrough
3. **Financial review** — Detailed model walkthrough with management
4. **Legal review** — SAFT agreement, regulatory analysis
5. **Term sheet negotiation** — Finalize terms
6. **Closing** — Fund and execute

### Contact

**Email:** [founders@primechain.network]
**Docs:** docs.primechain.network
**GitHub:** PrimeNumbersLabs
**Testnet:** Chain ID 7919

---

*This FAQ is confidential and for qualified investors only. It does not constitute investment advice or an offer to sell securities. All forward-looking statements involve risk and uncertainty. Past performance of comparable projects does not guarantee future results.*
