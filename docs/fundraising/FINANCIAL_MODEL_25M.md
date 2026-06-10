# Mersennet — Financial Model & Unit Economics

> **Confidential** | Series A — $25M | March 2026

---

## 1. Revenue Model Architecture

Mersennet generates revenue through four complementary streams, all driven by the volume of tokenized assets trading on the network.

```
                    ┌──────────────────────────┐
                    │   TOKENIZED ASSETS ON     │
                    │   MERSENNET (TVA)        │
                    └─────────┬────────────────┘
                              │
          ┌───────────┬───────┼───────┬──────────────┐
          │           │       │       │              │
    ┌─────▼─────┐ ┌───▼───┐ ┌▼─────┐ ┌▼──────────┐  │
    │ TRADING   │ │ INFRA │ │ API  │ │ DATA &    │  │
    │ FEES      │ │LICENSE│ │ACCESS│ │ ANALYTICS │  │
    │ 0.1–0.5%  │ │per yr │ │/month│ │per year   │  │
    │           │ │       │ │      │ │           │  │
    │ 60% rev   │ │20% rev│ │12%rev│ │ 8% rev    │  │
    └───────────┘ └───────┘ └──────┘ └───────────┘  │
                                                     │
                    ┌────────────────────────────────┘
                    │ VALIDATOR ECONOMICS (PRIM TOKEN)
                    │ Block rewards, staking yields,
                    │ gas fees — separate from protocol revenue
                    └────────────────────────────────
```

---

## 2. Key Assumptions

### Market Assumptions

| Assumption | Value | Source |
|-----------|-------|--------|
| RWA tokenization TAM (2030) | $16T | BCG, McKinsey |
| On-chain credit TAM (2030) | $2.5T+ | BIS, World Economic Forum |
| Combined TAM (2030) | $18.5T+ | Composite |
| Annual growth rate (2024–2030) | ~85% CAGR | BCG estimates |
| Mersennet market penetration (Year 3) | 0.14% of TAM | Conservative |

### Business Assumptions

| Assumption | Conservative | Base Case | Optimistic |
|-----------|-------------|-----------|------------|
| Year 1 Institutional Partners | 2 | 3 | 5 |
| Year 2 Institutional Partners | 8 | 12 | 20 |
| Year 3 Institutional Partners | 20 | 30 | 50 |
| Avg. TVA per Partner (Year 1) | $100M | $167M | $200M |
| Avg. TVA per Partner (Year 2) | $250M | $417M | $500M |
| Avg. TVA per Partner (Year 3) | $500M | $833M | $1B |
| Trading Volume / TVA ratio | 0.5%/day | 1%/day | 1.5%/day |
| Average fee rate | 0.15% | 0.20% | 0.25% |

---

## 3. Revenue Projections — Base Case

### 3.1 Total Value of Assets (TVA) Growth

```
        TVA GROWTH TRAJECTORY (Base Case)
        
        $100B ┤                                              ╱
              │                                           ╱
              │                                        ╱
        $75B  ┤                                     ╱
              │                                  ╱
              │                               ╱
        $50B  ┤                            ╱
              │                         ╱
              │                      ╱
        $25B  ┤                   ╱
              │                ╱
              │         ╱──╱
        $5B   ┤      ╱
              │   ╱
        $500M ┤╱
              └──────┬────────┬────────┬────────┬────────
                   Year 1   Year 2   Year 3   Year 4   Year 5
```

| Metric | Year 1 | Year 2 | Year 3 | Year 4 | Year 5 |
|--------|--------|--------|--------|--------|--------|
| Institutional Partners | 3 | 12 | 30 | 50 | 80 |
| TVA | $500M | $5B | $25B | $100B | $300B |
| Daily Trading Volume | $5M | $50M | $250M | $1B | $3B |
| Annual Trading Volume | $1.83B | $18.25B | $91.25B | $365B | $1.1T |

### 3.2 Revenue Breakdown

| Revenue Stream | Year 1 | Year 2 | Year 3 | Year 4 | Year 5 |
|---------------|--------|--------|--------|--------|--------|
| **Trading Fees** (0.2% avg) | $3.65M | $36.5M | $182.5M | $730M | $2.19B |
| **Infrastructure Licensing** | $2.5M | $20M | $85M | $240M | $500M |
| **API Access** | $1.2M | $14M | $75M | $140M | $250M |
| **Data & Analytics** | $1.15M | $14.5M | $82.5M | $90M | $160M |
| **Total Revenue** | **$8.5M** | **$85M** | **$425M** | **$1.2B** | **$3.1B** |

### 3.3 Revenue by Scenario

```
       REVENUE SCENARIOS (5 Year)
       
       $5B   ┤                                              ╱ Optimistic
             │                                           ╱
             │                                    ╱───╱
       $3B   ┤                              ╱──╱        Base Case
             │                         ╱──╱
             │                    ╱──╱
       $1B   ┤              ╱──╱
             │         ╱──╱                            ╱ Conservative
             │    ╱──╱                          ╱───╱
       $100M ┤╱─╱                         ╱──╱
             │                       ╱──╱
             │                  ╱──╱
       $10M  ┤╱────────────╱──╱
             └──────┬────────┬────────┬────────┬────────
                  Year 1   Year 2   Year 3   Year 4   Year 5
```

