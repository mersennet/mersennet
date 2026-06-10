---
slug: /validators/run-a-node
sidebar_position: 2
title: "Run a Validator Node"
---

# Run a Validator Node

This guide walks you through building, configuring, and running a Mersennet validator node from source.

## Prerequisites

### Hardware Requirements

| Resource | Minimum | Recommended | Notes |
|----------|---------|-------------|-------|
| **CPU** | 4 cores | 8+ cores | Block production and EVM execution are CPU-bound |
| **RAM** | 8 GB | 16 GB | State trie and mempool reside in memory |
| **Storage** | 100 GB SSD | 500 GB NVMe SSD | State grows over time; NVMe recommended for I/O |
| **Network** | 100 Mbps | 1 Gbps | Low latency matters for consensus round-trips |
| **OS** | Ubuntu 22.04+ | Ubuntu 24.04 LTS | Any modern Linux; macOS for development only |

:::caution
Running on HDD (spinning disk) is not recommended. The sled storage backend performs frequent random reads/writes that require SSD-class IOPS.
:::

### Software Requirements

| Requirement | Version | Purpose |
|-------------|---------|---------|
| **Rust** | 1.75 or later | Compiler for building from source |
| **Git** | Latest | Clone the repository |
| **build-essential** | Latest | C linker and system libraries |
| **pkg-config** | Latest | Library discovery for native dependencies |
| **libssl-dev** | Latest | TLS support for networking |

Install all dependencies on Ubuntu/Debian:

```bash
sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev git
```

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustc --version  # Should be 1.75+
```

## Build from Source

Clone the Mersennet repository and build the release binary:

```bash
git clone https://github.com/PrimeNumbersLabs/prime-chain.git
cd prime-chain
cargo build --release
```

The binary will be at `target/release/prime-chain` (built from the `prime-chain-node` crate).

## Configuration

Mersennet uses a JSON configuration file. Create `config.json` with the sections relevant to your deployment.

### Minimal Validator Configuration

```json
{
  "engine": {
    "chain_id": 7919,
    "state_path": "/var/lib/prime-chain/state",
    "storage_backend": "sled"
  },
  "p2p": {
    "node_key_path": "/var/lib/prime-chain/state/node_key.json",
    "listen": "0.0.0.0:30303",
    "peers": ["46.225.30.187:30303"],
    "block_time_ms": 1000
  },
  "rpc": {
    "enabled": true,
    "addr": "0.0.0.0:8545"
  },
  "ws": {
    "enabled": true,
    "addr": "0.0.0.0:9945"
  }
}
```

### Full Configuration Reference

#### `engine` — Core Engine

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `chain_id` | `u64` | `7919` | Chain identifier — must match genesis and network |
| `state_path` | `string` | `"state"` | Directory for state database and metadata |
| `gas_limit_per_block` | `u64` | `30000000` | Maximum gas per block (30M) |
| `fee_elasticity_multiplier` | `u64` | `2` | EIP-1559 elasticity (target = limit / multiplier) |
| `fee_max_change_denominator` | `u64` | `8` | Max base fee change per block (1/8 = 12.5%) |
| `storage_backend` | `string` | `"sled"` | `"sled"`, `"redb"`, or `"memory"` |

#### `mempool` — Transaction Pool

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_total` | `usize` | `10000` | Maximum transactions in the mempool |
| `max_per_sender` | `usize` | `1000` | Max pending txs per sender address |
| `bump_bps` | `u64` | `1000` | Min gas price bump for replacement (10%) |

#### `p2p` — Peer-to-Peer Networking

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `node_key_path` | `string` | `"state/node_key.json"` | Path to node identity keypair |
| `peer_store_path` | `string` | `"state/peers.json"` | Path to persistent peer list |
| `listen` | `string` | `"0.0.0.0:30303"` | P2P listen address |
| `peers` | `string[]` | `[]` | Seed peers for bootstrap |
| `block_time_ms` | `u64` | `1000` | Block production interval (ms) |
| `noise_enabled` | `bool` | `false` | Encrypt P2P with Noise Protocol |

#### `rpc` — JSON-RPC Server

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `false` | Enable the HTTP JSON-RPC server |
| `addr` | `string` | `"127.0.0.1:8545"` | RPC listen address |

#### `ws` — WebSocket Server

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `false` | Enable the WebSocket server |
| `addr` | `string` | `"127.0.0.1:9945"` | WebSocket listen address |

