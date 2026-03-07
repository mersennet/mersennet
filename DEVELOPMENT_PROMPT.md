# Prime Chain: Comprehensive Development Prompt

> **Purpose:** This document is a self-contained engineering brief for building Prime Chain from prototype to production. It encodes deep knowledge extracted from studying the full GitHub repositories of Reth, CometBFT, Cosmos SDK, Substrate/Polkadot, Solana (Agave), dYdX v4, Sei Network, and Hyperliquid — cross-referenced against Prime Chain's v5.0 whitepaper and existing Rust codebase.

---

## 1. What Prime Chain Is

Prime Chain is a **Layer 1 blockchain written in Rust** that unifies three execution domains under a single consensus layer and state root:

| Domain | Purpose | Analogues |
|--------|---------|-----------|
| **PrimeEVM** | Full EVM (Shanghai spec via `revm`) for general smart contracts | Ethereum, Reth |
| **PrimeOrders** | Native deterministic CLOB with price-time priority, margin, liquidation | dYdX v4, Hyperliquid |
| **Bridge** | FIFO message queues with nonce-based replay protection for cross-domain calls | Polkadot XCMP (simplified) |

**The unique value proposition**: Atomic EVM-to-order-book composability within a single block and state root. dYdX has no EVM. Hyperliquid's EVM writes are async. Sei deprecated its native DEX. No production chain achieves what Prime Chain targets.

**Current state**: Working prototype with `src/` containing engine, consensus, PrimeOrders, bridge, mempool, RPC, P2P, governance, identity, and metrics modules. Integration tests exist. No mainnet. No transaction signature verification. Network is unencrypted. P2P is partial.

---

## 2. Architecture Principles (Learned from the Best)

### 2.1 Trait-Based Domain Separation (from Reth)

Reth splits every domain into **traits crate + implementation crate**. Prime Chain must adopt this pattern:

```
crates/
  consensus/
    consensus-api/        # Traits: Consensus, HeaderValidator, FullConsensus
    consensus-impl/       # Implementation: PrimeConsensus
  execution/
    evm-api/              # Traits: EvmExecutor, BlockExecutor
    evm-impl/             # Implementation: PrimeEvmExecutor (revm)
    orders-api/           # Traits: MatchingEngine, RiskEngine, OrderBook
    orders-impl/          # Implementation: PrimeOrders
  bridge/
    bridge-api/           # Traits: BridgeQueue, MessageRouter
    bridge-impl/          # Implementation: PrimeBridge
  storage/
    storage-api/          # Traits: StateProvider, AccountReader, StorageReader
    storage-impl/         # Implementation: SledStateDB (→ later MDBX/RocksDB)
  network/
    network-api/          # Traits: NetworkHandle, PeerManager
    network-impl/         # Implementation: UDP gossip + TCP sync
  pool/
    pool-api/             # Traits: TransactionPool, TransactionValidator
    pool-impl/            # Implementation: FeeOrderedMempool
  rpc/
    rpc-api/              # Traits: RpcServer, method definitions
    rpc-impl/             # Implementation: JSON-RPC server
  primitives/             # Shared types: Block, Transaction, Receipt, Address, U256
  primitives-traits/      # no_std trait-only crate for maximum portability
```

**Why**: Enables pluggable implementations, clean testing with mocks, and future chain variants. Reth supports Ethereum + Optimism from the same codebase because of this pattern.

### 2.2 Consensus-Execution Decoupling (from CometBFT ABCI)

CometBFT's ABCI (Application Blockchain Interface) cleanly separates consensus from application logic. Prime Chain should formalize a similar boundary:

```rust
trait ExecutionEngine: Send + Sync {
    fn prepare_proposal(&self, state: &State, txs: Vec<Transaction>) -> ProposalBlock;
    fn process_proposal(&self, state: &State, block: &Block) -> Result<(), ValidationError>;
    fn finalize_block(&self, state: &mut State, block: &Block) -> BlockResult;
    fn commit(&self, state: &mut State) -> StateRoot;
}
```

**Key ABCI 2.0 methods to mirror**:
- `PrepareProposal`: Proposer can reorder/filter transactions (MEV protection opportunity)
- `ProcessProposal`: Validators reject invalid proposals (prevote `nil`)
- `FinalizeBlock`: Apply all state transitions atomically
- `Commit`: Persist state, return state root

### 2.3 Parallel Execution Where Possible (from Solana Sealevel)

Solana achieves parallelism by requiring transactions to declare account access upfront. Prime Chain can apply this to PrimeOrders:

