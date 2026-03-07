"use strict";
/**
 * Helper to encode calls to the CLOB precompile at 0x0100.
 * Uses manual ABI encoding (no ethers.js dependency).
 * Each param is 32 bytes; uint256 is left-padded; address is left-padded; bool is 0/1.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.PrimePrecompile = exports.SELECTORS = exports.PRECOMPILE_ADDRESS = void 0;
exports.encodePlaceOrder = encodePlaceOrder;
exports.encodeCancelOrder = encodeCancelOrder;
exports.encodeDepositCollateral = encodeDepositCollateral;
exports.encodeWithdrawCollateral = encodeWithdrawCollateral;
exports.encodeGetPosition = encodeGetPosition;
exports.encodeGetCollateral = encodeGetCollateral;
exports.encodeIsLiquidatable = encodeIsLiquidatable;
exports.encodeGetBestBidAsk = encodeGetBestBidAsk;
/** Precompile address for Prime Orders CLOB */
exports.PRECOMPILE_ADDRESS = '0x0000000000000000000000000000000000000100';
/**
 * Function selectors (first 4 bytes of keccak256 of signature).
 * Precomputed for known CLOB precompile functions.
 */
exports.SELECTORS = {
    placeOrder: '0x4c570d73',
    cancelOrder: '0x514fcac7',
    depositCollateral: '0xbad4a01f',
    withdrawCollateral: '0x6112fe2e',
    getPosition: '0x0f85fc5a',
    getCollateral: '0x5c1548fb',
    isLiquidatable: '0x042e02cf',
    getBestBidAsk: '0x8ee0a7fa',
};
/** Pad a hex string to 32 bytes (64 hex chars), left-padded with zeros */
function pad32(hex) {
    const stripped = hex.startsWith('0x') ? hex.slice(2) : hex;
    return stripped.padStart(64, '0').toLowerCase();
}
/** Encode uint256 as 32-byte hex */
function encodeU256(val) {
    const hex = val.toString(16);
    return pad32(hex);
}
/** Encode uint64 as 32-byte hex (left-padded) */
function encodeU64(val) {
    return pad32(val.toString(16));
}
/** Encode bool as 32-byte hex (0 or 1 in last byte) */
function encodeBool(val) {
    return pad32(val ? '1' : '0');
}
/** Encode address as 32-byte hex (left-padded, 20 bytes = 40 hex chars) */
function encodeAddress(addr) {
    const stripped = addr.startsWith('0x') ? addr.slice(2) : addr;
    return stripped.padStart(64, '0').toLowerCase();
}
/** Encode uint8 as 32-byte hex */
function encodeU8(val) {
    return pad32(val.toString(16));
}
/**
 * Encode placeOrder call.
 * placeOrder(uint64 marketId, bool isBuy, uint256 price, uint256 size, uint8 tif)
 */
function encodePlaceOrder(marketId, isBuy, price, size, tif) {
    const sel = exports.SELECTORS.placeOrder;
    const args = encodeU64(marketId) +
        encodeBool(isBuy) +
        encodeU256(price) +
        encodeU256(size) +
        encodeU8(tif);
    return sel + args;
}
/**
 * Encode cancelOrder call.
 * cancelOrder(uint256 orderId)
 */
function encodeCancelOrder(orderId) {
    const sel = exports.SELECTORS.cancelOrder;
    const args = encodeU256(orderId);
    return sel + args;
}
/**
 * Encode depositCollateral call.
 * depositCollateral(uint256 amount)
 */
function encodeDepositCollateral(amount) {
    const sel = exports.SELECTORS.depositCollateral;
    const args = encodeU256(amount);
    return sel + args;
}
/**
 * Encode withdrawCollateral call.
 * withdrawCollateral(uint256 amount)
 */
function encodeWithdrawCollateral(amount) {
    const sel = exports.SELECTORS.withdrawCollateral;
    const args = encodeU256(amount);
    return sel + args;
}
/**
 * Encode getPosition call.
 * getPosition(uint64 marketId)
 */
function encodeGetPosition(marketId) {
    const sel = exports.SELECTORS.getPosition;
    const args = encodeU64(marketId);
    return sel + args;
}
/**
 * Encode getCollateral call.
 * getCollateral()
 */
function encodeGetCollateral() {
    return exports.SELECTORS.getCollateral;
}
/**
 * Encode isLiquidatable call.
 * isLiquidatable(address account)
 */
function encodeIsLiquidatable(account) {
    const sel = exports.SELECTORS.isLiquidatable;
    const args = encodeAddress(account);
    return sel + args;
}
/**
 * Encode getBestBidAsk call.
 * getBestBidAsk(uint64 marketId)
 */
function encodeGetBestBidAsk(marketId) {
    const sel = exports.SELECTORS.getBestBidAsk;
    const args = encodeU64(marketId);
    return sel + args;
}
/**
 * PrimePrecompile class - static helpers for CLOB precompile encoding.
 */
class PrimePrecompile {
    static encodePlaceOrder(marketId, isBuy, price, size, tif) {
        return encodePlaceOrder(marketId, isBuy, price, size, tif);
    }
    static encodeCancelOrder(orderId) {
        return encodeCancelOrder(orderId);
    }
    static encodeDepositCollateral(amount) {
        return encodeDepositCollateral(amount);
    }
    static encodeWithdrawCollateral(amount) {
        return encodeWithdrawCollateral(amount);
    }
    static encodeGetPosition(marketId) {
        return encodeGetPosition(marketId);
    }
    static encodeGetCollateral() {
        return encodeGetCollateral();
    }
    static encodeIsLiquidatable(account) {
        return encodeIsLiquidatable(account);
    }
    static encodeGetBestBidAsk(marketId) {
        return encodeGetBestBidAsk(marketId);
    }
}
exports.PrimePrecompile = PrimePrecompile;
PrimePrecompile.ADDRESS = exports.PRECOMPILE_ADDRESS;
PrimePrecompile.SELECTORS = exports.SELECTORS;
