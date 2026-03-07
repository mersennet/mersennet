# Prime Chain (PRIM) Tokenomics

Complete technical documentation of PRIM token economics, emission schedule, allocations, validator incentives, and slashing mechanics.

---

## 1. Overview

| Parameter | Value |
|-----------|-------|
| **Token Name** | Prime Chain |
| **Ticker** | PRIM |
| **Total Supply** | 1,000,000,000 (1 Billion) |
| **Decimal Places** | 18 (1 PRIM = 10^18 wei) |
| **Consensus** | Proof-of-Stake (Tendermint BFT) |
| **Block Time** | ~2 seconds |

---

## 2. Token Allocation

| Category | Percentage | Tokens | Vesting Schedule |
|----------|-----------|--------|-----------------|
| **Block Rewards** | 70% | 700,000,000 | Halving every ~2.2 years (see §3) |
| **Ecosystem & Grants** | 10% | 100,000,000 | 5-year linear from TGE |
| **Foundation Reserve** | 10% | 100,000,000 | 1-year cliff + 4-year linear |
| **Team & Core Contributors** | 5% | 50,000,000 | 1-year cliff + 3-year linear |
| **Sales (Private + Public)** | 5% | 50,000,000 | 6-month cliff + 18-month linear |

### 2.1 Block Rewards (70%)

The largest allocation ensures long-term validator incentives and network security. Tokens are minted on every block according to the halving schedule (§3) and distributed proportionally to validator stake. No block rewards are pre-minted — they are created as each block is produced.

### 2.2 Ecosystem & Grants (10%)

Funds developer grants, DApp incentives, hackathons, bridge integrations, and strategic partnerships. Controlled by governance with a 5-year linear vesting to prevent dumping.

### 2.3 Foundation Reserve (10%)

Protocol development, security audits, infrastructure costs, legal, and operational runway. Subject to a 1-year cliff followed by 4-year linear vesting.

### 2.4 Team & Core Contributors (5%)

Aligned with standard 4-year vesting: no tokens for the first year (cliff), then monthly linear unlock over the remaining 3 years. Ensures long-term commitment.

### 2.5 Sales (5%)

Combined private and public sale allocation for fundraising. 6-month cliff followed by 18-month linear vesting. This conservative unlock protects early token price stability.

---

## 3. Block Reward Emission Schedule

### 3.1 Parameters

| Parameter | Value |
|-----------|-------|
| **Block Rewards Pool** | 700,000,000 PRIM |
| **Initial Reward** | 10 PRIM per block |
| **Halving Interval** | 35,000,000 blocks (~2.22 years at 2s) |
| **Block Time** | ~2 seconds |

### 3.2 Halving Curve

Rewards follow a Bitcoin-style halving schedule:

```
Era   Block Range                  Reward/Block    Minted in Era
───   ───────────────────────────  ────────────    ─────────────
0     0 — 34,999,999              10 PRIM         350,000,000
1     35,000,000 — 69,999,999      5 PRIM         175,000,000
2     70,000,000 — 104,999,999     2.5 PRIM        87,500,000
3     105,000,000 — 139,999,999    1.25 PRIM       43,750,000
4     140,000,000 — 174,999,999    0.625 PRIM      21,875,000
5     175,000,000 — 209,999,999    0.3125 PRIM     10,937,500
6     210,000,000 — 244,999,999    0.15625 PRIM     5,468,750
...   (continues halving)
```

The geometric series converges to exactly 700,000,000 PRIM:

```
total = initial_reward × halving_interval × 2
      = 10 × 35,000,000 × 2
      = 700,000,000 PRIM ✓
```

### 3.3 Emission Timeline

| Milestone | Era | Approximate Time | Block Rewards Minted | % of Pool |
|-----------|-----|-----------------|---------------------|-----------|
| First halving | 1 | ~2.2 years | 350,000,000 | 50.0% |
| Second halving | 2 | ~4.4 years | 525,000,000 | 75.0% |
| 87.5% minted | 3 | ~6.7 years | 612,500,000 | 87.5% |
| 93.75% minted | 4 | ~8.9 years | 656,250,000 | 93.75% |
| 96.9% minted | 5 | ~11.1 years | 678,125,000 | 96.9% |
| **99.2% minted** | **6** | **~13.3 years** | **689,062,500** | **98.4%** |
| 99.6% minted | 7 | ~15.6 years | 694,531,250 | 99.2% |

**99% of block rewards are emitted by approximately year 13.** The supply cap enforces a hard ceiling — if somehow the remaining supply is less than the scheduled reward, only the remainder is distributed.

### 3.4 Supply Cap Enforcement

Before distributing any reward, the system checks:

```
remaining_supply = max_supply - total_minted
effective_reward = min(scheduled_reward, remaining_supply)
```

Individual validator rewards are calculated from `effective_reward`, not the scheduled reward, ensuring `total_minted` can never exceed `max_supply`.

