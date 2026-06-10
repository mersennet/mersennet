# Mersennet Solidity Example Contracts

This directory contains example Solidity contracts that demonstrate **atomic EVM ↔ CLOB interaction** via the Prime Orders precompile at address `0x0100`. These strategies are **impossible on Hyperliquid** and other chains where the CLOB and EVM run asynchronously.

## CLOB Precompile at 0x0100

Mersennet embeds a native CLOB (Central Limit Order Book) matching engine that is **synchronously callable from EVM contracts**. The precompile at `0x0100` exposes:

| Selector | Function | Description |
|----------|----------|-------------|
| `placeOrder(marketId, side, price, amount, tif)` | External | Place limit order; fills **immediately** in same tx |
| `cancelOrder(orderId)` | External | Cancel open order |
| `getPosition(marketId, trader)` | View | Get position size and avg entry |
| `depositCollateral(marketId, amount)` | External | Deposit margin |
| `withdrawCollateral(marketId, amount)` | External | Withdraw margin |

**Key property:** When a contract calls `placeOrder`, any matching liquidity is filled **in the same transaction**. The contract can then read `getPosition`, place follow-up orders, or call other contracts — all atomically.

## Why This Is Impossible on Hyperliquid

On Hyperliquid (and similar architectures):

- **EVM** and **perpetuals CLOB** run on separate systems
- An EVM contract cannot place an order and get filled in the same block
- Cross-domain messaging (e.g. IBC, bridges) introduces latency and async semantics
- You cannot build **atomic** strategies like: "buy on CLOB, sell on AMM, all-or-nothing"

On Mersennet, the CLOB is a **precompile** — a native extension of the EVM. One transaction can:

1. Deposit collateral
2. Place a limit order
3. Get filled (if liquidity exists)
4. Read the new position
5. Call an AMM to hedge
6. All revert if any step fails

## Layout

| Path | Contents |
|---|---|
| `src/primeorders/` | CLOB precompile example strategies (VaultStrategy, AtomicArbitrage, SmartContractMM) |
| `src/foundation/` | WPRIM, Multicall3, MockERC20 |
| `src/dex/` | PrimeSwap V2-style AMM (factory, pair, router) |
| `src/zk/` | Ethereum-side ZK bridge — Groth16Verifier + PrimeChainBridge (see [`src/zk/README.md`](src/zk/README.md)) |
| `test/` | Foundry tests — 49 tests total, incl. 21 in `test/zk/` (`forge test`) |

## Contracts Overview

### VaultStrategy.sol

- **Purpose:** Simple market-making vault that updates bid/ask quotes around a mid price
- **Atomic flow:** Cancel old orders → Place new bid + ask in one tx
- **Use case:** Automated quoting for vaults, index products, or structured products

### AtomicArbitrage.sol

- **Purpose:** Buy on CLOB, sell on AMM (or vice versa) in a single transaction
- **Atomic flow:** Place IOC order on CLOB → Verify fill → Swap on AMM
- **Use case:** MEV-resistant arbitrage between CLOB and AMM pools

### SmartContractMM.sol

- **Purpose:** Full grid market-making with inventory management
- **Features:**
  - N levels on each side of the book
  - Spread adjustment based on net position
  - Automatic rebalancing when inventory exceeds threshold
  - P&L tracking
- **Use case:** On-chain market making without off-chain bots

## Deployment and Testing

### Prerequisites

- [Foundry](https://book.getfoundry.sh/) or [Hardhat](https://hardhat.org/)
- Mersennet node (testnet or local)

### Deploy with Foundry

```bash
cd contracts
forge create src/primeorders/VaultStrategy.sol:VaultStrategy \
  --constructor-args 1 50 1000 \
  --rpc-url https://rpc.primechain.xyz
```

Constructor args for `VaultStrategy`: `(marketId, spreadBps, maxPositionSize)`.

### Deploy with Hardhat

```javascript
const VaultStrategy = await ethers.getContractFactory("VaultStrategy");
const vault = await VaultStrategy.deploy(1, 50, 1000);
await vault.deployed();
```

### Optional: publish a public code-hash attestation

After privacy activation, raw bytecode is not exposed over public RPC.
If a project wants users to verify a deployed contract, the recorded
deployer can opt in by calling the code-publication precompile at
`0x0000000000000000000000000000000000000202`.

Conceptually, the Hardhat flow is:

1. Deploy the contract normally.
2. Wait for the deployment receipt and get the contract address.
3. Call `publishCodeHash(address,string)` on the precompile from the
   same deployer address.
4. Users verify against `prime_getCodeHash` / `prime_getCodeAttestation`
   by recompiling locally and comparing the runtime bytecode hash.

Example sketch:

```javascript
const publication = await ethers.getContractAt(
  ["function publishCodeHash(address contractAddr, string metadataUri) external"],
  "0x0000000000000000000000000000000000000202"
);

await publication.publishCodeHash(
  await vault.getAddress(),
  "ipfs://<build-metadata>"
);
```

If the deployer does not make that second call, the contract remains
private-by-default from the public RPC perspective: `prime_getCodeHash`
returns `null` and raw `getCode` remains disabled after privacy mode.

### Testing

1. **Fund the contract** with native token (for collateral)
2. **Call `deposit()`** (or `depositAndInit()` for SmartContractMM) with `msg.value`
3. **Call `updateQuotes(midPrice)`** or equivalent to start quoting

### Gas Estimates

| Operation | Est. Gas |
|-----------|----------|
| `placeOrder` (no fill) | ~50,000 |
| `placeOrder` (with fill) | ~80,000–150,000 |
| `cancelOrder` | ~30,000 |
| `updateQuotes` (VaultStrategy) | ~150,000–200,000 |
| `arbitrage` (AtomicArbitrage) | ~200,000–300,000 |
| `rebalance` (SmartContractMM) | ~300,000–500,000 |

Actual gas depends on order book depth and number of fills.

## Security Notes

- Contracts use `onlyOwner` for sensitive operations
- Precompile address `0x0100` is fixed in Mersennet; do not deploy on other chains
- Always verify `getPosition` after `placeOrder` when expecting fills
- Consider reentrancy if integrating with external protocols