- Order submissions declare the target `market_id`
- Orders targeting different markets have **zero state overlap** → match in parallel
- Use Rayon or Tokio tasks to process independent markets concurrently
- EVM transactions remain sequential (EVM semantics require it)
- Bridge messages are processed sequentially (nonce ordering)

**Estimated impact**: 4-8x throughput improvement on multi-market workloads with 8 cores.

### 2.4 Trie-Based State Roots (from Substrate + Reth)

**Current approach** (whitepaper §3.2): Flat sorted hash `R = H(sort({(k,v)}))`. This is O(N log N) per block and doesn't support:
- Incremental updates (must rehash everything)
- Membership proofs (no Merkle paths)
- Light client verification

**Target approach**: Modified Merkle Patricia Trie with child tries per domain:

```
State Root
├── Child Trie: S_evm (accounts, storage, code)
├── Child Trie: S_orders (markets, orders, positions, collateral)
└── Child Trie: S_bridge (queue_o2e, queue_e2o, nonces)
```

Each child trie produces an independent root hash. The global state root commits all three. This gives:
- O(log N) incremental updates per modified key
- O(log N) membership/non-membership proofs
- Light client support with compact state proofs

### 2.5 Pipelined Block Processing (from Solana TPU)

Solana pipelines four stages across different hardware. Prime Chain should pipeline:

```
Stage 1 (Network)    → Receive transactions, forward to pool
Stage 2 (CPU/GPU)    → Validate signatures, check nonces (can parallelize)
Stage 3 (CPU)        → Execute EVM txs + match PrimeOrders + process bridge
Stage 4 (Disk)       → Commit state, compute state root, persist
```

While Stage 4 commits block N, Stage 3 can begin executing block N+1's transactions. This hides disk latency behind computation.

---

## 3. Consensus Implementation Guide

### 3.1 BFT Finality (from CometBFT + Whitepaper §4)

The consensus protocol follows Tendermint's round-based model:

```
NewHeight → (Propose → Prevote → Precommit)+ → Commit → NewHeight
```

**Critical implementation details from CometBFT**:

1. **Locking mechanism**: When a validator sees +2/3 prevotes for block B at round R, it **locks** on B and sets `LockedRound = R`. It will only unlock if it sees a Proof-of-Lock-Change (PoLC) at a later round R' > R. This prevents equivocation.

2. **Proposer selection**: Weighted round-robin with priority scaling. Each validator maintains a priority value `A(v)`. On each round: `A(v) += stake(v)`, elect `argmax(A)`, subtract total stake from winner. New validators get penalty `A(v) = -1.125 * TotalStake` to prevent join/leave gaming.

3. **Timeout progression**: Timeouts increase with each failed round: `timeout(R) = baseTimeout + R * timeoutDelta`. This guarantees liveness — eventually a round will have enough time for honest communication.

4. **Evidence handling**: Double-sign evidence (two conflicting votes at same height/round) must be gossiped, verified, and included in blocks. Use a dedicated evidence pool with TTL. Slash based on evidence, not just local observation.

5. **Canonical vs. subjective commit**: Each validator sees their own commit set. The **canonical commit** is what the next proposer includes in `LastCommit`. Always use the canonical commit for protocol logic.

### 3.2 Validator Set Management (from Cosmos x/staking)

- Active validator set capped at `MaxValidators` (e.g., 100)
- Validators enter via staking transactions; exit via unbonding
- Validator set updates applied at epoch boundaries (end of block)
- Changes queued: slashes → unbonds → new stakes (order matters for correctness)
- Unbonding period: 21 days equivalent (configurable blocks), during which stake is still slashable

### 3.3 Slashing (from Cosmos x/slashing + dYdX)

| Infraction | Detection | Penalty | Additional |
|------------|-----------|---------|------------|
| Double-sign | Evidence in block | 5% stake | Tombstone (permanent ban) |
| Downtime | Missed blocks counter | 0.01-1% stake | Jail (temporary, unjailable) |

**Implementation details**:
- Track `ValidatorSigningInfo` per validator: start height, index offset, jailed_until, tombstoned, missed_blocks_counter
- Use a sliding window (`SignedBlocksWindow`) to detect liveness failures
- Evidence has a max age (`MaxAgeNumBlocks`) — old evidence is ignored
- Escalation (whitepaper §4.6.3): `P_escalated = P_base + (offenses * step) + (consecutive_misses * step)`, capped at `escalation_max`

---

## 4. PrimeOrders Matching Engine — Production Hardening

### 4.1 Dual-State Model (from dYdX v4)

