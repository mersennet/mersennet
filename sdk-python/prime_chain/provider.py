"""PrimeProvider - JSON-RPC client for Prime Chain."""

import json
from typing import Any, Dict, Optional

try:
    import requests
    _USE_REQUESTS = True
except ImportError:
    _USE_REQUESTS = False

from .types import Block, ViewNotesEntry, ViewNotesResult


class PrimeChainError(Exception):
    """Prime Chain RPC or SDK error."""

    def __init__(self, message: str, code: Optional[int] = None):
        super().__init__(message)
        self.code = code


def _hex_to_number(hex_str: str) -> int:
    s = hex_str[2:] if hex_str.startswith("0x") else hex_str
    return int(s, 16)


def _hex_to_string(hex_str: str) -> str:
    s = hex_str[2:] if hex_str.startswith("0x") else hex_str
    if not s or s == "0":
        return "0x0"
    return "0x" + s.lower()


class PrimeProvider:
    """Prime Chain JSON-RPC provider. Connects to the RPC endpoint via HTTP."""

    def __init__(self, rpc_url: str):
        self.url = rpc_url
        self._id = 0

    def _request(self, method: str, params: Optional[list] = None) -> Any:
        self._id += 1
        payload = {
            "jsonrpc": "2.0",
            "id": self._id,
            "method": method,
            "params": params or [],
        }
        try:
            if _USE_REQUESTS:
                resp = requests.post(
                    self.url,
                    json=payload,
                    headers={"Content-Type": "application/json"},
                    timeout=30,
                )
                resp.raise_for_status()
                data = resp.json()
            else:
                import urllib.request
                import urllib.error
                body = json.dumps(payload).encode("utf-8")
                req = urllib.request.Request(
                    self.url,
                    data=body,
                    headers={"Content-Type": "application/json"},
                    method="POST",
                )
                with urllib.request.urlopen(req, timeout=30) as resp:
                    data = json.loads(resp.read().decode())
        except Exception as e:
            raise PrimeChainError(str(e))

        if "error" in data:
            err = data["error"]
            raise PrimeChainError(
                err.get("message", "Unknown RPC error"),
                code=err.get("code"),
            )
        return data.get("result")

    def get_block(self, number: int | str, include_txs: bool = False) -> Optional[Block]:
        """Get block by number. Use 'latest' for latest block."""
        tag = "latest" if number == "latest" else hex(number)
        result = self._request("eth_getBlockByNumber", [tag, include_txs])
        if result is None:
            return None
        return self._parse_block(result)

    def get_block_by_hash(self, hash: str, include_txs: bool = False) -> Optional[Block]:
        """Get block by hash."""
        result = self._request("eth_getBlockByHash", [hash, include_txs])
        if result is None:
            return None
        return self._parse_block(result)

    def _parse_block(self, obj: Dict) -> Block:
        return Block(
            number=obj.get("number", "0x0"),
            hash=obj.get("hash", "0x0"),
            gas_limit=obj.get("gas_limit", "0x0"),
            gas_used=obj.get("gas_used", "0x0"),
            base_fee=obj.get("base_fee", "0x0"),
            state_root=obj.get("state_root", "0x0"),
            transactions=obj.get("transactions", []),
            domain_events=obj.get("domain_events"),
        )

    def get_transaction(self, hash: str) -> Optional[Dict]:
        """Get transaction by hash."""
        return self._request("eth_getTransactionByHash", [hash])

    def get_transaction_receipt(self, hash: str) -> Optional[Dict]:
        """Get transaction receipt by hash."""
        return self._request("eth_getTransactionReceipt", [hash])

    def get_balance(self, address: str) -> str:
        """Get balance of address (hex string)."""
        result = self._request("eth_getBalance", [address])
        return _hex_to_string(result) if result else "0x0"

    def get_nonce(self, address: str) -> int:
        """Get nonce of address."""
        result = self._request("eth_getTransactionCount", [address])
        return _hex_to_number(result) if result else 0

    def send_raw_transaction(self, raw_tx: str) -> str:
        """Send raw signed transaction. Returns tx hash."""
        return self._request("eth_sendRawTransaction", [raw_tx])

    def call(self, tx_object: Dict) -> str:
        """eth_call - simulate contract call."""
        result = self._request("eth_call", [tx_object])
        return result or "0x"

    def chain_id(self) -> int:
        """Get chain ID."""
        result = self._request("eth_chainId")
        return _hex_to_number(result) if result else 0

    def block_number(self) -> int:
        """Get latest block number."""
        result = self._request("eth_blockNumber")
        return _hex_to_number(result) if result else 0

    def view_notes(
        self,
        grant_id_hex: str,
        limit: Optional[int] = None,
        cursor_hex: Optional[str] = None,
    ) -> ViewNotesResult:
        """Get grant-gated encrypted note exports via prime_viewNotes."""
        request: Dict[str, Any] = {"grantIdHex": grant_id_hex}
        if limit is not None:
            request["limit"] = limit
        if cursor_hex is not None:
            request["cursorHex"] = cursor_hex
        result = self._request("prime_viewNotes", [request])
        notes = [
            ViewNotesEntry(
                note_commitment=entry.get("noteCommitment", "0x"),
                encrypted_note=entry.get("encryptedNote", "0x"),
            )
            for entry in result.get("notes", [])
        ]
        return ViewNotesResult(
            grant_id=result.get("grantId", "0x"),
            grantor_commitment=result.get("grantorCommitment", "0x"),
            block_number=result.get("blockNumber", 0),
            shielded_state_root=result.get("shieldedStateRoot", "0x"),
            total_encrypted_note_count=result.get("totalEncryptedNoteCount", 0),
            returned_encrypted_note_count=result.get("returnedEncryptedNoteCount", 0),
            next_cursor=result.get("nextCursor"),
            notes=notes,
            signature_verified=result.get("signatureVerified", False),
        )

    def gas_price(self) -> str:
        """Get current gas price (hex)."""
        result = self._request("eth_gasPrice")
        return _hex_to_string(result) if result else "0x0"
