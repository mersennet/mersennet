# Mersennet Go SDK

Go client for Mersennet - JSON-RPC and CLOB (order book) operations.

## Installation

```bash
go get github.com/PrimeNumbersLabs/prime-chain-sdk-go
```

## Quick Start

```go
package main

import (
    "fmt"
    "log"

    mersennet "github.com/PrimeNumbersLabs/prime-chain-sdk-go"
)

func main() {
    provider := mersennet.NewProvider("http://localhost:8545")

    chainID, err := provider.ChainID()
    if err != nil {
        log.Fatal(err)
    }
    fmt.Println("Chain ID:", chainID)

    blockNum, err := provider.BlockNumber()
    if err != nil {
        log.Fatal(err)
    }
    fmt.Println("Block:", blockNum)

    balance, err := provider.GetBalance("0xYourAddress")
    if err != nil {
        log.Fatal(err)
    }
    fmt.Println("Balance:", balance)

    orders := mersennet.NewOrders(provider)
    book, err := orders.GetOrderBook(1)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Println("Bids:", book.Bids)
    fmt.Println("Asks:", book.Asks)
}
```

## API Reference

### Provider

| Method | Description |
|--------|-------------|
| `GetBlock(number, includeTxs)` | Get block by number or "latest" |
| `GetBlockByHash(hash, includeTxs)` | Get block by hash |
| `GetTransaction(hash)` | Get transaction by hash |
| `GetBalance(address)` | Get balance |
| `GetNonce(address)` | Get nonce |
| `SendRawTransaction(rawTx)` | Send signed transaction |
| `Call(txObject)` | Simulate call |
| `ChainID()` | Chain ID |
| `BlockNumber()` | Latest block number |
| `ViewNotes(grantID, limit, cursorHex)` | Grant-gated encrypted note export |
| `GasPrice()` | Current gas price |

### Orders

| Method | Description |
|--------|-------------|
| `AddMarket(base, quote, lot, tick)` | Add market (admin) |
| `PlaceOrder(market, side, price, amount, tif, owner)` | Place order |
| `CancelOrder(orderID)` | Cancel order |
| `GetOrderBook(market)` | Get order book |

### Shielded Notes

Use `ViewNotes` to fetch encrypted note envelopes, then call `ScanGrantedNotes`
with a decrypt function that applies your delegated viewing material locally.

```go
material := mersennet.GrantedViewingMaterial{
    GrantID:            "0x...",
    RecipientPublicKey: "0x...",
    Decrypt:            mersennet.NewMockNoteDecryptor("0x..."),
}

result, err := mersennet.ScanGrantedNotes(provider, material, &mersennet.GrantedNoteScanOptions{
    Limit: 64,
})
if err != nil {
    log.Fatal(err)
}
fmt.Println("Decrypted notes:", len(result.Notes))
```

See the runnable end-to-end example in [examples/view-notes-end-to-end/main.go](examples/view-notes-end-to-end/main.go).
