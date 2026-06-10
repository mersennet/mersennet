# Mersennet Testnet Setup Guide

Bring up the public Mersennet testnet (chain ID **131071** = 2¹⁷−1) from
fresh servers, and wire it into the live ecosystem
(explorer / faucet / trade on `*.mersennet.com`).

There are three ways to run it — pick one:

| Path | Use case | Time |
|---|---|---|
| **A. One-command VPS deploy** (recommended) | Real public testnet on 5 servers | ~15 min |
| **B. Manual step-by-step** | Custom topology, or to understand what A does | ~1 h |
| **C. Local Docker Compose** | Smoke-testing on one machine | ~5 min |

---

## 0. What you are deploying

```
┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│ Validator 1 │ │ Validator 2 │ │ Validator 3 │ │ Validator 4 │
│ P2P :30303  │ │ P2P :30303  │ │ P2P :30303  │ │ P2P :30303  │
│ RPC :8545*  │ │ RPC :8545*  │ │ RPC :8545*  │ │ RPC :8545*  │
└──────┬──────┘ └──────┬──────┘ └──────┬──────┘ └──────┬──────┘
       └───────────────┴───P2P mesh────┴───────────────┘
                            │
              ┌─────────────┴─────────────┐
              │        Public node        │
              │  RPC  :8545 (HTTP, open)  │
              │  WS   :8546 (open)        │
              │  Faucet :8080 (open)      │
              │  Grafana :3000            │
              └───────────────────────────┘
              * validator RPC is firewalled — P2P only
```

- **4 validators** produce blocks (BFT PoS, ~1–2 s block time).
- **1 public node** is a non-validating full node that exposes RPC/WS to
  the world and runs the faucet + monitoring. Validators are never
  exposed directly.
- Genesis, validator keys, and the faucet key are generated locally by
  the `genesis` tool and shipped to the servers — no key ever needs to
  be created on a server.

### Network parameters

| Parameter | Value |
|---|---|
| Chain ID | `131071` (hex `0x1FFFF`) |
| Currency | MRSN / PRIM, 18 decimals |
| Block time | 2000 ms default (`BLOCK_TIME_MS`) |
| P2P port | 30303 tcp+udp |
| RPC / WS | 8545 / 8546 |
| Faucet | 8080 (`POST /faucet`, 1,000 tokens, 1 req/addr/hour) |

---

## 1. Provision servers

Any provider works; the scripts were written for Hetzner Cloud.

| Role | Count | Spec | Example |
|---|---|---|---|
| Validator | 4 | 2 vCPU / 4 GB RAM / 40 GB disk | Hetzner CX22 (~€4/mo) |
| Public node | 1 | 4 vCPU / 8 GB RAM / 80 GB disk | Hetzner CX32 (~€7/mo) |

Requirements:

- **Ubuntu 24.04**, your SSH key added at creation (`ssh root@IP` must work).
- All 5 servers must reach each other on **30303 tcp/udp** (same region
  or public IPs are both fine — the setup scripts configure `ufw`).
- Note all 5 public IPs.

---

## 2. Path A — one-command deploy (recommended)

Run from your workstation, in the chain repo
(`~/mersennet/mersennet`). You need the Rust toolchain locally — the
script builds release binaries and pushes them out.

### 2.1 Configure

```bash
cd ~/mersennet/mersennet
cp deploy/nodes.conf.example deploy/nodes.conf
nano deploy/nodes.conf
```

```bash
# deploy/nodes.conf
VALIDATOR_1=<ip-1>
VALIDATOR_2=<ip-2>
VALIDATOR_3=<ip-3>
VALIDATOR_4=<ip-4>
PUBLIC_NODE=<ip-5>

SSH_USER=root
CHAIN_ID=131071          # already the default
BLOCK_TIME_MS=2000

# Optional: if you point rpc.mersennet.com at the public node,
# Caddy + TLS are installed automatically:
RPC_DOMAIN=rpc.mersennet.com
FAUCET_DOMAIN=faucet-api.mersennet.com
```

`RPC_DOMAIN` is optional — without it, RPC is plain HTTP on `:8545`,
which is what the current ecosystem Caddy proxies expect.

### 2.2 Deploy

```bash
./deploy/deploy-testnet.sh
```

The script:

1. `cargo build --release -p mersennet-node` (produces `mersennet`,
   `faucet`, `genesis` binaries)
2. Runs the **genesis ceremony** → `deploy/.genesis-output/`
   (validator keys, faucet key, genesis state, per-node configs)
