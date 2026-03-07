"use strict";
/**
 * PrimeProvider - JSON-RPC client for Prime Chain.
 * Uses fetch for HTTP (no external deps). Supports eth_* and prime_* methods.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.PrimeProvider = void 0;
/** Parse hex string to number */
function hexToNumber(hex) {
    const s = hex.startsWith('0x') ? hex.slice(2) : hex;
    return parseInt(s, 16);
}
/** Parse hex to string (for amounts) */
function hexToString(hex) {
    const s = hex.startsWith('0x') ? hex.slice(2) : hex;
    if (!s || s === '0')
        return '0x0';
    return '0x' + s.toLowerCase();
}
/**
 * Prime Chain JSON-RPC provider.
 * Connects to the RPC endpoint via HTTP.
 */
class PrimeProvider {
    /**
     * @param url - HTTP RPC URL (e.g. http://localhost:8545)
     * @param wsUrl - Optional WebSocket URL for subscriptions
     */
    constructor(url, wsUrl) {
        this.id = 0;
        this.url = url;
        this.wsUrl = wsUrl;
    }
    /**
     * Core JSON-RPC 2.0 request.
     */
    async request(method, params) {
        const body = JSON.stringify({
            jsonrpc: '2.0',
            id: ++this.id,
            method,
            params: params ?? [],
        });
        const res = await fetch(this.url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body,
        });
        const json = (await res.json());
        if (json.error) {
            const err = json.error;
            throw new Error(`RPC error ${err.code}: ${err.message}`);
        }
        return json.result;
    }
    /** eth_blockNumber / prime_blockNumber */
    async getBlockNumber() {
        const result = (await this.request('eth_blockNumber'));
        return hexToNumber(result);
    }
    /** eth_getBalance / prime_getBalance */
    async getBalance(address) {
        const result = (await this.request('eth_getBalance', [address]));
        return hexToString(result);
    }
    /** eth_getTransactionCount / prime_getTransactionCount */
    async getTransactionCount(address) {
        const result = (await this.request('eth_getTransactionCount', [
            address,
        ]));
        return hexToNumber(result);
    }
    /** eth_getCode / prime_getCode */
    async getCode(address) {
        const result = (await this.request('eth_getCode', [address]));
        return result || '0x';
    }
    /** eth_getStorageAt / prime_getStorageAt */
    async getStorageAt(address, slot) {
        const result = (await this.request('eth_getStorageAt', [
            address,
            slot,
        ]));
        return hexToString(result);
    }
    /** eth_getBlockByNumber / prime_getBlockByNumber */
    async getBlockByNumber(blockNumber, includeTxs = false) {
        const tag = blockNumber === 'latest'
            ? 'latest'
            : '0x' + blockNumber.toString(16);
        const result = (await this.request('eth_getBlockByNumber', [
            tag,
            includeTxs,
        ]));
        return result;
    }
    /** eth_getBlockByHash */
    async getBlockByHash(hash, includeTxs = false) {
        const result = (await this.request('eth_getBlockByHash', [
            hash,
            includeTxs,
        ]));
        return result;
    }
    /** eth_getTransactionReceipt / prime_getTransactionReceipt */
    async getTransactionReceipt(hash) {
        const result = (await this.request('eth_getTransactionReceipt', [
            hash,
        ]));
        return result;
    }
    /** eth_call / prime_call */
    async call(tx) {
        const result = (await this.request('eth_call', [tx]));
        return result || '0x';
    }
    /** eth_estimateGas */
    async estimateGas(tx) {
        const result = (await this.request('eth_estimateGas', [tx]));
        return hexToNumber(result);
    }
    /** eth_sendTransaction / prime_sendTransaction */
    async sendTransaction(tx) {
        const result = (await this.request('eth_sendTransaction', [tx]));
        return result;
    }
    /** eth_chainId / prime_chainId */
    async getChainId() {
        const result = (await this.request('eth_chainId'));
        return hexToNumber(result);
    }
    /** eth_gasPrice / prime_gasPrice */
    async getGasPrice() {
        const result = (await this.request('eth_gasPrice'));
        return hexToString(result);
    }
    /** Get the WebSocket URL if configured */
    getWsUrl() {
        return this.wsUrl;
    }
    /** Get the HTTP RPC URL */
    getUrl() {
        return this.url;
    }
}
exports.PrimeProvider = PrimeProvider;