#### `slashing` — Slashing Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `double_sign_bps` | `u64` | `500` | Double-sign penalty (5%) |
| `timeout_bps` | `u64` | `100` | Timeout penalty (1%) |
| `escalation_step_bps` | `u64` | `25` | Penalty increase per offense (0.25%) |
| `escalation_max_bps` | `u64` | `1000` | Maximum penalty cap (10%) |
| `round_timeout_ms` | `u64` | `500` | Consensus round timeout (ms) |
| `unbonding_period` | `u64` | `2` | Epochs before unbonded stake withdrawable |

#### `token_economics` — Rewards & Supply

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_supply` | `string` | `"1000000000000000000000000000"` | Max PRIM supply (1B × 10¹⁸) |
| `initial_reward_per_block` | `string` | `"10000000000000000000"` | Block reward (10 PRIM) |
| `halving_interval` | `u64` | `35000000` | Blocks between reward halvings |

#### `prime_orders` — PrimeOrders Precompile

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `initial_margin_bps` | `u64` | `0` | Initial margin requirement (bps) |
| `maintenance_margin_bps` | `u64` | `0` | Maintenance margin (bps) |

#### `bridge` — Cross-Domain Bridge

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_queue_len` | `usize` | `10000` | Max pending bridge messages |

#### `zk` — Zero-Knowledge Proofs

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `false` | Enable ZK proof generation |
| `checkpoint_interval` | `u64` | `100` | Blocks between ZK checkpoints |

:::tip
Check the [Network Information](/getting-started/network-info) page for current seed peer addresses.
:::

## Genesis Setup

For **mainnet** or **testnet**, you need the correct genesis allocations. Genesis is not a separate file — it is the `genesis` section of your `config.json`, defining initial accounts and validators. The chain ID is the numeric `engine.chain_id` (7919 for testnet, 13370 for mainnet; see `mainnet/genesis.json` in the repository for the canonical mainnet parameters).

Example `genesis` section:

```json
{
  "engine": {
    "chain_id": 7919
  },
  "genesis": {
    "accounts": [
      {
        "address": "0x...",
        "balance": "10000000000000000000000000",
        "nonce": 0
      }
    ],
    "validators": [
      {
        "address": "0x...",
        "stake": "1000000"
      }
    ]
  }
}
```

## Starting the Node

Run the node with your config in validator mode:

```bash
./target/release/prime-chain \
  --config config.json \
  --mode validator
```

The `--mode` flag accepts `validator`, `full`, or `devnet` (the default if omitted, which runs a local demo — always pass `--mode` for real deployments).

For a **non-validator** (sentinel/full node), start with `--mode full`. The node will sync and serve RPC but will not propose blocks.

## Registering as a Validator

To join the validator set:

1. **Stake PRIM** — Send a staking transaction to register your validator address with your desired stake amount. See [Staking Guide](/validators/staking).

2. **Ensure your node is synced** — Wait until your node has caught up to the latest block height.

3. **Node key** — Your `p2p.node_key_path` must point to a key file that corresponds to the address you staked from. The node will automatically begin participating in consensus once registered and synced.

Validator registration happens through the genesis configuration (for initial validators) or via staking transactions. There is no separate RPC method for registration—once you stake PRIM and your node is synced with the correct validator key, you join the active set.

## Systemd Service (Production)

For production deployments, run the node as a systemd service with proper resource limits and security hardening.

### Create a Dedicated User

```bash
sudo useradd --system --home-dir /var/lib/prime-chain --shell /usr/sbin/nologin prime
sudo mkdir -p /var/lib/prime-chain/state
sudo chown -R prime:prime /var/lib/prime-chain
```

### Service File

Create `/etc/systemd/system/prime-chain.service`:

```ini
[Unit]
Description=Mersennet Validator Node
Documentation=https://docs.mersennet.io
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=prime
Group=prime
WorkingDirectory=/var/lib/prime-chain

ExecStart=/usr/local/bin/prime-chain \
    --config /etc/prime-chain/config.json \
    --mode validator

Restart=always
RestartSec=10
StartLimitInterval=200
StartLimitBurst=5

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=prime-chain

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/prime-chain
PrivateTmp=yes

# Resource limits
LimitNOFILE=65535
LimitNPROC=4096
MemoryMax=12G

# Environment
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"

[Install]
WantedBy=multi-user.target
```

### Install and Start

```bash
# Copy binary
sudo cp target/release/prime-chain /usr/local/bin/
sudo chmod +x /usr/local/bin/prime-chain

# Copy config
sudo mkdir -p /etc/prime-chain
sudo cp config.json /etc/prime-chain/

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable prime-chain
sudo systemctl start prime-chain

# Check status
sudo systemctl status prime-chain
sudo journalctl -u prime-chain -f
```

### Log Levels

Control verbosity via the `RUST_LOG` environment variable:

