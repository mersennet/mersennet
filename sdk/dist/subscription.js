"use strict";
/**
 * PrimeSubscription - WebSocket subscriptions for Prime Chain.
 * Uses eth_subscribe and prime_subscribe. In Node.js, WebSocket is available
 * in Node 18+; for older Node, use a polyfill (e.g. ws package).
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.PrimeSubscription = void 0;
/**
 * WebSocket subscription manager.
 * Connect to the WS endpoint and subscribe to events.
 */
class PrimeSubscription {
    constructor(provider) {
        this.callbacks = new Map();
        this.nextRequestId = 1;
        this.pending = new Map();
        this.connected = false;
        this.provider = provider;
    }
    /** Connect to the WebSocket endpoint. */
    async connect() {
        let wsUrl = this.provider.getWsUrl?.();
        if (!wsUrl && this.provider.getUrl) {
            wsUrl = this.provider.getUrl().replace(/^http/, 'ws');
        }
        if (!wsUrl)
            throw new Error('No WebSocket URL configured');
        this.ws = new WebSocket(wsUrl);
        return new Promise((resolve, reject) => {
            if (!this.ws)
                return reject(new Error('WebSocket not created'));
            this.ws.onopen = () => {
                this.connected = true;
                resolve();
            };
            this.ws.onerror = (e) => reject(e);
            this.ws.onmessage = (ev) => this.handleMessage(ev.data);
        });
    }
    /** Disconnect and clear subscriptions. */
    disconnect() {
        this.connected = false;
        this.callbacks.clear();
        this.pending.clear();
        if (this.ws) {
            this.ws.close();
            this.ws = undefined;
        }
    }
    handleMessage(data) {
        try {
            const msg = JSON.parse(typeof data === 'string' ? data : data.toString());
            if (msg.id != null && this.pending.has(msg.id)) {
                const resolve = this.pending.get(msg.id);
                this.pending.delete(msg.id);
                resolve(msg.result ?? '');
                return;
            }
            if (msg.method === 'eth_subscription' && msg.params?.subscription) {
                const subId = String(msg.params.subscription);
                const entry = this.callbacks.get(subId);
                if (entry) {
                    entry.callback(msg.params.result);
                }
            }
        }
        catch {
            // ignore parse errors
        }
    }
    async subscribe(method, params, callback) {
        if (!this.ws || !this.connected) {
            await this.connect();
        }
        const id = this.nextRequestId++;
        return new Promise((resolve, reject) => {
            const checkSub = (subId) => {
                this.callbacks.set(subId, { id: subId, callback });
                resolve(subId);
            };
            this.pending.set(id, checkSub);
            const req = JSON.stringify({
                jsonrpc: '2.0',
                id,
                method,
                params,
            });
            this.ws.send(req);
            setTimeout(() => {
                if (this.pending.has(id)) {
                    this.pending.delete(id);
                    reject(new Error('Subscribe timeout'));
                }
            }, 10000);
        });
    }
    /** Subscribe to new blocks. Returns subscription ID. */
    async onNewBlock(callback) {
        return this.subscribe('eth_subscribe', ['newHeads'], (result) => {
            callback(result);
        });
    }
    /** Subscribe to new transactions. Returns subscription ID. */
    async onNewTransaction(callback) {
        return this.subscribe('eth_subscribe', ['newPendingTransactions'], (result) => {
            const obj = result;
            callback(obj?.hash ?? String(result));
        });
    }
    /** Subscribe to trades, optionally filtered by market. Returns subscription ID. */
    async onTrade(callback, marketId) {
        const params = marketId != null
            ? ['PrimeOrdersTrades', marketId]
            : ['PrimeOrdersTrades'];
        return this.subscribe('prime_subscribe', params, (result) => {
            callback(result);
        });
    }
    /** Subscribe to order book updates for a market. Returns subscription ID. */
    async onBookUpdate(callback, marketId) {
        return this.subscribe('prime_subscribe', ['PrimeOrdersBook', marketId], (result) => {
            callback({ ...result, market_id: marketId });
        });
    }
    /** Subscribe to logs, optionally filtered by address and topics. Returns subscription ID. */
    async onLog(callback, address, topics) {
        const filter = {};
        if (address)
            filter.address = address;
        if (topics && topics.length > 0)
            filter.topics = topics;
        const params = ['logs', Object.keys(filter).length > 0 ? filter : undefined];
        return this.subscribe('eth_subscribe', params, (result) => {
            callback(result);
        });
    }
    /** Unsubscribe by ID. */
    unsubscribe(subscriptionId) {
        this.callbacks.delete(subscriptionId);
        if (this.ws?.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({
                jsonrpc: '2.0',
                id: this.nextRequestId++,
                method: 'eth_unsubscribe',
                params: [subscriptionId],
            }));
        }
    }
}
exports.PrimeSubscription = PrimeSubscription;
