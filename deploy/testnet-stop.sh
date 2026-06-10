#!/usr/bin/env bash
set -euo pipefail

# Stop all testnet nodes gracefully.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/nodes.conf"

VALIDATORS=("$VALIDATOR_1" "$VALIDATOR_2" "$VALIDATOR_3" "$VALIDATOR_4")
SSH_USER="${SSH_USER:-root}"

echo "Stopping all Mersennet testnet nodes..."

for i in 1 2 3 4; do
    IP="${VALIDATORS[$((i-1))]}"
    echo "  Stopping validator-$i ($IP)..."
    ssh -o StrictHostKeyChecking=no "$SSH_USER@$IP" "systemctl stop mersennet" 2>/dev/null &
done

echo "  Stopping public node ($PUBLIC_NODE)..."
ssh -o StrictHostKeyChecking=no "$SSH_USER@$PUBLIC_NODE" "\
    systemctl stop mersennet-faucet 2>/dev/null; \
    systemctl stop mersennet" 2>/dev/null &

wait
echo "All nodes stopped."
