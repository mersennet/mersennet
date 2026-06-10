# Mersennet Testnets

This directory contains tooling to bring up two distinct Mersennet
networks:

| Compose file | Chain ID | Purpose |
|---|---|---|
| `docker-compose.testnet.yml` | **131071** | Transparent public testnet — EVM + PrimeOrders CLOB |
| `docker-compose.privacy.yml` | **131071** | Privacy testnet — shielded accounts, sealed-bid liquidations, threshold mempool, 5-of-7 DKG |

Both compose files run a multi-validator stack plus an RPC observer,
faucet, Prometheus, and Grafana.

---

## A. Privacy testnet (chain 131071) — recommended starting point

Full operator runbook:
[`../docs/runbooks/privacy-testnet-bootstrap.md`](../docs/runbooks/privacy-testnet-bootstrap.md).

```bash
# Build + generate keys + (optionally) migrate a pre-fork snapshot
./scripts/bootstrap-privacy-genesis.sh

# Bring up 7 validators + RPC observer + faucet + monitoring
docker compose -f docker-compose.privacy.yml up -d --build
```

After ~60s:

| Service | URL |
|---|---|
| RPC observer (HTTP + WS) | http://localhost:8545 / ws://localhost:8546 |
| Faucet | http://localhost:8081 |
| Grafana (privacy dashboard auto-loaded) | http://localhost:3001 |
| Prometheus | http://localhost:9100 |
| Validator RPCs | http://localhost:18545..18551 |

Sanity check:

```bash
curl -s http://localhost:8545/health
curl -s -X POST -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"prime_getChainConfig","params":[]}' \
  http://localhost:8545
```

Synthetic load + chaos drill:

```bash
./scripts/privacy-load.sh             # ~10 min synthetic order flow
./scripts/chaos-kill-validator.sh     # kill-recover liveness test
```

Tear down (wipes volumes):

```bash
docker compose -f docker-compose.privacy.yml down -v
```

---

## B. Transparent testnet (chain 131071)

This is the original public testnet — same network as
`https://rpc.primechain.xyz` runs.

## Quick Start

### 1. Generate Genesis Configuration

From the project root:

```bash
cargo run --bin genesis -- --validators 4 --chain-id 131071 --output-dir genesis-output
```

This creates:
- `genesis-output/configs/` – Per-validator and RPC node configs
- `genesis-output/keys/` – Validator and faucet private keys
- `genesis-output/genesis.json` – Genesis state
- `genesis-output/faucet-key.json` – Faucet funding key (keep secure!)
- `genesis-output/docker-compose.yml` – Standalone validator compose

### 2. Launch the Testnet

```bash
cd testnet
docker compose -f docker-compose.testnet.yml up -d --build
```

### 3. Verify Services

- **RPC**: http://localhost:8545
- **Faucet**: http://localhost:8080
- **Grafana**: http://localhost:3000 (admin / `changeme`, override with `GRAFANA_ADMIN_PASSWORD`)
- **Prometheus**: http://localhost:9099

## Network Parameters

| Parameter    | Value |
|-------------|-------|
| Chain ID    | 131071   |
| RPC URL     | http://localhost:8545 |
| Faucet URL  | http://localhost:8080 |
| Block Time  | ~1 second |
| Currency    | MRSN (18 decimals) |

## Connecting MetaMask

1. Open MetaMask → Networks → Add Network
2. Use:
   - **Network Name**: Mersennet Testnet
   - **RPC URL**: `http://localhost:8545` (or your public RPC)
   - **Chain ID**: 131071
   - **Currency Symbol**: MRSN

3. Import an account or create one, then use the faucet to fund it.

## Using the Faucet

### Web UI

Visit http://localhost:8080 and enter your wallet address (0x...). Click "Request Tokens" to receive 1,000 test MRSN.

### API

```bash
curl -X POST http://localhost:8080/faucet \
  -H "Content-Type: application/json" \
  -d '{"address": "0xYourAddress"}'
```

**Rate limit**: 1 request per address per hour.

## Architecture

- **4 Validators** – Produce blocks, run consensus
- **1 RPC Node** – Non-validator, syncs from validators, exposes port 8545
- **1 Faucet** – HTTP service that funds accounts via `eth_sendRawTransaction`
- **Prometheus** – Scrapes metrics from validators and RPC
- **Grafana** – Dashboards for monitoring

## Ports

| Service    | Port | Description        |
|-----------|------|--------------------|
| RPC Node  | 8545 | Main public RPC    |
| Validator 1 | 8546 | Validator RPC  |
| Validator 2 | 8546 | Validator RPC (compose currently maps the same host port as validator 1) |
| Validator 3 | 8547 | Validator RPC  |
| Validator 4 | 8548 | Validator RPC  |
| Faucet    | 8080 | Faucet HTTP API    |
| Grafana   | 3000 | Monitoring UI      |
| Prometheus| 9099 | Metrics            |

## Stopping the Testnet

```bash
docker compose -f docker-compose.testnet.yml down
```

To remove persistent data (state, keys):

```bash
docker compose -f docker-compose.testnet.yml down -v
```

## Security Notes

- **faucet-key.json** holds the faucet’s private key. Do not commit it or expose it.
- Validator keys in `genesis-output/keys/` are for testnet only.
- For production, use proper key management and secure genesis ceremonies.

---

## See also

- [`docs/runbooks/privacy-testnet-bootstrap.md`](../docs/runbooks/privacy-testnet-bootstrap.md) — full privacy-testnet bring-up.
- [`docs/runbooks/zk-fork-activation.md`](../docs/runbooks/zk-fork-activation.md) — mainnet hard-fork checklist.
- [`docs/DEVELOPER_GUIDE.md`](../docs/DEVELOPER_GUIDE.md) — repo-wide developer entry point.
- [`docs/STATUS.md`](../docs/STATUS.md) — workstream tracker.
