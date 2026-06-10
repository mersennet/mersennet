---
title: "Node Architecture"
---

This page describes the internal architecture of a Mersennet node — the Rust binary that produces blocks, executes transactions, participates in consensus, and serves the JSON-RPC API.

## Overview

A running Mersennet node is composed of five cooperating subsystems:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          MERSENNET NODE                               │
│                                                                         │
│  ┌───────────┐   ┌───────────┐   ┌──────────┐   ┌──────────────────┐  │
│  │  JSON-RPC  │   │ WebSocket │   │   P2P    │   │  Block Producer  │  │
│  │  Server    │   │  Server   │   │ Network  │   │  (Engine Loop)   │  │
│  │ :8545      │   │ :9945     │   │ :30303   │   │                  │  │
│  └─────┬──────┘   └─────┬─────┘   └────┬─────┘   └───────┬──────────┘  │
│        │                │               │                 │             │
│        └────────────────┴───────┬───────┴─────────────────┘             │
│                                 │                                       │
│                    ┌────────────┴────────────┐                          │
│                    │        ENGINE           │                          │
│                    │                         │                          │
│                    │  ┌─────────────────┐    │                          │
│                    │  │   EVM (revm)    │    │                          │
│                    │  │   + Precompiles │    │                          │
│                    │  └────────┬────────┘    │                          │
│                    │           │             │                          │
│                    │  ┌────────┴────────┐    │                          │
│                    │  │  State Backend  │    │                          │
│                    │  │ sled / redb     │    │                          │
│                    │  └─────────────────┘    │                          │
│                    │                         │                          │
│                    │  ┌─────────────────┐    │                          │
│                    │  │   Consensus     │    │                          │
│                    │  │   (PoS + BFT)   │    │                          │
│                    │  └─────────────────┘    │                          │
│                    │                         │                          │
│                    │  ┌─────────────────┐    │                          │
│                    │  │   Mempool       │    │                          │
│                    │  └─────────────────┘    │                          │
│                    │                         │                          │
│                    │  ┌─────────────────┐    │                          │
│                    │  │  PrimeOrders    │    │                          │
│                    │  │  Order Book     │    │                          │
│                    │  └─────────────────┘    │                          │
│                    └────────────────────────┘                           │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

| Component | Role |
|-----------|------|
| **Engine** | Central coordinator: drives block production, EVM execution, consensus, and state management |
| **P2P Network** | TCP/UDP transport for block propagation, transaction gossip, and consensus votes |
| **JSON-RPC Server** | HTTP endpoint implementing Ethereum-compatible JSON-RPC methods |
| **WebSocket Server** | Persistent connection endpoint for subscriptions (`eth_subscribe`) |
| **Block Producer** | Timer-driven loop that triggers block creation at the configured `block_time_ms` |

## Module Breakdown

The codebase is organized into six Rust crates: `core`, `network`, `rpc`, `node`, `zkp`, and `state-proof`. The four below make up the node runtime; `crates/zkp/` (`mersennet-zkp`) provides the zero-knowledge primitives (Poseidon hash, Pedersen commitments, threshold ElGamal, Noir circuit harness) and `crates/state-proof/` (`mersennet-state-proof`) provides the SP1 state-transition proof envelopes.

### `crates/core/` — Core Domain Logic

The heart of the node. Contains all types, execution logic, and consensus:

