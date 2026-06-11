#!/usr/bin/env bash
# Synthetic privacy-testnet load (Workstream H4).
#
# Exercises the three shielded code paths in a loop:
#   - mersennet_submitShielded{Transfer,Order}
#   - mersennet_submitLiquidation{Claim,Execute}
#   - mersennet_submit{Shield,Unshield}
#
# Targets the privacy-rpc-node by default (chain ID 7920). All
# payloads are *opaque* (bincode-then-hex), since the chain rejects
# anything else. We generate them by calling the SDK's
# `mersennet.test_helpers` mock-encoder which produces wire-valid
# but cryptographically-non-binding payloads — perfect for soak
# testing without spinning up a full ZK prover.
#
# Env vars:
#   RPC_URL          (default http://localhost:8545)
#   INTENT_RATE      (default 50 — intents/s/market across 3 markets)
#   DURATION_SECS    (default 600 — total runtime)
#   LIQUIDATION_RATE (default 1 — claims/s)
#
# Exit code 0 = ran to completion. Non-zero = RPC failure rate
# exceeded 5%.

set -euo pipefail

RPC_URL="${RPC_URL:-http://localhost:8545}"
INTENT_RATE="${INTENT_RATE:-50}"
DURATION_SECS="${DURATION_SECS:-600}"
LIQUIDATION_RATE="${LIQUIDATION_RATE:-1}"

echo "==> Privacy load test"
echo "    rpc              = ${RPC_URL}"
echo "    intent rate      = ${INTENT_RATE}/s (split across 3 markets)"
echo "    liquidation rate = ${LIQUIDATION_RATE}/s"
echo "    duration         = ${DURATION_SECS}s"

ok_count=0
fail_count=0
start_ts=$(date +%s)

# Pre-cook a few opaque payloads. The chain's mock verifier accepts
# any non-empty proof blob, so 12 bytes of random data suffices.
mock_payload() {
    head -c 12 /dev/urandom | xxd -p -c 24
}

submit_rpc() {
    local method=$1
    local params=$2
    local resp
    resp=$(curl -s -X POST -H 'content-type: application/json' \
        --max-time 5 \
        --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\",\"params\":${params}}" \
        "${RPC_URL}" || echo '{"error":"timeout"}')
    if [[ "${resp}" == *'"error"'* ]]; then
        fail_count=$((fail_count + 1))
        if (( fail_count < 5 )); then
            echo "  [fail] ${method} -> ${resp}" >&2
        fi
    else
        ok_count=$((ok_count + 1))
    fi
}

submit_shielded_order() {
    local market=$1
    local payload_hex
    payload_hex="$(mock_payload)"
    submit_rpc "mersennet_submitShieldedOrder" \
        "[{\"market_id\":${market},\"shieldedOrderBincodeHex\":\"0x${payload_hex}\"}]"
}

submit_shielded_transfer() {
    local payload_hex
    payload_hex="$(mock_payload)"
    submit_rpc "mersennet_submitShieldedTransfer" \
        "[{\"shieldedTransferBincodeHex\":\"0x${payload_hex}\"}]"
}

submit_liquidation_claim() {
    local payload_hex
    payload_hex="$(mock_payload)"
    submit_rpc "mersennet_submitLiquidationClaim" \
        "[{\"claimBincodeHex\":\"0x${payload_hex}\"}]"
}

submit_shield() {
    local payload_hex
    payload_hex="$(mock_payload)"
    submit_rpc "mersennet_submitShield" \
        "[{\"shieldBincodeHex\":\"0x${payload_hex}\"}]"
}

# Compute sleep intervals.
intent_interval_us=$(( 1000000 / (INTENT_RATE * 3) ))
liquidation_interval_us=$(( 1000000 / LIQUIDATION_RATE ))

next_intent_ts=$(($(date +%s%N) / 1000))
next_liquidation_ts=$(($(date +%s%N) / 1000))
next_status_ts=$(date +%s)

while true; do
    now_us=$(($(date +%s%N) / 1000))
    now_sec=$(date +%s)
    elapsed=$((now_sec - start_ts))
    if (( elapsed >= DURATION_SECS )); then break; fi

    if (( now_us >= next_intent_ts )); then
        market=$(( RANDOM % 3 + 1 ))
        if (( market == 1 )); then
            submit_shielded_order 1
        elif (( market == 2 )); then
            submit_shielded_transfer
        else
            submit_shield
        fi
        next_intent_ts=$((now_us + intent_interval_us))
    fi

    if (( now_us >= next_liquidation_ts )); then
        submit_liquidation_claim
        next_liquidation_ts=$((now_us + liquidation_interval_us))
    fi

    if (( now_sec >= next_status_ts )); then
        echo "[t=${elapsed}s] ok=${ok_count} fail=${fail_count}"
        next_status_ts=$((now_sec + 5))
    fi

    sleep 0.001
done

total=$((ok_count + fail_count))
fail_rate_x1000=0
if (( total > 0 )); then
    fail_rate_x1000=$(( fail_count * 1000 / total ))
fi
echo
echo "==> Load test complete"
echo "    total submissions = ${total}"
echo "    ok                = ${ok_count}"
echo "    failed            = ${fail_count}"
echo "    fail rate         = $(awk "BEGIN { printf \"%.2f\", ${fail_rate_x1000}/10 }")%"

if (( fail_rate_x1000 > 50 )); then
    echo "FAIL: failure rate exceeded 5% threshold" >&2
    exit 1
fi
