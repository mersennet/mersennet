#!/usr/bin/env bash
set -euo pipefail

RPC_URL="${RPC_URL:-http://localhost:8545}"

rpc_call() {
    local method="$1"
    local params="$2"
    curl -sf -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":$params}"
}

echo "==> Funding test accounts on $RPC_URL"
echo ""

TEST_ACCOUNTS=(
    "0x000000000000000000000000000000000000dEaD"
    "0x0000000000000000000000000000000000000042"
    "0x0000000000000000000000000000000000C0FFEE"
)

FUND_AMOUNT="1000000000000000000000"

for account in "${TEST_ACCOUNTS[@]}"; do
    echo "Funding $account with $FUND_AMOUNT wei..."
    result=$(rpc_call "eth_sendTransaction" "[{
        \"from\": \"0x0000000000000000000000000000000000000001\",
        \"to\": \"$account\",
        \"value\": \"0x$(printf '%x' "$FUND_AMOUNT" 2>/dev/null || echo "3635c9adc5dea00000")\"
    }]") || {
        echo "  WARNING: Failed to fund $account (node may not support eth_sendTransaction yet)"
        continue
    }
    echo "  tx: $result"
done

echo ""
echo "==> Creating test market on MersennetOrders..."

result=$(rpc_call "mersennet_createMarket" "[{
    \"base_asset\": \"MRSN\",
    \"quote_asset\": \"USDC\",
    \"tick_size\": \"1000000000000000\",
    \"min_order_size\": \"100000000000000000\"
}]") || {
    echo "  WARNING: Failed to create market (mersennet_createMarket may not be available yet)"
    exit 0
}
echo "  market: $result"

echo ""
echo "==> Done. Test accounts funded and market created."