### 3.5 Implementation

```rust
pub fn reward_per_block(&self, height: u64) -> U256 {
    let halvings = height / self.halving_interval;
    let mut reward = self.initial_reward_per_block;
    for _ in 0..halvings {
        reward = reward / 2;
        if reward.is_zero() { break; }
    }
    reward
}
```

---

## 4. Reward Distribution

### 4.1 Stake-Proportional Split

Block rewards are distributed to **all active validators** proportional to their stake:

```
validator_reward = (effective_reward × validator_stake) / total_stake
```

**Example with 4 equal-stake validators:**

```
total_stake    = 4,000,000 PRIM (1M each)
reward/block   = 10 PRIM
each validator = 10 × 1,000,000 / 4,000,000 = 2.5 PRIM per block
```

### 4.2 Unequal Stake Example

```
Validator A: 5M stake → 5/10 = 50% of reward = 5.0 PRIM
Validator B: 3M stake → 3/10 = 30% of reward = 3.0 PRIM
Validator C: 1.5M stake → 1.5/10 = 15% of reward = 1.5 PRIM
Validator D: 0.5M stake → 0.5/10 =  5% of reward = 0.5 PRIM
                                                    ─────────
                                                    10.0 PRIM
```

### 4.3 Rounding and Burns

Due to integer division with 18-decimal precision, the sum of individual rewards may be slightly less than the effective reward. The difference is **implicitly burned**:

```
total_distributed = sum(all validator rewards)
burned_reward     = effective_reward - total_distributed
```

This burn is negligible (typically 0-2 wei per block) but ensures no tokens are created beyond the cap.

### 4.4 How Rewards Are Applied

Rewards are credited directly to each validator's account balance — no lockup, no vesting, no claiming required:

```rust
fn apply_rewards(&mut self, rewards: &[Reward]) -> Result<()> {
    for reward in rewards {
        let mut info = self.evm.db.basic(reward.address)?;
        info.balance += reward.amount;
        self.evm.db.insert_account_info(reward.address, info);
    }
}
```

---

## 5. Validator Economics

### 5.1 Becoming a Validator

To become a validator, an address must stake PRIM tokens:

```rust
engine.add_validator(address, stake)
```

- **Minimum stake:** Any non-zero amount (governance may set higher minimums)
- **Multiple stakes:** A validator can increase stake with additional `stake()` calls
- **New validators** are added via `pending_changes` and applied at the next block

### 5.2 Unbonding

Validators can withdraw stake, subject to an **unbonding period**:

```
unbonding_period = 2 blocks (testnet default, governance-adjustable)
```

When a validator unbonds:
1. Stake is immediately deducted from their voting power
2. Tokens are locked for `unbonding_period` blocks
3. After the period, tokens are returned to the validator's balance
4. During unbonding, tokens can still be slashed

### 5.3 Proposer Selection

The block proposer rotates using a **weighted round-robin** algorithm:

```
proposer = validator with highest priority
priority[i] += normalized_weight[i]   (each round)
priority[proposer] -= total_weight     (after selection)
```

Weights are normalized to prevent overflow with 18-decimal stake values. Validators propose blocks proportionally to their stake, identical to Tendermint/CometBFT.

### 5.4 Estimated Validator APY

At genesis with 4 validators (1M PRIM staked each):

```
blocks/year       ≈ 15,768,000 (at 2s block time)
reward/block      = 10 PRIM
validator/block   = 2.5 PRIM (with 4 equal validators)
annual/validator  = 2.5 × 15,768,000 = 39,420,000 PRIM
APY               = 39,420,000 / 1,000,000 = 3,942%
```

This high initial APY incentivizes early staking. As more validators join and total stake increases, APY decreases proportionally. As halvings occur, the reward rate drops further.

---

## 6. Slashing

### 6.1 Offense Types

| Offense | Base Penalty (bps) | Description |
|---------|-------------------|-------------|
| **Double Sign** | 500 (5%) | Signing two different blocks at the same height |
| **Precommit Timeout** | 100 (1%) | Failing to precommit in a consensus round |

*bps = basis points. 100 bps = 1%.*

### 6.2 Escalation

Penalties **escalate** with repeated offenses:

```
actual_penalty = base_bps + (escalation_step × offense_count) + (escalation_step × rounds_missed)
actual_penalty = min(actual_penalty, max_escalation_bps)
```

With defaults:
```
escalation_step  = 25 bps (0.25%)
max_escalation   = 1000 bps (10%)

First timeout:   100 bps = 1%
Second timeout:  125 bps = 1.25%
Third timeout:   150 bps = 1.5%
...
Cap at:          1000 bps = 10%
```

### 6.3 Slashing Source Priority