| Module | Purpose |
|--------|---------|
| `engine` | `Engine` struct coordinating block production, EVM execution, and state transitions. Defines `Block`, `Transaction`, `Receipt`, `LogEntry` types |
| `consensus` | PoS validator management, proposer rotation, BFT finalization, slashing, rewards |
| `hotstuff2` | Alternative HotStuff-2 consensus pipeline (optional) |
| `crypto` | secp256k1 signing/verification, `tx_signing_hash`, `sign_transaction`, `recover_signer` |
| `state` | `PersistentState` with sled backend, Merkle tree computation, snapshots |
| `state_redb` | Alternative `RedbState` backend using the redb embedded database |
| `flat_state` | `FlatState` — flat key-value representation for fast reads |
| `state_trait` | `StateBackend` trait abstracting storage (sled, redb, in-memory) |
| `mempool` | Transaction pool with nonce ordering, gas price priority, per-sender limits, replacement logic |
| `precompiles` | PrimeOrders EVM precompile registration at address `0x0100` |
| `precompile_abi` | ABI encoding/decoding for precompile function selectors |
| `prime_orders` | Order book state: markets, orders, positions, matching engine |
| `config` | `AppConfig` and all sub-config structs (`EngineConfig`, `P2pConfig`, etc.) |
| `events` | Domain event types for PrimeOrders and Bridge operations |
| `errors` | Error types for RPC validation and PrimeOrders |
| `network` | Network simulation and consensus message types (`Prevote`, `Precommit`, etc.) |
| `bridge` | Cross-domain bridge queue (EVM ↔ PrimeOrders) |
| `prometheus` | Prometheus metrics registry and `/metrics` endpoint |
| `identity` | Node identity (keypair) generation and persistence |
| `governance` | On-chain governance proposals and voting |
| `pipeline` | Block execution pipeline orchestration |
| `parallel` | Parallel transaction execution engine |
| `fba` | Frequent Batch Auction (FBA) matching engine |
| `mainnet` | Mainnet safety guards and invariant checks |
| `metrics` | Internal metrics collection |
| `zk_proofs` | Zero-knowledge proof generation |
| `zk_sp1` | SP1 zkVM integration |

### `crates/node/` — Node Binary

The executable entry point that wires all components together:

| File | Purpose |
|------|---------|
| `bin/mersennet.rs` | Main binary: CLI parsing, config loading, identity management, engine initialization, block production loop, P2P networking, RPC server startup |
| `bin/genesis.rs` | Genesis file generator utility |
| `bin/faucet.rs` | Testnet faucet HTTP server |
| `bin/stresstest.rs` | Transaction stress test tool |
| `bin/loadtest.rs` | Network load testing tool |
| `bin/migrate_genesis.rs` | Genesis migration utility |

### `crates/rpc/` — JSON-RPC Server

HTTP and WebSocket RPC servers:

| File | Purpose |
|------|---------|
| `rpc.rs` | HTTP JSON-RPC server using `tiny_http`. Handles `eth_*` and `prime_*` methods. Includes CORS, metrics, and Prometheus `/metrics` endpoint |
| `ws.rs` | WebSocket server for `eth_subscribe` (new blocks, pending transactions, logs) |
| `rpc_router.rs` | Method dispatch router mapping RPC method names to handler functions |

### `crates/network/` — P2P Networking

Peer-to-peer communication layer:

| File | Purpose |
|------|---------|
| `p2p.rs` | `P2pNetwork` and `NetworkNode` — peer management, message routing, block/tx/vote handling |
| `net_transport.rs` | `TcpSync` (reliable block sync) and `UdpGossip` (low-latency tx/vote propagation) |
| `noise.rs` | Noise Protocol Framework encryption for P2P channels |

## Execution Pipeline

When the block timer fires, the following sequence executes:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     BLOCK PRODUCTION PIPELINE                           │
└─────────────────────────────────────────────────────────────────────────┘

  Timer fires (every block_time_ms)
      │
      ▼
  ┌─────────────────────────┐
  │  1. PROPOSER SELECTION  │  Consensus selects the validator with the
  │                         │  highest accumulated priority (stake-weighted
  │                         │  round-robin). If this node is not the
  │                         │  proposer, it waits for an incoming block.
  └────────────┬────────────┘
               │
               ▼
  ┌─────────────────────────┐
  │  2. TX GATHERING        │  Drain pending transactions from the mempool,
  │                         │  ordered by gas_price (highest first). Stop
  │                         │  when cumulative gas reaches gas_limit_per_block.
  └────────────┬────────────┘
               │
               ▼
  ┌─────────────────────────┐
  │  3. EVM EXECUTION       │  For each transaction:
  │                         │    a. Build revm::Env (caller, callee, value,
  │                         │       data, gas_limit, gas_price)
  │                         │    b. Execute via revm with PrimeOrders
  │                         │       precompile at 0x0100
  │                         │    c. Collect ExecutionResult → Receipt
  │                         │    d. Update account states in StateBackend
  └────────────┬────────────┘
               │
               ▼
  ┌─────────────────────────┐
  │  4. BRIDGE PROCESSING   │  Process cross-domain bridge messages:
  │                         │    - EVM → PrimeOrders deposits
  │                         │    - PrimeOrders → EVM withdrawals
  └────────────┬────────────┘
               │
               ▼
  ┌─────────────────────────┐
  │  5. STATE ROOT          │  Compute Merkle root over all account states
  │                         │  (sorted address → serialized account data).
  │                         │  This becomes the block's state_root.
  └────────────┬────────────┘
               │
               ▼
  ┌─────────────────────────┐
  │  6. REWARD DISTRIBUTION │  Calculate block reward based on token
  │                         │  economics (10 MRSN/block, halving every
  │                         │  35M blocks). Distribute proportionally
  │                         │  to validators by stake weight.
  └────────────┬────────────┘
               │
               ▼
  ┌─────────────────────────┐
  │  7. FINALIZATION        │  Run BFT consensus:
  │                         │    - Broadcast block to peers
  │                         │    - Collect prevotes (2/3+ stake)
  │                         │    - Collect precommits (2/3+ stake)
  │                         │    - Process slashing evidence
  │                         │    - Mark block as finalized
  └────────────┬────────────┘
               │
               ▼
  ┌─────────────────────────┐
  │  8. BROADCAST           │  Send finalized block to all connected
  │                         │  peers via TCP. Update Prometheus metrics.
  │                         │  Notify WebSocket subscribers.
  └─────────────────────────┘
