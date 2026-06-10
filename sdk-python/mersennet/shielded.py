"""Shielded note helpers for grant-gated note scanning and local decryption."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
from typing import Callable, List, Optional

from .provider import PrimeProvider


@dataclass
class EncryptedNoteEnvelope:
    recipient: str
    ciphertext: bytes
    ephemeral_pk: str


@dataclass
class ShieldedNote:
    value: int
    asset_id: int
    owner_pk: str
    rho: str
    psi: str


@dataclass
class GrantedNoteDecryptInput:
    note_commitment: str
    encrypted_note_hex: str
    envelope: EncryptedNoteEnvelope


@dataclass
class GrantedViewingMaterial:
    grant_id_hex: str
    decrypt_note_ciphertext: Callable[[GrantedNoteDecryptInput], Optional[bytes]]
    recipient_public_key: Optional[str] = None


@dataclass
class GrantedDecryptedNote:
    note_commitment: str
    envelope: EncryptedNoteEnvelope
    note: ShieldedNote


@dataclass
class GrantedNoteScanResult:
    grant_id: str
    block_number: int
    total_encrypted_note_count: int
    fetched_encrypted_note_count: int
    next_cursor: Optional[str]
    skipped_malformed_count: int
    notes: List[GrantedDecryptedNote]


def parse_encrypted_note_payload(payload_hex: str) -> EncryptedNoteEnvelope:
    payload = _decode_hex_bytes(payload_hex)
    if len(payload) < 72:
        raise ValueError("malformed encrypted note payload")
    offset = 0
    recipient = _bytes_to_hex(payload[offset : offset + 32])
    offset += 32
    ciphertext_len = int.from_bytes(payload[offset : offset + 8], "little")
    offset += 8
    if offset + ciphertext_len + 32 > len(payload):
        raise ValueError("malformed encrypted note payload")
    ciphertext = payload[offset : offset + ciphertext_len]
    offset += ciphertext_len
    ephemeral_pk = _bytes_to_hex(payload[offset : offset + 32])
    offset += 32
    if offset != len(payload):
        raise ValueError("encrypted note payload has trailing bytes")
    return EncryptedNoteEnvelope(
        recipient=recipient,
        ciphertext=ciphertext,
        ephemeral_pk=ephemeral_pk,
    )


def parse_shielded_note_plaintext(plaintext: bytes) -> ShieldedNote:
    if len(plaintext) != 116:
        raise ValueError("malformed note plaintext")
    offset = 0
    value = int.from_bytes(plaintext[offset : offset + 16], "little")
    offset += 16
    asset_id = int.from_bytes(plaintext[offset : offset + 4], "little")
    offset += 4
    owner_pk = _bytes_to_hex(plaintext[offset : offset + 32])
    offset += 32
    rho = _bytes_to_hex(plaintext[offset : offset + 32])
    offset += 32
    psi = _bytes_to_hex(plaintext[offset : offset + 32])
    return ShieldedNote(
        value=value,
        asset_id=asset_id,
        owner_pk=owner_pk,
        rho=rho,
        psi=psi,
    )


def scan_granted_notes(
    provider: PrimeProvider,
    granted_viewing_material: GrantedViewingMaterial,
    *,
    limit: Optional[int] = None,
    cursor_hex: Optional[str] = None,
    max_pages: Optional[int] = None,
    ignore_malformed: bool = True,
) -> GrantedNoteScanResult:
    fetched_encrypted_note_count = 0
    total_encrypted_note_count = 0
    block_number = 0
    next_cursor = cursor_hex
    skipped_malformed_count = 0
    decrypted_notes: List[GrantedDecryptedNote] = []
    page_count = 0

    while max_pages is None or page_count < max_pages:
        page = provider.view_notes(
            granted_viewing_material.grant_id_hex,
            limit=limit,
            cursor_hex=next_cursor,
        )
        total_encrypted_note_count = page.total_encrypted_note_count
        fetched_encrypted_note_count += page.returned_encrypted_note_count
        block_number = page.block_number
        next_cursor = page.next_cursor

        for entry in page.notes:
            try:
                envelope = parse_encrypted_note_payload(entry.encrypted_note)
            except ValueError:
                if ignore_malformed:
                    skipped_malformed_count += 1
                    continue
                raise

            if (
                granted_viewing_material.recipient_public_key
                and not _hex_equal(envelope.recipient, granted_viewing_material.recipient_public_key)
            ):
                continue

            plaintext = granted_viewing_material.decrypt_note_ciphertext(
                GrantedNoteDecryptInput(
                    note_commitment=entry.note_commitment,
                    encrypted_note_hex=entry.encrypted_note,
                    envelope=envelope,
                )
            )
            if not plaintext:
                continue

            try:
                note = parse_shielded_note_plaintext(plaintext)
            except ValueError:
                if ignore_malformed:
                    skipped_malformed_count += 1
                    continue
                raise

            decrypted_notes.append(
                GrantedDecryptedNote(
                    note_commitment=entry.note_commitment,
                    envelope=envelope,
                    note=note,
                )
            )

        page_count += 1
        if page.next_cursor is None:
            break

    return GrantedNoteScanResult(
        grant_id=granted_viewing_material.grant_id_hex,
        block_number=block_number,
        total_encrypted_note_count=total_encrypted_note_count,
        fetched_encrypted_note_count=fetched_encrypted_note_count,
        next_cursor=next_cursor,
        skipped_malformed_count=skipped_malformed_count,
        notes=decrypted_notes,
    )


def make_mock_note_decryptor(view_secret_hex: str) -> Callable[[GrantedNoteDecryptInput], Optional[bytes]]:
    """Return the example/mock decryptor used by the SDK examples.

    This is not production viewing-key cryptography.
    """

    def decrypt(input: GrantedNoteDecryptInput) -> bytes:
        return xor_bytes(
            input.envelope.ciphertext,
            expand_mock_key(
                derive_mock_shared_secret(view_secret_hex, input.envelope.ephemeral_pk),
                len(input.envelope.ciphertext),
            ),
        )

    return decrypt


def _decode_hex_bytes(value: str) -> bytes:
    trimmed = value[2:] if value.startswith("0x") else value
    if len(trimmed) % 2 != 0:
        raise ValueError("hex string must have an even number of characters")
    return bytes.fromhex(trimmed)


def _bytes_to_hex(value: bytes) -> str:
    return "0x" + value.hex()


def _hex_equal(left: str, right: str) -> bool:
    return left.removeprefix("0x").lower() == right.removeprefix("0x").lower()


def derive_mock_shared_secret(view_secret_hex: str, ephemeral_pk_hex: str) -> bytes:
    return hashlib.sha256(_decode_hex_bytes(view_secret_hex) + _decode_hex_bytes(ephemeral_pk_hex)).digest()


def expand_mock_key(seed: bytes, length: int) -> bytes:
    key = bytearray()
    block = hashlib.sha256(seed).digest()
    while len(key) < length:
        key.extend(block)
        block = hashlib.sha256(block).digest()
    return bytes(key[:length])


def xor_bytes(left: bytes, right: bytes) -> bytes:
    return bytes(a ^ b for a, b in zip(left, right))