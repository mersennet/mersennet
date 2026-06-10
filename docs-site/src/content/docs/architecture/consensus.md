---
title: "Consensus Mechanism"
---

Mersennet uses **Proof-of-Stake (HotStuff-2 BFT)** with round-robin proposer selection and priority-based weighting. This document provides a deep dive into how consensus works, from validator selection to block finalization and slashing.

## Overview

| Parameter | Value |
|-----------|-------|
| **Consensus** | Proof-of-Stake (HotStuff-2 BFT) |
| **Block Time** | ~1 second |
| **Finality** | BFT (Byzantine Fault Tolerant) |
| **Implementation** | Rust |

## Validator Selection

Validators are nodes that have **staked** MRSN tokens and registered in the validator set. Voting power is proportional to stake:

```
voting_power(validator) ∝ staked_amount
```

Token holders can **delegate** their MRSN to validators, increasing that validator's voting power. The validator set is dynamic—new validators can join by staking, and existing validators can leave by unbonding.

## Proposer Rotation

Block production uses a **round-robin** algorithm with **priority-based weighting**:

1. Each validator has a **priority** value that accumulates over time.
2. The validator with the **highest priority** is selected as the proposer for the current block.
3. After selection, the proposer's priority is reduced by the total weight of all validators.
4. All validators' priorities are incremented by their normalized stake weight each round.

This ensures:
- **Fair rotation** — No single validator dominates block production
- **Stake-weighted frequency** — Validators with more stake are chosen more often
- **Determinism** — Given the same validator set and heights, proposer selection is reproducible

### Priority Algorithm (Conceptual)

```
priority[i] += normalized_weight[i]   (each round)
priority[proposer] -= total_weight     (after selection)
proposer = argmax(priority)
```

Weights are normalized from stake to prevent overflow with 18-decimal MRSN values.

## Block Production Cycle

The block production cycle proceeds as follows:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     BLOCK PRODUCTION CYCLE                              │
└─────────────────────────────────────────────────────────────────────────┘

  Height N
    │
    │  1. PROPOSER SELECTION
    │     └─ Select validator with highest priority
    │
    │  2. PROPOSAL PHASE
    │     └─ Proposer builds block (txs, orders, state)
    │     └─ Broadcasts block to all validators
    │
    │  3. PREVOTE PHASE
    │     └─ Validators receive block, validate
    │     └─ Broadcast prevote for block hash
    │     └─ Wait for 2/3+ prevotes
    │
    │  4. PRECOMMIT PHASE
    │     └─ Validators broadcast precommit
    │     └─ Wait for 2/3+ precommits
    │
    │  5. FINALIZATION
    │     └─ Block committed to chain
    │     └─ State root updated
    │     └─ Rewards distributed
    │
    ▼
  Height N+1
```

### Diagram: Block Production Flow

```
     Validator A          Validator B          Validator C          Validator D
     (Proposer)           (Voter)              (Voter)               (Voter)
          │                     │                    │                     │
          │  Create Block       │                    │                     │
          │─────────────────────┤                    │                     │
          │                     │                    │                     │
          │  Broadcast Block    │                    │                     │
          │─────────────────────┼────────────────────┼─────────────────────┤
          │                     │                    │                     │
          │                     │  Prevote           │  Prevote             │  Prevote
          │                     │◄───────────────────┼─────────────────────┤
          │                     │                    │                     │
          │                     │  Precommit (2/3+)  │  Precommit           │  Precommit
          │                     │◄───────────────────┼─────────────────────┤
          │                     │                    │                     │
          │  FINALIZE           │                    │                     │
          │─────────────────────┼────────────────────┼─────────────────────┤
          │                     │                    │                     │
          ▼                     ▼                    ▼                     ▼
