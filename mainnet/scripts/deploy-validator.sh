#!/bin/bash
# Deploy a Mersennet validator node
# Usage: ./deploy-validator.sh --data-dir /var/lib/prime-chain --config genesis.json

set -e

DATA_DIR=""
CONFIG_PATH=""
USE_DOCKER=true
CONFIG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
    echo "Usage: $0 --data-dir DIR --config CONFIG [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --data-dir DIR     Data directory for state (default: /var/lib/prime-chain)"
    echo "  --config FILE      Path to config.json (default: mainnet/config/config.json)"
    echo "  --no-docker        Use systemd/native binary instead of Docker"
    echo "  -h, --help         Show this help"
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --data-dir)
            DATA_DIR="$2"
            shift 2
            ;;
        --config)
            CONFIG_PATH="$2"
            shift 2
            ;;
        --no-docker)
            USE_DOCKER=false
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

DATA_DIR="${DATA_DIR:-/var/lib/prime-chain}"
CONFIG_PATH="${CONFIG_PATH:-$CONFIG_DIR/config/config.json}"

# Check system requirements
check_requirements() {
    echo "==> Checking system requirements..."
    local errors=0

    if [[ $(nproc) -lt 8 ]]; then
        echo "  [WARN] CPU: Less than 8 cores (recommended: 16+)"
    fi

    local mem_gb=$(free -g | awk '/^Mem:/{print $2}')
    if [[ $mem_gb -lt 32 ]]; then
        echo "  [WARN] RAM: Less than 32GB (recommended: 64GB+)"
    fi

    if [[ "$USE_DOCKER" == true ]]; then
        if ! command -v docker &>/dev/null; then
            echo "  [ERROR] Docker not found. Install Docker or use --no-docker"
            errors=1
        fi
    fi

    if [[ ! -f "$CONFIG_PATH" ]]; then
        echo "  [ERROR] Config not found: $CONFIG_PATH"
        echo "  Copy config.json.example to config.json and customize."
        errors=1
    fi

    [[ $errors -eq 0 ]] || exit 1
    echo "  OK"
}

# Create data directories
setup_dirs() {
    echo "==> Creating data directories..."
    mkdir -p "$DATA_DIR"/{state,keys}
    chmod 700 "$DATA_DIR/keys" 2>/dev/null || true
    echo "  Created $DATA_DIR"
}

# Copy configuration
setup_config() {
    echo "==> Setting up configuration..."
    if [[ -f "$CONFIG_PATH" ]]; then
        cp "$CONFIG_PATH" "$DATA_DIR/config.json"
        echo "  Config copied to $DATA_DIR/config.json"
    fi
}

# Start via Docker
start_docker() {
    echo "==> Starting validator via Docker..."
    local project_root="$(cd "$CONFIG_DIR/../.." && pwd)"
    cd "$project_root"
    docker compose -f mainnet/docker-compose.mainnet.yml up -d validator
    echo "  Validator starting..."
}

# Start via systemd (placeholder - user would need to install the binary)
start_native() {
    echo "==> Native mode: ensure prime-chain binary is installed and run:"
    echo "  prime-chain --config $DATA_DIR/config.json --validator --rpc"
    echo ""
    echo "  Or create a systemd unit and: systemctl start prime-chain"
}

# Wait for sync
wait_for_sync() {
    echo "==> Waiting for node to respond (up to 60s)..."
    local rpc="${RPC_URL:-http://127.0.0.1:8545}"
    local i=0
    while [[ $i -lt 12 ]]; do
        if curl -sf "$rpc/health" >/dev/null 2>&1; then
            echo "  Node is healthy"
            return 0
        fi
        sleep 5
        ((i++)) || true
    done
    echo "  [WARN] Node did not respond in time. Check logs."
}

# Print status
print_status() {
    echo ""
    echo "==> Mersennet Validator Deployment Complete"
    echo ""
    echo "  Data dir:  $DATA_DIR"
    echo "  Config:    $CONFIG_PATH"
    echo "  RPC:       http://127.0.0.1:8545"
    echo ""
    echo "  Check status: ./health-check.sh --rpc-url http://127.0.0.1:8545"
    echo ""
}

# Main
check_requirements
setup_dirs
setup_config

if [[ "$USE_DOCKER" == true ]]; then
    start_docker
else
    start_native
fi

wait_for_sync
print_status
