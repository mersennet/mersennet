use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum PrimeOrdersError {
    #[error("unknown market")]
    UnknownMarket,
    #[error("size must be > 0")]
    InvalidSize,
    #[error("fok not fillable")]
    FokNotFillable,
    #[error("insufficient collateral for initial margin")]
    InsufficientCollateral,
}

impl PrimeOrdersError {
    pub fn message(&self) -> &'static str {
        match self {
            PrimeOrdersError::UnknownMarket => "unknown market",
            PrimeOrdersError::InvalidSize => "size must be > 0",
            PrimeOrdersError::FokNotFillable => "fok not fillable",
            PrimeOrdersError::InsufficientCollateral => "insufficient collateral for initial margin",
        }
    }
}

#[derive(Debug, Clone, Error)]
pub enum RpcInputError {
    #[error("invalid params")]
    InvalidParams,
    #[error("transaction object required")]
    TransactionRequired,
    #[error("invalid block tag")]
    InvalidBlockTag,
    #[error("invalid address")]
    InvalidAddress,
    #[error("invalid address length")]
    InvalidAddressLength,
    #[error("hash required")]
    HashRequired,
    #[error("invalid hash length")]
    InvalidHashLength,
    #[error("invalid topic")]
    InvalidTopic,
    #[error("hex decode error: {0}")]
    HexDecode(String),
    #[error("invalid hex: {0}")]
    InvalidHex(String),
    #[error("invalid log filter: {0}")]
    InvalidLogFilter(String),
    #[error("invalid transaction: {0}")]
    InvalidTransaction(String),
}
