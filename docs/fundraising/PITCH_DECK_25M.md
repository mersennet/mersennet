# MERSENNET — $25M Series A Pitch Deck

> **Confidential** | March 2026 | For Qualified Investors Only

---

## Slide 1: Cover

```
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║                        ◆ MERSENNET ◆                           ║
║                                                                  ║
║     The Institutional Trading Layer for Real-World Assets         ║
║              + On-Chain Credit Markets                            ║
║                                                                  ║
║    ─────────────────────────────────────────────────────          ║
║    The Only L1 with Atomic EVM ↔ CLOB Composability              ║
║                                                                  ║
║    Series A  ·  $25M  ·  March 2026                              ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
```

**Speaker Notes:**
- Open with the category-defining line: "We are the only Layer 1 blockchain where a Solidity smart contract can trade on a native order book in the same transaction."
- Pause. Let that sink in. No one else can say this — not Ethereum, not Solana, not even Hyperliquid.

---

## Slide 2: The Problem — The $18.5 Trillion Infrastructure Gap

```
┌─────────────────────────────────────┐
│     TRADITIONAL FINANCE              │         EXISTING BLOCKCHAINS
│     ════════════════════             │         ════════════════════
│     $18.5T in assets ready           │         ❌ 12s finality (Ethereum)
│     to be tokenized                  │         ❌ No native order matching
│     ───────────────────              │         ❌ MEV & front-running
│     Bonds · Real Estate              │         ❌ No margin/liquidation
│     Commodities · Credit             │         ❌ Not audit-ready
│     Trade Finance · PE               │
│                                      │
│         "READY TO GO"                │         "CAN'T HANDLE IT"
└─────────────────────────────────────┘

         ┌─────────────────────────────────────────┐
         │                                         │
         │      THE GAP: $18.5T BLOCKED BY         │
         │      MISSING INFRASTRUCTURE              │
         │                                         │
         └─────────────────────────────────────────┘
```

### The Four Walls Blocking Institutional Adoption

| Wall | Problem | Impact |
|------|---------|--------|
| **Speed** | 12-second finality (Ethereum) | Institutions need sub-second for trading |
| **Fairness** | Smart contract DEXs = MEV, front-running | $1.2B+ extracted annually via MEV |
| **Compliance** | Probabilistic execution = unauditable | Regulators cannot approve |
| **Infrastructure** | No native margin, liquidation, positions | Every institution must build from scratch |

**Key Stat:** BlackRock, Fidelity, and Standard Chartered are tokenizing billions — but have nowhere to trade them with institutional-grade infrastructure.

---

## Slide 3: The Solution — Mersennet

```
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║    ┌──────────┐    ┌──────────────┐    ┌──────────┐              ║
║    │   EVM    │◄──►│  PrimeOrders │◄──►│  Bridge  │              ║
║    │ (revm)   │    │   (CLOB)     │    │ (X-Chain)│              ║
║    └──────────┘    └──────────────┘    └──────────┘              ║
║         │                 │                  │                    ║
║         └─────────┬───────┘──────────────────┘                   ║
║                   │                                              ║
║           ┌───────▼───────┐                                      ║
║           │  SINGLE ATOMIC │                                     ║
║           │     STATE      │                                     ║
║           │  S = (Sevm,     │                                    ║
║           │   Sorders,     │                                     ║
║           │   Sbridge)     │                                     ║
║           └───────────────┘                                      ║
║                                                                  ║
║    PRECOMPILE 0x0100: Solidity trades on CLOB in same tx         ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
```

### What Makes Us The Only One

| Capability | How It Works | Why It Matters |
|-----------|-------------|----------------|
| **Native CLOB** | Order matching at consensus level, not smart contracts | Provably fair, deterministic, no MEV |
| **Atomic Composability** | EVM ↔ CLOB in the SAME transaction via precompile `0x0100` | DeFi smart contracts trade on institutional order books |
| **~200ms Finality** | HotStuff-2 BFT consensus | 60x faster than Ethereum |
| **Built-in Risk Mgmt** | Margin, liquidation, position tracking native | No more duct-taping Aave + Uniswap |
| **Full EVM** | revm v12, Shanghai spec | All Ethereum tooling works day one |

