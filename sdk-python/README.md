# Mersennet Python SDK

Python client for Mersennet - JSON-RPC, CLOB (order book), and WebSocket subscriptions.

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
| `view_notes(grant_id_hex, limit, cursor_hex)` | Grant-gated encrypted note export |
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

### Shielded Notes

Use `view_notes` to fetch encrypted note envelopes, then call
`scan_granted_notes` with a decrypt function that applies your granted
viewing material locally.

```python
from prime_chain import GrantedViewingMaterial, PrimeProvider, make_mock_note_decryptor, scan_granted_notes

provider = PrimeProvider("http://localhost:8545")

material = GrantedViewingMaterial(
    grant_id_hex="0x...",
    recipient_public_key="0x...",
    decrypt_note_ciphertext=make_mock_note_decryptor("0x..."),
)

result = scan_granted_notes(provider, material, limit=64)
print("Decrypted notes:", len(result.notes))
```

See the runnable end-to-end example in [examples/view_notes_end_to_end.py](examples/view_notes_end_to_end.py).

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
