#!/usr/bin/env bash
set -euo pipefail

# Stream logs from a testnet node.
# Usage: ./deploy/testnet-logs.sh [1|2|3|4|public]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/nodes.conf"

VALIDATORS=("$VALIDATOR_1" "$VALIDATOR_2" "$VALIDATOR_3" "$VALIDATOR_4")
SSH_USER="${SSH_USER:-root}"

NODE="${1:-1}"

case "$NODE" in
    1|2|3|4)
        IP="${VALIDATORS[$((NODE-1))]}"
        echo "==> Streaming logs from validator-$NODE ($IP)..."
        ssh "$SSH_USER@$IP" "journalctl -u prime-chain -f --no-hostname -n 50"
        ;;
    public|rpc|5)
        echo "==> Streaming logs from public node ($PUBLIC_NODE)..."
        ssh "$SSH_USER@$PUBLIC_NODE" "journalctl -u prime-chain -f --no-hostname -n 50"
        ;;
    faucet)
        echo "==> Streaming faucet logs from public node ($PUBLIC_NODE)..."
        ssh "$SSH_USER@$PUBLIC_NODE" "journalctl -u prime-chain-faucet -f --no-hostname -n 50"
        ;;
    *)
        echo "Usage: $0 [1|2|3|4|public|faucet]"
        exit 1
        ;;
esac