dYdX's most important insight: not all orders belong in consensus state.

| Order Type | Storage | Consensus | Rationale |
|------------|---------|-----------|-----------|
| **Short-term** (IOC, FOK, short-lived limit) | In-memory only | Not in committed state | 99% of HFT volume; persisting would choke consensus |
| **Stateful** (GTC, conditional) | On-chain | In committed state | Must survive restarts; user expects persistence |

**Implementation**:
- Short-term orders live in `MemClob` — a per-validator in-memory data structure
- Each block, the proposer runs matching against `MemClob` and includes resulting fills in the block
- Other validators verify fills deterministically during `ProcessProposal`
- Short-term orders are replayed from the mempool on each new block (like re-CheckTx in CometBFT)
- Stateful orders are persisted in state and loaded into `MemClob` at block start

### 4.2 Order Book Data Structures (from dYdX + Hyperliquid)

dYdX uses **hash maps with cached best bid/ask** rather than sorted trees:

```rust
struct OrderBook {
    bids: HashMap<Price, PriceLevel>,  // All bid levels
    asks: HashMap<Price, PriceLevel>,  // All ask levels
    best_bid: Option<Price>,           // Cached — O(1) top-of-book
    best_ask: Option<Price>,           // Cached — O(1) top-of-book
}

struct PriceLevel {
    orders: VecDeque<OrderId>,  // FIFO within level
    total_size: u64,            // Cached aggregate
}
```

**Why not BTreeMap** (current Prime Chain approach):
- Hash maps are faster for point lookups and insertions
- Best bid/ask cache gives O(1) top-of-book access (vs. O(log N) for BTreeMap min/max)
- Cache invalidation only on fills/cancels that remove the best level
- Trade-off: range queries (get N best levels) require sorting — but this is only needed for RPC display, not matching

### 4.3 Memory Pooling (from dYdX)

dYdX recently achieved **30-40% latency reduction** with zero-allocation matching:

```rust
struct OrderPool {
    free_orders: Vec<Box<Order>>,  // Recycled order objects
    active_orders: HashMap<OrderId, Box<Order>>,
}

impl OrderPool {
    fn acquire(&mut self) -> Box<Order> {
        self.free_orders.pop().unwrap_or_else(|| Box::new(Order::default()))
    }
    fn release(&mut self, order: Box<Order>) {
        self.free_orders.push(order);
    }
}
```

Pre-allocate order objects, recycle on cancel/fill. Eliminates allocator pressure in the hot matching path.

### 4.4 Semantic Transaction Ordering (from Hyperliquid)

Within each block, order transactions by type:

1. **Liquidations** (highest priority — risk management)
2. **Order cancellations** (free up margin)
3. **Order placements** (new orders)
4. **EVM transactions** (general computation)
5. **Bridge messages** (cross-domain)

This is a zero-cost optimization: the proposer simply sorts transactions before inclusion. It ensures risk management runs before new orders can worsen exposure.

### 4.5 Match-Time Margin Validation (from dYdX + Hyperliquid)

**Current gap**: Prime Chain validates margin at order placement time only.

**Production requirement**: Validate at **fill time** too, because:
- Oracle prices may have moved since placement
- Other fills may have consumed collateral
- Position sizes may have changed

```rust
fn execute_fill(taker: &Order, maker: &Order, fill_size: u64, price: u64) -> Result<Trade> {
    let taker_account = self.accounts.get_mut(&taker.owner)?;
    let maker_account = self.accounts.get_mut(&maker.owner)?;

    // Compute post-fill positions
    let taker_new_position = taker_account.position(taker.market) + fill_size;
    let maker_new_position = maker_account.position(maker.market) - fill_size;

    // Validate margin at current mark price (not order price)
    let mark_price = self.get_mark_price(taker.market);
    ensure!(taker_account.equity(mark_price) >= margin_required(taker_new_position, mark_price));
    ensure!(maker_account.equity(mark_price) >= margin_required(maker_new_position, mark_price));

    // Execute fill...
}
```

### 4.6 Insurance Fund and Deleveraging (CRITICAL GAP)

**No production trading system operates without**:

1. **Insurance Fund**: Absorbs losses from underwater liquidations (when liquidation price < bankruptcy price). Funded by a fraction of liquidation penalties and trading fees.

2. **Auto-Deleveraging (ADL)**: When the insurance fund is depleted, profitable traders on the opposite side are force-closed in order of profit/leverage ratio. This is the backstop mechanism.

3. **Socialized Loss**: Alternative to ADL — spread losses across all profitable positions. Less targeted, more equitable.

