#!/usr/bin/env bash
set -euo pipefail

# Rolling upgrade that ships the `--features prover` binary plus the
# Barretenberg verification toolchain (bb + compiled circuit artifacts
# + verify adapter) and wires the real verifier into systemd.
#
# Safe to re-run: every step is idempotent. Validators are upgraded one
# at a time with a health check (block height must advance) before
# moving on, then the public/RPC node last.
#
# Pre-req: the prover release binary must already be built:
#   cargo build --release -p mersennet-node --features prover

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
source "$SCRIPT_DIR/nodes.conf"

SSH_USER="${SSH_USER:-root}"
SSH_OPTS=(-o StrictHostKeyChecking=no -o ConnectTimeout=10)

BINARY="$PROJECT_DIR/target/release/mersennet"
FAUCET_BIN="$PROJECT_DIR/target/release/faucet"
BB_BIN="${BB_BIN:-$HOME/.bb/bb}"
ARTIFACTS_DIR="$PROJECT_DIR/crates/zkp/params/noir"
ADAPTER="$PROJECT_DIR/scripts/zk/barretenberg_verify_adapter.py"
ADAPTER_COMMON="$PROJECT_DIR/scripts/zk/common.py"
SELFTEST_DIR="$SCRIPT_DIR/zk-selftest"

for f in "$BINARY" "$BB_BIN" "$ADAPTER" "$ADAPTER_COMMON"; do
    [ -f "$f" ] || { echo "FATAL: missing $f" >&2; exit 1; }
done
[ -d "$ARTIFACTS_DIR/spend" ] || { echo "FATAL: artifacts not built at $ARTIFACTS_DIR" >&2; exit 1; }

height() {
    local ip="$1"
    timeout 15 ssh "${SSH_OPTS[@]}" "$SSH_USER@$ip" \
        "curl -s --max-time 8 http://127.0.0.1:8545 -X POST -H 'content-type: application/json' \
         --data '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_blockNumber\",\"params\":[]}'" 2>/dev/null \
        | sed -n 's/.*\"result\":\"0x\([0-9a-f]*\)\".*/\1/p'
}

stage_prover_assets() {
    local ip="$1"
    echo "    staging bb + artifacts + adapter..."
    ssh "${SSH_OPTS[@]}" "$SSH_USER@$ip" \
        "mkdir -p /opt/mersennet/zk/scripts /opt/mersennet/zk/artifacts /opt/mersennet/zk/selftest /etc/systemd/system/mersennet.service.d"

    scp "${SSH_OPTS[@]}" -q "$BB_BIN" "$SSH_USER@$ip:/opt/mersennet/bin/bb.new"
    scp "${SSH_OPTS[@]}" -q "$ADAPTER" "$ADAPTER_COMMON" "$SSH_USER@$ip:/opt/mersennet/zk/scripts/"
    scp "${SSH_OPTS[@]}" -rq "$ARTIFACTS_DIR/." "$SSH_USER@$ip:/opt/mersennet/zk/artifacts/"
    scp "${SSH_OPTS[@]}" -q "$SELFTEST_DIR/output_proof" "$SELFTEST_DIR/output_public_inputs" \
        "$SSH_USER@$ip:/opt/mersennet/zk/selftest/"

    ssh "${SSH_OPTS[@]}" "$SSH_USER@$ip" "bash -s" <<'REMOTE'
set -e
mv /opt/mersennet/bin/bb.new /opt/mersennet/bin/bb
chmod +x /opt/mersennet/bin/bb

cat > /opt/mersennet/bin/bb-verify-adapter <<'WRAP'
#!/usr/bin/env bash
export MERSENNET_BB_BIN=/opt/mersennet/bin/bb
exec python3 /opt/mersennet/zk/scripts/barretenberg_verify_adapter.py "$@"
WRAP
chmod +x /opt/mersennet/bin/bb-verify-adapter

cat > /etc/systemd/system/mersennet.service.d/prover.conf <<'DROPIN'
[Service]
Environment=MERSENNET_NOIR_ARTIFACTS_DIR=/opt/mersennet/zk/artifacts
Environment=MERSENNET_BB_VERIFY_ADAPTER=/opt/mersennet/bin/bb-verify-adapter
Environment=MERSENNET_BB_BIN=/opt/mersennet/bin/bb
DROPIN

chown -R mersennet:mersennet /opt/mersennet/zk /opt/mersennet/bin/bb /opt/mersennet/bin/bb-verify-adapter
REMOTE
}

