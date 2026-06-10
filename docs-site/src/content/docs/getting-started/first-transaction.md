---
title: "Your First Transaction"
---

This guide walks you through sending MRSN, deploying a simple contract, and viewing it on the explorer.

## Prerequisites

- [Wallet configured](/getting-started/wallet-setup) with Mersennet testnet
- [Testnet MRSN](/getting-started/faucet) from the faucet

---

## Step 1: Send MRSN

### Using MetaMask

1. Click **Send** in MetaMask.
2. Enter the recipient address.
3. Enter the amount in MRSN.
4. Confirm the transaction.

### Using ethers.js

```javascript
import { ethers } from 'ethers';

const provider = new ethers.JsonRpcProvider('http://46.225.30.187:8545');
const wallet = new ethers.Wallet('YOUR_PRIVATE_KEY', provider);

const tx = await wallet.sendTransaction({
  to: '0xRecipientAddress',
  value: ethers.parseEther('0.1'),
  gasLimit: 21000,
});

console.log('Tx hash:', tx.hash);
await tx.wait();
console.log('Confirmed!');
```

> **Security Warning:** Never use real private keys in code. Use environment variables or a key management solution. Never commit private keys to version control.

---

## Step 2: Deploy a Simple Contract

Deploy a minimal Solidity contract using ethers.js. First, compile your contract with Hardhat or Foundry, then deploy:

```javascript
import { ethers } from 'ethers';

const provider = new ethers.JsonRpcProvider('http://46.225.30.187:8545');
const wallet = new ethers.Wallet('YOUR_PRIVATE_KEY', provider);

// Load compiled artifact (from Hardhat: artifacts/contracts/Counter.sol/Counter.json)
const artifact = require('./artifacts/contracts/Counter.sol/Counter.json');
const factory = new ethers.ContractFactory(artifact.abi, artifact.bytecode, wallet);

const contract = await factory.deploy();
await contract.waitForDeployment();

console.log('Deployed at:', await contract.getAddress());
```

> **Security Warning:** Never use real private keys in code. Use environment variables or a key management solution. Never commit private keys to version control.

**Minimal Solidity example (Counter.sol):**

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Counter {
    uint256 public count;

    function increment() external {
        count++;
    }
}
```

:::tip
For production deployments, use [Hardhat](/developers/quick-start/hardhat) or [Foundry](/developers/quick-start/foundry) to compile, test, and deploy. They handle compilation, gas estimation, and verification.
:::

---

## Step 3: View on the Explorer

1. Copy the transaction hash from your wallet or script output.
2. Open the [block explorer](http://46.225.30.187).
3. Paste the hash into the search bar.
4. View the transaction details, block number, gas used, and status.

For contract deployments, the explorer shows the contract address. You can click it to see the contract page and interact with it (if verified).

---

## Summary

| Action | Tool |
|--------|------|
| Send MRSN | MetaMask or `wallet.sendTransaction()` |
| Deploy contract | ethers.js, Hardhat, Foundry, or Remix |
| View tx | http://46.225.30.187 |

Next, explore the [developer guides](/developers/quick-start/hardhat) to deploy production contracts with Hardhat or Foundry.
