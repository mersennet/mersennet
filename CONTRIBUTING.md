# Contributing to Prime Chain

Thanks for contributing. This document describes the branching model, commit
conventions, review policy, and the workflow for the in-progress
**ZK privacy** redesign.

---

## Branching model

Prime Chain uses a **trunk-based** model with one long-lived integration branch
for the active architecture redesign.

| Branch | Purpose | Stability |
|---|---|---|
| `main` | Production trunk. Tagged releases cut from here. | Always green. Protected. |
| `feat/zk-privacy` | Long-lived integration for the privacy / ZK redesign (Phases 1–7 of the architecture plan). | Green per phase. Protected. |
| `feat/*`, `fix/*`, `docs/*`, `chore/*` | Short-lived feature branches. One PR each. | Deleted on merge. |
| `archive/*` (tags, not branches) | Read-only snapshots of retired work. Recoverable forever. | Frozen. |

### Rules

1. Never push directly to `main` or `feat/zk-privacy`. Open a PR.
2. Squash-merge into `main`. Merge-commit into `feat/zk-privacy` is OK for
   preserving phase boundaries.
3. Delete the branch when the PR is merged.
4. Force-push only to your own short-lived branches, never to `main` or
   `feat/zk-privacy`.
5. Before deleting a branch with non-trivial history, tag it as
   `archive/<branch-name>` and push the tag.

### Recovering an archived branch

```bash
git fetch origin --tags
git checkout -b restored-from-archive archive/<original-branch-name>
```

---

## Commit messages

Conventional Commits. The type prefixes used in this repo:

- `feat:` — user-facing functionality
- `fix:` — bug fix
- `refactor:` — code change with no behavior change
- `perf:` — performance improvement
- `docs:` — documentation only
- `test:` — tests only
- `chore:` — tooling, deps, formatting
- `build:` — Cargo/npm/Foundry config changes
- `ci:` — GitHub Actions changes

A scope is encouraged: `feat(zkp): add Poseidon hasher`, `fix(rpc): ...`.

The body should explain the *why* in a few lines. Reference issues / PRs.
A trailing `Made-with: Cursor` line is fine when applicable.

---

## Pull request workflow

1. Branch from `main` (or `feat/zk-privacy` for ZK-redesign work).
2. Open a draft PR early. CI runs on every push.
3. Move to ready-for-review when CI is green and the PR description is
   complete (see `.github/PULL_REQUEST_TEMPLATE.md`).
4. One approving review from a CODEOWNER is required.
5. Squash-merge.

### CI gates

Run locally before opening a PR; all of these are also enforced by
`.github/workflows/ci.yml`:

```bash
cargo check --workspace                                   # fast type-check
cargo build --workspace                                   # default features
cargo test --workspace --lib --tests                      # ~5 min, 286+ tests
cargo test -p prime-zkp --features prover                 # real crypto path (43 tests)
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/ci/check-privacy-invariants.sh               # CI K2 grep, 7 rules
```

Additional jobs run in CI:

- **`sdk_typecheck`** — `tsc --noEmit` over `sdk/`
- **`contracts`** — `forge build --sizes && forge test -vvv` in `contracts/`
- **`audit`** — `cargo audit`
- Optional local Noir workflow via `scripts/zk/compile_noir_artifacts.py`
   plus the Barretenberg adapter smoke tests in `scripts/zk/README.md`

---

## ZK-privacy redesign workflow

The privacy redesign is structured as a series of gated phases and
parallel workstreams (A–K). The phase view tracks the original plan;
the workstream view tracks the current granular tasks. See
[`docs/STATUS.md`](docs/STATUS.md) for live status and
[`docs/internal/zk-privacy-plan.md`](docs/internal/zk-privacy-plan.md)
for the full plan.

### Phases