---

## Slide 4: Why Now — The Perfect Storm

```
    2020        2022        2024        2026        2028        2030
     │           │           │           │           │           │
     ▼           ▼           ▼           ▼           ▼           ▼
   ┌───┐      ┌───┐      ┌─────┐    ┌───────┐   ┌────────┐  ┌──────────┐
   │$5B│      │$50B│     │$310B│    │$1.5T  │   │ $6T    │  │ $18.5T   │
   └───┘      └───┘      └─────┘    └───────┘   └────────┘  └──────────┘

   DeFi        NFT         RWA        RWA          RWA         RWA+Credit
   Summer     Boom       Tokenization Institutional  Scale       Maturity
                          Begins      Adoption

         ══════════════════════════════════════════════════
                     50x GROWTH IN 6 YEARS
         ══════════════════════════════════════════════════
```

### Three Converging Forces

| Force | Evidence | Timeline |
|-------|----------|----------|
| **Regulatory Clarity** | MiCA (EU), SEC tokenized securities guidance, Basel III credit rules | 2024–2026 |
| **Institutional Entry** | BlackRock BUIDL fund ($500M+), Fidelity tokenized funds, Standard Chartered RWA division | Active now |
| **Infrastructure Demand** | Every major bank building tokenization — none have trading infra | Gap widening |

**The Window:** First blockchain to provide compliant institutional trading infrastructure captures the category. That window is 12–18 months.

---

## Slide 5: Market Opportunity — $18.5T+ TAM by 2030

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│    $18.5T+ TOTAL ADDRESSABLE MARKET BY 2030                  │
│                                                              │
│    ┌────────────────────────────────────────────┐            │
│    │          RWA TRADING: $16T                  │            │
│    │  ┌─────────┐ ┌──────┐ ┌──────┐ ┌────────┐ │            │
│    │  │Tokenized│ │ Real │ │Funds │ │ Trade  │ │            │
│    │  │ Bonds   │ │Estate│ │      │ │Finance │ │            │
│    │  │ $1.3T   │ │$2.8T │ │ $5T  │ │ $500B  │ │            │
│    │  └─────────┘ └──────┘ └──────┘ └────────┘ │            │
│    └────────────────────────────────────────────┘            │
│                                                              │
│    ┌─────────────────────────────────┐                       │
│    │   ON-CHAIN CREDIT: $2.5T+       │                       │
│    │  ┌─────────┐ ┌──────┐ ┌──────┐ │                       │
│    │  │Corporate│ │Trade │ │Lend- │ │                       │
│    │  │ Credit  │ │Fin.  │ │ ing  │ │                       │
│    │  │ $500B   │ │ $1T  │ │$450B │ │                       │
│    │  └─────────┘ └──────┘ └──────┘ │                       │
│    └─────────────────────────────────┘                       │
│                                                              │
│    TODAY: $310B  ────────────►  2030: $18.5T+  (50x)         │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Market Segments (Conservative Penetration Assumptions)

| Segment | Global Market | Tokenization % | Addressable Market |
|---------|--------------|----------------|--------------------|
| Bonds | $130T | 1% | $1.3T |
| Real Estate | $280T | 1% | $2.8T |
| Funds (ETF/Mutual) | $50T | 10% | $5.0T |
| Trade Finance | $5T | 10% | $500B |
| Commodities | $5T | 5% | $250B |
| Private Equity | $6T | 5% | $300B |
| **RWA Subtotal** | | | **$10.15T** |
| Corporate Credit | $10T | 5% | $500B |
| Trade Finance Credit | $5T | 20% | $1.0T |
| Institutional Lending | $15T | 3% | $450B |
| Credit Derivatives | $15T | 3% | $350B |
| **Credit Subtotal** | | | **$2.3T** |
| **TOTAL TAM** | | | **$18.5T+** |

**Source:** BCG, McKinsey, BIS, World Economic Forum estimates

