# Prime Chain: Phase 2 Development Prompt — From Prototype to Production Mainnet

> **Purpose:** This document is a self-contained engineering brief for taking Prime Chain from a working prototype (v6.0) to a production-ready mainnet. It encodes knowledge from deep research into Reth, Monad, Sei v2, Aptos (Block-STM), Hyperliquid (HyperEVM), Grevm, HotStuff-2, Narwhal/Bullshark, SP1 zkVM, libmdbx, BEAST-MEV, and every major L1 blockchain — cross-referenced against Prime Chain's v6.0 whitepaper and 11,833-line Rust codebase.

---

## 0. Where We Are (v6.0 Baseline)

### What Exists and Works

| Component | Lines | Status | Benchmark |
|-----------|-------|--------|-----------|
| EVM Execution (revm Shanghai) | 1,505 | Production engine | **72,181 TPS** (transfers) |
| CLOB Matching Engine | 800 | Full lifecycle | **2,484,170 match ops/s** |
| HotStuff-2 Consensus | 760 | 2-phase BFT, QC, 2-chain commit | **0.001ms/round** |
| CLOB Precompile (0x0100) | 399 | 8 Solidity functions, ABI | **50K gas/placeOrder** |
| Parallel Executor (Block-STM) | 589 | Union-Find, MVCC, fallback | Needs complex-tx tuning |
| FBA Batch Auctions | 391 | Clearing price, pro-rata | **5,053,782 ops/s** |
| Commit-Reveal MEV Protection | 129 | Hash-commit, 2-block window | Working |
| Multi-Pool Mempool | 512 | Pending/queued/base_fee | EIP-1559 compatible |
| Transaction Signing (ECDSA) | 151 | secp256k1 sign/recover | Ethereum-compatible |
| CometBFT Consensus (legacy) | 850 | Prevote/precommit/commit | ~500ms finality |
| State Persistence (sled) | 958 | Merkle tree, proofs, pruning | Dev-grade |
| P2P Networking | 1,090 | UDP gossip, TCP sync, peers | Functional |
| RPC Server | 1,763 | 30+ methods, eth_* aliases | MetaMask-ready |
| Configuration | 359 | JSON, all subsystems | Hot-reload |
| Prometheus Metrics | 136 | 27 metrics | Full observability |
| Docker Testnet | — | 3-validator compose | Deployable |
| Test Suite | 1,300 | 62 tests, all passing | Good coverage |
| Documentation | 5,898 | Whitepaper v6 + Tech Ref + Architecture | Investor-grade |

### What We Don't Have (and Must Build)

| Gap | Why Critical | Priority |
|-----|-------------|----------|
| **libmdbx Storage** | sled is dev-only; Reth/Erigon use libmdbx for ACID, crash-safety, 10-100x I/O | **P0** |
| **WebSocket Subscriptions** | Every DeFi app, explorer, trading bot needs real-time events | **P0** |
| **Block Pipeline** | Overlapping execution+consensus doubles throughput (Monad pattern) | **P1** |
| **Production P2P (Noise)** | Current UDP gossip is unencrypted; validators won't run it | **P1** |
| **ZK State Proofs (SP1)** | Trustless bridges, light clients, L2 rollups | **P1** |
| **Workspace Restructure** | Single crate → Cargo workspace with trait separation (Reth pattern) | **P2** |
| **Account Abstraction** | ERC-4337 for smart wallets using CLOB precompile | **P2** |
| **Cross-Chain Bridges** | IBC (Cosmos), trustless bridge to Ethereum via ZK | **P2** |
| **Block Explorer** | Cannot demo without a working explorer | **P2** |
| **SDK (TypeScript)** | Developers need client libraries | **P2** |

---

## 1. Our Competitive Edge (Why We Win)

### The Atomic Composability Moat

**Prime Chain is the only blockchain with TRUE atomic EVM ↔ CLOB composability.**

Every competitor has a fundamental architectural gap:

| Chain | EVM | CLOB | EVM ↔ CLOB | The Gap |
|-------|-----|------|-----------|---------|
| **Hyperliquid** | HyperEVM (Cancun) | HyperCore (200K ops/s) | **Async** — EVM reads previous block, writes execute in next block, CoreWriter delayed by seconds | NOT atomic. Cannot get fill result in same tx. |
| **dYdX v4** | Limited (Cosmos app) | Native (in-memory CLOB) | Bridge/IBC | Separate chain. Not composable with EVM at all. |
| **Sei v2** | Parallel EVM | Deprecated native DEX | None | Abandoned CLOB entirely. |
| **Solana** | No EVM | Serum/Phoenix (program) | Program calls | Not EVM. Requires Move/Rust, not Solidity. |
| **Monad** | Parallel EVM | None | N/A | No order book at all. |
| **Ethereum** | Sequential EVM | Contract-based only | N/A | 150K+ gas per Uniswap swap, AMM not CLOB. |
| **Prime Chain** | **Parallel EVM (72K TPS)** | **Native (2.4M ops/s)** | **Precompile 0x0100 — same tx, instant fill result** | **None. True atomic.** |