```bash
# Default — info level
Environment="RUST_LOG=info"

# Debug networking issues
Environment="RUST_LOG=info,prime_chain_network=debug"

# Trace EVM execution (very verbose)
Environment="RUST_LOG=info,prime_chain::engine=trace"

# Quiet mode — warnings and errors only
Environment="RUST_LOG=warn"
```

## Log Interpretation Guide

Understanding node log messages helps diagnose issues quickly.

### Normal Operation

| Log Message | Meaning |
|------------|---------|
| `node identity loaded address=0x...` | Node keypair loaded successfully |
| `registered genesis validator address=0x...` | Validator registered from genesis config |
| `RPC server listening on 0.0.0.0:8545` | JSON-RPC server started |
| `P2P listening on 0.0.0.0:30303` | Peer-to-peer networking active |
| `block produced number=N txs=M gas_used=G` | Block successfully produced |
| `block finalized number=N hash=0x...` | Block reached 2/3+ consensus |
| `peer connected addr=1.2.3.4:30303` | New peer connection established |

### Warning Signs

| Log Message | Meaning | Action |
|------------|---------|--------|
| `consensus timeout round=N` | Failed to get 2/3+ votes in time | Check peer connectivity |
| `mempool full, rejecting tx` | Mempool at capacity (10K default) | Increase `mempool.max_total` or reduce load |
| `peer disconnected addr=...` | Lost connection to a peer | Check network; peer may restart automatically |
| `slashing evidence kind=timeout` | This node missed a consensus round | Ensure clock sync and network stability |

### Error Messages

| Log Message | Meaning | Action |
|------------|---------|--------|
| `failed to read config file` | Config path invalid or unreadable | Verify `--config` path and file permissions |
| `invalid address hex` | Malformed address in genesis config | Fix the hex address in config.json |
| `signing failed` | Node key corrupted or missing | Restore from backup or regenerate identity |
| `state lock poisoned` | Internal state corruption (panic recovery) | Restart the node; report if persistent |

## Monitoring Setup

Mersennet exposes Prometheus-compatible metrics at the `/metrics` endpoint on the RPC port.

### Available Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `prime_chain_up` | Gauge | Node liveness (always 1 while running) |
| `prime_chain_height` | Gauge | Latest committed block height |
| `prime_chain_blocks_produced_total` | Counter | Total blocks produced |
| `prime_chain_block_gas_used` | Gauge | Gas consumed in latest block |
| `prime_chain_block_tx_count` | Gauge | Transaction count in latest block |
| `prime_chain_block_execution_seconds` | Histogram | Block execution duration |
| `prime_chain_base_fee_wei` | Gauge | Current EIP-1559 base fee |
| `prime_chain_consensus_rounds` | Counter | Total consensus rounds |
| `prime_chain_consensus_finalized` | Counter | Total blocks finalized |
| `prime_chain_validators_active` | Gauge | Active validator count |
| `prime_chain_total_stake` | Gauge | Total staked PRIM |
| `prime_chain_slashing_events` | Counter | Slashing events by kind |
| `prime_chain_mempool_size` | Gauge | Current mempool transaction count |
| `prime_chain_mempool_rejected` | Counter | Rejected transactions by reason |
| `prime_chain_rpc_requests` | Counter | RPC requests by method |
| `prime_chain_rpc_errors` | Counter | RPC errors by method and code |
| `prime_chain_rpc_duration_seconds` | Histogram | RPC request duration |
| `prime_chain_orders_submitted` | Counter | Orders submitted to order book |
| `prime_chain_orders_filled` | Counter | Orders fully filled |
| `prime_chain_trades_executed` | Counter | Trade fills executed |
| `prime_chain_markets_active` | Gauge | Active markets count |

### Prometheus Configuration

Add the following scrape target to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'prime-chain'
    scrape_interval: 15s
    metrics_path: '/metrics'
    static_configs:
      - targets: ['localhost:8545']
        labels:
          chain: 'prime-chain-testnet'
          node: 'validator-01'