---

## Slide 6: Technology — Why We Win

```
┌──────────────────────────────────────────────────────────────┐
│                    PERFORMANCE BENCHMARKS                     │
│                                                              │
│   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│   │   72,181      │  │   2,484,170   │  │    ~200ms    │      │
│   │   EVM TPS     │  │   CLOB ops/s  │  │   Finality   │      │
│   │              │  │              │  │              │      │
│   │  ████████████│  │  ████████████│  │  ████████████│      │
│   │  ████████████│  │  ████████████│  │  ████████████│      │
│   └──────────────┘  └──────────────┘  └──────────────┘      │
│                                                              │
│   Ethereum: 15      Hyperliquid:       Ethereum: 12,000ms   │
│   Solana: 4,000     200K ops/s         Solana: 400ms        │
│   Monad: 10,000     dYdX: 100K ops/s   Hyperliquid: 200ms   │
│                                                              │
│   ═══════════════════════════════════════════════════════     │
│   ONLY L1 WITH ALL THREE: HIGH TPS + FAST CLOB + LOW        │
│   FINALITY IN A SINGLE ATOMIC STATE                          │
│   ═══════════════════════════════════════════════════════     │
└──────────────────────────────────────────────────────────────┘
```

### The CLOB Precompile: Our Unfair Advantage

```solidity
// Any Solidity contract can do this — ON NO OTHER CHAIN
function tradeOnOrderBook(uint256 marketId, int256 qty, uint256 price) external {
    // This call hits the native CLOB at precompile 0x0100
    // Fill happens ATOMICALLY in the same transaction
    (bool success, bytes memory result) = address(0x0100).call(
        abi.encode(marketId, qty, price)
    );
    // Smart contract can immediately use the fill result
    // DeFi + TradFi in one atomic operation
}
```

**Translation for Non-Technical Investors:** Imagine if Uniswap could trade on the NYSE order book in the same transaction — that's what we built, but it's native to the chain.

---

## Slide 7: Competitive Landscape — The Only Complete Solution

```
                        NATIVE         ATOMIC
                        CLOB      EVM↔CLOB     EVM     FINALITY    RWA
                        ────      ────────     ───     ────────    ───
  Ethereum               ❌          ❌         ✅      12,000ms    ❌
  Solana                  ❌          ❌         ❌        400ms     ❌
  Hyperliquid             ✅       ❌ ASYNC      ⚠️        200ms     ❌
  Monad                   ❌          ❌         ✅       ~1,000ms   ❌
  Sei v2                  ⚠️          ❌         ✅        400ms     ❌
  dYdX                    ✅          ❌         ❌        ~1s       ❌
  ─────────────────────────────────────────────────────────────────────
  MERSENNET             ✅       ✅ ATOMIC     ✅        200ms     ✅
```

### Head-to-Head: Mersennet vs. Hyperliquid (Our Closest Competitor)

| Dimension | Hyperliquid | Mersennet | Winner |
|-----------|------------|-------------|--------|
| CLOB Speed | 200K ops/s | **2.4M ops/s** | **Mersennet (12x)** |
| EVM ↔ CLOB | Async (CoreWriter, seconds delay, stale reads) | **Atomic (same transaction)** | **Mersennet** |
| EVM Maturity | HyperEVM (alpha, limited) | **Full revm v12, Shanghai** | **Mersennet** |
| Use Case | Crypto perps/derivatives | **RWA + Credit + DeFi** | **Mersennet (bigger TAM)** |
| Institutional | No compliance features | **Deterministic, auditable** | **Mersennet** |
| DeFi Composability | Limited (async boundary) | **Full (atomic boundary)** | **Mersennet** |

**The Punchline:** Hyperliquid trades crypto derivatives. We trade the entire $18.5T institutional finance stack.

### Why Replication Is Hard

Native order matching requires **consensus-level integration** — you can't bolt it on top. Ethereum would need a hard fork. Solana would need a new VM. This is a 2+ year engineering moat.

---

