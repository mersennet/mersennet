# Prime Chain Tokenomics

Complete technical documentation of Prime Chain's token economics, block rewards, validator incentives, and slashing mechanics.

---

## 1. Token Supply

| Parameter | Value |
|-----------|-------|
| **Max Supply** | 42,000,000 tokens |
| **Decimal Places** | 18 (like ETH, 1 token = 10^18 wei) |
| **Initial Circulating** | Defined by genesis accounts |
| **Emission** | Block rewards only (no pre-mine beyond genesis) |

The max supply of 42M tokens is a hard cap enforced at the consensus level. Once `total_minted` reaches `max_supply`, block rewards drop to zero permanently. No more tokens can ever be created.

**Raw value in config:** `"max_supply": "42000000000000000000000000"` (42M × 10^18)

---

## 2. Block Rewards

### 2.1 Base Reward Schedule

| Parameter | Value |
|-----------|-------|
| **Initial Reward** | 100 tokens per block |
| **Halving Interval** | 4,200,000 blocks |
| **Block Time** | ~2 seconds (testnet) |

The reward per block follows a Bitcoin-style halving curve:

```
Block Height          Reward per Block
─────────────         ────────────────
0 — 4,199,999         100 tokens
4,200,000 — 8,399,999  50 tokens
8,400,000 — 12,599,999 25 tokens
12,600,000 — 16,799,999 12.5 tokens
...                     (continues halving)
```

**Implementation** (from `consensus.rs`):

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

### 2.2 Supply Cap Enforcement

Before distributing any reward, the system checks remaining supply:

```
remaining_supply = max_supply - total_minted
effective_reward = min(scheduled_reward, remaining_supply)
```

If only 30 tokens remain in the supply and the scheduled reward is 100, only 30 tokens are distributed. When remaining supply hits 0, rewards stop forever.

### 2.3 Time Estimates

At 2-second block time:

| Event | Blocks | Approximate Time |
|-------|--------|-----------------|
| First halving | 4,200,000 | ~97 days |
| Second halving | 8,400,000 | ~194 days |
| 50% minted | ~4,200,000 | ~97 days |
| 75% minted | ~8,400,000 | ~194 days |
| 99% minted | ~37,800,000 | ~2.4 years |

---

## 3. Reward Distribution

### 3.1 Stake-Proportional Split

Block rewards are distributed to **all active validators** proportional to their stake:

```
validator_reward = (reward_per_block × validator_stake) / total_stake
```

**Example with the current testnet (4 equal-stake validators):**

```
total_stake    = 400,000 tokens (100K each)
reward/block   = 100 tokens
each validator = 100 × 100,000 / 400,000 = 25 tokens per block
```

This is why each validator earns ~25 tokens/block and by block 2,276 has accrued:

```
initial_balance + (blocks × reward_per_validator)
1,000,000 + (2,276 × ~25) ≈ 1,016,175 tokens  ← matches actual balance
```

### 3.2 Unequal Stake Example

If validator stakes were different:

```
Validator A: 50 stake  → 50/100 = 50% of reward = 50 tokens
Validator B: 30 stake  → 30/100 = 30% of reward = 30 tokens
Validator C: 15 stake  → 15/100 = 15% of reward = 15 tokens
Validator D:  5 stake  →  5/100 =  5% of reward =  5 tokens
                                                   ─────────
                                                   100 tokens
```

### 3.3 Rounding and Burns

Due to integer division, the sum of individual rewards may be slightly less than the scheduled reward. The difference is **burned**:

```
total_distributed = sum(all validator rewards)
burned_reward     = effective_reward - total_distributed
```

This small burn (typically 0-2 wei per block) is tracked and reported in every block's `burned_reward` field. Over millions of blocks, the burn is negligible but ensures no tokens are created out of thin air.

### 3.4 How Rewards Are Applied

Rewards are credited directly to each validator's EVM account balance:

```rust
fn apply_rewards(&mut self, rewards: &[Reward]) -> Result<()> {
    for reward in rewards {
        let mut info = self.evm.db.basic(reward.address)?;
        info.balance += reward.amount;
        self.evm.db.insert_account_info(reward.address, info);
    }
}
```

This means validators can immediately spend, transfer, or stake their rewards — no lockup, no vesting, no claiming required.

---

## 4. Validator Economics

### 4.1 Becoming a Validator

To become a validator, an address must stake tokens:

```rust
engine.add_validator(address, stake)
```

- **Minimum stake:** Any non-zero amount (governance may set higher minimums)
- **Multiple stakes:** A validator can increase stake with additional `stake()` calls
- **New validators** are added via `pending_changes` and applied at the next block

### 4.2 Unbonding

Validators can withdraw stake, subject to an **unbonding period**:

```
unbonding_period = 100 blocks (testnet default)
```

When a validator unbonds:
1. Stake is immediately deducted from their voting power
2. Tokens are locked for `unbonding_period` blocks
3. After the period, tokens are returned to the validator's balance
4. During unbonding, tokens can still be slashed

### 4.3 Proposer Selection

The block proposer rotates using a **weighted round-robin** algorithm:

```
proposer = validator with highest priority
priority[i] += voting_power[i]   (each round)
priority[proposer] -= total_voting_power  (after selection)
```

