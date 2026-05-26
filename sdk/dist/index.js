"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
exports.parseShieldedNotePlaintext = exports.parseEncryptedNotePayload = exports.scanGrantedNotes = exports.createMockNoteDecryptor = exports.ViewingKeyHelpers = exports.ShieldedClient = exports.encodeGetBestBidAsk = exports.encodeIsLiquidatable = exports.encodeGetCollateral = exports.encodeGetPosition = exports.encodeWithdrawCollateral = exports.encodeDepositCollateral = exports.encodeCancelOrder = exports.encodePlaceOrder = exports.SELECTORS = exports.PRECOMPILE_ADDRESS = exports.PrimePrecompile = exports.PrimeSubscription = exports.PrimeOrders = exports.PrimeProvider = void 0;
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
Object.defineProperty(exports, "scanGrantedNotes", { enumerable: true, get: function () { return shielded_1.scanGrantedNotes; } });
Object.defineProperty(exports, "parseEncryptedNotePayload", { enumerable: true, get: function () { return shielded_1.parseEncryptedNotePayload; } });
Object.defineProperty(exports, "parseShieldedNotePlaintext", { enumerable: true, get: function () { return shielded_1.parseShieldedNotePlaintext; } });
