# Prime Chain Public Testnet

This directory contains tooling to run a Prime Chain public testnet with multiple validators, an RPC node, a faucet, and monitoring.

## Quick Start

### 1. Generate Genesis Configuration

From the project root:

```bash
cargo run --bin genesis -- --validators 4 --chain-id 999 --output-dir genesis-output
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
- **Grafana**: http://localhost:3000 (admin/primechain)
- **Prometheus**: http://localhost:9099

## Network Parameters

| Parameter    | Value |
|-------------|-------|
| Chain ID    | 999   |
| RPC URL     | http://localhost:8545 |
| Faucet URL  | http://localhost:8080 |
| Block Time  | ~1 second |
| Currency    | PRIME (18 decimals) |

## Connecting MetaMask

1. Open MetaMask → Networks → Add Network
2. Use:
   - **Network Name**: Prime Chain Testnet
   - **RPC URL**: `http://localhost:8545` (or your public RPC)
   - **Chain ID**: 999
   - **Currency Symbol**: PRIME

3. Import an account or create one, then use the faucet to fund it.

## Using the Faucet

### Web UI

Visit http://localhost:8080 and enter your wallet address (0x...). Click "Request Tokens" to receive 1000 test PRIME.

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
| Validator 2 | 8547 | Validator RPC  |
| Validator 3 | 8548 | Validator RPC  |
| Validator 4 | 8549 | Validator RPC  |
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