## Slide 8: Traction & Partnerships

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│      STANDARD         XDC            BINANCE      GREG       │
│      CHARTERED        NETWORK                     KIDD       │
│      ──────────       ────────       ────────     ─────      │
│      RWA + Credit     Trade Finance  Exchange     Advisor    │
│      Pilot Q1 2026    Integration    Infrastructure Coinbase │
│      $3–5M invest     $2M invest     $5–7M invest  $1M      │
│      Regulatory       Asian markets  Global reach  Network   │
│      introductions    Developer eco  Market making  access   │
│                                                              │
│      ═══════════════════════════════════════════════          │
│      COMBINED STRATEGIC VALUE: $11–15M + MARKET ACCESS       │
│      ═══════════════════════════════════════════════          │
│                                                              │
│      TESTNET LIVE (Chain ID 131071):                           │
│      ✅ 4-Validator Network  ✅ PrimeScan Explorer            │
│      ✅ PrimeSwap DEX        ✅ PrimeNodes Dashboard          │
│      ✅ Faucet               ✅ Documentation Portal           │
│      ✅ Grafana Monitoring   ✅ WebSocket RPC                  │
│      ✅ 7 Smart Contracts    ✅ 3 Liquidity Pools              │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Products Built & Ready

| Product | Type | Status | Deployment |
|---------|------|--------|------------|
| PrimeScan Explorer | Block Explorer | **Live on testnet** | Production |
| PrimeSwap DEX | AMM (UniV2) | **Live on testnet** | Production |
| PrimeNodes | Validator Dashboard | **Live on testnet** | Production |
| PrimeFi | Lending (Aave-style) | **Built** | Ready to deploy |
| Primeport | NFT Marketplace (Seaport) | **Built** | Ready to deploy |
| PrimeXDC Wallet | Browser Extension + Mobile | **Built** | Ready to deploy |
| xdc-markets | Prediction Markets | **Built** | Ready to deploy |
| Liquid Staking | LST Contracts | **Built** | Ready to deploy |
| Developer SDKs | Python, Go, TypeScript | **Built** | Ready to deploy |

**12 products built.** 6 live on testnet. That's more ecosystem at pre-mainnet than most chains at launch.

---

## Slide 9: Use Cases — Real Applications, Real Revenue

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│   USE CASE 1: STANDARD CHARTERED TOKENIZED BONDS             │
│   ──────────────────────────────────────────────             │
│                                                              │
│   TODAY:                          WITH MERSENNET:           │
│   ┌──────────┐                    ┌──────────────┐           │
│   │ $100M    │  T+2 settlement    │ $100M        │           │
│   │ Corporate│  High fees ($$$)   │ Corporate    │  Instant  │
│   │ Bond     │  Market hours only │ Bond         │  settle   │
│   │ Issue    │  No composability  │ Issue        │  24/7     │
│   └──────────┘                    └──────────────┘  DeFi     │
│                                                     composable│
│   Settlement: 2 days              Settlement: 200ms           │
│   Fee: $500K+                     Fee: <$100                  │
│   Transparency: Limited           Transparency: Full on-chain │
│   Secondary market: Illiquid      Secondary: Order book       │
│                                                              │
│   ═══════════════════════════════════════════════════════     │
│   SAVINGS: 99.98% cost reduction · 864,000x faster           │
│   ═══════════════════════════════════════════════════════     │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Revenue-Generating Use Cases

| Use Case | Partner | Revenue Model | Year 1 Est. |
|----------|---------|---------------|-------------|
| Tokenized Bond Trading | Standard Chartered | 0.1–0.3% per trade | $2M |
| Trade Finance Settlement | XDC Network | 0.1% per settlement | $1M |
| Institutional Credit Trading | Standard Chartered | 0.2–0.5% per trade | $1.5M |
| White-label Matching Engine | Binance | License + rev share | $3M |
| Premium API Access | Institutions | Monthly subscription | $500K |
| Market Data & Analytics | All partners | Data licensing | $500K |

---