3. Patches each config with the real IPs (P2P peer lists, listen
   addrs, state paths, WS on the public node)
4. SCPs binaries + configs to all 5 servers in parallel and runs
   `setup-node.sh`, which installs the systemd service
   (`mersennet.service`, plus `mersennet-faucet.service` on the public
   node), applies `ufw` rules, and starts everything
5. Prints a health dashboard

### 2.3 Verify

```bash
./deploy/testnet-status.sh                # all-node health

# Chain ID must be 0x1ffff (131071):
curl -s -X POST http://<PUBLIC_IP>:8545 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}'

# Blocks are being produced (run twice, number should grow):
curl -s http://<PUBLIC_IP>:8545/health

# Faucet works end-to-end (signs + submits a tx through RPC):
curl -s -X POST http://<PUBLIC_IP>:8080/faucet \
  -H 'content-type: application/json' \
  -d '{"address":"0x000000000000000000000000000000000000dEaD"}'
```

### 2.4 Day-2 operations

```bash
./deploy/testnet-status.sh    # health of all 5 nodes
./deploy/testnet-logs.sh      # tail journald logs from all nodes
./deploy/testnet-restart.sh   # rolling restart
./deploy/testnet-upgrade.sh   # rebuild binary + rolling upgrade (state kept)
./deploy/testnet-stop.sh      # stop all nodes

# Single node:
ssh root@<IP> journalctl -u mersennet -f
ssh root@<IP> systemctl restart mersennet
```

Monitoring: Grafana at `http://<PUBLIC_IP>:3000` (admin/changeme —
change it), Prometheus scrapes every node's `:8545/metrics`.

---

## 3. Path B — manual step-by-step

What Path A automates, in case you want a different topology or to do
the genesis ceremony separately.

### 3.1 Build

```bash
cargo build --release -p mersennet-node
# → target/release/{mersennet,faucet,genesis}
```

### 3.2 Genesis ceremony (on your workstation)

```bash
cargo run --release --bin genesis -- \
  --validators 4 --chain-id 131071 --output-dir genesis-output
```

Produces:

```
genesis-output/
├── genesis.json              # genesis state (same on every node)
├── configs/
│   ├── validator-{1..4}.json # per-validator config (includes genesis)
│   └── rpc-node.json         # public full-node config
└── keys/
    ├── validator-{1..4}.json # validator signing keys — testnet only
    └── faucet-key.json       # funds the faucet — keep private
```

### 3.3 Patch configs

For each `validator-N.json`, set:

```json5
{
  "engine": { "chain_id": 131071, "state_path": "/opt/mersennet/data/state" },
  "p2p": {
    "listen": "0.0.0.0:30303",
    "peers": ["<other-validator-ip>:30303", "...", "<public-ip>:30303"],
    "block_time_ms": 2000,
    "node_key_path": "/opt/mersennet/keys/node_key.json",
    "peer_store_path": "/opt/mersennet/data/peers.json"
  },
  "rpc": { "addr": "0.0.0.0:8545" }
}
```

For `rpc-node.json`, additionally enable WebSocket:

```json5
{ "ws": { "enabled": true, "addr": "0.0.0.0:8546" } }
```

### 3.4 Install on each server

```bash
# Layout
useradd -r -m -s /bin/bash mersennet
mkdir -p /opt/mersennet/{bin,config,data,keys}

# Files (from your workstation)
scp target/release/mersennet  root@IP:/opt/mersennet/bin/
scp configs/validator-N.json  root@IP:/opt/mersennet/config/config.json
scp keys/validator-N.json     root@IP:/opt/mersennet/keys/node_key.json
# Public node also gets: faucet binary + keys/faucet-key.json
```

Systemd unit (validators use `--validator`, the public node uses
`--mode full`):

```ini
# /etc/systemd/system/mersennet.service
[Service]
User=mersennet
ExecStart=/opt/mersennet/bin/mersennet \
    --config /opt/mersennet/config/config.json \
    --validator --rpc          # public node: --mode full --rpc
Restart=always
```

Faucet on the public node:

```ini
# /etc/systemd/system/mersennet-faucet.service
ExecStart=/opt/mersennet/bin/faucet \
    --port 8080 \
    --rpc-url http://127.0.0.1:8545 \
    --private-key /opt/mersennet/keys/faucet-key.json
```

Firewall:

- Validators: allow **22, 30303 tcp+udp** only (`deploy/firewall-validator.sh`)
- Public node: also **8545, 8546, 8080, 3000, 80/443** (`deploy/firewall-public.sh`)

