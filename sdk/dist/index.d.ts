/**
 * @prime-chain/sdk - TypeScript SDK for Mersennet
 *
 * Mersennet is a Layer 1 blockchain with JSON-RPC API and WebSocket subscriptions.
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
export { PrimePrecompile, PRECOMPILE_ADDRESS, SELECTORS, encodePlaceOrder, encodeCancelOrder, encodeDepositCollateral, encodeWithdrawCollateral, encodeGetPosition, encodeGetCollateral, encodeIsLiquidatable, encodeGetBestBidAsk, } from './precompile';
export type { Block, Transaction, Receipt, LogEntry, Order, OrderBook, OrderBookLevel, OrderOutcome, Position, Trade, CallParams, CodeAttestation, ContractPublicationStatus, TransactionParams, BatchOrderParams, SubscriptionEvent, BookUpdate, } from './types';
export { ShieldedClient, ViewingKeyHelpers, createMockNoteDecryptor, createOwnerViewingMaterial, scanGrantedNotes, parseEncryptedNotePayload, parseShieldedNotePlaintext, } from './shielded';
export type { Note, EncryptedNote, GrantedDecryptedNote, GrantedNoteScanOptions, GrantedNoteScanResult, GrantedViewingMaterial, ShieldedBalance, ViewingKey, OrderPlacePublicInputs, ZkProver, ShieldedClientOptions, } from './shielded';
export type { Fr } from './shielded';
export { reconstructPortfolio, defaultNullifierDeriver, scanAndReconstructBalances, } from './reconstruction';
export type { NullifierDeriver, PortfolioNote, ReconstructedPortfolio, ReconstructOptions, ScanReconstructOptions, BalanceReconstructionResult, } from './reconstruction';
export { NoirWasmProver } from './noir-prover';
export type { NoirCircuitName, NoirInputValue, NoirProvingBackend, NoirWasmProverOptions, } from './noir-prover';
export { reconstructOpenOrders, reconstructPositions, } from './positions';
export type { OrderSide, OrderRecord, FillRecord, OpenOrder, ReconstructedPosition, ReconstructTradingOptions, } from './positions';
export { deriveMigrationNote, matchesMigrationNote, defaultNoteCommitment, planMigration, confirmMigration, MIGRATION_RHO_LABEL, MIGRATION_PSI_LABEL, } from './migration';
export type { MigrationNote, MigrationNoteParams, MigrationPlan, MigrationConfirmation, NoteCommitmentHasher, } from './migration';
//# sourceMappingURL=index.d.ts.map