## Slide 10: Business Model — Multiple Revenue Streams

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│                    REVENUE ENGINE                             │
│                                                              │
│   ┌─────────────────┐   ┌─────────────────┐                 │
│   │  TRADING FEES    │   │  INFRASTRUCTURE  │                │
│   │  0.1–0.5%/trade  │   │  LICENSING       │                │
│   │  ██████████████  │   │  ████████████    │                │
│   │  60% of revenue  │   │  20% of revenue  │                │
│   └─────────────────┘   └─────────────────┘                 │
│                                                              │
│   ┌─────────────────┐   ┌─────────────────┐                 │
│   │  API ACCESS      │   │  DATA &          │                │
│   │  Premium tier    │   │  ANALYTICS       │                │
│   │  ████████        │   │  ██████          │                │
│   │  12% of revenue  │   │  8% of revenue   │                │
│   └─────────────────┘   └─────────────────┘                 │
│                                                              │
│   YEAR 1        YEAR 2        YEAR 3        YEAR 4           │
│   $8.5M         $85M          $425M         $1.2B            │
│   ████           ████████      █████████████  █████████████   │
│   3 partners     12 partners   30 partners   50+ partners    │
│   $500M TVA      $5B TVA       $25B TVA      $100B TVA       │
│                                                              │
│   TVA = Total Value of Assets traded on Mersennet          │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Unit Economics

| Metric | Year 1 | Year 2 | Year 3 |
|--------|--------|--------|--------|
| Tokenized Assets (TVA) | $500M | $5B | $25B |
| Avg. Daily Trading Volume | $5M | $50M | $250M |
| Avg. Fee Rate | 0.2% | 0.18% | 0.15% |
| Trading Fee Revenue | $3.65M | $32.85M | $136.9M |
| Infrastructure Licensing | $2.5M | $25M | $100M |
| API & Data Revenue | $2.35M | $27.15M | $188.1M |
| **Total Revenue** | **$8.5M** | **$85M** | **$425M** |
| Gross Margin | 85% | 88% | 92% |
| Partners | 3 | 12 | 30 |

---

## Slide 11: Tokenomics — Sound Economic Design

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│         MRSN TOKEN — 1 BILLION MAX SUPPLY                    │
│                                                              │
│   ┌─────────────────────────────────────────────────┐        │
│   │                                                 │        │
│   │   ██████████████████████████████████████ 70%    │ Block  │
│   │   Block Rewards (700M MRSN)              Rewards│        │
│   │                                                 │        │
│   │   ██████ 10%  Ecosystem & Grants (100M)         │        │
│   │   ██████ 10%  Foundation Reserve (100M)         │        │
│   │   ███  5%     Team & Contributors (50M)         │        │
│   │   ███  5%     Sales — Private + Public (50M)    │        │
│   │                                                 │        │
│   └─────────────────────────────────────────────────┘        │
│                                                              │
│   HALVING SCHEDULE (Bitcoin-style):                          │
│   Era 0: 10 MRSN/block  ────►  50% emitted by Year 2.2      │
│   Era 1:  5 MRSN/block  ────►  75% emitted by Year 4.4      │
│   Era 2: 2.5 MRSN/block ────►  87.5% emitted by Year 6.7    │
│   Era 3: 1.25 MRSN/block ───►  99% emitted by Year 13       │
│                                                              │
│   VESTING:                                                   │
│   Team:  1y cliff + 3y linear                                │
│   Sales: 6m cliff + 18m linear                               │
│   Foundation: 1y cliff + 4y linear                           │
│   Ecosystem: 5y linear from TGE                              │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### Comparison to Peer L1 Tokenomics

| Chain | Supply | Block Rewards | Team | Investors | Community | 99% Emitted |
|-------|--------|--------------|------|-----------|-----------|-------------|
| Bitcoin | 21M | 100% | 0% | 0% | 0% | ~25 years |
| Avalanche | 720M | 50% | 20% | 4% | 19% | ~10 years |
| Celestia | 1B | Inflation | 17.6% | 35.6% | 26.8% | Perpetual |
| Aptos | ~1.1B | Inflation | 19% | 49% | 32% | Perpetual |
| Sui | 10B | Inflation | 14% | 48% | 38% | Perpetual |
| **MRSN** | **1B** | **70%** | **5%** | **5%** | **20%** | **~7 years** |

