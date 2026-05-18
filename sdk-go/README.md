# Prime Chain Go SDK

Go client for Prime Chain - JSON-RPC and CLOB (order book) operations.

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

    primechain "github.com/PrimeNumbersLabs/prime-chain-sdk-go"
)

func main() {
    provider := primechain.NewProvider("http://localhost:8545")

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

    orders := primechain.NewOrders(provider)
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
| `GasPrice()` | Current gas price |

### Orders

| Method | Description |
|--------|-------------|
| `AddMarket(base, quote, lot, tick)` | Add market (admin) |
| `PlaceOrder(market, side, price, amount, tif, owner)` | Place order |
| `CancelOrder(orderID)` | Cancel order |
| `GetOrderBook(market)` | Get order book |
