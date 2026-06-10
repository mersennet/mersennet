package main


import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"log"
	"math/big"
	"net/http"
	"net/http/httptest"

	mersennet "github.com/PrimeNumbersLabs/prime-chain-sdk-go"
)

const (
	grantIDHex         = "0x1111111111111111111111111111111111111111111111111111111111111111"
	recipientPublicKey = "0x2222222222222222222222222222222222222222222222222222222222222222"
	viewSecretHex      = "0x3333333333333333333333333333333333333333333333333333333333333333"
	ephemeralPkHex     = "0x4444444444444444444444444444444444444444444444444444444444444444"
)

func main() {
	note := sampleNote()
	plaintext := encodeNotePlaintext(note)
	key := expandKey(deriveSharedSecret(viewSecretHex, ephemeralPkHex), len(plaintext))
	ciphertext := xorBytes(plaintext, key)
	encryptedNoteHex := encodeEncryptedNotePayload(recipientPublicKey, ciphertext, ephemeralPkHex)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var request map[string]any
		_ = json.NewDecoder(r.Body).Decode(&request)
		response := map[string]any{
			"jsonrpc": "2.0",
			"id":      request["id"],
			"result": map[string]any{
				"grantId":                  grantIDHex,
				"grantorCommitment":        repeatedHex('7'),
				"blockNumber":              42,
				"shieldedStateRoot":        repeatedHex('8'),
				"totalEncryptedNoteCount":  1,
				"returnedEncryptedNoteCount": 1,
				"nextCursor":               nil,
				"notes": []map[string]any{{
					"noteCommitment": repeatedHex('9'),
					"encryptedNote":  encryptedNoteHex,
				}},
				"signatureVerified": true,
			},
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	provider := mersennet.NewProvider(server.URL)
	result, err := mersennet.ScanGrantedNotes(provider, mersennet.GrantedViewingMaterial{
		GrantID:            grantIDHex,
		RecipientPublicKey: recipientPublicKey,
		Decrypt:            mersennet.NewMockNoteDecryptor(viewSecretHex),
	}, &mersennet.GrantedNoteScanOptions{Limit: 64, IgnoreMalformed: true})
	if err != nil {
		log.Fatal(err)
	}

	if len(result.Notes) == 0 {
		log.Fatal("no decrypted notes returned")
	}

	fmt.Printf("Recovered %d note(s)\n", len(result.Notes))
	fmt.Printf("First note value=%s asset=%d owner=%s\n",
		result.Notes[0].Note.Value.String(),
		result.Notes[0].Note.AssetID,
		result.Notes[0].Note.OwnerPK,
	)
}

func sampleNote() mersennet.ShieldedNote {
	return mersennet.ShieldedNote{
		Value:   big.NewInt(2500),
		AssetID: 7,
		OwnerPK: recipientPublicKey,
		Rho:     "0x5555555555555555555555555555555555555555555555555555555555555555",
		Psi:     "0x6666666666666666666666666666666666666666666666666666666666666666",
	}
}

func encodeNotePlaintext(note mersennet.ShieldedNote) []byte {
	out := make([]byte, 116)
	offset := 0
	copy(out[offset:offset+16], padLittleEndian(note.Value.Bytes(), 16))
	offset += 16
	binary.LittleEndian.PutUint32(out[offset:offset+4], note.AssetID)
	offset += 4
	copy(out[offset:offset+32], mustDecodeHex(note.OwnerPK))
	offset += 32
	copy(out[offset:offset+32], mustDecodeHex(note.Rho))
	offset += 32
	copy(out[offset:offset+32], mustDecodeHex(note.Psi))
	return out
}

func encodeEncryptedNotePayload(recipient string, ciphertext []byte, ephemeralPk string) string {
	length := make([]byte, 8)
	binary.LittleEndian.PutUint64(length, uint64(len(ciphertext)))
	payload := append([]byte{}, mustDecodeHex(recipient)...)
	payload = append(payload, length...)
	payload = append(payload, ciphertext...)
	payload = append(payload, mustDecodeHex(ephemeralPk)...)
	return "0x" + hex.EncodeToString(payload)
}

func deriveSharedSecret(viewSecret string, ephemeralPk string) []byte {
	payload := append([]byte{}, mustDecodeHex(viewSecret)...)
	payload = append(payload, mustDecodeHex(ephemeralPk)...)
	sum := sha256.Sum256(payload)
	return sum[:]
}

func expandKey(seed []byte, length int) []byte {
	key := make([]byte, 0, length)
	hash := sha256.Sum256(seed)
	chunk := hash[:]
	for len(key) < length {
		key = append(key, chunk...)
		next := sha256.Sum256(chunk)
		chunk = next[:]
	}
	return key[:length]
}

func xorBytes(left []byte, right []byte) []byte {
	out := make([]byte, len(left))
	for index := range left {
		out[index] = left[index] ^ right[index]
	}
	return out
}

func mustDecodeHex(value string) []byte {
	trimmed := value
	if len(trimmed) >= 2 && trimmed[:2] == "0x" {
		trimmed = trimmed[2:]
	}
	decoded, err := hex.DecodeString(trimmed)
	if err != nil {
		panic(err)
	}
	return decoded
}

func padLittleEndian(value []byte, length int) []byte {
	out := make([]byte, length)
	for index := range value {
		out[index] = value[len(value)-1-index]
	}
	return out
}

func repeatedHex(ch byte) string {
	return "0x" + string(bytes.Repeat([]byte{ch}, 64))
}