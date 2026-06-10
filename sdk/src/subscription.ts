/**
 * PrimeSubscription - WebSocket subscriptions for Mersennet.
 * Uses eth_subscribe and prime_subscribe. In Node.js, WebSocket is available
 * in Node 18+; for older Node, use a polyfill (e.g. ws package).
 */

import type { PrimeProvider } from './provider';
import type { Block, BookUpdate, LogEntry, Trade } from './types';

type Callback = (data: unknown) => void;

interface SubscriptionEntry {
  id: string;
  callback: Callback;
}

/**
 * WebSocket subscription manager.
 * Connect to the WS endpoint and subscribe to events.
 */
export class PrimeSubscription {
  private provider: PrimeProvider;
  private ws?: WebSocket;
  private callbacks: Map<string, SubscriptionEntry> = new Map();
  private nextRequestId = 1;
  private pending: Map<number, (subId: string) => void> = new Map();
  private connected = false;

  constructor(provider: PrimeProvider) {
    this.provider = provider;
  }

  /** Connect to the WebSocket endpoint. */
  async connect(): Promise<void> {
    let wsUrl = this.provider.getWsUrl?.();
    if (!wsUrl && this.provider.getUrl) {
      wsUrl = this.provider.getUrl().replace(/^http/, 'ws');
    }
    if (!wsUrl) throw new Error('No WebSocket URL configured');
    this.ws = new WebSocket(wsUrl);

    return new Promise((resolve, reject) => {
      if (!this.ws) return reject(new Error('WebSocket not created'));

      this.ws.onopen = () => {
        this.connected = true;
        resolve();
      };

      this.ws.onerror = (e) => reject(e);
      this.ws.onmessage = (ev) => this.handleMessage(ev.data);
    });
  }

  /** Disconnect and clear subscriptions. */
  disconnect(): void {
    this.connected = false;
    this.callbacks.clear();
    this.pending.clear();
    if (this.ws) {
      this.ws.close();
      this.ws = undefined;
    }
  }

  private handleMessage(data: string | Buffer): void {
    try {
      const msg = JSON.parse(
        typeof data === 'string' ? data : data.toString()
      ) as {
        id?: number;
        result?: string;
        method?: string;
        params?: { subscription?: string; result?: unknown };
      };

      if (msg.id != null && this.pending.has(msg.id)) {
        const resolve = this.pending.get(msg.id)!;
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
    } catch {
      // ignore parse errors
    }
  }

  private async subscribe(
    method: string,
    params: unknown[],
    callback: Callback
  ): Promise<string> {
    if (!this.ws || !this.connected) {
      await this.connect();
    }

    const id = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      const checkSub = (subId: string) => {
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

      this.ws!.send(req);

      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error('Subscribe timeout'));
        }
      }, 10000);
    });
  }

  /** Subscribe to new blocks. Returns subscription ID. */
  async onNewBlock(callback: (block: Block) => void): Promise<string> {
    return this.subscribe('eth_subscribe', ['newHeads'], (result) => {
      callback(result as Block);
    });
  }

  /** Subscribe to new transactions. Returns subscription ID. */
  async onNewTransaction(callback: (txHash: string) => void): Promise<string> {
    return this.subscribe('eth_subscribe', ['newPendingTransactions'], (result) => {
      const obj = result as { hash?: string };
      callback(obj?.hash ?? String(result));
    });
  }

  /** Subscribe to trades, optionally filtered by market. Returns subscription ID. */
  async onTrade(callback: (trade: Trade) => void, marketId?: number): Promise<string> {
    const params =
      marketId != null
        ? ['PrimeOrdersTrades', marketId]
        : ['PrimeOrdersTrades'];
    return this.subscribe('prime_subscribe', params, (result) => {
      callback(result as Trade);
    });
  }

  /** Subscribe to order book updates for a market. Returns subscription ID. */
  async onBookUpdate(callback: (update: BookUpdate) => void, marketId: number): Promise<string> {
    return this.subscribe('prime_subscribe', ['PrimeOrdersBook', marketId], (result) => {
      callback({ ...(result as BookUpdate), market_id: marketId });
    });
  }

  /** Subscribe to logs, optionally filtered by address and topics. Returns subscription ID. */
  async onLog(
    callback: (log: LogEntry) => void,
    address?: string,
    topics?: string[]
  ): Promise<string> {
    const filter: Record<string, unknown> = {};
    if (address) filter.address = address;
    if (topics && topics.length > 0) filter.topics = topics;
    const params = ['logs', Object.keys(filter).length > 0 ? filter : undefined];
    return this.subscribe('eth_subscribe', params, (result) => {
      callback(result as LogEntry);
    });
  }

  /** Unsubscribe by ID. */
  unsubscribe(subscriptionId: string): void {
    this.callbacks.delete(subscriptionId);
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(
        JSON.stringify({
          jsonrpc: '2.0',
          id: this.nextRequestId++,
          method: 'eth_unsubscribe',
          params: [subscriptionId],
        })
      );
    }
  }
}
