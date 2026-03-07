# Prime Chain — Architecture Decision Records

**Version 1.0 — March 2026**
**Classification: Technical Architecture Document**
**Audience: Engineering leadership, technical due diligence, protocol contributors**

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [System Architecture](#2-system-architecture)
3. [Architecture Decision Records](#3-architecture-decision-records)
   - [ADR-001: Multi-Domain State Model](#adr-001-multi-domain-state-model)
   - [ADR-002: Block-STM Parallel Execution](#adr-002-block-stm-parallel-execution)
   - [ADR-003: HotStuff-2 Consensus](#adr-003-hotstuff-2-consensus)
   - [ADR-004: CLOB Precompile at 0x0100](#adr-004-clob-precompile-at-0x0100)
   - [ADR-005: Frequent Batch Auctions](#adr-005-frequent-batch-auctions)
   - [ADR-006: Commit-Reveal MEV Protection](#adr-006-commit-reveal-mev-protection)
   - [ADR-007: revm for EVM Execution](#adr-007-revm-for-evm-execution)
   - [ADR-008: sled for Storage](#adr-008-sled-for-storage)
   - [ADR-009: Multi-Pool Mempool](#adr-009-multi-pool-mempool)
   - [ADR-010: Binary Merkle Tree for State Root](#adr-010-binary-merkle-tree-for-state-root)
   - [ADR-011: ECDSA secp256k1 for Transaction Signing](#adr-011-ecdsa-secp256k1-for-transaction-signing)
   - [ADR-012: Insurance Fund and Auto-Deleveraging](#adr-012-insurance-fund-and-auto-deleveraging)
   - [ADR-013: True Atomic vs. Async EVM ↔ CLOB Composability](#adr-013-true-atomic-vs-async-evm--clob-composability)
4. [Data Flow Diagrams](#4-data-flow-diagrams)
5. [Security Architecture](#5-security-architecture)
6. [Deployment Architecture](#6-deployment-architecture)
7. [Module Index](#7-module-index)

---

## 1. Executive Summary

Prime Chain is a **Layer 1 blockchain** that combines a parallel EVM execution engine with a native, high-performance Central Limit Order Book (CLOB). The core differentiator is **atomic EVM ↔ CLOB composability** through a custom revm precompile at address `0x0100`, enabling Solidity smart contracts to place orders, manage collateral, and query positions in a single transaction with no bridge latency.

**Competitive positioning:** Hyperliquid has HyperEVM (alpha) alongside its native CLOB, but EVM ↔ CLOB composability is **async** — CoreWriter actions are delayed by seconds, reads are 1 block stale. Prime Chain's CLOB precompile is the key architectural differentiator: **true atomic same-transaction EVM ↔ CLOB** — unique in the industry.

### Key Performance Characteristics

| Metric | Measured | Mechanism |
|---|---|---|
| EVM Throughput | 72,181 TPS | Parallel execution (Block-STM) on 8 cores |
| CLOB Operations | 2,484,170 ops/s | Native Rust matching engine (O(log n) BTreeMap) |
| FBA Throughput | 5,053,782 ops/s | Frequent batch auctions, clearing price |
| HotStuff-2 Round | 0.001 ms/round | Two-phase BFT, 2-chain commit |
| Finality | ~200 ms | HotStuff-2 consensus |
| Block Gas Limit | 30,000,000 | EIP-1559 fee market with dynamic base fee |

### Codebase Profile

| Dimension | Value |
|---|---|
| Language | Rust (2024 edition) |
| Source files | ~28 modules |
| Lines of code | ~10,600 |
| EVM backend | revm v12 (Shanghai spec) |
| Storage backend | sled 0.34 (embedded) |
| Cryptography | k256 0.13 (secp256k1 ECDSA) |

### Architectural Thesis

Existing blockchains force a choice: general-purpose smart contracts (Ethereum) **or** high-performance order matching (Hyperliquid). Multi-chain approaches (dYdX v4) lose atomic composability. Prime Chain resolves this by embedding both execution domains in a single state tuple:

```
S = (S_evm, S_orders, S_bridge)
```

All three domains share one consensus layer, one block structure, one state root, and one finality guarantee.

---

## 2. System Architecture

### 2.1 High-Level Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           PRIME CHAIN NODE                              │
│                                                                         │
│  ┌─────────────┐    ┌──────────────────────────────────────────────┐   │
│  │  JSON-RPC    │    │              ENGINE  (engine.rs)             │   │
│  │  Server      │───▶│                                              │   │
│  │  (tiny_http) │    │  ┌────────────┐  ┌────────────────────────┐ │   │
│  │              │    │  │ MEMPOOL    │  │ COMMIT-REVEAL POOL     │ │   │
│  │  Eth-compat: │    │  │ pending    │  │ commit()  → hash store │ │   │
│  │  eth_*       │    │  │ queued     │  │ reveal()  → tx data    │ │   │
│  │  prime_*     │    │  │ base_fee   │  │ window = 2 blocks      │ │   │
│  └─────────────┘    │  └─────┬──────┘  └────────────────────────┘ │   │
│                      │        │                                      │   │
│  ┌─────────────┐    │        ▼                                      │   │
│  │ PROMETHEUS  │    │  ┌─────────────────────────────────────────┐ │   │
│  │ /metrics    │    │  │         BLOCK PRODUCTION                 │ │   │
│  │ (port 9090) │    │  │                                         │ │   │
│  └─────────────┘    │  │  ┌───────────────┐ ┌─────────────────┐ │ │   │
│                      │  │  │ Sequential    │ │ Parallel        │ │ │   │
│  ┌─────────────┐    │  │  │ execute_block │ │ execute_block_  │ │ │   │
│  │ P2P NETWORK │    │  │  │               │ │ parallel        │ │ │   │
│  │             │    │  │  └───────┬───────┘ └────────┬────────┘ │ │   │
│  │ UDP Gossip  │    │  │          │                   │          │ │   │
│  │ (port 30303)│    │  │          ▼                   ▼          │ │   │
│  │             │    │  │  ┌─────────────────────────────────────┐│ │   │
│  │ TCP Sync    │    │  │  │    revm (Shanghai) + Precompile    ││ │   │
│  │ (blocks)    │    │  │  │    ┌─────────────────────────┐     ││ │   │
│  └──────┬──────┘    │  │  │    │ CLOB Precompile 0x0100  │     ││ │   │
│         │           │  │  │    │ placeOrder / cancel /    │     ││ │   │
│         │           │  │  │    │ deposit / withdraw /     │     ││ │   │
│         │           │  │  │    │ getPosition / liquidate  │     ││ │   │
│         │           │  │  │    └───────────┬─────────────┘     ││ │   │
│         │           │  │  └────────────────┼───────────────────┘│ │   │
│         │           │  │                   │                     │ │   │
│         │           │  │                   ▼                     │ │   │
│         │           │  │  ┌─────────────────────────────────────┐│ │   │
│         │           │  │  │         PrimeOrders State           ││ │   │
│         │           │  │  │  Markets │ OrderBooks │ Positions   ││ │   │
│         │           │  │  │  Collateral │ Margin │ Insurance    ││ │   │
│         │           │  │  └───────────────┬─────────────────────┘│ │   │
│         │           │  │                  │                      │ │   │
│         │           │  │  ┌───────────────┴─────────────────────┐│ │   │
│         │           │  │  │       FBA Engine (fba.rs)            ││ │   │
│         │           │  │  │  BatchAuction → clearing_price       ││ │   │
│         │           │  │  │  Uniform-price │ Pro-rata allocation ││ │   │
│         │           │  │  └─────────────────────────────────────┘│ │   │
│         │           │  │                                         │ │   │
│         │           │  │  ┌─────────────────────────────────────┐│ │   │
│         │           │  │  │      BRIDGE  (bridge.rs)             ││ │   │
│         │           │  │  │  orders_to_evm │ evm_to_orders       ││ │   │
│         │           │  │  │  VecDeque<BridgeMessage> (nonce seq) ││ │   │
│         │           │  │  └─────────────────────────────────────┘│ │   │
│         │           │  └─────────────────────────────────────────┘ │   │
│         │           │                                              │   │
│         │           │  ┌──────────────────────────────────────────┐│   │
│         │           │  │        CONSENSUS                         ││   │
│         ▼           │  │                                          ││   │
│  ┌─────────────┐    │  │  ┌──────────────┐  ┌──────────────────┐ ││   │
│  │ Import /    │    │  │  │ CometBFT-    │  │ HotStuff-2       │ ││   │
│  │ Broadcast   │───▶│  │  │ style        │  │ 2-phase BFT      │ ││   │
│  │ Blocks      │    │  │  │ (fallback)   │  │ 2-chain commit   │ ││   │
│  └─────────────┘    │  │  └──────────────┘  └──────────────────┘ ││   │
│                      │  │                                          ││   │
│                      │  │  Validators │ Staking │ Slashing │       ││   │
│                      │  │  Unbonding  │ Rewards │ Escalation       ││   │
│                      │  └──────────────────────────────────────────┘│   │
│                      │                                              │   │
│                      │  ┌──────────────────────────────────────────┐│   │
│                      │  │  PERSISTENT STATE  (state.rs + sled)     ││   │
│                      │  │                                          ││   │
│                      │  │  accounts │ storage │ prime_orders │      ││   │
│                      │  │  bridge_queues │ blocks │ pruning │       ││   │
│                      │  │  height_meta                              ││   │
│                      │  │                                          ││   │
│                      │  │  Binary Merkle Tree → state_root (B256)  ││   │
│                      │  │  Snapshot export/import with root verify  ││   │
│                      │  └──────────────────────────────────────────┘│   │
│                      └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Module Dependency Graph

```
                    ┌───────────────┐
                    │  prime-chain  │  (binary entry point)
                    │  src/bin/     │
                    └───────┬───────┘
                            │
                            ▼
                    ┌───────────────┐
                    │    Engine     │  (orchestrator)
                    └───┬───┬───┬──┘
                        │   │   │
          ┌─────────────┤   │   ├──────────────┐
          ▼             │   │   │              ▼
   ┌────────────┐       │   │   │      ┌─────────────┐
   │ Consensus  │       │   │   │      │   Mempool    │
   │ + HotStuff2│       │   │   │      └─────────────┘
   └────────────┘       │   │   │
                        ▼   │   ▼
                ┌──────────┐│┌──────────────┐
                │ EVM/revm │││ PrimeOrders  │
                │ + Precomp│││ + FBA Engine │
                └──────────┘│└──────────────┘
                            ▼
                   ┌──────────────┐
                   │    Bridge    │
                   │ orders↔evm  │
                   └──────────────┘
                            │
                            ▼
          ┌─────────────────────────────────┐
          │   PersistentState (sled)        │
          │   accounts│storage│prime_orders │
          │   bridge_queues│blocks│merkle   │
          └─────────────────────────────────┘
```

### 2.3 State Composition

```
State Root (B256, keccak256 Binary Merkle)
├── S_evm
│   ├── accounts tree (address → balance, nonce, code_hash, code)
│   └── storage tree  (address||slot → value)
├── S_orders
│   ├── markets      (MarketId → Market)
│   ├── orders       (OrderId → Order)
│   ├── books        (MarketId → OrderBook {bids, asks})
│   ├── accounts     (Address → collateral, positions, open_orders)
│   └── insurance_fund, margin_params
└── S_bridge
    ├── orders_to_evm queue (VecDeque<BridgeMessage>)
    └── evm_to_orders queue (VecDeque<BridgeMessage>)
```

---

## 3. Architecture Decision Records

---

### ADR-001: Multi-Domain State Model

**Status:** Accepted

**Context:**
Prime Chain's thesis requires both general-purpose smart contracts (EVM) and a high-performance order matching engine (CLOB) within a single blockchain. The core question is whether these two execution domains should maintain separate state or share a unified canonical state.

Separate state models (as used by dYdX v4 on its app-chain, or Polkadot parachains) introduce composability gaps: operations that span both domains require asynchronous bridge messages, breaking atomicity and introducing latency. For institutional use cases—where a smart contract must atomically deposit collateral, place an order, and react to fill results—this gap is unacceptable.

**Decision:**
Adopt a unified multi-domain state tuple:

```
S = (S_evm, S_orders, S_bridge)
```

All three sub-states are committed atomically within `PersistentState::commit_state()`, producing a single Merkle root. The `Engine` struct holds the `EvmEngine`, `OrdersEngine`, and `BridgeEngine` as co-equal fields, and `execute_block()` processes EVM transactions, order operations, and bridge messages in a deterministic order within each block.

In sled, sub-states are stored as separate trees:

| Tree | Contents |
|---|---|
| `accounts` | EVM account records (balance, nonce, code_hash, code) |
| `storage` | EVM contract storage (address+slot → value) |
| `prime_orders` | Serialized PrimeOrdersSnapshot (markets, orders, books, accounts) |
| `bridge_orders_to_evm` | Serialized BridgeQueueRecord |
| `bridge_evm_to_orders` | Serialized BridgeQueueRecord |
| `blocks` | JSON-serialized Block records |
| `pruning` | Height → state_root mapping for pruning |
| `height_meta` | Latest committed height |

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Separate chains** (dYdX v4) | Independent scaling, simpler per-chain logic | No atomic composability, bridge latency, separate finality |
| **L2 rollup for CLOB** | Inherits L1 security | L2→L1 finality delay (7+ days optimistic, hours ZK), no atomic interaction |
| **Single-domain EVM only** | Maximum compatibility | Order book in Solidity is 10–50x more expensive in gas, latency-bound |
| **Sidechain with IBC** | Cosmos ecosystem tooling | Asynchronous message passing, packet relay latency |

**Consequences:**
- *Positive:* Atomic composability—a single EVM transaction can call the CLOB precompile, place an order, and react to the fill in the same execution context.
- *Positive:* Single state root simplifies light client verification.
- *Negative:* State management complexity increases; each commit must serialize all three sub-states.
- *Negative:* The CLOB precompile uses a global `Mutex<PrimeOrdersState>`, which serializes concurrent precompile access within parallel execution.

---

### ADR-002: Block-STM Parallel Execution

**Status:** Accepted

**Context:**
Sequential EVM execution is the primary throughput bottleneck of Ethereum-compatible chains, typically limited to ~1,400 TPS on commodity hardware. Transactions that access disjoint accounts (e.g., independent transfers, separate contract interactions) are executed one-at-a-time despite having no data dependencies.

The Block-STM algorithm (pioneered by Aptos for Move VM) and the Grevm adaptation for EVM demonstrate that optimistic concurrency control can yield significant speedups on multi-core hardware without requiring developers to declare access lists.

**Decision:**
Implement a `ParallelExecutor` (in `src/core/parallel.rs`) that:

1. **Static dependency analysis**: Extracts read/write sets per transaction (`TxAccessSet`) from `tx.from` and `tx.to`. Coinbase is excluded to avoid false dependencies (every transaction writes to coinbase).
2. **Union-find grouping**: Groups transactions into independent sets using a disjoint-set data structure. Transactions with overlapping write addresses are grouped together.
3. **Parallel execution**: Each independent group executes on a forked `InMemoryDB` snapshot using `std::thread::scope` (scoped threads, no async runtime).
4. **MVCC validation**: A `MultiVersionMemory` stores per-address, per-transaction versions. After parallel execution, a validation pass checks that no cross-group conflicts occurred.
5. **Merge**: Independent group DB snapshots are merged. Coinbase balance deltas are accumulated across all groups.
6. **Fallback**: If the batch has fewer than `PARALLEL_THRESHOLD` (8) transactions, only one group exists, or MVCC validation detects a conflict, execution falls back to sequential mode.

The engine exposes both `execute_block()` (sequential) and `execute_block_parallel()` paths. The parallel path is invoked when the block producer opts in.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Sealevel** (Solana) | Declared access lists enable precise scheduling | Requires developers to declare all accessed accounts; not EVM-compatible |
| **Pure optimistic** (Monad-style) | No static analysis needed | Higher re-execution cost on conflict; Monad's approach is proprietary |
| **Sequential only** | Simplest, battle-tested | ~1,400 TPS ceiling, wastes multi-core hardware |
| **Sharding** | Horizontal scaling | Cross-shard atomicity is unsolved for general EVM workloads |

**Consequences:**
- *Positive:* ~6x measured speedup on 8 cores for independent-transfer workloads.
- *Positive:* Zero developer burden—no access list declarations required.
- *Positive:* Automatic sequential fallback preserves correctness under all conditions.
- *Negative:* Static analysis is conservative (only examines `from`/`to`, not storage-level access). Contract interactions that touch shared storage may be grouped together unnecessarily.
- *Negative:* Forking InMemoryDB per group incurs memory overhead proportional to state size × group count.

---

### ADR-003: HotStuff-2 Consensus

**Status:** Accepted

**Context:**
Classical BFT protocols like CometBFT (Tendermint) use a three-phase protocol (propose → prevote → precommit) requiring 3 message rounds per block. This adds approximately 300ms to finality even on a healthy network. For a financial trading chain targeting sub-200ms finality, this overhead is significant.

HotStuff-2 (Malkhi and Nayak, 2023) reduces the protocol to **two phases** while retaining the same safety guarantees (BFT tolerance of f < n/3 Byzantine validators).

**Decision:**
Implement HotStuff-2 as a standalone module (`src/core/hotstuff2.rs`) with the following design:

**Protocol phases:**
1. **Propose**: The leader for the current round (determined by weighted round-robin) broadcasts a `Proposal` containing the block hash and a `justify` QC from the previous round.
2. **Vote**: Validators check safety predicates and, if safe, return a `Vote` with their stake. When aggregate stake reaches the ⅔+1 threshold, a `QuorumCertificate` (QC) is formed.

**2-chain commit rule** (the key HotStuff-2 innovation):
```
Round r  : QC_r   certifies block B
Round r+1: QC_r+1 certifies block B' that extends B
→ Block B is committed
```

A block commits when two consecutive rounds each form a QC. This replaces the three-QC chain of original HotStuff.

**Implementation details:**
- `threshold()` = ⌊2 × total_stake / 3⌋ + 1
- Weighted round-robin leader election: deterministic modular selection based on stake weights
- Timeout handling with exponential backoff: `base_ms + round × delta_ms`
- Timeout quorum triggers round advance via `NewView` messages
- Safety predicates: never vote for a round already voted on; proposal must extend or supersede the locked QC
- Tombstoned validators are excluded from quorum calculations

The existing CometBFT-style three-phase consensus (`consensus.rs`) is retained as a fallback. The `ConsensusEngine` exposes `run_hotstuff2_round()` which lazily initializes the HotStuff-2 state machine.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **CometBFT** (Tendermint) | Battle-tested, large ecosystem | 3-phase adds ~300ms, quadratic message complexity |
| **Jolteon** (Aptos) | Optimistic fast path | More complex, Aptos-specific optimizations |
| **Bullshark/Narwhal DAG** (Sui) | High throughput via DAG-based consensus | DAG consensus is significantly more complex to implement and reason about |
| **MonadBFT** | Tailored for parallel EVM | Proprietary, limited public specification |

**Consequences:**
- *Positive:* 33% faster finality compared to three-phase protocols (~200ms target).
- *Positive:* Linear message complexity (O(n) per round vs. O(n²) for classic PBFT).
- *Positive:* Same BFT safety guarantees (tolerates f < n/3 Byzantine validators).
- *Negative:* Two-chain commit means a block is not finalized until the *next* round's QC forms—one round of latency is inherent.
- *Negative:* Less battle-tested in production than CometBFT.

---

### ADR-004: CLOB Precompile at 0x0100

**Status:** Accepted

**Context:**
The central value proposition of Prime Chain is that EVM smart contracts can interact atomically with the native order book. The question is *how* to expose order book operations to EVM execution.

Options range from asynchronous bridge messages (latent, no atomicity), to implementing the CLOB entirely in Solidity (gas-prohibitive), to a custom EVM precompile that directly mutates native state.

**Decision:**
Register a custom precompile at the fixed address `0x0000000000000000000000000000000000000100` using revm's `append_handler_register` API. The precompile is an environment-aware function (`Precompile::Env`) that receives the `Env` (including `tx.caller`) and dispatches on ABI-encoded function selectors.

**Precompile interface (ABI):**

| Function | Selector | Gas | Signature |
|---|---|---|---|
| `placeOrder` | keccak256 of sig | 50,000 | `(uint64 marketId, bool isBuy, uint256 price, uint256 size, uint8 tif) → (uint256 orderId, uint256 filled, uint256 remaining)` |
| `cancelOrder` | keccak256 of sig | 20,000 | `(uint256 orderId) → (bool success)` |
| `depositCollateral` | keccak256 of sig | 25,000 | `(uint256 amount) → (bool success)` |
| `withdrawCollateral` | keccak256 of sig | 25,000 | `(uint256 amount) → (bool success)` |
| `getPosition` | keccak256 of sig | 5,000 | `(uint64 marketId) → (int128 size, uint256 entryPrice)` |
| `getCollateral` | keccak256 of sig | 3,000 | `() → (uint256 collateral)` |
| `isLiquidatable` | keccak256 of sig | 10,000 | `(address account) → (bool)` |
| `getBestBidAsk` | keccak256 of sig | 5,000 | `(uint64 marketId) → (uint256 bestBid, uint256 bestAsk)` |

**State access mechanism:** The precompile accesses `PrimeOrdersState` through a global `Lazy<Mutex<Option<Arc<Mutex<PrimeOrdersState>>>>>`. Before block execution, `set_prime_orders_context()` installs the shared state; after execution, `clear_prime_orders_context()` removes it. The engine reclaims sole ownership via `Arc::try_unwrap`.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Bridge messages** (async) | Clean separation of concerns | 1+ block latency, no atomic composability |
| **Contract-based CLOB** (Solidity) | No protocol changes needed | 10-50x gas overhead, BTreeMap impossible in EVM, ~$5+ per order on Ethereum gas |
| **Oracle pattern** | Read-only, simple | Cannot atomically place orders; write operations need separate tx |
| **System contract** (Solana-style) | Account-model based | Requires different EVM execution model, breaks Ethereum compatibility |

**Consequences:**
- *Positive:* Atomic composability—unique among all existing L1 chains. A Solidity vault can deposit collateral, place a limit order, and handle the fill callback in a single transaction.
- *Positive:* 3–4x cheaper than DEX swap operations (50K gas for placeOrder vs. 150K+ for Uniswap V3 swap).
- *Positive:* Full ABI encoding means standard Ethereum tooling (ethers.js, viem) can interact with the precompile.
- *Negative:* The global mutex serializes all precompile access within parallel execution, limiting the parallelism benefit for CLOB-heavy workloads.
- *Negative:* Precompile address is non-standard (not in EIP-1 range); tooling may need configuration.
- *Negative:* Tight coupling between EVM execution and order book state.

---

### ADR-005: Frequent Batch Auctions

**Status:** Accepted

**Context:**
Continuous limit order books are vulnerable to front-running and MEV extraction. In a continuous CLOB, speed advantage translates directly to profit: a faster participant can observe an incoming order and place their own order ahead of it (sandwich attack) or extract value from the spread (latency arbitrage). This problem is well-documented in traditional finance (see Budish, Cramton, Shim 2015) and is amplified in blockchain settings where mempool contents are public.

**Decision:**
Implement Frequent Batch Auctions (FBA) via the `FBAEngine` and `BatchAuction` structs in `src/core/fba.rs`. The mechanism works as follows:

1. **Collection phase**: Orders are submitted via `submit_order()` with a `sequence` number (monotonically increasing within a batch). Orders accumulate in per-market `BatchAuction` instances.
2. **Clearing price discovery**: At batch execution time, the engine evaluates all candidate price levels (union of buy and sell prices). For each candidate, it computes aggregate demand (buy volume at or above the price) and supply (sell volume at or below). The price that maximizes matched volume wins; ties are broken in favor of the higher price (benefits sell side, standard FBA convention).
3. **Pro-rata allocation**: When one side is oversubscribed, fills are allocated proportionally to order size, preventing any single participant from capturing disproportionate fill.
4. **Pairing**: Buyers and sellers are paired sequentially to produce individual `AuctionFill` records.

The batch interval is configurable (default: 100ms). The engine exposes `execute_batch_auctions()` which runs all pending auctions and applies fills to `PrimeOrdersState`.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Continuous matching only** | Simpler, lower latency per order | Front-running, MEV extraction, latency arms race |
| **Encrypted order flow** (threshold encryption) | Strongest privacy | Requires threshold key ceremony, complex setup, decryption latency |
| **Proposer-Builder Separation** (PBS) | Separates ordering from execution | Doesn't eliminate MEV, just redistributes; complex protocol change |
| **Commit-reveal only** | Hides individual tx content | Doesn't address order-level MEV in the CLOB specifically |

**Consequences:**
- *Positive:* Eliminates front-running and latency arbitrage within each batch window—all orders in a batch are treated as simultaneous.
- *Positive:* Uniform price ensures fair execution (no adverse selection from sequential matching).
- *Positive:* Simple implementation (~390 lines) with no cryptographic ceremony.
- *Negative:* Adds execution latency equal to the batch interval (~100ms default).
- *Negative:* Pro-rata allocation may partially fill large orders more than a continuous book would.

---

### ADR-006: Commit-Reveal MEV Protection

**Status:** Accepted

**Context:**
EVM transactions are visible in the mempool before execution. This creates a well-known MEV vector: searchers can inspect pending transactions and construct sandwich attacks, generalized front-running, or back-running strategies. While FBA addresses MEV in the CLOB domain, the EVM transaction pool needs its own protection mechanism.

**Decision:**
Implement a two-phase commit-reveal scheme in `src/core/commit_reveal.rs`:

**Phase 1 — Commit:**
The user submits a `TxCommitment` containing:
- `commitment_hash`: `keccak256(encrypted_tx || salt)`
- `sender`: The user's address
- `block_number`: The block at which the commitment was made

The pool stores this in a `HashMap<B256, TxCommitment>`.

**Phase 2 — Reveal:**
Within the `commit_window` (default: 2 blocks), the user submits a `TxReveal` containing:
- `commitment_hash`: Must match a pending commitment
- `encrypted_tx`: The actual transaction data
- `salt`: Random nonce used in the commitment hash

The pool verifies `keccak256(encrypted_tx || salt) == commitment_hash`. If valid, the transaction data is extracted and made available for execution.

**Lifecycle management:**
- `prune_expired()` removes commitments that exceed the window
- `drain_revealed()` extracts ready-to-execute transaction data
- Duplicate commitments and double-reveals are rejected

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Encrypted mempool** (Flashbots SUAVE) | Full privacy until inclusion | Complex infrastructure, requires trusted enclave or threshold encryption |
| **Threshold encryption** (Shutter Network) | Cryptographic guarantees | Requires distributed key generation ceremony, adds protocol complexity |
| **No protection** | Zero overhead | Users are fully exposed to sandwich attacks and front-running |
| **Private transaction pools** (Flashbots Protect) | Practical, available today | Centralized trust assumption, doesn't protect against proposer MEV |

**Consequences:**
- *Positive:* Simple implementation (~130 lines) with no cryptographic ceremony.
- *Positive:* Prevents content-based front-running during the commit window.
- *Negative:* Adds 2-block latency before a committed transaction can execute.
- *Negative:* Does not protect against *timing-based* MEV (the commitment itself reveals that *some* transaction exists).
- *Negative:* Users must interact with the chain twice (commit + reveal) instead of once.

---

### ADR-007: revm for EVM Execution

**Status:** Accepted

**Context:**
Prime Chain needs a production-quality EVM implementation in Rust. The implementation must support the Shanghai specification (the latest stable EVM hard fork), custom precompile registration, and integration with Rust-native state backends.

**Decision:**
Use **revm v12** with the following configuration:
- `SpecId::SHANGHAI` for all block execution
- Features: `std`, `serde` (default features disabled for minimal binary size)
- Custom precompile registration via `append_handler_register()`
- `InMemoryDB` as the in-execution database, backed by `PersistentState` (sled) for durability
- State transitions applied via `transact_commit()` (sequential) or `transact()` + manual DB management (parallel)

revm is the EVM implementation used by **Reth** (the Rust Ethereum client by Paradigm), ensuring it tracks Ethereum protocol changes closely and has significant production testing.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **evmone** (C++) | Fastest pure interpreter | Cross-language FFI complexity, no native Rust precompile support |
| **go-ethereum** (Go) | Most battle-tested | Go runtime overhead, CGo FFI is painful, different memory model |
| **Custom EVM** | Full control | Enormous development and audit cost, high risk of consensus-critical bugs |
| **SputnikVM** (Rust) | Pure Rust, alternative to revm | Less actively maintained, smaller ecosystem than revm |

**Consequences:**
- *Positive:* Same-language integration with zero FFI overhead.
- *Positive:* Active Reth ecosystem means precompile APIs, spec updates, and bug fixes flow upstream.
- *Positive:* `Precompile::Env` variant gives precompiles access to the full execution environment (including `tx.caller`), enabling authenticated CLOB operations.
- *Negative:* Coupled to revm's API surface; major version bumps require migration.

---

### ADR-008: sled for Storage

**Status:** Accepted (with planned migration)

**Context:**
Prime Chain needs an embedded key-value store for persisting EVM account state, contract storage, order book data, bridge queues, and block history. The store must support atomic writes, prefix scans (for iterating an account's storage slots), and reasonable throughput for initial development.

**Decision:**
Use **sled 0.34**, a Rust-native embedded database built on a lock-free B+ tree with zero-copy reads. sled is used through 8 separate trees within a single database:

```
accounts, storage, prime_orders, bridge_orders_to_evm,
bridge_evm_to_orders, blocks, pruning, height_meta
```

State is committed via `PersistentState::commit_state()` which writes dirty EVM accounts (tracked by a `Mutex<HashSet<Address>>`), serializes the PrimeOrders snapshot, serializes bridge queues, and flushes to disk. The dirty-tracking optimization avoids full-state rewrites on each block.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **libmdbx** (used by Reth, Erigon) | High throughput, memory-mapped, battle-tested | C library requires FFI bindings, more complex build |
| **RocksDB** (used by Sui, older Ethereum clients) | Mature, well-understood performance profile | C++ dependency, heavy build, tuning complexity |
| **LMDB** | Simple, fast reads | Single writer limitation, manual memory management |
| **In-memory only** | Fastest possible, zero I/O | No persistence; suitable only for testing |

**Consequences:**
- *Positive:* Pure Rust—no C/C++ build dependencies, no FFI.
- *Positive:* Simple API that maps well to the multi-tree state model.
- *Positive:* Adequate for development and testnet workloads.
- *Negative:* sled is not considered production-ready by its author (still alpha). Performance under heavy write loads is inferior to libmdbx/RocksDB.
- *Negative:* No MVCC or transactional isolation across trees.
- *Migration plan:* Migrate to **libmdbx** (Reth's storage backend) for production deployment.

---

### ADR-009: Multi-Pool Mempool

**Status:** Accepted

**Context:**
A naive single-queue mempool cannot correctly handle EIP-1559 base fee dynamics, nonce gaps, or transaction replacement. When the base fee rises, previously valid transactions become under-priced; when it falls, previously excluded transactions become eligible. Nonce gaps (e.g., nonce 5 arrives before nonce 4) require parking transactions until the gap is filled.

**Decision:**
Implement a three-pool architecture in `src/core/mempool.rs`:

```
┌───────────────────────────────────────────────┐
│                   MEMPOOL                      │
│                                                │
│  ┌──────────┐  ┌──────────┐  ┌──────────────┐│
│  │ PENDING  │  │ QUEUED   │  │ BASE_FEE_POOL││
│  │          │  │          │  │              ││
│  │ Ready to │  │ Future   │  │ Under current││
│  │ execute  │  │ nonce    │  │ base fee     ││
│  │ (sorted  │  │ (nonce   │  │ (promoted    ││
│  │  by fee) │  │  gaps)   │  │  when fee    ││
│  │          │  │          │  │  drops)      ││
│  └──────────┘  └──────────┘  └──────────────┘│
│                                                │
│  Eviction: lowest-fee tx across all pools     │
│  Replacement: bump_bps minimum (default 10%)  │
│  Per-sender limit: 1,000 txs                  │
│  Global limit: 10,000 txs                     │
└───────────────────────────────────────────────┘
```

Each pool is a `HashMap<Address, BTreeMap<u64, Transaction>>` — per-sender queues ordered by nonce.

**Transaction lifecycle:**
1. **insert()**: Validates nonce, balance, and gas price. Routes to pending (ready), queued (future nonce), or base_fee_pool (under-priced).
2. **fill_gaps()**: When a pending tx is inserted, checks if queued txs for that sender now have consecutive nonces and promotes them.
3. **promote()**: Called after each block when base fee changes. Moves txs from base_fee_pool to pending if their gas price now meets the base fee.
4. **demote()**: Moves txs from pending to base_fee_pool if the base fee has risen above their gas price.
5. **remove_mined()**: Removes a confirmed tx and promotes any queued successors.
6. **evict_for_fee()**: When the pool is full, evicts the globally lowest-fee transaction (with a minimum bump requirement).

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Single priority queue** | Simpler implementation | Cannot handle nonce gaps, no base fee awareness |
| **No nonce gap handling** | Simpler | Drops valid future transactions, degraded UX |
| **Ethereum's TxPool** (geth) | Battle-tested | Complex Go implementation, not directly portable |

**Consequences:**
- *Positive:* Correctly handles EIP-1559 base fee dynamics with automatic promotion/demotion.
- *Positive:* Nonce gap tolerance enables out-of-order transaction submission.
- *Positive:* Fee-based eviction ensures the highest-value transactions are retained under memory pressure.
- *Negative:* Three-pool design adds complexity to insertion and replacement logic.

---

### ADR-010: Binary Merkle Tree for State Root

**Status:** Accepted

**Context:**
Every block must produce a verifiable commitment to the entire chain state. This commitment (the state root) enables light clients to verify state proofs without downloading the full state. The choice of tree structure affects proof size, computation cost, and compatibility with existing tooling.

**Decision:**
Implement a **binary Merkle tree** over sorted key-value pairs in `src/core/state.rs`. The algorithm:

1. Collect all key-value pairs from all sled trees (accounts, storage, prime_orders, bridge queues).
2. Sort by key (lexicographic).
3. Hash each pair: `leaf = keccak256(key || value)`.
4. Build a binary tree bottom-up: pairs of leaves are combined as `keccak256(left || right)`. Odd leaves are promoted.
5. The root hash is the state root committed to each block.

Merkle proofs (`StateProof`) contain the sibling hashes along the path from the target leaf to the root, plus a boolean path indicating left/right at each level. Verification is implemented in `StateProof::verify()`.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Ethereum Patricia Trie** (MPT) | Direct Ethereum compatibility, compact proofs for sparse state | Complex implementation, 4-node trie types, high implementation risk |
| **Jellyfish Merkle Tree** (Aptos) | Optimized for account-based state, supports versioning | Aptos-specific, more complex than binary Merkle |
| **Flat hash** (no tree) | Simplest possible | No proofs, no light client support |
| **Verkle Tree** | Smaller proofs than Merkle | Requires polynomial commitment schemes (KZG), cryptographic complexity |

**Consequences:**
- *Positive:* Simple, auditable implementation (~100 lines for tree + proof).
- *Positive:* Proof generation and verification are straightforward.
- *Positive:* keccak256 is Ethereum-native, enabling cross-chain proof verification.
- *Negative:* Proofs are O(log n) in size, larger than Patricia trie proofs for sparse key ranges.
- *Negative:* Full state must be sorted on every commit, which is O(n log n).

---

### ADR-011: ECDSA secp256k1 for Transaction Signing

**Status:** Accepted

**Context:**
Transactions must be cryptographically authenticated. The choice of signature scheme affects wallet compatibility, verification speed, and aggregate signature possibilities.

**Decision:**
Use **ECDSA over the secp256k1 curve** via the `k256` crate (v0.13), matching Ethereum's transaction signing scheme. Implementation details (`src/crypto/mod.rs`):

- **Signing hash**: `keccak256(chain_id || nonce || gas_price || gas_limit || to || value || data)` — an EIP-155-inspired format with chain ID replay protection.
- **Signature format**: `(r, s, v)` where `v = recovery_id + 35 + chain_id * 2` (EIP-155).
- **Recovery**: `VerifyingKey::recover_from_prehash()` recovers the signer's public key from the message hash and signature.
- **Address derivation**: `keccak256(uncompressed_pubkey[1..])[-20:]` — standard Ethereum address derivation.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Ed25519** | 2-3x faster verification, simpler implementation | No Ethereum wallet compatibility, no public key recovery from signature |
| **BLS** (BLS12-381) | Signature aggregation for consensus | Slower individual verification, no Ethereum wallet support |
| **Schnorr** | Batch verification, key aggregation | Not Ethereum-standard, limited wallet support |

**Consequences:**
- *Positive:* Full Ethereum wallet compatibility (MetaMask, Ledger, Trezor, etc.).
- *Positive:* Public key recovery from signature eliminates the need to transmit public keys.
- *Positive:* Chain ID replay protection prevents cross-chain signature reuse.
- *Negative:* ECDSA verification is slower than Ed25519 (~2-3x).
- *Negative:* No native signature aggregation (each consensus vote must carry a full signature).

---

### ADR-012: Insurance Fund and Auto-Deleveraging

**Status:** Accepted

**Context:**
In a leveraged trading system, positions can become insolvent (negative equity) if the market moves faster than liquidation can execute. Without a backstop mechanism, the deficit must be absorbed by someone—either the protocol, other traders, or it creates a bad debt that undermines system solvency.

**Decision:**
Implement a two-tier insolvency protection mechanism in `PrimeOrdersState`:

**Tier 1 — Insurance Fund:**
- An `insurance_fund` (U256) accumulates from a configurable fraction of trade fees (`insurance_contribution_rate_bps`, default: 10 bps = 0.1%).
- When a liquidation results in a deficit (position value < maintenance margin), the insurance fund absorbs the loss up to its balance.
- The fund is part of `S_orders` and committed with every block.

**Tier 2 — Auto-Deleveraging (ADL):**
- When the insurance fund is insufficient to cover a deficit, ADL is triggered.
- Profitable traders on the opposite side of the market are force-closed, starting with the most profitable, until the deficit is covered.
- ADL events are recorded with the deleveraged accounts and position adjustments.

**Margin parameters:**
- `initial_margin_bps`: Required collateral to open a position (configurable, default: 0 for spot markets).
- `maintenance_margin_bps`: Minimum collateral before liquidation is triggered.
- `is_liquidatable()`: Returns true when a trader's collateral falls below the maintenance margin requirement.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Socialized losses** | Simplest, no complex logic | Unpredictable losses for all traders, poor UX |
| **No backstop** | Zero protocol overhead | Bad debt accumulates, system becomes insolvent |
| **External insurance** (third-party pool) | Decouples insurance from protocol | Introduces trust dependency, coverage gaps |
| **Protocol bailout** (mint tokens) | Always solvent | Inflationary, undermines token value |

**Consequences:**
- *Positive:* Protocol solvency is guaranteed under all market conditions.
- *Positive:* Industry-standard approach (used by BitMEX, Binance, dYdX).
- *Positive:* Transparent and deterministic—ADL priority is computable by any node.
- *Negative:* ADL risk for profitable traders (their positions may be involuntarily closed).
- *Negative:* Insurance fund must be bootstrapped; new deployments start with zero coverage.

---

### ADR-013: True Atomic vs. Async EVM ↔ CLOB Composability

**Status:** Accepted

**Context:**
Hyperliquid chose dual-execution (HyperCore + HyperEVM) for maximum CLOB performance but at the cost of async composability. HyperEVM runs as a separate Cancun-spec EVM alongside the native CLOB; they execute sequentially. EVM reads HyperCore state from the previous block (1 block stale). CoreWriter at `0x333...333` queues orders for the next block — seconds delay. This design optimizes for raw CLOB throughput (200K ops/s) but makes atomic EVM ↔ CLOB flows impossible.

**Decision:**
Prime Chain chose integrated execution with a precompile at `0x0100` that runs PrimeOrders operations **synchronously** within EVM transaction execution. The CLOB state is co-located with EVM state in the same block; precompile calls execute inline during `revm.transact_commit()`. A single transaction can deposit collateral, place an order, and react to the fill in one atomic step.

**Alternatives Considered:**

| Alternative | Pros | Cons |
|---|---|---|
| **Dual-execution** (Hyperliquid) | Max CLOB throughput, independent scaling | Async composability — CoreWriter delayed by seconds, reads stale by 1 block |
| **Bridge messages** (dYdX) | Clean domain separation | Latent — multi-block latency, no atomicity |
| **Contract-based CLOB** (Solidity) | No protocol changes | Gas-prohibitive (10–50x), BTreeMap impossible in EVM |

**Consequences:**
- *Positive:* True atomic composability — unique in the industry. Enables use cases impossible on any other chain (vault: deposit → place order → handle fill in one tx).
- *Positive:* Measured 2,484,170 CLOB ops/s and 72,181 EVM TPS — competitive with or exceeding dual-execution designs.
- *Negative:* Precompile uses global mutex, limiting parallel precompile calls within a block. Worth it — atomic composability is the core value proposition.

---

## 4. Data Flow Diagrams

### 4.1 EVM Transaction Lifecycle

```
    User                  Node                    Engine
     │                     │                        │
     │  eth_sendTransaction │                        │
     │ ───────────────────▶│                        │
     │                     │  validate_tx_basic()    │
     │                     │  (chain_id, gas_limit,  │
     │                     │   signature recovery)   │
     │                     │───────────────────────▶ │
     │                     │                        │
     │                     │  mempool.validate()     │
     │                     │  (nonce, balance,        │
     │                     │   gas_price vs base_fee)│
     │                     │───────────────────────▶ │
     │                     │                        │
     │                     │  mempool.insert()       │
     │                     │  → pending / queued /   │
     │                     │    base_fee_pool        │
     │                     │───────────────────────▶ │
     │                     │                        │
     │                     │     execute_block()     │
     │                     │                        │
     │                     │  1. Sort senders by fee │
     │                     │  2. Select ready txs    │
     │                     │  3. Execute via revm    │
     │                     │     (+ precompile)      │
     │                     │  4. Produce Receipt     │
     │                     │───────────────────────▶ │
     │                     │                        │
     │                     │  Consensus (finalize)   │
     │                     │  State commit (merkle)  │
     │                     │  Base fee adjustment    │
     │                     │  mempool promote/demote │
     │                     │───────────────────────▶ │
     │                     │                        │
     │  eth_getTransactionReceipt                   │
     │ ◀───────────────────│                        │
```

### 4.2 Order Lifecycle (Continuous Matching)

```
  Trader                  Engine                PrimeOrders
    │                       │                       │
    │  prime_submitOrder     │                       │
    │  (or precompile call)  │                       │
    │ ─────────────────────▶│                       │
    │                       │  validate:             │
    │                       │  - market exists?      │
    │                       │  - market active?      │
    │                       │  - tick/lot alignment? │
    │                       │  - collateral check    │
    │                       │──────────────────────▶ │
    │                       │                       │
    │                       │  match_order():        │
    │                       │  - Walk opposing book  │
    │                       │  - Price-time priority │
    │                       │  - Generate trades     │
    │                       │  - Update positions    │
    │                       │  - Post remaining      │
    │                       │──────────────────────▶ │
    │                       │                       │
    │                       │  Return OrderOutcome   │
    │                       │  {order_id, filled,    │
    │                       │   remaining, trades[]} │
    │ ◀─────────────────────│                       │
    │                       │                       │
    │                       │  Emit DomainEvent:     │
    │                       │  OrderSubmitted, Trade │
    │                       │──────────────────────▶ │
```

### 4.3 Block Lifecycle

```
  Block Production                          Consensus
       │                                        │
  1. Collect txs from mempool                   │
  2. Sort by fee (descending)                   │
  3. Execute txs (sequential or parallel)       │
  4. Generate receipts                          │
       │                                        │
  5. apply_pending_changes()  ──────────────── │
       │                                        │
  6. run_finality_rounds()                      │
     ├── run_hotstuff2_round()                  │
     │   ├── Propose (leader)                   │
     │   ├── Vote (all validators)              │
     │   ├── Form QC (≥⅔ stake)                 │
     │   └── try_commit (2-chain rule)          │
     └── OR: CometBFT fallback                  │
       │                                        │
  7. Slashing evidence processing               │
  8. finalize() → Finalization                  │
  9. process_unbonding()                        │
  10. apply_pending_slashes()                   │
  11. apply_rewards() → validator balances      │
       │                                        │
  12. commit_state()                            │
      ├── write_evm_state (dirty accounts)      │
      ├── commit_prime_orders (snapshot)         │
      ├── commit_bridge_queues                   │
      └── compute_state_root (binary merkle)    │
       │                                        │
  13. store_block() → sled blocks tree          │
  14. Update base_fee (EIP-1559)                │
  15. mempool.promote() / demote()              │
  16. Increment block_number                    │
```

### 4.4 FBA Lifecycle

```
  Batch Window (100ms default)
       │
  ┌────┴────┐
  │ Collect  │  Orders arrive via submit_order()
  │ Phase    │  Assigned monotonic sequence numbers
  │          │  Accumulated in per-market BatchAuction
  └────┬────┘
       │
  ┌────┴────┐
  │ Clear   │  For each market:
  │ Phase   │  1. Separate buys and sells
  │         │  2. Sort: buys desc by price, sells asc
  │         │  3. Sweep all price levels
  │         │  4. Find clearing price (max volume)
  │         │  5. Break ties: highest price wins
  └────┬────┘
       │
  ┌────┴────┐
  │ Fill    │  Pro-rata allocation for oversubscribed side
  │ Phase   │  Pair buyers with sellers
  │         │  Generate AuctionFill records
  └────┬────┘
       │
  ┌────┴────┐
  │ Apply   │  apply_results() → PrimeOrdersState
  │ Phase   │  Update positions (buyer: +size, seller: -size)
  │         │  Update market.last_price
  │         │  Return unmatched orders
  └─────────┘
```

---

## 5. Security Architecture

### 5.1 Transaction Authentication

```
Layer              Mechanism                     Implementation
─────────────────────────────────────────────────────────────
Signing            ECDSA secp256k1               k256 crate
Hash function      keccak256                     sha3 / revm
Replay protection  EIP-155 chain ID              chain_id in signing hash
Recovery           Public key from (r, s, v)     VerifyingKey::recover_from_prehash
Address derivation keccak256(pubkey)[-20:]        Standard Ethereum
```

All transactions with a signature are verified in `verify_tx_signature()` before mempool admission. Unsigned transactions are accepted with a warning log (for backward compatibility during development).

### 5.2 Consensus Safety

| Property | Guarantee | Mechanism |
|---|---|---|
| **Safety** | No two honest nodes commit conflicting blocks | BFT with ⅔+1 stake threshold |
| **Liveness** | Network eventually produces blocks | Timeout + round advance with exponential backoff |
| **Finality** | Committed blocks are irreversible | 2-chain commit rule (HotStuff-2) |
| **Slashing** | Byzantine behavior is economically penalized | Escalating slashing (double-sign: 500 bps base, timeout: 100 bps, +25 bps per repeat, max 1000 bps) |
| **Unbonding** | Delayed withdrawal prevents nothing-at-stake | Configurable unbonding period (default: 2 blocks, production: longer) |

### 5.3 MEV Protection Layers

```
┌─────────────────────────────────────────────────────────┐
│                  MEV Protection Stack                     │
│                                                          │
│  Layer 1: Commit-Reveal (EVM transactions)               │
│  ├── Hides tx content during commit window (2 blocks)    │
│  ├── keccak256(tx_data || salt) commitment               │
│  └── Prevents content-based front-running                │
│                                                          │
│  Layer 2: Frequent Batch Auctions (CLOB orders)          │
│  ├── Discrete-time uniform-price auctions                │
│  ├── Orders within batch are indistinguishable by time   │
│  ├── Eliminates latency arbitrage                        │
│  └── Pro-rata allocation prevents size manipulation      │
│                                                          │
│  Layer 3: Fee-based ordering (mempool)                   │
│  ├── Transactions ordered by gas price (highest first)   │
│  └── Transparent, predictable inclusion criteria         │
└─────────────────────────────────────────────────────────┘
```

### 5.4 Economic Security

| Parameter | Default | Purpose |
|---|---|---|
| Max token supply | 1,000,000,000 PRIM × 10¹⁸ | Deflationary cap |
| Block reward | 10 PRIM × 10¹⁸ (halving every 35M blocks) | Validator incentive |
| Double-sign slash | 500 bps (5%) | Equivocation deterrent |
| Timeout slash | 100 bps (1%) | Liveness incentive |
| Escalation step | 25 bps per repeat offense | Progressive punishment |
| Escalation max | 1000 bps (10%) | Safety cap on slashing |
| Burn ratio | 50% of block reward | Deflationary pressure |

### 5.5 Network Security

| Mechanism | Implementation | Purpose |
|---|---|---|
| Message deduplication | `SeenCache` with TTL (120s) and max entries (4096) | Prevents replay and amplification |
| Peer management | `PeerManager` with TTL (60s) and max peers (50) | Bounds resource consumption |
| Gossip fanout | Configurable (default: 3) | Balances propagation speed vs. bandwidth |
| Retry backoff | Exponential (100ms base, 5s max) | Prevents thundering herd on reconnection |
| Packet authentication | keccak256-based ID per gossip packet | Enables deduplication without signature overhead |

---

## 6. Deployment Architecture

### 6.1 Docker Build

Multi-stage build optimized for minimal production image:

```
┌──────────────────────────────────────┐
│  Stage 1: Builder (rust:1.85-slim)   │
│  - Copies Cargo.toml, src/, tests/   │
│  - cargo build --release --bin       │
└──────────────┬───────────────────────┘
               │
               ▼
┌──────────────────────────────────────┐
│  Stage 2: Runtime (debian:bookworm)  │
│  - ca-certificates, libssl3, curl    │
│  - Copies release binary only        │
│  - Exposes: 8545, 9090, 9100        │
│  - ENTRYPOINT: prime-chain           │
│  - CMD: --config /etc/.../config.json│
└──────────────────────────────────────┘
```

**Exposed ports:**

| Port | Protocol | Service |
|---|---|---|
| 8545 | HTTP | JSON-RPC (Ethereum-compatible) |
| 9090 | HTTP | Prometheus metrics |
| 9100 | HTTP | Reserved (future monitoring) |

### 6.2 Docker Compose Testnet

Three-validator testnet deployed via `docker-compose.yml`:

```
┌────────────┐   ┌────────────┐   ┌────────────┐
│validator-1 │   │validator-2 │   │validator-3 │
│            │   │            │   │            │
│ RPC: 8545  │   │ RPC: 8546  │   │ RPC: 8547  │
│ Prom: 9090 │   │ Prom: 9091 │   │ Prom: 9092 │
│            │   │            │   │            │
│ Volume:    │   │ Volume:    │   │ Volume:    │
│ v1-data    │   │ v2-data    │   │ v3-data    │
└─────┬──────┘   └─────┬──────┘   └─────┬──────┘
      │                │                │
      └────────────────┼────────────────┘
                       │
              ┌────────┴────────┐
              │  prime-testnet  │
              │  (bridge network)│
              └─────────────────┘
```

**Health checks:** Each validator is monitored via HTTP health endpoint (`curl -sf http://localhost:8545/health`) with 10s interval, 5 retries, and 30s start period.

**Configuration:** Each validator mounts its own config file from `testnet/configs/validator-{1,2,3}.json`, enabling different P2P peer lists and validator keys per node.

### 6.3 Configuration Schema

The `AppConfig` structure supports JSON configuration with sensible defaults:

| Section | Key Parameters | Defaults |
|---|---|---|
| `engine` | chain_id, state_path, gas_limit_per_block | 7919, "state", 30M |
| `mempool` | max_total, max_per_sender, bump_bps | 10K, 1K, 1000 (10%) |
| `prime_orders` | initial_margin_bps, maintenance_margin_bps | 0, 0 |
| `bridge` | max_queue_len | 10,000 |
| `slashing` | double_sign_bps, timeout_bps, escalation | 500, 100, 25/1000 |
| `token_economics` | max_supply, reward_per_block, halving | 1B PRIM, 10 PRIM, 35M blocks |
| `rpc` | enabled, addr | false, 127.0.0.1:8545 |
| `p2p` | listen, peers, block_time_ms | 0.0.0.0:30303, [], 1000 |

---

## 7. Module Index

| Module | Path | Lines | Responsibility |
|---|---|---|---|
| `engine` | `src/core/engine.rs` | ~1,505 | Block production, tx execution, EIP-1559 fee market, coordinator |
| `consensus` | `src/core/consensus.rs` | ~851 | CometBFT-style consensus, validator set, staking, slashing, rewards |
| `hotstuff2` | `src/core/hotstuff2.rs` | ~760 | HotStuff-2 two-phase BFT, QC formation, 2-chain commit |
| `parallel` | `src/core/parallel.rs` | ~589 | Block-STM parallel executor, dependency analysis, MVCC, merge |
| `prime_orders` | `src/core/prime_orders.rs` | ~800 | CLOB matching engine, margin, liquidation, ADL, insurance fund |
| `precompiles` | `src/core/precompiles.rs` | ~292 | revm precompile at 0x0100, dispatch, global state context |
| `precompile_abi` | `src/core/precompile_abi.rs` | ~109 | ABI encoding/decoding, function selectors, gas constants |
| `fba` | `src/core/fba.rs` | ~392 | Frequent batch auctions, clearing price, pro-rata allocation |
| `commit_reveal` | `src/core/commit_reveal.rs` | ~130 | Two-phase commit-reveal for MEV protection |
| `state` | `src/core/state.rs` | ~959 | sled persistence, Merkle tree, snapshots, proofs, pruning |
| `mempool` | `src/core/mempool.rs` | ~513 | Three-pool mempool, nonce gaps, fee eviction, promote/demote |
| `bridge` | `src/core/bridge.rs` | ~85 | Cross-domain message queues (orders↔evm) |
| `events` | `src/core/events.rs` | ~varies | Domain event types (PrimeOrders, Bridge) |
| `errors` | `src/errors.rs` | ~varies | Error types for PrimeOrders and RPC |
| `rpc` | `src/rpc/rpc.rs` | ~970 | JSON-RPC server (tiny_http), Ethereum-compatible API |
| `rpc_router` | `src/rpc/rpc_router.rs` | ~795 | RPC method dispatch, parameter parsing, response formatting |
| `p2p` | `src/network/p2p.rs` | ~499 | P2P network simulation, node management, real gossip loop |
| `net_transport` | `src/network/net_transport.rs` | ~525 | UDP gossip, TCP sync, peer management, deduplication |
| `network` | `src/network/network.rs` | ~varies | Network simulation for consensus testing |
| `crypto` | `src/crypto/mod.rs` | ~152 | ECDSA signing, recovery, address derivation |
| `config` | `src/config/config.rs` | ~360 | JSON configuration loading, defaults, parsing utilities |
| `prometheus` | `src/prometheus/prometheus.rs` | ~137 | Prometheus metrics exporter, metric registry |
| `metrics` | `src/metrics/metric.rs` | ~5 | Re-export of prometheus module |
| `identity` | `src/identity/identity.rs` | ~varies | Node identity management |
| `governance` | `src/governance/governance.rs` | ~varies | Governance system |
| `lib` | `src/lib.rs` | ~47 | Module declarations and path mapping |

---

*Document generated from source analysis of prime-chain at commit HEAD on branch `docs/whitepaper-v5-technical`.*
*For protocol specification details, see [whitepaper.md](./whitepaper.md).*
