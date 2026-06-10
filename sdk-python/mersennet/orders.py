"""PrimeOrders - CLOB interaction for Mersennet."""

from typing import List, Optional

from .provider import PrimeProvider
from .types import Order, OrderBook, OrderBookLevel, Trade


def _to_hex_amount(s: str) -> str:
    if s.startswith("0x"):
        return s
    return hex(int(s))


class PrimeOrders:
    """High-level API for Mersennet order book operations."""

    def __init__(self, provider: PrimeProvider):
        self.provider = provider

    def add_market(
        self, base: str, quote: str, lot: str, tick: str
    ) -> int:
        """Add a new market (admin). Returns market ID."""
        result = self.provider._request("primeorders_addMarket", [
            f"{base}-{quote}",
            _to_hex_amount(tick),
            _to_hex_amount(lot),
        ])
        return int(result, 16) if isinstance(result, str) else int(result)

    def place_order(
        self,
        market: int,
        side: str,
        price: str,
        amount: str,
        tif: str = "gtc",
        owner: Optional[str] = None,
    ) -> dict:
        """Place an order. owner required for RPC (unlocked account)."""
        if owner is None:
            raise ValueError("owner required for place_order")
        params = [{
            "owner": owner,
            "market_id": market,
            "side": side,
            "price": _to_hex_amount(price),
            "size": _to_hex_amount(amount),
            "tif": tif,
        }]
        return self.provider._request("primeorders_submitOrder", params)

    def cancel_order(self, order_id: int) -> bool:
        """Cancel an order by ID."""
        result = self.provider._request("primeorders_cancelOrder", [
            hex(order_id),
        ])
        return bool(result)

    def get_order_book(self, market: int) -> OrderBook:
        """Get order book for a market."""
        result = self.provider._request("primeorders_getOrderBook", [
            hex(market),
        ])
        if result is None:
            return OrderBook(bids=[], asks=[])
        bids = [
            OrderBookLevel(price=str(l.get("price", "0x0")), size=str(l.get("size", "0x0")))
            for l in result.get("bids", [])
        ]
        asks = [
            OrderBookLevel(price=str(l.get("price", "0x0")), size=str(l.get("size", "0x0")))
            for l in result.get("asks", [])
        ]
        return OrderBook(bids=bids, asks=asks)

    def get_trades(self, market: int) -> List[Trade]:
        """Get recent trades for a market (via domain events or RPC if available)."""
        result = self.provider._request("prime_getDomainEvents", [{
            "domain": "primeorders",
            "kind": "trade",
        }])
        if not result or not isinstance(result, list):
            return []
        trades = []
        for evt in result:
            data = evt.get("data", {}) if isinstance(evt, dict) else {}
            if str(data.get("market_id", "")) == hex(market):
                trades.append(Trade(
                    taker=str(data.get("taker", "")),
                    maker=str(data.get("maker", "")),
                    market_id=str(data.get("market_id", "")),
                    side=str(data.get("side", "buy")),
                    price=str(data.get("price", "0x0")),
                    size=str(data.get("size", "0x0")),
                ))
        return trades

    def get_positions(self, address: str, market: Optional[int] = None) -> dict:
        """Get positions for address. Uses precompile if available."""
        if market is not None:
            data = "0x" + "0" * 24 + hex(market)[2:].zfill(64)
            result = self.provider.call({
                "from": address,
                "to": "0x0000000000000000000000000000000000000100",
                "data": data,
            })
            if result and result != "0x":
                return {"market": market, "raw": result}
        return {}
