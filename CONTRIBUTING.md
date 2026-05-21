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

- `cargo build --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace -- -D warnings`
- `cargo fmt --check`
- (When circuits land) `nargo test` for Noir circuits
- (When ZK is real) `cargo test -p zkp --features prover` for proof
  round-trips against pinned KZG SRS.

---

## ZK-privacy redesign workflow

The privacy redesign is structured as a series of gated phases. See
`docs/internal/zk-privacy-plan.md` (added in Phase 0) for the full plan.

| Phase | Branch | Status |
|---|---|---|
| 0. Repo consolidation | `feat/zk-privacy` (initial) | In progress |
| 1. ZK primitives + threshold mempool | `feat/zk-privacy` | In progress |
| 2. Shielded CLOB | `feat/zk-privacy` | Gated |
| 3. Liquidation auctions | `feat/zk-privacy` | Gated |
| 4. Shielded EVM accounts | `feat/zk-privacy` | Gated |
| 5. Real SP1 state proofs | `feat/zk-privacy` | Gated |
| 6. SDK / RPC / wallet | `feat/zk-privacy` | Gated |
| 7. Hard fork + testnet bake | `release/zk-fork` | Gated |

At each phase boundary, `feat/zk-privacy` is merged into `main` and tagged
`zk-phase-<n>-rc.<x>`.

### Privacy ground rules

- **No address-keyed state for trader balances or positions** anywhere in
  `crates/core/src/prime_orders.rs` or its successor modules.
- **No address fields in events** for shielded transactions or trades.
- **No plaintext addresses in metrics labels**. Use aggregates only.
- **No `panic!` in proof verification code paths**. Proof failures must
  return an `Err`, never crash the node.
- **All shielded state writes go through the nullifier set**. A nullifier
  that already exists in the set is a fatal validation error for the
  containing transaction.

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

---

## Reporting security issues

See [SECURITY.md](docs/SECURITY.md). Do not open public issues for
vulnerabilities.

---

## License

Proprietary — PrimeNumbers Labs. By contributing you agree your contribution
is licensed under the same terms.
