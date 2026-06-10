# Competitive Analysis: Native Order Book Blockchains

**Prepared for Mersennet Development Team**
**Date: March 2026**

---

## Executive Summary

This document analyzes the three most significant production blockchain projects that implement native order books — dYdX v4, Sei Network, and Hyperliquid — and extracts architectural patterns, lessons learned, and specific recommendations for Mersennet. Each represents a distinct design philosophy, and Mersennet's unified EVM+PrimeOrders architecture can learn from both their successes and their limitations.

**Key finding:** The industry is converging on a consensus that high-performance trading requires order books outside the traditional EVM execution path, but disagrees on how tightly coupled the order book should be with general-purpose smart contracts. Mersennet's approach of embedding both in a single state and block is architecturally novel and, if executed well, addresses the primary weakness of every competitor analyzed.

---

## 1. dYdX v4 (dYdX Chain)

**Repository:** https://github.com/dydxprotocol/v4-chain
**Stack:** Cosmos SDK + CometBFT, written in Go
**Status:** Production mainnet since October 2023

### 1.1 Architecture Overview

dYdX v4 is a standalone Cosmos SDK L1 chain built specifically for perpetual futures trading. The core innovation is a **decentralized, in-memory order book** maintained by each validator node, with only matched trades committed to consensus.

**Three-tier architecture:**
- **Validators:** Maintain in-memory order books, gossip transactions, produce blocks via weighted round-robin
- **Full Nodes:** Process blocks without consensus participation, feed the Indexer
- **Indexer:** Read-only service (Postgres + Redis + Kafka) serving data via WebSocket/REST

### 1.2 The x/clob Module Design

The `x/clob` module is the centerpiece, organized into these subpackages:

| Package | Purpose |
|---------|---------|
| `memclob/` | In-memory price-time priority order book (`MemClobPriceTimePriority`) |
| `keeper/` | State management, order persistence, liquidation logic |
| `types/` | Proto-generated types, interfaces, error definitions |
| `ante/` | Transaction validation (AnteHandler decorators) |
| `rate_limit/` | Order submission rate limiting |
| `mev_telemetry/` | MEV detection and metrics |
| `e2e/` | End-to-end tests |

**The `MemClobPriceTimePriority` struct** is the core matching engine:

```go
type MemClobPriceTimePriority struct {
    orderbooks             map[types.ClobPairId]*Orderbook
    operationsToPropose    types.OperationsToPropose
    clobKeeper             types.MemClobKeeper
    generateOffchainUpdates bool
    generateOrderbookUpdates bool
}
```

Each `Orderbook` maintains:
- `Bids` and `Asks` as `map[Subticks]*Level` (hash maps keyed by price level, not sorted trees)
- `BestBid` and `BestAsk` as cached `uint64` values for O(1) top-of-book access
- `orderIdToLevelOrder` for O(1) order lookup by ID
- `SubaccountOpenClobOrders` tracking per-subaccount open orders
- `blockExpirationsForOrders` for GTB (Good-Til-Block) expiry management
- `orderIdToCancelExpiry` and `cancelExpiryToOrderIds` for cancel tracking

### 1.3 Order State Management: The Dual-State Model

**This is dYdX's most important architectural decision and the one most relevant to Mersennet.**

dYdX separates orders into two categories with fundamentally different state management:

#### Short-Term Orders (Off-Chain)
- Live only in validator memory for up to 20 blocks
- Only fill amounts and expiry committed to on-chain state
- Designed for market makers / high-frequency traders
- Do NOT survive network restarts
- Placed optimistically during CheckTx by the block proposer
- Higher throughput, lower latency

#### Stateful Orders (On-Chain)
- Fully persisted in the blockchain state
- Survive validator restarts (reconstructed into memory from state)
- Placed in block N+1 if MsgPlaceOrder is in block N
- Designed for retail traders and long-lived limit orders
- Lower throughput, consensus-speed placement

**Mersennet implication:** Mersennet's PrimeOrders currently treats all orders uniformly. dYdX's dual-state approach is a pragmatic optimization — short-term orders never touch consensus, dramatically reducing state bloat and increasing throughput. Mersennet should consider a similar tiered model, but the advantage of Mersennet's unified state is that it can offer stronger guarantees than dYdX's "optimistic" matching for short-term orders.

### 1.4 How Orders Interact with Consensus

The ABCI lifecycle in dYdX v4 is highly customized:

1. **`PreBlocker`**: Initializes the CLOB keeper for the new block
2. **`BeginBlocker`**: Resets process-proposer-match events and delivered order IDs
3. **`PrepareProposal`** (implicit): Block proposer extracts matched operations from their local `MemClob` and includes them in the proposed block
4. **`ProcessProposal`**: Validators verify the proposed matches against their local state
5. **`EndBlocker`**: Prunes expired orders, triggers conditional orders, generates TWAP suborders, triggers liquidation checks
6. **`PrepareCheckState`** (post-commit): The most complex phase — replays local operations, purges invalid state, places stateful orders from last block, runs liquidations and deleveraging

