/**
 * TypeScript interfaces for Prime Chain SDK.
 * All amounts are strings to avoid BigInt precision issues in JSON.
 */
/** Block data returned by eth_getBlockByNumber / eth_getBlockByHash */
export interface Block {
    number: string;
    hash: string;
    gas_limit: string;
    gas_used: string;
    base_fee: string;
    state_root: string;
    transactions: string[] | Transaction[];
    domain_events?: unknown[];
}
/** Transaction data */
export interface Transaction {
    hash: string;
    from: string;
    to: string | null;
    value: string;
    nonce: string;
    gas: string;
    gas_price: string;
    input: string;
}
/** Transaction receipt */
export interface Receipt {
    transaction_hash: string;
    block_hash: string;
    block_number: string;
    transaction_index: string;
    gas_used: string;
    status: string;
    contract_address: string | null;
    output: string;
    logs: LogEntry[];
}
/** Log entry from a contract event */
export interface LogEntry {
    address: string;
    topics: string[];
    data: string;
    block_number: string;
    block_hash: string;
    transaction_hash: string;
    transaction_index: string;
    log_index: string;
}
/** Parameters for eth_call / eth_estimateGas */
export interface CallParams {
    from?: string;
    to?: string;
    data?: string;
    value?: string;
    gas?: string;
}
/** Parameters for eth_sendTransaction */
export interface TransactionParams {
    from: string;
    to?: string;
    value?: string;
    data?: string;
    gas?: string;
    gas_price?: string;
    nonce?: string;
    chain_id?: number;
}
/** Order in the order book or open orders list */
export interface Order {
    id: string;
    owner: string;
    market_id: string;
    side: 'buy' | 'sell';
    price: string;
    size: string;
    tif: 'gtc' | 'ioc' | 'fok';
}
/** Order book levels */
export interface OrderBookLevel {
    price: string;
    size: string;
}
/** Full order book snapshot */
export interface OrderBook {
    bids: OrderBookLevel[];
    asks: OrderBookLevel[];
}
/** Result of submitting an order */
export interface OrderOutcome {
    order_id: string | null;
    filled: string;
    remaining: string;
    trades: Trade[];
}
/** Trade execution record */
export interface Trade {
    taker: string;
    maker: string;
    market_id: string;
    side: 'buy' | 'sell';
    price: string;
    size: string;
}
/** Position for a market */
export interface Position {
    size: string;
    entry_price: string;
}
/** Parameters for batch order submission */
export interface BatchOrderParams {
    owner: string;
    orders: Array<{
        marketId: number;
        side: 'buy' | 'sell';
        price: string;
        size: string;
        tif?: 'gtc' | 'ioc' | 'fok';
    }>;
}
/** WebSocket subscription event envelope */
export interface SubscriptionEvent {
    jsonrpc: string;
    method: string;
    params?: {
        subscription: string;
        result: unknown;
    };
}
/** Order book update from WebSocket */
export interface BookUpdate {
    market_id: number;
    bids?: OrderBookLevel[];
    asks?: OrderBookLevel[];
}
//# sourceMappingURL=types.d.ts.map