```rust
struct InsuranceFund {
    balance: U256,
    contribution_rate_bps: u16,  // Fraction of liquidation penalty
}

fn liquidate(&mut self, account: Address) -> LiquidationResult {
    let deficit = account.negative_equity();
    if deficit > 0 {
        if self.insurance_fund.balance >= deficit {
            self.insurance_fund.balance -= deficit;
        } else {
            let remaining = deficit - self.insurance_fund.balance;
            self.insurance_fund.balance = 0;
            self.auto_deleverage(remaining);
        }
    }
}
```

### 4.7 MEV Protection (from dYdX — CRITICAL GAP)

Block proposers in Prime Chain can currently:
- **Front-run**: See incoming orders and place their own first
- **Sandwich**: Wrap victim orders between two exploitative orders
- **Censor**: Exclude transactions selectively

**Mitigation strategies** (from most to least practical):

1. **Semantic ordering** (immediate): Sort by type, then by hash within type. Removes proposer discretion.

2. **Frequent Batch Auctions (FBA)** (medium-term): All orders submitted in a block execute at a **single clearing price** per market. Eliminates front-running because order of arrival within a block doesn't affect execution price.

3. **Encrypted mempools** (long-term): Orders are encrypted with threshold keys; only revealed after block inclusion is committed. Prevents observation-based MEV.

4. **ABCI++ Vote Extensions** (from dYdX): Validators include order data in their precommit vote extensions. The next proposer must include all valid extensions, preventing censorship.

---

## 5. Storage and State Management

### 5.1 Database Selection

| Database | Used By | Strengths | Weaknesses |
|----------|---------|-----------|------------|
| **sled** (current) | Prime Chain | Pure Rust, embedded | Unmaintained, known corruption bugs |
| **MDBX** | Reth | Fastest reads (mmap), battle-tested | C dependency, complex configuration |
| **RocksDB** | Substrate, Cosmos | Mature, well-understood | C++ dependency, write amplification |
| **Custom** | Solana (AccountsDB) | Optimized for specific access patterns | High development cost |

**Recommendation**: Migrate from sled to **MDBX** (via `libmdbx-rs`). Rationale:
- Reth proves MDBX works for blockchain workloads at scale
- Memory-mapped I/O gives fast reads (critical for state lookups during execution)
- ACID transactions with multi-reader concurrency
- Named databases (tables) for domain separation

### 5.2 Storage Schema (inspired by Reth)

```
MDBX Tables:
├── accounts:           Address → AccountRecord
├── storage:            (Address, Slot) → U256
├── code:               CodeHash → Bytecode
├── markets:            MarketId → Market
├── orders:             OrderId → Order
├── positions:          (Address, MarketId) → Position
├── order_accounts:     Address → OrdersAccount
├── bridge_o2e:         Nonce → BridgeMessage
├── bridge_e2o:         Nonce → BridgeMessage
├── blocks:             BlockNumber → Block
├── block_hashes:       B256 → BlockNumber
├── receipts:           TxHash → Receipt
├── events:             (BlockNumber, EventIndex) → DomainEvent
├── validators:         Address → Validator
├── snapshots:          Height → SnapshotMetadata
└── config:             Key → Value
```

### 5.3 State Root Computation

**Target**: Modified Merkle Patricia Trie with three child tries.

```
GlobalRoot = H(EvmRoot || OrdersRoot || BridgeRoot || Height || PrevRoot)

EvmRoot:
  AccountsTrie: Address → H(balance || nonce || codeHash || storageRoot)
    StorageTrie[addr]: Slot → Value

OrdersRoot:
  MarketsTrie: MarketId → H(market_data)
  PositionsTrie: (Address, MarketId) → H(position_data)
  AccountsTrie: Address → H(collateral || open_orders_root)

BridgeRoot:
  H(o2e_queue_hash || e2o_queue_hash || o2e_nonce || e2o_nonce)
```

**Implementation path**: Start with the current flat-hash approach (working, correct), add trie-based computation as a parallel verification step, then switch when verified to produce identical roots.

---

## 6. Networking

### 6.1 P2P Architecture (from Reth + CometBFT)

**Reactor pattern** (CometBFT): Each subsystem registers as a Reactor with the Switch:

```rust
trait Reactor: Send + Sync {
    fn on_peer_connected(&self, peer: PeerId);
    fn on_peer_disconnected(&self, peer: PeerId);
    fn on_message(&self, peer: PeerId, channel: ChannelId, msg: Bytes);
    fn channels(&self) -> Vec<ChannelDescriptor>;
}

// Reactors:
// - ConsensusReactor (votes, proposals, block parts)
// - MempoolReactor (transaction gossip)
// - EvidenceReactor (slashing evidence)
// - BlockSyncReactor (block download)
// - StateSyncReactor (snapshot transfer)
// - PeerExchangeReactor (peer discovery)
```