The `PrepareCheckState` function is particularly noteworthy — it runs a 9-step pipeline:
1. Remove local operations queue from memclob
2. Purge invalid memclob state (filled orders, expired orders, cancelled orders)
3. First pass: place post-only orders from last block
4. Place stateful long-term orders from last block
5. Place conditional orders triggered in last block
6. Replay local validator's operations onto the book
7. Identify and attempt to liquidate undercollateralized subaccounts
8. Deleverage subaccounts that can't be liquidated
9. Gate withdrawals if negative-TNC subaccounts exist

**Mersennet implication:** dYdX's approach of replaying local state against committed state is necessary because each validator's order book can diverge. Mersennet's deterministic matching within a single block avoids this complexity, but must ensure the matching engine is fast enough to handle the load that dYdX offloads to the off-chain memclob.

### 1.5 Matching Algorithm

dYdX uses price-time priority matching identical in concept to Mersennet's PrimeOrders, but with several important implementation details:

- **Branched context for matching:** Uses `ctx.CacheContext()` to create a branched state, only writing if matching succeeds. This ensures atomic matching — if any step fails (collateralization check, etc.), all state changes are discarded.
- **Collateralization checks during matching:** Each fill is validated against the subaccount's collateral in real-time, not just at order placement.
- **Reduce-only order handling:** After matching, the engine checks if position sign has flipped and cancels reduce-only orders accordingly.
- **Replacement orders:** Instead of cancel+place (which can cause double-fills), dYdX supports atomic order replacement via same OrderId with higher GTB.
- **Self-trade prevention:** Built into the matching loop — orders from the same subaccount cannot match against each other.

### 1.6 MEV Protection

dYdX has evolved through multiple MEV mitigation strategies:

#### Current: Social Mitigation
- MEV telemetry module monitors proposer behavior
- Validators can be socially slashed for MEV extraction
- Limited effectiveness

