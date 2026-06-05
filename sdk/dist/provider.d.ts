/**
 * PrimeProvider - JSON-RPC client for Prime Chain.
 * Uses fetch for HTTP (no external deps). Supports eth_* and prime_* methods.
 */
import type { Block, CallParams, CodeAttestation, ContractPublicationStatus, Receipt, TransactionParams, ViewBalancesResult, ViewNotesResult, ViewTradingResult } from './types';
/**
 * Prime Chain JSON-RPC provider.
 * Connects to the RPC endpoint via HTTP.
 */
export declare class PrimeProvider {
    private url;
    private wsUrl?;
    private id;
    /**
     * @param url - HTTP RPC URL (e.g. http://localhost:8545)
     * @param wsUrl - Optional WebSocket URL for subscriptions
     */
    constructor(url: string, wsUrl?: string);
    /**
     * Core JSON-RPC 2.0 request.
     */
    request(method: string, params?: unknown[]): Promise<unknown>;
    /** eth_blockNumber / prime_blockNumber */
    getBlockNumber(): Promise<number>;
    /** eth_getBalance / prime_getBalance */
    getBalance(address: string): Promise<string>;
    /** eth_getTransactionCount / prime_getTransactionCount */
    getTransactionCount(address: string): Promise<number>;
    /** eth_getCode / prime_getCode */
    getCode(address: string): Promise<string>;
    /** prime_getCodeHash for published contracts; null when unpublished. */
    getCodeHash(address: string): Promise<string | null>;
    /** prime_getCodeAttestation for published contracts; null when unpublished. */
    getCodeAttestation(address: string): Promise<CodeAttestation | null>;
    /** Derive the public contract label used by explorers and SDK consumers. */
    getContractPublicationStatus(address: string): Promise<ContractPublicationStatus>;
    /** eth_getStorageAt / prime_getStorageAt */
    getStorageAt(address: string, slot: string): Promise<string>;
    /** eth_getBlockByNumber / prime_getBlockByNumber */
    getBlockByNumber(blockNumber: number | 'latest', includeTxs?: boolean): Promise<Block | null>;
    /** eth_getBlockByHash */
    getBlockByHash(hash: string, includeTxs?: boolean): Promise<Block | null>;
    /** eth_getTransactionReceipt / prime_getTransactionReceipt */
    getTransactionReceipt(hash: string): Promise<Receipt | null>;
    /** eth_call / prime_call */
    call(tx: CallParams): Promise<string>;
    /** eth_estimateGas */
    estimateGas(tx: CallParams): Promise<number>;
    /** eth_sendTransaction / prime_sendTransaction */
    sendTransaction(tx: TransactionParams): Promise<string>;
    /** eth_chainId / prime_chainId */
    getChainId(): Promise<number>;
    /** prime_viewNotes for grant-gated encrypted note export. */
    viewNotes(grantIdHex: string, options?: {
        limit?: number;
        cursorHex?: string;
    }): Promise<ViewNotesResult>;
    /**
     * prime_viewBalances — grant-gated (`balances:read`) balance-reconstruction
     * read. Returns the encrypted note page plus the spent-nullifier set; pair
     * with `reconstructPortfolio` to derive spendable balances client-side.
     */
    viewBalances(grantIdHex: string, options?: {
        limit?: number;
        cursorHex?: string;
    }): Promise<ViewBalancesResult>;
    /**
     * prime_viewPositions — grant-gated (`positions:read`) read. Returns the
     * public market context + grant binding; pair with `reconstructPositions`
     * over the wallet's local fill records.
     */
    viewPositions(grantIdHex: string): Promise<ViewTradingResult>;
    /**
     * prime_viewOrders — grant-gated (`orders:read`) read. Returns the public
     * market context + grant binding; pair with `reconstructOpenOrders` over the
     * wallet's local order records.
     */
    viewOrders(grantIdHex: string): Promise<ViewTradingResult>;
    /** eth_gasPrice / prime_gasPrice */
    getGasPrice(): Promise<string>;
    /** Get the WebSocket URL if configured */
    getWsUrl(): string | undefined;
    /** Get the HTTP RPC URL */
    getUrl(): string;
}
//# sourceMappingURL=provider.d.ts.map