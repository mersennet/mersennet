use revm::primitives::{Address, U256, keccak256};

/// Precompile address: 0x0000000000000000000000000000000000000100
pub const MERSENNET_ORDERS_PRECOMPILE: Address = {
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

/// Code publication registry precompile:
/// 0x0000000000000000000000000000000000000202.
/// Lets a recorded deployer opt a contract into public code-hash
/// attestation without exposing raw bytecode.
pub const CODE_PUBLICATION_PRECOMPILE: Address = {
    let mut addr = [0u8; 20];
    addr[18] = 0x02;
    addr[19] = 0x02;
    Address::new(addr)
};

/// Delegated staking precompile: 0x0000000000000000000000000000000000000400.
/// Delegate MRSN to validators, earn a pro-rata share of block rewards.
/// Principal + delegator rewards are escrowed at this address.
pub const STAKING_PRECOMPILE: Address = {
    let mut addr = [0u8; 20];
    addr[18] = 0x04;
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
pub const GAS_CODE_PUBLICATION_UPDATE: u64 = 45_000;
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

pub fn publish_code_hash_selector() -> [u8; 4] {
    selector("publishCodeHash(address,string)")
}

pub fn revoke_code_hash_selector() -> [u8; 4] {
    selector("revokeCodeHash(address)")
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
pub const GAS_CREATE_MARKET: u64 = 500_000;

/// Listing fee for permissionless market creation, in wei (100 MRSN).
/// Deducted from the caller's native balance and credited to the CLOB
/// insurance fund — spam pricing, not revenue.
pub const CREATE_MARKET_FEE_WEI: u128 = 100_000_000_000_000_000_000;

fn selector(sig: &str) -> [u8; 4] {
    let hash = keccak256(sig.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

// --- Delegated staking (0x…0400) ---

pub const GAS_DELEGATE: u64 = 40_000;
pub const GAS_UNDELEGATE: u64 = 40_000;
pub const GAS_CLAIM_REWARDS: u64 = 30_000;
pub const GAS_WITHDRAW_UNBONDED: u64 = 30_000;
pub const GAS_STAKING_VIEW: u64 = 5_000;

pub fn delegate_selector() -> [u8; 4] {
    selector("delegate(address,uint256)")
}

pub fn undelegate_selector() -> [u8; 4] {
    selector("undelegate(address,uint256)")
}

pub fn claim_rewards_selector() -> [u8; 4] {
    selector("claimRewards(address)")
}

pub fn withdraw_unbonded_selector() -> [u8; 4] {
    selector("withdrawUnbonded()")
}

pub fn get_delegation_selector() -> [u8; 4] {
    selector("getDelegation(address,address)")
}

pub fn get_validator_staking_selector() -> [u8; 4] {
    selector("getValidatorStaking(address)")
}

pub fn get_unbonding_selector() -> [u8; 4] {
    selector("getUnbonding(address)")
}

// Open validator set (same precompile, 0x…0400)
pub fn register_validator_selector() -> [u8; 4] {
    selector("registerValidator(address,uint256,uint256,bytes)")
}
pub fn add_self_stake_selector() -> [u8; 4] {
    selector("addSelfStake(address,uint256)")
}
pub fn unregister_validator_selector() -> [u8; 4] {
    selector("unregisterValidator(address)")
}
pub fn rotate_validator_key_selector() -> [u8; 4] {
    selector("rotateValidatorKey(address,address,bytes)")
}
pub const GAS_REGISTER_VALIDATOR: u64 = 80_000;
pub const GAS_VALIDATOR_ADMIN: u64 = 40_000;

/// Read a dynamic `bytes` argument whose offset word sits at `word_index`.
pub fn read_bytes_arg(input: &[u8], word_index: usize) -> Option<Vec<u8>> {
    let off = decode_u256(read_word(input, word_index)?);
    let off: usize = off.try_into().ok()?;
    let data = &input[4..];
    let len_word: [u8; 32] = data.get(off..off + 32)?.try_into().ok()?;
    let len: usize = decode_u256(len_word).try_into().ok()?;
    if len > 4096 {
        return None;
    }
    data.get(off + 32..off + 32 + len).map(|b| b.to_vec())
}

pub fn place_order_selector() -> [u8; 4] {
    selector("placeOrder(uint64,bool,uint256,uint256,uint8)")
}

/// Extended placeOrder with maker flags:
/// `flags` is a bitfield (bit 0 = post-only), `expireAtBlock` is the
/// good-till-date height (0 = never expires).
pub fn place_order_ext_selector() -> [u8; 4] {
    selector("placeOrderExt(uint64,bool,uint256,uint256,uint8,uint8,uint64)")
}

/// Permissionless market listing. `symbol` is a right-padded bytes32
/// (ASCII, trailing zeros trimmed). Charges a listing fee in native MRSN
/// from the caller's balance into the CLOB insurance fund.
pub fn create_market_selector() -> [u8; 4] {
    selector("createMarket(bytes32,uint256,uint256)")
}

pub fn cancel_order_selector() -> [u8; 4] {
    selector("cancelOrder(uint256)")
}

/// Agent delegation (from `mersennet_orders.agent_delegation_height`).
/// `setAgent(agent, expiresAtBlock)`: transactions signed by `agent` place and
/// cancel orders as the caller (never deposits/withdrawals). `0` = no expiry.
pub fn set_agent_selector() -> [u8; 4] {
    selector("setAgent(address,uint64)")
}
pub fn revoke_agent_selector() -> [u8; 4] {
    selector("revokeAgent(address)")
}
/// `agentOf(agent) -> (address owner, uint64 expiresAtBlock)`; zero owner = none.
pub fn agent_of_selector() -> [u8; 4] {
    selector("agentOf(address)")
}
pub const GAS_SET_AGENT: u64 = 30_000;
pub const GAS_AGENT_OF: u64 = 3_000;

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

// --- Multi-Asset Collateral ---

pub const GAS_DEPOSIT_COLLATERAL_MULTI: u64 = 60_000;
pub const GAS_WITHDRAW_COLLATERAL_MULTI: u64 = 60_000;
pub const GAS_GET_COLLATERAL_MULTI: u64 = 5_000;

pub fn deposit_collateral_multi_selector() -> [u8; 4] {
    selector("depositTokenCollateral(address,uint256)")
}

pub fn withdraw_collateral_multi_selector() -> [u8; 4] {
    selector("withdrawTokenCollateral(address,uint256)")
}

pub fn get_collateral_multi_selector() -> [u8; 4] {
    selector("getTokenCollateral(address,address)")
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

// ── Calldata builders for server-routed CLOB txs ──
// These produce the `selector || abi-words` calldata the precompile
// handlers decode via read_word(), so the unsigned order RPC can submit
// the operation through the mempool instead of mutating state directly.

/// placeOrder(uint64 marketId, bool isBuy, uint256 price, uint256 size, uint8 tif)
pub fn encode_place_order(
    market_id: u64,
    is_buy: bool,
    price: U256,
    size: U256,
    tif: u8,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 32 * 5);
    out.extend_from_slice(&place_order_selector());
    out.extend_from_slice(&encode_u256(U256::from(market_id)));
    out.extend_from_slice(&encode_bool(is_buy));
    out.extend_from_slice(&encode_u256(price));
    out.extend_from_slice(&encode_u256(size));
    out.extend_from_slice(&encode_u256(U256::from(tif as u64)));
    out
}

/// cancelOrder(uint256 orderId)
pub fn encode_cancel_order(order_id: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 32);
    out.extend_from_slice(&cancel_order_selector());
    out.extend_from_slice(&encode_u256(U256::from(order_id)));
    out
}

/// depositCollateral(uint256 amount)
pub fn encode_deposit_collateral(amount: U256) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 32);
    out.extend_from_slice(&deposit_collateral_selector());
    out.extend_from_slice(&encode_u256(amount));
    out
}
