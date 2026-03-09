---
slug: /architecture/tokenomics
sidebar_position: 4
title: "Token Economics"
---

# Token Economics

This document provides complete tokenomics documentation for Prime Chain (PRIM), including supply, allocation, emission schedule, and validator reward distribution.

## Overview

| Parameter | Value |
|-----------|-------|
| **Token Name** | Prime Chain |
| **Ticker** | PRIM |
| **Max Supply** | 1,000,000,000 (1 billion) |
| **Decimals** | 18 |
| **Chain ID** | 7919 |

## Allocation Breakdown

| Category | Percentage | Tokens | Vesting |
|----------|------------|--------|---------|
| **Block Rewards** | 70% | 700,000,000 | Halving schedule (see below) |
| **Ecosystem & Grants** | 10% | 100,000,000 | 5-year linear from TGE |
| **Foundation Reserve** | 10% | 100,000,000 | 1-year cliff + 4-year linear |
| **Team & Core Contributors** | 5% | 50,000,000 | 1-year cliff + 3-year linear |
| **Sales (Private + Public)** | 5% | 50,000,000 | 6-month cliff + 18-month linear |

### Block Rewards (70%)

The largest allocation ensures long-term validator incentives and network security. Tokens are **minted on every block** according to the halving schedule—none are pre-minted. Distribution is proportional to validator stake.

### Ecosystem & Grants (10%)

Developer grants, DApp incentives, hackathons, bridge integrations, and strategic partnerships. Controlled by governance with 5-year linear vesting.

### Foundation Reserve (10%)

Protocol development, security audits, infrastructure, legal, and operations. 1-year cliff followed by 4-year linear vesting.

### Team & Core Contributors (5%)

4-year vesting: 1-year cliff, then monthly linear unlock over 3 years.

### Sales (5%)

Private and public sale allocation. 6-month cliff + 18-month linear vesting.

## Emission Schedule

### Parameters

| Parameter | Value |
|-----------|-------|
| **Block Rewards Pool** | 700,000,000 PRIM |
| **Initial Reward per Block** | 10 PRIM |
| **Halving Interval** | 35,000,000 blocks |
| **Block Time** | ~1 second |
| **Halving Period** | ~2.22 years |

### Halving Epochs

Rewards follow a Bitcoin-style halving schedule:

| Era | Block Range | Reward/Block | Minted in Era |
|-----|-------------|--------------|---------------|
| 0 | 0 — 34,999,999 | 10 PRIM | 350,000,000 |
| 1 | 35,000,000 — 69,999,999 | 5 PRIM | 175,000,000 |
| 2 | 70,000,000 — 104,999,999 | 2.5 PRIM | 87,500,000 |
| 3 | 105,000,000 — 139,999,999 | 1.25 PRIM | 43,750,000 |
| 4 | 140,000,000 — 174,999,999 | 0.625 PRIM | 21,875,000 |
| 5 | 175,000,000 — 209,999,999 | 0.3125 PRIM | 10,937,500 |
| 6 | 210,000,000 — 244,999,999 | 0.15625 PRIM | 5,468,750 |
| ... | (continues halving) | ... | ... |

The geometric series converges to exactly 700,000,000 PRIM:

```
total = initial_reward × halving_interval × 2
      = 10 × 35,000,000 × 2
      = 700,000,000 PRIM ✓
```

### Emission Timeline

| Milestone | Era | Approx. Time | Block Rewards Minted | % of Pool |
|-----------|-----|---------------|----------------------|------------|
| First halving | 1 | ~2.2 years | 350,000,000 | 50.0% |
| Second halving | 2 | ~4.4 years | 525,000,000 | 75.0% |
| 87.5% minted | 3 | ~6.7 years | 612,500,000 | 87.5% |
| 93.75% minted | 4 | ~8.9 years | 656,250,000 | 93.75% |
| 96.9% minted | 5 | ~11.1 years | 678,125,000 | 96.9% |
| **99%+ minted** | **6** | **~13 years** | **689,062,500** | **98.4%** |

**99% of block rewards are emitted by approximately year 13.**

## Reward Distribution

Block rewards are distributed to **all active validators** proportionally to their stake:

```
validator_reward = (effective_reward × validator_stake) / total_stake
```

### Example: Equal Stake

With 4 validators each staking 1,000,000 PRIM:

- Total stake = 4,000,000 PRIM
- Block reward = 10 PRIM
- Each validator = 10 × (1,000,000 / 4,000,000) = **2.5 PRIM per block**

### Example: Unequal Stake

| Validator | Stake | Share | Reward/Block |
|-----------|-------|-------|--------------|
| A | 5,000,000 | 50% | 5.0 PRIM |
| B | 3,000,000 | 30% | 3.0 PRIM |
| C | 1,500,000 | 15% | 1.5 PRIM |
| D | 500,000 | 5% | 0.5 PRIM |
| **Total** | **10,000,000** | **100%** | **10.0 PRIM** |

### Supply Cap Enforcement

Before distributing any reward, the system checks:

```
remaining_supply = max_supply - total_minted
effective_reward = min(scheduled_reward, remaining_supply)
```

Individual validator rewards are calculated from `effective_reward`. This ensures `total_minted` never exceeds 1,000,000,000 PRIM.

### Rounding and Burns

Due to integer division with 18-decimal precision, the sum of individual rewards may be slightly less than the effective reward. The difference is **implicitly burned**—typically negligible (0–2 wei per block) but enforces the supply cap.

## Summary

| Topic | Summary |
|-------|---------|
| **Max supply** | 1 billion PRIM |
| **Block rewards** | 70% of supply, 10 PRIM/block initially |
| **Halving** | Every 35M blocks (~2.22 years) |
| **99% emission** | ~year 13 |
| **Distribution** | Proportional to validator stake |
| **Allocations** | 70% rewards, 10% ecosystem, 10% foundation, 5% team, 5% sales |