**Key Insight:** MRSN has the lowest team + investor allocation (10%) of any major L1 launched in the last 3 years. 70% goes to validators. This is a network-first, community-first token design.

---

## Slide 12: Roadmap — Path to $1B+ TVA

```
     Q1 2026              Q2 2026              Q3 2026              Q4 2026
    ┌──────────┐         ┌──────────┐         ┌──────────┐         ┌──────────┐
    │ PROOF OF │─────────│VALIDATION│─────────│  SCALE   │─────────│ MAINNET  │
    │ CONCEPT  │         │          │         │          │         │ LAUNCH   │
    └──────────┘         └──────────┘         └──────────┘         └──────────┘

    ✅ SC pilot           ✅ Live RWA          ✅ Public testnet    ✅ Mainnet
      commitment            trading              (15+ validators)    (Chain 8191)
    ✅ XDC partnership    ✅ Regulatory         ✅ Developer eco    ✅ Institutional
    ✅ Greg Kidd            feedback             growth               trading live
      advisory            ✅ 3+ institutional  ✅ Security audit   ✅ $100M+ TVA
    ✅ $5M strategic        partners              passed
      investment          ✅ $15M raised       ✅ Bridge live      ✅ Governance
    ✅ Testnet live        ✅ Credit pilot      ✅ $25M closed        portal

    ─────────────────────────────────────────────────────────────────────────
    $500M TVA             $5B TVA              $25B TVA             $100B+ TVA
    (Year 1)              (Year 2)             (Year 3)             (Year 4)
```

### 2026 Milestones (Mapped to Funding)

| Quarter | Technical | Business | Funding |
|---------|-----------|----------|---------|
| Q1 | Consensus over P2P, block timestamps, RPC completion | SC pilot, XDC partnership | $5M strategic |
| Q2 | Indexer, bridge (testnet↔Sepolia), SDKs published | Live RWA + Credit trading, regulatory feedback | $15M cumulative |
| Q3 | Public testnet (15+ validators), security audit | Developer ecosystem, DeFi integrations | $25M closed |
| Q4 | Mainnet launch (Chain ID 8191), genesis | Institutional trading live, $100M+ TVA | Strategic partnerships |

---

## Slide 13: Team & Advisors

