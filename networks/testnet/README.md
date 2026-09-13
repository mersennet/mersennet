# Mersennet Testnet (chain ID 131071)

Canonical artifacts for joining the public Mersennet testnet as a node
operator. Full guide: https://docs.mersennet.com/validators/run-a-node/

| File | Purpose |
|------|---------|
| `config.json` | Canonical full-node config. The `genesis`, `engine.chain_id`, and `token_economics` sections MUST NOT be modified — every node derives the same genesis state from them. Runtime sections (`rpc`, `ws`, `p2p.listen`, paths) are yours to adjust. |
| `mersennet.service` | Production systemd unit (hardened, auto-restart). |
| `install.sh` | One-command installer: binary + config + user + systemd service. Works from the release bundle or a source checkout. |

| `mersennet-check` | Operator health check (service, height vs network, peers, disk); installed to /usr/local/bin. |
| `bootstrap.sh` | Published as https://mersennet.com/downloads/install.sh — the one-line installer (fetches, verifies and runs the bundle's install.sh). |
| `package-release.sh` | Builds the public release bundle (binary + these files + checksums) published at https://mersennet.com/downloads/ for operators without source access. |

## Quick start

```bash
# Without source access: one-line install (downloads + verifies the latest bundle)
curl -fsSL https://mersennet.com/downloads/install.sh | sudo bash
#   ... --data-dir /mnt/blockstorage/mersennet   to keep chain data on a mounted volume
mersennet-check                                   # syncing? peers? disk?

# From a source checkout (Rust 1.85+, see docs for prerequisites)
cargo build --release --bin mersennet
sudo bash networks/testnet/install.sh

# ...or run in the foreground from any working directory
sudo install -m 0755 target/release/mersennet /usr/local/bin/mersennet
mkdir -p ~/mersennet-node && cp networks/testnet/config.json ~/mersennet-node/ && cd ~/mersennet-node
RUST_LOG=info mersennet --config config.json --mode full --rpc
```

## Snapshot bootstrap

Fresh installs start from the latest published state snapshot (`http://46.225.30.187:8088/latest.json`, tar+zstd of `data/state`, ~1 GB, SHA-256 in the manifest) and then sync only the tail: minutes instead of the ~20 hours a genesis replay takes at current transaction density. `--from-genesis` opts out. Snapshots are produced on the public node every 6 hours by `deploy/mersennet-snapshot.sh` (`PUBLISH=1`) and served by Caddy on port 8088 (plain HTTP by IP because Cloudflare caps proxied downloads at 100 MB). Upgrades never touch existing data.

## Publishing a new bundle

```bash
cargo build --release --bin mersennet
networks/testnet/package-release.sh          # -> dist/<bundle>.tar.gz, SHA256SUMS, install.sh, latest.json
# commit first (the bundle is named by the commit sha), then upload the tarball, SHA256SUMS and
# dist/install.sh and dist/latest.json to /var/www/downloads/ on the app host and repoint the -latest symlink.
# Never overwrite a published tarball: versioned files are cached as immutable.
```

## Network parameters

| Parameter | Value |
|-----------|-------|
| Chain ID | `131071` (0x1FFFF, the Mersenne prime 2^17 − 1) |
| Block time | ~2 s |
| P2P | UDP 30303 (gossip) + TCP 30303 (block sync) |
| Bootnodes | `46.225.30.187:30303`, `46.225.183.192:30303`, `49.13.54.79:30303` |
| Public RPC | https://rpc.mersennet.com (HTTPS) / wss://rpc.mersennet.com (WebSocket) |
| Explorer | https://explorer.mersennet.com |
| Faucet | https://faucet.mersennet.com |

A node key is generated automatically at `keys/node_key.json` on first
start. Inbound connectivity is optional for following the chain — a node
behind NAT still syncs via outbound TCP — but opening 30303/tcp+udp lets
other peers sync from you, which strengthens the network.
