---
slug: /developers/contracts/defi
sidebar_position: 3
title: "DeFi Integration"
---

# DeFi Integration

Mersennet has an on-chain DEX called **PrimeSwap** — a Uniswap V2–style AMM. This guide shows how to swap tokens, add liquidity, and query prices using the deployed Router contract.

## Deployed Contract Addresses

| Contract | Address |
|----------|---------|
| PrimeSwapFactory | `0x63f7a64db6d2b965189b8b48b7435668021f6b17` |
| PrimeSwapRouter | `0x9f337f433e71ce969b991511f1dcd3d0622116bb` |
| WPRIM | `0x079bf1207b51acda83e2e8178344f62a883f8479` |
| MockUSDC | `0xb22f77d89122e9e3784bfd3eee9616273f38238d` |
| MockUSDT | `0x877feca38919acd7aaf7cb81f100e0454aa95c17` |
| MockDAI | `0xb88d63a65691effbf4b6808325b1588912c15cf4` |

## Router ABI (Key Functions)

```javascript
const ROUTER_ABI = [
  "function getAmountsOut(uint256 amountIn, address[] path) view returns (uint256[] amounts)",
  "function getAmountsIn(uint256 amountOut, address[] path) view returns (uint256[] amounts)",
  "function swapExactTokensForTokens(uint256 amountIn, uint256 amountOutMin, address[] path, address to, uint256 deadline) returns (uint256[] amounts)",
  "function swapExactPRIMForTokens(uint256 amountOutMin, address[] path, address to, uint256 deadline) payable returns (uint256[] amounts)",
  "function swapExactTokensForPRIM(uint256 amountIn, uint256 amountOutMin, address[] path, address to, uint256 deadline) returns (uint256[] amounts)",
  "function addLiquidity(address tokenA, address tokenB, uint256 amountADesired, uint256 amountBDesired, uint256 amountAMin, uint256 amountBMin, address to, uint256 deadline) returns (uint256 amountA, uint256 amountB, uint256 liquidity)",
  "function addLiquidityPRIM(address token, uint256 amountTokenDesired, uint256 amountTokenMin, uint256 amountPRIMMin, address to, uint256 deadline) payable returns (uint256 amountToken, uint256 amountPRIM, uint256 liquidity)",
  "function removeLiquidity(address tokenA, address tokenB, uint256 liquidity, uint256 amountAMin, uint256 amountBMin, address to, uint256 deadline) returns (uint256 amountA, uint256 amountB)",
  "function removeLiquidityPRIM(address token, uint256 liquidity, uint256 amountTokenMin, uint256 amountPRIMMin, address to, uint256 deadline) returns (uint256 amountToken, uint256 amountPRIM)",
  "function factory() view returns (address)",
  "function WPRIM() view returns (address)",
];
```

## Query Prices

Get the expected output amount for a swap (without executing):

```javascript
const { ethers } = require("ethers");

const RPC_URL = "http://46.225.30.187:8545";
const CHAIN_ID = 7919;
const ROUTER = "0x9f337f433e71ce969b991511f1dcd3d0622116bb";
const WPRIM = "0x079bf1207b51acda83e2e8178344f62a883f8479";
const MOCK_USDC = "0xb22f77d89122e9e3784bfd3eee9616273f38238d";

const provider = new ethers.JsonRpcProvider(RPC_URL, CHAIN_ID);
const router = new ethers.Contract(ROUTER, ROUTER_ABI, provider);

// How much USDC for 1 PRIM? (path: WPRIM -> USDC)
const amountIn = ethers.parseEther("1");
const path = [WPRIM, MOCK_USDC];
const amounts = await router.getAmountsOut(amountIn, path);
console.log("1 PRIM -> USDC:", ethers.formatUnits(amounts[1], 6));

// How much PRIM for 100 USDC? (path: USDC -> WPRIM)
const amountOut = ethers.parseUnits("100", 6);
const pathReverse = [MOCK_USDC, WPRIM];
const amountsIn = await router.getAmountsIn(amountOut, pathReverse);
console.log("100 USDC needs PRIM:", ethers.formatEther(amountsIn[0]));
```

## Swap Tokens

### Token → Token (e.g., USDC → USDT)