```
┌──────────────────────────────────────────────────────────────┐
│                                                              │
│                      CORE TEAM                               │
│                                                              │
│   Rodolfo Cova — Founder & Lead Engineer                     │
│   ─────────────────────────────────────                      │
│   Full-stack blockchain architect. Built Mersennet from     │
│   scratch: 44,000+ lines of Rust, 6-crate workspace.           │
│   Complete L1 with EVM, CLOB, consensus, P2P, RPC.           │
│                                                              │
│                   STRATEGIC ADVISORS                          │
│                                                              │
│   ┌─────────────────────────────────────────────────┐        │
│   │                                                 │        │
│   │  Greg Kidd           Standard Chartered VP      │        │
│   │  ──────────          ──────────────────         │        │
│   │  Early Coinbase      RWA tokenization leader    │        │
│   │  investor            Regulatory relationships   │        │
│   │  Ripple, Square      FCA, MAS, HKMA access      │        │
│   │  Global ID founder   Institutional network      │        │
│   │                                                 │        │
│   │  XDC Network Core    Binance Leadership         │        │
│   │  ──────────────      ─────────────────          │        │
│   │  Trade finance       Exchange infrastructure    │        │
│   │  blockchain          Global liquidity           │        │
│   │  Asian market access Institutional clients      │        │
│   │                                                 │        │
│   └─────────────────────────────────────────────────┘        │
│                                                              │
│   PLANNED HIRES WITH FUNDING:                                │
│   • CTO (protocol eng.) • Head of BD (institutional)         │
│   • VP Engineering (5 engineers) • General Counsel            │
│   • Head of Compliance • DevRel Lead                         │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## Slide 14: The Ask — $25M Series A

```
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║                    $25,000,000                                    ║
║                    SERIES A                                       ║
║                                                                  ║
║   ┌────────────────────────────────────────────────────┐         ║
║   │                                                    │         ║
║   │   ████████████████████████  35%  Engineering       │         ║
║   │   $8.75M                                           │         ║
║   │   Protocol development, security audits,           │         ║
║   │   performance optimization, developer tools        │         ║
║   │                                                    │         ║
║   │   ██████████████████  28%  Business Development    │         ║
║   │   $7M                                              │         ║
║   │   Institutional partnerships, regulatory           │         ║
║   │   engagement, pilot programs                       │         ║
║   │                                                    │         ║
║   │   █████████████  20%  Operations & Team            │         ║
║   │   $5M                                              │         ║
║   │   Key hires, infrastructure, legal/compliance      │         ║
║   │                                                    │         ║
║   │   ████████  12%  Ecosystem & Grants                │         ║
║   │   $3M                                              │         ║
║   │   Developer grants, hackathons, integrations       │         ║
║   │                                                    │         ║
║   │   ███  5%  Reserves                                │         ║
║   │   $1.25M                                           │         ║
║   │   Contingency, strategic opportunities             │         ║
║   │                                                    │         ║
║   └────────────────────────────────────────────────────┘         ║
║                                                                  ║
║   FUNDING STRUCTURE:                                             ║
║   ──────────────────                                             ║
║   Tier 1 — Strategic Partners:     $11–15M                       ║
║     Standard Chartered: $3–5M                                    ║
║     Binance Labs:       $5–7M                                    ║
║     XDC Foundation:     $2–3M                                    ║
║                                                                  ║
║   Tier 2 — Crypto VCs:            $8–12M                         ║
║     Hard Yaka (Greg Kidd): $1–2M                                 ║
║     Polychain / Paradigm / a16z: $5–8M                           ║
║     Additional VCs: $2–3M                                        ║
║                                                                  ║
║   Tier 3 — Strategic Reserve:      $2–3M                         ║
║     Traditional finance investors (post-pilot)                   ║
║                                                                  ║
║   RUNWAY: 24+ months at full burn rate                           ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
```

### What $25M Buys

| Investment | Deliverable | Timeline |
|-----------|-------------|----------|
| $8.75M Engineering | Mainnet launch, security audit, bridge, SDKs | 12 months |
| $7M Business Dev | 10+ institutional partners, $5B+ TVA pipeline | 18 months |
| $5M Operations | 15-person team, legal structure, compliance | 6 months |
| $3M Ecosystem | 100+ developers, 20+ DApps, grants program | 12 months |
| $1.25M Reserves | Strategic flexibility, runway extension | 24 months |

### Return Expectations

| Scenario | TVA Year 3 | Revenue Year 3 | Implied Network Value |
|----------|-----------|----------------|----------------------|
| Conservative | $10B | $200M | $2B+ |
| Base Case | $25B | $425M | $5B+ |
| Optimistic | $50B | $900M | $10B+ |

---

## Slide 15: Why Mersennet — The Investment Thesis

```
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║     5 REASONS TO INVEST IN MERSENNET                           ║
║                                                                  ║
║     1. ONLY L1 WITH ATOMIC EVM ↔ CLOB                           ║
║        2+ year engineering moat. Can't be bolted on.             ║
║                                                                  ║
║     2. $18.5T MARKET, 50x GROWTH                                 ║
║        Institutional RWA + Credit = largest crypto TAM.          ║
║                                                                  ║
║     3. STANDARD CHARTERED + BINANCE + XDC                        ║
║        Not cold outreach. Active relationships, pilot-ready.     ║
║                                                                  ║
║     4. BUILT, NOT PLANNED                                        ║
║        Testnet live. 12 products built. 72K TPS measured.        ║
║                                                                  ║
║     5. CATEGORY-DEFINING TIMING                                  ║
║        12–18 month window to own "institutional trading L1."     ║
║                                                                  ║
║     ═══════════════════════════════════════════════════           ║
║                                                                  ║
║     "The NYSE of tokenized assets — built on blockchain."        ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
```

---

## Slide 16: Contact & Next Steps

```
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║                        ◆ MERSENNET ◆                           ║
║                                                                  ║
║     The Institutional Trading Layer for                           ║
║     Real-World Assets + On-Chain Credit Markets                  ║
║                                                                  ║
║     ─────────────────────────────────────────                    ║
║                                                                  ║
║     🌐  docs.primechain.network                                  ║
║     📧  [founders@primechain.network]                            ║
║     𝕏   [@MersennetHQ]                                         ║
║     📍  GitHub: PrimeNumbersLabs                                 ║
║                                                                  ║
║     ─────────────────────────────────────────                    ║
║                                                                  ║
║     TESTNET LIVE:  Chain ID 131071                                 ║
║     Explorer:      primescan.io                                  ║
║     DEX:           primeswap.io                                  ║
║     Docs:          docs.primechain.network                       ║
║                                                                  ║
║     NEXT STEPS:                                                  ║
║     1. Technical deep-dive call                                  ║
║     2. Live testnet demo                                         ║
║     3. Partner pilot discussion                                  ║
║     4. Term sheet review                                         ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
```

---

## Appendix A: Technical Architecture Deep Dive

### State Model

```
S = (S_evm, S_orders, S_bridge)