```bash
systemctl daemon-reload
systemctl enable --now mersennet            # all nodes
systemctl enable --now mersennet-faucet     # public node only
```

Start all validators within a few seconds of each other; consensus
needs 3 of 4 online to produce blocks.

---

## 4. Path C — local Docker Compose (smoke test)

```bash
cargo run --bin genesis -- --validators 4 --chain-id 131071 --output-dir genesis-output
cd testnet
docker compose -f docker-compose.testnet.yml up -d --build
```

- RPC `http://localhost:8545`, faucet `http://localhost:8080`,
  Grafana `http://localhost:3000`, Prometheus `http://localhost:9099`
- Tear down (wipes state): `docker compose -f docker-compose.testnet.yml down -v`

---

## 5. Wire the new testnet into the ecosystem

The web properties on **178.104.211.138** currently point at the old
testnet host (`46.225.30.187`). After your new testnet is up, repoint
them — `<PUBLIC_IP>` below is your new public node.

### 5.1 Caddy proxies (explorer RPC + faucet API)

On `178.104.211.138`, edit `/etc/caddy/Caddyfile`:

| Replace | With |
|---|---|
| `http://46.225.30.187:8545` (explorer `/rpc`, 2 places) | `http://<PUBLIC_IP>:8545` |
| `http://46.225.30.187:4003` (faucet `/api`, 2 places) | `http://<PUBLIC_IP>:8080` |

```bash
sed -i 's|46.225.30.187:8545|<PUBLIC_IP>:8545|g; s|46.225.30.187:4003|<PUBLIC_IP>:8080|g' /etc/caddy/Caddyfile
caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy
```

> Note: the new faucet backend serves `POST /faucet` on port **8080**
> (the old one used 4003). The Caddy `handle_path /api/*` strip
> already forwards `/api/faucet` → `/faucet`, so only the IP:port
> changes.

### 5.2 Trade stack (API + indexer)

```bash
ssh root@178.104.211.138
cd /opt/mersennet-trade
RPC_URL=http://<PUBLIC_IP>:8545 WS_URL=ws://<PUBLIC_IP>:8546 \
  docker compose up -d
```

(`RPC_URL`/`WS_URL` are env-substituted in `docker-compose.yml`; no
rebuild needed.)

### 5.3 Docs + website RPC endpoints

`docs` (network-info, quick-starts) and the website CodeTabs embed the
RPC URL `http://46.225.30.187:8545`. Replace with the new endpoint
(or better, a `rpc.mersennet.com` domain) in:

- `docs/src/` — search `46.225.30.187`
- `website/app/site.ts` (`rpcUrl`) and `app/components/CodeTabs.tsx`

then rebuild + rsync both (see repo READMEs).

### 5.4 DNS (optional but recommended)

| Record | Type | Target |
|---|---|---|
| `rpc.mersennet.com` | A | `<PUBLIC_IP>` |

With `RPC_DOMAIN` set in `nodes.conf`, the deploy script installs
Caddy + TLS on the public node automatically, giving you
`https://rpc.mersennet.com` — then use that everywhere instead of the
raw IP, which also fixes HTTPS mixed-content for MetaMask users.

---

## 6. Smoke-test checklist

After wiring everything:

- [ ] `eth_chainId` returns `0x1ffff` via `https://explorer.mersennet.com/rpc`
- [ ] Explorer home shows live blocks; a block page shows the
      ZK-proof row in **Privacy & Proofs**
- [ ] `https://faucet.mersennet.com` funds an address (check the tx
      link lands on the explorer)
- [ ] MetaMask connects with Chain ID 131071 via the faucet/explorer
      "Connect" buttons
- [ ] Trade terminal shows the order book; the indexer logs
      `WebSocket connected` (`docker compose logs -f indexer`)
- [ ] Grafana dashboards show 5 healthy nodes

---

## 7. Security notes

- `genesis-output/keys/*` and `faucet-key.json` are **testnet** keys,
  but still: never commit them, never reuse for mainnet.
- Validator RPC must stay firewalled (the provided `ufw` scripts do
  this) — only P2P 30303 is public on validators.
- Change the Grafana admin password on first login.
- Keep `deploy/.genesis-output/` — you need the same genesis to add
  more nodes later. To add a full node: copy `rpc-node.json`, give it
  a fresh `node_key`, point `peers` at the validators.
- Re-genesis (`deploy-testnet.sh` from scratch) wipes the chain — all
  balances and deployed contracts are lost. For binary upgrades that
  keep state, use `./deploy/testnet-upgrade.sh`.
