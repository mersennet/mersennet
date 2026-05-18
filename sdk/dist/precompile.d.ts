/**
 * Helper to encode calls to the CLOB precompile at 0x0100.
 * Uses manual ABI encoding (no ethers.js dependency).
 * Each param is 32 bytes; uint256 is left-padded; address is left-padded; bool is 0/1.
 */
/** Precompile address for Prime Orders CLOB */
export declare const PRECOMPILE_ADDRESS = "0x0000000000000000000000000000000000000100";
/**
 * Function selectors (first 4 bytes of keccak256 of signature).
 * Precomputed for known CLOB precompile functions.
 */
export declare const SELECTORS: Record<string, string>;
/**
 * Encode placeOrder call.
 * placeOrder(uint64 marketId, bool isBuy, uint256 price, uint256 size, uint8 tif)
 */
export declare function encodePlaceOrder(marketId: number, isBuy: boolean, price: bigint, size: bigint, tif: number): string;
/**
 * Encode cancelOrder call.
 * cancelOrder(uint256 orderId)
 */
export declare function encodeCancelOrder(orderId: bigint): string;
/**
 * Encode depositCollateral call.
 * depositCollateral(uint256 amount)
 */
export declare function encodeDepositCollateral(amount: bigint): string;
/**
 * Encode withdrawCollateral call.
 * withdrawCollateral(uint256 amount)
 */
export declare function encodeWithdrawCollateral(amount: bigint): string;
/**
 * Encode getPosition call.
 * getPosition(uint64 marketId)
 */
export declare function encodeGetPosition(marketId: number): string;
/**
 * Encode getCollateral call.
 * getCollateral()
 */
export declare function encodeGetCollateral(): string;
/**
 * Encode isLiquidatable call.
 * isLiquidatable(address account)
 */
export declare function encodeIsLiquidatable(account: string): string;
/**
 * Encode getBestBidAsk call.
 * getBestBidAsk(uint64 marketId)
 */
export declare function encodeGetBestBidAsk(marketId: number): string;
/**
 * PrimePrecompile class - static helpers for CLOB precompile encoding.
 */
export declare class PrimePrecompile {
    static readonly ADDRESS = "0x0000000000000000000000000000000000000100";
    static readonly SELECTORS: Record<string, string>;
    static encodePlaceOrder(marketId: number, isBuy: boolean, price: bigint, size: bigint, tif: number): string;
    static encodeCancelOrder(orderId: bigint): string;
    static encodeDepositCollateral(amount: bigint): string;
    static encodeWithdrawCollateral(amount: bigint): string;
    static encodeGetPosition(marketId: number): string;
    static encodeGetCollateral(): string;
    static encodeIsLiquidatable(account: string): string;
    static encodeGetBestBidAsk(marketId: number): string;
}
//# sourceMappingURL=precompile.d.ts.map