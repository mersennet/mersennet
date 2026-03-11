/// Chain Feature Flags
///
/// Controls activation of new chain-level features.
/// Each flag should be toggled via governance or config once the feature is production-ready.

/// Phase 1: Zero Gas for CLOB Operations
/// When enabled, transactions to the CLOB precompile (placeOrder, cancelOrder)
/// are gas-exempt. Users only pay trading fees.
pub const FEATURE_ZERO_GAS_CLOB: bool = false;

/// Phase 3: Native Advanced Orders (TP/SL, Trailing Stop, Stop Limit)
/// When enabled, the matching engine processes trigger orders natively
/// instead of relying on client-side monitoring.
pub const FEATURE_NATIVE_ADVANCED_ORDERS: bool = false;

/// Phase 3: Spot Trading
/// When enabled, the CLOB precompile accepts spot market operations
/// alongside perpetual orders.
pub const FEATURE_SPOT_TRADING: bool = false;

/// Phase 3: On-Chain Sub-Accounts
/// When enabled, the precompile supports creating and managing sub-accounts
/// with isolated margin per account.
pub const FEATURE_ON_CHAIN_SUB_ACCOUNTS: bool = false;

/// Phase 2: Multi-Asset Collateral
/// When enabled, the precompile accepts USDC, USDT, ETH, BTC as collateral
/// with oracle-based pricing.
pub const FEATURE_MULTI_ASSET_COLLATERAL: bool = false;

/// Phase 4: Frequent Batch Auctions (FBA)
/// The FBAEngine is already implemented. This flag wires it into execute_block().
/// When enabled, orders are batched per block and executed at uniform clearing price.
pub const FEATURE_FBA_ACTIVE: bool = false;

/// Phase 4: Encrypted Mempool
/// The EncryptedMempool module exists. This flag activates commit-reveal
/// for order submissions, preventing front-running at the protocol level.
pub const FEATURE_ENCRYPTED_MEMPOOL: bool = false;

/// Selectors that are gas-exempt when FEATURE_ZERO_GAS_CLOB is enabled.
/// These correspond to placeOrder and cancelOrder precompile functions.
pub const ZERO_GAS_SELECTORS: [[u8; 4]; 2] = [
    [0x00, 0x00, 0x00, 0x00], // Placeholder: will be set to actual selectors
    [0x00, 0x00, 0x00, 0x00], // Placeholder: will be set to actual selectors
];

/// Returns whether a given precompile call should be gas-exempt
pub fn is_gas_exempt_selector(selector: &[u8; 4]) -> bool {
    if !FEATURE_ZERO_GAS_CLOB {
        return false;
    }
    ZERO_GAS_SELECTORS.iter().any(|s| s == selector)
}