When slashing occurs, tokens are taken from:
1. **Unbonding queue first** — tokens being withdrawn
2. **Active stake second** — if unbonding doesn't cover it
3. **Remainder burned** — if the validator doesn't have enough

### 6.4 Jail and Tombstone

| State | Meaning | Recovery |
|-------|---------|----------|
| **Jailed** | Temporarily excluded from consensus | Can `unjail()` after jail period |
| **Tombstoned** | Permanently banned | Cannot rejoin, cannot stake |

- Double-signing → **tombstoned** (permanent)
- Repeated timeouts → **jailed** (temporary)
- Tombstoned validators cannot re-stake or participate ever again

### 6.5 Evidence Expiration

Slashing evidence has a **max age** (default: 10,000 blocks). Evidence older than this is ignored, preventing ancient history from being used to slash validators.

---

## 7. Consensus Economics

### 7.1 BFT Finality

- **Threshold:** >2/3 of total stake required to finalize a block
- **With 4 equal validators:** need 3 of 4 (tolerates 1 failure)
- **Finalized blocks are irreversible** — no chain reorganizations

### 7.2 Block Structure (economic fields)

Every block includes:

```json
{
  "number": 1000,
  "rewards": [
    { "address": "0x5b86...", "amount": "2500000000000000000" },
    { "address": "0x4139...", "amount": "2500000000000000000" },
    { "address": "0xf7de...", "amount": "2500000000000000000" },
    { "address": "0x3314...", "amount": "2500000000000000000" }
  ],
  "total_reward": "10000000000000000000",
  "burned_reward": "0",
  "scheduled_reward": "10000000000000000000",
  "remaining_supply": "999999990000000000000000000"
}
```

### 7.3 EIP-1559 Fee Market

Prime Chain implements EIP-1559 base fee adjustment:

```
gas_limit_per_block     = 30,000,000
fee_elasticity          = 2x
max_change_denominator  = 8  (12.5% max change per block)
```

- **Base fee** adjusts up when blocks are >50% full, down when <50%
- Transaction fees are **not** given to validators (implicitly burned by gas accounting)
- Validators earn exclusively from block rewards, not gas fees

---

## 8. Governance

Token economics parameters can be changed via **on-chain governance**:

| Parameter | Governance Changeable |
|-----------|----------------------|
| Gas limit per block | Yes (`SetFeeMarket` proposal) |
| Fee elasticity | Yes |
| Max supply | Yes (`SetTokenEconomics` proposal) |
| Reward per block | Yes |
| Halving interval | Yes |

Governance requires:
- **Quorum:** 60% of total stake must vote
- **Pass threshold:** 50% of votes must be `yes`
- **Voting power** = validator stake

---

## 9. Comparison to Other L1s

| Chain | Total Supply | Block Rewards | Team | Investors | Ecosystem | 99% Emitted |
|-------|-------------|--------------|------|-----------|-----------|-------------|
| **Bitcoin** | 21M | 100% | 0% | 0% | 0% | ~25 years |
| **Avalanche** | 720M | 50% | 20% | 4% | 19% | ~10 years |
| **Cosmos** | ~750M | 68.5% | 3.2% | 3.8% | 3.1% | Perpetual |
| **Celestia** | 1B | (inflation) | 17.6% | 35.6% | 26.8% | Perpetual |
| **Polkadot** | 2.1B | Halving | ~5% | ~13% | ~17% | ~12+ years |
| **PRIM** | **1B** | **70%** | **5%** | **5%** | **20%** | **~13 years** |

Prime Chain's allocation is closest to Cosmos (high block rewards) with the halving discipline of Bitcoin/Polkadot and a conservative team allocation.

---

## 10. Configuration Reference

All token economics are set via the node config JSON:

```json
{
  "token_economics": {
    "max_supply": "1000000000000000000000000000",
    "initial_reward_per_block": "10000000000000000000",
    "halving_interval": 35000000
  },
  "slashing": {
    "double_sign_bps": 500,
    "timeout_bps": 100,
    "escalation_step_bps": 25,
    "escalation_max_bps": 1000,
    "round_timeout_ms": 500,
    "unbonding_period": 2
  }
}
```

---

## 11. Source Code Reference

| Component | File |
|-----------|------|
| Reward calculation | `crates/core/src/consensus.rs` → `reward_per_block()`, `finalize()` |
| Reward application | `crates/core/src/engine.rs` → `apply_rewards()` |
| Slashing logic | `crates/core/src/consensus.rs` → `slash_amount_for_evidence()` |
| Unbonding | `crates/core/src/consensus.rs` → `unbond()`, `process_unbonding()` |
| Proposer selection | `crates/core/src/consensus.rs` → `proposer()` |
| Fee market | `crates/core/src/engine.rs` → `set_fee_market_params()` |
| Governance | `crates/core/src/governance.rs` |
| Config defaults | `crates/core/src/config.rs` → `TokenEconomicsConfig` |