Where:
  S_evm    = Ethereum-compatible state (accounts, storage, code)
  S_orders = PrimeOrders state (markets, order books, positions, margins)
  S_bridge = Cross-chain bridge state (queues, proofs)

All three domains share a SINGLE Merkle root.
Atomic commits across all domains in every block.
```

### Consensus: HotStuff-2 BFT

- **2-phase commit** (vs. 3-phase in classical PBFT): fewer round trips
- **Pipeline mode**: overlapping phases for throughput
- **Threshold:** >2/3 stake to finalize (tolerates <1/3 Byzantine)
- **Finality:** Immediate, irreversible (~200ms)

### MEV Protection: Frequent Batch Auctions (FBA)

- Orders collected in discrete time intervals
- Matched simultaneously at uniform clearing price
- Eliminates front-running, sandwich attacks, and order manipulation
- 5,053,782 FBA ops/s measured

---

## Appendix B: Comparable Transactions & Valuations

| Project | Last Round | Valuation | Revenue | Comparable Metric |
|---------|-----------|-----------|---------|-------------------|
| Hyperliquid | TGE (Nov 2024) | $4.5B FDV | ~$500M annualized fees | CLOB chain, no atomic EVM |
| dYdX | Series C ($65M) | $2.0B FDV | ~$200M annualized | App-chain, no EVM |
| Monad | Series A ($225M) | $3.0B FDV | Pre-revenue | Parallel EVM, no CLOB |
| Sei | Series A ($30M) | $800M FDV | ~$50M annualized | Deprecated DEX module |
| Berachain | Series A ($69M) | $1.5B FDV | Pre-revenue | DeFi-native, no CLOB |

**Mersennet at $25M raise:** Early-stage pricing for a project that combines the best features of Hyperliquid (native CLOB) + Monad (parallel EVM) + institutional focus no one else has.

---

## Appendix C: Risk Factors & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Regulatory uncertainty | High | Early FCA/MAS/SEC engagement via Standard Chartered |
| Technical execution | Medium | Testnet live, 72K TPS proven, 241 tests passing |
| Competition | Medium | 2+ year technical moat (consensus-level CLOB) |
| Market timing | Medium | Multiple use cases (RWA + Credit + DeFi), flexible positioning |
| Adoption | Medium | Strategic partners (SC, XDC, Binance) provide guaranteed early TVA |
| Security | Medium | Planned audit Q3 2026, bug bounty, formal verification |

---

*This document is confidential and intended solely for the use of the intended recipient(s). This is not an offer to sell or a solicitation of an offer to buy any securities. Any investment in digital assets involves a high degree of risk.*