```

## Block Finalization

A block is **finalized** when:

1. **Prevote threshold**: More than 2/3 of total stake has broadcast a prevote for the block hash
2. **Precommit threshold**: More than 2/3 of total stake has broadcast a precommit

```
T = 2/3 × total_stake + 1
finalized ⟺ prevotes ≥ T AND precommits ≥ T
```

Finalized blocks are **irreversible**—there are no chain reorganizations. This provides fast, deterministic finality for applications.

## Epoch Transitions

Mersennet may use **epochs** for validator set updates (e.g. applying pending stake changes, unbonding completions). At epoch boundaries:

- Pending validator additions/removals are applied
- Unbonding queues are processed
- Validator set is updated for the next epoch

The exact epoch length is configurable. Validator set changes take effect at the start of the next epoch to ensure consensus continuity.

## Slashing Mechanism

### Slashing Types

| Type | Trigger | Base Penalty | Consequence |
|------|---------|--------------|-------------|
| **Double-sign** | Signing two different blocks at same height | 5% of stake | **Tombstoned** (permanent ban) |
| **Timeout** | Failing to precommit after prevoting | 1% of stake | **Jailed** (temporary exclusion) |

### Escalation

Penalties **escalate** with repeated offenses:

```
actual_penalty = base_bps + (escalation_step × offense_count) + (escalation_step × rounds_missed)
actual_penalty = min(actual_penalty, max_escalation_bps)
```

With default parameters:
- `escalation_step` = 25 bps (0.25%)
- `max_escalation` = 1000 bps (10%)

First timeout: 1%. Second: 1.25%. Third: 1.5%. And so on, up to 10%.

### Slashing Source Priority

When slashing is executed, tokens are taken from:
1. **Unbonding queue first** — tokens being withdrawn
2. **Active stake second** — if unbonding doesn't cover the penalty
3. **Remainder burned** — if the validator doesn't have enough to cover

### Tombstone vs. Jail

- **Tombstoned**: Permanently banned. Cannot rejoin.
- **Jailed**: Temporarily excluded. Can `unjail()` after the jail period.

## Block Structure

Every finalized block contains the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `number` | `u64` | Sequential block height starting from 0 (genesis) |
| `hash` | `B256` | Keccak-256 hash uniquely identifying this block |
| `chain_id` | `u64` | Network identifier (131071 for testnet) |
| `state_root` | `B256` | Merkle root of the post-execution state trie |
| `transactions` | `Vec<Transaction>` | Ordered list of transactions included in the block |
| `receipts` | `Vec<Receipt>` | Execution receipts corresponding 1:1 with transactions |
| `proposer` | `Address` | Validator address that proposed this block |
| `coinbase` | `Address` | Address receiving block rewards (same as proposer) |
| `gas_limit` | `u64` | Maximum gas allowed in this block (default 30,000,000) |
| `gas_used` | `u64` | Total gas consumed by all transactions |
| `base_fee` | `U256` | EIP-1559 base fee for this block (adjusts per block) |
| `finalized` | `bool` | Whether 2/3+ stake committed to this block |
| `consensus` | `Finalization` | BFT finalization data (prevotes, precommits, round info) |
| `rewards` | `Vec<Reward>` | Per-validator block reward distributions |
| `total_reward` | `U256` | Sum of all rewards paid this block |
| `burned_reward` | `U256` | Portion of rewards burned (e.g. slashed stake) |
| `slashes` | `Vec<Slashing>` | Slashing events applied in this block |
| `unbonded` | `Vec<Unbonding>` | Completed unbonding operations |
| `domain_events` | `Vec<DomainEvent>` | PrimeOrders and bridge events emitted during execution |

### Receipt Format

Each transaction produces a `Receipt`:

| Field | Type | Description |
|-------|------|-------------|
| `success` | `bool` | Whether the transaction executed without revert |
| `gas_used` | `u64` | Gas consumed by this transaction |
| `output` | `Bytes` | Return data (ABI-encoded for contract calls) |
| `created_address` | `Option<Address>` | Contract address if this was a deployment |
| `error` | `Option<String>` | Human-readable error message on failure |
| `logs` | `Vec<LogEntry>` | Emitted EVM logs (events) |

## Transaction Lifecycle

A transaction moves through the following stages from submission to finalization:

```
┌─────────────────────────────────────────────────────────────────────┐
│                     TRANSACTION LIFECYCLE                            │
└─────────────────────────────────────────────────────────────────────┘

  1. SUBMISSION
     │  Client sends signed transaction via JSON-RPC
     │  (eth_sendRawTransaction or prime_sendTransaction)
     │
  2. VALIDATION
     │  ├─ Verify ECDSA signature (recover signer from r, s, v)
     │  ├─ Check chain_id matches (131071 for testnet)
     │  ├─ Verify nonce == account.nonce (no gaps, no replays)
     │  ├─ Verify sender balance ≥ value + gas_limit × gas_price
     │  └─ Check gas_price ≥ base_fee (EIP-1559)
     │
  3. MEMPOOL
     │  ├─ Insert into pending pool (max 10,000 total txs)
     │  ├─ Per-sender limit: 1,000 pending txs
     │  ├─ Replacement: new tx must bump gas_price by ≥10% (1000 bps)
     │  └─ Gossip transaction hash to connected peers
     │
  4. BLOCK INCLUSION
     │  ├─ Proposer gathers txs from mempool ordered by gas_price
     │  ├─ Txs included up to block gas_limit (30M gas)
     │  └─ Proposer builds candidate block
     │
  5. EVM EXECUTION
     │  ├─ Execute each tx sequentially in revm
     │  ├─ Apply state transitions (balance changes, storage writes)
     │  ├─ Process PrimeOrders precompile calls (if any)
     │  ├─ Generate receipt with logs, gas_used, status
     │  └─ Compute post-execution state_root
     │
  6. CONSENSUS & FINALIZATION
     │  ├─ Proposer broadcasts block to validators
     │  ├─ Validators verify block, broadcast prevote
     │  ├─ On 2/3+ prevotes, broadcast precommit
     │  └─ On 2/3+ precommits, block is finalized (irreversible)
     │
  7. RECEIPT
     └─ Client queries receipt via eth_getTransactionReceipt
        Receipt includes: status, gas_used, logs, created_address
