---
slug: /architecture/evm-compatibility
sidebar_position: 2
title: "EVM Compatibility"
---

# EVM Compatibility

Mersennet implements an **EVM-compatible** execution environment, allowing developers to deploy and run existing Ethereum smart contracts with minimal or no modification. This document describes the EVM implementation, supported features, and differences from Ethereum mainnet.

## Overview

| Aspect | Mersennet |
|--------|-------------|
| **EVM Version** | Shanghai |
| **Chain ID** | 7919 |
| **Token** | PRIM (18 decimals) |
| **Block Time** | ~1 second |

## Shanghai EVM

Mersennet targets the **Shanghai** EVM specification, which includes:

- All pre-Shanghai opcodes and semantics
- **PUSH0** (EIP-3855) — Push constant 0 onto the stack

This ensures compatibility with the vast majority of Solidity contracts and tooling (Hardhat, Foundry, Remix, and standard wallets).

## Supported Opcodes

Mersennet supports the standard Ethereum opcodes defined in the Shanghai spec, including:

- **Arithmetic**: ADD, SUB, MUL, DIV, MOD, etc.
- **Comparison**: LT, GT, SLT, SGT, EQ, etc.
- **Bitwise**: AND, OR, XOR, NOT, SHL, SHR, SAR
- **Crypto**: KECCAK256, ECRECOVER
- **Memory/Storage**: MSTORE, MLOAD, SLOAD, SSTORE
- **Control flow**: JUMP, JUMPI, PC, JUMPDEST
- **System**: CALL, DELEGATECALL, STATICCALL, CREATE, CREATE2
- **Block/Context**: BLOCKHASH, TIMESTAMP, NUMBER, etc.

## Precompiles

### Standard Ethereum Precompiles

Mersennet supports all standard Ethereum precompiles:

| Address | Precompile | Description |
|---------|------------|-------------|
| 0x01 | ecRecover | ECDSA signature recovery |
| 0x02 | SHA256 | SHA-256 hash |
| 0x03 | RIPEMD160 | RIPEMD-160 hash |
| 0x04 | identity | Identity (copy input to output) |
| 0x05 | modexp | Modular exponentiation |
| 0x06 | ecAdd | Elliptic curve point addition |
| 0x07 | ecMul | Elliptic curve scalar multiplication |
| 0x08 | ecPairing | BN254 pairing |

### Mersennet Extension: PrimeOrders

Mersennet adds a **custom precompile** for the native order matching engine:

| Address | Precompile | Description |
|---------|------------|-------------|
| **0x0100** | **PrimeOrders** | Native on-chain CLOB |

See [PrimeOrders (On-chain CLOB)](/architecture/prime-orders) for full documentation.

## Gas Metering

Mersennet uses gas metering consistent with Ethereum:

- Each opcode has a cost (e.g. ADD = 3, SSTORE = 20,000 for cold)
- Transactions specify a `gasLimit`; execution stops if gas is exhausted
- Gas is paid in PRIM (converted at the current gas price)

Gas costs align with Ethereum's Shanghai spec for predictable behavior when porting contracts.

## Differences from Ethereum Mainnet

### Transaction Format

Mersennet uses a **custom binary transaction format** alongside standard Ethereum RLP-encoded (EIP-155) transactions — `eth_sendRawTransaction` accepts both. Key points:

- Transactions include: `from`, `to`, `value`, `data`, `gasLimit`, `gasPrice`, `nonce`
- Chain ID 7919 is used for replay protection (testnet)

### EIP-1559 Base Fee (No Priority Tip)

Mersennet implements EIP-1559's dynamic base fee: the base fee adjusts each block based on target utilization (`fee_elasticity_multiplier: 2`, `fee_max_change_denominator: 8`, i.e. up to 12.5% change per block). There is **no separate priority tip** — `eth_maxPriorityFeePerGas` returns `0x0`.

Validators earn primarily from **block rewards**, not transaction fees. Fee market parameters can be updated via governance.

### Block Structure

Mersennet blocks include additional fields beyond standard Ethereum:

- **Rewards**: Per-validator block reward distribution
- **PrimeOrders events**: Order submissions, trades, liquidations (if applicable)

The RPC and block structure expose these for explorers and indexers.

### Native Token

- **Ethereum**: ETH (18 decimals)
- **Mersennet**: PRIM (18 decimals)

Same decimal precision, so contract logic that assumes 18 decimals works unchanged.

## Summary

| Feature | Status |
|---------|--------|
| Shanghai EVM | ✅ Supported |
| Standard opcodes | ✅ Supported |
| Standard precompiles | ✅ Supported |
| PrimeOrders precompile (0x0100) | ✅ Supported |
| Custom tx format | ✅ Custom binary + Ethereum RLP (EIP-155) both accepted |
| EIP-1559 | ✅ Dynamic base fee (no priority tip) |
| Gas metering | ✅ Ethereum-compatible |

Mersennet is designed for **EVM ecosystem compatibility**—deploy your contracts, use your tools, and leverage the native PrimeOrders precompile for advanced DeFi strategies.
