#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$PROJECT_ROOT"

echo "==> Building and starting 3-validator testnet..."
docker compose up -d --build

echo "==> Waiting for validators to become healthy..."

VALIDATORS=("mersennet-validator-1" "mersennet-validator-2" "mersennet-validator-3")
MAX_RETRIES=30
RETRY_INTERVAL=5

for validator in "${VALIDATORS[@]}"; do
    retries=0
    while true; do
        status=$(docker inspect --format='{{.State.Health.Status}}' "$validator" 2>/dev/null || echo "not_found")
        if [ "$status" = "healthy" ]; then
            echo "    $validator is healthy"
            break
        fi
        retries=$((retries + 1))
        if [ "$retries" -ge "$MAX_RETRIES" ]; then
            echo "ERROR: $validator did not become healthy after $((MAX_RETRIES * RETRY_INTERVAL))s"
            echo "       Last status: $status"
            docker compose logs "$validator" | tail -20
            exit 1
        fi
        sleep "$RETRY_INTERVAL"
    done
done

echo ""
echo "==> Testnet is running!"
echo ""
echo "RPC Endpoints:"
echo "  validator-1: http://localhost:8545"
echo "  validator-2: http://localhost:8546"
echo "  validator-3: http://localhost:8547"
echo ""
echo "P2P Ports:"
echo "  validator-1: 9090"
echo "  validator-2: 9091"
echo "  validator-3: 9092"
echo ""
echo "Metrics:"
echo "  validator-1: http://localhost:9100"
echo "  validator-2: http://localhost:9101"
echo "  validator-3: http://localhost:9102"
echo ""
echo "To stop: docker compose down"
echo "To stop and wipe data: docker compose down -v"
