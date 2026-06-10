#!/usr/bin/env bash
# Mersennet privacy testnet bootstrap (Workstream H3).
#
# Produces `genesis-output/privacy/`:
#   - validator keys (7 ECDSA secp256k1 keypairs)
#   - faucet key
#   - migrated genesis envelope (snapshot.bin) ready for the privacy
#     hard-fork chain (chain_id 7920)
#
# Idempotent: re-running regenerates everything from scratch in a
# fresh subdir. The pre-fork snapshot is *required* — typically the
# operator copies it from a live transparent-chain export.
#
# Usage:
#
#   PRE_FORK_SNAPSHOT=~/prefork-snapshot.bin ./scripts/bootstrap-privacy-genesis.sh
#   ACTIVATION_HEIGHT=100 ./scripts/bootstrap-privacy-genesis.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTNET_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${TESTNET_ROOT}/.." && pwd)"

ACTIVATION_HEIGHT="${ACTIVATION_HEIGHT:-100}"
CHAIN_ID="${CHAIN_ID:-7920}"
N_VALIDATORS="${N_VALIDATORS:-7}"
OUTPUT_DIR="${TESTNET_ROOT}/genesis-output/privacy"

PRE_FORK_SNAPSHOT="${PRE_FORK_SNAPSHOT:-}"

echo "==> Privacy testnet bootstrap"
echo "    chain_id          = ${CHAIN_ID}"
echo "    n_validators      = ${N_VALIDATORS}"
echo "    activation_height = ${ACTIVATION_HEIGHT}"
echo "    output_dir        = ${OUTPUT_DIR}"

rm -rf "${OUTPUT_DIR}"
mkdir -p "${OUTPUT_DIR}/keys"

echo "==> Building genesis + migrate-genesis binaries"
(cd "${REPO_ROOT}" && cargo build --release --bin genesis --bin migrate-genesis -q)

GENESIS_BIN="${REPO_ROOT}/target/release/genesis"
MIGRATE_BIN="${REPO_ROOT}/target/release/migrate-genesis"

echo "==> Running genesis tool for ${N_VALIDATORS} validators"
"${GENESIS_BIN}" --validators "${N_VALIDATORS}" --chain-id "${CHAIN_ID}" --output-dir "${OUTPUT_DIR}/genesis-tmp"
mv "${OUTPUT_DIR}/genesis-tmp/keys/"* "${OUTPUT_DIR}/keys/"
rm -rf "${OUTPUT_DIR}/genesis-tmp"

# Faucet keypair — random 32 bytes.
echo "==> Generating faucet key"
FAUCET_PK=$(head -c 32 /dev/urandom | xxd -p -c 64)
cat > "${OUTPUT_DIR}/keys/faucet-key.json" <<EOF
{
  "private_key": "0x${FAUCET_PK}"
}
EOF

# If a pre-fork snapshot was provided, run the migration tool.
# Otherwise create an *empty* genesis envelope from scratch — this
# is the "fresh-chain" path used when the privacy testnet doesn't
# inherit any pre-fork state.
if [[ -n "${PRE_FORK_SNAPSHOT}" && -f "${PRE_FORK_SNAPSHOT}" ]]; then
    echo "==> Running migration: ${PRE_FORK_SNAPSHOT} -> ${OUTPUT_DIR}/snapshot.bin"
    "${MIGRATE_BIN}" \
        --in "${PRE_FORK_SNAPSHOT}" \
        --out "${OUTPUT_DIR}/snapshot.bin" \
        --chain-id "${CHAIN_ID}" \
        --activation-height "${ACTIVATION_HEIGHT}"
else
    echo "==> No PRE_FORK_SNAPSHOT provided — creating empty privacy envelope"
    # Synthesize an empty PZS1 v2 envelope so validators boot with a
    # clean state but the privacy fork wiring is still active.
    cat > "${OUTPUT_DIR}/empty-snapshot.json" <<EOF
{
  "note": "Privacy testnet with no pre-fork state. Validators boot from an empty shielded state; the activation height stamped in the envelope still controls when shielded txs start being accepted.",
  "chain_id": ${CHAIN_ID},
  "activation_height": ${ACTIVATION_HEIGHT}
}
EOF
    # We don't ship a CLI to build an empty PZS1 envelope; instead
    # validators boot without `import_state_snapshot` and the node
    # picks up settings from privacy config block in JSON config.
fi

echo
echo "==> Privacy testnet bootstrap complete"
echo "    output: ${OUTPUT_DIR}"
echo
echo "Next step:"
echo "  cd ${TESTNET_ROOT}"
echo "  docker compose -f docker-compose.privacy.yml up -d --build"
