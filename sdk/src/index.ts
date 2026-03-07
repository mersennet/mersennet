/**
 * @prime-chain/sdk - TypeScript SDK for Prime Chain
 *
 * Prime Chain is a Layer 1 blockchain with JSON-RPC API and WebSocket subscriptions.
 * This SDK provides a typed interface for eth_*, prime_*, and primeorders_* methods.
 *
 * @example
 * ```ts
 * import { PrimeProvider, PrimeOrders } from '@prime-chain/sdk';
 *
 * const provider = new PrimeProvider('http://localhost:8545');
 * const orders = new PrimeOrders(provider);
 *
 * const book = await orders.getOrderBook(1);
 * const balance = await provider.getBalance('0x...');
 * ```
 */

export { PrimeProvider } from './provider';
export { PrimeOrders } from './orders';
export { PrimeSubscription } from './subscription';
export {
  PrimePrecompile,
  PRECOMPILE_ADDRESS,
  SELECTORS,
  encodePlaceOrder,
  encodeCancelOrder,
  encodeDepositCollateral,
  encodeWithdrawCollateral,
  encodeGetPosition,
  encodeGetCollateral,
  encodeIsLiquidatable,
  encodeGetBestBidAsk,
} from './precompile';

export type {
  Block,
  Transaction,
  Receipt,
  LogEntry,
  Order,
  OrderBook,
  OrderBookLevel,
  OrderOutcome,
  Position,
  Trade,
  CallParams,
  TransactionParams,
  BatchOrderParams,
  SubscriptionEvent,
  BookUpdate,
} from './types';
