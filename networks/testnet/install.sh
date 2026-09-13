#!/usr/bin/env bash
set -euo pipefail

# Mersennet testnet full-node installer.
#
#   sudo bash install.sh [--data-dir DIR] [--rpc-public] [--from-genesis] [--operator 0xWALLET]
#
#   --data-dir DIR   Put chain data and the node key under DIR instead of
#                    /var/lib/mersennet (e.g. a mounted block volume:
#                    --data-dir /mnt/blockstorage/mersennet). Existing data in
#                    the previous location is moved there.
#   --rpc-public     Listen for JSON-RPC on 0.0.0.0:8545 instead of localhost.
#   --operator ADDR  Your wallet address. The node signs it into its `whoami`
#                    attestation so you can claim the node as a verified node
#                    runner on trade.mersennet.com/points (daily points).
#   --from-genesis   Do not bootstrap a fresh node from the latest published
#                    state snapshot; replay the whole chain instead (many
#                    hours). Default: snapshot (SHA-256 verified), then the
#                    node syncs only the tail — minutes. Upgrades never touch
#                    existing data.
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
FROM_SNAPSHOT=1
OPERATOR=""
SNAPSHOT_MANIFEST="${MERSENNET_SNAPSHOT_MANIFEST:-http://46.225.30.187:8088/latest.json}"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --data-dir) DATA_DIR="${2:?--data-dir needs a path}"; shift 2 ;;
        --data-dir=*) DATA_DIR="${1#*=}"; shift ;;
        --rpc-public) RPC_ADDR="0.0.0.0:8545"; shift ;;
        --from-genesis) FROM_SNAPSHOT=0; shift ;;
        --operator) OPERATOR="${2:?--operator needs a 0x address}"; shift 2 ;;
        --operator=*) OPERATOR="${1#*=}"; shift ;;
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
    /home|/home/*|/root|/root/*)
        echo "error: $DATA_DIR is under a home directory, which the hardened service cannot access (ProtectHome)." >&2
        echo "       Use /var/lib/mersennet, /srv/mersennet or /mnt/<volume>/mersennet." >&2
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
if [[ -f /usr/local/bin/mersennet ]] && ! cmp -s "$BIN" /usr/local/bin/mersennet; then
    cp -a /usr/local/bin/mersennet /usr/local/bin/mersennet.prev   # rollback copy
fi
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
if [[ -n "$OPERATOR" ]]; then
    if [[ "$OPERATOR" =~ ^0x[0-9a-fA-F]{40}$ ]]; then
        echo "==> Recording operator wallet $OPERATOR (p2p.operator_address)"
        python3 - "$OPERATOR" <<'PY'
import json, sys
p = "/etc/mersennet/config.json"
cfg = json.load(open(p))
cfg.setdefault("p2p", {})["operator_address"] = sys.argv[1].lower()
json.dump(cfg, open(p, "w"), indent=2)
PY
    else
        echo "error: --operator must be a 0x address (40 hex chars)" >&2; exit 1
    fi
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

# Fresh node: start from the latest published state snapshot instead of
# replaying ~1.3M blocks (which takes most of a day at current tx density).
# The tarball's SHA-256 must match the manifest; the node then verifies
# every block it imports on top, and the watchdog-style fork detection
# would reject a state that disagrees with the network.
if [[ $FROM_SNAPSHOT -eq 1 && ! -d "$DATA_DIR/data/state" ]]; then
    echo "==> Bootstrapping chain state from the latest snapshot ($SNAPSHOT_MANIFEST)"
    SNAP_TMP="$(mktemp -d "$DATA_DIR/.snapshot.XXXXXX")"
    if curl -fsSL --max-time 20 -o "$SNAP_TMP/latest.json" "$SNAPSHOT_MANIFEST"; then
        SNAP_URL=$(sed -n 's/.*"url": *"\([^"]*\)".*/\1/p' "$SNAP_TMP/latest.json")
        SNAP_SHA=$(sed -n 's/.*"sha256": *"\([0-9a-f]*\)".*/\1/p' "$SNAP_TMP/latest.json")
        SNAP_HEIGHT=$(sed -n 's/.*"height": *\([0-9]*\).*/\1/p' "$SNAP_TMP/latest.json")
        SNAP_SIZE=$(sed -n 's/.*"size": *\([0-9]*\).*/\1/p' "$SNAP_TMP/latest.json")
        if [[ -n "$SNAP_URL" && -n "$SNAP_SHA" ]]; then
            echo "    snapshot at height ${SNAP_HEIGHT:-?}, $(( ${SNAP_SIZE:-0} / 1048576 )) MB — downloading"
            if curl -fL --retry 3 --progress-bar -o "$SNAP_TMP/state.tar.zst" "$SNAP_URL" \
               && echo "$SNAP_SHA  $SNAP_TMP/state.tar.zst" | sha256sum -c --quiet - ; then
                if ! command -v zstd >/dev/null 2>&1; then
                    echo "    installing zstd"; apt-get install -y -qq zstd >/dev/null 2>&1 || true
                fi
                if command -v zstd >/dev/null 2>&1; then
                    mkdir -p "$DATA_DIR/data/state"
                    if zstd -dc "$SNAP_TMP/state.tar.zst" | tar -xf - -C "$DATA_DIR/data/state"; then
                        echo "    state restored from snapshot (height ${SNAP_HEIGHT:-?}); the node will sync the remaining blocks"
                    else
                        echo "    extraction failed — falling back to a full sync" >&2
                        rm -rf "$DATA_DIR/data/state"
                    fi
                else
                    echo "    zstd unavailable — falling back to a full sync" >&2
                fi
            else
                echo "    download or checksum failed — falling back to a full sync" >&2
            fi
        else
            echo "    manifest unreadable — falling back to a full sync" >&2
        fi
    else
        echo "    snapshot server unreachable — falling back to a full sync" >&2
    fi
    rm -rf "$SNAP_TMP"
    chown -R mersennet:mersennet "$DATA_DIR" 2>/dev/null || true
fi

echo "==> Starting node"
systemctl restart mersennet
sleep 3
if ! systemctl is-active --quiet mersennet; then
    echo "error: the service did not stay up. Last log lines:" >&2
    journalctl -u mersennet -n 20 --no-pager >&2 || true
    if [[ -f /usr/local/bin/mersennet.prev ]]; then
        echo "To roll back to the previous binary:  sudo cp /usr/local/bin/mersennet.prev /usr/local/bin/mersennet && sudo systemctl restart mersennet" >&2
    fi
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
