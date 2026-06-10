package mersennet

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
)

// MersennetError represents an RPC or SDK error
type MersennetError struct {
	Message string
	Code    int
}

func (e *MersennetError) Error() string {
	return e.Message
}

// Provider is the JSON-RPC client for Mersennet
type Provider struct {
	URL string
	id  int
}

// NewProvider creates a new Provider
func NewProvider(rpcURL string) *Provider {
	return &Provider{URL: rpcURL}
}

type rpcRequest struct {
	JSONRPC string        `json:"jsonrpc"`
	ID      int           `json:"id"`
	Method  string        `json:"method"`
	Params  []interface{} `json:"params"`
}

type rpcResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      int             `json:"id"`
	Result  json.RawMessage `json:"result"`
	Error   *struct {
		Code    int    `json:"code"`
		Message string `json:"message"`
	} `json:"error,omitempty"`
}

func (p *Provider) request(method string, params []interface{}) (json.RawMessage, error) {
	p.id++
	req := rpcRequest{
		JSONRPC: "2.0",
		ID:      p.id,
		Method:  method,
		Params:  params,
	}
	body, err := json.Marshal(req)
	if err != nil {
		return nil, err
	}
	resp, err := http.Post(p.URL, "application/json", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	var rpcResp rpcResponse
	if err := json.Unmarshal(data, &rpcResp); err != nil {
		return nil, err
	}
	if rpcResp.Error != nil {
		return nil, &MersennetError{
			Message: rpcResp.Error.Message,
			Code:    rpcResp.Error.Code,
		}
	}
	return rpcResp.Result, nil
}

// GetBlock returns a block by number (use "latest" for latest)
func (p *Provider) GetBlock(number interface{}, includeTxs bool) (*Block, error) {
	var tag string
	switch v := number.(type) {
	case string:
		tag = v
	case int:
		tag = fmt.Sprintf("0x%x", v)
	default:
		tag = "latest"
	}
	result, err := p.request("eth_getBlockByNumber", []interface{}{tag, includeTxs})
	if err != nil {
		return nil, err
	}
	if bytes.Equal(result, []byte("null")) {
		return nil, nil
	}
	var block Block
	if err := json.Unmarshal(result, &block); err != nil {
		return nil, err
	}
	return &block, nil
}

// GetBlockByHash returns a block by hash
func (p *Provider) GetBlockByHash(hash string, includeTxs bool) (*Block, error) {
	result, err := p.request("eth_getBlockByHash", []interface{}{hash, includeTxs})
	if err != nil {
		return nil, err
	}
	if bytes.Equal(result, []byte("null")) {
		return nil, nil
	}
	var block Block
	if err := json.Unmarshal(result, &block); err != nil {
		return nil, err
	}
	return &block, nil
}

// GetTransaction returns a transaction by hash
func (p *Provider) GetTransaction(hash string) (*Transaction, error) {
	result, err := p.request("eth_getTransactionByHash", []interface{}{hash})
	if err != nil {
		return nil, err
	}
	if bytes.Equal(result, []byte("null")) {
		return nil, nil
	}
	var tx Transaction
	if err := json.Unmarshal(result, &tx); err != nil {
		return nil, err
	}
	return &tx, nil
}

// GetBalance returns the balance of an address
func (p *Provider) GetBalance(address string) (string, error) {
	result, err := p.request("eth_getBalance", []interface{}{address})
	if err != nil {
		return "", err
	}
	var s string
	if err := json.Unmarshal(result, &s); err != nil {
		return "", err
	}
	return s, nil
}

// GetNonce returns the nonce of an address
func (p *Provider) GetNonce(address string) (uint64, error) {
	result, err := p.request("eth_getTransactionCount", []interface{}{address, "latest"})
	if err != nil {
		return 0, err
	}
	var s string
	if err := json.Unmarshal(result, &s); err != nil {
		return 0, err
	}
	var n uint64
	if _, err := fmt.Sscanf(s, "0x%x", &n); err != nil {
		return 0, err
	}
	return n, nil
}

// SendRawTransaction sends a raw signed transaction
func (p *Provider) SendRawTransaction(rawTx string) (string, error) {
	result, err := p.request("eth_sendRawTransaction", []interface{}{rawTx})
	if err != nil {
		return "", err
	}
	var s string
	if err := json.Unmarshal(result, &s); err != nil {
		return "", err
	}
	return s, nil
}

// Call performs eth_call
func (p *Provider) Call(txObject map[string]interface{}) (string, error) {
	result, err := p.request("eth_call", []interface{}{txObject})
	if err != nil {
		return "", err
	}
	var s string
	if err := json.Unmarshal(result, &s); err != nil {
		return "", err
	}
	return s, nil
}

// ChainID returns the chain ID
func (p *Provider) ChainID() (uint64, error) {
	result, err := p.request("eth_chainId", nil)
	if err != nil {
		return 0, err
	}
	var s string
	if err := json.Unmarshal(result, &s); err != nil {
		return 0, err
	}
	var n uint64
	if _, err := fmt.Sscanf(s, "0x%x", &n); err != nil {
		return 0, err
	}
	return n, nil
}

// BlockNumber returns the latest block number
func (p *Provider) BlockNumber() (uint64, error) {
	result, err := p.request("eth_blockNumber", nil)
	if err != nil {
		return 0, err
	}
	var s string
	if err := json.Unmarshal(result, &s); err != nil {
		return 0, err
	}
	var n uint64
	if _, err := fmt.Sscanf(s, "0x%x", &n); err != nil {
		return 0, err
	}
	return n, nil
}

// ViewNotes returns grant-gated encrypted notes from prime_viewNotes.
func (p *Provider) ViewNotes(grantID string, limit *int, cursorHex *string) (*ViewNotesResult, error) {
	request := map[string]interface{}{
		"grantIdHex": grantID,
	}
	if limit != nil {
		request["limit"] = *limit
	}
	if cursorHex != nil && *cursorHex != "" {
		request["cursorHex"] = *cursorHex
	}
	result, err := p.request("prime_viewNotes", []interface{}{request})
	if err != nil {
		return nil, err
	}
	var out ViewNotesResult
	if err := json.Unmarshal(result, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// GasPrice returns the current gas price
func (p *Provider) GasPrice() (string, error) {
	result, err := p.request("eth_gasPrice", nil)
	if err != nil {
		return "", err
	}
	var s string
	if err := json.Unmarshal(result, &s); err != nil {
		return "", err
	}
	return s, nil
}