```

## Storage

### State Backend Architecture

State persistence is abstracted behind the `StateBackend` trait, allowing pluggable storage engines:

```
         ┌─────────────────────┐
         │    StateBackend      │  (trait)
         │  ─────────────────   │
         │  get_account()       │
         │  set_account()       │
         │  get_storage()       │
         │  set_storage()       │
         │  get_code()          │
         │  state_root()        │
         │  snapshot() / load() │
         └──────────┬──────────┘
                    │
        ┌───────────┼───────────┐
        │           │           │
        ▼           ▼           ▼
  ┌──────────┐ ┌──────────┐ ┌──────────┐
  │ Sled     │ │ Redb     │ │ InMemory │
  │ (default)│ │          │ │ (testing)│
  └──────────┘ └──────────┘ └──────────┘
```

### Sled Backend (Default)

The default `sled` backend persists all state to disk at the configured `state_path`:

```
state/
├── node_key.json      # Node identity (secp256k1 keypair)
├── peers.json         # Known peer addresses
└── sled_db/           # sled embedded database
    ├── accounts       # Address → AccountInfo (nonce, balance, code_hash)
    ├── storage        # (Address, Slot) → U256 value
    ├── code           # CodeHash → Bytecode
    └── metadata       # Chain height, state root, etc.
```

### Snapshots

The state backend supports snapshots for:

- **Crash recovery** — Restore to last consistent state on unexpected shutdown
- **State sync** — Share state snapshots with new nodes joining the network
- **Archive queries** — Historical state lookups at specific block heights

## EVM Integration

Mersennet uses [revm](https://github.com/bluealloy/revm) (Rust EVM) for transaction execution with the Shanghai specification.

### Execution Flow

For each transaction, the engine:

1. Constructs a `revm::Env` with block context (number, timestamp, coinbase, base_fee, gas_limit) and transaction context (caller, callee, value, data, gas_limit, gas_price)
2. Builds an `Evm` instance with the in-memory database and registered precompiles
3. Executes the transaction, which may:
   - Transfer MRSN between accounts
   - Deploy a new contract (when `to` is `None`)
   - Call an existing contract
   - Interact with the PrimeOrders precompile at `0x0100`
4. Captures the `ExecutionResult` (success/revert/halt, gas used, output, logs)
5. Commits state changes to the `StateBackend`

### Precompiles

Beyond the standard Ethereum precompiles (ecRecover, SHA-256, RIPEMD-160, identity, modexp, ecAdd, ecMul, ecPairing, blake2f), Mersennet adds:

| Address | Name | Description |
|---------|------|-------------|
| `0x0100` | **PrimeOrders** | Native on-chain order book. Solidity contracts can place/cancel orders, query order books, and manage positions atomically within a transaction |
| `0x0200` | **Shielded Transfer** | Private note-to-note transfer (`shieldedTransfer(bytes)`); activates with the privacy hard fork |
| `0x0201` | **Shield / Unshield** | Transparent ⇄ shielded bridge (`shield`, `unshield`); activates with the privacy hard fork |
| `0x0202` | **Code Publication** | Register/revoke a contract code attestation |
| `0x0300` | **State-Proof Verifier** | Verify an SP1 state-transition proof on-chain (`verifyStateProof(bytes)`) |

The PrimeOrders precompile is registered via a custom `EvmHandler` that injects it into the precompile table before each block's execution. A global context (`PRIME_ORDERS_CTX`) provides the precompile access to the order book state.

### Gas Metering

Gas follows standard EVM rules:

- Base transaction cost: 21,000 gas
- Contract creation: 32,000 gas + code deposit cost
- Storage operations: 20,000 gas (SSTORE cold), 5,000 gas (SSTORE warm)
- PrimeOrders precompile calls: fixed gas costs per operation type
- Block gas limit: 30,000,000 (configurable)

### EIP-1559 Fee Market

Mersennet implements EIP-1559 dynamic base fee:

```
if gas_used > target_gas (gas_limit / elasticity):
    base_fee increases (up to 12.5% per block)
