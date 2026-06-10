#!/usr/bin/env bash
set -euo pipefail

# Check the health of all testnet nodes.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/nodes.conf"

VALIDATORS=("$VALIDATOR_1" "$VALIDATOR_2" "$VALIDATOR_3" "$VALIDATOR_4")

echo "╔══════════════════════════════════════════════════╗"
echo "║          Mersennet Testnet Status              ║"
echo "╠══════════════════════════════════════════════════╣"

for i in 1 2 3 4; do
    IP="${VALIDATORS[$((i-1))]}"
    HEALTH=$(curl -sf --connect-timeout 3 "http://$IP:8545/health" 2>/dev/null || echo '{}')
    STATUS=$(echo "$HEALTH" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('status','DOWN'))" 2>/dev/null || echo "DOWN")
    HEIGHT=$(echo "$HEALTH" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('height','-'))" 2>/dev/null || echo "-")
    if [ "$STATUS" = "ok" ]; then
        echo "║  ● Validator $i ($IP)  height=$HEIGHT  ✓"
    else
        echo "║  ○ Validator $i ($IP)  DOWN"
    fi
done

PUB_HEALTH=$(curl -sf --connect-timeout 3 "http://$PUBLIC_NODE:8545/health" 2>/dev/null || echo '{}')
PUB_STATUS=$(echo "$PUB_HEALTH" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('status','DOWN'))" 2>/dev/null || echo "DOWN")
PUB_HEIGHT=$(echo "$PUB_HEALTH" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('height','-'))" 2>/dev/null || echo "-")
if [ "$PUB_STATUS" = "ok" ]; then
    echo "║  ● Public RPC ($PUBLIC_NODE)   height=$PUB_HEIGHT  ✓"
else
    echo "║  ○ Public RPC ($PUBLIC_NODE)   DOWN"
fi

FAUCET=$(curl -sf --connect-timeout 3 "http://$PUBLIC_NODE:8080/health" 2>/dev/null || echo '{}')
FAUCET_STATUS=$(echo "$FAUCET" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('status','DOWN'))" 2>/dev/null || echo "DOWN")
if [ "$FAUCET_STATUS" = "ok" ]; then
    echo "║  ● Faucet ($PUBLIC_NODE:8080)             ✓"
else
    echo "║  ○ Faucet ($PUBLIC_NODE:8080)             DOWN"
fi

echo "╠══════════════════════════════════════════════════╣"
echo "║  RPC:     http://$PUBLIC_NODE:8545"
echo "║  Faucet:  http://$PUBLIC_NODE:8080"
echo "║  Grafana: http://$PUBLIC_NODE:3000"
echo "║  Metrics: http://$PUBLIC_NODE:8545/metrics"
echo "╚══════════════════════════════════════════════════╝"