### Hyperliquid Deep Comparison (Our #1 Competitor)

Hyperliquid's HyperEVM architecture:
- **Dual execution**: HyperCore (CLOB) and HyperEVM (Cancun EVM) run **sequentially in separate environments**
- **Read**: EVM reads HyperCore state from the **previous block** (stale by 1 block)
- **Write**: CoreWriter at `0x333...333` queues actions for the **next HyperCore block**
- **Delay**: CoreWriter orders are **intentionally delayed by several seconds** to prevent latency arbitrage
- **Dual blocks**: Big blocks (~1 min, 30M gas) and small blocks (~1 sec, 2M gas)
- **Account init bug**: Account must exist on HyperCore before EVM block; can't init + trade in same block
- **Status**: Still alpha as of early 2026; write precompiles evolving

**What Hyperliquid CANNOT do (that Prime Chain CAN):**
1. Place an order and get the fill result in the same Solidity function call
2. Read the current (not previous) order book state from a smart contract
3. Execute a vault rebalance → order → fill → callback in one transaction
4. Build a smart contract market maker with real-time quote management
5. Atomically check liquidation status and hedge in one call

**This is structural.** Hyperliquid would need to rebuild their dual-execution architecture to match our precompile approach. That's a multi-year effort for a chain with $10B+ in production.

---

## 2. Phase 2 Implementation Plan

### Phase 2.1: libmdbx Storage Engine (CRITICAL)

**Why:** sled is a hobby database. Every serious blockchain (Reth, Erigon, Akula) uses libmdbx. It provides:
- MVCC (Multi-Version Concurrency Control) — consistent reads without locks
- Full ACID transactions — crash-safe, no corruption on power loss
- Single-writer architecture — optimized for blockchain's write-then-read pattern
- 10-100x I/O improvement over sled on real workloads

**Implementation:**

```rust
// New crate: crates/storage/src/mdbx.rs

use libmdbx::{Environment, Database, WriteFlags, Geometry};

pub struct MdbxStateDB {
    env: Environment,
    accounts: Database,      // Address → AccountRecord
    storage: Database,       // (Address, Slot) → U256
    prime_orders: Database,  // "state" → PrimeOrdersSnapshot
    blocks: Database,        // height → Block
    bridge: Database,        // queue → BridgeQueueRecord
    metadata: Database,      // keys like "height", "state_root"
}

impl MdbxStateDB {
    pub fn open(path: &Path) -> Result<Self> {
        let env = Environment::builder()
            .set_geometry(Geometry {
                size: Some(0..1_099_511_627_776), // Up to 1TB
                growth_step: Some(1_073_741_824),  // 1GB growth
                page_size: Some(4096),
                ..Default::default()
            })
            .set_max_dbs(8)
            .open(path)?;
        // Open named databases...
    }
}
```

**Key design decisions:**
- Use the `reth-libmdbx` crate (Apache-licensed fork of libmdbx-rs)
- Implement the same `StateProvider` trait interface as sled, allowing hot-swap
- Use DUPSORT tables for storage (Reth pattern) — one key per address, sorted sub-keys for slots
- Flat state: Store current state directly, compute Merkle only on demand
- Separate historical state into an archive DB (optional, for block explorers)

**Dependencies:** `reth-libmdbx` or `libmdbx` crate

---

### Phase 2.2: WebSocket Subscriptions (CRITICAL)

**Why:** Every DeFi application, block explorer, and trading bot needs real-time event streaming. Without WebSocket support, no ecosystem can form.

**Implementation:**

```rust
// New module: src/rpc/ws.rs

pub enum SubscriptionKind {
    NewHeads,                    // New block headers
    NewPendingTransactions,     // Mempool transactions
    Logs { filter: LogFilter }, // EVM event logs (eth_subscribe style)
    PrimeOrdersTrades { market: Option<MarketId> }, // Trade stream
    PrimeOrdersOrderBook { market: MarketId, depth: usize }, // L2 book updates
    PrimeOrdersPositions { owner: Address },  // Position changes
    BatchAuctionResults { market: Option<MarketId> }, // FBA results
}

pub struct WsSubscriptionManager {
    subscribers: HashMap<SubscriptionId, (Sender<WsMessage>, SubscriptionKind)>,
    next_id: AtomicU64,
}

impl WsSubscriptionManager {
    pub fn subscribe(&mut self, kind: SubscriptionKind) -> (SubscriptionId, Receiver<WsMessage>);
    pub fn unsubscribe(&mut self, id: SubscriptionId);
    pub fn broadcast_block(&self, block: &Block);
    pub fn broadcast_trade(&self, trade: &Trade);
    pub fn broadcast_book_update(&self, market: MarketId, update: &BookDelta);
}
```

