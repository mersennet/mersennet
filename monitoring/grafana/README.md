# Mersennet Grafana Dashboards

This directory contains Grafana dashboard JSON files for monitoring Mersennet nodes.

## Dashboards

### Mersennet Overview (`mersennet-overview.json`)
Comprehensive overview of chain health, throughput, execution engine, CLOB, and consensus:

- **Chain Health**: Blocks produced, block height, node uptime, block time
- **Transaction Throughput**: Tx per block, TPS, RPC requests, RPC latency
- **Execution Engine**: Gas used, EVM execution time, mempool size, block throughput
- **MersennetOrders CLOB**: Orders matched, active markets, CLOB activity
- **Consensus & Network**: Consensus rounds, validators, finalization, RPC messages

### Mersennet Performance (`mersennet-performance.json`)
Performance-focused metrics:

- **EVM TPS**: Transaction throughput over time
- **CLOB Ops/s**: Order and trade rate
- **Block Production Latency**: p50, p95, p99 block execution time
- **State Commit Time**: Block execution duration
- **Parallel Execution Efficiency**: Transactions per second of execution
- **Resource Proxies**: Mempool size, RPC latency

## Importing Dashboards

### Option 1: Manual Import (UI)

1. Start Grafana (see `../docker-compose.monitoring.yml`)
2. Log in as `admin` with the password from `GRAFANA_ADMIN_PASSWORD` (required — there is no default)
3. Go to **Dashboards** → **Import**
4. Click **Upload JSON file** and select a dashboard file
5. Select your Prometheus datasource from the dropdown
6. Click **Import**

### Option 2: Provisioning (Automatic)

1. The provisioning config is already checked in at
   `monitoring/grafana/provisioning/dashboards/dashboards.yml`:
   ```yaml
   apiVersion: 1
   providers:
     - name: 'Mersennet'
       orgId: 1
       folder: 'Mersennet'
       type: file
       disableDeletion: false
       updateIntervalSeconds: 30
       options:
         path: /var/lib/grafana/dashboards
   ```

2. Mount the provisioning path and the dashboard JSON files in
   docker-compose (see `../docker-compose.monitoring.yml`, which mounts
   this directory at `/var/lib/grafana/dashboards`)

### Option 3: Grafana API

```bash
curl -X POST -H "Content-Type: application/json" \
  -d @monitoring/grafana/mersennet-overview.json \
  -u "admin:$GRAFANA_ADMIN_PASSWORD" \
  http://localhost:3000/api/dashboards/db
```

## Prerequisites

- **Prometheus** must be scraping the Mersennet node's `/metrics` endpoint
- Configure the Prometheus datasource in Grafana to point to your Prometheus instance (default: `http://prometheus:9090` when using docker-compose)

## Datasource Variable

Dashboards use `${DS_PROMETHEUS}` for the Prometheus datasource. When importing, you will be prompted to select the datasource. Ensure a Prometheus datasource is configured first:

1. **Configuration** → **Data sources** → **Add data source**
2. Select **Prometheus**
3. Set URL to `http://prometheus:9090` (Docker) or `http://localhost:9090` (local)
4. Save & Test

## Refresh & Time Range

- Default refresh: **10 seconds**
- Default time range: **Last 1 hour**
- Theme: **Dark**