```javascript
const { ethers } = require("ethers");

const provider = new ethers.JsonRpcProvider(RPC_URL, CHAIN_ID);
const wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);
const router = new ethers.Contract(ROUTER, ROUTER_ABI, wallet);

const USDC = "0xb22f77d89122e9e3784bfd3eee9616273f38238d";
const USDT = "0x877feca38919acd7aaf7cb81f100e0454aa95c17";

const amountIn = ethers.parseUnits("10", 6); // 10 USDC
const path = [USDC, USDT];
const amounts = await router.getAmountsOut(amountIn, path);
const amountOutMin = amounts[1] * 95n / 100n; // 5% slippage

const deadline = Math.floor(Date.now() / 1000) + 60 * 20; // 20 min

// Approve router to spend USDC
const erc20 = new ethers.Contract(USDC, ["function approve(address,uint256) returns (bool)"], wallet);
await erc20.approve(ROUTER, amountIn);

const tx = await router.swapExactTokensForTokens(
  amountIn,
  amountOutMin,
  path,
  wallet.address,
  deadline
);
await tx.wait();
console.log("Swap complete");
```

### PRIM → Token (e.g., PRIM → USDC)

```javascript
const amountOutMin = 0n; // Or use getAmountsOut for a minimum
const path = [WPRIM, MOCK_USDC];
const value = ethers.parseEther("0.5"); // 0.5 PRIM
const deadline = Math.floor(Date.now() / 1000) + 60 * 20;

const tx = await router.swapExactPRIMForTokens(
  amountOutMin,
  path,
  wallet.address,
  deadline,
  { value }
);
await tx.wait();
```

### Token → PRIM (e.g., USDC → PRIM)

```javascript
const amountIn = ethers.parseUnits("50", 6); // 50 USDC
const path = [MOCK_USDC, WPRIM];
const amounts = await router.getAmountsOut(amountIn, path);
const amountOutMin = amounts[1] * 95n / 100n;

await erc20.approve(ROUTER, amountIn);

const tx = await router.swapExactTokensForPRIM(
  amountIn,
  amountOutMin,
  path,
  wallet.address,
  deadline
);
await tx.wait();
```

## Add Liquidity

### Add ERC-20 / ERC-20 (e.g., USDC/USDT)

```javascript
const amountADesired = ethers.parseUnits("100", 6); // 100 USDC
const amountBDesired = ethers.parseUnits("100", 6); // 100 USDT
const amountAMin = amountADesired * 95n / 100n;
const amountBMin = amountBDesired * 95n / 100n;
const deadline = Math.floor(Date.now() / 1000) + 60 * 20;

await erc20A.approve(ROUTER, amountADesired);
await erc20B.approve(ROUTER, amountBDesired);

const tx = await router.addLiquidity(
  USDC,
  USDT,
  amountADesired,
  amountBDesired,
  amountAMin,
  amountBMin,
  wallet.address,
  deadline
);
await tx.wait();
```

### Add PRIM / Token (e.g., PRIM/USDC)

```javascript
const amountTokenDesired = ethers.parseUnits("100", 6);
const amountTokenMin = amountTokenDesired * 95n / 100n;
const amountPRIMMin = ethers.parseEther("0.5");
const deadline = Math.floor(Date.now() / 1000) + 60 * 20;

await erc20.approve(ROUTER, amountTokenDesired);

const tx = await router.addLiquidityPRIM(
  MOCK_USDC,
  amountTokenDesired,
  amountTokenMin,
  amountPRIMMin,
  wallet.address,
  deadline,
  { value: ethers.parseEther("1") } // PRIM sent as msg.value
);
await tx.wait();
```

## Remove Liquidity

```javascript
const pairAddress = await factory.getPair(USDC, USDT);
const lpToken = new ethers.Contract(pairAddress, ["function approve(address,uint256) returns (bool)"], wallet);

const liquidity = ethers.parseEther("1"); // Amount of LP tokens
await lpToken.approve(ROUTER, liquidity);

const tx = await router.removeLiquidity(
  USDC,
  USDT,
  liquidity,
  0n,
  0n,
  wallet.address,
  deadline
);
await tx.wait();
```

## Factory: Get Pair Address

```javascript
const FACTORY_ABI = ["function getPair(address, address) view returns (address)"];
const factory = new ethers.Contract(
  "0x63f7a64db6d2b965189b8b48b7435668021f6b17",
  FACTORY_ABI,
  provider
);

const pair = await factory.getPair(WPRIM, MOCK_USDC);
console.log("WPRIM/USDC pair:", pair);
```

## Pair: Get Reserves

```javascript
const PAIR_ABI = [
  "function getReserves() view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast)",
  "function token0() view returns (address)",
  "function token1() view returns (address)",
];

const pair = new ethers.Contract(pairAddress, PAIR_ABI, provider);
const [reserve0, reserve1] = await pair.getReserves();
const token0 = await pair.token0();
const token1 = await pair.token1();

console.log("Reserves:", reserve0.toString(), reserve1.toString());
```
