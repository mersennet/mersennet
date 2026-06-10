---
title: "NFT (ERC-721) Guide"
---

This guide walks you through deploying and interacting with an NFT collection on Mersennet (Chain ID 131071).

## Network Details

| Parameter | Value |
|-----------|-------|
| Chain ID | 131071 |
| RPC URL | `http://46.225.30.187:8545` |
| Block Explorer | http://46.225.30.187 |

## Full Solidity Contract

Here is a complete ERC-721 implementation with metadata, minting, and transfer:

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract MersennetNFT {
    string public name;
    string public symbol;
    string public baseTokenURI;

    mapping(uint256 => address) private _owners;
    mapping(address => uint256) private _balances;
    mapping(uint256 => address) private _tokenApprovals;
    mapping(address => mapping(address => bool)) private _operatorApprovals;

    uint256 private _nextTokenId = 1;
    address public owner;

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    event Approval(address indexed owner, address indexed approved, uint256 indexed tokenId);
    event ApprovalForAll(address indexed owner, address indexed operator, bool approved);

    constructor(string memory _name, string memory _symbol, string memory _baseURI) {
        name = _name;
        symbol = _symbol;
        baseTokenURI = _baseURI;
        owner = msg.sender;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "Not owner");
        _;
    }

    function mint(address to) external onlyOwner returns (uint256) {
        uint256 tokenId = _nextTokenId++;
        _owners[tokenId] = to;
        _balances[to]++;
        emit Transfer(address(0), to, tokenId);
        return tokenId;
    }

    function balanceOf(address account) external view returns (uint256) {
        return _balances[account];
    }

    function ownerOf(uint256 tokenId) external view returns (address) {
        address tokenOwner = _owners[tokenId];
        require(tokenOwner != address(0), "Token does not exist");
        return tokenOwner;
    }

    function tokenURI(uint256 tokenId) external view returns (string memory) {
        require(_owners[tokenId] != address(0), "Token does not exist");
        return string(abi.encodePacked(baseTokenURI, _toString(tokenId), ".json"));
    }

    function approve(address to, uint256 tokenId) external {
        address tokenOwner = _owners[tokenId];
        require(msg.sender == tokenOwner || _operatorApprovals[tokenOwner][msg.sender], "Not authorized");
        _tokenApprovals[tokenId] = to;
        emit Approval(tokenOwner, to, tokenId);
    }

    function setApprovalForAll(address operator, bool approved) external {
        _operatorApprovals[msg.sender][operator] = approved;
        emit ApprovalForAll(msg.sender, operator, approved);
    }

    function transferFrom(address from, address to, uint256 tokenId) external {
        require(_isApprovedOrOwner(msg.sender, tokenId), "Not authorized");
        _transfer(from, to, tokenId);
    }

    function safeTransferFrom(address from, address to, uint256 tokenId) external {
        require(_isApprovedOrOwner(msg.sender, tokenId), "Not authorized");
        _transfer(from, to, tokenId);
    }

    function _isApprovedOrOwner(address spender, uint256 tokenId) internal view returns (bool) {
        address tokenOwner = _owners[tokenId];
        return spender == tokenOwner ||
            _tokenApprovals[tokenId] == spender ||
            _operatorApprovals[tokenOwner][spender];
    }

    function _transfer(address from, address to, uint256 tokenId) internal {
        require(_owners[tokenId] == from, "Wrong from");
        require(to != address(0), "Transfer to zero");

        delete _tokenApprovals[tokenId];
        _balances[from]--;
        _balances[to]++;
        _owners[tokenId] = to;

        emit Transfer(from, to, tokenId);
    }

    function _toString(uint256 value) internal pure returns (string memory) {
        if (value == 0) return "0";
        uint256 temp = value;
        uint256 digits;
        while (temp != 0) {
            digits++;
            temp /= 10;
        }
        bytes memory buffer = new bytes(digits);
        while (value != 0) {
            digits -= 1;
            buffer[digits] = bytes1(uint8(48 + uint256(value % 10)));
            value /= 10;
        }
        return string(buffer);
    }

    function setBaseURI(string memory _baseURI) external onlyOwner {
        baseTokenURI = _baseURI;
    }
}
```

## Metadata Format

For each token, host a JSON file at `{baseTokenURI}{tokenId}.json`:

```json
{
  "name": "Prime NFT #1",
  "description": "A unique NFT on Mersennet",
  "image": "ipfs://Qm...",
  "attributes": [
    { "trait_type": "Rarity", "value": "Common" }
  ]
}
```

## Deployment

### Using Hardhat

```typescript
const MersennetNFT = await ethers.getContractFactory("MersennetNFT");
const nft = await MersennetNFT.deploy(
  "Prime Collectibles",
  "PRIME",
  "https://api.mysite.com/metadata/"
);
await nft.waitForDeployment();
console.log("NFT deployed to:", await nft.getAddress());
```

### Using Remix

1. Compile the contract in Remix.
2. Connect MetaMask to Mersennet (Chain ID 131071).
3. Deploy with constructor args: `"Prime Collectibles"`, `"PRIME"`, `"https://api.mysite.com/metadata/"`.

## Interacting with the NFT

### Minting

```javascript
const { ethers } = require("ethers");

const provider = new ethers.JsonRpcProvider("http://46.225.30.187:8545", 131071);
const wallet = new ethers.Wallet(process.env.PRIVATE_KEY, provider);

const NFT_ABI = [
  "function mint(address to) returns (uint256)",
  "function balanceOf(address) view returns (uint256)",
  "function ownerOf(uint256) view returns (address)",
  "function tokenURI(uint256) view returns (string)",
  "function transferFrom(address from, address to, uint256 tokenId)",
];

const nft = new ethers.Contract("0xYourNFTAddress", NFT_ABI, wallet);

// Mint to an address
const tx = await nft.mint("0xRecipientAddress");
const receipt = await tx.wait();
console.log("Minted token");
```

### Reading Metadata

```javascript
const tokenId = 1;
const owner = await nft.ownerOf(tokenId);
const uri = await nft.tokenURI(tokenId);
const balance = await nft.balanceOf(wallet.address);

console.log("Owner:", owner);
console.log("Token URI:", uri);
console.log("Balance:", balance.toString());
```

### Transferring

```javascript
await nft.transferFrom(wallet.address, "0xNewOwner", tokenId);
```

## Integration with Primeport

[Primeport](/ecosystem/primeport) is the NFT marketplace on Mersennet. To list your collection:

1. Deploy your NFT contract.
2. Mint tokens and host metadata (IPFS or your API).
3. Integrate with Primeport's listing flow (Seaport protocol).
