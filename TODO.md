# Mersennet TODO

## Core Networking & Execution
- [x] Proper mempool + transaction validation pipeline.
- [x] Full consensus rounds with timeouts and slashing evidence.
- [x] RPC layer (JSON-RPC, logs, receipts).

## Configuration
- [x] Genesis config + config management.

## Testing
- [x] State tests.
- [x] Consensus simulations.
- [x] Fuzzing.

## Next Steps
- [x] Expand JSON-RPC: tx submit, logs, filters, receipts by hash.
- [x] Harden P2P gossip: retry/backoff, peer discovery, dedup tuning.
- [x] Implement state sync: snapshot download + verification.
- [x] Add integration tests: full block execution + receipt checks.
- [x] Add CLI modes: validator/full node, devnet flags.
- [x] Add metrics/logging and structured error reporting.
- [x] Networked state sync: chunked snapshot transfer + verification over TCP.
- [x] Structured error types across RPC/mempool/engine.
- [x] Persist peers + node identity keys.
- [x] Config hot-reload + health endpoints.

## PrimeOrders + PrimeEVM
- [x] Define PrimeOrders state schema (accounts, orders, positions, markets).
- [x] Implement deterministic matching engine (limit/market, price-time).
- [x] Add risk engine (margin, liquidation, funding index).
- [x] Add cross-domain bridge queue (PrimeOrders ⇄ PrimeEVM).
- [x] Expose PrimeOrders RPC endpoints + docs.
- [x] Add end-to-end tests for matching + settlement.
