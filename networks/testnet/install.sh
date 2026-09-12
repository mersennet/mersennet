#!/usr/bin/env bash
set -euo pipefail

# Mersennet testnet full-node installer.
#
#   sudo bash install.sh [--data-dir DIR] [--rpc-public]
#
#   --data-dir DIR   Put chain data and the node key under DIR instead of
#                    /var/lib/mersennet (e.g. a mounted block volume:
#                    --data-dir /mnt/blockstorage/mersennet). Existing data in
#                    the previous location is moved there.
#   --rpc-public     Listen for JSON-RPC on 0.0.0.0:8545 instead of localhost.
#
# Works from either layout:
#   1. The release bundle (https://mersennet.com/downloads/): binary, config
#      and unit file sit next to this script.
#   2. A source checkout after `cargo build --release --bin mersennet`:
#      run as `sudo bash networks/testnet/install.sh`.
#
# Installs the binary to /usr/local/bin, the canonical testnet config to
# /etc/mersennet, creates the mersennet system user and data directory,
# enables + starts the systemd service, and installs `mersennet-check`.
# Re-running upgrades the binary in place; data and keys are kept.

if [[ $EUID -ne 0 ]]; then
    echo "error: run with sudo (installs a system service)" >&2
    exit 1
fi

DATA_DIR="/var/lib/mersennet"
RPC_ADDR=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --data-dir) DATA_DIR="${2:?--data-dir needs a path}"; shift 2 ;;
        --data-dir=*) DATA_DIR="${1#*=}"; shift ;;
        --rpc-public) RPC_ADDR="0.0.0.0:8545"; shift ;;
        -h|--help) sed -n '3,22p' "$0"; exit 0 ;;
        *) echo "error: unknown option $1 (see --help)" >&2; exit 1 ;;
    esac
