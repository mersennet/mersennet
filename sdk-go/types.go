package mersennet

// Block represents block data from eth_getBlockByNumber / eth_getBlockByHash
type Block struct {
	Number        string        `json:"number"`
	Hash          string        `json:"hash"`
	GasLimit      string        `json:"gas_limit"`
	GasUsed       string        `json:"gas_used"`
	BaseFee       string        `json:"base_fee"`
	StateRoot     string        `json:"state_root"`
	Transactions  []interface{} `json:"transactions"`
	DomainEvents  []interface{} `json:"domain_events,omitempty"`
}

// Transaction represents transaction data
type Transaction struct {
	Hash     string  `json:"hash"`
	From     string  `json:"from"`
	To       *string `json:"to"`
	Value    string  `json:"value"`
	Nonce    string  `json:"nonce"`
	Gas      string  `json:"gas"`
	GasPrice string  `json:"gas_price"`
	Input    string  `json:"input"`
}

// Receipt represents a transaction receipt
type Receipt struct {
	TransactionHash  string   `json:"transaction_hash"`
	BlockHash        string   `json:"block_hash"`
	BlockNumber      string   `json:"block_number"`
	TransactionIndex string   `json:"transaction_index"`
	GasUsed          string   `json:"gas_used"`
	Status           string   `json:"status"`
	ContractAddress  *string  `json:"contract_address"`
	Output           string   `json:"output"`
	Logs             []LogEntry `json:"logs"`
}

// LogEntry represents a log entry
type LogEntry struct {
	Address          string   `json:"address"`
	Topics           []string `json:"topics"`
	Data             string   `json:"data"`
	BlockNumber      string   `json:"block_number"`
	BlockHash        string   `json:"block_hash"`
	TransactionHash  string   `json:"transaction_hash"`
	TransactionIndex string   `json:"transaction_index"`
	LogIndex         string   `json:"log_index"`
}

// Order represents an order in the order book
type Order struct {
	ID       string `json:"id"`
	Owner    string `json:"owner"`
	MarketID string `json:"market_id"`
	Side     string `json:"side"`
	Price    string `json:"price"`
	Size     string `json:"size"`
	TIF      string `json:"tif"`
}

// OrderBookLevel represents a price level
type OrderBookLevel struct {
	Price string `json:"price"`
	Size  string `json:"size"`
}

// OrderBook represents the full order book
type OrderBook struct {
	Bids []OrderBookLevel `json:"bids"`
	Asks []OrderBookLevel `json:"asks"`
}

// Trade represents a trade execution
type Trade struct {
	Taker    string `json:"taker"`
	Maker    string `json:"maker"`
	MarketID string `json:"market_id"`
	Side     string `json:"side"`
	Price    string `json:"price"`
	Size     string `json:"size"`
}

// ViewNotesEntry represents one encrypted note returned by prime_viewNotes.
type ViewNotesEntry struct {
	NoteCommitment string `json:"noteCommitment"`
	EncryptedNote  string `json:"encryptedNote"`
}

// ViewNotesResult represents the prime_viewNotes response.
type ViewNotesResult struct {
	GrantID                string           `json:"grantId"`
	GrantorCommitment      string           `json:"grantorCommitment"`
	BlockNumber            uint64           `json:"blockNumber"`
	ShieldedStateRoot      string           `json:"shieldedStateRoot"`
	TotalEncryptedNoteCount int             `json:"totalEncryptedNoteCount"`
	ReturnedEncryptedNoteCount int          `json:"returnedEncryptedNoteCount"`
	NextCursor             *string          `json:"nextCursor"`
	Notes                  []ViewNotesEntry `json:"notes"`
	SignatureVerified      bool             `json:"signatureVerified"`
}
