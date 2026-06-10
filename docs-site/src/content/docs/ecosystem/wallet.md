---
title: "PrimeXDC Wallet"
---

**PrimeXDC Wallet** is the native browser extension wallet for Mersennet. It provides send/receive, token management, dApp connectivity, and transaction signing—optimized for the Mersennet ecosystem.

## Overview

| Feature | PrimeXDC Wallet |
|---------|-----------------|
| **Platform** | Chrome/Chromium extension |
| **Standard** | EIP-1193 (MetaMask-compatible) |
| **Chain** | Mersennet (Chain ID 7919) supported out of the box |
| **Stack** | React, ethers.js, Chrome Manifest V3 |
| **Status** | Built |

## Features

### Send & Receive MRSN

- Send native MRSN to any address on Mersennet.
- Receive MRSN by sharing your wallet address.
- View transaction history and confirmations.

### Token Management

- View ERC-20 token balances (WMRSN, USDC, USDT, etc.).
- Send and receive tokens.
- Add custom tokens by contract address.

### dApp Connectivity

- Connect to Mersennet dApps via the standard Ethereum provider (`window.ethereum`).
- Approve transactions and sign messages.
- Compatible with wagmi, ethers.js, viem, and other Web3 libraries.

### Transaction Signing

- Sign transactions with your private key (stored locally, never transmitted).
- Approve token allowances for DeFi protocols.
- Sign typed data (EIP-712) for Seaport and other protocols.

## Installation

1. Install the PrimeXDC Wallet browser extension (Chrome/Chromium).
2. Create a new wallet or import via seed phrase.
3. Mersennet testnet (Chain ID 7919) is preconfigured.
4. Use the [Faucet](http://46.225.30.187:4003) to get testnet MRSN.

:::tip
PrimeXDC Wallet uses the same provider interface as MetaMask. dApps that support MetaMask can connect to PrimeXDC Wallet when it's installed.
:::

## Mobile Wallet

**PrimeXDC Mobile** is a React Native wallet for iOS and Android:

- Same functionality as the browser extension.
- APK available for Android.
- Send, receive, token management, and dApp connectivity on mobile.

## Supported dApps

PrimeXDC Wallet works with all Mersennet dApps, including:

- [PrimeSwap](/ecosystem/primeswap) — Swap and add liquidity
- [PrimeFi](/ecosystem/primefi) — Supply and borrow (when deployed)
- [Primeport](/ecosystem/primeport) — NFT marketplace (when deployed)
- Block Explorer — View transactions and addresses

## Alternative: MetaMask

If you prefer MetaMask, you can add Mersennet manually. See [Wallet Setup](/getting-started/wallet-setup) for instructions. PrimeXDC Wallet offers a tailored experience and may include Mersennet–specific features (e.g., PrimeOrders integration) in future updates.

## Security

- Private keys and seed phrases never leave your device.
- No data is sent to external servers for key management.
- Always verify contract addresses and transaction details before signing.

## Related Resources

- [Wallet Setup](/getting-started/wallet-setup) — Add Mersennet to MetaMask or PrimeXDC
- [Faucet](/getting-started/faucet) — Get testnet MRSN
- [First Transaction](/getting-started/first-transaction) — Send your first MRSN