done
DATA_DIR="${DATA_DIR%/}"
case "$DATA_DIR" in
    /tmp|/tmp/*|/var/tmp|/var/tmp/*|/dev/shm|/dev/shm/*)
        echo "error: $DATA_DIR is temporary storage (wiped on reboot and hidden from the service by PrivateTmp)." >&2
        echo "       Use a persistent path such as /var/lib/mersennet or /mnt/<volume>/mersennet." >&2
        exit 1 ;;
    /*) ;;
    *) echo "error: --data-dir must be an absolute path" >&2; exit 1 ;;
esac

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -x "$HERE/mersennet" && -f "$HERE/config.json" ]]; then
    BIN="$HERE/mersennet"; CONF="$HERE/config.json"; UNIT="$HERE/mersennet.service"; CHECK="$HERE/mersennet-check"
else
    REPO_ROOT="$(cd "$HERE/../.." && pwd)"
    BIN="$REPO_ROOT/target/release/mersennet"
    CONF="$REPO_ROOT/networks/testnet/config.json"
    UNIT="$REPO_ROOT/networks/testnet/mersennet.service"
    CHECK="$REPO_ROOT/networks/testnet/mersennet-check"
fi
[[ -x "$BIN" ]] || { echo "error: $BIN not found — download the release bundle or run: cargo build --release --bin mersennet" >&2; exit 1; }
[[ -f "$CONF" ]] || { echo "error: $CONF not found" >&2; exit 1; }
[[ -f "$UNIT" ]] || { echo "error: $UNIT not found" >&2; exit 1; }
command -v systemctl >/dev/null || { echo "error: systemd is required (no systemctl found)" >&2; exit 1; }

UPGRADE=0
if systemctl is-active --quiet mersennet 2>/dev/null; then
    UPGRADE=1
    echo "==> Existing node detected: upgrading binary and restarting (data and keys are kept)"
    systemctl stop mersennet
fi

echo "==> Installing binary to /usr/local/bin/mersennet"
install -m 0755 "$BIN" /usr/local/bin/mersennet
if [[ -f "$CHECK" ]]; then
    install -m 0755 "$CHECK" /usr/local/bin/mersennet-check
fi

echo "==> Creating mersennet user and data directory $DATA_DIR"
id -u mersennet >/dev/null 2>&1 || useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin mersennet
# Moving an existing node to a new location (e.g. onto block storage).
PREV_DIR="$(systemctl show mersennet -p WorkingDirectory --value 2>/dev/null || true)"
if [[ -n "$PREV_DIR" && "$PREV_DIR" != "$DATA_DIR" && -d "$PREV_DIR/data" && ! -d "$DATA_DIR/data" ]]; then
    echo "    moving existing chain data from $PREV_DIR to $DATA_DIR"
    mkdir -p "$DATA_DIR"
    mv "$PREV_DIR/data" "$PREV_DIR/keys" "$DATA_DIR/" 2>/dev/null || true
fi
mkdir -p "$DATA_DIR/data" "$DATA_DIR/keys" /etc/mersennet
chown -R mersennet:mersennet "$DATA_DIR"

echo "==> Installing canonical testnet config to /etc/mersennet/config.json"
if [[ -f /etc/mersennet/config.json ]]; then
    echo "    /etc/mersennet/config.json already exists — leaving it untouched (canonical copy: $CONF)"
else
    install -m 0644 "$CONF" /etc/mersennet/config.json
fi
if [[ -n "$RPC_ADDR" ]]; then
    echo "==> Exposing JSON-RPC on $RPC_ADDR (was 127.0.0.1:8545)"
    sed -i "s#\"addr\": \"127.0.0.1:8545\"#\"addr\": \"$RPC_ADDR\"#" /etc/mersennet/config.json
fi

echo "==> Installing systemd service (data dir: $DATA_DIR)"
# The config's paths (data/state, keys/node_key.json, ...) are relative to the
# service WorkingDirectory, so pointing the unit at DATA_DIR is all it takes.
sed "s#/var/lib/mersennet#$DATA_DIR#g" "$UNIT" > /etc/systemd/system/mersennet.service
if [[ "$DATA_DIR" == /home/* || "$DATA_DIR" == /root/* ]]; then
    # ProtectHome=yes would hide the data dir from the service; ReadWritePaths
    # takes precedence over read-only, so relax it just enough.
    sed -i 's/^ProtectHome=yes/ProtectHome=read-only/' /etc/systemd/system/mersennet.service
fi
chmod 0644 /etc/systemd/system/mersennet.service
systemctl daemon-reload
systemctl enable mersennet >/dev/null 2>&1

echo "==> Opening firewall ports (P2P 30303 tcp+udp) if ufw is active"
if command -v ufw >/dev/null 2>&1 && ufw status | grep -q "Status: active"; then
    ufw allow 30303/udp comment "Mersennet P2P gossip" >/dev/null || true
    ufw allow 30303/tcp comment "Mersennet P2P sync" >/dev/null || true
fi

echo "==> Starting node"
systemctl restart mersennet
sleep 3
if ! systemctl is-active --quiet mersennet; then
    echo "error: the service did not stay up. Last log lines:" >&2
    journalctl -u mersennet -n 20 --no-pager >&2 || true
    exit 1
fi

cat <<EOF

Done. Your node is $([[ $UPGRADE -eq 1 ]] && echo "upgraded and " || true)running as a systemd service.

  mersennet-check                     # is it syncing? (height vs network, peers, disk)
  journalctl -u mersennet -f          # follow logs
  sudo systemctl restart mersennet    # restart

First start replays the chain from the bootnodes (typically 20-40 minutes
depending on hardware and network), then follows live blocks. Chain data and your node key
live in $DATA_DIR — back up $DATA_DIR/keys/node_key.json to keep your peer
identity. The RPC listens on ${RPC_ADDR:-127.0.0.1:8545}.

This is a full node: it verifies and serves the chain. The validator set is
fixed at genesis for now, so it will not appear in the validator list — see
https://docs.mersennet.com/validators/run-a-node/#becoming-a-validator
EOF
