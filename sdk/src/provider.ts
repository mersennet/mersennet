/**
 * PrimeProvider - JSON-RPC client for Prime Chain.
 * Uses fetch for HTTP (no external deps). Supports eth_* and prime_* methods.
 */

import type { Block, CallParams, Receipt, TransactionParams } from './types';

/** Parse hex string to number */
function hexToNumber(hex: string): number {
  const s = hex.startsWith('0x') ? hex.slice(2) : hex;
  return parseInt(s, 16);
}

/** Parse hex to string (for amounts) */
function hexToString(hex: string): string {
  const s = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (!s || s === '0') return '0x0';
  return '0x' + s.toLowerCase();
}

/**
 * Prime Chain JSON-RPC provider.
 * Connects to the RPC endpoint via HTTP.
 */
export class PrimeProvider {
  private url: string;
  private wsUrl?: string;
  private id: number = 0;

  /**
   * @param url - HTTP RPC URL (e.g. http://localhost:8545)
   * @param wsUrl - Optional WebSocket URL for subscriptions
   */
  constructor(url: string, wsUrl?: string) {
    this.url = url;
    this.wsUrl = wsUrl;
  }

  /**
   * Core JSON-RPC 2.0 request.
   */
  async request(method: string, params?: unknown[]): Promise<unknown> {
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

    const json = (await res.json()) as {
      jsonrpc?: string;
      id?: number;
      result?: unknown;
      error?: { code: number; message: string; data?: unknown };
    };

    if (json.error) {
      const err = json.error;
      throw new Error(`RPC error ${err.code}: ${err.message}`);
    }

    return json.result;
  }

  /** eth_blockNumber / prime_blockNumber */
  async getBlockNumber(): Promise<number> {
    const result = (await this.request('eth_blockNumber')) as string;
    return hexToNumber(result);
  }

  /** eth_getBalance / prime_getBalance */
  async getBalance(address: string): Promise<string> {
    const result = (await this.request('eth_getBalance', [address])) as string;
    return hexToString(result);
  }

  /** eth_getTransactionCount / prime_getTransactionCount */
  async getTransactionCount(address: string): Promise<number> {
    const result = (await this.request('eth_getTransactionCount', [
      address,
    ])) as string;
    return hexToNumber(result);
  }

  /** eth_getCode / prime_getCode */
  async getCode(address: string): Promise<string> {
    const result = (await this.request('eth_getCode', [address])) as string;
    return result || '0x';
  }

  /** eth_getStorageAt / prime_getStorageAt */
  async getStorageAt(address: string, slot: string): Promise<string> {
    const result = (await this.request('eth_getStorageAt', [
      address,
      slot,
    ])) as string;
    return hexToString(result);
  }

  /** eth_getBlockByNumber / prime_getBlockByNumber */
  async getBlockByNumber(
    blockNumber: number | 'latest',
    includeTxs = false
  ): Promise<Block | null> {
    const tag =
      blockNumber === 'latest'
        ? 'latest'
        : '0x' + blockNumber.toString(16);
    const result = (await this.request('eth_getBlockByNumber', [
      tag,
      includeTxs,
    ])) as Block | null;
    return result;
  }

  /** eth_getBlockByHash */
  async getBlockByHash(
    hash: string,
    includeTxs = false
  ): Promise<Block | null> {
    const result = (await this.request('eth_getBlockByHash', [
      hash,
      includeTxs,
    ])) as Block | null;
    return result;
  }

  /** eth_getTransactionReceipt / prime_getTransactionReceipt */
  async getTransactionReceipt(hash: string): Promise<Receipt | null> {
    const result = (await this.request('eth_getTransactionReceipt', [
      hash,
    ])) as Receipt | null;
    return result;
  }

  /** eth_call / prime_call */
  async call(tx: CallParams): Promise<string> {
    const result = (await this.request('eth_call', [tx])) as string;
    return result || '0x';
  }

  /** eth_estimateGas */
  async estimateGas(tx: CallParams): Promise<number> {
    const result = (await this.request('eth_estimateGas', [tx])) as string;
    return hexToNumber(result);
  }

  /** eth_sendTransaction / prime_sendTransaction */
  async sendTransaction(tx: TransactionParams): Promise<string> {
    const result = (await this.request('eth_sendTransaction', [tx])) as string;
    return result;
  }

  /** eth_chainId / prime_chainId */
  async getChainId(): Promise<number> {
    const result = (await this.request('eth_chainId')) as string;
    return hexToNumber(result);
  }

  /** eth_gasPrice / prime_gasPrice */
  async getGasPrice(): Promise<string> {
    const result = (await this.request('eth_gasPrice')) as string;
    return hexToString(result);
  }

  /** Get the WebSocket URL if configured */
  getWsUrl(): string | undefined {
    return this.wsUrl;
  }

  /** Get the HTTP RPC URL */
  getUrl(): string {
    return this.url;
  }
}