**Connection management** (from Reth):
- Separate spawned tasks for: discovery, session management, tx propagation, request handling
- `NetworkManager` as the central state machine routing messages between tasks
- Channel-based inter-task communication (tokio mpsc/watch/broadcast)

### 6.2 Message Propagation

**Current**: Simple gossip with TTL. 
**Target**: Hybrid approach:

| Message Type | Propagation | Rationale |
|-------------|-------------|-----------|
| Blocks | Erasure-coded tree (Turbine-style) | Large, must reach all validators fast |
| Transactions | Gossip to known peers | Small, can tolerate some redundancy |
| Votes | Direct to peers | Time-critical for finality |
| Evidence | Periodic broadcast | Not time-critical, must eventually reach all |
| Snapshots | TCP point-to-point | Large, one-time transfer |

### 6.3 Encryption (CRITICAL GAP)

Current: Unencrypted UDP/TCP.
Target: Noise protocol (like Substrate/libp2p) or ECIES (like Reth/Ethereum).

At minimum, encrypt all validator-to-validator communication. Transactions in the mempool are especially sensitive (MEV).

---

## 7. Transaction Pool (Mempool)

### 7.1 Multi-Pool Architecture (from Reth)

Reth's 4-subpool design:

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│ Pending   │    │ BaseFee   │    │ Queued    │    │ PrimeOrd │
│ (ready)   │◄──►│ (below    │◄──►│ (nonce    │    │ (order   │
│           │    │  basefee) │    │  gap)     │    │  pool)   │
└──────────┘    └──────────┘    └──────────┘    └──────────┘
```

Add a **PrimeOrders pool** for order transactions that bypasses EVM fee logic and uses order-specific validation (margin checks, market existence).

### 7.2 Validation Pipeline (from CometBFT AnteHandler)

Decorator pattern for composable validation:

```rust
fn validate_transaction(tx: &Transaction) -> Result<ValidTx> {
    // 1. Structural validation (size, format, chain_id)
    validate_structure(tx)?;
    // 2. Signature verification (ECDSA recovery)
    verify_signature(tx)?;
    // 3. Nonce check (>= account nonce)
    check_nonce(tx)?;
    // 4. Balance check (>= value + gas_limit * gas_price)
    check_balance(tx)?;
    // 5. Fee check (gas_price >= base_fee)
    check_fee(tx)?;
    // 6. Gas limit check (<= block_gas_limit)
    check_gas_limit(tx)?;

    Ok(ValidTx { priority: tx.gas_price - base_fee, ..})
}
```

### 7.3 Transaction Forwarding (from Solana Gulf Stream)

Since Prime Chain has a deterministic proposer schedule (`proposer(h) = validators[h % n]`), transactions can be forwarded directly to the next proposer:

```rust
fn forward_transaction(&self, tx: Transaction) {
    let next_proposer = self.validator_set.proposer(self.height + 1);
    self.network.send_to(next_proposer, tx);
    // Also gossip to 2-3 peers as backup
    self.network.gossip(tx, fanout: 3);
}
```

This reduces inclusion latency by one block hop.

---

## 8. RPC Layer

### 8.1 Ethereum-Compatible Endpoints

Maintain compatibility with existing Ethereum tooling (MetaMask, ethers.js, Hardhat):

```
eth_chainId, eth_blockNumber, eth_getBalance, eth_getBlockByNumber,
eth_getBlockByHash, eth_sendRawTransaction, eth_getTransactionReceipt,
eth_getTransactionByHash, eth_call, eth_estimateGas, eth_gasPrice,
eth_getCode, eth_getStorageAt, eth_getLogs, eth_getTransactionCount,
net_version, web3_clientVersion
```

### 8.2 Prime Chain Custom Endpoints

```
// PrimeOrders
primeorders_addMarket, primeorders_submitOrder, primeorders_cancelOrder,
primeorders_getOrderBook, primeorders_getOpenOrders, primeorders_getPosition,
primeorders_getPositions, primeorders_depositCollateral, primeorders_withdrawCollateral,
primeorders_liquidate, primeorders_getMarkets, primeorders_getTrades,
primeorders_getInsuranceFund

