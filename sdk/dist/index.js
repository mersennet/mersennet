"use strict";
/**
 * @mersennet/sdk - TypeScript SDK for Mersennet
 *
 * Mersennet is a Layer 1 blockchain with JSON-RPC API and WebSocket subscriptions.
 * This SDK provides a typed interface for eth_*, prime_*, and primeorders_* methods.
 *
 * @example
 * ```ts
 * import { PrimeProvider, PrimeOrders } from '@mersennet/sdk';
 *
 * const provider = new PrimeProvider('http://localhost:8545');
 * const orders = new PrimeOrders(provider);
 *
 * const book = await orders.getOrderBook(1);
 * const balance = await provider.getBalance('0x...');
 * ```
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.MIGRATION_PSI_LABEL = exports.MIGRATION_RHO_LABEL = exports.confirmMigration = exports.planMigration = exports.defaultNoteCommitment = exports.matchesMigrationNote = exports.deriveMigrationNote = exports.reconstructPositions = exports.reconstructOpenOrders = exports.NoirWasmProver = exports.scanAndReconstructBalances = exports.defaultNullifierDeriver = exports.reconstructPortfolio = exports.parseShieldedNotePlaintext = exports.parseEncryptedNotePayload = exports.scanGrantedNotes = exports.createOwnerViewingMaterial = exports.createMockNoteDecryptor = exports.ViewingKeyHelpers = exports.ShieldedClient = exports.encodeGetBestBidAsk = exports.encodeIsLiquidatable = exports.encodeGetCollateral = exports.encodeGetPosition = exports.encodeWithdrawCollateral = exports.encodeDepositCollateral = exports.encodeCancelOrder = exports.encodePlaceOrder = exports.SELECTORS = exports.PRECOMPILE_ADDRESS = exports.PrimePrecompile = exports.PrimeSubscription = exports.PrimeOrders = exports.PrimeProvider = void 0;
var provider_1 = require("./provider");
Object.defineProperty(exports, "PrimeProvider", { enumerable: true, get: function () { return provider_1.PrimeProvider; } });
var orders_1 = require("./orders");
Object.defineProperty(exports, "PrimeOrders", { enumerable: true, get: function () { return orders_1.PrimeOrders; } });
var subscription_1 = require("./subscription");
Object.defineProperty(exports, "PrimeSubscription", { enumerable: true, get: function () { return subscription_1.PrimeSubscription; } });
var precompile_1 = require("./precompile");
Object.defineProperty(exports, "PrimePrecompile", { enumerable: true, get: function () { return precompile_1.PrimePrecompile; } });
Object.defineProperty(exports, "PRECOMPILE_ADDRESS", { enumerable: true, get: function () { return precompile_1.PRECOMPILE_ADDRESS; } });
Object.defineProperty(exports, "SELECTORS", { enumerable: true, get: function () { return precompile_1.SELECTORS; } });
Object.defineProperty(exports, "encodePlaceOrder", { enumerable: true, get: function () { return precompile_1.encodePlaceOrder; } });
Object.defineProperty(exports, "encodeCancelOrder", { enumerable: true, get: function () { return precompile_1.encodeCancelOrder; } });
Object.defineProperty(exports, "encodeDepositCollateral", { enumerable: true, get: function () { return precompile_1.encodeDepositCollateral; } });
Object.defineProperty(exports, "encodeWithdrawCollateral", { enumerable: true, get: function () { return precompile_1.encodeWithdrawCollateral; } });
Object.defineProperty(exports, "encodeGetPosition", { enumerable: true, get: function () { return precompile_1.encodeGetPosition; } });
Object.defineProperty(exports, "encodeGetCollateral", { enumerable: true, get: function () { return precompile_1.encodeGetCollateral; } });
Object.defineProperty(exports, "encodeIsLiquidatable", { enumerable: true, get: function () { return precompile_1.encodeIsLiquidatable; } });
Object.defineProperty(exports, "encodeGetBestBidAsk", { enumerable: true, get: function () { return precompile_1.encodeGetBestBidAsk; } });
// Shielded SDK (Phase 6 of the privacy redesign). Use this when
// connecting to a chain that has activated the ZK privacy hard fork.
var shielded_1 = require("./shielded");
Object.defineProperty(exports, "ShieldedClient", { enumerable: true, get: function () { return shielded_1.ShieldedClient; } });
Object.defineProperty(exports, "ViewingKeyHelpers", { enumerable: true, get: function () { return shielded_1.ViewingKeyHelpers; } });
Object.defineProperty(exports, "createMockNoteDecryptor", { enumerable: true, get: function () { return shielded_1.createMockNoteDecryptor; } });
Object.defineProperty(exports, "createOwnerViewingMaterial", { enumerable: true, get: function () { return shielded_1.createOwnerViewingMaterial; } });
Object.defineProperty(exports, "scanGrantedNotes", { enumerable: true, get: function () { return shielded_1.scanGrantedNotes; } });
Object.defineProperty(exports, "parseEncryptedNotePayload", { enumerable: true, get: function () { return shielded_1.parseEncryptedNotePayload; } });
Object.defineProperty(exports, "parseShieldedNotePlaintext", { enumerable: true, get: function () { return shielded_1.parseShieldedNotePlaintext; } });
// Client-side portfolio reconstruction (ADR-019 balances:read).
var reconstruction_1 = require("./reconstruction");
Object.defineProperty(exports, "reconstructPortfolio", { enumerable: true, get: function () { return reconstruction_1.reconstructPortfolio; } });
Object.defineProperty(exports, "defaultNullifierDeriver", { enumerable: true, get: function () { return reconstruction_1.defaultNullifierDeriver; } });
Object.defineProperty(exports, "scanAndReconstructBalances", { enumerable: true, get: function () { return reconstruction_1.scanAndReconstructBalances; } });
// Client-side Noir proving (Workstream F1). The backend is injected by the
// wallet and wraps @noir-lang/noir_js + @aztec/bb.js.
var noir_prover_1 = require("./noir-prover");
Object.defineProperty(exports, "NoirWasmProver", { enumerable: true, get: function () { return noir_prover_1.NoirWasmProver; } });
// Client-side open-order & position reconstruction (Workstream F5,
// orders:read / positions:read).
var positions_1 = require("./positions");
Object.defineProperty(exports, "reconstructOpenOrders", { enumerable: true, get: function () { return positions_1.reconstructOpenOrders; } });
Object.defineProperty(exports, "reconstructPositions", { enumerable: true, get: function () { return positions_1.reconstructPositions; } });
// Privacy-fork migration UX helpers (Workstream F4).
var migration_1 = require("./migration");
Object.defineProperty(exports, "deriveMigrationNote", { enumerable: true, get: function () { return migration_1.deriveMigrationNote; } });
Object.defineProperty(exports, "matchesMigrationNote", { enumerable: true, get: function () { return migration_1.matchesMigrationNote; } });
Object.defineProperty(exports, "defaultNoteCommitment", { enumerable: true, get: function () { return migration_1.defaultNoteCommitment; } });
Object.defineProperty(exports, "planMigration", { enumerable: true, get: function () { return migration_1.planMigration; } });
Object.defineProperty(exports, "confirmMigration", { enumerable: true, get: function () { return migration_1.confirmMigration; } });
Object.defineProperty(exports, "MIGRATION_RHO_LABEL", { enumerable: true, get: function () { return migration_1.MIGRATION_RHO_LABEL; } });
Object.defineProperty(exports, "MIGRATION_PSI_LABEL", { enumerable: true, get: function () { return migration_1.MIGRATION_PSI_LABEL; } });