selftest_node() {
    local ip="$1"
    echo "    running on-node verify-adapter self-test (real path)..."
    # Exercise the exact adapter the node invokes, with the same
    # big-endian hex-text public-inputs format BarretenbergVerifier
    # writes. Positive must pass; a tampered input must fail.
    local pos neg
    pos=$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$ip" \
        "/opt/mersennet/bin/bb-verify-adapter \
            --circuit output \
            --artifacts /opt/mersennet/zk/artifacts/output \
            --proof /opt/mersennet/zk/selftest/output_proof \
            --public-inputs /opt/mersennet/zk/selftest/output_public_inputs \
         >/dev/null 2>&1 && echo OK || echo FAIL")
    neg=$(ssh "${SSH_OPTS[@]}" "$SSH_USER@$ip" \
        "sed '1s/^1/2/' /opt/mersennet/zk/selftest/output_public_inputs > /tmp/bad_pi && \
         /opt/mersennet/bin/bb-verify-adapter \
            --circuit output \
            --artifacts /opt/mersennet/zk/artifacts/output \
            --proof /opt/mersennet/zk/selftest/output_proof \
            --public-inputs /tmp/bad_pi \
         >/dev/null 2>&1 && echo ACCEPTED || echo REJECTED; rm -f /tmp/bad_pi")
    if [ "$pos" = "OK" ] && [ "$neg" = "REJECTED" ]; then
        echo "    ✓ on-node verifier OK (valid accepted, tampered rejected)"
    else
        echo "    ✗ on-node verifier self-test FAILED (pos=$pos neg=$neg)" >&2
        exit 1
    fi
}

swap_binary_and_restart() {
    local ip="$1"; local role="$2"
    scp "${SSH_OPTS[@]}" -q "$BINARY" "$SSH_USER@$ip:/opt/mersennet/bin/mersennet.new"
    if [ "$role" = "public" ]; then
        scp "${SSH_OPTS[@]}" -q "$FAUCET_BIN" "$SSH_USER@$ip:/opt/mersennet/bin/faucet.new" 2>/dev/null || true
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$ip" "\
            systemctl stop mersennet-faucet 2>/dev/null || true; \
            systemctl stop mersennet; \
            mv /opt/mersennet/bin/mersennet.new /opt/mersennet/bin/mersennet; \
            [ -f /opt/mersennet/bin/faucet.new ] && mv /opt/mersennet/bin/faucet.new /opt/mersennet/bin/faucet || true; \
            chmod +x /opt/mersennet/bin/mersennet /opt/mersennet/bin/faucet 2>/dev/null || true; \
            chown mersennet:mersennet /opt/mersennet/bin/mersennet; \
            systemctl daemon-reload; \
            systemctl start mersennet; \
            systemctl start mersennet-faucet 2>/dev/null || true"
    else
        ssh "${SSH_OPTS[@]}" "$SSH_USER@$ip" "\
            systemctl stop mersennet; \
            mv /opt/mersennet/bin/mersennet.new /opt/mersennet/bin/mersennet; \
            chmod +x /opt/mersennet/bin/mersennet; \
            chown mersennet:mersennet /opt/mersennet/bin/mersennet; \
            systemctl daemon-reload; \
            systemctl start mersennet"
    fi
}

wait_for_progress() {
    local ip="$1"
    local before="$2"
    local before_dec=$((16#${before:-0}))
    for _ in $(seq 1 20); do
        sleep 3
        local now; now=$(height "$ip")
        if [ -n "$now" ]; then
            local now_dec=$((16#$now))
            if [ "$now_dec" -gt "$before_dec" ]; then
                echo "    ✓ height advancing: $before_dec -> $now_dec"
                return 0
            fi
        fi
    done
    echo "    ✗ height did not advance within 60s (was $before_dec)" >&2
    return 1
}

upgrade_one() {
    local ip="$1"; local role="$2"; local label="$3"
    echo "==> Upgrading $label ($ip) [role=$role]"
    local h0; h0=$(height "$ip"); echo "    pre-upgrade height: $((16#${h0:-0}))"
    stage_prover_assets "$ip"
    selftest_node "$ip"
    swap_binary_and_restart "$ip" "$role"
    wait_for_progress "$ip" "$h0"
    echo "    $label done."
    echo
}

echo "==> Prover rolling upgrade starting (binary: $BINARY)"
echo

upgrade_one "$VALIDATOR_1" validator "validator-1"
upgrade_one "$VALIDATOR_2" validator "validator-2"
upgrade_one "$VALIDATOR_3" validator "validator-3"
upgrade_one "$VALIDATOR_4" validator "validator-4"
upgrade_one "$PUBLIC_NODE" public "public-node"

echo "==> Prover rolling upgrade complete."
echo "    Verify env active on a node with:"
echo "    ssh $SSH_USER@$PUBLIC_NODE systemctl show mersennet -p Environment"