**Methods to implement:**
- `eth_subscribe` / `eth_unsubscribe` (Ethereum standard)
- `prime_subscribe` / `prime_unsubscribe` (Prime Chain extensions)
- Topics: `newHeads`, `logs`, `pendingTransactions`, `primeorders_trades`, `primeorders_book`, `primeorders_positions`, `fba_results`

**Dependencies:** `tungstenite` (already in Cargo.toml) or `tokio-tungstenite`

---

### Phase 2.3: Block Pipeline (HIGH IMPACT)

**Why:** Currently we execute a block, THEN run consensus. Monad's key insight: decouple them so execution of block N+1 overlaps with consensus of block N. This effectively doubles throughput.

**Architecture:**

```
Time ──────────────────────────────────────────────────────►

Block N:  [Execute]──────[Consensus]
Block N+1:              [Execute]──────[Consensus]
Block N+2:                            [Execute]──────[Consensus]

vs. current:

Block N:  [Execute]──[Consensus]
Block N+1:                      [Execute]──[Consensus]
Block N+2:                                            [Execute]──[Consensus]
```

**Implementation:**

```rust
pub struct BlockPipeline {
    execution_thread: JoinHandle<()>,
    consensus_thread: JoinHandle<()>,
    pending_blocks: Channel<ExecutedBlock>,   // Execution → Consensus
    committed_blocks: Channel<FinalizedBlock>, // Consensus → State
}

impl BlockPipeline {
    pub fn start(engine: Arc<Mutex<Engine>>, consensus: Arc<Mutex<HotStuff2>>) {
        // Thread 1: Execution
        // - Drain mempool, execute transactions, produce ExecutedBlock
        // - Send to pending_blocks channel
        
        // Thread 2: Consensus  
        // - Receive ExecutedBlock from channel
        // - Run HotStuff-2 round (propose → vote → QC → commit)
        // - On commit, persist to state DB
        // - Signal execution thread to build next block
    }
}
```

**Expected impact:** ~2x effective throughput (144K+ TPS for independent transfers)

---

### Phase 2.4: Production P2P with Noise Encryption

**Why:** Current UDP gossip has no encryption. Validators communicating over public internet MUST have encrypted, authenticated channels.

**Implementation:**
- Use the **Noise Protocol Framework** (same as libp2p, WireGuard)
- `snow` crate for Rust Noise implementation
- Noise_XX handshake pattern (mutual authentication)
- Encrypt all gossip messages (blocks, transactions, votes, peer discovery)
- Authenticated peer connections using validator keys

```rust
pub struct SecureTransport {
    noise_builder: snow::Builder,
    sessions: HashMap<SocketAddr, snow::TransportState>,
}

impl SecureTransport {
    pub fn handshake(&mut self, peer: SocketAddr) -> Result<()>;
    pub fn encrypt_and_send(&self, peer: SocketAddr, data: &[u8]) -> Result<()>;
    pub fn receive_and_decrypt(&self, peer: SocketAddr) -> Result<Vec<u8>>;
}
```

**Dependencies:** `snow` crate

---

### Phase 2.5: ZK State Proofs with SP1

**Why:** ZK proofs enable trustless bridges, light clients, and L2 rollups. SP1 allows writing proof logic in Rust, reusing our existing crates.

**Architecture:**

```
Prime Chain State Transition
         │
         ▼
   SP1 Program (Rust)
   - Takes: prev_state_root, block, transactions
   - Executes: revm + PrimeOrders matching
   - Outputs: new_state_root
   - Proves: correct execution
         │
         ▼
   STARK Proof (succinct)
         │
    ┌────┴────┐
    │         │
    ▼         ▼
 Ethereum   Light
 Bridge     Clients
```

**Implementation:**

```rust
// sp1-program/src/main.rs (runs inside SP1 zkVM)
#![no_main]
sp1_zkvm::entrypoint!(main);

fn main() {
    let prev_root: B256 = sp1_zkvm::io::read();
    let block: Block = sp1_zkvm::io::read();
    
    // Re-execute all transactions
    let new_root = execute_block_and_compute_root(&prev_root, &block);
    
    // Commit the result
    sp1_zkvm::io::commit(&new_root);
}
```

**Dependencies:** `sp1-sdk`, `sp1-zkvm`

---

### Phase 2.6: Workspace Restructure (Reth Pattern)

**Why:** Single-crate structure doesn't scale. Need trait-based separation for:
- Pluggable implementations (swap sled for libmdbx)
- Clean testing with mocks
- Future chain variants (testnet, mainnet, devnet configs)

