---
slug: /validators/monitoring
sidebar_position: 4
title: "Monitoring & Alerts"
---

# Monitoring & Alerts

Running a validator requires 24/7 visibility into node health, consensus participation, and resource usage. This guide covers setting up Prometheus and Grafana for Prime Chain monitoring, plus recommended alert rules.

## Overview

A typical monitoring stack includes:

| Component | Purpose |
|-----------|---------|
| **Prometheus** | Scrapes metrics from the Prime Chain node |
| **Grafana** | Dashboards and visualization |
| **Alertmanager** | Routes alerts (email, Slack, PagerDuty) |

## Key Metrics

Prime Chain exposes metrics that you should monitor:

| Metric | Description |
|--------|-------------|
| `prime_chain_block_height` | Current block height; should increase steadily |
| `prime_chain_total_stake` | Total staked PRIM across all validators |
| `prime_chain_tx_count` | Transaction count (per block or cumulative) |
| `prime_chain_pending_txs` | Mempool size; high values may indicate congestion |
| `prime_chain_peer_count` | Number of connected P2P peers |
| `prime_chain_validator_missed_blocks` | Blocks you failed to sign (slashing risk) |

:::tip
Metric names may vary by implementation. Check your node's `/metrics` endpoint or documentation for the exact names.
:::

## Prometheus Setup

### 1. Install Prometheus

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install prometheus

# Or use the official binary
wget https://github.com/prometheus/prometheus/releases/download/v2.45.0/prometheus-2.45.0.linux-amd64.tar.gz
tar xvfz prometheus-*.tar.gz
cd prometheus-*
```

### 2. Configure Scraping

Edit `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'prime-chain'
    static_configs:
      - targets: ['localhost:9091']  # Prime Chain metrics port
```

Ensure your Prime Chain node exposes metrics on the configured port (e.g. 9091). The exact port is set in the node config.

### 3. Start Prometheus

```bash
./prometheus --config.file=prometheus.yml
```

## Grafana Setup

### 1. Install Grafana

```bash
# Ubuntu/Debian
sudo apt install -y software-properties-common
sudo add-apt-repository "deb https://packages.grafana.com/oss/deb stable main"
wget -q -O - https://packages.grafana.com/gpg.key | sudo apt-key add -
sudo apt update
sudo apt install grafana
sudo systemctl enable grafana-server
sudo systemctl start grafana-server
```

### 2. Add Prometheus Data Source

1. Open Grafana at `http://localhost:3000`
2. Login (default: admin/admin)
3. **Configuration** → **Data Sources** → **Add data source**
4. Select **Prometheus**
5. URL: `http://localhost:9090`
6. **Save & Test**

### 3. Import or Create Dashboards

Create panels for:

- **Block height** — Graph of `prime_chain_block_height` over time
- **Total stake** — Gauge or stat for `prime_chain_total_stake`
- **Transaction count** — Rate of `prime_chain_tx_count`
- **Pending transactions** — `prime_chain_pending_txs`
- **Peer count** — `prime_chain_peer_count`
- **Missed blocks** — `prime_chain_validator_missed_blocks` (critical for validators)

## Alert Rules

Configure Prometheus alerting to catch issues before they cause slashing or downtime.

### Prometheus Alert Rules

Create `alerts.yml` (or add to `prometheus.yml`):

```yaml
groups:
  - name: prime-chain
    rules:
      # Block production stalled
      - alert: PrimeChainBlockStalled
        expr: increase(prime_chain_block_height[5m]) == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Prime Chain block production stalled"
          description: "No new blocks in 5 minutes. Node may be out of sync or consensus may be stuck."

      # Missed blocks (slashing risk)
      - alert: PrimeChainMissedBlocks
        expr: increase(prime_chain_validator_missed_blocks[1h]) > 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Validator missed blocks"
          description: "Validator has missed blocks in the last hour. Risk of downtime slashing."

      # Low disk space
      - alert: PrimeChainLowDiskSpace
        expr: (node_filesystem_avail_bytes{mountpoint="/"} / node_filesystem_size_bytes{mountpoint="/"}) < 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Low disk space on Prime Chain node"
          description: "Less than 10% disk space remaining. Node may stop if disk fills."

      # Peer count too low
      - alert: PrimeChainLowPeerCount
        expr: prime_chain_peer_count < 3
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Low P2P peer count"
          description: "Fewer than 3 peers connected. Network connectivity may be degraded."
```

Reference the rules file in `prometheus.yml`:

```yaml
rule_files:
  - 'alerts.yml'
```

### Alertmanager (Optional)

To send alerts to email, Slack, or PagerDuty:

1. Install [Alertmanager](https://prometheus.io/docs/alerting/latest/alertmanager/)
2. Configure receivers (e.g. Slack webhook)
3. Set `alertmanager.url` in Prometheus config

## Best Practices

| Practice | Recommendation |
|----------|----------------|
| **Uptime** | Aim for 99.9%+ to avoid downtime slashing |
| **Disk** | Monitor and expand before hitting 10% free |
| **Peers** | Maintain at least 5–10 stable peers |
| **Backups** | Backup validator key and config; never expose the key |
| **Alerts** | Route critical alerts to a channel you monitor 24/7 |

## Next Steps

- [Validator Overview](/validators/overview) — Understand validator roles and risks
- [Staking Guide](/validators/staking) — Manage stake and delegations