| Phase | Status |
|---|---|
| 0. Repo consolidation | **Done** |
| 1. ZK primitives + threshold mempool | **Done** |
| 2. Shielded CLOB | **Done** |
| 3. Liquidation auctions | **Done** |
| 4. Shielded EVM accounts | **Done** |
| 5. Real SP1 state proofs | In progress (E1-E3 complete; E4 delegated network proof and E5 bridge wrap remain) |
| 6. SDK / RPC / wallet | In progress (Rust RPC done, wallet/SDK tracked in F1–F6) |
| 7. Hard fork + testnet bake | **Testnet ready** (8-week bake gated on E completion) |

### Workstreams (granular)

| Code | Workstream | Status |
|---|---|---|
| A | State + snapshot | **Done** |
| B | Shielded subsystems | **Done** |
| C | RPC + WS | **Done** |
| D1–D4 | Crypto primitives + DKG | **Done** |
| D5 | Barretenberg verifier + prover adapters | **Done** |
| D6 | Noir circuit compilation | **Done** |
| D7 | Cryptography spec for auditor | **Done** ([here](docs/security/cryptography-spec.md)) |
| E1–E5 | SP1 toolchain + program body | E1-E3 complete; E4-E5 pending |
| F1–F6 | WASM Noir prover + UI + SDKs | Tracked separately |
| G1–G4 | Solidity bridge + Foundry tests | Tracked separately |
| H1–H7 | Testnet bring-up | **Done** ([runbook](docs/runbooks/privacy-testnet-bootstrap.md)) |
| I1–I6 | Third-party audit | Awaiting E |
| J1–J6 | Governance activation | Awaiting audit |
| K1–K5 | CI / coverage / dev container | **Done** |

At each phase boundary, `feat/zk-privacy` is merged into `main` and tagged
`zk-phase-<n>-rc.<x>`.

### Privacy ground rules (CI-enforced)

These are non-negotiable. The CI job `privacy_invariants` greps for
violations on every PR. See
[`docs/security/privacy-invariants.md`](docs/security/privacy-invariants.md)
for the full rule set.

- **No address-keyed state for trader balances or positions** anywhere
  inside `crates/core/src/shielded_*` or `crates/zkp/`.
- **No address fields in events** for shielded transactions or trades.
- **No plaintext addresses in metrics labels**. Use aggregates only.
- **No `panic!` in proof verification code paths**. Proof failures must
  return an `Err`, never crash the node.
- **All shielded state writes go through the nullifier set**. A nullifier
  that already exists in the set is a fatal validation error for the
  containing transaction.

If you need to cross the transparent/shielded boundary on purpose
(e.g. the migration tool, the shield/unshield bridge), add a
`// privacy-allow: <reason>` comment. The grep respects that.

### Privacy testnet quick reference

```bash
# Bring up a local 7-validator privacy testnet (chain 7920)
cd testnet
./scripts/bootstrap-privacy-genesis.sh
docker compose -f docker-compose.privacy.yml up -d --build

# Synthetic load
./scripts/privacy-load.sh

# Chaos drill — kill a random validator, verify liveness + recovery
./scripts/chaos-kill-validator.sh

# Generate a privacy-fork genesis from a pre-fork snapshot
cargo run --release --bin migrate-genesis -- \
    --in  ./prefork-snapshot.bin \
    --out ./postfork-snapshot.bin \
    --chain-id 7920 \
    --activation-height 100
```

Full runbook: [`docs/runbooks/privacy-testnet-bootstrap.md`](docs/runbooks/privacy-testnet-bootstrap.md).

---

## Local development

```bash
cargo build --workspace
cargo test --workspace
cargo run --bin prime-chain                  # devnet demo
cargo run --bin prime-chain -- --rpc         # devnet with JSON-RPC on 8545
```

Solidity contracts:

```bash
cd contracts && forge build && forge test
```

TypeScript SDK:

```bash
cd sdk && npm install && npx tsc --noEmit
```

Privacy CI grep (catches regressions before pushing):

```bash
bash scripts/ci/check-privacy-invariants.sh
```

---

## Reporting security issues

See [SECURITY.md](docs/SECURITY.md). Do not open public issues for
vulnerabilities.

---

## License

Proprietary — PrimeNumbers Labs. By contributing you agree your contribution
is licensed under the same terms.
