---
title: "Validator Overview"
---

Validators are the backbone of Mersennet. They run full nodes, participate in consensus, produce blocks, and earn block rewards in proportion to their stake. This guide explains what validators do, how the Proof-of-Stake consensus works, and what you need to become one.

## What Validators Do

Validators perform three critical functions:

1. **Block Production** — Validators take turns proposing new blocks containing transactions, order submissions, and state updates. The proposer is selected via a round-robin algorithm weighted by stake.

2. **Consensus Participation** — All validators participate in the BFT (Byzantine Fault Tolerant) consensus process. They vote on proposed blocks (prevote and precommit phases) to achieve finality. A block is finalized when more than 2/3 of total stake has voted for it.

3. **Network Security** — By staking MRSN tokens, validators have economic skin in the game. Malicious or negligent behavior is penalized through slashing, which protects the network from attacks and downtime.

## Proof-of-Stake (HotStuff-2 BFT)

Mersennet uses stake-weighted BFT consensus with the following characteristics:

| Aspect | Description |
|--------|-------------|
| **Proposer Selection** | Round-robin with priority-based weighting. Validators with higher stake are selected more frequently to propose blocks. |
| **Voting Power** | Proportional to staked MRSN. One validator with 2M MRSN has twice the voting power of a validator with 1M MRSN. |
| **Finality Threshold** | >2/3 of total stake must precommit for a block to be finalized. |
| **Block Time** | ~1 second per block. |

:::note
Delegation is not yet implemented — all stake is currently bonded directly
by validators. See the [Staking Guide](/validators/staking/) for details.
:::

## Block Production

Each block is produced by a single **proposer**—the validator selected for that height. The proposer:

1. Collects transactions from the mempool
2. Executes EVM transactions and PrimeOrders operations
3. Applies state transitions
4. Broadcasts the proposed block to other validators

Other validators then vote on the block. Once 2/3+ stake has precommitted, the block is **finalized** and irreversible. There are no chain reorganizations for finalized blocks.

## Earning Rewards

Validators earn **block rewards** in MRSN, distributed proportionally to stake:

```
validator_reward = (block_reward × validator_stake) / total_stake
```

- **Initial reward**: 10 MRSN per block
- **Halving**: Every 35,000,000 blocks (~1.1 years at ~1 s blocks), the reward halves
- **Distribution**: Rewards are credited directly to validator addresses—no claiming required

The more stake you have (your own + delegations), the larger your share of each block's reward.

## Slashing Risks

Validators can lose stake through **slashing** for consensus violations:

| Offense | Penalty | Consequence |
|---------|---------|-------------|
| **Double-signing** | 5% of stake (base) | **Tombstoned** — permanently banned from the validator set |
| **Downtime / Precommit timeout** | 1% of stake (base) | **Jailed** — temporarily excluded; can unjail after period |

Penalties **escalate** with repeated offenses (up to 10% max). Double-signing is the most severe: a tombstoned validator cannot rejoin the network.

:::caution
Never run the same validator key on multiple nodes. Double-signing occurs when two nodes with the same key sign different blocks at the same height—this will get you tombstoned.
:::

## Requirements Overview

Before running a validator, ensure you meet:

| Category | Requirement |
|----------|-------------|
| **Hardware** | 4 CPU cores, 8 GB RAM, 100 GB SSD, 100 Mbps network |
| **Software** | Rust 1.75+ (for building from source) |
| **Stake** | Minimum stake to register (governance may set higher minimums) |
| **Operational** | 24/7 uptime, monitoring, key management, backup procedures |

See [Run a Validator Node](/validators/run-a-node) for the complete setup guide.
