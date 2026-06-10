package mersennet

import (
	"encoding/binary"
	"encoding/hex"
	"math/big"
	"testing"
)

func TestNewMockNoteDecryptorRoundTripsNotePayload(t *testing.T) {
	viewSecretHex := "0x" + repeatHexByte('3')
	recipientHex := "0x" + repeatHexByte('2')
	ephemeralPkHex := "0x" + repeatHexByte('4')
	note := ShieldedNote{
		Value:   big.NewInt(2500),
		AssetID: 7,
		OwnerPK: recipientHex,
		Rho:     "0x" + repeatHexByte('5'),
		Psi:     "0x" + repeatHexByte('6'),
	}

	plaintext := encodeTestNotePlaintext(note)
	seed := deriveMockSharedSecret(viewSecretHex, ephemeralPkHex)
	ciphertext := xorBytes(plaintext, expandMockKey(seed, len(plaintext)))
	payloadHex := encodeTestEncryptedPayload(recipientHex, ciphertext, ephemeralPkHex)
	envelope, err := ParseEncryptedNotePayload(payloadHex)
	if err != nil {
		t.Fatalf("parse payload: %v", err)
	}

	decryptor := NewMockNoteDecryptor(viewSecretHex)
	decrypted, err := decryptor(GrantedNoteDecryptInput{
		NoteCommitment:   "0x" + repeatHexByte('9'),
		EncryptedNoteHex: payloadHex,
		Envelope:         *envelope,
	})
	if err != nil {
		t.Fatalf("decrypt: %v", err)
	}

	parsed, err := ParseShieldedNotePlaintext(decrypted)
	if err != nil {
		t.Fatalf("parse plaintext: %v", err)
	}
	if parsed.Value.String() != "2500" {
		t.Fatalf("unexpected value: %s", parsed.Value.String())
	}
	if parsed.AssetID != 7 {
		t.Fatalf("unexpected asset id: %d", parsed.AssetID)
	}
	if parsed.OwnerPK != recipientHex {
		t.Fatalf("unexpected owner pk: %s", parsed.OwnerPK)
	}
	if parsed.Rho != note.Rho {
		t.Fatalf("unexpected rho: %s", parsed.Rho)
	}
	if parsed.Psi != note.Psi {
		t.Fatalf("unexpected psi: %s", parsed.Psi)
	}
}

func TestParseEncryptedNotePayloadRejectsMalformedPayload(t *testing.T) {
	if _, err := ParseEncryptedNotePayload("0x1234"); err == nil {
		t.Fatal("expected malformed payload error")
	}
}

func encodeTestNotePlaintext(note ShieldedNote) []byte {
	out := make([]byte, 116)
	offset := 0
	copy(out[offset:offset+16], padLittleEndian(note.Value.Bytes(), 16))
	offset += 16
	binary.LittleEndian.PutUint32(out[offset:offset+4], note.AssetID)
	offset += 4
	copy(out[offset:offset+32], mustDecodeHexLenient(note.OwnerPK))
	offset += 32
	copy(out[offset:offset+32], mustDecodeHexLenient(note.Rho))
	offset += 32
	copy(out[offset:offset+32], mustDecodeHexLenient(note.Psi))
	return out
}

func encodeTestEncryptedPayload(recipient string, ciphertext []byte, ephemeralPk string) string {
	length := make([]byte, 8)
	binary.LittleEndian.PutUint64(length, uint64(len(ciphertext)))
	payload := append([]byte{}, mustDecodeHexLenient(recipient)...)
	payload = append(payload, length...)
	payload = append(payload, ciphertext...)
	payload = append(payload, mustDecodeHexLenient(ephemeralPk)...)
	return "0x" + hex.EncodeToString(payload)
}

func padLittleEndian(value []byte, length int) []byte {
	out := make([]byte, length)
	for index := range value {
		out[index] = value[len(value)-1-index]
	}
	return out
}

func repeatHexByte(ch byte) string {
	return string(makeRepeatedBytes(ch, 64))
}

func makeRepeatedBytes(ch byte, count int) []byte {
	out := make([]byte, count)
	for index := range out {
		out[index] = ch
	}
	return out
}