if gas_used < target_gas:
    base_fee decreases (up to 12.5% per block)
```

Configuration:
- `fee_elasticity_multiplier`: 2 (target gas = gas_limit / 2 = 15M)
- `fee_max_change_denominator`: 8 (max 12.5% change per block)

## Custom Transaction Format

Mersennet uses a custom binary format for transaction signing, inspired by EIP-155:

### Signing Hash Input

The signing hash is `keccak256` of the following concatenated big-endian fields:

```
┌──────────────────────────────────────────────────────────────┐
│  Field        │ Size    │ Encoding        │ Description      │
├───────────────┼─────────┼─────────────────┼──────────────────┤
│  chain_id     │ 8 bytes │ u64 big-endian  │ Network ID (131071)│
│  nonce        │ 8 bytes │ u64 big-endian  │ Sender nonce     │
│  gas_price    │ 32 bytes│ U256 big-endian │ Price per gas    │
│  gas_limit    │ 8 bytes │ u64 big-endian  │ Max gas          │
│  to           │ 20 bytes│ Address or 0x00 │ Recipient (0x00  │
│               │         │                 │ for deploy)      │
│  value        │ 32 bytes│ U256 big-endian │ MRSN to transfer │
│  data         │ N bytes │ Raw bytes       │ Calldata         │
└──────────────────────────────────────────────────────────────┘

signing_hash = keccak256(chain_id || nonce || gas_price || gas_limit || to || value || data)
```

### Signed Transaction Wire Format

After signing, the complete transaction includes the original fields plus the ECDSA signature:

```
┌──────────────────────────────────────────────────────────────┐
│  Signing payload (as above)                                   │
│  chain_id(8) || nonce(8) || gas_price(32) || gas_limit(8)    │
│  || to(20) || value(32) || data(N)                           │
├──────────────────────────────────────────────────────────────┤
│  Signature                                                    │
│  r(32) || s(32) || v(1)                                      │
│                                                               │
│  v = recovery_id + 35 + chain_id × 2   (EIP-155)            │
└──────────────────────────────────────────────────────────────┘
```

### Signature Verification

To recover the signer:

1. Reconstruct the signing hash from the transaction fields
2. Compute `recovery_id = v - 35 - chain_id × 2`
3. Recover the secp256k1 public key from `(r, s, recovery_id, hash)`
4. Derive the Ethereum address: `keccak256(public_key)[12..32]`

## Startup Sequence

When the `mersennet` binary starts:

```
1. Initialize tracing (structured logging from RUST_LOG env)
2. Initialize Prometheus metrics exporter
3. Parse CLI arguments and load config.json
4. Load or create node identity (secp256k1 keypair)
5. Initialize Engine with configured storage backend
6. Configure mempool limits, fee market, slashing parameters
7. Apply genesis state (fund accounts, register validators)
8. Set token economics (max supply, rewards, halving)
9. Start JSON-RPC server (if enabled)
10. Start WebSocket server (if enabled)
11. Start P2P networking (TCP sync + UDP gossip)
12. Enter block production loop:
    └─ Every block_time_ms:
       ├─ Process incoming P2P messages (blocks, txs, votes)
       ├─ If proposer: produce_block()
       ├─ Run consensus finalization
       ├─ Broadcast results to peers
       └─ Update metrics
13. On SIGINT/SIGTERM: graceful shutdown
```
