# Mersennet — Technical Reference

**Version:** 0.1.0  
**Authors:** Rodolfo Cova (@rodaemonic)  
**Rust Edition:** 2024  
**EVM Target:** Shanghai (SpecId::SHANGHAI)  
**Chain ID:** 7919 (default)

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Module Map](#2-module-map)
3. [Engine — Block Production Pipeline](#3-engine--block-production-pipeline)
4. [Parallel EVM Execution](#4-parallel-evm-execution)
5. [HotStuff-2 Consensus](#5-hotstuff-2-consensus)
6. [CometBFT-Style Consensus](#6-cometbft-style-consensus)
7. [PrimeOrders — CLOB Matching Engine](#7-primeorders--clob-matching-engine)
8. [CLOB Precompile](#8-clob-precompile)
9. [Frequent Batch Auctions (FBA)](#9-frequent-batch-auctions-fba)
10. [Commit-Reveal MEV Protection](#10-commit-reveal-mev-protection)
11. [Mempool](#11-mempool)
12. [State & Persistence](#12-state--persistence)
13. [Networking](#13-networking)
14. [RPC Interface](#14-rpc-interface)
15. [Cryptography](#15-cryptography)
16. [Bridge](#16-bridge)
17. [Observability](#17-observability)
18. [Configuration Reference](#18-configuration-reference)
19. [API Reference](#19-api-reference)
20. [Deployment Guide](#20-deployment-guide)
21. [Performance Tuning](#21-performance-tuning)
22. [Competitive Comparison](#22-competitive-comparison)
23. [Security Considerations](#23-security-considerations)
24. [Testing](#24-testing)

---

## 1. Architecture Overview

Mersennet is a high-performance EVM-compatible blockchain with a native central limit order book (CLOB) embedded at the protocol level as an EVM precompile. It combines parallel transaction execution, two-phase BFT consensus, frequent batch auctions for fair price discovery, and commit-reveal MEV protection into a single vertically-integrated stack.

```
┌─────────────────────────────────────────────────────────────────────┐
│                         CLIENT LAYER                                │
│   JSON-RPC 2.0 (tiny_http)  ·  eth_* compatibility  ·  CORS       │
│   primeorders_*  ·  primebridge_*  ·  /health  ·  /metrics        │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────────┐
│                        ENGINE (engine.rs)                           │
│  ┌──────────┐ ┌──────────────┐ ┌──────────┐ ┌─────────────────┐   │
│  │ Mempool  │ │   EVM (revm) │ │  Bridge  │ │ Commit-Reveal   │   │
│  │ pending  │ │  + Precompile│ │ ←→ Queue │ │ Pool            │   │
│  │ queued   │ │  (CLOB)      │ │          │ │                 │   │
│  │ basefee  │ │              │ │          │ │                 │   │
│  └──────────┘ └──────────────┘ └──────────┘ └─────────────────┘   │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │            ParallelExecutor (Block-STM / Grevm)               │ │
│  │   analyze_dependencies → find_independent_groups → fork+exec  │ │
│  │   MVCC validate → merge_fork_dbs                              │ │
│  └───────────────────────────────────────────────────────────────┘ │
│  ┌──────────────────────────┐  ┌────────────────────────────────┐ │
│  │   PrimeOrders (CLOB)    │  │   FBA Engine                   │ │
│  │   BTreeMap order books   │  │   Batch auctions per market    │ │
│  │   Price-time priority    │  │   Clearing price + pro-rata    │ │
│  │   GTC / IOC / FOK        │  │                                │ │
│  └──────────────────────────┘  └────────────────────────────────┘ │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────────┐
│                      CONSENSUS LAYER                                │
│  ┌──────────────────────┐   ┌──────────────────────────────────┐   │
│  │  CometBFT-style      │   │  HotStuff-2                     │   │
│  │  prevote/precommit   │   │  2-chain commit rule             │   │
│  │  weighted proposer   │   │  Weighted round-robin leader     │   │
│  │  slashing + jailing  │   │  QC-based finality               │   │
│  └──────────────────────┘   └──────────────────────────────────┘   │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────────┐
│                      NETWORK LAYER                                  │
│  UDP Gossip (dedup, TTL)  ·  TCP Snapshot Sync  ·  PeerManager     │
│  Wire formats: WireTx, WireBlock, WireReceipt                      │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────────┐
│                     STORAGE LAYER                                   │
│  sled embedded DB  ·  Binary Merkle tree  ·  Incremental commits   │
│  State pruning  ·  Merkle proofs  ·  Snapshot export/import        │
│  Trees: accounts, storage, prime_orders, bridge_*, blocks, pruning │
└─────────────────────────────────────────────────────────────────────┘
```

### Codebase Statistics

| Category | Files | Lines (approx.) |
|----------|-------|-----------------|
| Core sources (`src/`) | 28 | ~10,600 |
| Tests (`tests/`) | 10 | ~2,500 |
| Benchmarks (`benches/`) | 1 | ~200 |
| **Total** | **39** | **~13,300** |

### Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `revm` | 12 | EVM execution engine (Shanghai spec) |
| `sled` | 0.34 | Embedded persistent key-value store |
| `k256` | 0.13 | secp256k1 ECDSA signatures |
| `tiny_http` | 0.12 | Lightweight HTTP server for RPC |
| `metrics` / `metrics-exporter-prometheus` | 0.19 / 0.10 | Prometheus metrics |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | Structured logging |
| `serde` / `serde_json` / `bincode` | 1.0 / 1.0 / 1.3 | Serialization |

---

## 2. Module Map

```
src/
├── lib.rs                          # Module re-exports
├── bin/prime-chain.rs              # Node entry point (CLI)
├── core/
│   ├── engine.rs          (1505)   # Block production, tx execution, subsystem orchestration
│   ├── consensus.rs        (850)   # CometBFT-style prevote/precommit/commit
│   ├── hotstuff2.rs        (760)   # HotStuff-2 protocol state machine
│   ├── prime_orders.rs     (800)   # CLOB matching engine
│   ├── precompiles.rs      (292)   # EVM precompile bridge to CLOB
│   ├── precompile_abi.rs   (109)   # ABI encoding/decoding, selectors, gas costs
│   ├── parallel.rs         (589)   # Block-STM parallel EVM execution
│   ├── fba.rs              (392)   # Frequent batch auctions
│   ├── commit_reveal.rs    (129)   # Commit-reveal MEV protection
│   ├── state.rs            (958)   # Persistent state, Merkle tree, pruning
│   ├── mempool.rs          (512)   # Multi-pool transaction management
│   ├── bridge.rs                   # Cross-domain bridge queues
│   └── events.rs                   # Domain event types
├── network/
│   ├── net_transport.rs            # UDP gossip + TCP sync transport
│   ├── network.rs                  # NetworkSim (vote simulation)
│   └── p2p.rs                      # P2P node, wire types, NetworkNode
├── rpc/
│   ├── rpc.rs                      # JSON-RPC 2.0 server
│   └── rpc_router.rs               # Method dispatch router
├── crypto/
│   └── mod.rs                      # ECDSA signing, recovery, key generation
├── config/
│   └── config.rs                   # AppConfig, JSON config, defaults
├── prometheus/
│   └── prometheus.rs               # Prometheus exporter, metric registry
├── errors.rs                       # Error types
├── metrics/
│   └── metric.rs                   # Metric helpers
├── identity/
│   └── identity.rs                 # Node identity
└── governance/
    └── governance.rs               # Governance module
```

---

## 3. Engine — Block Production Pipeline

**File:** `src/core/engine.rs` (~1505 lines)

The `Engine` struct is the central orchestrator that owns all subsystems and drives the block production lifecycle.

### Engine Struct

```rust
pub struct Engine {
    pub chain_id: u64,                  // Default: 7919
    pub block_number: u64,              // Next block to produce
    pub base_fee: U256,                 // EIP-1559 base fee
    pub coinbase: Address,              // Block reward recipient
    pub gas_limit_per_block: u64,       // Default: 30,000,000
    pub spec_id: SpecId,                // SHANGHAI
    pub consensus: ConsensusEngine,     // CometBFT + HotStuff-2
    pub evm: EvmEngine,                // State DB + InMemoryDB
    pub orders: OrdersEngine,           // CLOB state
    pub bridge: BridgeEngine,           // Cross-domain message queues
    pub fba_engine: FBAEngine,          // Batch auction engine
    pub commit_reveal: CommitRevealPool,// MEV protection
    mempool: Mempool,                   // Transaction pool
    // ...
}
```

### Block Production Flow (`execute_block`)

```
┌─────────────────────────────────────────────────────┐
│ 1. Set CLOB precompile context                      │
│    set_prime_orders_context(Arc<Mutex<PrimeOrders>>) │
├─────────────────────────────────────────────────────┤
│ 2. Drain mempool by gas-price priority              │
│    - Sort senders by highest ready gas price        │
│    - Check gas limit budget                         │
│    - Execute tx via revm (execute_tx)               │
│    - Track nonces, accumulate gas_used              │
│    - Loop until no more progress                    │
├─────────────────────────────────────────────────────┤
│ 3. Clear CLOB precompile context                    │
│    clear_prime_orders_context()                     │
├─────────────────────────────────────────────────────┤
│ 4. Compute block hash                               │
│    keccak256(number ‖ chain_id ‖ gas_limit ‖        │
│              gas_used ‖ base_fee ‖ coinbase ‖       │
│              tx_count)                              │
├─────────────────────────────────────────────────────┤
│ 5. Consensus finalization                           │
│    - apply_pending_changes (stake/unbond)           │
│    - run_finality_rounds (prevote/precommit x 2)    │
│    - Process slashing evidence                      │
│    - finalize() → weighted proposer, rewards        │
│    - process_unbonding()                            │
│    - apply_pending_slashes()                        │
├─────────────────────────────────────────────────────┤
│ 6. Apply rewards to validator accounts              │
├─────────────────────────────────────────────────────┤
│ 7. Commit state                                     │
│    commit_state(evm_db, prime_orders, bridge, h)    │
│    → writes dirty accounts to sled                  │
│    → serializes CLOB + bridge state                 │
│    → computes Merkle state_root                     │
├─────────────────────────────────────────────────────┤
│ 8. Store block to sled, emit metrics                │
├─────────────────────────────────────────────────────┤
│ 9. Update EIP-1559 base fee                         │
│    next_base_fee(gas_used)                          │
│    promote/demote mempool txs                       │
├─────────────────────────────────────────────────────┤
│ 10. Increment block_number                          │
└─────────────────────────────────────────────────────┘
```

### EIP-1559 Base Fee Update

The `next_base_fee` function implements the standard EIP-1559 algorithm:

- **Target gas:** `gas_limit_per_block / fee_elasticity_multiplier` (default: 15M)
- **Max change denominator:** 8 (default)
- If `gas_used > target`: base fee increases proportionally
- If `gas_used < target`: base fee decreases proportionally
- Minimum base fee: 1 wei

### Transaction Execution (`execute_tx`)

Each transaction is executed through `revm` with the PrimeOrders precompile registered:

```rust
let mut evm = Evm::builder()
    .with_db(self.evm.db.clone())
    .with_spec_id(self.spec_id)
    .with_env(Box::new(env))
    .append_handler_register(precompiles::register_prime_orders_precompile)
    .build();

let result = evm.transact_commit()?;
```

Post-execution, the DB state is swapped back and dirty addresses are tracked for incremental persistence.

### Transaction Validation Pipeline

```
submit_tx(tx)
  │
  ├─ validate_tx_basic()
  │    ├─ gas_limit ≤ block gas limit
  │    ├─ chain_id matches (if present)
  │    └─ verify_tx_signature() → ECDSA recovery
  │
  ├─ mempool.validate()
  │    ├─ nonce ≥ account nonce
  │    ├─ balance ≥ gas_cost + value
  │    └─ gas_price ≥ base_fee
  │
  └─ mempool.insert() → pending / queued / base_fee_pool
```

---

## 4. Parallel EVM Execution

**File:** `src/core/parallel.rs` (589 lines)

Implements optimistic parallel transaction execution following the Block-STM / Grevm pattern. Achieves ~6x speedup on 8 cores for workloads with independent transactions.

### Algorithm

```
┌──────────────────────────────────────────────────────────────┐
│ 1. ANALYZE: Static read/write set analysis per tx            │
│    - reads:  {tx.from, tx.to}                                │
│    - writes: {tx.from, tx.to}                                │
│    - Coinbase excluded (would collapse all groups)           │
├──────────────────────────────────────────────────────────────┤
│ 2. GROUP: Union-Find over conflicting addresses              │
│    - Build write_map: address → [tx indices]                 │
│    - Union all writers to same address                       │
│    - Union readers with writers of same address              │
│    - Extract disjoint groups                                 │
├──────────────────────────────────────────────────────────────┤
│ 3. EXECUTE: std::thread::scope, one thread per group         │
│    - Fork InMemoryDB per group                               │
│    - Execute txs sequentially within group                   │
│    - Record MVCC writes: (address, tx_index) → AccountInfo   │
├──────────────────────────────────────────────────────────────┤
│ 4. VALIDATE: MVCC conflict detection                         │
│    - For each tx's read set, verify no lower-indexed tx      │
│      in another group modified the same address              │
│    - On conflict → sequential fallback                       │
├──────────────────────────────────────────────────────────────┤
│ 5. MERGE: Combine forked DBs                                 │
│    - Each group's dirty accounts → merged DB (disjoint)      │
│    - Coinbase: accumulate balance deltas across groups        │
│      merged_coinbase = base + Σ(group_coinbase - base)       │
└──────────────────────────────────────────────────────────────┘
```

### Fallback Conditions

Sequential execution is used when:
- Transaction count < `PARALLEL_THRESHOLD` (8)
- Union-find produces only 1 group (all txs are dependent)
- MVCC validation detects a conflict

### Key Structs

| Struct | Purpose |
|--------|---------|
| `ParallelExecutor` | Top-level executor with `num_threads` and `max_retries` |
| `TxAccessSet` | Per-tx `reads: HashSet<Address>` and `writes: HashSet<Address>` |
| `MultiVersionMemory` | MVCC store: `HashMap<Address, BTreeMap<usize, AccountInfo>>` |
| `ParallelExecutionResult` | Results vec, merged DB, dirty addresses set |
| `UnionFind` | Disjoint-set with path compression and union-by-rank |

### Integration

The engine's `execute_block_parallel()` method collects transactions from the mempool using the same priority ordering as `execute_block()`, then delegates to `ParallelExecutor::execute()`.

```rust
let num_threads = std::thread::available_parallelism()
    .map(|n| n.get())
    .unwrap_or(4);
let executor = ParallelExecutor::new(num_threads, 3);
let par_result = executor.execute(&collected_txs, &self.evm.db, ...);
```

---

## 5. HotStuff-2 Consensus

**File:** `src/core/hotstuff2.rs` (760 lines)

Implements the HotStuff-2 protocol — a linear-communication BFT consensus with a **2-chain commit rule**, reducing the original HotStuff's 3-chain requirement to 2 consecutive certified rounds.

### Data Structures

```rust
pub struct QuorumCertificate {
    pub block_hash: B256,
    pub height: u64,
    pub round: u64,
    pub signers: Vec<Address>,
    pub aggregate_stake: U256,
}

pub enum HotStuffMessage {
    Propose(Proposal),
    Vote { block_hash, height, round, voter, voter_stake },
    NewView { round, high_qc, sender },
    Timeout { round, high_qc, sender },
}
```

### State Machine

```rust
pub struct HotStuff2 {
    pub address: Address,
    pub validators: Vec<Validator>,
    pub current_round: u64,
    pub current_height: u64,
    pub locked_qc: Option<QuorumCertificate>,    // Safety lock
    pub high_qc: Option<QuorumCertificate>,      // Highest known QC
    pub last_committed_round: u64,
    pub pending_votes: HashMap<(u64, u64), Vec<(Address, U256)>>,
    pub pending_timeouts: HashMap<u64, Vec<(Address, Option<QC>)>>,
    pub last_voted_round: u64,
    pub tombstoned: HashSet<Address>,
    pub round_timeout_base_ms: u64,              // Default: 3000
    pub round_timeout_delta_ms: u64,             // Default: 500
}
```

### Protocol Flow

```
Round r:
  Leader ─── Propose(block, parent_qc, justify) ──→ All validators
  
  Validators:
    1. Verify leader for round
    2. Check is_safe_to_vote(proposal)
       - round > last_voted_round
       - justify.round ≥ locked_qc.round (or extends locked block)
    3. Send Vote(block_hash, round, voter, stake)
  
  Leader collects votes:
    - on_vote() accumulates (voter, stake) per (height, round)
    - When aggregate_stake ≥ threshold → form QC
    - threshold = ⌊2 * total_stake / 3⌋ + 1
  
  try_commit(qc):
    If locked_qc.round == r AND qc.round == r+1:
      → COMMIT block at round r  (2-chain rule)
    Always advance locked_qc to higher QC

Timeout:
  on_timeout() → broadcast Timeout message
  on_timeout_msg() → collect timeouts
  If quorum of timeouts → advance_round via NewView
  Timeout duration = base_ms + round * delta_ms
```

### Leader Election

Deterministic weighted selection:

```rust
pub fn leader_for_round(&self, round: u64) -> Address {
    let total_weight = Σ(eligible validators' stake);
    let target = round % total_weight;
    // Walk through validators accumulating stake until target
}
```

### Safety Invariants

1. **No equivocation:** A validator never votes twice for the same round (`last_voted_round` monotonically increases)
2. **Lock-based safety:** Votes require `justify.round ≥ locked_qc.round` or the proposal must directly extend the locked block
3. **Commit safety:** Commits only fire on consecutive round QCs (the 2-chain rule)

### Garbage Collection

On `advance_round(new_round)`:
- `pending_votes` retains only entries where `round ≥ new_round - 2`
- `pending_timeouts` retains only entries where `round ≥ new_round - 2`

---

## 6. CometBFT-Style Consensus

**File:** `src/core/consensus.rs` (~850 lines)

A CometBFT-inspired prevote/precommit consensus with validator set management, slashing with escalation, jailing, tombstoning, and token economics.

### Finality Rounds

Each block runs up to `N` finality rounds (default: 2):

```
For each round:
  1. PREVOTE:  Eligible validators broadcast prevotes
  2. Collect prevotes; detect double-signs
  3. If prevote_stake ≥ 2/3+1 → lock on block (PoLC)
  4. PRECOMMIT: Eligible validators broadcast precommits
  5. Collect precommits; detect timeouts
  6. If prevote_stake ≥ threshold AND precommit_stake ≥ threshold → FINALIZED
  7. If finalized → unlock, break
```

### Slashing

| Evidence Type | Base BPS | Default |
|---------------|----------|---------|
| Double-sign | `double_sign_bps` | 500 (5%) |
| Precommit timeout | `timeout_bps` | 100 (1%) |

**Escalation formula:**

```
effective_bps = base_bps
    + escalation_step_bps × offense_count
    + escalation_step_bps × (rounds_missed - 1)
effective_bps = min(effective_bps, max_escalation_bps)
```

Slashing draws from unbonding entries first, then active stake.

### Jailing & Tombstoning

- **Jailing:** Validator excluded from consensus until `jailed_until` height
- **Tombstoning:** Permanent exclusion (cannot rejoin); triggered by double-sign evidence
- **Unjail:** Requires jail period expiry and no tombstone

### Weighted Proposer Selection

CometBFT-style weighted round-robin using `proposer_priorities`:

```
For each validator:
  priority += stake
Winner = max(priority)
winner.priority -= total_voting_stake
```

New validators receive a penalty: `-(total_voting_stake + total_voting_stake / 8)` to prevent immediate selection.

### Token Economics

- **Halving:** `reward_per_block(height) = initial_reward / 2^(height / halving_interval)`
- **Distribution:** Pro-rata by stake
- **Burn:** `burned = scheduled_reward - Σ(distributed_rewards)` (rounding dust)
- **Supply cap:** Rewards capped at `remaining_supply = max_supply - total_minted`
- **Defaults:** 1B PRIM max supply (18 decimals), 10 PRIM/block initial reward, 35M block halving interval. Block rewards: 70% of supply.

---

## 7. PrimeOrders — CLOB Matching Engine

**File:** `src/core/prime_orders.rs` (~800 lines)

A full central limit order book with price-time priority matching, margin validation, insurance fund, auto-deleveraging, and market circuit breakers.

### Data Model

```rust
pub struct PrimeOrdersState {
    pub next_order_id: u64,
    pub markets: HashMap<MarketId, Market>,
    pub orders: HashMap<OrderId, Order>,
    pub accounts: HashMap<Address, AccountState>,
    pub books: HashMap<MarketId, OrderBook>,
    pub initial_margin_bps: u64,
    pub maintenance_margin_bps: u64,
    pub insurance_fund: U256,
    pub insurance_contribution_rate_bps: u64,
}

pub struct OrderBook {
    pub bids: BTreeMap<U256, VecDeque<OrderId>>,  // Price → FIFO queue
    pub asks: BTreeMap<U256, VecDeque<OrderId>>,   // Price → FIFO queue
}
```

### Order Types

| Time-in-Force | Behavior |
|---------------|----------|
| **GTC** (Good-Til-Cancelled) | Rests on book if not fully filled |
| **IOC** (Immediate-or-Cancel) | Fills what it can, cancels remainder |
| **FOK** (Fill-or-Kill) | Rejected if full liquidity unavailable |

### Matching Algorithm (`submit_order`)

```
1. Validate market exists and is Active
2. Validate order size > 0
3. Check initial margin requirement
4. For FOK: verify available_liquidity ≥ size
5. Collect opposing price levels:
   - Buy order  → matching_asks (asks ≤ limit price)
   - Sell order → matching_bids (bids ≥ limit price)
6. For each level (price-time priority):
   a. For each resting order in FIFO queue:
      - Compute fill = min(remaining, maker.size)
      - Validate fill margin for both taker and maker
      - Apply fill: update positions, maker order size
      - Compute insurance fee: notional × insurance_rate_bps / 10,000
      - Record Trade
      - Remove fully-filled maker orders
7. Clean empty price levels
8. If GTC and remaining > 0: place_order (rest on book)
9. Return OrderOutcome { order_id, filled, remaining, trades }
```

### VWAP Entry Price

When a position increases in the same direction, entry price is updated via volume-weighted average:

```
new_entry = (|old_size| × old_entry + |delta| × fill_price) / (|old_size| + |delta|)
```

When reducing or flipping a position, realized PnL is computed against the entry price.

### Margin System

| Parameter | Purpose |
|-----------|---------|
| `initial_margin_bps` | Required collateral to open a position |
| `maintenance_margin_bps` | Minimum equity to avoid liquidation |

**Liquidation check:** `account_equity < maintenance_margin_required`

Where:
- `equity = collateral + Σ(position_size × (mark_price - entry_price))`
- `maintenance_required = Σ(|position_size| × mark_price × maintenance_bps / 10,000)`

### Auto-Deleveraging (ADL)

When the insurance fund cannot cover a liquidation deficit:

1. Find all accounts with profitable positions in the same market
2. Sort by PnL-per-unit (highest first)
3. Close positions pro-rata against profitable counterparties until deficit covered

### Market Controls

- `halt_market(id)` — Sets status to `Halted`, blocks new orders
- `resume_market(id)` — Sets status to `Active`
- `MarketStatus::SettleOnly` — Settle-only mode

---

## 8. CLOB Precompile

**Files:** `src/core/precompiles.rs` (292 lines), `src/core/precompile_abi.rs` (109 lines)

The CLOB is exposed to EVM smart contracts via a stateful precompile at a fixed address.

### Precompile Address

```
0x0000000000000000000000000000000000000100
```

### Function Signatures & Gas Costs

| Function | Selector | Gas | Input | Output |
|----------|----------|-----|-------|--------|
| `placeOrder(uint64,bool,uint256,uint256,uint8)` | `keccak256(sig)[0:4]` | 50,000 | marketId, isBuy, price, size, tif | (orderId, filled, remaining) |
| `cancelOrder(uint256)` | — | 20,000 | orderId | (success) |
| `depositCollateral(uint256)` | — | 25,000 | amount | (success) |
| `withdrawCollateral(uint256)` | — | 25,000 | amount | (success) |
| `getPosition(uint64)` | — | 5,000 | marketId | (int128 size, uint256 entryPrice) |
| `getCollateral()` | — | 3,000 | — | (uint256 collateral) |
| `isLiquidatable(address)` | — | 10,000 | account | (bool) |
| `getBestBidAsk(uint64)` | — | 5,000 | marketId | (uint256 bestBid, uint256 bestAsk) |

### ABI Encoding

Standard Solidity ABI: 4-byte function selector followed by 32-byte words. All integers are big-endian padded to 32 bytes.

### Context Lifecycle

```rust
static PRIME_ORDERS_CTX: Lazy<Mutex<Option<Arc<Mutex<PrimeOrdersState>>>>> = ...;

// Before block execution:
set_prime_orders_context(Arc::clone(&shared_orders));

// After block execution:
clear_prime_orders_context();
```

The precompile acquires the global mutex, gets the `Arc`, then locks the inner `PrimeOrdersState` for each call. This ensures EVM transactions can call the CLOB atomically during execution.

### Registration

```rust
pub fn register_prime_orders_precompile(handler: &mut EvmHandler<'_, (), InMemoryDB>) {
    let prev_load = handler.pre_execution.load_precompiles.clone();
    handler.pre_execution.load_precompiles = Arc::new(move || {
        let mut precompiles = prev_load();
        precompiles.extend([(
            PRIME_ORDERS_PRECOMPILE,
            ContextPrecompile::Ordinary(Precompile::Env(prime_orders_precompile)),
        )]);
        precompiles
    });
}
```

---

## 9. Frequent Batch Auctions (FBA)

**File:** `src/core/fba.rs` (392 lines)

Implements frequent batch auctions for fair price discovery, reducing the advantage of speed-based MEV strategies.

### Architecture

```rust
pub struct FBAEngine {
    pub auctions: HashMap<MarketId, BatchAuction>,
    pub batch_interval_ms: u64,          // Default: 100ms
    pub current_batch_start: u64,
}

pub struct BatchAuction {
    pub market: MarketId,
    pub orders: Vec<BatchOrder>,
    pub next_sequence: u64,              // Arrival ordering
}
```

### Clearing Price Algorithm

```
1. Separate orders into buys and sells
2. Sort: buys by price DESC (then sequence ASC), sells by price ASC (then sequence ASC)
3. If no crossing (best_ask > best_bid) → no match
4. Collect all unique price levels from both sides
5. For each candidate price:
   - demand = Σ(buy.size where buy.price ≥ candidate)
   - supply = Σ(sell.size where sell.price ≤ candidate)
   - matched = min(demand, supply)
6. Select price that maximizes matched volume
   - Ties broken by highest price (benefits sell side)
7. Pro-rata allocation for oversubscribed side:
   - fill_i = order_i.size × matched_volume / total_side_volume
8. Pair buyers with sellers to produce fills
```

### Integration with Engine

```rust
pub fn submit_batch_order(&mut self, order: BatchOrder) {
    self.fba_engine.submit_order(order);
}

pub fn execute_batch_auctions(&mut self) -> Vec<AuctionResult> {
    let results = self.fba_engine.execute_all();
    self.fba_engine.apply_results(&results, &mut self.orders.state);
    results
}
```

`apply_results` writes fills back to `PrimeOrdersState`, updating buyer/seller positions and market `last_price`.

---

## 10. Commit-Reveal MEV Protection

**File:** `src/core/commit_reveal.rs` (129 lines)

A two-phase commit-reveal scheme that prevents front-running and sandwich attacks by hiding transaction details until after ordering is finalized.

### Protocol

```
Block N:   User submits commitment = keccak256(tx_bytes ‖ salt)
Block N+1: User reveals (tx_bytes, salt)
           Verification: keccak256(tx_bytes ‖ salt) == commitment_hash
Block N+2: If not revealed by block N + commit_window → pruned
```

### Data Structures

```rust
pub struct TxCommitment {
    pub commitment_hash: B256,    // keccak256(tx ‖ salt)
    pub sender: Address,
    pub block_number: u64,
}

pub struct TxReveal {
    pub commitment_hash: B256,
    pub encrypted_tx: Bytes,      // Original tx bytes
    pub salt: B256,
}

pub struct CommitRevealPool {
    commitments: HashMap<B256, TxCommitment>,
    revealed: Vec<(B256, Bytes)>,
    revealed_set: HashMap<B256, ()>,  // Dedup
    pub commit_window: u64,           // Default: 2 blocks
    pub current_block: u64,
}
```

### Error Cases

| Error | Condition |
|-------|-----------|
| `DuplicateCommitment` | Same commitment hash already pending |
| `CommitmentNotFound` | Reveal for unknown commitment |
| `CommitmentExpired` | `current_block > commitment.block_number + commit_window` |
| `InvalidReveal` | Hash mismatch: `keccak256(tx ‖ salt) ≠ commitment_hash` |
| `AlreadyRevealed` | Commitment was already successfully revealed |

### Lifecycle

```rust
engine.commit_tx(commitment)?;           // Phase 1
let tx_bytes = engine.reveal_tx(reveal)?; // Phase 2
let drained = pool.drain_revealed();      // Collect for execution
pool.prune_expired(current_block);        // Garbage collect
```

---

## 11. Mempool

**File:** `src/core/mempool.rs` (~512 lines)

A three-pool transaction management system inspired by Geth's design.

### Pool Architecture

```
┌──────────────────────────────────────────────────────┐
│                      Mempool                          │
│                                                       │
│  ┌─────────┐   ┌─────────┐   ┌──────────────────┐   │
│  │ pending  │   │ queued  │   │  base_fee_pool   │   │
│  │         │   │         │   │                  │   │
│  │ Ready to │   │ Future  │   │ Gas price below  │   │
│  │ execute  │   │ nonce   │   │ current base fee │   │
│  │         │   │ gap     │   │                  │   │
│  └─────────┘   └─────────┘   └──────────────────┘   │
│       ↕              ↕              ↕                  │
│   promote()      fill_gaps()    promote()/demote()    │
└──────────────────────────────────────────────────────┘
```

### Per-Pool Structure

Each pool is a `HashMap<Address, BTreeMap<u64, Transaction>>` — sender → nonce-sorted queue.

### Transaction Lifecycle

1. **Insert:** Route to `pending`, `queued`, or `base_fee_pool` based on nonce gap and gas price vs. base fee
2. **Gap filling:** When a pending tx is inserted, check queued pool for consecutive nonces to promote
3. **Promotion:** After base fee decrease, move qualifying txs from `base_fee_pool` to `pending`
4. **Demotion:** After base fee increase, move under-priced txs from `pending` to `base_fee_pool`
5. **Mining:** `take_ready(sender, nonce)` extracts for execution; `remove_mined(sender, nonce)` post-confirmation
6. **Replacement:** Existing tx at same nonce can be replaced if `new_gas_price ≥ old_gas_price × (1 + bump_bps/10000)`
7. **Eviction:** When `max_total` reached, lowest-fee tx across all pools is evicted (if incoming fee is higher)

### Limits

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_total` | 10,000 | Total txs across all pools |
| `max_per_sender` | 1,000 | Per-sender limit across all pools |
| `min_replace_bump_bps` | 1,000 (10%) | Minimum fee increase for replacement |

### Validation

```rust
pub fn validate(&self, tx, base_fee, account_nonce, account_balance) -> Result<(), TxRejection>
```

Checks: nonce ordering, balance sufficiency (`gas_limit × gas_price + value ≤ balance`), base fee compliance.

---

## 12. State & Persistence

**File:** `src/core/state.rs` (~958 lines)

### Storage Backend

Uses **sled** (embedded B-tree database) with the following trees:

| Tree | Key | Value |
|------|-----|-------|
| `accounts` | `Address` (20 bytes) | `AccountRecord` (bincode) |
| `storage` | `Address ‖ Slot` (52 bytes) | `U256` (32 bytes BE) |
| `prime_orders` | `"state"` | `PrimeOrdersSnapshot` (bincode) |
| `bridge_orders_to_evm` | `"queue"` | `BridgeQueueRecord` (bincode) |
| `bridge_evm_to_orders` | `"queue"` | `BridgeQueueRecord` (bincode) |
| `blocks` | `height` (8 bytes BE) | `Block` (JSON) |
| `pruning` | `height` (8 bytes BE) | `state_root` (32 bytes) |
| `height_meta` | `"latest_height"` | `height` (8 bytes) |

### Incremental Commits

The `dirty_accounts: Mutex<HashSet<Address>>` set tracks which accounts were modified during block execution. On commit, only dirty accounts are re-serialized to sled — avoiding a full DB rewrite.

```rust
pub fn commit_state(
    &self,
    evm_db: &InMemoryDB,
    prime_orders: &PrimeOrdersState,
    bridge_orders_to_evm: &BridgeQueue,
    bridge_evm_to_orders: &BridgeQueue,
    height: u64,
) -> Result<B256> {
    self.write_evm_state(evm_db)?;        // Only dirty accounts
    self.commit_prime_orders(prime_orders)?;
    self.commit_bridge_queues(..)?;
    let root = self.compute_state_root();
    self.record_height(height, root)?;
    Ok(root)
}
```

### Binary Merkle Tree

The state root is a binary Merkle tree computed over all key-value pairs from all trees:

```
1. Collect all (key, value) pairs from: accounts, storage, prime_orders, bridge_*
2. Sort by key
3. Leaf = keccak256(key ‖ value)
4. Internal = keccak256(left ‖ right)
5. Odd leaves are promoted without hashing
```

### State Proofs

```rust
pub fn generate_proof(&self, key: &[u8]) -> Result<StateProof>

pub struct StateProof {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub siblings: Vec<B256>,
    pub path: Vec<bool>,        // true = sibling is on left
    pub root: B256,
}
```

Verification: walk from leaf to root, hashing with siblings according to `path`.

### Pruning

```rust
pub fn prune_before(&self, height: u64) -> Result<u64>
```

Removes all pruning metadata and block data below the given height. Returns count of entries removed.

### Snapshots

```rust
pub fn export_snapshot(&self, path, height, state_root) -> Result<()>
pub fn import_snapshot(&self, path) -> Result<SnapshotMeta>
```

Exports/imports a self-contained bincode archive containing all account records, storage, CLOB state, and bridge queues. Import verifies the state root matches after restoration.

---

## 13. Networking

**Files:** `src/network/p2p.rs`, `src/network/net_transport.rs`

### Transport Layers

| Protocol | Purpose | Port Offset |
|----------|---------|-------------|
| **UDP** | Gossip protocol (blocks, txs) | Base (e.g., 30303) |
| **TCP** | Snapshot sync (full chain) | Base + 1000 (e.g., 31303) |

### UDP Gossip

- **Deduplication:** Packet ID tracking prevents re-processing
- **TTL:** Each packet has a hop count that decrements; dropped at 0
- **Peer discovery:** Periodic discovery loop (every 30s)
- **Peer pruning:** Stale peers removed periodically

```rust
pub struct GossipPacket {
    pub topic: String,     // "block", "tx", "sync_request", "sync_response"
    pub data: Vec<u8>,     // Serialized wire format
    pub id: String,        // Unique packet ID for dedup
    pub ttl: u8,           // Hop count
}
```

### TCP Snapshot Sync

On connection:
1. Requester sends `GossipPacket { topic: "sync_request" }`
2. Responder serializes all blocks as `Vec<WireBlock>` JSON
3. Requester imports blocks via `engine.import_block()`

### Wire Formats

| Type | Fields |
|------|--------|
| `WireTx` | from, to, value, data, gas_limit, gas_price, nonce, chain_id (all hex strings) |
| `WireBlock` | number, chain_id, gas_limit, gas_used, base_fee, coinbase, hash, proposer, finalized, state_root, total_reward, burned_reward, transactions, receipts |
| `WireReceipt` | success, gas_used, output, created_address, error |

### NetworkNode

Wraps `Engine` + `UdpGossip` for real P2P networking:

```rust
pub struct NetworkNode {
    gossip: Arc<Mutex<UdpGossip>>,
    running: Arc<AtomicBool>,
}
```

Spawns three background threads:
1. **gossip-listener** — Receives and processes incoming packets
2. **tcp-snapshot** — Serves sync requests
3. **peer-discovery** — Periodic peer discovery and pruning

---

## 14. RPC Interface

**File:** `src/rpc/rpc.rs` (~970 lines)

JSON-RPC 2.0 server built on `tiny_http` with CORS support and Ethereum-compatible method aliases.

### HTTP Endpoints

| Path | Method | Purpose |
|------|--------|---------|
| `/` | POST | JSON-RPC 2.0 dispatch |
| `/health` | GET | `{ status, height, chain_id }` |
| `/metrics` | GET | Prometheus text format |
| `*` | OPTIONS | CORS preflight (204) |

### Supported Methods

#### Core Chain

| Method | Eth Alias | Description |
|--------|-----------|-------------|
| `prime_chainId` | `eth_chainId` | Returns chain ID |
| `prime_blockNumber` | `eth_blockNumber` | Latest block height |
| `prime_getBalance` | `eth_getBalance` | Account balance |
| `prime_getCode` | `eth_getCode` | Account bytecode |
| `prime_getStorageAt` | `eth_getStorageAt` | Storage slot value |
| `prime_getTransactionCount` | `eth_getTransactionCount` | Account nonce |
| `prime_gasPrice` | `eth_gasPrice` | Current base fee |
| `prime_call` | `eth_call` | Simulate call (no state change) |
| — | `eth_estimateGas` | Gas estimation |
| — | `net_version` | Network version |
| — | `web3_clientVersion` | Client version |

#### Blocks & Transactions

| Method | Description |
|--------|-------------|
| `prime_getBlockByNumber` / `eth_getBlockByNumber` | Block by number (supports "latest") |
| `eth_getBlockByHash` | Block by hash |
| `prime_sendTransaction` / `eth_sendTransaction` | Submit transaction |
| `prime_getTransactionReceipt` / `eth_getTransactionReceipt` | Transaction receipt |
| `prime_getTransactionByHash` / `eth_getTransactionByHash` | Transaction by hash |
| `prime_getLogs` / `eth_getLogs` | Log filtering with block range, addresses, topics |
| `prime_getDomainEvents` | Domain events (CLOB + bridge) |

#### PrimeOrders

| Method | Description |
|--------|-------------|
| `primeorders_addMarket` | Create new market |
| `primeorders_submitOrder` | Submit order to CLOB |
| `primeorders_cancelOrder` | Cancel order |
| `primeorders_getOrderBook` | Get order book levels |
| `primeorders_getOpenOrders` | Get open orders for account |
| `primeorders_setMarginParams` | Set margin parameters |
| `primeorders_depositCollateral` | Deposit collateral |
| `primeorders_isLiquidatable` | Check liquidation eligibility |
| `primeorders_liquidate` | Liquidate account |

#### Bridge

| Method | Description |
|--------|-------------|
| `primebridge_enqueueOrdersToEvm` | Enqueue message: orders → EVM |
| `primebridge_enqueueEvmToOrders` | Enqueue message: EVM → orders |
| `primebridge_dequeueOrdersToEvm` | Dequeue message: orders → EVM |
| `primebridge_dequeueEvmToOrders` | Dequeue message: EVM → orders |

### Error Codes

| Code | Meaning |
|------|---------|
| -32700 | Parse error (invalid JSON) |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32000 | Internal error |
| -32005 | Transaction rejected (with `reason` in data) |

---

## 15. Cryptography

**File:** `src/crypto/mod.rs` (152 lines)

ECDSA secp256k1 signatures using the `k256` crate.

### Transaction Signing (EIP-155 Inspired)

```
signing_hash = keccak256(chain_id ‖ nonce ‖ gas_price ‖ gas_limit ‖ to ‖ value ‖ data)
```

All fields are big-endian. `to` is 20 bytes (or 20 zero bytes for contract creation).

### Signature Format

```rust
pub struct SignedTransaction {
    pub tx: Transaction,
    pub v: U256,     // recovery_id + 35 + chain_id * 2
    pub r: U256,     // Signature R component
    pub s: U256,     // Signature S component
}
```

The `v` value follows EIP-155: `v = recovery_id + 35 + chain_id × 2`

### Key Operations

| Function | Purpose |
|----------|---------|
| `tx_signing_hash(tx)` | Compute deterministic hash for signing |
| `sign_transaction(tx, key)` | Sign with `SigningKey`, returns `SignedTransaction` |
| `recover_signer(signed_tx)` | Recover `Address` from signature |
| `address_from_signing_key(key)` | Derive address from private key |
| `generate_keypair()` | Generate random (key, address) pair |

### Address Derivation

Standard Ethereum: `keccak256(uncompressed_public_key[1..])[12..]`

---

## 16. Bridge

**Files:** `src/core/bridge.rs`

Bidirectional message queue between the PrimeOrders domain and the EVM domain.

### Domains

| Domain | Description |
|--------|-------------|
| `PrimeOrders` | CLOB / order book domain |
| `PrimeEvm` | EVM execution domain |

### Queue Structure

```rust
pub struct BridgeQueue {
    queue: VecDeque<BridgeMessage>,
    next_nonce: u64,
    max_len: Option<usize>,
}

pub struct BridgeMessage {
    pub nonce: u64,
    pub from: BridgeDomain,
    pub to: BridgeDomain,
    pub payload: Bytes,
}
```

Two independent queues: `orders_to_evm` and `evm_to_orders`. Each message gets a monotonically increasing nonce. Optional `max_len` caps queue size.

Bridge state is persisted to sled on each block commit and restored on node startup.

---

## 17. Observability

### Prometheus Metrics

**File:** `src/prometheus/prometheus.rs` (137 lines)

Initialized once via `prometheus::init()`. All metrics are pre-described with HELP/TYPE annotations.

| Metric | Type | Description |
|--------|------|-------------|
| `prime_chain_up` | Gauge | Node liveness (always 1) |
| `prime_chain_blocks_produced_total` | Counter | Total blocks produced |
| `prime_chain_height` | Gauge | Latest block height |
| `prime_chain_block_gas_used` | Gauge | Gas in latest block |
| `prime_chain_block_tx_count` | Gauge | Tx count in latest block |
| `prime_chain_block_execution_seconds` | Histogram | Block execution time |
| `prime_chain_base_fee_wei` | Gauge | Current base fee |
| `prime_chain_consensus_rounds` | Counter | Consensus rounds executed |
| `prime_chain_consensus_finalized` | Counter | Blocks finalized |
| `prime_chain_slashing_events` | Counter | Slashing events by kind |
| `prime_chain_validators_active` | Gauge | Active validator count |
| `prime_chain_total_stake` | Gauge | Total staked amount |
| `prime_chain_orders_submitted` | Counter | Orders submitted |
| `prime_chain_orders_filled` | Counter | Orders fully filled |
| `prime_chain_orders_cancelled` | Counter | Orders cancelled |
| `prime_chain_trades_executed` | Counter | Trade fills executed |
| `prime_chain_insurance_fund_balance` | Gauge | Insurance fund balance |
| `prime_chain_markets_active` | Gauge | Active market count |
| `prime_chain_mempool_size` | Gauge | Mempool total size |
| `prime_chain_mempool_rejected` | Counter | Rejected txs by reason |
| `prime_chain_rpc_requests` | Counter | RPC requests by method |
| `prime_chain_rpc_errors` | Counter | RPC errors by method+code |
| `prime_chain_rpc_duration_seconds` | Histogram | RPC latency by method |
| `parallel_execution_total` | Counter | Parallel execution runs |
| `parallel_execution_groups` | Gauge | Groups in last parallel run |
| `hotstuff2_commits` | Counter | HotStuff-2 commits |
| `hotstuff2_timeouts` | Counter | HotStuff-2 timeouts |
| `tx_submitted_total` | Counter | Total submitted txs |

### Structured Logging

Uses `tracing` with `tracing-subscriber` (fmt + env-filter). Controlled via `RUST_LOG` environment variable.

Key log points:
- Block production: `height`, `txs`, `gas`, `finalized`, `base_fee`
- Consensus rounds: `round`, `prevote_stake`, `precommit_stake`, `finalized`
- Slashing evidence: `validator`, `kind`, `height`, `round`
- Trades: `taker`, `maker`, `market`, `price`, `size`
- Parallel execution: `groups`, `txs`

---

## 18. Configuration Reference

**File:** `src/config/config.rs` (360 lines)

JSON-based configuration loaded from a file path. All fields have defaults.

### Complete Configuration Schema

```json
{
  "engine": {
    "chain_id": 7919,
    "state_path": "state",
    "gas_limit_per_block": 30000000,
    "fee_elasticity_multiplier": 2,
    "fee_max_change_denominator": 8
  },
  "mempool": {
    "max_total": 10000,
    "max_per_sender": 1000,
    "bump_bps": 1000
  },
  "prime_orders": {
    "initial_margin_bps": 0,
    "maintenance_margin_bps": 0
  },
  "bridge": {
    "max_queue_len": 10000
  },
  "genesis": {
    "accounts": [
      {
        "address": "0x...",
        "balance": "1000000000000000000000",
        "nonce": 0
      }
    ]
  },
  "slashing": {
    "double_sign_bps": 500,
    "timeout_bps": 100,
    "escalation_step_bps": 25,
    "escalation_max_bps": 1000,
    "round_timeout_ms": 500,
    "unbonding_period": 2
  },
  "token_economics": {
    "max_supply": "1000000000000000000000000000",
    "initial_reward_per_block": "10000000000000000000",
    "halving_interval": 35000000
  },
  "rpc": {
    "enabled": false,
    "addr": "127.0.0.1:8545"
  },
  "p2p": {
    "node_key_path": "state/node_key.json",
    "peer_store_path": "state/peers.json",
    "listen": "0.0.0.0:30303",
    "peers": [],
    "block_time_ms": 1000
  }
}
```

### Default Values

| Parameter | Default | Unit |
|-----------|---------|------|
| `chain_id` | 7919 | — |
| `gas_limit_per_block` | 30,000,000 | gas |
| `fee_elasticity_multiplier` | 2 | — |
| `fee_max_change_denominator` | 8 | — |
| `mempool.max_total` | 10,000 | txs |
| `mempool.max_per_sender` | 1,000 | txs |
| `mempool.bump_bps` | 1,000 | basis points (10%) |
| `slashing.double_sign_bps` | 500 | basis points (5%) |
| `slashing.timeout_bps` | 100 | basis points (1%) |
| `slashing.escalation_step_bps` | 25 | basis points (0.25%) |
| `slashing.escalation_max_bps` | 1,000 | basis points (10%) |
| `slashing.round_timeout_ms` | 500 | milliseconds |
| `slashing.unbonding_period` | 2 | blocks |
| `token_economics.max_supply` | 1B × 10^18 (PRIM) | wei |
| `token_economics.initial_reward_per_block` | 10 × 10^18 (PRIM) | wei |
| `token_economics.halving_interval` | 35,000,000 | blocks |
| `rpc.addr` | 127.0.0.1:8545 | — |
| `p2p.listen` | 0.0.0.0:30303 | — |
| `p2p.block_time_ms` | 1,000 | milliseconds |

---

## 19. API Reference

### JSON-RPC Request Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "prime_blockNumber",
  "params": []
}
```

### Example: Submit Transaction

```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "prime_sendTransaction",
    "params": [{
      "from": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "to": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "value": "0x3e8",
      "gas": "0x5208",
      "gas_price": "0x1",
      "nonce": "0x0",
      "chain_id": 7919
    }]
  }'
```

### Example: Get Block

```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "prime_getBlockByNumber",
    "params": ["latest", true]
  }'
```

### Example: Submit CLOB Order via Precompile

From a Solidity contract:

```solidity
interface IPrimeOrders {
    function placeOrder(
        uint64 marketId,
        bool isBuy,
        uint256 price,
        uint256 size,
        uint8 tif
    ) external returns (uint256 orderId, uint256 filled, uint256 remaining);
}

// Address: 0x0000000000000000000000000000000000000100
IPrimeOrders clob = IPrimeOrders(0x0000000000000000000000000000000000000100);

// TimeInForce: 0=GTC, 1=IOC, 2=FOK
(uint256 id, uint256 filled, uint256 remaining) = clob.placeOrder(1, true, 50000, 100, 0);
```

### Example: CLOB via RPC

```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "primeorders_addMarket",
    "params": ["ETH-USD", "0x1", "0x1"]
  }'
```

### Health Check

```bash
curl http://localhost:8545/health
# {"status":"ok","height":42,"chain_id":7919}
```

### Prometheus Metrics

```bash
curl http://localhost:8545/metrics
# HELP prime_chain_height Latest committed block height
# TYPE prime_chain_height gauge
# prime_chain_height 42
```

---

## 20. Deployment Guide

### Docker Build

```bash
docker build -t prime-chain:latest .
```

The multi-stage Dockerfile uses `rust:1.85-slim` for building and `debian:bookworm-slim` for the runtime image.

**Exposed ports:**
- `8545` — JSON-RPC
- `9090` — Prometheus (application-level, served via `/metrics` on 8545)
- `9100` — Reserved

### Docker Compose (3-Validator Testnet)

```bash
docker-compose up -d
```

This starts three validator nodes:

| Service | RPC Port | Metrics Port | Config |
|---------|----------|--------------|--------|
| `validator-1` | 8545 | 9090 | `testnet/configs/validator-1.json` |
| `validator-2` | 8546 | 9091 | `testnet/configs/validator-2.json` |
| `validator-3` | 8547 | 9092 | `testnet/configs/validator-3.json` |

Each validator has:
- Health check: `curl -sf http://localhost:8545/health`
- Restart policy: `unless-stopped`
- `RUST_LOG=info`
- Persistent data volume

### Manual Testnet Setup

1. Generate validator configs:

```json
{
  "engine": {
    "chain_id": 7919,
    "state_path": "/data/state"
  },
  "genesis": {
    "accounts": [
      { "address": "0x...", "balance": "1000000000000000000000000" }
    ]
  },
  "rpc": {
    "enabled": true,
    "addr": "0.0.0.0:8545"
  },
  "p2p": {
    "listen": "0.0.0.0:30303",
    "peers": ["validator-2:30303", "validator-3:30303"],
    "block_time_ms": 1000
  },
  "slashing": {
    "double_sign_bps": 500,
    "timeout_bps": 100,
    "round_timeout_ms": 500,
    "unbonding_period": 2
  },
  "token_economics": {
    "max_supply": "1000000000000000000000000000",
    "initial_reward_per_block": "10000000000000000000",
    "halving_interval": 35000000
  }
}
```

2. Start nodes:

```bash
prime-chain --config /etc/prime-chain/config.json --validator --rpc
```

3. Verify:

```bash
curl http://localhost:8545/health
curl http://localhost:8545/metrics
```

---

## 21. Performance Tuning

### Parallel Execution

| Parameter | Effect | Recommendation |
|-----------|--------|----------------|
| `PARALLEL_THRESHOLD` | Minimum tx count for parallel path | 8 (compile-time constant) |
| Thread count | `available_parallelism()` | Matches CPU cores automatically |
| `max_retries` | MVCC conflict retries | 3 (hardcoded in engine) |

**Workload characteristics for parallel benefit:**
- Many independent senders (different `from` / `to` addresses)
- Contract calls to different contracts
- Minimal shared state (storage slot contention)

**Degradation to sequential:** Single sender chains, DEX swaps to same pool, coinbase-heavy workloads (coinbase is excluded from grouping, but if all txs share other addresses, they collapse).

### Gas & Fee Market

| Parameter | Tuning Effect |
|-----------|---------------|
| `gas_limit_per_block` | Higher → more txs per block, lower finality density |
| `fee_elasticity_multiplier` | Higher → narrower target band, more volatile fees |
| `fee_max_change_denominator` | Higher → slower fee adjustment |

### Mempool

| Parameter | Tuning Effect |
|-----------|---------------|
| `max_total` | Higher → more pending txs, more memory |
| `max_per_sender` | Higher → allows longer nonce chains |
| `bump_bps` | Lower → easier replacement, higher spam risk |

### CLOB Performance

- Order book uses `BTreeMap<U256, VecDeque<OrderId>>` — O(log n) insert/lookup per price level
- FIFO queues within price levels — O(1) front/back operations
- FOK orders check `available_liquidity` before matching — avoids partial execution overhead

### FBA Tuning

| Parameter | Default | Effect |
|-----------|---------|--------|
| `batch_interval_ms` | 100 | Shorter → lower latency, less batch aggregation |

### Consensus Timing

| Parameter | Default | Effect |
|-----------|---------|--------|
| `round_timeout_ms` | 500 | Lower → faster finality, more timeout risk |
| `timeout_delta_ms` | 100 | Progressive backoff per round |
| HotStuff-2 `round_timeout_base_ms` | 3000 | Base timeout for HotStuff-2 rounds |
| HotStuff-2 `round_timeout_delta_ms` | 500 | Linear backoff per round |

### State Pruning

Periodic pruning of old blocks and height metadata via `prune_before(height)`. Recommended to prune blocks older than the finality window to keep sled compact.

### Measured Benchmarks

| Component | Measured Performance | Notes |
|-----------|----------------------|-------|
| EVM (parallel, 8 cores) | 72,181 TPS | Independent transfers, release mode |
| CLOB (PrimeOrders) | 2,484,170 ops/s | Native matching engine |
| FBA (Frequent Batch Auctions) | 5,053,782 ops/s | Clearing price + pro-rata |
| HotStuff-2 | 0.001 ms/round | Per-round latency |

---

## 22. Competitive Comparison

### Hyperliquid HyperEVM Architecture

Hyperliquid operates a dual-execution architecture:

- **HyperEVM** is a Cancun-spec EVM running alongside **HyperCore** (native CLOB) under HyperBFT consensus.
- The two are **separate execution environments** that run sequentially.
- EVM reads HyperCore state from the **previous block** — 1 block stale.
- **CoreWriter** at `0x333...333` queues orders for the **next block** — seconds delay.
- **Dual blocks**: big blocks (~1 min, 30M gas) and small blocks (~1 sec, 2M gas).
- HyperEVM remains in **alpha** as of 2026.

### EVM ↔ CLOB Composability Comparison

| Aspect | Mersennet (Precompile 0x0100) | Hyperliquid (HyperEVM) |
|--------|--------------------------------|------------------------|
| **Composability** | Atomic same-transaction | Async next-block |
| **Read latency** | Current block (live) | Previous block (1 block stale) |
| **Write latency** | Immediate (same tx) | CoreWriter queues for next block (seconds) |
| **Use case** | Vault: deposit → place order → react to fill in one tx | Requires multi-tx, multi-block flow |

Mersennet's CLOB precompile achieves **atomic same-tx composability** — a Solidity contract can deposit collateral, place an order, and react to the fill in a single transaction. HyperEVM's CoreWriter pattern is intentionally async, trading composability for maximum CLOB throughput.

---

## 23. Security Considerations

### Consensus Safety

- **BFT tolerance:** System is safe with up to `f < n/3` Byzantine validators (stake-weighted)
- **2-chain commit rule:** HotStuff-2 requires two consecutive QCs for commit — prevents spurious commits
- **Locked QC safety:** Validators only vote if the proposal's justify QC is at least as high as their locked QC
- **No equivocation:** `last_voted_round` prevents double-voting within a round
- **Tombstoning:** Double-sign evidence results in permanent validator exclusion

### Slashing Escalation

Repeat offenders face escalating penalties: `base_bps + escalation_step × offense_count`. Capped at `max_escalation_bps`.

### MEV Protection

- **Commit-reveal:** Transactions are committed as `keccak256(tx ‖ salt)` before revealing contents. Block producers cannot reorder based on transaction content during the commit phase.
- **FBA:** Batch auctions eliminate speed-based MEV by executing all orders at a uniform clearing price.

### Precompile Security

- **Gas metering:** Every precompile function checks `gas_limit ≥ required_gas` before execution
- **Context isolation:** CLOB state is set before block execution and cleared after — no cross-block state leakage
- **Caller identification:** `env.tx.caller` is used for all auth-sensitive operations (deposit, withdraw, position queries)

### Transaction Validation

- **Signature verification:** ECDSA recovery verifies `recovered_address == tx.from`
- **Replay protection:** Chain ID in signature hash (EIP-155 style)
- **Nonce ordering:** Strict nonce enforcement prevents double-spending
- **Balance checks:** `gas_limit × gas_price + value ≤ balance` verified at submission and execution

### State Integrity

- **Merkle state root:** Every block includes a Merkle root covering all state trees
- **State proofs:** Verifiable Merkle inclusion proofs for any key in the state
- **Snapshot verification:** Imported snapshots are verified against the embedded state root

### Known Limitations

- Unsigned transactions are accepted with a warning (backward compatibility) — production deployments should enforce signatures
- CLOB margin system uses `initial_margin_bps = 0` by default — must be configured for production
- UDP gossip is unencrypted — suitable for testnet; production should use encrypted transport
- The `InMemoryDB` clone during parallel execution has memory overhead proportional to state size

---

## 24. Testing

### Test Files

| File | Lines | Coverage |
|------|-------|----------|
| `tests/state_tests.rs` | ~500 | State persistence, Merkle tree, proofs, pruning, snapshots |
| `tests/consensus_tests.rs` | ~300 | Validator management, slashing, finality rounds, rewards |
| `tests/bridge_tests.rs` | ~200 | Bridge queue operations, persistence |
| `tests/rpc_tests.rs` | ~300 | JSON-RPC method dispatch, error handling |
| `tests/crypto_tests.rs` | ~150 | ECDSA signing, recovery, determinism |
| `tests/prime_orders_advanced.rs` | ~400 | CLOB matching, margin, liquidation, ADL |
| `tests/fuzz_mempool.rs` | ~200 | Fuzz testing for mempool operations |
| Inline tests in `hotstuff2.rs` | ~160 | Quorum threshold, 2-chain commit, timeouts, safety |
| Inline tests in `crypto/mod.rs` | ~50 | Sign/recover roundtrip |

### Running Tests

```bash
# All tests
cargo test

# Specific test file
cargo test --test state_tests

# With logging
RUST_LOG=debug cargo test -- --nocapture
```

### TPS Benchmark

```bash
cargo run --release --bin tps-bench
```

Measures transactions-per-second under controlled conditions.

### Fuzz Testing

The mempool fuzz test (`tests/fuzz_mempool.rs`) generates random transactions and exercises insert/evict/replace/promote/demote paths to verify invariants under adversarial input.

### Key Test Scenarios

**HotStuff-2:**
- Quorum threshold calculation: 4 validators × 100 stake → threshold = 267
- 2-chain commit: QC at round 0 + QC at round 1 → commit block from round 0
- Duplicate vote rejection
- Timeout-driven round advance
- Safety: no voting on rounds ≤ `last_voted_round`

**Parallel Execution:**
- Independent transactions execute in parallel groups
- Conflicting transactions fall back to sequential
- Coinbase balances correctly accumulated across groups

**CLOB:**
- Price-time priority matching
- GTC/IOC/FOK order types
- Margin validation at match time
- Liquidation triggers and insurance fund draw
- Auto-deleveraging when insurance insufficient

---

## Appendix A: Block Structure

```rust
pub struct Block {
    pub number: u64,
    pub chain_id: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub base_fee: U256,
    pub coinbase: Address,
    pub hash: B256,
    pub proposer: Address,
    pub finalized: bool,
    pub consensus: Finalization,
    pub unbonded: Vec<Unbonding>,
    pub applied_validator_changes: Vec<ValidatorChange>,
    pub slashes: Vec<Slashing>,
    pub finality_rounds: Vec<RoundResult>,
    pub slashing_evidence: Vec<SlashingEvidence>,
    pub rewards: Vec<Reward>,
    pub total_reward: U256,
    pub burned_reward: U256,
    pub state_root: B256,
    pub transactions: Vec<Transaction>,
    pub receipts: Vec<Receipt>,
    pub bridge_orders_to_evm: Vec<BridgeMessage>,
    pub bridge_evm_to_orders: Vec<BridgeMessage>,
    pub domain_events: Vec<DomainEvent>,
}
```

## Appendix B: Transaction Structure

```rust
pub struct Transaction {
    pub from: Address,
    pub to: Option<Address>,       // None = contract creation
    pub value: U256,
    pub data: Bytes,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub nonce: u64,
    pub chain_id: Option<u64>,
    pub signature: Option<(U256, U256, u64)>,  // (r, s, v)
}
```

## Appendix C: Domain Events

Events emitted by the engine for indexing and RPC subscription:

| Domain | Event Kind | Fields |
|--------|-----------|--------|
| PrimeOrders | `MarketAdded` | market_id, symbol, tick_size, lot_size |
| PrimeOrders | `OrderSubmitted` | order_id, owner, market_id, side, price, size, tif, filled, remaining |
| PrimeOrders | `OrderCancelled` | order_id, owner, market_id |
| PrimeOrders | `Trade` | taker, maker, market_id, side, price, size |
| PrimeOrders | `MarginParamsUpdated` | initial_bps, maintenance_bps |
| PrimeOrders | `CollateralDeposited` | owner, amount |
| PrimeOrders | `Liquidation` | owner, liquidated |
| Bridge | `Enqueued` | queue, nonce, from, to, payload |
| Bridge | `Dequeued` | queue, nonce, from, to, payload |
