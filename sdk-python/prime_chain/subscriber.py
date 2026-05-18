"""PrimeSubscriber - WebSocket subscriptions for Prime Chain."""

import json
import threading
from typing import Callable, Dict, List, Optional

try:
    import websocket
    HAS_WEBSOCKET = True
except ImportError:
    HAS_WEBSOCKET = False


class PrimeSubscriber:
    """WebSocket subscription manager for Prime Chain."""

    def __init__(self, ws_url: str):
        if not HAS_WEBSOCKET:
            raise ImportError("websocket-client required: pip install websocket-client")
        self.ws_url = ws_url.replace("http://", "ws://").replace("https://", "wss://")
        self._ws: Optional[websocket.WebSocketApp] = None
        self._callbacks: Dict[str, Callable] = {}
        self._next_id = 1
        self._pending: Dict[int, Callable] = {}
        self._connected = False
        self._thread: Optional[threading.Thread] = None

    def connect(self) -> None:
        """Connect to WebSocket endpoint."""
        self._ws = websocket.WebSocketApp(
            self.ws_url,
            on_open=self._on_open,
            on_message=self._on_message,
            on_error=lambda ws, err: None,
        )
        self._thread = threading.Thread(target=self._run)
        self._thread.daemon = True
        self._thread.start()
        import time
        for _ in range(50):
            if self._connected:
                return
            time.sleep(0.1)
        raise TimeoutError("WebSocket connect timeout")

    def _run(self) -> None:
        if self._ws:
            self._ws.run_forever()

    def _on_open(self, ws) -> None:
        self._connected = True

    def _on_message(self, ws, message: str) -> None:
        try:
            msg = json.loads(message)
            if "id" in msg and msg["id"] in self._pending:
                cb = self._pending.pop(msg["id"])
                cb(msg.get("result", ""))
                return
            if msg.get("method") == "eth_subscription" and "params" in msg:
                sub_id = str(msg["params"].get("subscription", ""))
                if sub_id in self._callbacks:
                    self._callbacks[sub_id](msg["params"].get("result"))
        except (json.JSONDecodeError, KeyError):
            pass

    def disconnect(self) -> None:
        """Disconnect and clear subscriptions."""
        self._connected = False
        self._callbacks.clear()
        self._pending.clear()
        if self._ws:
            self._ws.close()
            self._ws = None

    def _subscribe(self, method: str, params: list, callback: Callable) -> str:
        if not self._connected or not self._ws:
            self.connect()
        req_id = self._next_id
        self._next_id += 1
        sub_id_holder = [None]

        def on_result(sub_id: str) -> None:
            sub_id_holder[0] = sub_id
            self._callbacks[sub_id] = callback

        self._pending[req_id] = on_result
        self._ws.send(json.dumps({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params,
        }))
        import time
        for _ in range(100):
            if sub_id_holder[0]:
                return sub_id_holder[0]
            time.sleep(0.1)
        raise TimeoutError("Subscribe timeout")

    def subscribe_blocks(self, callback: Callable) -> str:
        """Subscribe to new blocks. Returns subscription ID."""
        return self._subscribe("eth_subscribe", ["newHeads"], callback)

    def subscribe_trades(self, market: int, callback: Callable) -> str:
        """Subscribe to trades for a market. Returns subscription ID."""
        return self._subscribe("prime_subscribe", ["PrimeOrdersTrades", market], callback)

    def subscribe_logs(
        self,
        callback: Callable,
        topics: Optional[List[str]] = None,
        address: Optional[str] = None,
    ) -> str:
        """Subscribe to logs. Returns subscription ID."""
        filt = {}
        if address:
            filt["address"] = address
        if topics:
            filt["topics"] = topics
        params = ["logs", filt] if filt else ["logs"]
        return self._subscribe("eth_subscribe", params, callback)

    def unsubscribe(self, sub_id: str) -> None:
        """Unsubscribe by ID."""
        self._callbacks.pop(sub_id, None)
        if self._ws and self._connected:
            self._ws.send(json.dumps({
                "jsonrpc": "2.0",
                "id": self._next_id,
                "method": "eth_unsubscribe",
                "params": [sub_id],
            }))
            self._next_id += 1
