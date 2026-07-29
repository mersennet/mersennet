use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum MersennetOrdersError {
    #[error("unknown market")]
    UnknownMarket,
    #[error("size must be > 0")]
    InvalidSize,
    #[error("fok not fillable")]
    FokNotFillable,
    #[error("insufficient collateral for initial margin")]
    InsufficientCollateral,
    #[error("insufficient equity")]
    InsufficientEquity,
    #[error("market is halted")]
    MarketHalted,
    #[error("withdrawal would bring equity below maintenance margin")]
    WithdrawalExceedsEquity,
    #[error("caller does not own this order")]
    NotOrderOwner,
}

impl MersennetOrdersError {
    pub fn message(&self) -> &'static str {
        match self {
            MersennetOrdersError::UnknownMarket => "unknown market",
            MersennetOrdersError::InvalidSize => "size must be > 0",
            MersennetOrdersError::FokNotFillable => "fok not fillable",
            MersennetOrdersError::InsufficientCollateral => {
                "insufficient collateral for initial margin"
            }
            MersennetOrdersError::InsufficientEquity => "insufficient equity",
            MersennetOrdersError::MarketHalted => "market is halted",
            MersennetOrdersError::WithdrawalExceedsEquity => {
                "withdrawal would bring equity below maintenance margin"
            }
            MersennetOrdersError::NotOrderOwner => "caller does not own this order",
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
