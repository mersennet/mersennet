#!/usr/bin/env bash
set -euo pipefail

# Rolling upgrade — rebuilds locally, then updates each node one-by-one.
# Validators are upgraded with a brief pause to maintain consensus.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
source "$SCRIPT_DIR/nodes.conf"

VALIDATORS=("$VALIDATOR_1" "$VALIDATOR_2" "$VALIDATOR_3" "$VALIDATOR_4")
SSH_USER="${SSH_USER:-root}"

echo "==> Building new release..."
cd "$PROJECT_DIR"
cargo build --release -p mersennet-node 2>&1 | tail -3

BINARY="$PROJECT_DIR/target/release/mersennet"
FAUCET_BIN="$PROJECT_DIR/target/release/faucet"

echo "==> Starting rolling upgrade..."

for i in 1 2 3 4; do
    IP="${VALIDATORS[$((i-1))]}"
    echo "  Upgrading validator-$i ($IP)..."
    scp -o StrictHostKeyChecking=no -q "$BINARY" "$SSH_USER@$IP:/opt/mersennet/bin/mersennet.new"
    ssh -o StrictHostKeyChecking=no "$SSH_USER@$IP" "\
        systemctl stop mersennet && \
        mv /opt/mersennet/bin/mersennet.new /opt/mersennet/bin/mersennet && \
        chmod +x /opt/mersennet/bin/mersennet && \
        systemctl start mersennet"
    echo "  validator-$i upgraded. Waiting 10s for it to rejoin consensus..."
    sleep 10
done

echo "  Upgrading public node ($PUBLIC_NODE)..."
scp -o StrictHostKeyChecking=no -q "$BINARY" "$SSH_USER@$PUBLIC_NODE:/opt/mersennet/bin/mersennet.new"
scp -o StrictHostKeyChecking=no -q "$FAUCET_BIN" "$SSH_USER@$PUBLIC_NODE:/opt/mersennet/bin/faucet.new"
ssh -o StrictHostKeyChecking=no "$SSH_USER@$PUBLIC_NODE" "\
    systemctl stop mersennet-faucet && \
    systemctl stop mersennet && \
    mv /opt/mersennet/bin/mersennet.new /opt/mersennet/bin/mersennet && \
    mv /opt/mersennet/bin/faucet.new /opt/mersennet/bin/faucet && \
    chmod +x /opt/mersennet/bin/mersennet /opt/mersennet/bin/faucet && \
    systemctl start mersennet && \
    systemctl start mersennet-faucet"

echo
echo "==> Rolling upgrade complete!"
echo "    Check status: ./deploy/testnet-status.sh"
