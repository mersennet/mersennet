#!/usr/bin/env bash
# Privacy-invariant CI check (K2).
#
# Verifies that shielded data paths in `crates/core/src/shielded_*` and
# `crates/zkp/src/` never accidentally store an `Address`, an
# `owner: Address`-style field, a public balance map, or any token
# amount keyed by EOA. If any of these patterns appear in the
# shielded source tree, this script exits non-zero so CI fails.
#
# To extend the rule set, add a new check at the bottom in the same
# pattern. To suppress a known false positive on a specific line,
# prefix the line with `// privacy-allow: <reason>` and the grep below
# will skip it.
#
# Run locally:
#
#   bash scripts/ci/check-privacy-invariants.sh
#
# Exit codes:
#   0  no violations
#   1  one or more violations

set -uo pipefail

if command -v rg >/dev/null 2>&1; then
    RG_BIN=$(command -v rg)
elif command -v rg.exe >/dev/null 2>&1; then
    RG_BIN=$(command -v rg.exe)
else
    echo "Privacy-invariant CI check requires ripgrep ('rg') on PATH." >&2
    exit 1
fi

# Workspace root.
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"

SHIELDED_FILES=(
    "crates/core/src/shielded_evm.rs"
    "crates/core/src/shielded_orders.rs"
    "crates/core/src/shielded_state.rs"
    "crates/core/src/shielded_persistence.rs"
    "crates/core/src/liquidation_auction.rs"
    "crates/core/src/threshold_mempool.rs"
)

ZKP_SUBSYSTEM_DIR="crates/zkp/src"

VIOLATIONS=0
FAIL_LIST=()

# Shared grep helper — searches for `$pattern` in the given files,
# skipping lines tagged `privacy-allow:`. Echoes the description and
# matches to stderr and bumps VIOLATIONS.
check() {
    local desc="$1"
    local pattern="$2"
    shift 2
    local files=("$@")

    # Filter to only existing files so a renamed file doesn't break
    # CI before the rest of the diff lands.
    local existing=()
    for f in "${files[@]}"; do
        [[ -f "$f" ]] && existing+=("$f")
    done
    if [[ ${#existing[@]} -eq 0 ]]; then
        return 0
    fi

    local matches
    matches=$(
        "$RG_BIN" --line-number --no-heading "$pattern" "${existing[@]}" 2>/dev/null |
            "$RG_BIN" -v "privacy-allow:"
    ) || true

    if [[ -n "$matches" ]]; then
        VIOLATIONS=$((VIOLATIONS + 1))
        FAIL_LIST+=("$desc")
        echo "::error::Privacy-invariant violation: $desc" >&2
        echo "$matches" >&2
        echo "" >&2
    fi
}

# ────────────────────────────────────────────────────────────────────
# Rule 1. Shielded files must never store `HashMap<Address, ...>`.
# An account-keyed balance map is a textbook privacy leak.
# ────────────────────────────────────────────────────────────────────
check \
    "shielded modules cannot store HashMap<Address, ...> balances" \
    "HashMap<Address" \
    "${SHIELDED_FILES[@]}"

# ────────────────────────────────────────────────────────────────────
# Rule 2. Shielded structs must never expose an `owner: Address` field.
# Owners are encoded as Poseidon-hashed public keys (`owner_pk: Fr`),
# never as 20-byte EOAs.
# ────────────────────────────────────────────────────────────────────
check \
    "shielded structs cannot have an 'owner: Address' field" \
    "owner: Address" \
    "${SHIELDED_FILES[@]}"

# ────────────────────────────────────────────────────────────────────
# Rule 3. Shielded code cannot accept `from: Address`/`to: Address` —
# private transfers are commitment-to-commitment, not address-to-
# address.
# ────────────────────────────────────────────────────────────────────
check \
    "shielded code cannot carry plaintext 'from: Address'" \
    "from: Address" \
    "${SHIELDED_FILES[@]}"
check \
    "shielded code cannot carry plaintext 'to: Address'" \
    "to: Address" \
    "${SHIELDED_FILES[@]}"

# ────────────────────────────────────────────────────────────────────
# Rule 4. Public domain events emitted from shielded subsystems
# cannot reference an address by name. The legitimate per-validator
# `validator_index: u32` plus the per-claim `winner_bond_commitment`
# field are the only public identifiers permitted.
# ────────────────────────────────────────────────────────────────────
check \
    "ShieldedEvent variants cannot embed 'address: Address'" \
    "address: Address" \
    "${SHIELDED_FILES[@]}"

# ────────────────────────────────────────────────────────────────────
# Rule 5. `mersennet-zkp` is supposed to be address-free entirely — Fr
# field elements only. A bare `Address` mention in `crates/zkp/src/`
# is almost always a layering bug. (Allowlist via `privacy-allow:` if
# unavoidable.)
# ────────────────────────────────────────────────────────────────────
ZKP_FILES=$(find "$ZKP_SUBSYSTEM_DIR" -name "*.rs" 2>/dev/null || true)
if [[ -n "$ZKP_FILES" ]]; then
    while IFS= read -r f; do
        m=$(
            "$RG_BIN" --line-number "(\b|::)Address\b" "$f" 2>/dev/null |
                "$RG_BIN" -v "privacy-allow:"
        ) || true
        if [[ -n "$m" ]]; then
            VIOLATIONS=$((VIOLATIONS + 1))
            FAIL_LIST+=("mersennet-zkp module $f references Address")
            echo "::error::Privacy-invariant violation: mersennet-zkp module references Address (layering bug)" >&2
            echo "$m" >&2
            echo "" >&2
        fi
    done <<<"$ZKP_FILES"
fi

# ────────────────────────────────────────────────────────────────────
# Rule 6. Shielded code path cannot use `println!` / `dbg!` /
# `tracing::info!` with the literal "address=" or "from=" string —
# protect against accidental log leaks.
# ────────────────────────────────────────────────────────────────────
check \
    "shielded code cannot log address-tagged fields" \
    '("address=|from=|to=)' \
    "${SHIELDED_FILES[@]}"

# ────────────────────────────────────────────────────────────────────
# Summary
# ────────────────────────────────────────────────────────────────────
if [[ $VIOLATIONS -gt 0 ]]; then
    echo "" >&2
    echo "Privacy-invariant CI check FAILED with $VIOLATIONS rule violation(s):" >&2
    for desc in "${FAIL_LIST[@]}"; do
        echo "  - $desc" >&2
    done
    echo "" >&2
    echo "See docs/security/privacy-invariants.md for the canonical rule set." >&2
    exit 1
fi

echo "Privacy-invariant CI check passed (all $((${#SHIELDED_FILES[@]} + 1)) rule(s) ok)."
exit 0
