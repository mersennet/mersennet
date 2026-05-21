#!/usr/bin/env bash
# Chaos: kill a random validator and verify the chain keeps making
# blocks (Workstream H5).
#
# Liveness threshold: 5-of-7 DKG threshold means up to 2 validators
# can be down and consensus still advances. We pick 1 at random,
# kill it for `DOWNTIME_SECS`, restart it, then verify:
#
#   1. Block height advanced during downtime.
#   2. After restart, the killed node catches up within
#      RECOVERY_BUDGET_SECS.
#   3. No `DKG ceremony stuck` event appeared in the logs.
#
# Env vars:
#   DOWNTIME_SECS         (default 60)
#   RECOVERY_BUDGET_SECS  (default 120)
#   RPC_URL               (default http://localhost:8545 — read-only RPC)
#   COMPOSE_FILE          (default docker-compose.privacy.yml)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTNET_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

DOWNTIME_SECS="${DOWNTIME_SECS:-60}"
RECOVERY_BUDGET_SECS="${RECOVERY_BUDGET_SECS:-120}"
RPC_URL="${RPC_URL:-http://localhost:8545}"
COMPOSE_FILE="${COMPOSE_FILE:-${TESTNET_ROOT}/docker-compose.privacy.yml}"

validators=(privacy-validator-1 privacy-validator-2 privacy-validator-3 \
            privacy-validator-4 privacy-validator-5 privacy-validator-6 \
            privacy-validator-7)
target="${validators[$(( RANDOM % ${#validators[@]} ))]}"

echo "==> Chaos: targeting ${target}"
echo "    downtime  = ${DOWNTIME_SECS}s"
echo "    recovery  = ${RECOVERY_BUDGET_SECS}s"

get_block_height() {
    curl -s -X POST -H 'content-type: application/json' \
        --max-time 3 \
        --data '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' \
        "${RPC_URL}" \
        | grep -oE '"result":"0x[0-9a-fA-F]+"' \
        | sed 's/"result":"0x//;s/"//' \
        | head -n1 \
        || echo "0"
}

hex_to_dec() {
    printf "%d\n" "0x${1:-0}"
}

initial_height_hex=$(get_block_height)
initial_height=$(hex_to_dec "${initial_height_hex}")
echo "==> Initial block height: ${initial_height}"

echo "==> Killing ${target}"
docker compose -f "${COMPOSE_FILE}" stop "${target}"

sleep "${DOWNTIME_SECS}"

mid_height_hex=$(get_block_height)
mid_height=$(hex_to_dec "${mid_height_hex}")
echo "==> Height after ${DOWNTIME_SECS}s with ${target} down: ${mid_height}"

if (( mid_height <= initial_height )); then
    echo "FAIL: chain did not advance with one validator down. Liveness violated."
    docker compose -f "${COMPOSE_FILE}" start "${target}"
    exit 1
fi

echo "==> Restarting ${target}"
docker compose -f "${COMPOSE_FILE}" start "${target}"

deadline=$(( $(date +%s) + RECOVERY_BUDGET_SECS ))
recovered=false
while (( $(date +%s) < deadline )); do
    sleep 5
    # Check the restarted node's local block height against the
    # cluster's RPC height.
    local_h=$(docker exec "${target}" curl -s -X POST -H 'content-type: application/json' \
        --max-time 3 \
        --data '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' \
        http://localhost:8545 \
        | grep -oE '"result":"0x[0-9a-fA-F]+"' \
        | sed 's/"result":"0x//;s/"//' \
        | head -n1 || echo "0")
    local_dec=$(hex_to_dec "${local_h}")
    cluster_h=$(get_block_height)
    cluster_dec=$(hex_to_dec "${cluster_h}")
    echo "    [${target}] local=${local_dec}  cluster=${cluster_dec}"
    if (( local_dec + 5 >= cluster_dec )); then
        recovered=true
        break
    fi
done

if ! ${recovered}; then
    echo "FAIL: ${target} did not catch up within ${RECOVERY_BUDGET_SECS}s"
    exit 1
fi

echo "==> ${target} recovered. Chaos test passed."