```

### Nonce Management

Nonces enforce strict transaction ordering per account:

- Each account has a monotonically increasing nonce starting at 0
- Transaction with nonce `N` can only execute when the account's current nonce is `N`
- If nonce `N+1` arrives before `N`, it stays in the mempool queue until `N` executes
- Sending a new transaction with the same nonce replaces the pending one (if gas price is bumped by ≥10%)

## State Management

Mersennet maintains EVM-compatible world state using a Merkle trie structure.

### State Trie

The world state is a mapping from addresses to account objects:

```
State Root (B256)
    │
    ├── Account 0x1234...
    │   ├── nonce: u64
    │   ├── balance: U256
    │   ├── code_hash: B256 (keccak256 of contract bytecode)
    │   └── storage_root: B256 (root of account's storage trie)
    │
    ├── Account 0x5678...
    │   └── ...
    │
    └── ...
```

Each block produces a new `state_root` — the Merkle root computed over all account state after executing every transaction. This provides:

- **Integrity verification** — Any node can verify state correctness by recomputing the root
- **Light client proofs** — Merkle proofs can prove account balances without full state
- **Determinism** — Same transactions on same pre-state always produce the same state root

### Storage Model

Account storage follows the EVM model: each contract has a 256-bit key → 256-bit value mapping. Storage slots are accessed via `SLOAD` and `SSTORE` opcodes.

The current storage backend options are:

| Backend | Description | Use Case |
|---------|-------------|----------|
| `sled` | Embedded key-value store (default) | Production nodes |
| `redb` | Rust-native embedded database | Alternative production backend |
| `memory` | In-memory (volatile) | Testing and development |

### State Root Computation

After each block, the state root is computed using a sorted Merkle tree:

1. Collect all `(address, account_data)` pairs
2. Sort by address (deterministic ordering)
3. Hash each pair: `keccak256(address || rlp(account))`
4. Build a binary Merkle tree from the leaf hashes
5. The root hash becomes the block's `state_root`

## Network Protocol

Mersennet nodes communicate using a custom peer-to-peer protocol built on TCP and UDP.

### Message Types

Nodes exchange three categories of messages:

| Message | Transport | Purpose |
|---------|-----------|---------|
| `Tx` | TCP + UDP gossip | Propagate new transactions to peers |
| `Block` | TCP sync | Broadcast finalized blocks |
| `Vote` | TCP + UDP gossip | Exchange prevote/precommit messages |

### Peer Discovery

Nodes discover peers through:

1. **Seed nodes** — Configured in `p2p.peers` (bootstrap addresses)
2. **Peer exchange** — Connected nodes share their known peer lists
3. **Persistent peer store** — Known peers are saved to `peers.json` for reconnection on restart

### Block Propagation

```
  Proposer Node                    Validator Node A              Validator Node B
       │                                │                              │
       │  1. Produce block              │                              │
       │  2. TCP broadcast ─────────────┤                              │
       │                                │──── TCP forward ─────────────┤
       │                                │                              │
       │  3. Collect prevotes ◄─────────┤                              │
       │                     ◄──────────┼──────────────────────────────┤
       │                                │                              │
       │  4. Collect precommits ◄───────┤                              │
       │                        ◄───────┼──────────────────────────────┤
       │                                │                              │
       │  5. Finalize & commit          │  Finalize & commit           │  Finalize & commit
       ▼                                ▼                              ▼
```

### Transport Layers

| Layer | Protocol | Purpose |
|-------|----------|---------|
| **TCP Sync** | TCP | Reliable block and state synchronization |
| **UDP Gossip** | UDP | Low-latency transaction and vote propagation |
| **Noise Protocol** | Optional | Encrypted P2P communication (enable with `noise_enabled: true`) |

### Gossip Configuration

Transaction and vote gossip uses configurable parameters:

- Messages are forwarded to all connected peers
- Duplicate message detection prevents re-broadcasting
- Connection health is monitored with periodic pings

## Configuration Reference

Complete reference of all configuration parameters with their default values.

### `engine` — Core Engine Settings

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `chain_id` | `u64` | `131071` | Chain identifier (must match genesis) |
| `state_path` | `string` | `"state"` | Directory for state storage |
| `gas_limit_per_block` | `u64` | `30000000` | Maximum gas per block (30M) |
| `fee_elasticity_multiplier` | `u64` | `2` | EIP-1559 elasticity multiplier |
| `fee_max_change_denominator` | `u64` | `8` | Max base fee change per block (12.5%) |
| `storage_backend` | `string` | `"sled"` | Storage backend: `"sled"`, `"redb"`, or `"memory"` |

### `mempool` — Transaction Pool

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_total` | `usize` | `10000` | Maximum transactions in the mempool |
| `max_per_sender` | `usize` | `1000` | Maximum pending txs per sender address |
| `bump_bps` | `u64` | `1000` | Minimum gas price bump for tx replacement (10%) |

### `p2p` — Peer-to-Peer Networking

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `node_key_path` | `string` | `"state/node_key.json"` | Path to the node identity key |
| `peer_store_path` | `string` | `"state/peers.json"` | Path to persistent peer list |
| `listen` | `string` | `"0.0.0.0:30303"` | P2P listen address |
| `peers` | `string[]` | `[]` | Seed peer addresses for bootstrap |
| `block_time_ms` | `u64` | `1000` | Target block production interval in ms |
| `noise_enabled` | `bool` | `false` | Enable Noise protocol encryption |

### `rpc` — JSON-RPC Server

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `false` | Enable the HTTP JSON-RPC server |
| `addr` | `string` | `"127.0.0.1:8545"` | RPC listen address and port |

### `ws` — WebSocket Server

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `false` | Enable the WebSocket server |
| `addr` | `string` | `"127.0.0.1:9945"` | WebSocket listen address and port |

### `slashing` — Slashing Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `double_sign_bps` | `u64` | `500` | Base penalty for double-signing (5%) |
| `timeout_bps` | `u64` | `100` | Base penalty for timeout (1%) |
| `escalation_step_bps` | `u64` | `25` | Penalty increase per repeated offense (0.25%) |
| `escalation_max_bps` | `u64` | `1000` | Maximum escalated penalty (10%) |
| `round_timeout_ms` | `u64` | `500` | Consensus round timeout in milliseconds |
| `unbonding_period` | `u64` | `2` | Epochs before unbonded stake is withdrawable |

### `token_economics` — Reward & Supply

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_supply` | `string` | `"1000000000000000000000000000"` | Maximum MRSN supply (1B × 10¹⁸) |
| `initial_reward_per_block` | `string` | `"10000000000000000000"` | Block reward (10 MRSN × 10¹⁸) |
| `halving_interval` | `u64` | `35000000` | Blocks between reward halvings |

### `prime_orders` — PrimeOrders Precompile

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_margin_bps` | `u64` | `0` | Initial margin requirement (basis points) |
| `maintenance_margin_bps` | `u64` | `0` | Maintenance margin requirement (basis points) |

### `bridge` — Cross-Domain Bridge

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_queue_len` | `usize` | `10000` | Maximum pending bridge messages |

### `zk` — Zero-Knowledge Proofs

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `false` | Enable ZK proof generation |
| `checkpoint_interval` | `u64` | `100` | Blocks between ZK checkpoints |

### Example Full Configuration

```json
{
  "engine": {
    "chain_id": 131071,
    "state_path": "state",
    "gas_limit_per_block": 30000000,
    "fee_elasticity_multiplier": 2,
    "fee_max_change_denominator": 8,
    "storage_backend": "sled"
  },
  "mempool": {
    "max_total": 10000,
    "max_per_sender": 1000,
    "bump_bps": 1000
  },
  "p2p": {
    "node_key_path": "state/node_key.json",
    "peer_store_path": "state/peers.json",
    "listen": "0.0.0.0:30303",
    "peers": [
      "46.225.30.187:30303"
    ],
    "block_time_ms": 1000,
    "noise_enabled": false
  },
  "rpc": {
    "enabled": true,
    "addr": "0.0.0.0:8545"
  },
  "ws": {
    "enabled": true,
    "addr": "0.0.0.0:9945"
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
  "prime_orders": {
    "initial_margin_bps": 0,
    "maintenance_margin_bps": 0
  },
  "bridge": {
    "max_queue_len": 10000
  },
  "zk": {
    "enabled": false,
    "checkpoint_interval": 100
  }
}
```

## Summary

Mersennet's PoS consensus provides:

- **Fast finality** (~1s block time)
- **Fair proposer rotation** (stake-weighted round-robin)
- **BFT security** (2/3+ stake required)
- **Economic security** (slashing for misbehavior)
- **No reversals** (finalized blocks are final)
