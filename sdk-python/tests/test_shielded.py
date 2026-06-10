import hashlib
import unittest

from mersennet.shielded import (
    GrantedNoteDecryptInput,
    make_mock_note_decryptor,
    parse_encrypted_note_payload,
    parse_shielded_note_plaintext,
)


class ShieldedMockDecryptorTests(unittest.TestCase):
    def test_mock_note_decryptor_round_trips_note_payload(self) -> None:
        view_secret_hex = "0x" + "33" * 32
        recipient_hex = "0x" + "22" * 32
        ephemeral_pk_hex = "0x" + "44" * 32
        note = {
            "value": 2500,
            "asset_id": 7,
            "owner_pk": recipient_hex,
            "rho": "0x" + "55" * 32,
            "psi": "0x" + "66" * 32,
        }
        plaintext = encode_note_plaintext(note)
        seed = hashlib.sha256(decode_hex(view_secret_hex) + decode_hex(ephemeral_pk_hex)).digest()
        ciphertext = xor_bytes(plaintext, expand_key(seed, len(plaintext)))
        payload_hex = encode_encrypted_note_payload(recipient_hex, ciphertext, ephemeral_pk_hex)

        envelope = parse_encrypted_note_payload(payload_hex)
        decryptor = make_mock_note_decryptor(view_secret_hex)
        decrypted = decryptor(
            GrantedNoteDecryptInput(
                note_commitment="0x" + "99" * 32,
                encrypted_note_hex=payload_hex,
                envelope=envelope,
            )
        )
        self.assertIsNotNone(decrypted)

        parsed = parse_shielded_note_plaintext(decrypted)
        self.assertEqual(parsed.value, 2500)
        self.assertEqual(parsed.asset_id, 7)
        self.assertEqual(parsed.owner_pk, recipient_hex)
        self.assertEqual(parsed.rho, note["rho"])
        self.assertEqual(parsed.psi, note["psi"])

    def test_parse_encrypted_note_payload_rejects_malformed_payload(self) -> None:
        with self.assertRaisesRegex(ValueError, "malformed encrypted note payload"):
            parse_encrypted_note_payload("0x1234")


def encode_note_plaintext(note: dict) -> bytes:
    out = bytearray(116)
    offset = 0
    out[offset : offset + 16] = note["value"].to_bytes(16, "little")
    offset += 16
    out[offset : offset + 4] = note["asset_id"].to_bytes(4, "little")
    offset += 4
    out[offset : offset + 32] = decode_hex(note["owner_pk"])
    offset += 32
    out[offset : offset + 32] = decode_hex(note["rho"])
    offset += 32
    out[offset : offset + 32] = decode_hex(note["psi"])
    return bytes(out)


def encode_encrypted_note_payload(recipient: str, ciphertext: bytes, ephemeral_pk: str) -> str:
    payload = bytearray()
    payload.extend(decode_hex(recipient))
    payload.extend(len(ciphertext).to_bytes(8, "little"))
    payload.extend(ciphertext)
    payload.extend(decode_hex(ephemeral_pk))
    return "0x" + payload.hex()


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
    unittest.main()