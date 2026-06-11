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

## MersennetOrders + MersennetEVM
- [x] Define MersennetOrders state schema (accounts, orders, positions, markets).
- [x] Implement deterministic matching engine (limit/market, price-time).
- [x] Add risk engine (margin, liquidation, funding index).
- [x] Add cross-domain bridge queue (MersennetOrders ⇄ MersennetEVM).
- [x] Expose MersennetOrders RPC endpoints + docs.
- [x] Add end-to-end tests for matching + settlement.

## Privacy Fork (current — see docs/delivery/privacy-fork-remaining-work.md)
- [x] Shielded state, CLOB/FBA, liquidation auctions, threshold mempool, DKG (A–D).
- [x] Shielded RPC/WS, SDK shielded clients (TS/Go/Python), privacy testnet tooling (C, F, H, K).
- [x] SP1 vkey pin + local prove/verify transcript (E1–E3); network host path turnkey (E4, credential-gated).
- [x] Ethereum bridge contracts + Foundry tests (G1–G3, 21 tests).
- [ ] E5: Groth16 wrapping circuit + verifying key (out-of-repo; bridge `Groth16Verifier` deployable but unkeyed).
- [ ] I1–I6: external crypto / protocol / Solidity audits + fix cycle.
- [ ] J1–J6: governance activation vote + mainnet hard-fork rollout.
- [ ] H6: 8-week testnet bake (gated on E completion).
