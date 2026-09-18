use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum MersennetOrdersError {
    #[error("unknown market")]
    UnknownMarket,
    #[error("account is not liquidatable")]
    NotLiquidatable,
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
    #[error("post-only order would cross the book")]
    PostOnlyWouldCross,
    #[error("post-only/expiry flags require GTC time-in-force")]
    InvalidOrderFlags,
    #[error("market symbol already exists")]
    DuplicateMarket,
    #[error("invalid market parameters")]
    InvalidMarketParams,
    #[error("token is not a registered collateral asset")]
    UnknownCollateralAsset,
    #[error("agent delegation: {0}")]
    Agent(&'static str),
}

impl MersennetOrdersError {
    pub fn message(&self) -> &'static str {
        match self {
            MersennetOrdersError::UnknownMarket => "unknown market",
            MersennetOrdersError::NotLiquidatable => "account is not liquidatable",
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
            MersennetOrdersError::PostOnlyWouldCross => "post-only order would cross the book",
            MersennetOrdersError::InvalidOrderFlags => {
                "post-only/expiry flags require GTC time-in-force"
            }
            MersennetOrdersError::DuplicateMarket => "market symbol already exists",
            MersennetOrdersError::InvalidMarketParams => "invalid market parameters",
            MersennetOrdersError::UnknownCollateralAsset => {
                "token is not a registered collateral asset"
            }
            MersennetOrdersError::Agent(msg) => msg,
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
