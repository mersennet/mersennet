"""Type definitions for Prime Chain SDK."""

from dataclasses import dataclass
from typing import List, Optional


@dataclass
class Block:
    """Block data from eth_getBlockByNumber / eth_getBlockByHash."""

    number: str
    hash: str
    gas_limit: str
    gas_used: str
    base_fee: str
    state_root: str
    transactions: List
    domain_events: Optional[List] = None


@dataclass
class Transaction:
    """Transaction data."""

    hash: str
    from_: str
    to: Optional[str]
    value: str
    nonce: str
    gas: str
    gas_price: str
    input: str


@dataclass
class Receipt:
    """Transaction receipt."""

    transaction_hash: str
    block_hash: str
    block_number: str
    transaction_index: str
    gas_used: str
    status: str
    contract_address: Optional[str]
    output: str
    logs: List


@dataclass
class Order:
    """Order in the order book or open orders list."""

    id: str
    owner: str
    market_id: str
    side: str  # 'buy' | 'sell'
    price: str
    size: str
    tif: str  # 'gtc' | 'ioc' | 'fok'


@dataclass
class OrderBookLevel:
    """Order book level."""

    price: str
    size: str


@dataclass
class OrderBook:
    """Full order book snapshot."""

    bids: List[OrderBookLevel]
    asks: List[OrderBookLevel]


@dataclass
class Trade:
    """Trade execution record."""

    taker: str
    maker: str
    market_id: str
    side: str
    price: str
    size: str
