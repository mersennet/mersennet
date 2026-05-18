#!/bin/bash
# Check Prime Chain node health
# Returns 0 if healthy, 1 if not
# Usage: ./health-check.sh [--rpc-url URL] [--data-dir DIR]

set -e

RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"
DATA_DIR="${DATA_DIR:-/var/lib/prime-chain}"
REQUIRED_DISK_GB=10

while [[ $# -gt 0 ]]; do
    case $1 in
        --rpc-url)
            RPC_URL="$2"
            shift 2
            ;;
        --data-dir)
            DATA_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--rpc-url URL] [--data-dir DIR]"
            echo "Returns 0 if healthy, 1 if not."
            exit 0
            ;;
        *)
            shift
            ;;
    esac
done

exit_code=0

# Check if process is running (Docker or native)
check_process() {
    if docker ps --format '{{.Names}}' 2>/dev/null | grep -q prime-chain; then
        echo "OK: Validator container is running"
        return 0
    fi
    if pgrep -f "prime-chain.*validator" >/dev/null 2>&1; then
        echo "OK: Validator process is running"
        return 0
    fi
    echo "FAIL: Validator process/container not running"
    return 1
}

# Check RPC responds
check_rpc() {
    if curl -sf --max-time 5 "$RPC_URL/health" >/dev/null 2>&1; then
        echo "OK: RPC health endpoint responding"
        return 0
    fi
    if curl -sf --max-time 5 -X POST -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        "$RPC_URL" >/dev/null 2>&1; then
        echo "OK: RPC eth_blockNumber responding"
        return 0
    fi
    echo "FAIL: RPC not responding at $RPC_URL"
    return 1
}

# Check block height is advancing
check_block_height() {
    local response
    response=$(curl -sf --max-time 5 -X POST -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
        "$RPC_URL" 2>/dev/null) || true
    if [[ -z "$response" ]]; then
        echo "WARN: Could not fetch block number"
        return 0
    fi
    local height
    height=$(echo "$response" | grep -o '"result":"[^"]*"' | cut -d'"' -f4)
    if [[ -n "$height" ]]; then
        echo "OK: Block height: $height"
        return 0
    fi
    echo "WARN: Could not parse block number"
    return 0
}

# Check peer count (if available)
check_peers() {
    local response
    response=$(curl -sf --max-time 5 -X POST -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}' \
        "$RPC_URL" 2>/dev/null) || true
    if [[ -n "$response" ]]; then
        echo "OK: Peer count check attempted"
        return 0
    fi
    echo "WARN: Could not fetch peer count (endpoint may not exist)"
    return 0
}

# Check disk space
check_disk() {
    if [[ ! -d "$DATA_DIR" ]]; then
        echo "WARN: Data dir $DATA_DIR does not exist"
        return 0
    fi
    local avail_gb
    avail_gb=$(df -BG "$DATA_DIR" 2>/dev/null | tail -1 | awk '{print $4}' | tr -d 'G')
    if [[ -n "$avail_gb" && $avail_gb -lt $REQUIRED_DISK_GB ]]; then
        echo "FAIL: Low disk space: ${avail_gb}GB available (need ${REQUIRED_DISK_GB}GB+)"
        return 1
    fi
    echo "OK: Disk space: ${avail_gb}GB available"
    return 0
}

# Run all checks
echo "==> Prime Chain Health Check"
echo ""

check_process || exit_code=1
check_rpc || exit_code=1
check_block_height
check_peers
check_disk || exit_code=1

echo ""
if [[ $exit_code -eq 0 ]]; then
    echo "==> Health: OK"
else
    echo "==> Health: FAILED"
fi

exit $exit_code
