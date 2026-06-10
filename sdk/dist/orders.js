"use strict";
/**
 * PrimeOrders - High-level API for Mersennet order book operations.
 * Uses primeorders_* RPC methods and the CLOB precompile for view calls.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.PrimeOrders = void 0;
const precompile_1 = require("./precompile");
/** Normalize hex string for amounts */
function toHexAmount(s) {
    if (s.startsWith('0x'))
        return s;
    const n = BigInt(s);
    return '0x' + n.toString(16);
}
/** Parse RPC response to Order[] */
function parseOrders(raw) {
    if (!Array.isArray(raw))
        return [];
    return raw.map((o) => ({
        id: String(o.id ?? '0x0'),
        owner: String(o.owner ?? ''),
        market_id: String(o.market_id ?? '0x0'),
        side: (o.side === 'sell' ? 'sell' : 'buy'),
        price: String(o.price ?? '0x0'),
        size: String(o.size ?? '0x0'),
        tif: (o.tif === 'ioc' ? 'ioc' : o.tif === 'fok' ? 'fok' : 'gtc'),
    }));
}
/** Parse RPC response to OrderBook */
function parseOrderBook(raw) {
    const obj = raw;
    const bids = (Array.isArray(obj?.bids) ? obj.bids : []).map((l) => ({
        price: String(l.price ?? '0x0'),
        size: String(l.size ?? '0x0'),
    }));
    const asks = (Array.isArray(obj?.asks) ? obj.asks : []).map((l) => ({
        price: String(l.price ?? '0x0'),
        size: String(l.size ?? '0x0'),
    }));
    return { bids, asks };
}
/** Parse OrderOutcome from RPC */
function parseOrderOutcome(raw) {
    const obj = raw;
    const trades = (Array.isArray(obj?.trades) ? obj.trades : []).map((t) => ({
        taker: String(t.taker ?? ''),
        maker: String(t.maker ?? ''),
        market_id: String(t.market_id ?? '0x0'),
        side: (t.side === 'sell' ? 'sell' : 'buy'),
        price: String(t.price ?? '0x0'),
        size: String(t.size ?? '0x0'),
    }));
    return {
        order_id: obj.order_id != null ? String(obj.order_id) : null,
        filled: String(obj.filled ?? '0x0'),
        remaining: String(obj.remaining ?? '0x0'),
        trades,
    };
}
/**
 * Prime Orders API.
 */
class PrimeOrders {
    constructor(provider) {
        this.provider = provider;
    }
    /** Add a new market (admin). Returns market ID. */
    async addMarket(symbol, tickSize, lotSize) {
        const result = (await this.provider.request('primeorders_addMarket', [
            symbol,
            toHexAmount(tickSize),
            toHexAmount(lotSize),
        ]));
        const marketId = parseInt(result, 16);
        return { marketId };
    }
    /** Submit a single order. */
    async submitOrder(owner, marketId, side, price, size, tif = 'gtc') {
        const result = (await this.provider.request('primeorders_submitOrder', [
            {
                owner,
                market_id: marketId,
                side,
                price: toHexAmount(price),
                size: toHexAmount(size),
                tif,
            },
        ]));
        return parseOrderOutcome(result);
    }
    /** Cancel an order by ID. */
    async cancelOrder(orderId) {
        const result = (await this.provider.request('primeorders_cancelOrder', [
            '0x' + orderId.toString(16),
        ]));
        return result;
    }
    /** Get order book for a market. */
    async getOrderBook(marketId) {
        const result = (await this.provider.request('primeorders_getOrderBook', [
            '0x' + marketId.toString(16),
        ]));
        if (result == null)
            return { bids: [], asks: [] };
        return parseOrderBook(result);
    }
    /** Get open orders for an owner. */
    async getOpenOrders(owner) {
        const result = (await this.provider.request('primeorders_getOpenOrders', [
            owner,
        ]));
        return parseOrders(result);
    }
    /** Deposit collateral (RPC). */
    async depositCollateral(owner, amount) {
        const result = (await this.provider.request('primeorders_depositCollateral', [
            owner,
            toHexAmount(amount),
        ]));
        return result;
    }
    /**
     * Withdraw collateral. Sends a transaction to the precompile.
     * Requires the RPC to have the owner account unlocked, or use a wallet to sign.
     */
    async withdrawCollateral(owner, amount) {
        const data = (0, precompile_1.encodeWithdrawCollateral)(BigInt(amount));
        await this.provider.sendTransaction({
            from: owner,
            to: precompile_1.PrimePrecompile.ADDRESS,
            data,
        });
        return true;
    }
    /**
     * Get position for owner in a market. Uses precompile via eth_call.
     */
    async getPosition(owner, marketId) {
        const data = precompile_1.PrimePrecompile.encodeGetPosition(marketId);
        const output = (await this.provider.call({
            from: owner,
            to: precompile_1.PrimePrecompile.ADDRESS,
            data,
        }));
        if (!output || output === '0x' || output.length < 130) {
            return { size: '0', entry_price: '0' };
        }
        const hex = output.startsWith('0x') ? output.slice(2) : output;
        const sizeHex = hex.slice(0, 64);
        const entryPriceHex = hex.slice(64, 128);
        const size = BigInt('0x' + sizeHex);
        const entryPrice = BigInt('0x' + entryPriceHex);
        return {
            size: size.toString(),
            entry_price: entryPrice.toString(),
        };
    }
    /**
     * Get collateral balance for owner. Uses precompile via eth_call.
     */
    async getCollateral(owner) {
        const data = precompile_1.PrimePrecompile.encodeGetCollateral();
        const output = (await this.provider.call({
            from: owner,
            to: precompile_1.PrimePrecompile.ADDRESS,
            data,
        }));
        if (!output || output === '0x' || output.length < 66) {
            return '0';
        }
        const hex = output.startsWith('0x') ? output.slice(2) : output;
        return BigInt('0x' + hex.slice(0, 64)).toString();
    }
    /** Check if account is liquidatable (RPC). */
    async isLiquidatable(owner) {
        const result = (await this.provider.request('primeorders_isLiquidatable', [
            owner,
        ]));
        return result;
    }
    /**
     * Submit multiple orders in sequence.
     */
    async submitBatchOrder(params) {
        for (const o of params.orders) {
            await this.submitOrder(params.owner, o.marketId, o.side, o.price, o.size, o.tif ?? 'gtc');
        }
    }
}
exports.PrimeOrders = PrimeOrders;