// Bridge
primebridge_enqueueOrdersToEvm, primebridge_enqueueEvmToOrders,
primebridge_dequeueOrdersToEvm, primebridge_dequeueEvmToOrders,
primebridge_getQueueStatus

// Governance
prime_submitProposal, prime_vote, prime_getProposals, prime_getProposal

// Admin
prime_getValidators, prime_getValidatorInfo, prime_getConfig,
prime_getDomainEvents, prime_getSnapshot

// Subscriptions (WebSocket)
prime_subscribe (newHeads, newPendingTransactions, logs, newTrades, orderBookUpdates)
```

### 8.3 Transport (from Reth)

- **HTTP**: For standard request-response (ethers.js, web3.py)
- **WebSocket**: For subscriptions (real-time trade feeds, order book updates)
- **IPC**: For local node management
- **Auth Server**: JWT-authenticated endpoint for admin operations

---

## 9. Testing Strategy

### 9.1 Test Pyramid (from Reth + Substrate)

```
                    /\
                   /  \  E2E Tests (multi-node testnet)
                  /    \
                 /──────\
                / Integ  \  Integration Tests (full block execution)
               /  Tests   \
              /────────────\
             / Property &   \  Property Tests (proptest), Fuzz Tests
            / Fuzz Tests     \
           /──────────────────\
          /    Unit Tests       \  Per-module with mocks
         /________________________\
```

### 9.2 Test Infrastructure

```rust
// test-utils feature flag on every crate (from Reth)
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils {
    pub struct MockConsensus { ... }
    pub struct MockStateDB { ... }
    pub struct MockNetwork { ... }
    pub struct NoopMempool { ... }
}

// Property-based tests (from Reth — proptest)
proptest! {
    #[test]
    fn matching_is_deterministic(orders in vec(arb_order(), 1..100)) {
        let result1 = match_orders(orders.clone());
        let result2 = match_orders(orders);
        assert_eq!(result1, result2);
    }

    #[test]
    fn state_root_is_consistent(txs in vec(arb_transaction(), 1..50)) {
        let root1 = execute_and_compute_root(txs.clone());
        let root2 = execute_and_compute_root(txs);
        assert_eq!(root1, root2);
    }
}
```

### 9.3 Critical Test Cases for PrimeOrders

1. **Determinism**: Same orders → same trades across all nodes
2. **Price-time priority**: Earlier orders at same price fill first
3. **TIF semantics**: GTC rests, IOC partial-fills and cancels remainder, FOK all-or-nothing
4. **Position accounting**: VWAP entry price, realized PnL on close, reversal handling
5. **Margin**: Initial margin blocks over-leveraged orders; maintenance margin triggers liquidation
6. **Liquidation cascade**: Liquidating one account doesn't crash the engine
7. **Insurance fund**: Depleted fund triggers ADL correctly
8. **Cross-domain**: Bridge message from EVM triggers PrimeOrders action atomically
9. **Rollback**: Failed block reverts all domain state changes

### 9.4 Benchmarks (from Reth)

```rust
// Criterion benchmarks for critical paths
fn bench_order_matching(c: &mut Criterion) {
    c.bench_function("match_1000_orders_10_levels", |b| {
        let book = setup_order_book(1000, 10);
        b.iter(|| book.match_taker_order(arb_taker()));
    });
}

fn bench_state_root(c: &mut Criterion) {
    c.bench_function("state_root_100k_accounts", |b| {
        let state = setup_state(100_000);
        b.iter(|| state.compute_root());
    });
}

fn bench_evm_execution(c: &mut Criterion) {
    c.bench_function("execute_100_transfers", |b| {
        let engine = setup_engine();
        let txs = generate_transfers(100);
        b.iter(|| engine.execute_transactions(&txs));
    });
}
```

---

## 10. Metrics and Observability (from Reth)

### 10.1 Metrics Stack

```toml
[dependencies]
metrics = "0.24"
metrics-exporter-prometheus = "0.18"
metrics-process = "2.1"
tracing = "0.1"
tracing-subscriber = "0.3"
opentelemetry = "0.31"        # Future: distributed tracing
```

### 10.2 Per-Component Metrics

```rust
// Consensus
consensus_height: Gauge
consensus_round: Gauge
consensus_finality_latency_ms: Histogram
consensus_prevotes_received: Counter
consensus_precommits_received: Counter
consensus_slashing_events: Counter

// Execution
block_execution_duration_ms: Histogram
evm_gas_used_per_block: Histogram
evm_transactions_per_block: Histogram

