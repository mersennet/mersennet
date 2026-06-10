/**
 * PrimeSubscription - WebSocket subscriptions for Mersennet.
 * Uses eth_subscribe and prime_subscribe. In Node.js, WebSocket is available
 * in Node 18+; for older Node, use a polyfill (e.g. ws package).
 */
import type { PrimeProvider } from './provider';
import type { Block, BookUpdate, LogEntry, Trade } from './types';
/**
 * WebSocket subscription manager.
 * Connect to the WS endpoint and subscribe to events.
 */
export declare class PrimeSubscription {
    private provider;
    private ws?;
    private callbacks;
    private nextRequestId;
    private pending;
    private connected;
    constructor(provider: PrimeProvider);
    /** Connect to the WebSocket endpoint. */
    connect(): Promise<void>;
    /** Disconnect and clear subscriptions. */
    disconnect(): void;
    private handleMessage;
    private subscribe;
    /** Subscribe to new blocks. Returns subscription ID. */
    onNewBlock(callback: (block: Block) => void): Promise<string>;
    /** Subscribe to new transactions. Returns subscription ID. */
    onNewTransaction(callback: (txHash: string) => void): Promise<string>;
    /** Subscribe to trades, optionally filtered by market. Returns subscription ID. */
    onTrade(callback: (trade: Trade) => void, marketId?: number): Promise<string>;
    /** Subscribe to order book updates for a market. Returns subscription ID. */
    onBookUpdate(callback: (update: BookUpdate) => void, marketId: number): Promise<string>;
    /** Subscribe to logs, optionally filtered by address and topics. Returns subscription ID. */
    onLog(callback: (log: LogEntry) => void, address?: string, topics?: string[]): Promise<string>;
    /** Unsubscribe by ID. */
    unsubscribe(subscriptionId: string): void;
}
//# sourceMappingURL=subscription.d.ts.map