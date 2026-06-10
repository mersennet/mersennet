---
title: "ERC-20 Token Guide"
---

This guide walks you through deploying and interacting with an ERC-20 token on Mersennet (Chain ID 7919).

## Network Details

| Parameter | Value |
|-----------|-------|
| Chain ID | 7919 |
| RPC URL | `http://46.225.30.187:8545` |
| Native Token | PRIM (18 decimals) |

## Full Solidity Contract

Here is a complete ERC-20 implementation you can deploy:

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract MersennetToken {
    string public name;
    string public symbol;
    uint8 public decimals = 18;
    uint256 public totalSupply;

    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    constructor(
        string memory _name,
        string memory _symbol,
        uint256 _initialSupply
    ) {
        name = _name;
        symbol = _symbol;
        totalSupply = _initialSupply * 10 ** decimals;
        balanceOf[msg.sender] = totalSupply;
        emit Transfer(address(0), msg.sender, totalSupply);
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        return _transfer(msg.sender, to, amount);
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        uint256 currentAllowance = allowance[from][msg.sender];
        require(currentAllowance >= amount, "ERC20: insufficient allowance");
        unchecked {
            allowance[from][msg.sender] = currentAllowance - amount;
        }
        return _transfer(from, to, amount);
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function _transfer(address from, address to, uint256 amount) internal returns (bool) {
        require(to != address(0), "ERC20: transfer to zero");
        uint256 fromBalance = balanceOf[from];
        require(fromBalance >= amount, "ERC20: insufficient balance");
        unchecked {
            balanceOf[from] = fromBalance - amount;
            balanceOf[to] += amount;
        }
        emit Transfer(from, to, amount);
        return true;
    }
}
```

## Deployment Steps

### Option 1: Hardhat

1. Create a Hardhat project and add the Mersennet network (see [Deploy with Hardhat](/developers/quick-start/hardhat)).
2. Save the contract as `contracts/MersennetToken.sol`.
3. Deploy:

```typescript
const MersennetToken = await ethers.getContractFactory("MersennetToken");
const token = await MersennetToken.deploy("My Token", "MTK", 1_000_000);
await token.waitForDeployment();
console.log("Deployed:", await token.getAddress());
```

### Option 2: Remix

1. Go to [Remix](https://remix.ethereum.org).
2. Create a new file, paste the contract, and compile.
3. In the Deploy tab, select **Injected Provider - MetaMask**.
4. Add Mersennet to MetaMask (Chain ID 7919, RPC `http://46.225.30.187:8545`).
5. Get testnet PRIM from the [faucet](http://46.225.30.187:4003).
6. Deploy and enter constructor args: `"My Token"`, `"MTK"`, `1000000`.

## Interacting with the Deployed Token

### Using ethers.js

```javascript
const { ethers } = require("ethers");

const provider = new ethers.JsonRpcProvider("http://46.225.30.187:8545", 7919);
const wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);

const TOKEN_ADDRESS = "0x..."; // Your deployed token address

const ERC20_ABI = [
  "function balanceOf(address) view returns (uint256)",
  "function transfer(address to, uint256 amount) returns (bool)",
  "function approve(address spender, uint256 amount) returns (bool)",
  "function allowance(address owner, address spender) view returns (uint256)",
];

const token = new ethers.Contract(TOKEN_ADDRESS, ERC20_ABI, wallet);

// Read balance
const balance = await token.balanceOf(wallet.address);
console.log("Balance:", ethers.formatEther(balance));

// Transfer tokens
const tx = await token.transfer("0xRecipientAddress", ethers.parseEther("100"));
await tx.wait();
console.log("Transfer confirmed");

// Approve spender (e.g., for DEX)
await token.approve("0xSpenderAddress", ethers.MaxUint256);
```

### Using cast (Foundry)

```bash
# Get balance
cast call 0xTokenAddress "balanceOf(address)(uint256)" 0xYourAddress \
  --rpc-url http://46.225.30.187:8545

# Encode transfer (for use with eth_sendTransaction)
cast calldata "transfer(address,uint256)" 0xRecipient 1000000000000000000
```

## Integrate with PrimeSwap

To list your token on [PrimeSwap](/ecosystem/primeswap), you need:

1. **WPRIM** for PRIM pairs: `0x079bf1207b51acda83e2e8178344f62a883f8479`
2. **PrimeSwapRouter** for adding liquidity: `0x9f337f433e71ce969b991511f1dcd3d0622116bb`

Approve the router and add liquidity:

```javascript
const ROUTER = "0x9f337f433e71ce969b991511f1dcd3d0622116bb";
await token.approve(ROUTER, ethers.MaxUint256);
// Then call router.addLiquidity
```

## Deployed Token Addresses

These are pre-deployed tokens on Mersennet testnet:

| Token | Address | Use Case |
|-------|---------|----------|
| WPRIM | `0x079bf1207b51acda83e2e8178344f62a883f8479` | Wrapped PRIM for DEX |
| MockUSDC | `0xb22f77d89122e9e3784bfd3eee9616273f38238d` | Test stablecoin |
| MockUSDT | `0x877feca38919acd7aaf7cb81f100e0454aa95c17` | Test stablecoin |
| MockDAI | `0xb88d63a65691effbf4b6808325b1588912c15cf4` | Test stablecoin |
