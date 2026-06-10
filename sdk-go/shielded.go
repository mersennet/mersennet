package mersennet

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"math/big"
	"strings"
)

// EncryptedNoteEnvelope is the canonical encrypted note wrapper emitted by Mersennet.
type EncryptedNoteEnvelope struct {
	Recipient   string
	Ciphertext  []byte
	EphemeralPK string
}

// ShieldedNote is the decoded note plaintext recovered client-side after decryption.
type ShieldedNote struct {
	Value   *big.Int
	AssetID uint32
	OwnerPK string
	Rho     string
	Psi     string
}

// GrantedNoteDecryptInput is the input passed to a caller-supplied note decryptor.
type GrantedNoteDecryptInput struct {
	NoteCommitment   string
	EncryptedNoteHex string
	Envelope         EncryptedNoteEnvelope
}

// GrantedViewingMaterial carries the grant id plus the decryptor that uses the delegated viewing material.
type GrantedViewingMaterial struct {
	GrantID            string
	RecipientPublicKey string
	Decrypt            func(GrantedNoteDecryptInput) ([]byte, error)
}

// GrantedNoteScanOptions controls pagination for note scanning.
type GrantedNoteScanOptions struct {
	Limit           int
	CursorHex       string
	MaxPages        int
	IgnoreMalformed bool
}

// GrantedDecryptedNote is one successfully decrypted note record.
type GrantedDecryptedNote struct {
	NoteCommitment string
	Envelope       EncryptedNoteEnvelope
	Note           ShieldedNote
}

// GrantedNoteScanResult is the aggregated result of scanning prime_viewNotes pages.
type GrantedNoteScanResult struct {
	GrantID                string
	BlockNumber            uint64
	TotalEncryptedNoteCount int
	FetchedEncryptedNoteCount int
	NextCursor             string
	SkippedMalformedCount  int
	Notes                  []GrantedDecryptedNote
}

// ParseEncryptedNotePayload decodes the bincode-encoded EncryptedNote envelope returned by prime_viewNotes.
func ParseEncryptedNotePayload(payloadHex string) (*EncryptedNoteEnvelope, error) {
	payload, err := decodeHexBytes(payloadHex)
	if err != nil {
		return nil, err
	}
	if len(payload) < 72 {
		return nil, fmt.Errorf("malformed encrypted note payload")
	}
	offset := 0
	recipient := bytesToHex(payload[offset : offset+32])
	offset += 32
	if offset+8 > len(payload) {
		return nil, fmt.Errorf("malformed encrypted note payload")
	}
	ciphertextLen := binary.LittleEndian.Uint64(payload[offset : offset+8])
	offset += 8
	if offset+int(ciphertextLen)+32 > len(payload) {
		return nil, fmt.Errorf("malformed encrypted note payload")
	}
	ciphertext := append([]byte(nil), payload[offset:offset+int(ciphertextLen)]...)
	offset += int(ciphertextLen)
	ephemeralPK := bytesToHex(payload[offset : offset+32])
	offset += 32
	if offset != len(payload) {
		return nil, fmt.Errorf("encrypted note payload has trailing bytes")
	}
	return &EncryptedNoteEnvelope{
		Recipient:   recipient,
		Ciphertext:  ciphertext,
		EphemeralPK: ephemeralPK,
	}, nil
}

// ParseShieldedNotePlaintext decodes a bincode-encoded Note plaintext.
func ParseShieldedNotePlaintext(plaintext []byte) (*ShieldedNote, error) {
	if len(plaintext) != 116 {
		return nil, fmt.Errorf("malformed note plaintext")
	}
	offset := 0
	value := littleEndianBigInt(plaintext[offset : offset+16])
	offset += 16
	assetID := binary.LittleEndian.Uint32(plaintext[offset : offset+4])
	offset += 4
	ownerPK := bytesToHex(plaintext[offset : offset+32])
	offset += 32
	rho := bytesToHex(plaintext[offset : offset+32])
	offset += 32
	psi := bytesToHex(plaintext[offset : offset+32])
	return &ShieldedNote{
		Value:   value,
		AssetID: assetID,
		OwnerPK: ownerPK,
		Rho:     rho,
		Psi:     psi,
	}, nil
}