**Target structure:**
```
crates/
├── primitives/           # Block, Transaction, Receipt, Address
├── consensus-traits/     # trait Consensus, trait Validator
├── consensus/            # HotStuff-2 + CometBFT implementations
├── execution-traits/     # trait EvmExecutor, trait MatchingEngine
├── execution/            # revm executor + PrimeOrders
├── precompiles/          # CLOB precompile at 0x0100
├── storage-traits/       # trait StateProvider, trait BlockReader
├── storage-sled/         # sled implementation
├── storage-mdbx/         # libmdbx implementation
├── pool/                 # Multi-pool mempool
├── network/              # P2P (gossip + noise encryption)
├── rpc/                  # JSON-RPC + WebSocket
├── fba/                  # Frequent Batch Auctions
├── mev-protection/       # Commit-Reveal pool
├── parallel/             # Block-STM parallel executor
├── zk-prover/            # SP1 state proof generation
└── node/                 # Binary, wiring everything together
```

---

### Phase 2.7: TypeScript SDK

**Why:** Developers need client libraries to build on Prime Chain.

**Features:**
- `PrimeProvider`: JSON-RPC + WebSocket connection
- `PrimeWallet`: Transaction signing (ethers.js compatible)
- `PrimeOrders`: Order submission, cancellation, position queries
- `PrimePrecompile`: Encode/decode CLOB precompile calls
- `PrimeFBA`: Batch order submission
- Full TypeScript types for all RPC methods

---

### Phase 2.8: Block Explorer

**Why:** Cannot demo or onboard developers without a visual explorer.

**Features:**
- Block list with real-time updates (WebSocket)
- Transaction details with decoded PrimeOrders operations
- Order book visualization per market
- Position and account views
- FBA auction history
- Validator dashboard (stake, uptime, slashing history)

**Stack:** React + Vite + TailwindCSS + ethers.js + WebSocket

---

## 3. Implementation Order

```
Week 1-2:   libmdbx storage engine (replace sled)
Week 2-3:   WebSocket subscriptions  
Week 3-4:   Block pipeline (overlapping execution + consensus)
Week 4-5:   Noise protocol encryption for P2P
Week 5-6:   Workspace restructure (Cargo workspace)
Week 6-8:   ZK state proofs (SP1 integration)
Week 8-9:   TypeScript SDK
Week 9-10:  Block explorer
Week 10-12: Cross-chain bridges (IBC + Ethereum via ZK)
Week 12:    Public testnet launch
```

---

## 4. Performance Targets (Post Phase 2)

| Metric | Current (v6.0) | Target (v7.0) | How |
|--------|---------------|---------------|-----|
| EVM TPS (transfers) | 72,181 | **140,000+** | Block pipeline + libmdbx |
| EVM TPS (DeFi mix) | ~25,000 | **50,000+** | Parallel executor tuning |
| CLOB ops/s | 2,484,170 | **3,000,000+** | libmdbx state writes |
| FBA ops/s | 5,053,782 | **5,000,000+** | Already there |
| Finality | ~200ms | **<150ms** | Optimized HotStuff-2 |
| State I/O | sled (dev) | **libmdbx (production)** | 10-100x improvement |
| Network | Unencrypted UDP | **Noise-encrypted** | Production-ready |
| Subscriptions | HTTP polling only | **WebSocket real-time** | Full ecosystem support |
| Proofs | None | **ZK state proofs** | Trustless bridges |
| SDK | None | **TypeScript + Rust** | Developer ecosystem |

---

## 5. The Moat We're Building

After Phase 2, Prime Chain will be:

1. **The fastest EVM chain** with native CLOB (~140K+ TPS)
2. **The only chain** with atomic EVM ↔ CLOB in one transaction
3. **The only chain** with MEV-immune batch auctions for order matching
4. **ZK-provable** — trustless bridges to Ethereum and other chains
5. **Production-grade** — encrypted P2P, ACID storage, real-time subscriptions
6. **Developer-friendly** — TypeScript SDK, block explorer, full Ethereum compatibility

No other chain — not Hyperliquid, not Monad, not Solana, not Ethereum — will have all of these simultaneously.

---

## 6. Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| libmdbx integration complexity | Use reth-libmdbx (battle-tested); implement behind trait interface for hot-swap |
| SP1 proof generation speed | Proofs are async; don't block block production. Start with hourly checkpoints. |
| Parallel executor overhead | Focus on complex DeFi workloads; sequential fallback is already 72K TPS |
| Workspace restructure breaks things | Incremental migration; one crate at a time; CI catches regressions |
| Hyperliquid ships atomic composability | Their dual-execution architecture makes this structurally very hard; multi-year effort |

---

**This prompt is designed to be fed directly to an AI coding assistant to execute Phase 2 of Prime Chain development. Every section contains enough context, code examples, and rationale to be implemented independently.**
