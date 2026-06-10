/**
 * PrimeOrders - High-level API for Mersennet order book operations.
 * Uses primeorders_* RPC methods and the CLOB precompile for view calls.
 */

import { PrimePrecompile, encodeWithdrawCollateral } from './precompile';
import type { PrimeProvider } from './provider';
import type {
  BatchOrderParams,
  Order,
  OrderBook,
  OrderOutcome,
  Position,
} from './types';

/** Normalize hex string for amounts */
function toHexAmount(s: string): string {
  if (s.startsWith('0x')) return s;
  const n = BigInt(s);
  return '0x' + n.toString(16);
}

/** Parse RPC response to Order[] */
function parseOrders(raw: unknown): Order[] {
  if (!Array.isArray(raw)) return [];
  return raw.map((o: Record<string, unknown>) => ({
    id: String(o.id ?? '0x0'),
    owner: String(o.owner ?? ''),
    market_id: String(o.market_id ?? '0x0'),
    side: (o.side === 'sell' ? 'sell' : 'buy') as 'buy' | 'sell',
    price: String(o.price ?? '0x0'),
    size: String(o.size ?? '0x0'),
    tif: (o.tif === 'ioc' ? 'ioc' : o.tif === 'fok' ? 'fok' : 'gtc') as
      | 'gtc'
      | 'ioc'
      | 'fok',
  }));
}

/** Parse RPC response to OrderBook */
function parseOrderBook(raw: unknown): OrderBook {
  const obj = raw as Record<string, unknown>;
  const bids = (Array.isArray(obj?.bids) ? obj.bids : []).map(
    (l: Record<string, unknown>) => ({
      price: String(l.price ?? '0x0'),
      size: String(l.size ?? '0x0'),
    })
  );
  const asks = (Array.isArray(obj?.asks) ? obj.asks : []).map(
    (l: Record<string, unknown>) => ({
      price: String(l.price ?? '0x0'),
      size: String(l.size ?? '0x0'),
    })
  );
  return { bids, asks };
}

/** Parse OrderOutcome from RPC */
function parseOrderOutcome(raw: unknown): OrderOutcome {
  const obj = raw as Record<string, unknown>;
  const trades = (Array.isArray(obj?.trades) ? obj.trades : []).map(
    (t: Record<string, unknown>) => ({
      taker: String(t.taker ?? ''),
      maker: String(t.maker ?? ''),
      market_id: String(t.market_id ?? '0x0'),
      side: (t.side === 'sell' ? 'sell' : 'buy') as 'buy' | 'sell',
      price: String(t.price ?? '0x0'),
      size: String(t.size ?? '0x0'),
    })
  );
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
export class PrimeOrders {
  private provider: PrimeProvider;

  constructor(provider: PrimeProvider) {
    this.provider = provider;
  }

  /** Add a new market (admin). Returns market ID. */
  async addMarket(
    symbol: string,
    tickSize: string,
    lotSize: string
  ): Promise<{ marketId: number }> {
    const result = (await this.provider.request('primeorders_addMarket', [
      symbol,
      toHexAmount(tickSize),
      toHexAmount(lotSize),
    ])) as string;
    const marketId = parseInt(result, 16);
    return { marketId };
  }

  /** Submit a single order. */
  async submitOrder(
    owner: string,
    marketId: number,
    side: 'buy' | 'sell',
    price: string,
    size: string,
    tif: 'gtc' | 'ioc' | 'fok' = 'gtc'
  ): Promise<OrderOutcome> {
    const result = (await this.provider.request('primeorders_submitOrder', [
      {
        owner,
        market_id: marketId,
        side,
        price: toHexAmount(price),
        size: toHexAmount(size),
        tif,
      },
    ])) as unknown;
    return parseOrderOutcome(result);
  }

  /** Cancel an order by ID. */
  async cancelOrder(orderId: number): Promise<boolean> {
    const result = (await this.provider.request('primeorders_cancelOrder', [
      '0x' + orderId.toString(16),
    ])) as boolean;
    return result;
  }

  /** Get order book for a market. */
  async getOrderBook(marketId: number): Promise<OrderBook> {
    const result = (await this.provider.request('primeorders_getOrderBook', [
      '0x' + marketId.toString(16),
    ])) as unknown;
    if (result == null) return { bids: [], asks: [] };
    return parseOrderBook(result);
  }

  /** Get open orders for an owner. */
  async getOpenOrders(owner: string): Promise<Order[]> {
    const result = (await this.provider.request('primeorders_getOpenOrders', [
      owner,
    ])) as unknown;
    return parseOrders(result);
  }

  /** Deposit collateral (RPC). */
  async depositCollateral(owner: string, amount: string): Promise<boolean> {
    const result = (await this.provider.request('primeorders_depositCollateral', [
      owner,
      toHexAmount(amount),
    ])) as boolean;
    return result;
  }

  /**
   * Withdraw collateral. Sends a transaction to the precompile.
   * Requires the RPC to have the owner account unlocked, or use a wallet to sign.
   */
  async withdrawCollateral(owner: string, amount: string): Promise<boolean> {
    const data = encodeWithdrawCollateral(BigInt(amount));
    await this.provider.sendTransaction({
      from: owner,
      to: PrimePrecompile.ADDRESS,
      data,
    });
    return true;
  }

  /**
   * Get position for owner in a market. Uses precompile via eth_call.
   */
  async getPosition(owner: string, marketId: number): Promise<Position> {
    const data = PrimePrecompile.encodeGetPosition(marketId);
    const output = (await this.provider.call({
      from: owner,
      to: PrimePrecompile.ADDRESS,
      data,
    })) as string;

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
  async getCollateral(owner: string): Promise<string> {
    const data = PrimePrecompile.encodeGetCollateral();
    const output = (await this.provider.call({
      from: owner,
      to: PrimePrecompile.ADDRESS,
      data,
    })) as string;

    if (!output || output === '0x' || output.length < 66) {
      return '0';
    }

    const hex = output.startsWith('0x') ? output.slice(2) : output;
    return BigInt('0x' + hex.slice(0, 64)).toString();
  }

  /** Check if account is liquidatable (RPC). */
  async isLiquidatable(owner: string): Promise<boolean> {
    const result = (await this.provider.request('primeorders_isLiquidatable', [
      owner,
    ])) as boolean;
    return result;
  }

  /**
   * Submit multiple orders in sequence.
   */
  async submitBatchOrder(params: BatchOrderParams): Promise<void> {
    for (const o of params.orders) {
      await this.submitOrder(
        params.owner,
        o.marketId,
        o.side,
        o.price,
        o.size,
        o.tif ?? 'gtc'
      );
    }
  }
}