// ScanGrantedNotes pages through prime_viewNotes and locally decrypts matching note envelopes.
func ScanGrantedNotes(provider *Provider, material GrantedViewingMaterial, opts *GrantedNoteScanOptions) (*GrantedNoteScanResult, error) {
	if material.Decrypt == nil {
		return nil, fmt.Errorf("granted viewing material must provide a decrypt function")
	}
	ignoreMalformed := true
	if opts != nil {
		ignoreMalformed = opts.IgnoreMalformed
	}
	var limitPtr *int
	var cursorPtr *string
	var maxPages int
	if opts != nil {
		if opts.Limit > 0 {
			limitPtr = &opts.Limit
		}
		if opts.CursorHex != "" {
			cursorPtr = &opts.CursorHex
		}
		maxPages = opts.MaxPages
	}
	pages := 0
	result := &GrantedNoteScanResult{
		GrantID: material.GrantID,
		Notes:   make([]GrantedDecryptedNote, 0),
	}

	for {
		page, err := provider.ViewNotes(material.GrantID, limitPtr, cursorPtr)
		if err != nil {
			return nil, err
		}
		result.BlockNumber = page.BlockNumber
		result.TotalEncryptedNoteCount = page.TotalEncryptedNoteCount
		result.FetchedEncryptedNoteCount += page.ReturnedEncryptedNoteCount

		for _, entry := range page.Notes {
			envelope, err := ParseEncryptedNotePayload(entry.EncryptedNote)
			if err != nil {
				if ignoreMalformed {
					result.SkippedMalformedCount++
					continue
				}
				return nil, err
			}
			if material.RecipientPublicKey != "" && !hexEqual(envelope.Recipient, material.RecipientPublicKey) {
				continue
			}
			plaintext, err := material.Decrypt(GrantedNoteDecryptInput{
				NoteCommitment:   entry.NoteCommitment,
				EncryptedNoteHex: entry.EncryptedNote,
				Envelope:         *envelope,
			})
			if err != nil {
				return nil, err
			}
			if len(plaintext) == 0 {
				continue
			}
			note, err := ParseShieldedNotePlaintext(plaintext)
			if err != nil {
				if ignoreMalformed {
					result.SkippedMalformedCount++
					continue
				}
				return nil, err
			}
			result.Notes = append(result.Notes, GrantedDecryptedNote{
				NoteCommitment: entry.NoteCommitment,
				Envelope:       *envelope,
				Note:           *note,
			})
		}

		pages++
		if page.NextCursor == nil {
			result.NextCursor = ""
			break
		}
		result.NextCursor = *page.NextCursor
		cursorPtr = page.NextCursor
		if maxPages > 0 && pages >= maxPages {
			break
		}
	}

	return result, nil
}

func decodeHexBytes(value string) ([]byte, error) {
	trimmed := strings.TrimPrefix(value, "0x")
	if len(trimmed)%2 != 0 {
		return nil, fmt.Errorf("hex string must have an even number of characters")
	}
	decoded, err := hex.DecodeString(trimmed)
	if err != nil {
		return nil, err
	}
	return decoded, nil
}

func bytesToHex(value []byte) string {
	return "0x" + hex.EncodeToString(value)
}

func littleEndianBigInt(value []byte) *big.Int {
	reversed := make([]byte, len(value))
	for index := range value {
		reversed[len(value)-1-index] = value[index]
	}
	return new(big.Int).SetBytes(reversed)
}

func hexEqual(left string, right string) bool {
	return strings.EqualFold(strings.TrimPrefix(left, "0x"), strings.TrimPrefix(right, "0x"))
}

// NewMockNoteDecryptor returns the example/mock decryptor used by the
// SDK examples. This is not production viewing-key cryptography.
func NewMockNoteDecryptor(viewSecretHex string) func(GrantedNoteDecryptInput) ([]byte, error) {
	return func(input GrantedNoteDecryptInput) ([]byte, error) {
		seed := deriveMockSharedSecret(viewSecretHex, input.Envelope.EphemeralPK)
		key := expandMockKey(seed, len(input.Envelope.Ciphertext))
		return xorBytes(input.Envelope.Ciphertext, key), nil
	}
}

func deriveMockSharedSecret(viewSecretHex string, ephemeralPk string) []byte {
	payload := append([]byte{}, mustDecodeHexLenient(viewSecretHex)...)
	payload = append(payload, mustDecodeHexLenient(ephemeralPk)...)
	sum := sha256.Sum256(payload)
	return sum[:]
}

func expandMockKey(seed []byte, length int) []byte {
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

func mustDecodeHexLenient(value string) []byte {
	decoded, err := decodeHexBytes(value)
	if err != nil {
		panic(err)
	}
	return decoded
}