---
slug: /developers/quick-start/foundry
sidebar_position: 2
title: "Deploy with Foundry"
---

# Deploy with Foundry

This guide explains how to build and deploy smart contracts to Prime Chain using Foundry. **Important:** Prime Chain uses a custom raw transaction format (not standard RLP), so `forge create` with `eth_sendRawTransaction` will not work. Use `eth_sendTransaction` or `prime_sendTransaction` instead.

## Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation) installed
- A funded wallet (get testnet PRIM from the [faucet](http://46.225.30.187:8080))

## Foundry Configuration

Create or update `foundry.toml` in your project root:

```toml
[profile.default]
src = "src"
out = "out"
libs = ["lib"]
solc = "0.8.20"
optimizer = true
optimizer_runs = 200
evm_version = "shanghai"

[rpc_endpoints]
prime_testnet = "http://46.225.30.187:8545"
```

## Build Your Contracts

```bash
forge build
```

Use the `--legacy` flag if your tooling expects legacy transaction format:

```bash
forge build --legacy
```

## Deployment Limitation

Prime Chain does **not** support standard RLP-encoded raw transactions via `eth_sendRawTransaction`. Tools that sign transactions locally and send them as raw hex (including `forge create`) will fail.

**Supported deployment methods:**

- `eth_sendTransaction` — Requires the RPC node to have the deployer account unlocked
- `prime_sendTransaction` — Prime Chain–specific method for sending transactions

## Node.js Deployment Helper

Use a Node.js script with ethers.js to deploy via `eth_sendTransaction` (with a wallet) or by having the node sign for an unlocked account.

### 1. Create a deploy script

Create `scripts/deploy.js`:

```javascript
const { ethers } = require("ethers");

const RPC_URL = "http://46.225.30.187:8545";
const CHAIN_ID = 7919;

async function main() {
  // Option A: Use private key (ethers signs, but we use eth_sendTransaction via a custom provider)
  // Option B: Use a node with unlocked account
  const provider = new ethers.JsonRpcProvider(RPC_URL, CHAIN_ID);
  const wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);

  const factory = new ethers.ContractFactory(
    require("./MyToken_abi.json"),
    require("./MyToken_bytecode.json"),
    wallet
  );

  const contract = await factory.deploy(1_000_000);
  await contract.waitForDeployment();

  console.log("Deployed to:", await contract.getAddress());
}

main().catch(console.error);
```

### 2. Compile and extract artifacts

```bash
forge build
```

Then create a script that reads Foundry artifacts:

```javascript
// deploy-with-forge-artifacts.js
const { ethers } = require("ethers");
const fs = require("fs");
const path = require("path");

const RPC_URL = "http://46.225.30.187:8545";
const CHAIN_ID = 7919;

async function main() {
  const provider = new ethers.JsonRpcProvider(RPC_URL, CHAIN_ID);
  const wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);

  const artifactPath = path.join(__dirname, "../out/MyToken.sol/MyToken.json");
  const artifact = JSON.parse(fs.readFileSync(artifactPath, "utf8"));

  const factory = new ethers.ContractFactory(
    artifact.abi,
    artifact.bytecode.object,
    wallet
  );

  const contract = await factory.deploy(1_000_000);
  await contract.waitForDeployment();

  console.log("MyToken deployed to:", await contract.getAddress());
}

main().catch(console.error);
```

### 3. Run the deploy script

```bash
npm install ethers
PRIVATE_KEY=0x_your_key node deploy-with-forge-artifacts.js
```

:::tip
ethers.js v6 uses `eth_sendTransaction` under the hood when you call `contract.deploy()`. The library signs the transaction and sends it. However, if Prime Chain expects a custom format for `eth_sendRawTransaction`, ethers may still use that. In that case, ensure your RPC supports `eth_sendTransaction` with a signed payload, or use a node with an unlocked account for deployment.
:::

## Alternative: Cast for Read-Only Operations

For read-only operations, `cast` works normally:

```bash
# Get balance
cast balance 0xYourAddress --rpc-url http://46.225.30.187:8545

# Call a view function
cast call 0xContractAddress "totalSupply()(uint256)" --rpc-url http://46.225.30.187:8545

# Get chain ID
cast chain-id --rpc-url http://46.225.30.187:8545
```

## Summary

| Operation | Supported | Notes |
|-----------|-----------|-------|
| `forge build` | ✅ | Use `--legacy` if needed |
| `forge test` | ✅ | Against local Anvil or Prime RPC |
| `forge create` | ❌ | Custom tx format; use Node.js helper |
| `cast call` | ✅ | Read-only |
| `cast send` | ❌ | Same limitation as `forge create` |
| `eth_sendTransaction` | ✅ | Use for deployment |
| `eth_feeHistory` | ❌ | Not supported on Prime Chain |
