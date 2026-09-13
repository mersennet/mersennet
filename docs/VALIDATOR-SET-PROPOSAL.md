# Opening the validator set — decision memo

Status: proposal, 2026-09-13. Needs three decisions from the founder (marked **DECIDE**); everything else is an implementation plan I can execute.

## Where we are

- Four genesis validators, 1,000,000 MRSN each, fixed in `config.json`. Leader schedule = sorted addresses, `(height + round) % 4`. Finality = 2/3 of stake (3 of 4).
- The staking precompile already tracks delegations (`staking` inside the orders state) and reserves selectors for registration; `Consensus` has `pending_changes: Vec<ValidatorChange>` and unbonding plumbing that nothing calls today.
- Validator identity = the node key (`keys/node_key.json`). There is no key rotation: an identity is whatever address is in genesis.
- Since today the engine has fork choice by finality, snapshots, verified-node attestations (`whoami`) and a registration intake (feedback category `validator`, one professional operator waiting). What is missing is only the rule that turns a registered candidate into a block producer.

## What "open" means here (proposal)

1. **Register**: an address calls `staking.registerValidator(nodeIdentity, commissionBps)` with at least `MIN_SELF_STAKE` bonded from its own balance. `nodeIdentity` is the address of the node key that will sign blocks (proved by a `whoami`-style signature over the registrant address, so nobody can register a node they do not run).
2. **Activate at an epoch boundary**: every `EPOCH_BLOCKS`, the engine recomputes the active set: the top `MAX_VALIDATORS` registered validators by total stake (self + delegated), each ≥ `MIN_SELF_STAKE`, that were registered ≥ 1 epoch ago. Deterministic from state, so every node agrees — the set becomes chain state, not config.
3. **Exit**: `unregister()` removes the validator at the next epoch; stake unbonds over `UNBONDING_BLOCKS`. Missing more than `MAX_MISSED_PER_EPOCH` leader slots in an epoch → jailed for one epoch (removed from the schedule, not slashed); equivocation → slashed (already implemented).
4. **Genesis validators** stay in the set through the same rule (their stake qualifies); the foundation can unregister them later.

## DECIDE — parameters

| Parameter | Proposal | Why |
|---|---|---|
| `MIN_SELF_STAKE` | **10,000 MRSN** | Reachable for a serious testnet operator (faucet gives 1,000/h, so a real commitment, not a click), far below the 1M genesis stakes so newcomers can enter without another mint. Mainnet value decided separately. |
| `MAX_VALIDATORS` | **9** | Odd; 2/3 of 9 = 6 finality votes; leader timeouts (19 s) with one node down cost 1/9 of heights instead of 1/4. Gossip fan-out and vote traffic stay trivial. Raise later by governance. |
| `EPOCH_BLOCKS` | **21,600** (~12 h at 2 s) | Long enough that set changes are rare and observable, short enough to onboard someone the same day. |
| `UNBONDING_BLOCKS` | 43,200 (~1 day) | Testnet: fast enough to test exits; mainnet will be weeks. |
| `MAX_MISSED_PER_EPOCH` | 20% of a validator's slots | Jails a dead node within an epoch without punishing a restart. |
| Delegation | keep enabled (already live) | Delegators' stake counts toward ranking; rewards split by `commissionBps`. |

## Safety analysis (why this does not break today's chain)

- **Determinism**: the set is derived from committed state at epoch boundaries; the same rule runs on every node, so leader schedules agree (the two-lineage bug found today would have broken this — fixed, and the resume state-root check now guards it).
- **Finality**: with N validators the threshold stays 2/3 of *stake*, so a large genesis stake can outvote newcomers until genesis stakes are reduced. That is intended for testnet; for mainnet the genesis stakes must be right-sized.
- **Rolling upgrade**: the rule activates at a configured height (`validator_set_activation_height` in `config.json`, canonical for everyone). Nodes without the upgrade fork at that height — the same class of fork the watchdog now heals, but every node must be upgraded first: publish the bundle, roll the fleet, give community nodes 48 h (`mersennet-check` shows "update available"), then set the activation height one more bundle later.
- **Key rotation**: `rotateNodeKey(newIdentity)` (same proof) takes effect at the next epoch. This is the validator key management item on the roadmap; it comes for free with registration.

## Work (mine) once parameters are decided

| Step | Size |
|---|---|
| Staking precompile: register / unregister / rotateNodeKey with the identity proof; state in orders state (already persisted + snapshotted) | M |
| Engine: epoch boundary recompute, active set from state, jail on missed slots, activation height gate | M |
| Config + docs + explorer "Validators" page showing registered/active/jailed and next epoch | S |
| Trade terminal: "Become a validator" flow (bond, register with node identity from `whoami`) | M |
| Bundle, fleet roll, 48 h community window, activation | S + waiting |

Total ≈ 4–5 working days including tests on a local multi-node net.

## Not in scope

Mainnet economics (genesis allocation, emission to validators vs. delegators), slashing severity, and governance over parameters — those need the token model finalised.
