#!/usr/bin/env bash
set -euo pipefail

# Mersennet testnet full-node installer.
#
# Run from the repository root after building the binary:
#   cargo build --release --bin mersennet
#   sudo bash networks/testnet/install.sh
#
# Installs the binary to /usr/local/bin, the canonical testnet config to
# /etc/mersennet, creates the mersennet system user and data directory,
# and enables a systemd service.

if [[ $EUID -ne 0 ]]; then
    echo "error: run with sudo (installs a system service)" >&2
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$REPO_ROOT/target/release/mersennet"
CONF="$REPO_ROOT/networks/testnet/config.json"
UNIT="$REPO_ROOT/networks/testnet/mersennet.service"

[[ -x "$BIN" ]] || { echo "error: $BIN not found — run: cargo build --release --bin mersennet" >&2; exit 1; }
[[ -f "$CONF" ]] || { echo "error: $CONF not found" >&2; exit 1; }

echo "==> Installing binary to /usr/local/bin/mersennet"
install -m 0755 "$BIN" /usr/local/bin/mersennet

echo "==> Creating mersennet user and directories"
id -u mersennet >/dev/null 2>&1 || useradd --system --home-dir /var/lib/mersennet --shell /usr/sbin/nologin mersennet
mkdir -p /var/lib/mersennet/{data,keys}
mkdir -p /etc/mersennet

echo "==> Installing canonical testnet config to /etc/mersennet/config.json"
if [[ -f /etc/mersennet/config.json ]]; then
    echo "    /etc/mersennet/config.json already exists — leaving it untouched."
    echo "    (Canonical copy: $CONF)"
else
    install -m 0644 "$CONF" /etc/mersennet/config.json
fi
chown -R mersennet:mersennet /var/lib/mersennet

echo "==> Installing systemd service"
install -m 0644 "$UNIT" /etc/systemd/system/mersennet.service
systemctl daemon-reload
systemctl enable mersennet

echo "==> Opening firewall ports (P2P 30303 tcp+udp) if ufw is active"
if command -v ufw >/dev/null 2>&1 && ufw status | grep -q "Status: active"; then
    ufw allow 30303/udp comment "Mersennet P2P gossip" || true
    ufw allow 30303/tcp comment "Mersennet P2P sync" || true
fi

echo "==> Starting node"
systemctl restart mersennet

cat <<'EOF'

Done. Useful commands:
  journalctl -u mersennet -f          # follow logs
  curl -s http://127.0.0.1:8545 -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'

The node first replays chain history from the bootnodes (several hundred
blocks/s; expect ~20-40 minutes depending on chain length and hardware),
then follows live gossip. The RPC listens on 127.0.0.1:8545 by default —
edit /etc/mersennet/config.json to expose it and restart.
EOF
