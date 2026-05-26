"""Executable end-to-end example for prime_viewNotes + scan_granted_notes.

Run from sdk-python/ with:

    python examples/view_notes_end_to_end.py
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

from prime_chain import GrantedViewingMaterial, PrimeProvider, ShieldedNote, make_mock_note_decryptor, scan_granted_notes

GRANT_ID_HEX = "0x" + "11" * 32
RECIPIENT_PUBLIC_KEY = "0x" + "22" * 32
VIEW_SECRET_HEX = "0x" + "33" * 32
EPHEMERAL_PK_HEX = "0x" + "44" * 32


def main() -> None:
    note = ShieldedNote(
        value=2500,
        asset_id=7,
        owner_pk=RECIPIENT_PUBLIC_KEY,
        rho="0x" + "55" * 32,
        psi="0x" + "66" * 32,
    )
    plaintext = encode_note_plaintext(note)
    key = expand_key(derive_shared_secret(VIEW_SECRET_HEX, EPHEMERAL_PK_HEX), len(plaintext))
    ciphertext = xor_bytes(plaintext, key)
    encrypted_note_hex = encode_encrypted_note_payload(
        recipient=RECIPIENT_PUBLIC_KEY,
        ciphertext=ciphertext,
        ephemeral_pk=EPHEMERAL_PK_HEX,
    )

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self) -> None:  # noqa: N802
            content_length = int(self.headers.get("Content-Length", "0"))
            payload = json.loads(self.rfile.read(content_length) or b"{}")
            result = {
                "jsonrpc": "2.0",
                "id": payload.get("id", 1),
                "result": {
                    "grantId": GRANT_ID_HEX,
                    "grantorCommitment": "0x" + "77" * 32,
                    "blockNumber": 42,
                    "shieldedStateRoot": "0x" + "88" * 32,
                    "totalEncryptedNoteCount": 1,
                    "returnedEncryptedNoteCount": 1,
                    "nextCursor": None,
                    "notes": [
                        {
                            "noteCommitment": "0x" + "99" * 32,
                            "encryptedNote": encrypted_note_hex,
                        }
                    ],
                    "signatureVerified": True,
                },
            }
            body = json.dumps(result).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, format: str, *args: object) -> None:  # noqa: A003
            return

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    provider = PrimeProvider(f"http://127.0.0.1:{server.server_address[1]}")
    result = scan_granted_notes(
        provider,
        GrantedViewingMaterial(
            grant_id_hex=GRANT_ID_HEX,
            recipient_public_key=RECIPIENT_PUBLIC_KEY,
            decrypt_note_ciphertext=make_mock_note_decryptor(VIEW_SECRET_HEX),
        ),
        limit=64,
    )

    print(
        json.dumps(
            {
                "decryptedNoteCount": len(result.notes),
                "firstNote": {
                    "noteCommitment": result.notes[0].note_commitment,
                    "value": result.notes[0].note.value,
                    "assetId": result.notes[0].note.asset_id,
                    "ownerPk": result.notes[0].note.owner_pk,
                },
            },
            indent=2,
        )
    )

    server.shutdown()
    server.server_close()


def encode_note_plaintext(note: ShieldedNote) -> bytes:
    out = bytearray(116)
    offset = 0
    out[offset : offset + 16] = note.value.to_bytes(16, "little")
    offset += 16
    out[offset : offset + 4] = note.asset_id.to_bytes(4, "little")
    offset += 4
    out[offset : offset + 32] = decode_hex(note.owner_pk)
    offset += 32
    out[offset : offset + 32] = decode_hex(note.rho)
    offset += 32
    out[offset : offset + 32] = decode_hex(note.psi)
    return bytes(out)


def encode_encrypted_note_payload(*, recipient: str, ciphertext: bytes, ephemeral_pk: str) -> str:
    payload = bytearray()
    payload.extend(decode_hex(recipient))
    payload.extend(len(ciphertext).to_bytes(8, "little"))
    payload.extend(ciphertext)
    payload.extend(decode_hex(ephemeral_pk))
    return "0x" + payload.hex()


def derive_shared_secret(view_secret_hex: str, ephemeral_pk_hex: str) -> bytes:
    return hashlib.sha256(decode_hex(view_secret_hex) + decode_hex(ephemeral_pk_hex)).digest()


def expand_key(seed: bytes, length: int) -> bytes:
    key = bytearray()
    block = hashlib.sha256(seed).digest()
    while len(key) < length:
        key.extend(block)
        block = hashlib.sha256(block).digest()
    return bytes(key[:length])


def xor_bytes(left: bytes, right: bytes) -> bytes:
    return bytes(a ^ b for a, b in zip(left, right))


def decode_hex(value: str) -> bytes:
    return bytes.fromhex(value[2:] if value.startswith("0x") else value)


if __name__ == "__main__":
    main()