// PrimeOrders
orders_submitted: Counter
orders_matched: Counter
orders_cancelled: Counter
trades_executed: Counter
matching_latency_us: Histogram
order_book_depth: Gauge (per market, per side)
open_interest: Gauge (per market)
liquidations: Counter

// Mempool
mempool_size: Gauge
mempool_pending: Gauge
mempool_queued: Gauge

// Network
peers_connected: Gauge
messages_sent: Counter
messages_received: Counter
bandwidth_bytes: Counter

// Bridge
bridge_o2e_queue_depth: Gauge
bridge_e2o_queue_depth: Gauge
bridge_messages_processed: Counter

// Storage
state_commit_duration_ms: Histogram
db_read_latency_us: Histogram
db_write_latency_us: Histogram
```

---

## 11. Configuration and Build System

### 11.1 Workspace Layout

```toml
# Cargo.toml (root)
[workspace]
resolver = "2"
members = [
    "bin/prime-chain",
    "crates/consensus/*",
    "crates/execution/*",
    "crates/bridge/*",
    "crates/storage/*",
    "crates/network/*",
    "crates/pool/*",
    "crates/rpc/*",
    "crates/primitives",
    "crates/primitives-traits",
    "crates/config",
    "crates/governance",
    "crates/identity",
    "crates/metrics",
    "crates/errors",
]

[workspace.dependencies]
# Pin all versions centrally
revm = "36.0"
sled = "0.34"          # → migrate to libmdbx
k256 = "0.13"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
metrics = "0.24"
thiserror = "2"
# ... etc
```

### 11.2 Build Profiles (from Reth)

```toml
[profile.release]
opt-level = 3
lto = "thin"
strip = "symbols"

[profile.profiling]
inherits = "release"
debug = true
strip = false

[profile.maxperf]
inherits = "release"
lto = "fat"
codegen-units = 1

[profile.dev]
opt-level = 1  # Faster dev builds with some optimization

[profile.dev.package."*"]
opt-level = 2  # Optimize dependencies even in dev
```

### 11.3 CI/CD Pipeline

```yaml
# .github/workflows/ci.yml
jobs:
  check:     # cargo check --all-targets
  clippy:    # cargo clippy -- -D warnings
  fmt:       # cargo fmt --check
  test:      # cargo test --all
  bench:     # cargo bench (with codspeed)
  doc:       # cargo doc --no-deps
  build:     # cargo build --release
  docker:    # Build Docker image
  e2e:       # Multi-node testnet test
```

---

## 12. Development Phases

### Phase 1: Foundation (Weeks 1-4)
- [ ] Transaction signature verification (ECDSA secp256k1 with k256)
- [ ] Workspace restructuring (split into crates with traits/impl pattern)
- [ ] Migrate from `sled` to `libmdbx` (or RocksDB as interim)
- [ ] Add `proptest` and `criterion` for property tests and benchmarks
- [ ] Basic CI/CD (GitHub Actions: check, test, clippy, fmt)

### Phase 2: Consensus Hardening (Weeks 5-8)
- [ ] Implement proper locking mechanism (PoLC-based unlock)
- [ ] Weighted round-robin proposer selection with priority scaling
- [ ] Timeout progression (base + round * delta)
- [ ] Evidence pool for double-sign detection and gossiping
- [ ] Unbonding queue with slash-during-unbonding
- [ ] Tombstone (permanent validator ban after double-sign)

### Phase 3: PrimeOrders Production (Weeks 9-14)
- [ ] Match-time margin validation
- [ ] Insurance fund with configurable contribution rate
- [ ] Auto-deleveraging mechanism
- [ ] Semantic transaction ordering within blocks
- [ ] Memory pooling for order objects
- [ ] Parallel matching across independent markets (Rayon)
- [ ] Short-term vs. stateful order separation (dYdX model)
- [ ] Comprehensive matching engine property tests

### Phase 4: Networking (Weeks 15-18)
- [ ] Noise protocol encryption for all P2P connections
- [ ] Peer discovery (Kademlia DHT or simple PEX)
- [ ] Reactor pattern for message routing
- [ ] Block sync (download and verify missing blocks)
- [ ] State sync (snapshot transfer with chunked TCP)
- [ ] Transaction forwarding to next proposer

### Phase 5: State & Performance (Weeks 19-22)
- [ ] Merkle Patricia Trie with child tries per domain
- [ ] Incremental state root computation
- [ ] Light client state proofs
- [ ] Pipelined block processing
- [ ] Benchmark suite with CI tracking
- [ ] Profile and optimize hot paths

### Phase 6: RPC & Developer Experience (Weeks 23-26)
- [ ] Full Ethereum JSON-RPC compatibility
- [ ] WebSocket subscriptions (newHeads, logs, trades, orderBook)
- [ ] SDK: TypeScript/JavaScript client library
- [ ] SDK: Rust client library
- [ ] Documentation: API reference, operator guide
- [ ] Docker image with multi-stage build

### Phase 7: Testnet (Weeks 27-32)
- [ ] Multi-node testnet deployment (3-7 validators)
- [ ] Chaos testing (network partitions, node crashes, Byzantine validators)
- [ ] Load testing (sustained throughput measurement)
- [ ] Fuzz testing campaign (mempool, matching engine, consensus)
- [ ] Security audit preparation
- [ ] Public testnet launch

### Phase 8: Mainnet Preparation (Weeks 33-40)
- [ ] Formal verification of matching engine (optional but valuable)
- [ ] Third-party security audit
- [ ] Genesis configuration and validator onboarding
- [ ] Monitoring dashboards (Grafana + Prometheus)
- [ ] Alerting and incident response procedures
- [ ] Mainnet launch

---

## 13. Key Dependencies (Recommended Versions)

```toml
# Core
revm = "36.0"                          # EVM execution
alloy-primitives = "1.0"              # Ethereum types (Address, B256, U256)
alloy-rlp = "0.3"                     # RLP encoding

