# Prime Chain: Comprehensive Technical Reference

**Version 1.0**  
**Last Updated: January 2026**

This document provides exhaustive technical documentation for the Prime Chain codebase, covering architecture, data structures, algorithms, APIs, configuration, and implementation details.

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Project Structure](#2-project-structure)
3. [Dependencies & Build](#3-dependencies--build)
4. [Core Architecture](#4-core-architecture)
5. [Execution Engine](#5-execution-engine)
6. [Consensus Mechanism](#6-consensus-mechanism)
7. [PrimeOrders Matching Engine](#7-primeorders-matching-engine)
8. [Bridge System](#8-bridge-system)
9. [State Management](#9-state-management)
10. [Mempool](#10-mempool)
11. [RPC Layer](#11-rpc-layer)
12. [Network Layer](#12-network-layer)
13. [Identity & Key Management](#13-identity--key-management)
14. [Governance](#14-governance)
15. [Events System](#15-events-system)
16. [Configuration](#16-configuration)
17. [Metrics & Observability](#17-metrics--observability)
18. [CLI & Entry Point](#18-cli--entry-point)
19. [Testing](#19-testing)
20. [Appendix: Data Formats](#20-appendix-data-formats)
21. [Algorithm Specifications](#21-algorithm-specifications)
22. [RPC Method Reference (Complete)](#22-rpc-method-reference-complete)
23. [Database Schema (sled)](#23-database-schema-sled)
24. [Snapshot Protocol (PSNP)](#24-snapshot-protocol-psnp)
25. [Security Considerations](#25-security-considerations)
26. [Performance Characteristics](#26-performance-characteristics)
27. [Environment Variables](#27-environment-variables)
28. [File I/O Summary](#28-file-io-summary)

---

## 1. System Overview

### 1.1 High-Level Architecture

Prime Chain is a Layer 1 blockchain that unifies four primary execution domains under a single consensus layer:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         CONSENSUS LAYER                                      │
│  Proof-of-Stake │ Finality Rounds (Prevote/Precommit) │ Slashing │ Rewards  │
└─────────────────────────────────────────────────────────────────────────────┘
                                        │
        ┌───────────────────────────────┼───────────────────────────────┐
        │                               │                               │
┌───────▼────────┐            ┌─────────▼─────────┐            ┌───────▼───────┐
│  PrimeEVM      │            │  PrimeOrders      │            │  Bridge       │
│  (revm)        │◄──────────►│  (Matching)       │◄──────────►│  (Queues)     │
│  EVM Exec      │            │  Order Books      │            │  Cross-Domain │
└───────┬────────┘            └─────────┬─────────┘            └───────┬───────┘
        │                               │                               │
        └───────────────────────────────┼───────────────────────────────┘
                                        │
                            ┌───────────▼───────────┐
                            │  PersistentState      │
                            │  (sled database)      │
                            │  accounts, storage,   │
                            │  prime_orders, bridge │
                            └───────────────────────┘
```

### 1.2 Design Principles

| Principle | Implementation |
|-----------|----------------|
| **Determinism** | All state transitions produce identical outputs for identical inputs |
| **Atomicity** | EVM, PrimeOrders, and Bridge operations commit atomically per block |
| **Composability** | Cross-domain bridge enables EVM ↔ PrimeOrders message passing |
| **Security** | Economic security via staking, slashing, and validator rotation |
| **Performance** | Sub-second finality, high throughput, optimized data structures |
| **Transparency** | All state transitions and events are publicly verifiable |

### 1.3 Technology Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 2024 Edition |
| EVM | revm v12 (Shanghai spec) |
| Database | sled v0.34 (embedded key-value) |
| Cryptography | k256 v0.13 (secp256k1 ECDSA) |
| Serialization | serde, bincode, serde_json |
| HTTP/RPC | tiny_http v0.12 |
| Logging | tracing, tracing-subscriber |
| Metrics | metrics v0.19, metrics-exporter-prometheus v0.10 |
| Networking | std::net (TcpListener, UdpSocket), tungstenite (WebSocket) |

---

## 2. Project Structure

### 2.1 Directory Layout

```
prime-chain/
├── Cargo.toml              # Package manifest, dependencies
├── Cargo.lock              # Locked dependency versions
├── src/
│   ├── lib.rs              # Library root, module exports
│   ├── main.rs             # Minimal entrypoint (redirects to bin)
│   ├── errors.rs           # Error types (PrimeOrdersError, RpcInputError)
│   ├── bin/
│   │   └── prime-chain.rs  # CLI binary, main application logic
│   ├── config/
│   │   └── config.rs       # Configuration structs, parsing
│   ├── core/
│   │   ├── engine.rs       # Main Engine, block execution
│   │   ├── consensus.rs    # PoS consensus, finality, slashing
│   │   ├── prime_orders.rs # Order matching, risk engine
│   │   ├── mempool.rs      # Transaction pool
│   │   ├── bridge.rs       # Cross-domain message queues
│   │   ├── state.rs        # Persistent state, snapshots
│   │   └── events.rs       # Domain events
│   ├── governance/
│   │   └── governance.rs   # On-chain governance
│   ├── identity/
│   │   └── identity.rs     # Node identity, key management
│   ├── network/
│   │   ├── network.rs      # NetworkSim (consensus message simulation)
│   │   ├── net_transport.rs# UDP gossip, TCP sync, snapshots
│   │   └── p2p.rs          # P2P network simulation
│   ├── rpc/
│   │   ├── rpc.rs          # JSON-RPC server, request handling
│   │   └── rpc_router.rs   # Method routing, parameter parsing
│   ├── prometheus/
│   │   └── prometheus.rs   # Prometheus metrics initialization
│   └── metrics/
│       └── metric.rs       # Metrics module (re-exports prometheus)
├── tests/
│   ├── consensus_sim.rs    # Consensus simulation tests
│   ├── fuzz_mempool.rs     # Mempool fuzzing
│   ├── integration_block.rs# Block execution integration
│   ├── prime_orders_integration.rs
│   └── state_tests.rs      # State persistence tests
└── docs/                   # Documentation
```

### 2.2 Module Dependency Graph

```
lib.rs
  ├── consensus
  ├── bridge
  ├── config
  ├── engine ─────┬── consensus, bridge, events, errors, mempool
  │               ├── prime_orders, state, network
  │               └── revm
  ├── identity
  ├── errors
  ├── events
  ├── governance
  ├── mempool
  ├── net_transport
  ├── network
  ├── p2p ──────────────── engine, network
  ├── prometheus
  ├── rpc_router ───────── engine, bridge, events, prime_orders, errors
  ├── prime_orders
  ├── rpc ───────────────── rpc_router, prometheus, engine
  └── state
```

---

## 3. Dependencies & Build

### 3.1 Cargo.toml Dependencies

```toml
[dependencies]
anyhow = "1.0"           # Error handling
hex = "0.4"              # Hex encoding/decoding
k256 = "0.13"            # ECDSA (secp256k1)
bincode = "1.3"          # Binary serialization
serde = "1.0"            # Serialization
sled = "0.34"            # Embedded database
tungstenite = "0.21"     # WebSocket
url = "2.5"              # URL parsing
serde_json = "1.0"       # JSON
revm = "12"              # EVM implementation
tiny_http = "0.12"       # HTTP server
thiserror = "1.0"        # Error derives
rand = "0.8"             # Randomness
tracing = "0.1"          # Logging
tracing-subscriber = "0.3"
metrics = "0.19"         # Metrics
metrics-exporter-prometheus = "0.10"
once_cell = "1.20"       # Lazy statics
```

### 3.2 Build Commands

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run binary
cargo run --bin prime-chain

# Run with arguments
cargo run --bin prime-chain -- --config config.json --rpc

# Run tests
cargo test

# Run specific test
cargo test prime_orders_limit_matching
```

### 3.3 Cargo Features

The project uses default features. revm is configured with `default-features = false` and `features = ["std"]` for minimal footprint.

---

## 4. Core Architecture

### 4.1 Engine Structure

The `Engine` struct (`src/core/engine.rs`) is the central coordinator:

```rust
pub struct Engine {
    pub chain_id: u64,
    pub block_number: u64,
    pub base_fee: U256,
    pub coinbase: Address,
    pub gas_limit_per_block: u64,
    pub spec_id: SpecId,                    // Shanghai
    pub consensus: ConsensusEngine,
    pub fee_max_change_denominator: u64,
    pub fee_elasticity_multiplier: u64,
    pub fee_target_gas: u64,
    pub evm: EvmEngine,
    pub chain: Vec<Block>,
    pub orders: OrdersEngine,
    pub bridge: BridgeEngine,
    pub pending_events: Vec<DomainEvent>,
    mempool: Mempool,
}
```

### 4.2 Sub-Engine Types

**EvmEngine:**
```rust
pub struct EvmEngine {
    pub state: PersistentState,  // sled database handle
    pub db: InMemoryDB,          // revm in-memory database
}
```

**ConsensusEngine:**
```rust
pub struct ConsensusEngine {
    pub inner: Consensus,        // Validators, slashing, rewards
    pub network: NetworkSim,     // Consensus message simulation
}
```

**OrdersEngine:**
```rust
pub struct OrdersEngine {
    pub state: PrimeOrdersState, // Markets, orders, positions
}
```

**BridgeEngine:**
```rust
pub struct BridgeEngine {
    pub orders_to_evm: BridgeQueue,
    pub evm_to_orders: BridgeQueue,
}
```

### 4.3 Block Execution Flow

```
1. Transaction Selection
   - Get senders from mempool
   - Sort by fee (descending)
   - For each sender: select tx with expected nonce
   - Stop when block gas limit reached

2. Transaction Execution Loop
   - Execute each selected tx via revm
   - Accumulate gas_used, receipts
   - Update nonce cache

3. Block Hash Computation
   - Hash: number, chain_id, gas_limit, gas_used, base_fee, coinbase, tx_count

4. Consensus Finalization
   - Apply pending validator changes
   - Run finality rounds (prevote, precommit)
   - Detect slashing evidence
   - Finalize block
   - Process unbonding
   - Apply pending slashes

5. Reward Distribution
   - Calculate rewards per validator
   - Apply to EVM account balances

6. State Commit
   - Write EVM state to sled
   - Commit PrimeOrders state
   - Commit bridge queues
   - Compute state root

7. Block Assembly
   - Dequeue bridge messages
   - Take pending events
   - Create Block struct
   - Push to chain

8. Post-Execution
   - Update base fee (EIP-1559)
   - Increment block_number
   - Emit metrics
```

---

## 5. Execution Engine

### 5.1 Transaction Structure

```rust
pub struct Transaction {
    pub from: Address,
    pub to: Option<Address>,      // None = contract creation
    pub value: U256,
    pub data: Bytes,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub nonce: u64,
    pub chain_id: Option<u64>,
}
```

### 5.2 Transaction Validation

**Basic Validation (`validate_tx_basic`):**
- `gas_limit <= block_gas_limit`
- `chain_id` matches (if provided)

**State Validation (`validate_tx_state`):**
- `nonce >= account.nonce`
- `gas_price >= base_fee`
- `balance >= gas_cost + value` where `gas_cost = gas_limit * gas_price`

### 5.3 Transaction Execution

Uses revm's `Evm::builder()`:
- **Env**: block number, coinbase, gas limit, base fee
- **Tx**: caller, gas limit, gas price, nonce, value, data
- **TransactTo**: `TxKind::Call(to)` or `TxKind::Create`

Execution result mapped to `TxExecution`:
- Success: gas_used, output, created_address, logs
- Revert: gas_used, output
- Halt: gas_used

### 5.4 Block Hash Computation

```rust
fn compute_block_hash(
    number, chain_id, gas_limit, gas_used,
    base_fee, coinbase, tx_count
) -> B256
```

Payload: 8+8+8+8+32+20+8 bytes, Keccak-256 hashed.

### 5.5 Base Fee Update (EIP-1559)

```
target = gas_limit / elasticity_multiplier
if gas_used == target: base_fee unchanged
if gas_used > target:
  delta = gas_used - target
  fee_delta = base_fee * delta / target / max_change_denominator
  base_fee = base_fee + fee_delta
if gas_used < target:
  delta = target - gas_used
  fee_delta = base_fee * delta / target / max_change_denominator
  base_fee = base_fee - fee_delta
base_fee = max(base_fee, 1)
```

### 5.6 ABI Utilities

```rust
pub mod abi {
    pub fn selector(signature: &str) -> [u8; 4]
    pub fn decode_u64(output: &Bytes) -> Option<u64>
}
```

Selector: Keccak-256(signature)[0..4]

---

## 6. Consensus Mechanism

### 6.1 Data Structures

**Validator:**
```rust
pub struct Validator {
    pub address: Address,
    pub stake: U256,
}
```

**Vote:**
```rust
pub struct Vote {
    pub validator: Address,
    pub block_hash: B256,
    pub stake: U256,
}
```

**Finalization:**
```rust
pub struct Finalization {
    pub block_hash: B256,
    pub proposer: Address,
    pub total_stake: U256,
    pub committed_stake: U256,
    pub threshold: U256,
    pub votes: Vec<Vote>,
    pub finalized: bool,
    pub rewards: Vec<Reward>,
    pub total_reward: U256,
    pub burned_reward: U256,
    pub scheduled_reward: U256,
    pub remaining_supply: U256,
}
```

**SlashingEvidence:**
```rust
pub struct SlashingEvidence {
    pub validator: Address,
    pub kind: EvidenceKind,    // DoubleSign | PrecommitTimeout
    pub height: u64,
    pub round: u64,
    pub rounds_missed: u64,
}
```

### 6.2 Proposer Selection

```rust
proposer(height) = validators[height % validators.len()]
```

Round-robin rotation.

### 6.3 Finality Threshold

```
threshold = (total_stake * 2 / 3) + 1
```

Block finalized when prevote_stake >= threshold AND precommit_stake >= threshold.

### 6.4 Finality Rounds Algorithm

For each round (0..rounds):
1. Validators broadcast prevote (except miss_prevote set)
2. Drain prevotes from network, detect double-sign
3. Validators broadcast precommit (except miss_precommit set)
4. Drain precommits, detect double-sign
5. Validators in prevote but not precommit → timeout evidence
6. If prevote_stake >= threshold AND precommit_stake >= threshold → finalized, break

### 6.5 Slashing Calculation

```
base_bps = DoubleSign ? 500 : 100
escalated = base_bps + (offenses * step_bps) + (rounds_missed * step_bps)
bps = min(escalated, max_escalation_bps)
slash = stake * bps / 10000
```

### 6.6 Reward Distribution

```
reward_per_validator = total_reward * validator_stake / total_stake
```

Capped by remaining supply. Burned = scheduled - distributed.

### 6.7 Unbonding

- Unbond queues amount with unlock_height = current + unbonding_period
- Each block: release entries where height >= unlock_height

---

## 7. PrimeOrders Matching Engine

### 7.1 Data Structures

**Market:**
```rust
pub struct Market {
    pub id: MarketId,
    pub symbol: String,
    pub tick_size: U256,
    pub lot_size: U256,
    pub last_price: U256,
}
```

**Order:**
```rust
pub struct Order {
    pub id: OrderId,
    pub owner: Address,
    pub market: MarketId,
    pub side: Side,           // Buy | Sell
    pub price: U256,
    pub size: U256,
    pub tif: TimeInForce,     // Gtc | Ioc | Fok
}
```

**OrderBook:**
```rust
pub struct OrderBook {
    pub bids: BTreeMap<U256, VecDeque<OrderId>>,  // price -> order queue
    pub asks: BTreeMap<U256, VecDeque<OrderId>>,
}
```

**Position:**
```rust
pub struct Position {
    pub size: i128,           // Long positive, short negative
    pub entry_price: U256,
    pub realized_pnl: i128,
}
```

### 7.2 Matching Algorithm (submit_order)

1. Validate: market exists, size > 0
2. Initial margin check (ensure_initial_margin)
3. FOK: check available_liquidity, reject if not fillable
4. Get matching price levels:
   - Buy: matching_asks(market, limit_price)
   - Sell: matching_bids(market, limit_price)
5. Iterate levels, match against opposite book
6. For each fill: apply_fill (update positions, last_price)
7. Remove fully filled orders from book
8. If remaining > 0 and GTC: place_order (add to book)
9. Return OrderOutcome (order_id?, filled, remaining, trades)

### 7.3 Price-Time Priority

- Bids: sorted descending (highest first)
- Asks: sorted ascending (lowest first)
- Within price level: VecDeque (FIFO)

### 7.4 Margin & Liquidation

**Initial Margin:**
```
required = notional * initial_margin_bps / 10000
```

**Maintenance Margin:**
```
required = sum over positions of (|size| * mark_price * maintenance_bps / 10000)
```

**Liquidation:** equity < maintenance_margin
- Cancel all open orders
- Clear positions
- Collateral remains

### 7.5 Error Types

- `UnknownMarket`
- `InvalidSize` (zero)
- `FokNotFillable`
- `InsufficientCollateral`

---

## 8. Bridge System

### 8.1 BridgeDomain

```rust
pub enum BridgeDomain {
    PrimeOrders,
    PrimeEvm,
}
```

### 8.2 BridgeMessage

```rust
pub struct BridgeMessage {
    pub nonce: u64,
    pub from: BridgeDomain,
    pub to: BridgeDomain,
    pub payload: Bytes,
}
```

### 8.3 BridgeQueue

- FIFO VecDeque
- Optional max_len (evicts oldest when full)
- push(): increment nonce, append
- pop(): remove from front
- snapshot/restore for persistence

### 8.4 Queue Processing

Per block: all messages dequeued and included in block. Bridge queues committed to state.

---

## 9. State Management

### 9.1 PersistentState (sled)

**Trees:**
- `accounts`: address -> AccountRecord
- `storage`: (address, slot) -> value
- `prime_orders`: "state" -> serialized PrimeOrdersSnapshot
- `bridge_orders_to_evm`: "queue" -> serialized BridgeQueueRecord
- `bridge_evm_to_orders`: "queue" -> serialized BridgeQueueRecord

### 9.2 AccountRecord (serialization)

```rust
struct AccountRecord {
    balance: [u8; 32],
    nonce: u64,
    code_hash: [u8; 32],
    code: Vec<u8>,
}
```

### 9.3 State Root Computation

- Collect all (key, value) from accounts, storage, prime_orders, bridge trees
- Sort by key
- Concatenate, Keccak-256

### 9.4 Snapshot Format

**SnapshotRecord (bincode):**
- height, state_root
- accounts: Vec<(Address, AccountRecord)>
- storage: Vec<((Address, U256), U256)>
- prime_orders: Option<Vec<u8>>
- bridge_orders_to_evm: Option<Vec<u8>>
- bridge_evm_to_orders: Option<Vec<u8>>

### 9.5 Snapshot Transfer (TCP)

**Header (49 bytes):**
- Magic: "PSNP" (4 bytes)
- Version: 1 (1 byte)
- Chunk size: u32 big-endian
- Total length: u64 big-endian
- Hash: 32 bytes (Keccak-256 of snapshot)

**Body:** Chunked snapshot bytes

---

## 10. Mempool

### 10.1 Structure

```rust
by_sender: HashMap<Address, BTreeMap<u64, Transaction>>
max_total: usize
max_per_sender: usize
min_replace_bump_bps: u64
```

### 10.2 Insert Logic

1. If full: evict_for_fee (lowest fee tx, require bump)
2. If sender queue full: evict_sender_for_fee
3. Reject duplicate nonce
4. Insert

### 10.3 Eviction

- Replace requires: new_fee >= old_fee * (1 + bump_bps/10000)
- Default bump: 1000 bps (10%)

### 10.4 TxRejection Codes

- DuplicateNonce, MempoolFull, SenderQueueFull, FeeTooLow
- GasLimitTooHigh, InvalidChainId, NonceTooLow
- GasPriceTooLow, InsufficientBalance, DatabaseError

---

## 11. RPC Layer

### 11.1 HTTP Server

- tiny_http Server
- Default: 127.0.0.1:8545
- POST: JSON-RPC
- GET /health: status, height, chain_id
- GET /metrics: Prometheus format

### 11.2 JSON-RPC Methods

**Chain:**
- `prime_chainId` -> hex chain ID
- `prime_blockNumber` -> hex latest height
- `prime_getBalance` (address, block?) -> hex balance

**Blocks:**
- `prime_getBlockByNumber` (number, include_txs) -> block DTO

**Transactions:**
- `prime_sendTransaction` (tx) -> tx hash
- `prime_getTransactionByHash` (hash) -> tx DTO
- `prime_getTransactionReceipt` (hash) -> receipt DTO

**Logs:**
- `prime_getLogs` (filter) -> log array

**Domain Events:**
- `prime_getDomainEvents` (filter) -> events array

**PrimeOrders:**
- `primeorders_addMarket` (symbol, tick_size, lot_size)
- `primeorders_submitOrder` (order object)
- `primeorders_cancelOrder` (order_id)
- `primeorders_getOrderBook` (market_id)
- `primeorders_getOpenOrders` (owner)
- `primeorders_setMarginParams` (initial_bps, maintenance_bps)
- `primeorders_depositCollateral` (owner, amount)
- `primeorders_isLiquidatable` (owner)
- `primeorders_liquidate` (owner)

**Bridge:**
- `primebridge_enqueueOrdersToEvm` (payload)
- `primebridge_enqueueEvmToOrders` (payload)
- `primebridge_dequeueOrdersToEvm`
- `primebridge_dequeueEvmToOrders`

### 11.3 Parameter Parsing

- Address: 0x-prefixed hex, 20 bytes
- Hash: 0x-prefixed hex, 32 bytes
- U256: 0x hex or decimal
- Block tag: "latest" or hex number

### 11.4 RPC Error Codes

- -32700: Parse error
- -32601: Method not found
- -32602: Invalid params
- -32000: Internal error
- -32005: Tx rejected
- -32010..-32013: PrimeOrders errors

---

## 12. Network Layer

### 12.1 NetworkSim (Consensus)

- In-memory message buffer
- broadcast(Message)
- drain_round(block_hash, height, round, stage) -> matching messages

### 12.2 UdpGossip

**GossipPacket:**
- topic, data, id, ttl

**GossipConfig:**
- fanout, max_peers, max_seen
- peer_ttl, seen_ttl
- retry_base, retry_max
- peer_discovery_topic

**Features:**
- Peer discovery via topic
- TTL-based forwarding
- Deduplication (seen set)
- Retry with exponential backoff
- Peer store persistence (JSON)

### 12.3 TcpSync

- send_packet/recv_packet: length-prefixed JSON
- send_snapshot/recv_snapshot: PSNP protocol

### 12.4 P2pNetwork (Simulation)

- HashMap of Node
- broadcast(from, message): deliver to other nodes
- sync_blocks(from, peer): copy blocks from peer to from

### 12.5 Node (P2p)

- inbox: Vec<P2pMessage>
- process_inbox: submit tx, import block, broadcast vote

---

## 13. Identity & Key Management

### 13.1 NodeIdentity

```rust
pub struct NodeIdentity {
    pub signing_key: SigningKey,  // k256 ECDSA
    pub address: Address,
}
```

### 13.2 Address Derivation

```
pubkey = verifying_key.to_encoded_point(false).as_bytes()
hash = keccak256(pubkey[1..])
address = hash[12..32]
```

### 13.3 Storage

- Path: configurable (default state/node_key.json)
- Format: JSON { "private_key": "0x..." }
- Create if not exists: SigningKey::random(OsRng)

---

## 14. Governance

### 14.1 ProposalKind

- SetFeeMarket { gas_limit_per_block, elasticity_multiplier, max_change_denominator }
- SetTokenEconomics { max_supply, initial_reward_per_block, halving_interval }

### 14.2 Proposal Lifecycle

1. submit(title, kind, current_height) -> proposal_id
2. vote(proposal_id, height, voter, stake, support)
3. execute(proposal_id, height, finalized, total_stake, apply_fn)

### 14.3 Execution Conditions

- Block finalized
- height > proposal.end_height
- tally.passed (quorum + pass threshold)
- Not already executed

---

## 15. Events System

### 15.1 DomainEvent

- PrimeOrders(PrimeOrdersEvent)
- Bridge(BridgeEvent)

### 15.2 PrimeOrdersEvent Variants

- MarketAdded
- OrderSubmitted
- OrderCancelled
- Trade
- MarginParamsUpdated
- CollateralDeposited
- Liquidation

### 15.3 BridgeEvent Variants

- Enqueued { queue, message }
- Dequeued { queue, message }

### 15.4 Indexing

- Events stored per block
- Query: domain_events_in_range(from, to, domain?, kind?)
- RPC: prime_getDomainEvents with filter

---

## 16. Configuration

### 16.1 AppConfig Structure

```json
{
  "engine": { "chain_id", "state_path", "gas_limit_per_block", ... },
  "mempool": { "max_total", "max_per_sender", "bump_bps" },
  "prime_orders": { "initial_margin_bps", "maintenance_margin_bps" },
  "bridge": { "max_queue_len" },
  "genesis": { "accounts": [...] },
  "slashing": { "double_sign_bps", "timeout_bps", ... },
  "token_economics": { "max_supply", "initial_reward_per_block", "halving_interval" },
  "rpc": { "enabled", "addr" },
  "p2p": { "node_key_path", "peer_store_path" }
}
```

### 16.2 Default Values

- chain_id: 999
- state_path: "state"
- gas_limit_per_block: 30_000_000
- max_total: 10_000
- max_per_sender: 1_000
- double_sign_bps: 500
- timeout_bps: 100
- rpc addr: 127.0.0.1:8545

### 16.3 Hot Reload

Config file watcher (2s interval) applies runtime changes without restart.

---

## 17. Metrics & Observability

### 17.1 Prometheus Metrics

| Name | Type | Labels | Description |
|------|------|--------|-------------|
| prime_chain_up | gauge | - | Node up (1.0) |
| block_height_gauge | gauge | - | Current block number |
| blocks_executed_total | counter | - | Blocks executed |
| blocks_finalized_total | counter | - | Blocks finalized |
| tx_submitted_total | counter | - | Transactions submitted |
| mempool_size_gauge | gauge | - | Mempool size |
| block_exec_duration_seconds | histogram | - | Block execution time |
| consensus_rounds_total | counter | - | Consensus rounds |
| consensus_slashing_evidence_total | counter | kind | Slashing evidence |
| rpc_requests_total | counter | method | RPC requests |
| rpc_request_errors_total | counter | code, method | RPC errors |
| rpc_request_duration_seconds | histogram | method | RPC latency |
| metrics_requests_total | counter | - | /metrics hits |
| network_messages_broadcast_total | counter | - | Messages broadcast |
| network_messages_pending_gauge | gauge | - | Pending messages |
| network_messages_drained_total | counter | stage | Messages drained |
| snapshot_chunks_sent_total | counter | - | Snapshot chunks sent |
| snapshot_bytes_sent_total | counter | - | Snapshot bytes sent |
| snapshot_chunks_received_total | counter | - | Snapshot chunks received |
| snapshot_bytes_received_total | counter | - | Snapshot bytes received |

### 17.2 Prometheus Initialization

- OnceCell for handle
- install_recorder() at startup
- Exposed via /metrics endpoint

---

## 18. CLI & Entry Point

### 18.1 CLI Arguments

| Argument | Description |
|----------|-------------|
| --config | Config file path |
| --state | Override state path |
| --mempool-max | Override mempool max |
| --mempool-per-sender | Override per-sender limit |
| --mempool-bump-bps | Override fee bump |
| --rpc | Enable RPC |
| --rpc-addr | RPC bind address |
| --mode | full | validator | devnet |
| --devnet | Alias for mode=devnet |
| --snapshot-listen | Listen for snapshot transfer |
| --snapshot-fetch | Fetch snapshot from address |
| --snapshot-out | Save snapshot to file |
| --snapshot-chunk-size | Chunk size bytes |
| --snapshot-max-bytes | Max receive size |
| --node-key-path | Identity key path |
| --peer-store-path | Peer store path |

### 18.2 Node Modes

- **Devnet**: Demo with validators, txs, governance
- **Full**: Full node (minimal logging)
- **Validator**: Validator node (minimal logging)

### 18.3 Snapshot CLI Flow

**Listen:** Bind TCP, accept connection, export snapshot, send chunked.

**Fetch:** Connect TCP, receive chunked, verify hash, import.

---

## 19. Testing

### 19.1 Test Categories

- **consensus_sim**: Consensus round simulation
- **fuzz_mempool**: Mempool insert/evict fuzzing
- **integration_block**: Full block execution
- **prime_orders_integration**: Order matching, GTC/IOC/FOK
- **state_tests**: State persistence, snapshot

### 19.2 Key Test Patterns

- TempDir for state isolation
- Engine::new_with_state(chain_id, path)
- Assert on receipts, balances, order book state

---

## 20. Appendix: Data Formats

### 20.1 Hex Encoding Conventions

- Addresses: 0x + 40 hex chars
- Hashes: 0x + 64 hex chars
- U256: 0x + variable hex (no leading zeros in output)
- u64: 0x + hex

### 20.2 JSON-RPC Request

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "prime_getBalance",
  "params": ["0x...", "latest"]
}
```

### 20.3 JSON-RPC Response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": "0x..."
}
```

### 20.4 Peer Store Format

```json
{
  "peers": ["127.0.0.1:42001", "127.0.0.1:42002"]
}
```

### 20.5 Node Key Format

```json
{
  "private_key": "0x..."
}
```

---

## 21. Algorithm Specifications

### 21.1 Block Hash Computation (Pseudocode)

```
FUNCTION compute_block_hash(number, chain_id, gas_limit, gas_used, base_fee, coinbase, tx_count):
    payload = concat(
        number.to_be_bytes(8),
        chain_id.to_be_bytes(8),
        gas_limit.to_be_bytes(8),
        gas_used.to_be_bytes(8),
        base_fee.to_be_bytes(32),
        coinbase.bytes(20),
        tx_count.to_be_bytes(8)
    )
    RETURN keccak256(payload)
```

### 21.2 Transaction Hash (RPC)

```
FUNCTION tx_hash(tx):
    payload = concat(
        tx.from,
        tx.to.is_some() ? 1 : 0,
        tx.to.unwrap_or_default(),
        tx.value.to_be_bytes(32),
        tx.gas_price.to_be_bytes(32),
        tx.gas_limit.to_be_bytes(8),
        tx.nonce.to_be_bytes(8),
        tx.data,
        tx.chain_id.unwrap_or(0).to_be_bytes(8)
    )
    RETURN keccak256(payload)
```

### 21.3 Mempool Transaction Selection

```
FUNCTION select_transactions_for_block(mempool, nonce_cache, gas_limit):
    senders = mempool.senders()
    SORT senders BY ready_fee(sender, expected_nonce) DESC
    gas_used = 0
    transactions = []
    FOR sender IN senders:
        expected_nonce = nonce_cache.get(sender) OR get_account_nonce(sender)
        tx = mempool.take_ready(sender, expected_nonce)
        IF tx AND gas_used + tx.gas_limit <= gas_limit:
            transactions.append(tx)
            gas_used += tx.gas_limit
            nonce_cache[sender] = expected_nonce + 1
    RETURN transactions
```

### 21.4 Order Matching (PrimeOrders) Pseudocode

```
FUNCTION submit_order(owner, market, side, price, size, tif):
    IF NOT market.exists: RETURN Err(UnknownMarket)
    IF size == 0: RETURN Err(InvalidSize)
    IF NOT ensure_initial_margin(owner, price, size): RETURN Err(InsufficientCollateral)
    IF tif == FOK AND available_liquidity(market, side, price) < size: RETURN Err(FokNotFillable)

    remaining = size
    trades = []
    levels = side == Buy ? matching_asks(market, price) : matching_bids(market, price)

    FOR level_price IN levels:
        IF remaining == 0: BREAK
        queue = book.opposite_side(level_price)
        WHILE queue not empty AND remaining > 0:
            maker_order = queue.front
            fill = min(remaining, maker_order.size)
            remaining -= fill
            maker_order.size -= fill
            apply_fill(market, buyer, seller, fill, level_price)
            trades.append(Trade{taker: owner, maker: maker_order.owner, ...})
            IF maker_order.size == 0: queue.pop_front
    IF remaining > 0 AND tif == GTC: place_order(owner, market, side, price, remaining, tif)
    RETURN OrderOutcome{filled: size - remaining, remaining, trades}
```

### 21.5 State Root Computation

```
FUNCTION compute_state_root():
    items = []
    FOR (key, value) IN accounts: items.append((key, value))
    FOR (key, value) IN storage: items.append((key, value))
    FOR (key, value) IN prime_orders: items.append((key, value))
    FOR (key, value) IN bridge_orders_to_evm: items.append((key, value))
    FOR (key, value) IN bridge_evm_to_orders: items.append((key, value))
    SORT items BY key
    buffer = concat all (key || value)
    RETURN keccak256(buffer)
```

---

## 22. RPC Method Reference (Complete)

### 22.1 prime_chainId

**Params:** `[]`  
**Returns:** `"0x3e7"` (hex chain ID)  
**Example:** `{"method":"prime_chainId","params":[],"id":1}`

### 22.2 prime_blockNumber

**Params:** `[]`  
**Returns:** `"0x5"` (hex block number)  
**Example:** `{"method":"prime_blockNumber","params":[],"id":1}`

### 22.3 prime_getBalance

**Params:** `[address: string, block?: string|number]`  
**Returns:** `"0x..."` (hex balance)  
**Example:** `{"method":"prime_getBalance","params":["0x..."],"id":1}`

### 22.4 prime_getBlockByNumber

**Params:** `[blockNumber: string|number, includeTransactions?: boolean]`  
**Returns:** Block object or null  
**Block fields:** number, hash, gas_limit, gas_used, base_fee, state_root, transactions, domain_events

### 22.5 prime_sendTransaction

**Params:** `[tx: object]`  
**Tx fields:** from, to?, value?, data?, gas?, gas_price?, nonce?, chain_id?  
**Returns:** `"0x..."` (tx hash)  
**Errors:** -32005 with reason on rejection

### 22.6 prime_getTransactionByHash

**Params:** `[hash: string]`  
**Returns:** Transaction object or null

### 22.7 prime_getTransactionReceipt

**Params:** `[hash: string]`  
**Returns:** Receipt object or null  
**Receipt fields:** transaction_hash, block_hash, block_number, gas_used, status, contract_address?, output, logs

### 22.8 prime_getLogs

**Params:** `[filter: object]`  
**Filter fields:** fromBlock?, toBlock?, address?, topics?  
**Returns:** Array of Log objects

### 22.9 prime_getDomainEvents

**Params:** `[filter?: object]`  
**Filter fields:** fromBlock?, toBlock?, domain?, kind?  
**Returns:** Array of {block_number, event_index, domain, kind, data}

### 22.10 primeorders_addMarket

**Params:** `[symbol: string, tickSize: string, lotSize: string]`  
**Returns:** `"0x1"` (hex market ID)

### 22.11 primeorders_submitOrder

**Params:** `[{owner, market_id, side, price, size, tif?}]`  
**Returns:** `{order_id?, filled, remaining, trades}`  
**side:** "buy" | "sell"  
**tif:** "gtc" | "ioc" | "fok" (default: gtc)

### 22.12 primeorders_cancelOrder

**Params:** `[orderId: string|number]`  
**Returns:** `true` if cancelled, `false` if not found

### 22.13 primeorders_getOrderBook

**Params:** `[marketId: string|number]`  
**Returns:** `{bids: [{price, size}], asks: [{price, size}]}` or null

### 22.14 primeorders_getOpenOrders

**Params:** `[owner: string]`  
**Returns:** Array of order objects

### 22.15 primebridge_enqueueOrdersToEvm

**Params:** `[payload: string]` (0x-prefixed hex)  
**Returns:** `{nonce, from, to, payload}`

### 22.16 primebridge_enqueueEvmToOrders

**Params:** `[payload: string]`  
**Returns:** Bridge message object

### 22.17 primebridge_dequeueOrdersToEvm

**Params:** `[]`  
**Returns:** Message object or null

### 22.18 primebridge_dequeueEvmToOrders

**Params:** `[]`  
**Returns:** Message object or null

---

## 23. Database Schema (sled)

### 23.1 accounts Tree

**Key:** 20-byte address  
**Value:** bincode(AccountRecord)  
**AccountRecord:** balance[32], nonce, code_hash[32], code[]

### 23.2 storage Tree

**Key:** 52 bytes (address[20] || slot[32])  
**Value:** 32-byte value (big-endian)

### 23.3 prime_orders Tree

**Key:** "state"  
**Value:** bincode(PrimeOrdersSnapshot)  
**PrimeOrdersSnapshot:** next_order_id, initial_margin_bps, maintenance_margin_bps, markets[], orders[], accounts[], books[]

### 23.4 bridge_orders_to_evm Tree

**Key:** "queue"  
**Value:** bincode(BridgeQueueRecord)

### 23.5 bridge_evm_to_orders Tree

**Key:** "queue"  
**Value:** bincode(BridgeQueueRecord)

---

## 24. Snapshot Protocol (PSNP)

### 24.1 Wire Format

| Offset | Size | Field |
|--------|------|-------|
| 0 | 4 | Magic "PSNP" |
| 4 | 1 | Version (1) |
| 5 | 4 | Chunk size (u32 BE) |
| 9 | 8 | Total length (u64 BE) |
| 17 | 32 | Keccak-256 hash |
| 49 | N | Chunked snapshot data |

### 24.2 Chunk Boundaries

Chunks are sequential. Last chunk may be shorter than chunk_size. Total bytes = total_len.

### 24.3 Verification

Receiver must compute Keccak-256 of reassembled data and compare with header hash. Mismatch = reject.

### 24.4 Size Limits

- Max packet (TcpSync): 4 MiB for generic packets
- Snapshot: configurable max_bytes (default 256 MiB)

---

## 25. Security Considerations

### 25.1 Transaction Validation

- Nonce ordering enforced
- Balance checked before execution
- Gas limits prevent runaway execution
- Chain ID prevents cross-chain replay (when provided)

### 25.2 Consensus Security

- 2/3 threshold for finality
- Double-sign detection and slashing
- Timeout slashing for missing precommits
- Unbonding period prevents quick exit attacks

### 25.3 State Integrity

- State root commits all domains
- Snapshot hash verification
- Atomic commits per block

### 25.4 Known Limitations

- No transaction signature verification (transactions accepted as-is)
- No peer authentication in UDP gossip
- No message encryption
- Centralized genesis/validator initialization

---

## 26. Performance Characteristics

### 26.1 Block Execution

- Complexity: O(n) in transactions
- Gas limit: 30M default
- Typical block: 1-1000 transactions

### 26.2 Mempool

- Lookup: O(1) per sender, O(log n) per nonce (BTreeMap)
- Eviction: O(n) scan for lowest fee
- Insert: O(log n) for BTreeMap

### 26.3 Order Book

- Price levels: BTreeMap O(log n) lookup
- Queue: VecDeque O(1) front/back
- Matching: O(levels * orders_per_level)

### 26.4 State

- sled: LSM-tree based, persistent
- In-memory: InMemoryDB for revm (full copy during execution)

---

## 27. Environment Variables

### 27.1 Logging

- `RUST_LOG`: tracing filter (e.g., `prime_chain=info`)

### 27.2 Tracing

- Uses tracing-subscriber with env-filter
- Configured in main() before any logging

---

## 28. File I/O Summary

| Path | Purpose |
|------|---------|
| state/ | sled database directory |
| state/node_key.json | Node identity (created if missing) |
| state/peers.json | Peer store (UDP gossip) |
| config.json | Application config (optional) |
| snapshot.bin | Exported snapshot (--snapshot-out) |

---

**End of Technical Reference**

*For strategic and fundraising documentation, see STRATEGIC_POSITIONING.md and related documents.*
