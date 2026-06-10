---
title: "PrimeFi Lending"
---

**PrimeFi** is an Aave-style lending and borrowing protocol built for Mersennet. It enables users to supply assets to earn interest and borrow against collateral—powering the DeFi credit layer of the ecosystem.

## Overview

| Feature | PrimeFi |
|---------|---------|
| **Model** | Aave-style (pool-based) |
| **Supply** | Deposit assets, earn variable APY |
| **Borrow** | Over-collateralized loans |
| **Liquidations** | Automatic when health factor &lt; 1 |
| **Status** | Contracts built, ready for deployment |

:::note
PrimeFi smart contracts are built and audited. They are ready for deployment to Mersennet testnet. The PrimeFi UI (v2) and liquidator bot are also built and will be deployed alongside the contracts.
:::

## How It Works

### Supply (Deposit)

1. Approve the PrimeFi pool contract to spend your tokens.
2. Call `supply(asset, amount, onBehalfOf)` to deposit.
3. Receive **aTokens** (interest-bearing receipt tokens) in return.
4. Your balance grows as interest accrues—redeem aTokens anytime for underlying + interest.

### Borrow

1. Supply collateral first (e.g., WPRIM, USDC).
2. Call `borrow(asset, amount, interestRateMode, onBehalfOf)`.
3. Receive borrowed assets to your wallet.
4. Interest accrues on the borrowed amount. Repay with `repay()` to reduce debt and free collateral.

### Interest Rates

- **Variable rate** — Fluctuates based on utilization (how much of the pool is borrowed).
- **Stable rate** (if supported) — More predictable, may have different parameters.
- Higher utilization → higher borrow APY, higher supply APY for lenders.

### Liquidations

When a borrower's **health factor** drops below 1 (e.g., collateral value falls or debt value rises), the position becomes liquidatable:

- Liquidators can repay part or all of the debt.
- In exchange, they receive collateral at a discount (liquidation bonus).
- This keeps the protocol solvent and incentivizes risk management.

## Supported Assets (Planned)

PrimeFi will support the core Mersennet assets:

| Asset | Use Case |
|-------|----------|
| **WPRIM** | Collateral, supply |
| **USDC** | Collateral, supply, borrow |
| **USDT** | Collateral, supply, borrow |
| **DAI** | Collateral, supply, borrow |

Exact support depends on deployment configuration and oracle integration.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    PrimeFi Protocol                      │
├─────────────────────────────────────────────────────────┤
│  Pool (LendingPool)                                      │
│  - supply() / withdraw()                                 │
│  - borrow() / repay()                                   │
│  - liquidationCall()                                     │
├─────────────────────────────────────────────────────────┤
│  PoolAddressesProvider  │  Oracle  │  aToken / debtToken  │
└─────────────────────────────────────────────────────────┘
```

## PrimeFi Components

| Component | Description |
|-----------|-------------|
| **PrimeFi Contracts** | Solidity, Hardhat, Aave-style logic |
| **PrimeFi UI v2** | React 19, Vite, wagmi — lending frontend |
| **PrimeFi Liquidator** | Node.js bot for monitoring and liquidating unhealthy positions |
| **PrimeFi Omni** | Cross-chain lending via LayerZero v2 (contracts ready) |

## Integration with Mersennet

- **PrimeSwap** — Borrow stablecoins, swap on PrimeSwap, supply for yield.
- **PrimeOrders** — Future: collateralize positions, use CLOB for hedging.
- **WPRIM** — Native token wrapper used as collateral and supply asset.

## Deployment Status

| Component | Status |
|-----------|--------|
| Lending contracts | Ready for deployment |
| PrimeFi UI v2 | Built (dist/) |
| Liquidator bot | Ready |
| PrimeFi Omni (cross-chain) | Contracts ready |

Once deployed, contract addresses will be published in [Deployed Contracts](/resources/contracts).

## Related Resources

- [PrimeSwap DEX](/ecosystem/primeswap) — Swap and provide liquidity
- [Deployed Contracts](/resources/contracts) — Contract addresses when live
- [Network Information](/getting-started/network-info) — RPC and configuration
