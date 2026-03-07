# Prime Chain Python SDK

Python client for Prime Chain - JSON-RPC, CLOB (order book), and WebSocket subscriptions.

## Installation

```bash
pip install prime-chain-sdk
```

Or from source:

```bash
cd sdk-python
pip install -e .
```

## Quick Start

```python
from prime_chain import PrimeProvider, PrimeOrders

provider = PrimeProvider("http://localhost:8545")

# Chain info
print("Chain ID:", provider.chain_id())
print("Block:", provider.block_number())
print("Gas price:", provider.gas_price())

# Account
balance = provider.get_balance("0xYourAddress")
print("Balance:", balance)

# Order book
orders = PrimeOrders(provider)
book = orders.get_order_book(1)
print("Bids:", book.bids)
print("Asks:", book.asks)
```

## API Reference

### PrimeProvider

| Method | Description |
|--------|-------------|
| `get_block(number, include_txs)` | Get block by number or "latest" |
| `get_block_by_hash(hash, include_txs)` | Get block by hash |
| `get_transaction(hash)` | Get transaction by hash |
| `get_balance(address)` | Get balance (hex string) |
| `get_nonce(address)` | Get nonce |
| `send_raw_transaction(raw_tx)` | Send signed transaction |
| `call(tx_object)` | Simulate call (eth_call) |
| `chain_id()` | Chain ID |
| `block_number()` | Latest block number |
| `gas_price()` | Current gas price |

### PrimeOrders

| Method | Description |
|--------|-------------|
| `add_market(base, quote, lot, tick)` | Add market (admin) |
| `place_order(market, side, price, amount, tif, owner)` | Place order |
| `cancel_order(order_id)` | Cancel order |
| `get_order_book(market)` | Get order book |
| `get_trades(market)` | Get recent trades |
| `get_positions(address, market)` | Get positions |

### PrimeSubscriber (WebSocket)

| Method | Description |
|--------|-------------|
| `connect()` | Connect to WebSocket |
| `disconnect()` | Disconnect |
| `subscribe_blocks(callback)` | Subscribe to new blocks |
| `subscribe_trades(market, callback)` | Subscribe to trades |
| `subscribe_logs(callback, topics, address)` | Subscribe to logs |
| `unsubscribe(id)` | Unsubscribe |

## WebSocket Example

```python
from prime_chain import PrimeSubscriber

sub = PrimeSubscriber("ws://localhost:8545")
sub.connect()

def on_block(block):
    print("New block:", block)

sub_id = sub.subscribe_blocks(on_block)
# ...
sub.unsubscribe(sub_id)
sub.disconnect()
```

## Error Handling

```python
from prime_chain.provider import PrimeChainError

try:
    balance = provider.get_balance("0x...")
except PrimeChainError as e:
    print(f"Error {e.code}: {e}")
```
