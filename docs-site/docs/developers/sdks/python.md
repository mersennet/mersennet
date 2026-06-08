---
slug: /developers/sdks/python
sidebar_position: 2
title: "Python SDK"
---

# Python SDK

The Mersennet Python SDK (`prime-chain-sdk`) provides a client for the JSON-RPC API, PrimeOrders (on-chain order book), and WebSocket subscriptions.

## Installation

From PyPI (when published):

```bash
pip install prime-chain-sdk
```

From source (Mersennet monorepo):

```bash
cd sdk-python
pip install -e .
```

Or with requirements:

```bash
pip install requests websocket-client
```

## Quick Start

```python
from prime_chain import PrimeProvider, PrimeOrders

provider = PrimeProvider("http://46.225.30.187:8545")
orders = PrimeOrders(provider)

# Query chain
block_num = provider.block_number()
chain_id = provider.chain_id()
print(f"Block: {block_num}, Chain ID: {chain_id}")

# Query balance
balance = provider.get_balance("0x7f5ce38fb2553e95dd8ef9182a80bc219c9a0d45")
print(f"Balance: {balance}")

# Query order book
book = orders.get_order_book(1)
print(f"Bids: {book.bids}, Asks: {book.asks}")
```

## Creating a Provider

```python
from prime_chain import PrimeProvider

provider = PrimeProvider("http://46.225.30.187:8545")
```

## Basic Usage

### Block and Chain Info

```python
# Latest block number
block_num = provider.block_number()

# Block by number
block = provider.get_block("latest", include_txs=False)
block = provider.get_block(12345, include_txs=True)

# Chain ID
chain_id = provider.chain_id()

# Gas price (hex)
gas_price = provider.gas_price()
```

### Account Queries

```python
# Balance (hex string)
balance = provider.get_balance("0xYourAddress")

# Nonce (transaction count)
nonce = provider.get_nonce("0xYourAddress")
```

### Transaction Queries

```python
# Transaction by hash
tx = provider.get_transaction("0xTxHash")

# Transaction receipt
receipt = provider.get_transaction_receipt("0xTxHash")
```

### Contract Calls (eth_call)

```python
# Simulate a contract call
result = provider.call({
    "from": "0x...",
    "to": "0x...",
    "data": "0x...",
})
```

### Sending Raw Transactions

```python
# Send signed raw transaction
tx_hash = provider.send_raw_transaction("0xSignedTxHex")
```

:::note
Mersennet uses a custom raw transaction format. Use `eth_sendTransaction` or `prime_sendTransaction` when the node has the account unlocked. For wallet-signed transactions, ensure your signing library produces the Mersennet format.
:::

## PrimeOrders

### Get Order Book

```python
book = orders.get_order_book(1)
for bid in book.bids:
    print(f"Bid: price={bid.price}, size={bid.size}")
for ask in book.asks:
    print(f"Ask: price={ask.price}, size={ask.size}")
```

### Place Order

Requires the RPC node to have the owner account unlocked:

```python
result = orders.place_order(
    market=1,
    side="buy",
    price="1000000",
    amount="10",
    tif="gtc",
    owner="0xOwnerAddress",
)
```

### Cancel Order

```python
success = orders.cancel_order(order_id=123)
```

### Get Trades

```python
trades = orders.get_trades(1)
for t in trades:
    print(f"Trade: {t.taker} -> {t.maker}, price={t.price}, size={t.size}")
```

### Get Positions

```python
positions = orders.get_positions("0xOwnerAddress", market=1)
```

### Add Market (Admin)

```python
market_id = orders.add_market(
    base="PRIM",
    quote="USDC",
    lot="1000000",
    tick="1000",
)
```

## WebSocket Subscriptions

```python
from prime_chain import PrimeSubscriber

# PrimeSubscriber takes the WebSocket URL directly
subscriber = PrimeSubscriber("ws://46.225.30.187:8546")

# Connect
subscriber.connect()

# Subscribe to new blocks
def on_block(block):
    print("New block:", block)

sub_id = subscriber.subscribe_blocks(on_block)

# Subscribe to PrimeOrders trades for a market
def on_trade(trade):
    print("Trade:", trade)

trade_sub_id = subscriber.subscribe_trades(1, on_trade)

# Subscribe to logs
def on_log(log):
    print("Log:", log)

log_sub_id = subscriber.subscribe_logs(on_log, address="0x...")

# Unsubscribe
subscriber.unsubscribe(sub_id)
subscriber.disconnect()
```

:::tip
WebSocket support may vary by node configuration. If subscriptions fail, use HTTP polling.
:::

## Error Handling

```python
from prime_chain import PrimeProvider
from prime_chain.provider import MersennetError

provider = PrimeProvider("http://46.225.30.187:8545")

try:
    balance = provider.get_balance("0x...")
except MersennetError as e:
    print(f"RPC error: {e}, code: {e.code}")
```

## Dependencies

- **Python 3.9+**
- **requests** (recommended for HTTP)
- **websocket-client** (for WebSocket subscriptions)

The SDK can fall back to `urllib` if `requests` is not installed, but `requests` is recommended for production use.
