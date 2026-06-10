#!/usr/bin/env bash
set -euo pipefail

# Restart all testnet nodes (or a specific one).
# Usage: ./deploy/testnet-restart.sh [1|2|3|4|public|all]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/nodes.conf"

VALIDATORS=("$VALIDATOR_1" "$VALIDATOR_2" "$VALIDATOR_3" "$VALIDATOR_4")
SSH_USER="${SSH_USER:-root}"

TARGET="${1:-all}"

restart_validator() {
    local idx=$1
    local ip="${VALIDATORS[$((idx-1))]}"
    echo "  Restarting validator-$idx ($ip)..."
    ssh -o StrictHostKeyChecking=no "$SSH_USER@$ip" "systemctl restart mersennet"
}

restart_public() {
    echo "  Restarting public node ($PUBLIC_NODE)..."
    ssh -o StrictHostKeyChecking=no "$SSH_USER@$PUBLIC_NODE" "\
        systemctl restart mersennet && \
        systemctl restart mersennet-faucet"
}

case "$TARGET" in
    1|2|3|4)
        restart_validator "$TARGET"
        ;;
    public|rpc|5)
        restart_public
        ;;
    all)
        echo "Restarting all testnet nodes..."
        for i in 1 2 3 4; do
            restart_validator "$i" &
        done
        restart_public &
        wait
        ;;
    *)
        echo "Usage: $0 [1|2|3|4|public|all]"
        exit 1
        ;;
esac

echo "Done. Check status with: ./deploy/testnet-status.sh"
