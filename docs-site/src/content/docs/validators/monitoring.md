---
title: "Monitoring & Alerts"
---

Running a validator requires 24/7 visibility into node health, consensus participation, and resource usage. This guide covers setting up Prometheus and Grafana for Mersennet monitoring, plus recommended alert rules.

## Overview

A typical monitoring stack includes:

| Component | Purpose |
|-----------|---------|
| **Prometheus** | Scrapes metrics from the Mersennet node |
| **Grafana** | Dashboards and visualization |
| **Alertmanager** | Routes alerts (email, Slack, PagerDuty) |

## Key Metrics

Mersennet exposes metrics that you should monitor:

| Metric | Description |
|--------|-------------|
| `mersennet_height` | Latest committed block height; should increase steadily |
| `mersennet_total_stake` | Total staked MRSN across all validators |
| `mersennet_block_tx_count` | Transaction count in the latest block |
| `mersennet_mempool_size` | Mempool size; high values may indicate congestion |
| `mersennet_validators_active` | Number of active validators |
| `mersennet_slashing_events` | Slashing evidence events by kind (slashing risk) |

:::tip
The full metric list is exposed at the node's `/metrics` endpoint (served on the RPC port). See [Run a Node — Monitoring Setup](/validators/run-a-node#monitoring-setup) for the complete table.
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
  - job_name: 'mersennet'
    metrics_path: '/metrics'
    static_configs:
      - targets: ['localhost:8545']  # Mersennet RPC port (serves /metrics)
```

Mersennet serves Prometheus metrics at `GET /metrics` on the JSON-RPC port (`rpc.addr`, default 8545). The RPC server must be enabled (`rpc.enabled: true`).

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

- **Block height** — Graph of `mersennet_height` over time
- **Total stake** — Gauge or stat for `mersennet_total_stake`
- **Blocks produced** — Rate of `mersennet_blocks_produced_total`
- **Pending transactions** — `mersennet_mempool_size`
- **Active validators** — `mersennet_validators_active`
- **Slashing events** — `mersennet_slashing_events` (critical for validators)

## Alert Rules

Configure Prometheus alerting to catch issues before they cause slashing or downtime.

### Prometheus Alert Rules

Create `alerts.yml` (or add to `prometheus.yml`):

```yaml
groups:
  - name: mersennet
    rules:
      # Block production stalled
      - alert: MersennetBlockStalled
        expr: increase(mersennet_height[5m]) == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Mersennet block production stalled"
          description: "No new blocks in 5 minutes. Node may be out of sync or consensus may be stuck."

      # Slashing events (slashing risk)
      - alert: MersennetSlashingEvents
        expr: increase(mersennet_slashing_events[1h]) > 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Validator slashing evidence recorded"
          description: "Slashing evidence (timeout or double-sign) recorded in the last hour."

      # Low disk space
      - alert: MersennetLowDiskSpace
        expr: (node_filesystem_avail_bytes{mountpoint="/"} / node_filesystem_size_bytes{mountpoint="/"}) < 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Low disk space on Mersennet node"
          description: "Less than 10% disk space remaining. Node may stop if disk fills."

      # Mempool approaching capacity
      - alert: MersennetHighMempool
        expr: mersennet_mempool_size > 8000
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Mempool approaching capacity"
          description: "Mempool above 8000 of the default 10000 limit. Network may be congested."
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