#### Proposed: Collaborative Block Building + FBA
- Uses Cosmos ABCI++ vote extensions for collaborative block building
- **Frequent Batch Auctions (FBA):** Orders grouped into batches, matched at a uniform clearing price, eliminating sequence-dependence
- **Validator voting on inclusion:** Validators vote on order hashes before block production; proposer can only include orders backed by >50% stake weight
- **Deterministic results:** Since matching is deterministic given the included orders, any validator can verify the result
- **Key properties:**
  - No inclusion MEV (proposers can't unilaterally control which orders are included)
  - No reordering MEV (order sequence doesn't affect matching outcome in FBA)
  - Reduced colocation advantage (must disseminate to majority, not just proposer)

**Mersennet implication:** Mersennet currently lacks any MEV protection mechanism. This is a critical gap. The FBA approach is particularly relevant because it's compatible with CLOB-style matching. Mersennet should implement either (a) FBA-style batch auctions per block, (b) ABCI-style collaborative block building, or (c) a threshold encryption scheme where order contents are encrypted until committed. Without MEV protection, block proposers in Mersennet have enormous power to front-run, sandwich, or censor trades.

### 1.7 Margin and Liquidation System

dYdX's risk system operates through:

- **Subaccounts:** Each user can have multiple subaccounts with isolated margin
- **Maintenance margin checks:** Continuous monitoring via daemon processes
- **Insurance fund:** Absorbs losses from underwater liquidations (cross-insurance fund balance tracked as a metric)
- **Deleveraging:** When insurance fund is insufficient, opposing positions are force-closed at bankruptcy price
- **Withdrawal gating:** If any subaccount has negative Total Net Collateral (TNC), withdrawals for ALL users are temporarily disabled — a drastic but effective safety mechanism
- **Liquidation fillable price:** Protocol-generated liquidation orders match at a calculated price, with up to 1.5% maximum penalty

### 1.8 Performance Optimizations

Recent optimizations (2025) include:
- **Memory pooling** for CLOB module objects (Order, ClobOrder, LevelOrder, MakerFill, MakerFillWithOrder) — achieved **30-40% latency reduction** with zero allocations in critical matching paths
- **Hash maps instead of sorted trees** for price levels (with cached best bid/ask) — O(1) lookup vs O(log n) for BTreeMap
- **Separate data structures for different concerns** — order-to-level mapping, block expiry tracking, cancel tracking all use dedicated maps
- **Telemetry and metrics** throughout the pipeline for monitoring

---

## 2. Sei Network

**Repository:** https://github.com/sei-protocol/sei-chain
**Stack:** Cosmos SDK + Modified Tendermint, written in Go
**Status:** Production mainnet (evolved from Sei v1 DEX-focused to Sei v2 parallelized EVM)

### 2.1 Architecture Overview

Sei represents an interesting case study because it **pivoted** from being a DEX-focused chain (Sei v1) with a built-in `x/dex` module to becoming a general-purpose parallelized EVM chain (Sei v2). This pivot itself contains lessons for Mersennet.

**Sei v1:** Built-in native order matching engine with CosmWasm integration
**Sei v2:** Parallelized EVM + CosmWasm with native optimizations, deprecated the built-in DEX module in favor of allowing DEXs to be built as smart contracts on the optimized runtime

### 2.2 Twin Turbo Consensus

Sei's primary innovation is its consensus optimization suite, not a novel algorithm but aggressive optimizations to Tendermint BFT:

#### Pipelined Consensus & Execution
- Transaction execution overlaps with BFT voting rounds
- Validators don't wait for prevote/precommit to complete before starting execution
- Uses `ProcessTXsWithOCC` and `DeliverTxBatch` for concurrent processing

#### Pre-Consensus Transaction Preparation
- Before block proposals, validators concurrently:
  - Collect and decode transactions (`DecodeTransactionsConcurrently`)
  - Analyze state dependencies (`GenerateEstimatedWritesets`)
  - Pre-fetch required state data from SeiDB

#### Aggressive Timeout Tuning
- `UnsafeProposeTimeoutOverride` and `UnsafeCommitTimeoutOverride` with much shorter durations than standard Tendermint
- Reduces standard ~6s Tendermint block times to ~400ms
- Single-slot finality

#### Result: ~400ms Block Times
- 2000x faster than Ethereum
- Deterministic finality within 1-2 blocks (~400-800ms)
- Multi-step DeFi operations (approve + swap + deposit) complete in ~1.2 seconds

**Mersennet implication:** Sei's consensus optimizations are directly applicable to Mersennet's BFT consensus. The key techniques — pipelining execution with voting, pre-consensus transaction preparation, and aggressive timeout tuning — should all be implemented. Mersennet's whitepaper targets "sub-second finality" but doesn't describe the specific optimizations to achieve it. Sei proves that aggressive Tendermint tuning can achieve 400ms blocks.

### 2.3 Parallel Order Processing (Optimistic Parallelization)

Sei's parallelization engine uses optimistic concurrency control (OCC):

1. **Multiple worker goroutines** process transactions concurrently
2. **`CacheMultiStore`** provides state buffering — each goroutine reads/writes to an isolated cache
3. **Conflict detection** at commit time — if two transactions touch the same state, one is re-executed
4. **Deterministic ordering** maintained despite parallel execution — results are the same as sequential execution

For order processing specifically:
- Orders that touch different markets can be processed in parallel
- Orders within the same market must be serialized (price-time priority requires sequential processing)
- Conflict detection catches any violations

**Mersennet implication:** Mersennet should implement parallel processing for independent markets. The key insight is that orders in BTC-PERP and ETH-PERP are independent and can be matched concurrently. Only when cross-market operations occur (e.g., portfolio margin checks) do they need to synchronize. PrimeOrders should be designed with per-market parallelism from the start.

### 2.4 Order Matching: Frequent Batch Auctions (FBA)

Sei v1's `x/dex` module used Frequent Batch Auctions as its primary matching mechanism:

#### How FBA Works
1. Orders are collected during a time window (one block period)
2. All orders in the batch are processed simultaneously, not one-by-one
3. A uniform clearing price is calculated to maximize traded volume
4. Orders whose limit prices cross the clearing price are filled
5. At the same price, orders participate pro-rata (not time-priority)

#### MEV Protection Properties
- **No frontrunning:** Since all orders in a batch execute at the same price, seeing an order before the batch closes doesn't help
- **No sandwich attacks:** The uniform clearing price makes sandwiching impossible
- **Sequence-independent:** The order in which transactions arrive doesn't affect matching
- **Inter-block MEV prevented:** Batch boundaries naturally limit MEV extraction windows

#### Sei's MEV Bifurcation
Sei distinguishes between:
- **"Bad" MEV** (frontrunning by validators/bots): Eliminated by FBA
- **"Good" MEV** (liquidations, arbitrage): Maximized through private, off-chain blind auctions similar to Flashbots' MEV-Boost

**Mersennet implication:** Mersennet's current price-time-priority matching is the industry standard but is vulnerable to MEV. Consider offering BOTH matching modes: standard CLOB for markets that need continuous price discovery, and FBA for markets where frontrunning protection is more important than order-by-order priority. This gives market creators flexibility.

### 2.5 CosmWasm Integration (The x/dex Module)

Sei v1's approach to composability:
- DEX module accessible via CosmWasm bindings (`sei-cosmwasm` crate)
- Custom messages: `PlaceOrders`, `CancelOrders`
- Custom queries: `DexTwaps`, `GetOrders`, `GetLatestPrice`, `GetOrderById`
- Order books organized by contract address + price denom + asset denom
- Multi-market support with shared liquidity

**Why Sei deprecated x/dex:** Sei's pivot to v2 (parallelized EVM) suggests the team concluded that a built-in DEX module was too limiting. By making the EVM fast enough, they could let DEXs be built as smart contracts, gaining more flexibility at the cost of some performance. This is the opposite of Mersennet's approach.

**Mersennet implication:** Sei's pivot is a cautionary tale but not necessarily applicable to Mersennet. Sei deprecated x/dex because they wanted to be a general-purpose chain — DEX was just one use case. Mersennet's thesis is that order matching is a first-class citizen alongside EVM, not an add-on. The key lesson is that the DEX module must be genuinely better than what smart contracts can do (in performance, features, or guarantees) to justify its existence as a native module.

### 2.6 SeiDB

Sei built a custom storage layer optimized for high-throughput state access:
- High I/O capabilities for rapid state commits
- Optimized for the read/write patterns of order book state
- State prefetching integrated with consensus pipeline

**Mersennet implication:** Mersennet uses `sled` as its storage backend. For order book workloads (many small writes, range queries on price levels), storage performance is critical. Consider whether sled's characteristics match order book access patterns, or whether a custom storage layer would be beneficial.

---

## 3. Hyperliquid

**Stack:** Custom L1, HyperBFT consensus, state transition logic in pure Rust
**Status:** Production mainnet, highest-volume on-chain perp DEX
**Open Source:** Limited — core trading engine is closed source

### 3.1 Architecture Overview

Hyperliquid is a purpose-built L1 for derivatives trading, later expanded with HyperEVM for smart contracts. Two environments under one consensus:

- **HyperCore:** Native trading engine — order books, matching, margin, liquidations
- **HyperEVM:** EVM-compatible smart contract environment (Cancun spec)
- **HyperBFT:** Custom delegated PoS consensus (Tendermint-derived), tolerates 1/3 Byzantine

### 3.2 Pure Rust State Transition

Unlike dYdX (Go) and Sei (Go), Hyperliquid implements all state transition logic in **pure Rust**, without relying on Cosmos SDK or other frameworks. This choice:
- Eliminates framework overhead
- Allows aggressive low-level optimizations
- Draws from HFT system design patterns
- Achieves significantly higher performance

**Performance figures:**
- 200,000 operations/second (orders, cancels, liquidations)
- Sub-200ms median end-to-end latency (for co-located clients)
- 0.9s 99th percentile latency
- Vanilla Tendermint comparison: ~1,000 TPS, 2-5x longer latency

**Mersennet implication:** Mersennet is also written in Rust with `revm` for EVM execution. This is a strong foundation. Hyperliquid proves that Rust + purpose-built consensus can dramatically outperform Go/Cosmos SDK implementations. Mersennet should leverage this advantage.

### 3.3 Fully On-Chain Order Book

Unlike dYdX (which keeps orders in-memory/off-chain), Hyperliquid commits **every order, cancel, and trade** to on-chain state:

- Each asset has its own on-chain order book
- Prices snap to discrete tick increments; quantities to fixed lot sizes
- Price-time priority matching (same as traditional CLOBs)
- Deterministic, censorship-resistant execution through consensus

#### Semantic-Aware Mempool

A unique Hyperliquid innovation: the mempool and consensus layer understand order book transaction types. Within a block, actions are sorted:

1. **First:** Actions that send at least one GTC or IOC order
2. **Second:** Cancels
3. **Third:** Actions that don't send orders to any book

Within each category, the original proposer ordering is preserved. Modifies are categorized according to the new order they place.

**Mersennet implication:** This is directly applicable. Mersennet should implement semantic-aware transaction ordering within blocks. By processing orders before cancels, and cancels before non-order transactions, the matching engine can operate more efficiently and provide better execution quality. This is simple to implement and provides immediate benefits.

### 3.4 Margin System

Hyperliquid's margin handling:
- Margin checks at order opening AND at each match
- This ensures margining consistency despite oracle price fluctuations between order placement and fill
- Critical for resting orders that may sit on the book for extended periods

**Mersennet implication:** Mersennet's whitepaper describes margin checks but doesn't specify whether they occur at placement only or also at match time. Hyperliquid's approach of checking at BOTH times is more robust and should be adopted. Oracle prices can move significantly between when a resting order is placed and when it's eventually filled.

### 3.5 HyperCore <> HyperEVM Integration

This is the area most relevant to Mersennet's cross-domain bridge.

#### Read Path (Synchronous)
Smart contracts can query HyperCore state via precompiled contracts at `0x...0800`:
- Perpetual positions
- Spot balances
- Oracle prices
- Staking data
- Order book depth

#### Write Path (Asynchronous)
Smart contracts send actions to HyperCore via `CoreWriter` system contract at `0x3333...3333`:
- Supports limit orders, vault transfers, staking, delegation
- **Actions are delayed a few seconds** to prevent latency advantages
- EVM transaction succeeds immediately, but HyperCore execution happens in the next L1 block
- EVM transaction does NOT revert if the orderbook action fails

#### Dual-Block Architecture
HyperEVM uses a two-tier block system:
- **Large blocks:** ~1 minute, 30M gas, for complex operations
- **Small blocks:** ~1 second, 2M gas, for quick transactions

#### Asset Transfers
- HyperEVM → HyperCore: Immediate (same L1 block)
- HyperCore → HyperEVM: Queued for next EVM block
- No wrapped tokens — same asset exists in both environments

**Critical limitation:** The async write path means smart contracts CANNOT atomically compose with order book operations. A smart contract can't place an order and react to the fill result in the same transaction. This is the primary weakness Hyperliquid acknowledges, and it's exactly what Mersennet's cross-domain bridge is designed to solve.

**Mersennet implication:** Mersennet's bridge providing atomic cross-domain calls is a genuine competitive advantage over Hyperliquid's async model. However, Mersennet must ensure the bridge doesn't introduce excessive latency. The ideal design allows a smart contract to:
1. Read order book state (synchronous)
2. Place orders and receive fill results (synchronous, within the same block)
3. React to fills with further smart contract logic (atomic)

This "synchronous composability" is Mersennet's single biggest differentiator.

### 3.6 Performance Engineering

Hyperliquid's performance comes from:
- **No framework overhead:** Pure Rust, no Cosmos SDK
- **Optimized consensus:** Tuned Tendermint achieving ~200ms finality
- **Efficient data representation:** Custom serialization, minimal allocations
- **No gas fees** during initial phase (enabled by surplus throughput)
- **Semantic block ordering:** Transaction type awareness in consensus

### 3.7 Security and Decentralization Concerns

- Only 23 validators (as of 2025)
- Hyper Foundation controls 78.54% of staked tokens
- 2-of-3 multisig for bridged assets
- Closed-source trading engine limits auditability

**Mersennet implication:** Mersennet should aim for greater decentralization from launch. A larger validator set and open-source codebase are competitive advantages.

### 3.8 HyperEVM Deep Dive (Updated March 2026)

This section provides a detailed technical analysis of Hyperliquid's HyperEVM, the EVM layer added to complement their native HyperCore trading engine. Understanding HyperEVM's architecture and limitations is critical for positioning Mersennet's unified execution model.

#### HyperEVM Architecture

Hyperliquid uses a **dual-execution model** under HyperBFT consensus:

- **HyperCore:** Native CLOB execution environment — order books, matching, margin, liquidations. Achieves ~200K ops/s.
- **HyperEVM:** Cancun-spec EVM for smart contracts.
- **Execution order:** HyperCore and HyperEVM run **sequentially**, not in parallel. They are **NOT** the same execution environment — state is bridged between two distinct runtimes.

This separation is fundamental. Unlike Mersennet's unified EVM+PrimeOrders architecture, Hyperliquid maintains two execution silos that communicate asynchronously.

#### Dual-Block Architecture

HyperEVM uses a two-tier block system with different latency characteristics:

| Block Type | Interval | Gas Limit | Use Case |
|------------|----------|-----------|----------|
| **Big blocks** | ~1 minute | 30M gas | Complex operations, batch processing |
| **Small blocks** | ~1 second | 2M gas | Quick transactions, low-latency flows |

**Implication:** EVM transaction latency varies significantly depending on which block type processes the transaction. Users cannot rely on consistent sub-second execution.

#### Read Precompiles (0x111...111)

Smart contracts can read HyperCore state via precompiled contracts at `0x1111...1111`:

- Oracle prices
- Spot balances
- Perpetual positions
- Order book depth

**Critical limitation:** Read data is from the **PREVIOUS block only**. Data is **1 block stale**. A contract cannot read the current block's order book state — it sees the state as of the prior HyperCore block. This prevents real-time composability.

#### CoreWriter (0x333...333)

The CoreWriter system contract at `0x3333...3333` allows smart contracts to **write** to HyperCore:

- Place limit orders
- Cancel orders
- Vault transfers
- Staking, delegation

**How it works:** Actions are **queued for the NEXT HyperCore block**. Execution is **intentionally delayed by several seconds** to prevent latency arbitrage. The EVM transaction succeeds immediately; the order book action executes asynchronously in a future block.

**Gas cost:** ~47K gas per CoreWriter call.

#### Critical Limitations

| Limitation | Impact |
|------------|--------|
| **No fill result in same tx** | Contract cannot know if order filled; must poll or use events in a later block |
| **No atomic vault rebalance** | Cannot do vault rebalance + place order + get fill + callback in one atomic flow |
| **Account init timing** | Account must exist on HyperCore before EVM block — initialization ordering issues |
| **Alpha status** | Write precompiles still evolving; API may change |
| **47K gas per call** | Non-trivial cost for high-frequency strategies |

#### Comparison: HyperEVM vs Mersennet Precompile

| Capability | Hyperliquid HyperEVM | Mersennet Precompile |
|------------|----------------------|------------------------|
| **Place order from Solidity** | Yes (via CoreWriter) | Yes (at 0x0100) |
| **Read current book** | No (1 block stale) | Yes (same-block state) |
| **Fill result in same tx** | No | Yes |
| **Vault rebalance + order + callback** | No (async, multi-block) | Yes (atomic) |
| **Smart contract market maker** | Limited (no fill feedback) | Full (atomic feedback loop) |
| **Gas cost** | ~47K per CoreWriter call | Lower (single precompile path) |

#### Why Mersennet Wins

Mersennet's precompile at `0x0100` executes in the **SAME transaction**:

1. Place order
2. Get fill result
3. React to fill (e.g., adjust position, trigger callback)
4. All in one atomic call

This is **atomic composability**. Hyperliquid's model is **asynchronous composability** — next-block execution with seconds of delay. This is a **structural architectural difference** that Hyperliquid cannot fix without rebuilding their dual-execution model. Their design choice (sequential execution, intentional write delay) is baked into the consensus and block structure.

#### Updated Mersennet Benchmarks (March 2026)

| Component | Throughput | Notes |
|-----------|------------|-------|
| **EVM** | 72,181 TPS | Measured, release mode |
| **CLOB** | 2,484,170 match ops/s | Measured |
| **FBA** | 5,053,782 ops/s | Measured |
| **HotStuff-2** | 0.001ms per round | Consensus |
| **Finality** | ~200ms | End-to-end |

---

## 4. General Patterns for On-Chain Order Books

### 4.1 State Management

| Approach | Used By | Pros | Cons |
|----------|---------|------|------|
| **In-memory only** (off-chain) | dYdX short-term orders | Highest performance, no state bloat | Orders lost on restart, non-deterministic across nodes |
| **Fully on-chain** | Hyperliquid, Mersennet | Deterministic, censorship-resistant, auditable | Higher state growth, consensus overhead |
| **Hybrid** | dYdX (short-term + stateful) | Best of both worlds | Complex state reconciliation logic |

**Recommendation for Mersennet:** Adopt a hybrid approach. Keep the on-chain order book as the source of truth (matching Mersennet's current design), but introduce a "fast path" for short-term orders that can be matched in-memory before being committed. This preserves determinism while improving latency.

### 4.2 Deterministic Matching Across Distributed Nodes

All three projects solve this differently:

- **dYdX:** Each node has its own order book; only the proposer's matches matter. Post-commit, all nodes replay against committed state. Non-deterministic order books, deterministic settlement.
- **Sei:** FBA ensures matching is independent of arrival order. Determinism comes from the batch semantics, not from synchronized order books.
- **Hyperliquid:** Fully on-chain order book. All nodes execute the same sequence of operations and arrive at the same state. True determinism.

**Mersennet's approach** (per the whitepaper) matches Hyperliquid's: all nodes process the same ordered sequence of operations deterministically. This is the strongest guarantee but requires the most consensus throughput. To make this work at scale, Mersennet needs:
1. Aggressive consensus optimization (like Sei's Twin Turbo)
2. Per-market parallelism (like Sei's OCC)
3. Efficient state management (like Hyperliquid's custom representation)

### 4.3 Risk Engine Integration

| Feature | dYdX | Hyperliquid | Mersennet (current) |
|---------|------|-------------|----------------------|
| Margin check at placement | Yes | Yes | Yes |
| Margin check at match | Yes (collateralization check during matching) | Yes (at each match) | Not specified |
| Insurance fund | Yes (cross-insurance fund) | Yes | Not described |
| Deleveraging | Yes (when insurance fund depleted) | Yes | Not described |
| Withdrawal gating | Yes (if negative TNC detected) | Unknown | Not described |
| Subaccounts | Yes (multiple per user) | Yes | Not described |
| Cross-margin | Yes | Yes | Not described |
| Isolated margin | Yes (isolated subaccounts) | Yes | Not described |

**Recommendation:** Mersennet's whitepaper describes a basic margin system (collateral, positions, PnL) but lacks several production-critical features:
1. **Insurance fund:** Must exist to absorb losses from liquidations where the position is underwater
2. **Deleveraging mechanism:** Needed when insurance fund is depleted — force-close opposing positions at bankruptcy price
3. **Withdrawal gating:** When any account has negative equity, restrict all withdrawals to prevent bank runs
4. **Subaccount support:** Allow users to isolate risk across multiple trading accounts
5. **Match-time margin checks:** Verify collateral at BOTH order placement and fill execution

### 4.4 Bridge Between Order Matching and Smart Contract Execution

| System | Composability Model | Atomicity | Latency |
|--------|---------------------|-----------|---------|
| dYdX v4 | None (CLOB isolated from EVM) | N/A | N/A |
| Hyperliquid | Read=sync, Write=async | Reads only | 1-2 blocks for writes |
| Sei v1 | CosmWasm bindings | Module-level | Same block |
| **Mersennet** | Cross-domain bridge with ordered queues | Full atomic | Same block |
| RISE (emerging) | EVM-native MarketCore | Full atomic (EVM) | Same tx |

**Mersennet's bridge is a genuine differentiator.** No production system currently offers truly atomic cross-domain composability between a native order book and EVM smart contracts. This is the feature that should be emphasized and perfected.

### 4.5 Order Book Data Structure Comparison

| System | Data Structure | Best Price Access | Insert | Cancel |
|--------|---------------|-------------------|--------|--------|
| dYdX | `map[Subticks]*Level` (hash map) + cached BestBid/BestAsk | O(1) cached | O(1) amortized | O(1) |
| Mersennet | `BTreeMap` (sorted tree) | O(1) (min/max) | O(log L) | O(log L + k) |
| Hyperliquid | Unknown (closed source), likely similar to traditional HFT | O(1) | O(1) | O(1) |

**Recommendation:** dYdX's approach of using hash maps with cached best prices is worth considering. For matching, you only need the best bid/ask (O(1) with caching), not a sorted view of all levels. BTreeMap's O(log L) insert is unnecessary overhead if you maintain best bid/ask invariants. However, BTreeMap provides better worst-case guarantees and simpler implementation for range queries (e.g., "get top 10 levels"). The choice depends on whether Mersennet prioritizes matching throughput or query flexibility.

---

## 5. Specific Recommendations for Mersennet

### 5.1 Implementation Status (Updated March 2026)

Many recommendations from this analysis have **already been implemented**. Below is the current status:

| Feature | Status | Notes |
|---------|--------|-------|
| **Parallel execution** | Done | Per-market parallelism implemented |
| **HotStuff-2 consensus** | Done | 0.001ms per round, ~200ms finality |
| **CLOB precompile (0x0100)** | Done | Atomic order placement + fill result in same tx |
| **FBA (Frequent Batch Auctions)** | Done | 5M+ ops/s measured |
| **Commit-reveal** | Done | MEV-resistant order submission |
| **Semantic-aware block ordering** | Done | Orders, cancels, other ops |
| **libmdbx storage** | Next | Replace sled for production I/O |
| **WebSocket API** | Next | Real-time order book / fill streaming |
| **Block pipeline** | Next | Overlap execution with consensus voting |
| **ZK proofs** | Next | Light client verification, privacy |

### 5.2 Critical Gaps to Address

1. **Insurance Fund and Deleveraging (HIGH PRIORITY)**
   - Design and implement an insurance fund funded by liquidation fees
   - Implement a deleveraging mechanism for when the insurance fund is insufficient
   - Implement withdrawal gating when negative equity is detected
   - This is non-negotiable for a production trading system

2. **Match-Time Margin Validation (MEDIUM PRIORITY)**
   - Add margin checks at fill time, not just placement time
   - Oracle prices can move between placement and fill, leaving the system exposed

3. **Subaccount Support (MEDIUM PRIORITY)**
   - Allow multiple isolated-margin sub-accounts per user
   - dYdX's subaccount model is well-proven

### 5.3 Patterns to Adopt (Remaining)

1. **Memory Pooling** (from dYdX)
   - Pool allocations for Order, Fill, and Level objects
   - dYdX achieved 30-40% latency reduction with this alone

2. **Branched State for Matching** (from dYdX)
   - Use copy-on-write state branching during matching
   - If any step fails (collateralization, self-trade), discard all changes
   - Ensures atomicity without complex rollback logic

3. **Reduce-Only Order Handling** (from dYdX)
   - Track position sign changes during matching
   - Automatically cancel reduce-only orders when positions flip
   - Critical for risk management

4. **Order Replacement** (from dYdX)
   - Support atomic order replacement (same ID, higher GTB/sequence number)
   - Prevents double-fill risk inherent in cancel+place

5. **Block Pipeline** (from Sei)
   - Execute transactions concurrently with consensus voting
   - Pre-fetch state during mempool processing
   - Further reduce block times beyond current ~200ms finality

### 5.4 Patterns to Avoid

1. **Fully Off-Chain Order Books** (dYdX's short-term orders)
   - While performant, they sacrifice determinism and auditability
   - Each validator's book diverges, requiring complex reconciliation
   - Mersennet's value proposition is deterministic on-chain matching — don't compromise this

2. **Async EVM-to-Orderbook Writes** (Hyperliquid's CoreWriter)
   - Breaking atomicity between smart contracts and order book operations negates the primary advantage of having both on the same chain
   - Mersennet's bridge must remain synchronous within a block

3. **Deprecating the Native Module** (Sei's pivot away from x/dex)
   - Sei abandoned its native DEX because they wanted to be general-purpose
   - Mersennet is purpose-built for trading+EVM — the native module IS the product
   - But the lesson stands: the native module must be significantly better than smart-contract alternatives

4. **Excessive Centralization** (Hyperliquid's 23 validators + foundation control)
   - Mersennet should target a larger validator set
   - Open-source everything from day one

### 5.5 Mersennet's Unique Advantages

Based on this analysis, Mersennet's genuine differentiators are:

1. **Atomic EVM ↔ Order Book Composability:** No competitor achieves this. dYdX has no EVM. Hyperliquid's writes are async. Sei deprecated its native DEX. Mersennet's bridge enabling synchronous cross-domain calls within a single block is architecturally unique.

2. **Single State Root:** All three domains (EVM, PrimeOrders, Bridge) share one canonical state root. This means light clients can verify order book state with the same proofs they use for EVM state — a significant advantage for cross-chain interoperability.

3. **Rust-Native Performance:** Like Hyperliquid, but with full EVM via revm. The Cosmos SDK overhead that limits dYdX and Sei doesn't apply.

4. **Open Source:** Unlike Hyperliquid's closed-source trading engine.

### 5.6 Prioritized Implementation Roadmap

Based on competitive analysis, the following order of implementation priorities is recommended. **Done** items have been implemented as of March 2026:

| Priority | Feature | Status | Competitor Reference | Impact |
|----------|---------|--------|----------------------|--------|
| P0 | Insurance fund + deleveraging | Next | dYdX, Hyperliquid | Production safety |
| P0 | MEV protection (semantic ordering, FBA, commit-reveal) | **Done** | Hyperliquid, Sei, dYdX | Fair trading |
| P0 | Match-time margin validation | Next | dYdX, Hyperliquid | Risk management |
| P1 | Memory pooling for matching objects | Next | dYdX PR #2860 | 30-40% latency improvement |
| P1 | Block pipeline (execution + consensus overlap) | Next | Sei Twin Turbo | Further latency reduction |
| P1 | Per-market parallel matching | **Done** | Sei OCC | Throughput scaling |
| P1 | Order replacement support | Next | dYdX | Double-fill prevention |
| P1 | CLOB precompile (atomic EVM ↔ order book) | **Done** | — | Mersennet differentiator |
| P1 | HotStuff-2 consensus | **Done** | — | ~200ms finality |
| P2 | Frequent Batch Auctions | **Done** | Sei, dYdX proposed | Stronger MEV protection |
| P2 | Subaccount support | Next | dYdX | User flexibility |
| P2 | libmdbx storage | Next | Sei SeiDB | I/O performance |
| P2 | WebSocket API | Next | dYdX Indexer | Real-time streaming |
| P2 | Conditional/triggered orders | Next | dYdX | Feature parity |
| P2 | TWAP orders | Next | dYdX | Institutional feature |
| P3 | Tiered order state (fast-path for short-term) | Next | dYdX short-term orders | Performance at scale |
| P3 | ZK proofs (light client, privacy) | Next | — | Verification, privacy |

---

## 6. Conclusion

The on-chain order book space is rapidly maturing. dYdX v4 has proven that a Cosmos-based CLOB can handle production volumes. Hyperliquid has shown that purpose-built Rust L1s can achieve CEX-grade performance. Sei's evolution demonstrates both the power and the risk of native trading modules.

Mersennet occupies a unique architectural position: a Rust-native L1 that combines full EVM compatibility with a deterministic order book and atomic cross-domain composability. No competitor currently offers all three. The key risks are:

1. **Execution risk:** The architecture is novel and unproven at scale (as the whitepaper honestly acknowledges)
2. **MEV vulnerability:** Without protection, the system will be exploited
3. **Missing safety infrastructure:** Insurance fund, deleveraging, and withdrawal gating are table stakes for production deployment

If these gaps are addressed, Mersennet's unified architecture represents the most compelling solution in the space — not just a trading chain or an EVM chain, but both in one.
