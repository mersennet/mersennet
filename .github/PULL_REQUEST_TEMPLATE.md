<!--
Thanks for the PR! Please fill out every section. Delete any that don't apply,
but don't delete the headers — empty sections make review harder.
-->

## Summary

<!-- One or two sentences. What does this PR do and why? -->

## Scope

- [ ] Code (`crates/`)
- [ ] Docs (`docs/`, `docs-site/`)
- [ ] Infra / CI (`.github/`, `deploy/`, `docker-compose.yml`)
- [ ] ZK / privacy (anything under `crates/zkp/`, `crates/core/src/shielded_state.rs`, or touching privacy invariants)

## ZK / privacy checklist (only if Scope includes ZK / privacy)

- [ ] No new address-keyed state was introduced for shielded balances, positions, or orders.
- [ ] No new event or metric leaks a trader address for a shielded transaction.
- [ ] Every new nullifier write is checked for double-spend before insertion.
- [ ] Every new proof verification path returns `Err` on failure (no `panic!`, no `unwrap`).
- [ ] If a new circuit was added, its public inputs are documented in the circuit's `// PUBLIC:` header comment and in `docs/zk/circuits.md`.

## Testing

<!-- Commands you ran. Paste the relevant tail of output for non-trivial changes. -->

```
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

## Risk

<!-- One or two sentences. What could break? What did you do to derisk it? -->

## Reviewer notes

<!-- Anything that needs special attention (rebases, large diffs, follow-ups). -->
