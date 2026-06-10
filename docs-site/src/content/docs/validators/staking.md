---
title: "Staking Guide"
---

Staking is how you participate in Mersennet consensus and earn rewards. This guide covers how staking works, delegation, rewards, unbonding, and slashing conditions.

## How Staking Works

In Mersennet's Delegated Proof-of-Stake (DPoS) model:

1. **Validators** stake MRSN to join the validator set and produce blocks.
2. **Delegators** stake MRSN with validators to share in rewards without running a node.
3. **Voting power** is proportional to total stake (validator + delegations).
4. **Block rewards** are distributed to all validators proportionally to their stake.

## Minimum Stake

The minimum stake to register as a validator is set by governance. As of the current testnet, any non-zero amount may be accepted. Check the latest network parameters for production mainnet.

## Delegation

Token holders can **delegate** MRSN to validators they trust:

- Delegated stake counts toward the validator's voting power.
- Delegators earn a share of block rewards (minus validator commission, if any).
- Delegators are subject to the same slashing risk as the validator they delegate to.

To delegate, send a delegation transaction specifying the validator address and amount. The exact interface (RPC, CLI, or staking UI) depends on the tooling available.

## Rewards

Block rewards are distributed **proportionally to stake**:

```
validator_reward = (block_reward × validator_stake) / total_stake
```

- **Initial block reward**: 10 MRSN per block
- **Halving**: Every 35,000,000 blocks (~1.1 years at ~1 s blocks)
- **Crediting**: Rewards are applied directly to validator/delegator balances—no claiming step required

Example with 4 validators each staking 1M MRSN:
- Total stake = 4M MRSN
- Block reward = 10 MRSN
- Each validator receives 10 × (1M / 4M) = **2.5 MRSN per block**

## Unbonding Period

When you **unbond** (withdraw) staked MRSN:

1. Your stake is **immediately** removed from voting power.
2. Tokens enter an **unbonding queue** for a fixed number of blocks.
3. After the unbonding period, tokens are returned to your balance.

| Network | Unbonding Period |
|---------|------------------|
| **Testnet** | 100 blocks (per the shipped testnet configs; the code default is 2) |
| **Mainnet** | 100 blocks at genesis; governance-configurable |

:::note
During unbonding, your tokens can still be slashed if the validator commits an offense. Only after the unbonding period completes are tokens safely returned.
:::

## Slashing Conditions

Validators (and their delegators) can lose stake through slashing:

### Double-Signing

| Aspect | Detail |
|--------|--------|
| **What** | Signing two different blocks at the same height |
| **Base penalty** | 5% of stake |
| **Consequence** | **Tombstoned** — permanently banned from the validator set |
| **Cause** | Running the same validator key on multiple nodes |

:::danger
Double-signing is permanent. A tombstoned validator cannot rejoin. Never duplicate your validator key across nodes.
:::

### Downtime (Precommit Timeout)

| Aspect | Detail |
|--------|--------|
| **What** | Failing to send a precommit vote in a consensus round |
| **Base penalty** | 1% of stake |
| **Consequence** | **Jailed** — temporarily excluded; can unjail after jail period |
| **Cause** | Node offline, network issues, slow hardware |

Penalties **escalate** with repeated offenses (e.g. +0.25% per offense, capped at 10%). Maintain high uptime and monitoring to avoid downtime slashing.

## Summary

| Topic | Summary |
|-------|---------|
| **Minimum stake** | Set by governance; check network params |
| **Delegation** | Stake with validators to earn rewards without running a node |
| **Rewards** | Proportional to stake; credited automatically |
| **Unbonding** | 100 blocks on testnet; tokens locked until period ends |
| **Slashing** | Double-sign → tombstoned; downtime → jailed + penalty |

For operational details, see [Validator Overview](/validators/overview) and [Monitoring & Alerts](/validators/monitoring).
