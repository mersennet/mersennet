# Mersennet Testnet Deployment

One-command deployment of a 4-validator + 1 public-node testnet to Hetzner VPS.

## Architecture

```
┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│ Validator 1  │  │ Validator 2  │  │ Validator 3  │  │ Validator 4  │
│  CX22 €4/mo  │  │  CX22 €4/mo  │  │  CX22 €4/mo  │  │  CX22 €4/mo  │
│  P2P :30303  │  │  P2P :30303  │  │  P2P :30303  │  │  P2P :30303  │
│  RPC :8545*  │  │  RPC :8545*  │  │  RPC :8545*  │  │  RPC :8545*  │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │                 │
       └─────────────────┼─────────────────┼─────────────────┘
                         │     P2P mesh    │
                    ┌────┴─────────────────┴────┐
                    │       Public Node          │
                    │       CX32 €7/mo           │
                    │  RPC :8545  (public)       │
                    │  Faucet :8080              │
                    │  Grafana :3000             │
                    │  Prometheus :9090          │
                    │  Caddy :443 (TLS, opt.)    │
                    └────────────────────────────┘
                    * Validator RPC is firewall-blocked from public
```

## Prerequisites

1. **5 Hetzner Cloud VPS** (Ubuntu 24.04):
   - 4x CX22 (2 vCPU, 4GB RAM) — validators
   - 1x CX32 (4 vCPU, 8GB RAM) — public node
2. **SSH key** added during VPS creation (so `ssh root@IP` works)
3. **Rust toolchain** installed locally

## Quick Start

```bash
# 1. Copy the example config and fill in your VPS IPs
cp deploy/nodes.conf.example deploy/nodes.conf
nano deploy/nodes.conf

# 2. Deploy everything (builds, generates genesis, deploys to all 5 nodes)
./deploy/deploy-testnet.sh
```

That's it. The script will:
- Build release binaries locally
- Run the genesis ceremony (generates keys + configs)
- SCP binaries and configs to each VPS
- Install systemd services and firewall rules
- Start all nodes
- Print a health check dashboard

## Management Scripts

```bash
# Check health of all nodes
./deploy/testnet-status.sh

# Stream logs from a specific node
./deploy/testnet-logs.sh 1        # validator 1
./deploy/testnet-logs.sh public   # public node
./deploy/testnet-logs.sh faucet   # faucet service

# Restart nodes
./deploy/testnet-restart.sh all     # all nodes
./deploy/testnet-restart.sh 2       # just validator 2
./deploy/testnet-restart.sh public  # just public node

# Stop all nodes
./deploy/testnet-stop.sh

# Rolling upgrade (rebuild + deploy one-by-one)
./deploy/testnet-upgrade.sh
```

## Endpoints (after deployment)

| Service | URL |
|---------|-----|
| JSON-RPC | `http://<PUBLIC_IP>:8545` |
| Faucet | `http://<PUBLIC_IP>:8080` |
| Grafana | `http://<PUBLIC_IP>:3000` (admin / `changeme`, override with `GRAFANA_ADMIN_PASSWORD`) |
| Prometheus | `http://<PUBLIC_IP>:9090` |
| Metrics | `http://<PUBLIC_IP>:8545/metrics` |

## Optional: TLS with Custom Domain

Set `RPC_DOMAIN` and `FAUCET_DOMAIN` in `nodes.conf`:

```
RPC_DOMAIN=rpc.primechain.io
FAUCET_DOMAIN=faucet.primechain.io
```

Point DNS A records to `PUBLIC_NODE` IP. Caddy auto-provisions Let's Encrypt certificates.

## Firewall Rules

**Validators** (locked down):
- SSH (22/tcp)
- P2P gossip (30303/udp+tcp)
- All other ports blocked

**Public node** (open):
- SSH, HTTP/HTTPS, RPC (8545), Faucet (8080), Grafana (3000), P2P (30303)

## Troubleshooting

```bash
# Check service status on a node
ssh root@<IP> systemctl status mersennet

# View full logs
ssh root@<IP> journalctl -u mersennet -n 100 --no-pager

# Check disk usage
ssh root@<IP> du -sh /opt/mersennet/data/

# Manual restart
ssh root@<IP> systemctl restart mersennet
```
