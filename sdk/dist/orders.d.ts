/**
 * PrimeOrders - High-level API for Mersennet order book operations.
 * Uses primeorders_* RPC methods and the CLOB precompile for view calls.
 */
import type { PrimeProvider } from './provider';
import type { BatchOrderParams, Order, OrderBook, OrderOutcome, Position } from './types';
/**
 * Prime Orders API.
 */
export declare class PrimeOrders {
    private provider;
    constructor(provider: PrimeProvider);
    /** Add a new market (admin). Returns market ID. */
    addMarket(symbol: string, tickSize: string, lotSize: string): Promise<{
        marketId: number;
    }>;
    /** Submit a single order. */
    submitOrder(owner: string, marketId: number, side: 'buy' | 'sell', price: string, size: string, tif?: 'gtc' | 'ioc' | 'fok'): Promise<OrderOutcome>;
    /** Cancel an order by ID. */
    cancelOrder(orderId: number): Promise<boolean>;
    /** Get order book for a market. */
    getOrderBook(marketId: number): Promise<OrderBook>;
    /** Get open orders for an owner. */
    getOpenOrders(owner: string): Promise<Order[]>;
    /** Deposit collateral (RPC). */
    depositCollateral(owner: string, amount: string): Promise<boolean>;
    /**
     * Withdraw collateral. Sends a transaction to the precompile.
     * Requires the RPC to have the owner account unlocked, or use a wallet to sign.
     */
    withdrawCollateral(owner: string, amount: string): Promise<boolean>;
    /**
     * Get position for owner in a market. Uses precompile via eth_call.
     */
    getPosition(owner: string, marketId: number): Promise<Position>;
    /**
     * Get collateral balance for owner. Uses precompile via eth_call.
     */
    getCollateral(owner: string): Promise<string>;
    /** Check if account is liquidatable (RPC). */
    isLiquidatable(owner: string): Promise<boolean>;
    /**
     * Submit multiple orders in sequence.
     */
    submitBatchOrder(params: BatchOrderParams): Promise<void>;
}
//# sourceMappingURL=orders.d.ts.map