This ensures validators propose blocks proportionally to their stake, identical to Tendermint/CometBFT.

---

## 5. Slashing

### 5.1 Offense Types

| Offense | Base Penalty (bps) | Description |
|---------|-------------------|-------------|
| **Double Sign** | 500 (5%) | Signing two different blocks at the same height |
| **Precommit Timeout** | 100 (1%) | Failing to precommit in a consensus round |

*bps = basis points. 100 bps = 1%.*

### 5.2 Escalation

Penalties **escalate** with repeated offenses:

```
actual_penalty = base_bps + (escalation_step × offense_count) + (escalation_step × rounds_missed)
actual_penalty = min(actual_penalty, max_escalation_bps)
```

With testnet defaults:
```
escalation_step  = 25 bps (0.25%)
max_escalation   = 1000 bps (10%)

First timeout:   100 bps = 1%
Second timeout:  125 bps = 1.25%
Third timeout:   150 bps = 1.5%
...
Cap at:          1000 bps = 10%
```

### 5.3 Slashing Source Priority

When slashing occurs, tokens are taken from:
1. **Unbonding queue first** — tokens being withdrawn
2. **Active stake second** — if unbonding doesn't cover it
3. **Remaining goes to burn** — if the validator doesn't have enough

### 5.4 Jail and Tombstone

| State | Meaning | Recovery |
|-------|---------|----------|
| **Jailed** | Temporarily excluded from consensus | Can `unjail()` after jail period |
| **Tombstoned** | Permanently banned | Cannot rejoin, cannot stake |

- Double-signing → **tombstoned** (permanent)
- Repeated timeouts → **jailed** (temporary)
- Tombstoned validators cannot re-stake or participate ever again

### 5.5 Evidence Expiration

Slashing evidence has a **max age** (default: 10,000 blocks). Evidence older than this is ignored, preventing ancient history from being used to slash validators.

---

## 6. Consensus Economics

### 6.1 BFT Finality

- **Threshold:** >2/3 of total stake required to finalize a block
- **With 4 equal validators:** need 3 of 4 (tolerates 1 failure)
- **Finalized blocks are irreversible** — no chain reorganizations

### 6.2 Block Structure (economic fields)

Every block includes:

```json
{
  "number": 2276,
  "rewards": [
    { "address": "0x5b86...", "amount": "25000000000000000000" },
    { "address": "0x4139...", "amount": "25000000000000000000" },
    { "address": "0xf7de...", "amount": "25000000000000000000" },
    { "address": "0x3314...", "amount": "25000000000000000000" }
  ],
  "total_reward": "100000000000000000000",
  "burned_reward": "0",
  "scheduled_reward": "100000000000000000000",
  "remaining_supply": "41999772400000000000000000",
  "slashes": [],
  "applied_validator_changes": []
}
```

### 6.3 EIP-1559 Fee Market

Prime Chain implements EIP-1559 base fee adjustment:

```
gas_limit_per_block     = 30,000,000
fee_elasticity          = 2x
max_change_denominator  = 8  (12.5% max change per block)
```

- **Base fee** adjusts up when blocks are >50% full, down when <50%
- Transaction fees are **not** given to validators (they're implicitly burned by gas accounting)
- Validators earn exclusively from block rewards, not gas fees

---

## 7. Governance

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

## 8. Current Testnet State

*Snapshot from block ~2,276:*

| Metric | Value |
|--------|-------|
| Block height | 2,276 |
| Block reward | 100 tokens/block |
| Blocks until halving | 4,197,724 |
| Total minted (est.) | ~227,600 tokens |
| Remaining supply | ~41,772,400 tokens |
| Validator count | 4 |
| Total stake | 400,000 tokens |
| Per-validator reward | 25 tokens/block |
| Per-validator balance | ~1,016,175 tokens |
| Faucet balance | ~9,998,000 tokens |

---

## 9. Configuration Reference

All token economics are set via the node config JSON:

```json
{
  "token_economics": {
    "max_supply": "42000000000000000000000000",
    "initial_reward_per_block": "100000000000000000000",
    "halving_interval": 4200000
  },
  "slashing": {
    "double_sign_bps": 500,
    "timeout_bps": 100,
    "escalation_step_bps": 25,
    "escalation_max_bps": 1000,
    "round_timeout_ms": 500,
    "unbonding_period": 100
  }
}
```

---

## 10. Source Code Reference

| Component | File |
|-----------|------|
| Reward calculation | `crates/core/src/consensus.rs` → `reward_per_block()`, `finalize()` |
| Reward application | `crates/core/src/engine.rs` → `apply_rewards()` |
| Slashing logic | `crates/core/src/consensus.rs` → `slash_amount_for_evidence()` |
| Unbonding | `crates/core/src/consensus.rs` → `unbond()`, `process_unbonding()` |
| Proposer selection | `crates/core/src/consensus.rs` → `proposer()` |
| Fee market | `crates/core/src/engine.rs` → `set_fee_market_params()` |
| Governance | `crates/core/src/governance.rs` |
| Config | `crates/core/src/config.rs` → `TokenEconomicsConfig` |