```

### Grafana Dashboard

Connect Grafana to your Prometheus instance and create dashboards for:

- **Block Production** — Height over time, block execution duration, gas usage trends
- **Consensus Health** — Finalization rate, consensus rounds, active validators
- **Mempool** — Pool size, rejection rate, gas price distribution
- **Network** — Peer count, P2P message rates
- **PrimeOrders** — Order submission rate, fill rate, active markets

The public Grafana dashboard is available at [http://46.225.30.187:3000](http://46.225.30.187:3000).

### Alerting Rules

Recommended Prometheus alert rules:

```yaml
groups:
  - name: prime-chain
    rules:
      - alert: NodeDown
        expr: up{job="prime-chain"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Mersennet node is down"

      - alert: BlockProductionStalled
        expr: increase(prime_chain_blocks_produced_total[5m]) == 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "No blocks produced in 5 minutes"

      - alert: HighMempoolSize
        expr: prime_chain_mempool_size > 8000
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "Mempool approaching capacity"

      - alert: SlashingEvent
        expr: increase(prime_chain_slashing_events[5m]) > 0
        labels:
          severity: critical
        annotations:
          summary: "Validator slashing event detected"

      - alert: ConsensusTimeouts
        expr: increase(prime_chain_consensus_rounds[5m]) - increase(prime_chain_consensus_finalized[5m]) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High number of consensus timeouts"
```

## Backup and Restore

### What to Back Up

| Path | Contents | Critical? |
|------|----------|-----------|
| `state/node_key.json` | Node identity keypair (validator signing key) | **Yes** — loss means new identity |
| `state/peers.json` | Known peer addresses | No — peers rediscovered on restart |
| `state/sled_db/` | Full chain state (accounts, storage, blocks) | Yes — loss requires full resync |
| `config.json` | Node configuration | Yes — keep in version control |

### Backup Procedure

```bash
# Stop the node first to ensure consistent state
sudo systemctl stop prime-chain

# Create timestamped backup
BACKUP_DIR="/backups/prime-chain/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$BACKUP_DIR"
cp -r /var/lib/prime-chain/state "$BACKUP_DIR/"
cp /etc/prime-chain/config.json "$BACKUP_DIR/"

# Restart the node
sudo systemctl start prime-chain
```

### Restore Procedure

```bash
# Stop the node
sudo systemctl stop prime-chain

# Restore from backup
BACKUP_DIR="/backups/prime-chain/20260101-120000"
rm -rf /var/lib/prime-chain/state
cp -r "$BACKUP_DIR/state" /var/lib/prime-chain/
chown -R prime:prime /var/lib/prime-chain

# Start the node — it will catch up from the restored height
sudo systemctl start prime-chain
```

:::warning
Never run two nodes with the same `node_key.json` simultaneously — this may trigger double-sign slashing.
:::

## Troubleshooting

### Common Issues

#### Node won't start

| Symptom | Cause | Solution |
|---------|-------|----------|
| `failed to read config file` | Config path wrong or missing | Check `--config` path; ensure file exists and is valid JSON |
| `address already in use` | Port conflict (8545, 30303, or 9945) | Stop conflicting process or change port in config |
| `permission denied` | File/directory permissions | `chown -R prime:prime /var/lib/prime-chain` |
| `failed to install Prometheus metrics exporter` | Duplicate recorder initialization | Ensure only one node instance is running |

#### Node not producing blocks

| Symptom | Cause | Solution |
|---------|-------|----------|
| No `block produced` logs | Not registered as validator | Stake PRIM and verify your node key matches |
| Only importing blocks | Node is syncing | Wait for sync to complete to chain tip |
| `consensus timeout` repeated | Network partition or clock drift | Check peers list; sync system clock with NTP |

#### Peer connection issues

| Symptom | Cause | Solution |
|---------|-------|----------|
| 0 connected peers | Firewall blocking port 30303 | Open TCP/UDP 30303 in firewall |
| Peers connect then disconnect | Chain ID mismatch | Verify `engine.chain_id` matches network (7919) |
| Slow block propagation | High network latency | Use peers geographically closer; check bandwidth |

#### High resource usage

| Symptom | Cause | Solution |
|---------|-------|----------|
| Memory >12GB and growing | State trie growth | Normal for long-running nodes; increase RAM or add swap |
| CPU constantly at 100% | EVM execution or block production | Check for spam transactions; consider `gas_limit_per_block` adjustment |
| Disk filling up | State database growth | Increase storage; consider pruning old data |

### Useful Commands

```bash
# Check node status
sudo systemctl status prime-chain

# Follow logs in real time
sudo journalctl -u prime-chain -f

# Check block height via RPC
curl -s http://localhost:8545 -X POST \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | jq

# Check connected peer count
curl -s http://localhost:8545 -X POST \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' | jq

# Check node sync status
curl -s http://localhost:8545 -X POST \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_syncing","params":[],"id":1}' | jq

# View Prometheus metrics
curl -s http://localhost:8545/metrics | grep prime_chain_height

# Check disk usage
du -sh /var/lib/prime-chain/state/
```

## Next Steps

- [Staking Guide](/validators/staking) — Stake PRIM and manage delegations
- [Monitoring & Alerts](/validators/monitoring) — Set up Prometheus and Grafana
- [Node Architecture](/architecture/node-architecture) — Understand the node internals
- [Consensus Mechanism](/architecture/consensus) — Deep dive into DPoS and BFT