| Scenario | Year 1 | Year 2 | Year 3 | Year 5 |
|----------|--------|--------|--------|--------|
| Conservative | $4M | $40M | $200M | $1.5B |
| **Base Case** | **$8.5M** | **$85M** | **$425M** | **$3.1B** |
| Optimistic | $15M | $150M | $900M | $5B+ |

---

## 4. Cost Structure

### 4.1 Operating Expenses (Base Case)

| Category | Year 1 | Year 2 | Year 3 |
|----------|--------|--------|--------|
| **Engineering** | $4.5M | $8M | $12M |
| - Core Protocol (5 → 10 engineers) | $2.5M | $5M | $7M |
| - Security & Audits | $1M | $1.5M | $2M |
| - Developer Tools & SDKs | $1M | $1.5M | $3M |
| **Business Development** | $3M | $5M | $8M |
| - Partnership Team | $1.5M | $2.5M | $4M |
| - Regulatory Engagement | $1M | $1.5M | $2M |
| - Pilot Programs | $500K | $1M | $2M |
| **Operations** | $2.5M | $5M | $8M |
| - Team & HR | $1.5M | $3M | $5M |
| - Infrastructure (servers, cloud) | $500K | $1M | $2M |
| - Legal & Compliance | $500K | $1M | $1M |
| **Ecosystem** | $1.5M | $3M | $5M |
| - Developer Grants | $750K | $1.5M | $3M |
| - Hackathons & Events | $500K | $1M | $1.5M |
| - Marketing & Community | $250K | $500K | $500K |
| **Total OpEx** | **$11.5M** | **$21M** | **$33M** |

### 4.2 Team Growth Plan

```
       TEAM SIZE PROJECTION
       
       100  ┤                                              ●
            │                                        ╱
            │                                  ╱───╱
        60  ┤                            ●───╱
            │                      ╱───╱
            │                ╱───╱
        35  ┤          ●───╱
            │    ╱───╱
            │╱──╱
        15  ●
            └──────┬────────┬────────┬────────
                 Year 1   Year 2   Year 3   Year 4
```

| Role | Year 1 | Year 2 | Year 3 |
|------|--------|--------|--------|
| Engineering | 8 | 18 | 30 |
| Business Development | 3 | 8 | 12 |
| Operations & Admin | 2 | 5 | 10 |
| Legal & Compliance | 1 | 2 | 4 |
| DevRel & Community | 1 | 2 | 4 |
| **Total** | **15** | **35** | **60** |

### 4.3 Profitability Path

| Metric | Year 1 | Year 2 | Year 3 |
|--------|--------|--------|--------|
| Revenue | $8.5M | $85M | $425M |
| OpEx | $11.5M | $21M | $33M |
| EBITDA | -$3M | $64M | $392M |
| EBITDA Margin | -35% | 75% | 92% |
| Cash Position (incl. raise) | $22M | $86M | $478M |

**Breakeven:** Month 14–16 (Q2 Year 2), when infrastructure licensing and trading fees cross OpEx.

---

## 5. Unit Economics

### 5.1 Per-Partner Economics

| Metric | Year 1 Avg. | Year 2 Avg. | Year 3 Avg. |
|--------|-------------|-------------|-------------|
| TVA per partner | $167M | $417M | $833M |
| Annual revenue per partner | $2.83M | $7.08M | $14.17M |
| Cost to acquire (CAC) | $500K | $300K | $200K |
| Annual retention rate | 95% | 97% | 98% |
| Lifetime value (LTV) | $28M | $71M | $142M |
| **LTV / CAC** | **56x** | **237x** | **710x** |

### 5.2 Per-Trade Economics

| Metric | Value |
|--------|-------|
| Average trade size | $50,000 |
| Average fee rate | 0.20% |
| Revenue per trade | $100 |
| Marginal cost per trade | ~$0.01 (gas + compute) |
| **Gross margin per trade** | **99.99%** |

### 5.3 Infrastructure Licensing Economics

| Metric | Value |
|--------|-------|
| Annual license fee per client | $1–5M |
| Setup cost | $100K |
| Ongoing support cost | $200K/year |
| Gross margin | 90%+ |
| Typical contract length | 3 years |

---

## 6. Funding Waterfall

### $25M Use Over 24 Months

```
       Month:  1   3   6   9   12  15  18  21  24
               │   │   │   │   │   │   │   │   │
   Engineering ████████████████████████████████████  $8.75M
               Mainnet dev → Security audit → Optimization
               
   Business Dev ██████████████████████████████████  $7.0M
                Partnerships → Pilots → Scale
                
   Operations   ████████████████████████████████    $5.0M
                Key hires → Build team → Scale ops
                
   Ecosystem    ██████████████████████████          $3.0M
                Grants → Hackathons → DApp growth
                
   Reserves     ████████                            $1.25M
                Strategic optionality
```

