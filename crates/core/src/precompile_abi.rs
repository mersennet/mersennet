use revm::primitives::{keccak256, Address, U256};

/// Precompile address: 0x0000000000000000000000000000000000000100
pub const PRIME_ORDERS_PRECOMPILE: Address = {
    let mut addr = [0u8; 20];
    addr[18] = 0x01;
    // addr[19] = 0x00 (already zero)
    Address::new(addr)
};

/// Shielded transfer precompile: 0x0000000000000000000000000000000000000200.
/// Handles spend/output/join-split operations on the shielded note pool.
/// Phase 4 of the privacy redesign.
pub const SHIELDED_TRANSFER_PRECOMPILE: Address = {
    let mut addr = [0u8; 20];
    addr[18] = 0x02;
    Address::new(addr)
};

/// Shield / unshield precompile: 0x0000000000000000000000000000000000000201.
/// Bridges between transparent EVM balances and the shielded note pool.
/// Phase 4 of the privacy redesign.
pub const SHIELD_BRIDGE_PRECOMPILE: Address = {
    let mut addr = [0u8; 20];
    addr[18] = 0x02;
    addr[19] = 0x01;
    Address::new(addr)
};

/// State-transition proof verifier: 0x0000000000000000000000000000000000000300.
/// Verifies SP1-produced block proofs for light clients and bridge
/// consumption. Phase 5 of the privacy redesign.
pub const STATE_PROOF_VERIFIER_PRECOMPILE: Address = {
    let mut addr = [0u8; 20];
    addr[18] = 0x03;
    Address::new(addr)
};

// --- Phase 4: Shielded EVM ---
pub const GAS_SHIELDED_TRANSFER: u64 = 80_000;
pub const GAS_SHIELD: u64 = 60_000;
pub const GAS_UNSHIELD: u64 = 60_000;
pub const GAS_STATE_PROOF_VERIFY: u64 = 250_000;

pub fn shielded_transfer_selector() -> [u8; 4] {
    selector("shieldedTransfer(bytes)")
}

pub fn shield_selector() -> [u8; 4] {
    selector("shield(uint256,bytes)")
}

pub fn unshield_selector() -> [u8; 4] {
    selector("unshield(bytes)")
}

pub fn verify_state_proof_selector() -> [u8; 4] {
    selector("verifyStateProof(bytes)")
}

pub const GAS_PLACE_ORDER: u64 = 50_000;
pub const GAS_CANCEL_ORDER: u64 = 20_000;
pub const GAS_DEPOSIT_COLLATERAL: u64 = 25_000;
pub const GAS_WITHDRAW_COLLATERAL: u64 = 25_000;
pub const GAS_GET_POSITION: u64 = 5_000;
pub const GAS_GET_COLLATERAL: u64 = 3_000;
pub const GAS_IS_LIQUIDATABLE: u64 = 10_000;
pub const GAS_GET_BEST_BID_ASK: u64 = 5_000;

