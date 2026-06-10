---
title: "Deploy with Foundry"
---

This guide explains how to build and deploy smart contracts to Mersennet using Foundry. Mersennet's `eth_sendRawTransaction` accepts standard Ethereum RLP-encoded transactions (legacy, EIP-2930, and EIP-1559) in addition to its own custom binary format, so `forge create` and `cast send` work with locally signed transactions.

## Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation) installed
- A funded wallet (get testnet MRSN from the [faucet](http://46.225.30.187:4003))

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

## Deploy with forge create

```bash
forge create src/MyToken.sol:MyToken \
  --rpc-url http://46.225.30.187:8545 \
  --private-key $PRIVATE_KEY \
  --legacy \
  --constructor-args 1000000
```

The `--legacy` flag is recommended: Mersennet implements an EIP-1559 base fee but no priority tip (`eth_maxPriorityFeePerGas` returns `0x0`), so legacy gas-price transactions are the simplest fit.

**Other deployment methods:**

- `eth_sendTransaction` — Requires the RPC node to have the deployer account unlocked
- `prime_sendTransaction` — Mersennet–specific method for sending transactions

## Node.js Deployment Helper

Alternatively, use a Node.js script with ethers.js, which signs locally and submits via `eth_sendRawTransaction`.

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
ethers.js v6 signs the transaction locally and submits it with `eth_sendRawTransaction`. Mersennet accepts standard Ethereum RLP-encoded transactions, so this works out of the box.
:::

## Cast

`cast` works for both reads and writes:

```bash
# Get balance
cast balance 0xYourAddress --rpc-url http://46.225.30.187:8545

# Call a view function
cast call 0xContractAddress "totalSupply()(uint256)" --rpc-url http://46.225.30.187:8545

# Get chain ID
cast chain-id --rpc-url http://46.225.30.187:8545

# Send a transaction (signed locally)
cast send 0xRecipient --value 1ether --legacy \
  --rpc-url http://46.225.30.187:8545 --private-key $PRIVATE_KEY
```

## Summary

| Operation | Supported | Notes |
|-----------|-----------|-------|
| `forge build` | ✅ | |
| `forge test` | ✅ | Against local Anvil or Prime RPC |
| `forge create` | ✅ | Use `--legacy` |
| `cast call` | ✅ | Read-only |
| `cast send` | ✅ | Use `--legacy` |
| `eth_sendTransaction` | ✅ | Alternative when the node has an unlocked account |
| `eth_feeHistory` | ✅ | Supported (priority fee rewards are always 0) |
