#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# Mersennet Testnet Deployment
# Deploys a 4-validator + 1 public-RPC testnet to Hetzner VPS instances.
#
# Usage:
#   1. Fill in deploy/nodes.conf with your 5 VPS IPs
#   2. Run: ./deploy/deploy-testnet.sh
#
# Prerequisites:
#   - SSH key access to all 5 VPS (ssh root@IP should work)
#   - Rust toolchain installed locally (for building)
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CONF_FILE="$SCRIPT_DIR/nodes.conf"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log()  { echo -e "${GREEN}[DEPLOY]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
err()  { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# 1. Load configuration
# ---------------------------------------------------------------------------
if [ ! -f "$CONF_FILE" ]; then
    err "Config file not found: $CONF_FILE\n  Copy deploy/nodes.conf.example and fill in your IPs."
fi

source "$CONF_FILE"

VALIDATORS=("$VALIDATOR_1" "$VALIDATOR_2" "$VALIDATOR_3" "$VALIDATOR_4")
SSH_USER="${SSH_USER:-root}"
CHAIN_ID="${CHAIN_ID:-131071}"
BLOCK_TIME_MS="${BLOCK_TIME_MS:-2000}"

for i in 1 2 3 4; do
    var="VALIDATOR_$i"
    if [ -z "${!var}" ]; then
        err "VALIDATOR_$i is not set in $CONF_FILE"
    fi
done
[ -z "$PUBLIC_NODE" ] && err "PUBLIC_NODE is not set in $CONF_FILE"

log "Configuration loaded:"
log "  Validators: ${VALIDATORS[*]}"
log "  Public:     $PUBLIC_NODE"
log "  Chain ID:   $CHAIN_ID"
log "  Block time: ${BLOCK_TIME_MS}ms"
echo

# ---------------------------------------------------------------------------
# 2. Build release binaries
# ---------------------------------------------------------------------------
log "Building release binaries..."
cd "$PROJECT_DIR"
cargo build --release -p mersennet-node 2>&1 | tail -3

BINARY="$PROJECT_DIR/target/release/mersennet"
FAUCET_BIN="$PROJECT_DIR/target/release/faucet"
GENESIS_BIN="$PROJECT_DIR/target/release/genesis"

[ -f "$BINARY" ]     || err "Binary not found: $BINARY"
[ -f "$FAUCET_BIN" ] || err "Faucet binary not found: $FAUCET_BIN"
[ -f "$GENESIS_BIN" ] || err "Genesis binary not found: $GENESIS_BIN"

log "Binaries built successfully ($(du -sh "$BINARY" | cut -f1))"
echo

# ---------------------------------------------------------------------------
# 3. Run genesis ceremony
# ---------------------------------------------------------------------------
GENESIS_DIR="$PROJECT_DIR/deploy/.genesis-output"
rm -rf "$GENESIS_DIR"

log "Running genesis ceremony (4 validators, chain_id=$CHAIN_ID)..."
"$GENESIS_BIN" --validators 4 --chain-id "$CHAIN_ID" --output-dir "$GENESIS_DIR"
echo

# ---------------------------------------------------------------------------
# 4. Patch configs with real IPs and settings
# ---------------------------------------------------------------------------
log "Patching configs with VPS IPs..."

ALL_NODES=("${VALIDATORS[@]}" "$PUBLIC_NODE")

for i in 1 2 3 4; do
    CONFIG="$GENESIS_DIR/configs/validator-${i}.json"
    IP="${VALIDATORS[$((i-1))]}"

    PEERS_JSON="["
    for j in 1 2 3 4; do
        [ "$j" -eq "$i" ] && continue
        PEER_IP="${VALIDATORS[$((j-1))]}"
        [ -n "$PEERS_JSON" ] && [ "$PEERS_JSON" != "[" ] && PEERS_JSON+=","
        PEERS_JSON+="\"${PEER_IP}:30303\""
    done
    PEERS_JSON+=",\"${PUBLIC_NODE}:30303\""
    PEERS_JSON+="]"

    python3 -c "
import json, sys
with open('$CONFIG') as f:
    c = json.load(f)
c['p2p']['listen'] = '0.0.0.0:30303'
c['p2p']['peers'] = json.loads('$PEERS_JSON')
c['p2p']['block_time_ms'] = $BLOCK_TIME_MS
c['p2p']['node_key_path'] = '/opt/mersennet/keys/node_key.json'
c['p2p']['peer_store_path'] = '/opt/mersennet/data/peers.json'
c['engine']['state_path'] = '/opt/mersennet/data/state'
c['rpc']['addr'] = '0.0.0.0:8545'
with open('$CONFIG', 'w') as f:
    json.dump(c, f, indent=2)
"
    log "  validator-$i ($IP): $(echo "$PEERS_JSON" | python3 -c "import sys,json; print(len(json.load(sys.stdin)),'peers')")"
done

# Public/RPC node config — same genesis, all validators as peers, no block production
RPC_CONFIG="$GENESIS_DIR/configs/rpc-node.json"
ALL_PEERS_JSON="["
for j in 1 2 3 4; do
    [ "$ALL_PEERS_JSON" != "[" ] && ALL_PEERS_JSON+=","
    ALL_PEERS_JSON+="\"${VALIDATORS[$((j-1))]}:30303\""
done
ALL_PEERS_JSON+="]"

python3 -c "
import json
with open('$RPC_CONFIG') as f:
    c = json.load(f)
c['p2p']['listen'] = '0.0.0.0:30303'
c['p2p']['peers'] = json.loads('$ALL_PEERS_JSON')
c['p2p']['block_time_ms'] = $BLOCK_TIME_MS
c['p2p']['node_key_path'] = '/opt/mersennet/keys/node_key.json'
c['p2p']['peer_store_path'] = '/opt/mersennet/data/peers.json'
c['engine']['state_path'] = '/opt/mersennet/data/state'
c['rpc']['addr'] = '0.0.0.0:8545'
c['ws'] = {'enabled': True, 'addr': '0.0.0.0:8546'}
with open('$RPC_CONFIG', 'w') as f:
    json.dump(c, f, indent=2)
"
log "  rpc-node ($PUBLIC_NODE): 4 peers"
echo

# Patch monitoring prometheus with real IPs
PROM_CFG="$SCRIPT_DIR/monitoring/prometheus.yml"
sed -i "s/{VALIDATOR_1}/${VALIDATORS[0]}/g" "$PROM_CFG"
sed -i "s/{VALIDATOR_2}/${VALIDATORS[1]}/g" "$PROM_CFG"
sed -i "s/{VALIDATOR_3}/${VALIDATORS[2]}/g" "$PROM_CFG"
sed -i "s/{VALIDATOR_4}/${VALIDATORS[3]}/g" "$PROM_CFG"

# ---------------------------------------------------------------------------
# 5. Deploy to validators
# ---------------------------------------------------------------------------
deploy_validator() {
    local idx=$1
    local ip=$2
    local ssh_target="$SSH_USER@$ip"

    log "Deploying validator-$idx to $ip..."

    ssh -o StrictHostKeyChecking=no "$ssh_target" "mkdir -p /tmp/mersennet-deploy"

    scp -o StrictHostKeyChecking=no -q \
        "$BINARY" \
        "$GENESIS_DIR/configs/validator-${idx}.json" \
        "$SCRIPT_DIR/setup-node.sh" \
        "$SCRIPT_DIR/mersennet-validator.service" \
        "$SCRIPT_DIR/firewall-validator.sh" \
        "$ssh_target:/tmp/mersennet-deploy/"

    # Also scp the key file separately (named differently from config)
    scp -o StrictHostKeyChecking=no -q \
        "$GENESIS_DIR/keys/validator-${idx}.json" \
        "$ssh_target:/tmp/mersennet-deploy/node_key.json"

    ssh -o StrictHostKeyChecking=no "$ssh_target" "\
        mv /tmp/mersennet-deploy/validator-${idx}.json /tmp/mersennet-deploy/config.json && \
        cp /tmp/mersennet-deploy/firewall-validator.sh /tmp/mersennet-deploy/firewall.sh && \
        chmod +x /tmp/mersennet-deploy/setup-node.sh && \
        bash /tmp/mersennet-deploy/setup-node.sh validator"

    log "  validator-$idx ($ip) deployed!"
}

for i in 1 2 3 4; do
    deploy_validator "$i" "${VALIDATORS[$((i-1))]}" &
done
wait
echo

# ---------------------------------------------------------------------------
# 6. Deploy public node
# ---------------------------------------------------------------------------
log "Deploying public node to $PUBLIC_NODE..."

PUBLIC_SSH="$SSH_USER@$PUBLIC_NODE"

ssh -o StrictHostKeyChecking=no "$PUBLIC_SSH" "mkdir -p /tmp/mersennet-deploy"

# Prepare Caddyfile with domains if set
CADDYFILE="$SCRIPT_DIR/Caddyfile"
if [ -n "${RPC_DOMAIN:-}" ]; then
    CADDY_TMP=$(mktemp)
    sed "s/{RPC_DOMAIN}/$RPC_DOMAIN/g; s/{FAUCET_DOMAIN}/${FAUCET_DOMAIN:-faucet.$RPC_DOMAIN}/g" "$CADDYFILE" > "$CADDY_TMP"
    CADDYFILE="$CADDY_TMP"
fi

scp -o StrictHostKeyChecking=no -q \
    "$BINARY" \
    "$FAUCET_BIN" \
    "$GENESIS_DIR/configs/rpc-node.json" \
    "$GENESIS_DIR/keys/faucet-key.json" \
    "$SCRIPT_DIR/setup-node.sh" \
    "$SCRIPT_DIR/mersennet-rpc.service" \
    "$SCRIPT_DIR/mersennet-faucet.service" \
    "$SCRIPT_DIR/firewall-public.sh" \
    "$CADDYFILE" \
    "$PUBLIC_SSH:/tmp/mersennet-deploy/"

# Copy monitoring stack
scp -o StrictHostKeyChecking=no -q -r \
    "$SCRIPT_DIR/monitoring/"* \
    "$PUBLIC_SSH:/tmp/mersennet-deploy/"

# Copy Grafana dashboards
scp -o StrictHostKeyChecking=no -q \
    "$PROJECT_DIR/monitoring/grafana/"*.json \
    "$PUBLIC_SSH:/tmp/mersennet-deploy/grafana/" 2>/dev/null || true

ssh -o StrictHostKeyChecking=no "$PUBLIC_SSH" "\
    mv /tmp/mersennet-deploy/rpc-node.json /tmp/mersennet-deploy/config.json && \
    cp /tmp/mersennet-deploy/firewall-public.sh /tmp/mersennet-deploy/firewall.sh && \
    export RPC_DOMAIN='${RPC_DOMAIN:-}' && \
    chmod +x /tmp/mersennet-deploy/setup-node.sh && \
    bash /tmp/mersennet-deploy/setup-node.sh public"

log "  public node ($PUBLIC_NODE) deployed!"
echo

# ---------------------------------------------------------------------------
# 7. Health checks
# ---------------------------------------------------------------------------
log "Waiting 10 seconds for nodes to start..."
sleep 10

echo
echo -e "${CYAN}╔══════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║          Mersennet Testnet Status              ║${NC}"
echo -e "${CYAN}╠══════════════════════════════════════════════════╣${NC}"

ALL_OK=true
for i in 1 2 3 4; do
    IP="${VALIDATORS[$((i-1))]}"
    HEALTH=$(curl -sf --connect-timeout 5 "http://$IP:8545/health" 2>/dev/null || echo '{"status":"DOWN"}')
    STATUS=$(echo "$HEALTH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','DOWN'))" 2>/dev/null || echo "DOWN")
    HEIGHT=$(echo "$HEALTH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('height','-'))" 2>/dev/null || echo "-")
    if [ "$STATUS" = "ok" ]; then
        echo -e "${CYAN}║${NC}  ${GREEN}●${NC} Validator $i ($IP)  height=$HEIGHT"
    else
        echo -e "${CYAN}║${NC}  ${RED}●${NC} Validator $i ($IP)  DOWN"
        ALL_OK=false
    fi
done

PUB_HEALTH=$(curl -sf --connect-timeout 5 "http://$PUBLIC_NODE:8545/health" 2>/dev/null || echo '{"status":"DOWN"}')
PUB_STATUS=$(echo "$PUB_HEALTH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','DOWN'))" 2>/dev/null || echo "DOWN")
PUB_HEIGHT=$(echo "$PUB_HEALTH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('height','-'))" 2>/dev/null || echo "-")
if [ "$PUB_STATUS" = "ok" ]; then
    echo -e "${CYAN}║${NC}  ${GREEN}●${NC} Public RPC ($PUBLIC_NODE)   height=$PUB_HEIGHT"
else
    echo -e "${CYAN}║${NC}  ${RED}●${NC} Public RPC ($PUBLIC_NODE)   DOWN"
    ALL_OK=false
fi

echo -e "${CYAN}╠══════════════════════════════════════════════════╣${NC}"
echo -e "${CYAN}║${NC}  RPC:     http://$PUBLIC_NODE:8545"
echo -e "${CYAN}║${NC}  Faucet:  http://$PUBLIC_NODE:8080"
echo -e "${CYAN}║${NC}  Grafana: http://$PUBLIC_NODE:3000  (admin/changeme by default)"
echo -e "${CYAN}║${NC}  Metrics: http://$PUBLIC_NODE:8545/metrics"
if [ -n "${RPC_DOMAIN:-}" ]; then
    echo -e "${CYAN}║${NC}  TLS RPC: https://$RPC_DOMAIN"
fi
echo -e "${CYAN}╚══════════════════════════════════════════════════╝${NC}"
echo

if $ALL_OK; then
    log "${GREEN}Testnet deployment complete! All nodes healthy.${NC}"
else
    warn "Some nodes are still starting. Check with: ssh $SSH_USER@<IP> journalctl -u mersennet -f"
fi

# Save deployment info
cat > "$SCRIPT_DIR/.deployment-info" <<EOF
DEPLOYED_AT=$(date -Iseconds)
CHAIN_ID=$CHAIN_ID
VALIDATORS=${VALIDATORS[*]}
PUBLIC_NODE=$PUBLIC_NODE
RPC_URL=http://$PUBLIC_NODE:8545
FAUCET_URL=http://$PUBLIC_NODE:8080
GRAFANA_URL=http://$PUBLIC_NODE:3000
EOF

log "Deployment info saved to deploy/.deployment-info"
