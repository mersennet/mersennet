"use strict";
/**
 * PrimeProvider - JSON-RPC client for Mersennet.
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
 * Mersennet JSON-RPC provider.
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
    /** prime_getCodeHash for published contracts; null when unpublished. */
    async getCodeHash(address) {
        const result = (await this.request('prime_getCodeHash', [address]));
        return result ?? null;
    }
    /** prime_getCodeAttestation for published contracts; null when unpublished. */
    async getCodeAttestation(address) {
        const result = (await this.request('prime_getCodeAttestation', [address]));
        return result ?? null;
    }
    /** Derive the public contract label used by explorers and SDK consumers. */
    async getContractPublicationStatus(address) {
        const attestation = await this.getCodeAttestation(address);
        if (!attestation) {
            return 'unpublished';
        }
        return attestation.metadataUri ? 'source-published' : 'attested';
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
    /** prime_viewNotes for grant-gated encrypted note export. */
    async viewNotes(grantIdHex, options = {}) {
        const request = { grantIdHex };
        if (options.limit !== undefined) {
            request.limit = options.limit;
        }
        if (options.cursorHex) {
            request.cursorHex = options.cursorHex;
        }
        return (await this.request('prime_viewNotes', [request]));
    }
    /**
     * prime_viewBalances — grant-gated (`balances:read`) balance-reconstruction
     * read. Returns the encrypted note page plus the spent-nullifier set; pair
     * with `reconstructPortfolio` to derive spendable balances client-side.
     */
    async viewBalances(grantIdHex, options = {}) {
        const request = { grantIdHex };
        if (options.limit !== undefined) {
            request.limit = options.limit;
        }
        if (options.cursorHex) {
            request.cursorHex = options.cursorHex;
        }
        return (await this.request('prime_viewBalances', [request]));
    }
    /**
     * prime_viewPositions — grant-gated (`positions:read`) read. Returns the
     * public market context + grant binding; pair with `reconstructPositions`
     * over the wallet's local fill records.
     */
    async viewPositions(grantIdHex) {
        return (await this.request('prime_viewPositions', [{ grantIdHex }]));
    }
    /**
     * prime_viewOrders — grant-gated (`orders:read`) read. Returns the public
     * market context + grant binding; pair with `reconstructOpenOrders` over the
     * wallet's local order records.
     */
    async viewOrders(grantIdHex) {
        return (await this.request('prime_viewOrders', [{ grantIdHex }]));
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