### Monthly Burn Rate Progression

| Period | Monthly Burn | Cumulative Spend | Cash Remaining |
|--------|-------------|-----------------|----------------|
| Months 1–6 | $750K | $4.5M | $20.5M |
| Months 7–12 | $1.17M | $11.5M | $13.5M |
| Months 13–18 | $1.25M | $19M | $6M + Revenue |
| Months 19–24 | $1.33M | $27M | Revenue-funded |

**Breakeven:** Month 14–16, after which operations are funded by revenue.

---

## 7. Valuation Framework

### 7.1 Comparable Transaction Analysis

| Project | Stage | Raise | FDV | Revenue Multiple | TPS | CLOB |
|---------|-------|-------|-----|-----------------|-----|------|
| Hyperliquid | TGE | Community | $4.5B | 9x ann. revenue | ~100K | Native (async) |
| Monad | Series A | $225M | $3.0B | N/A (pre-rev) | ~10K | None |
| Berachain | Series A | $69M | $1.5B | N/A (pre-rev) | ~5K | None |
| Sei | Series A | $30M | $800M | 16x ann. revenue | ~5K | Deprecated |
| dYdX | Series C | $65M | $2.0B | 10x ann. revenue | App-chain | Native |

### 7.2 Implied Valuation Range

| Method | Implied FDV | Rationale |
|--------|-------------|-----------|
| Comparable raises (pre-revenue L1s) | $150M–$300M | Average pre-revenue L1 raises at $800M–$3B FDV |
| Comparable revenue multiples (Year 2) | $425M–$850M | 5–10x Year 2 projected revenue of $85M |
| Discounted Hyperliquid | $300M–$500M | Hyperliquid at $4.5B; Mersennet has better tech, earlier stage |
| Sum-of-parts (tech + partnerships) | $200M–$400M | Engineering moat + institutional partnerships |

**Suggested valuation:** $150M–$250M pre-money (attractive entry for $25M raise at 10–17% dilution).

### 7.3 Return Scenarios for $25M Investment

| Scenario | Year 3 FDV | Return Multiple | IRR |
|----------|-----------|----------------|-----|
| Conservative (10x Year 3 rev) | $2B | 8–13x | 100%+ |
| Base Case (15x Year 3 rev) | $6.4B | 26–43x | 200%+ |
| Optimistic (20x Year 3 rev) | $18B | 72–120x | 350%+ |

---

## 8. Sensitivity Analysis

### Revenue Sensitivity to Key Variables

| Variable | -50% | Base | +50% | Impact on Year 3 Revenue |
|----------|------|------|------|-------------------------|
| Number of Partners | 15 | 30 | 45 | $213M / $425M / $638M |
| Avg. TVA per Partner | $417M | $833M | $1.25B | $213M / $425M / $638M |
| Avg. Fee Rate | 0.10% | 0.20% | 0.30% | $213M / $425M / $638M |
| Trading Volume / TVA | 0.5%/day | 1%/day | 1.5%/day | $213M / $425M / $638M |

### Downside Case (Everything Goes Wrong)

| Assumption | Downside | Impact |
|-----------|----------|--------|
| Only 1 major partner (no Binance, no XDC) | TVA: $200M Year 1 | Revenue: $2M Year 1 |
| Regulatory delays (6-month setback) | Launch pushed to Q2 2027 | 6 months additional burn |
| Fee compression (0.05% avg) | Revenue cut by 75% | Breakeven pushed to Month 24 |
| **Combined downside** | | **$25M provides 30+ months runway, enough to pivot** |

---

## 9. Key Financial Metrics Summary

```
╔══════════════════════════════════════════════════════════════╗
║                                                              ║
║   KEY METRICS AT A GLANCE                                    ║
║                                                              ║
║   Year 1 Revenue:        $8.5M        Partners: 3            ║
║   Year 2 Revenue:        $85M         Partners: 12           ║
║   Year 3 Revenue:        $425M        Partners: 30           ║
║                                                              ║
║   Gross Margin:          85–92%                              ║
║   Breakeven:             Month 14–16                         ║
║   Burn Rate (Year 1):    ~$960K/month                        ║
║   Runway (at raise):     24+ months                          ║
║                                                              ║
║   LTV/CAC:               56x (Year 1) → 710x (Year 3)       ║
║   Revenue/Employee:      $567K (Y1) → $7.1M (Y3)            ║
║   Gross Margin/Trade:    99.99%                              ║
║                                                              ║
║   Suggested FDV:         $150M–$250M pre-money               ║
║   Dilution:              10–17%                              ║
║   Target Return:         26–43x (Base Case, Year 3)          ║
║                                                              ║
╚══════════════════════════════════════════════════════════════╝
```

---

*This financial model contains forward-looking projections based on management estimates and market research. Actual results may differ materially. This document is confidential and for qualified investors only.*