fn selector(sig: &str) -> [u8; 4] {
    let hash = keccak256(sig.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

pub fn place_order_selector() -> [u8; 4] {
    selector("placeOrder(uint64,bool,uint256,uint256,uint8)")
}

pub fn cancel_order_selector() -> [u8; 4] {
    selector("cancelOrder(uint256)")
}

pub fn deposit_collateral_selector() -> [u8; 4] {
    selector("depositCollateral(uint256)")
}

pub fn withdraw_collateral_selector() -> [u8; 4] {
    selector("withdrawCollateral(uint256)")
}

pub fn get_position_selector() -> [u8; 4] {
    selector("getPosition(uint64)")
}

pub fn get_collateral_selector() -> [u8; 4] {
    selector("getCollateral()")
}

pub fn is_liquidatable_selector() -> [u8; 4] {
    selector("isLiquidatable(address)")
}

pub fn get_best_bid_ask_selector() -> [u8; 4] {
    selector("getBestBidAsk(uint64)")
}

// ═══════════════════════════════════════════════════════════════════
// STUB SELECTORS — Future chain changes. Uncomment and implement
// when the corresponding precompile handlers are ready.
// ═══════════════════════════════════════════════════════════════════

// --- Phase 1: Zero Gas ---
// Gas exemption for CLOB precompile calls. When enabled, order
// placement/cancellation costs 0 gas — users only pay trading fees.
pub const ZERO_GAS_ENABLED: bool = false;

pub const GAS_PLACE_ORDER_ZERO: u64 = 0;
pub const GAS_CANCEL_ORDER_ZERO: u64 = 0;

// --- Phase 3: Native Advanced Orders ---
// On-chain TP/SL, trailing stops, stop-limit — removes dependency
// on client-side order monitoring.

pub fn place_stop_limit_selector() -> [u8; 4] {
    selector("placeStopLimit(uint64,bool,uint256,uint256,uint256,uint8)")
}

pub fn place_take_profit_selector() -> [u8; 4] {
    selector("placeTakeProfit(uint64,uint256)")
}

pub fn place_stop_loss_selector() -> [u8; 4] {
    selector("placeStopLoss(uint64,uint256)")
}

pub fn place_trailing_stop_selector() -> [u8; 4] {
    selector("placeTrailingStop(uint64,uint256,uint256)")
}

// --- Phase 3: Spot Trading ---

pub fn place_spot_order_selector() -> [u8; 4] {
    selector("placeSpotOrder(uint64,bool,uint256,uint256)")
}

pub fn cancel_spot_order_selector() -> [u8; 4] {
    selector("cancelSpotOrder(uint256)")
}

pub fn get_spot_balance_selector() -> [u8; 4] {
    selector("getSpotBalance(address,uint64)")
}

pub fn get_spot_orderbook_selector() -> [u8; 4] {
    selector("getSpotOrderBook(uint64)")
}

// --- Phase 3: On-Chain Sub-Accounts ---

pub fn create_sub_account_selector() -> [u8; 4] {
    selector("createSubAccount(string)")
}

pub fn transfer_between_sub_accounts_selector() -> [u8; 4] {
    selector("transferBetweenSubAccounts(uint256,uint256,uint256)")
}

pub fn get_sub_accounts_selector() -> [u8; 4] {
    selector("getSubAccounts(address)")
}

// --- Phase 2: Multi-Asset Collateral ---

pub fn deposit_collateral_multi_selector() -> [u8; 4] {
    selector("depositCollateral(address,uint256)")
}

pub fn get_collateral_multi_selector() -> [u8; 4] {
    selector("getCollateral(address)")
}

/// Read a 32-byte ABI word at the given index (0-indexed, after the 4-byte selector).
pub fn read_word(input: &[u8], word_index: usize) -> Option<[u8; 32]> {
    let start = 4 + word_index * 32;
    let end = start + 32;
    if input.len() < end {
        return None;
    }
    let mut word = [0u8; 32];
    word.copy_from_slice(&input[start..end]);
    Some(word)
}

pub fn decode_u256(word: [u8; 32]) -> U256 {
    U256::from_be_bytes(word)
}

pub fn decode_u64(word: [u8; 32]) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&word[24..32]);
    u64::from_be_bytes(bytes)
}

pub fn decode_bool(word: [u8; 32]) -> bool {
    word[31] != 0
}

pub fn decode_u8(word: [u8; 32]) -> u8 {
    word[31]
}

pub fn decode_address(word: [u8; 32]) -> Address {
    Address::from_slice(&word[12..32])
}

pub fn encode_u256(val: U256) -> [u8; 32] {
    val.to_be_bytes::<32>()
}

pub fn encode_bool(val: bool) -> [u8; 32] {
    let mut word = [0u8; 32];
    if val {
        word[31] = 1;
    }
    word
}

pub fn encode_i128(val: i128) -> [u8; 32] {
    let mut word = if val < 0 { [0xFF; 32] } else { [0u8; 32] };
    let bytes = val.to_be_bytes();
    word[16..32].copy_from_slice(&bytes);
    word
}
