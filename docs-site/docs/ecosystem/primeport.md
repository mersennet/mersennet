---
slug: /ecosystem/primeport
sidebar_position: 3
title: "Primeport NFT Marketplace"
---

# Primeport NFT Marketplace

**Primeport** is the native NFT marketplace on Mersennet, built on OpenSea's Seaport protocol. It supports ERC-721 and ERC-1155 NFTs with listing, buying, selling, and auction functionality.

## Overview

| Feature | Primeport |
|---------|-----------|
| **Protocol** | Seaport-based |
| **Standards** | ERC-721, ERC-1155 |
| **Actions** | List, buy, sell, auctions |
| **Frontend** | Next.js 15 |
| **Backend** | NestJS, Prisma, GraphQL |
| **Status** | Built, ready for deployment |

## Supported NFT Standards

### ERC-721

Single-token NFTs (one token ID per unique asset). Ideal for profile pictures, collectibles, and unique digital art.

### ERC-1155

Multi-token NFTs (multiple token IDs in one contract, with fungible and non-fungible variants). Ideal for in-game items, editions, and mixed collections.

## Core Features

### Listing

- Sellers list NFTs with a fixed price or auction parameters.
- Listings are stored on-chain or indexed by the backend.
- Seaport orders are signed off-chain and fulfilled on-chain for gas efficiency.

### Buying & Selling

- Buyers purchase listed NFTs in a single transaction.
- Sellers receive payment (PRIM or ERC-20) upon sale.
- Royalties can be configured per collection.

### Auctions

- Time-limited auctions with bidding.
- Highest bidder wins when the auction ends.
- Supports reserve prices and minimum bid increments.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Primeport Ecosystem                       │
├─────────────────────────────────────────────────────────────┤
│  Primeport UI (Next.js 15)                                   │
│  - Browse, list, buy, sell, bid                              │
│  - Wallet connect (MetaMask, PrimeXDC)                       │
├─────────────────────────────────────────────────────────────┤
│  Primeport Server (NestJS)                                   │
│  - GraphQL API                                               │
│  - Prisma ORM, indexing                                      │
│  - Order validation, metadata                                │
├─────────────────────────────────────────────────────────────┤
│  Seaport Contracts (Mersennet)                             │
│  - Order fulfillment                                         │
│  - ERC-721 / ERC-1155 transfers                              │
└─────────────────────────────────────────────────────────────┘
```

## Tech Stack

| Layer | Technology |
|-------|------------|
| **Frontend** | Next.js 15, wagmi, viem/ethers |
| **Backend** | NestJS, Prisma, GraphQL |
| **Protocol** | Seaport |
| **Chain** | Mersennet (Chain ID 7919) |

## Deployment Status

| Component | Status |
|-----------|--------|
| Primeport UI | Built (.next/) |
| Primeport Server | Ready (NestJS, Prisma) |
| Seaport contracts | To be deployed to Mersennet |

## Integration

- **PrimeXDC Wallet** — Connect and sign transactions.
- **Mersennet** — All NFT and marketplace activity on-chain.
- **PrimeSwap** — Optional: trade NFT-related tokens or royalties.

## Related Resources

- [PrimeXDC Wallet](/ecosystem/wallet) — Recommended wallet for Primeport
- [Deployed Contracts](/resources/contracts) — Seaport addresses when live
- [Network Information](/getting-started/network-info) — RPC and configuration