# Cryptography
k256 = "0.13"                          # secp256k1 ECDSA
sha3 = "0.10"                          # Keccak-256
rand = "0.8"                           # Random number generation

# Storage
libmdbx = "0.6"                        # Primary database (or sled as interim)

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
bincode = "1"                          # Binary serialization for snapshots

# Async
tokio = { version = "1", features = ["full"] }
rayon = "1.10"                         # Parallel matching

# Networking
noise-protocol = "0.2"                # Connection encryption
quinn = "0.11"                         # QUIC transport (future)

# RPC
jsonrpsee = "0.24"                     # JSON-RPC server

# Observability
tracing = "0.1"
tracing-subscriber = "0.3"
metrics = "0.24"
metrics-exporter-prometheus = "0.18"

# Testing
proptest = "1.7"
criterion = "0.5"
arbitrary = "1.3"
tempfile = "3"
```

---

## 14. Non-Negotiable Production Requirements

Before any mainnet launch, these must be complete:

1. **Transaction signature verification** — Currently absent. Without this, anyone can submit transactions as anyone else.

2. **Network encryption** — Currently unencrypted. Validators and transactions are visible to network observers.

3. **MEV protection** — At minimum, deterministic semantic ordering. Proposers must not have discretionary power over transaction order in a trading system.

4. **Insurance fund + ADL** — Without these, a single large liquidation can bankrupt the system.

5. **Match-time margin checks** — Placement-only checks allow undercollateralized fills.

6. **Evidence-based slashing** — Must detect and penalize double-signing with cryptographic proof, not just timeout heuristics.

7. **Proper state roots** — Flat-hash approach doesn't support light clients or state proofs. Need Merkle trie.

8. **Unbonding with slashing** — Validators must not be able to unstake immediately after misbehavior.

9. **Chain ID in all signatures** — Prevent cross-chain replay attacks.

10. **Determinism audit** — Verify no floating-point, no system time, no hash-table iteration order dependency, no randomness in consensus-critical paths.

---

## 15. Competitive Positioning Summary

| Feature | dYdX v4 | Hyperliquid | Sei v2 | **Prime Chain** |
|---------|---------|-------------|--------|-----------------|
| Native CLOB | Yes | Yes | Deprecated | **Yes** |
| EVM | No | Async only | Yes (parallel) | **Yes (atomic)** |
| Atomic EVM↔CLOB | No | No | No | **Yes** |
| Consensus | Cosmos BFT | Custom BFT | Cosmos BFT | Custom BFT |
| Language | Go | Rust | Go | **Rust** |
| Throughput (claimed) | ~500 ops/s | 200K ops/s | ~20K TPS | **Target: 50K+ ops/s** |
| Finality | ~1.5s | <200ms | ~400ms | **<1s** |

**Prime Chain's moat**: The only L1 where a smart contract can atomically place an order, check if it filled, and take action based on the result — all in a single transaction within a single block.

---

*This prompt should be read by any AI assistant or developer working on Prime Chain. It encodes the collective knowledge from studying Bitcoin, Ethereum (Reth), CometBFT, Cosmos SDK, Substrate, Polkadot, Solana, dYdX v4, Sei, and Hyperliquid — distilled into actionable engineering guidance for Prime Chain's specific architecture.*
