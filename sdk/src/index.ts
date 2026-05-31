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
  CodeAttestation,
  ContractPublicationStatus,
  TransactionParams,
  BatchOrderParams,
  SubscriptionEvent,
  BookUpdate,
} from './types';

// Shielded SDK (Phase 6 of the privacy redesign). Use this when
// connecting to a chain that has activated the ZK privacy hard fork.
export {
  ShieldedClient,
  ViewingKeyHelpers,
  createMockNoteDecryptor,
  createOwnerViewingMaterial,
  scanGrantedNotes,
  parseEncryptedNotePayload,
  parseShieldedNotePlaintext,
} from './shielded';
export type {
  Note,
  EncryptedNote,
  GrantedDecryptedNote,
  GrantedNoteScanOptions,
  GrantedNoteScanResult,
  GrantedViewingMaterial,
  ShieldedBalance,
  ViewingKey,
  OrderPlacePublicInputs,
  ZkProver,
  ShieldedClientOptions,
} from './shielded';
export type { Fr } from './shielded';

// Client-side portfolio reconstruction (ADR-019 balances:read).
export {
  reconstructPortfolio,
  defaultNullifierDeriver,
} from './reconstruction';
export type {
  NullifierDeriver,
  PortfolioNote,
  ReconstructedPortfolio,
  ReconstructOptions,
} from './reconstruction';

// Client-side Noir proving (Workstream F1). The backend is injected by the
// wallet and wraps @noir-lang/noir_js + @aztec/bb.js.
export { NoirWasmProver } from './noir-prover';
export type {
  NoirCircuitName,
  NoirInputValue,
  NoirProvingBackend,
  NoirWasmProverOptions,
} from './noir-prover';

// Privacy-fork migration UX helpers (Workstream F4).
export {
  deriveMigrationNote,
  matchesMigrationNote,
  defaultNoteCommitment,
  MIGRATION_RHO_LABEL,
  MIGRATION_PSI_LABEL,
} from './migration';
export type {
  MigrationNote,
  MigrationNoteParams,
  NoteCommitmentHasher,
} from './migration';
