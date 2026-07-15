# Mainnet launch runbook

Everything required to take Mersennet from the current testnet
(chain id 131071) to a production mainnet. Written as a gated
checklist: each phase has explicit **go/no-go criteria** — do not
proceed past a gate that isn't green. Nothing in this document is a
live operation by itself; the launch is executed top-to-bottom in a
scheduled window once every gate passes.

---

## Phase 0 — Decisions that must be made first (humans, not ops)

These are irreversible-by-social-contract choices. Record each in a
signed decision doc before any infra work starts.

| Decision | Current testnet value | Mainnet needs |
|---|---|---|
| Chain ID | `131071` (2^17−1) | A distinct Mersenne prime id, e.g. `524287` (2^19−1). Must not collide with [chainlist](https://chainlist.org). Register it. |
| Block time | 2000 ms | Keep 2000 ms unless load tests say otherwise. |
| Emission | `2^61 − 1` wei/block (~2.3 MRSN), halving every 5th-perfect-number blocks | Confirm or finalize new schedule; publish the supply curve. |
| Genesis allocations | Faucet-funded test accounts | Real allocation table: investors (per SAFT), team vesting, treasury, ecosystem fund. Comes from the cap table — legal sign-off required. |
| Validator set | 4 in-house validators + 1 public RPC | Minimum 4 for BFT (f=1); target ≥7 (f=2) with at least 2 operated by external parties before decentralization claims. |
| Privacy fork | Primitives live, fork **not activated** | Decide: launch privacy-active from genesis, or activate later at a published block height. Later activation is the lower-risk path (same as testnet posture). |
| Prover mode | `development` (mock proofs) | Mainnet must run real SP1 proving (see `sp1-prover.md`) or clearly label proofs as development-mode in the explorer until it does. |
| Genesis CLOB markets | Seeded from `GenesisConfig.markets` | Final market list, tick/lot sizes, margin params, and whether the in-house market maker runs at launch (label it in the UI if so). |

## Phase 1 — Code freeze & audit gate

- [ ] Tag a release candidate (`vX.Y.Z-rc1`) of `mersennet/mersennet`.
      All consensus-affecting work (block hash, tx hashing, BFT voting,
      CLOB-in-consensus) is already in; nothing consensus-breaking may
      merge after the tag.
- [ ] **Third-party security audit** of, at minimum:
      `crates/core` (engine, state, precompiles, mempool),
      `crates/network` (p2p, vote relay), `crates/rpc`
      (tx acceptance, shielded gating), `crates/zkp` (Poseidon2, note
      commitments, circuits). *This is the single longest-lead item —
      book the firm now.*
- [ ] Fix-and-retag loop until the audit report has no open
      high/critical findings.
- [ ] Reproducible build: document the exact toolchain
      (`rust-toolchain.toml`) and produce checksummed release binaries.

**Gate: no unaudited consensus code ships to mainnet.**

## Phase 2 — Dress rehearsal on a scratch network

Use `deploy/deploy-testnet.sh` with a scratch `nodes.conf` (5 fresh
VPSes) and the **mainnet genesis file** (real chain id, real
allocations, faucet disabled):

- [ ] Boot from the actual mainnet genesis; verify allocations with
      `eth_getBalance` against the allocation table (every row).
- [ ] Run ≥72h with the chain watchdog (`chain-watchdog.sh`) attached;
      zero halts, zero state-root mismatches in logs.
- [ ] Kill-test: stop the leader mid-round → next leader takes over
      within the timeout; stop 1 validator entirely → chain continues
      (f=1); restart it → it resyncs to head.
- [ ] Restart-test every node one by one; verify no nonce regressions
      and CLOB state survives (the `orders.state` restore path).
- [ ] Wallet e2e: MetaMask EIP-155 transfer, ERC-20 deploy + transfer,
      precompile order placement — all mined and indexed.
- [ ] Explorer + indexer pointed at the rehearsal net show correct
      blocks/txs/events from genesis.
- [ ] If launching privacy-active (or activating later): run the
      shielded self-test (`deploy/zk-selftest`) and a grant-gated
      viewNotes/viewBalances scan with the SDKs.

**Gate: 72 clean hours + all kill-tests pass on the release binary.**

## Phase 3 — Production infrastructure

- [ ] Provision validator hosts (≥4, ideally 7): 4 vCPU / 8 GB min,
      NVMe, separate providers/regions for fault independence.
- [ ] Public RPC tier: ≥2 nodes behind Cloudflare (the testnet layout),
      `firewall-public.sh` applied; validators locked down with
      `firewall-validator.sh` (p2p ports only, no public RPC).
- [ ] Validator keys generated **on the hosts**, never transmitted;
      offline backup of each (sealed, 2-person rule). These sign BFT
      votes — compromise = consensus compromise.
- [ ] Dedicated SP1 prover box (see `sp1-prover.md` for sizing) if real
      proving is a launch requirement.
- [ ] Monitoring: `mersennet-watchdog.service` on every node, plus
      alerting (the testnet watchdog's alert hook) wired to a paged
      channel, not just logs.
- [ ] DNS + TLS for `rpc.`, `explorer.`, `trade.`, `docs.` mainnet
      equivalents; decide whether testnet keeps running (recommended:
      yes, testnet stays as the staging net).

## Phase 4 — Genesis ceremony

- [ ] Freeze the genesis file (allocations, markets, chain id, emission,
      privacy-fork height). Publish its SHA-256 in advance.
- [ ] Each validator operator independently verifies the genesis hash
      and the release binary checksum before starting.
- [ ] Coordinated start (same procedure as the testnet reset runbook,
      `RESET-RUNBOOK.md`): start validators, confirm block 1 has
      signatures from ≥2f+1 validators, then open the public RPC tier.
- [ ] Verify: chain id, block cadence, emission per block, allocation
      balances, CLOB markets present, explorer indexing from block 0.

**Go/no-go call 1 hour after genesis: any state-root mismatch or
missing validator signature → halt, diagnose, restart ceremony.**

## Phase 5 — Launch week operations

- [ ] 24/7 on-call rotation for week 1; watchdog paging live.
- [ ] Publish: genesis hash, validator set, RPC endpoints, bridge/faucet
      policy (no faucet on mainnet), audited-release notes.
- [ ] Snapshot cadence: state snapshot every 6h for the first week
      (restore drill was rehearsed in Phase 2).
- [ ] Explorer "development prover" labeling stays until real SP1
      proofs are live (honesty posture carried over from testnet).
- [ ] Post-launch review at day 7: incident log, perf numbers, decision
      on opening validator applications to external operators.

---

## What is already done (testnet evidence)

- Networked single-leader BFT with signed votes, quorum commit, and
  timeout failover — live since the correctness overhaul reset.
- Content-committing block hashes + parent-hash linkage.
- Canonical RLP/EIP-155 transactions (MetaMask verified e2e).
- CLOB routed through consensus (genesis-seeded markets, signed
  precompile txs, unsigned order RPC disabled).
- Chain-halt watchdog + systemd hardening + memory caps in production.
- Shielded primitives (notes, nullifiers, Poseidon2, state proofs)
  anchored in every block; privacy fork gated and inactive.
- SDK privacy parity (TS/Python/Go): note scanning, balance/position
  reconstruction, viewing keys, compliance attestations.

## What is NOT done (and blocks mainnet)

1. Third-party audit (Phase 1) — longest lead time, start first.
2. Real SP1 proving in production (or explicit dev-prover labeling).
3. Final tokenomics/allocation table with legal sign-off.
4. External validator operators (decentralization floor).
5. The dress rehearsal itself (Phase 2).
