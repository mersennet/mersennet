# Prime Chain Infrastructure Roadmap

**Version:** 1.0
**Period:** 12 months (Q1 2026 -- Q1 2027)
**Target:** Mainnet launch (Chain ID 13370) by end of Q4

---

## Executive Summary

This roadmap defines a 4-quarter path from the current testnet (Chain ID 7919, 4 validators on Hetzner VPS) to a production-grade mainnet with 21+ validators, a complete developer ecosystem, and institutional-grade operations. The plan is conservative by design: the first three quarters focus on hardening, testing, and security, with mainnet launch gated by a completed external audit.

---

## Current State and Gap Analysis

### What Exists

| Layer | Component | Status |
|-------|-----------|--------|
| **Consensus** | HotStuff-2 + CometBFT (DPoS) | Implemented, tested in simulation |
| **EVM** | revm v12, Shanghai spec, parallel execution | Implemented |
| **PrimeOrders** | Native CLOB via precompile at 0x0100 | Implemented |
| **Networking** | Custom UDP gossip, TCP block sync, Noise encryption | Implemented |
| **State** | Sled + ReDB backends, Merkle proofs, snapshots | Implemented |
| **Mempool** | Fee-ordered pool with replacement/eviction | Implemented |
| **RPC** | Core eth_* methods, prime_* extensions | Partial |
| **Testnet** | 4 validators, explorer, DEX, faucet, docs, monitoring | Live |
| **CI** | cargo check/test/clippy/fmt via GitHub Actions | Basic |
| **Monitoring** | Prometheus + Grafana dashboards | Configured |
| **Deployment** | Shell scripts, systemd, Docker Compose, Caddy TLS | Manual |

### Critical Gaps

| Gap | Impact | Severity |
|-----|--------|----------|
| Consensus uses `NetworkSim` (in-memory mock) instead of real P2P | Consensus not battle-tested over network | Critical |
| Block headers lack timestamps | Cannot calculate block time, breaks tooling assumptions | Critical |
| Missing `eth_subscribe`, filter APIs, `eth_feeHistory` | Incompatible with standard Ethereum tooling (wagmi, viem, ethers.js) | High |
| No WebSocket JSON-RPC subscriptions | Cannot use real-time event listeners | High |
| No snap sync or light client protocol | New nodes must download full history | High |
| Unsigned transactions accepted | Security vulnerability | High |
| No external security audit | Cannot launch mainnet | Critical |
| No indexer/subgraph | Cannot build data-rich dApps | High |
| No bridge | No cross-chain liquidity or interoperability | High |
| No published SDKs | Developers cannot easily integrate | Medium |
| No CI/CD beyond lint/test | Manual builds, manual deploys, no release automation | Medium |
| No alert rules in monitoring | Silent failures go undetected | Medium |
| Ecosystem apps hardcoded to testnet URLs | Cannot switch to mainnet without code changes | Medium |
| No contract test suite | Deployed contracts untested | Medium |
| Hardcoded private keys in seed scripts | Security risk | Medium |
| No RPC rate limiting | Vulnerable to abuse | Medium |

---

## Q1 -- Foundation and Hardening (Months 1-3)

**Theme:** Make the chain production-worthy at the protocol level.

### 1.1 Core Chain

| Task | Files/Components | Description |
|------|-----------------|-------------|
| Wire consensus over P2P | `crates/core/src/consensus.rs`, `crates/network/src/p2p.rs` | Replace `NetworkSim` with real UDP gossip for vote/proposal propagation. Consensus messages become first-class gossip topics alongside block/tx announcements. |
| Block header timestamps | `crates/core/src/engine.rs`, `crates/core/src/state.rs` | Add Unix timestamp to block headers. Validate monotonic increase. Expose via `eth_getBlockByNumber` response. |
| RPC: subscription support | `crates/rpc/src/ws.rs`, `crates/rpc/src/rpc.rs` | Implement `eth_subscribe` (newHeads, logs, pendingTransactions) and `eth_unsubscribe` over WebSocket. Current WS only serves PrimeOrders trade events. |
| RPC: filter APIs | `crates/rpc/src/rpc_router.rs` | Implement `eth_newFilter`, `eth_newBlockFilter`, `eth_newPendingTransactionFilter`, `eth_getFilterChanges`, `eth_getFilterLogs`, `eth_uninstallFilter`. |
| RPC: fee history | `crates/rpc/src/rpc.rs` | Implement `eth_feeHistory` and `eth_maxPriorityFeePerGas` for EIP-1559 tooling compatibility. |
| Snap sync protocol | `crates/network/src/net_transport.rs` | Implement state trie download by key range. New nodes sync state at a recent block, then backfill headers. Reduces sync from hours to minutes. |
| Reject unsigned transactions | `crates/core/src/engine.rs` | Enforce signature validation for all transactions. Remove `allow_unsigned` path. Migration period with deprecation warning on testnet. |

### 1.2 CI/CD and Testing

| Task | Location | Description |
|------|----------|-------------|
| Docker image CI | `.github/workflows/ci.yml` | Build multi-arch Docker images on push/tag. Push to GitHub Container Registry (`ghcr.io/primenumberslabs/prime-chain`). |
| Integration test job | `.github/workflows/integration.yml` | Spin up 3-node Docker testnet in CI. Run RPC conformance test suite against it (eth_* method coverage). |
| Contract test suite | `contracts/test/` | Hardhat/Foundry tests for WPRIM, PrimeSwapFactory, PrimeSwapRouter, MockERC20 tokens. Cover swap, liquidity, edge cases. |
| Release automation | `.github/workflows/release.yml` | Tag-based builds. Auto-generate changelog from conventional commits. Attach binaries + Docker tags to GitHub Releases. |
| Benchmark regression | `.github/workflows/ci.yml` | Run `tps_bench` and `parallel_bench` in CI. Fail if TPS drops below threshold. Store results as artifacts for trend tracking. |

### 1.3 Monitoring and Alerting

| Task | Location | Description |
|------|----------|-------------|
| Alert rules | `deploy/monitoring/alerts.yml` | Block production stall (no new block in 30s), validator offline, RPC error rate > 5%, disk usage > 80%, memory usage > 90%. |
| Alertmanager config | `deploy/monitoring/alertmanager.yml` | Configure Slack and/or Discord webhook receivers. Escalation to PagerDuty or email for critical alerts. |
| Uptime monitoring | External service or `monitoring/` | Deploy Uptime Kuma or Prometheus Blackbox Exporter. Monitor all public endpoints (RPC, Explorer, Faucet, DEX, Docs). |
| Status page | Public URL | Public status page (e.g., Uptime Kuma status page or Cachet). Show real-time status of all services. |

### Q1 Deliverables

- [ ] Consensus messages propagated over real P2P network
- [ ] Block headers include timestamps
- [ ] `eth_subscribe`, filter APIs, and `eth_feeHistory` implemented
- [ ] Snap sync protocol working
- [ ] Unsigned transactions rejected
- [ ] Docker images auto-built in CI
- [ ] Integration tests running in CI against Docker testnet
- [ ] Contract test suite passing
- [ ] Release automation on tags
- [ ] Alert rules and receivers configured
- [ ] Public status page live

---

## Q2 -- Production Readiness (Months 4-6)

**Theme:** Build the missing infrastructure services and harden security.

### 2.1 Indexer and Data Layer

| Task | Description |
|------|-------------|
| Subgraph/indexer service | Build or integrate an event indexer. Index all contract events, token transfers, DEX swaps. Expose GraphQL API for frontend queries. |
| Historical log indexing | Extend `eth_getLogs` with pagination support and a persistent log index (not just in-memory scan). Enable block-range queries over full chain history. |
| Debug/trace APIs | Implement `debug_traceTransaction` and `trace_block` (at minimum `callTracer` and `prestateTracer`). Required for advanced debugging and security tooling. |

### 2.2 Bridge

| Task | Description |
|------|-------------|
| Testnet bridge | Prime Chain (7919) <-> Sepolia bridge. Lock-and-mint architecture for ETH and ERC-20 tokens. Relayer service. |
| Bridge monitoring | Dashboard for bridge balances, pending transfers, relayer health. Alerts for stuck transactions or balance discrepancies. |
| Audit preparation | Document bridge contract architecture, threat model, and known risks. Prepare scope for inclusion in the security audit. |

### 2.3 SDK and Developer Experience

| Task | Description |
|------|-------------|
| TypeScript SDK | Publish `@primechain/sdk` to npm. Wraps ethers.js/viem with Prime Chain defaults (chain config, PrimeOrders ABI, contract addresses). |
| Python SDK | Publish `primechain` to PyPI. Web3.py wrapper with PrimeOrders support. |
| SDK documentation | API reference, getting-started guide, code examples for common operations (connect, send tx, call PrimeOrders, read events). |
| Hardhat plugin | `@primechain/hardhat-plugin`: auto-configure network, deploy helpers, PrimeOrders task integration. |

### 2.4 Security Hardening

| Task | Description |
|------|-------------|
| Remove hardcoded keys | Replace all hardcoded private keys in `contracts/script/seed-liquidity.cjs` and deploy scripts with environment variables. Document key management in validator guide. |
| RPC rate limiting | Per-IP and per-method rate limits on the public RPC endpoint. Configurable via node config. Return HTTP 429 with Retry-After header. |
| Validator key rotation | Tooling to rotate validator keys without downtime: generate new key, register on-chain, switch config, deregister old key. |
| Noise protocol review | Internal review of the Noise_XX_25519_ChaChaPoly_BLAKE2s implementation in `crates/network/src/noise.rs`. Document threat model and known limitations. |
| Audit scope document | Define the scope for the external security audit: consensus, EVM execution, PrimeOrders precompile, bridge contracts, P2P networking. Select 2-3 audit firms and request proposals. |

### 2.5 Ecosystem App Hardening

| Task | Description |
|------|-------------|
| Configurable endpoints | All apps (explorer, DEX, validator dashboard) read RPC URL, explorer URL, and chain ID from a config file or environment variable. No hardcoded URLs. |
| Build pipeline | Add Vite or esbuild bundling for each app. Minification, source maps, cache-busting asset hashes. |
| E2E tests | Playwright test suite for critical flows: explorer block/tx/address pages, DEX swap flow, validator dashboard load. |
| Error handling | Graceful error states for RPC failures, wallet not connected, network mismatch. Offline detection and retry logic. |

### Q2 Deliverables

- [ ] Indexer service live with GraphQL API
- [ ] `debug_traceTransaction` and `trace_block` implemented
- [ ] Testnet bridge (Prime Chain <-> Sepolia) operational
- [ ] TypeScript SDK published to npm
- [ ] Python SDK published to PyPI
- [ ] Hardhat plugin published
- [ ] All hardcoded private keys removed
- [ ] RPC rate limiting enabled
- [ ] Validator key rotation tooling ready
- [ ] Audit scope document and firm selection complete
- [ ] All ecosystem apps configurable and bundled
- [ ] E2E test suite passing for all apps

---

## Q3 -- Public Testnet and Ecosystem Growth (Months 7-9)

**Theme:** Open the testnet, grow the validator set, and battle-test everything.

### 3.1 Public Testnet

| Task | Description |
|------|-------------|
| Permissionless validator onboarding | Open staking contract on testnet. Any node operator can stake and join the validator set. Documentation for the full process. |
| Validator growth | Target 15-21 validators (from current 4). Recruit from community, partners, and infrastructure providers. |
| Faucet hardening | Rate limiting (per-address, per-IP), CAPTCHA, daily limits. Prevent testnet token hoarding. |
| Testnet incentive program | Bug bounty for chain-level bugs. Validator uptime rewards. Developer grants for building on testnet. |

### 3.2 Chain Improvements

| Task | Description |
|------|-------------|
| Light client protocol | Header-only sync mode. Verify block hashes and state proofs without downloading full state. Enables mobile wallets and browser-based verification. |
| DAG mempool | Integrate `crates/core/src/dag_mempool.rs` for higher throughput transaction ordering. Evaluate impact on block production latency. |
| PrimeOrders per-market locking | Replace global `Arc<Mutex<>>` with per-market locks in the precompile. Enables parallel order matching across different markets. |
| State pruning automation | Configurable state retention window. Automatic pruning of old state beyond the window. Reduces disk usage for long-running nodes. |
| Cancun EVM evaluation | Evaluate adding EIP-4844 (blob transactions), EIP-1153 (transient storage), EIP-6780 (SELFDESTRUCT restriction). Implement if feasible without consensus-breaking changes. |

### 3.3 Governance and Operations

| Task | Description |
|------|-------------|
| On-chain governance | Proposal creation, voting (stake-weighted), time-locked execution. Minimum quorum and approval thresholds. Used for parameter changes and protocol upgrades. |
| Multi-sig for upgrades | Deploy a Safe-style multi-sig for protocol-level operations. Require N-of-M validator signatures for critical changes. |
| Incident response runbook | Step-by-step procedures for: block production stall, validator compromise, RPC outage, bridge halt, chain halt, and emergency upgrade. |
| Validator coordination | Private communication channel for validators. Upgrade coordination process. On-call rotation schedule. |

### 3.4 External Security Audit

| Task | Timeline |
|------|----------|
| Audit kickoff | Month 7 |
| Audit scope: consensus, EVM, PrimeOrders, bridge, P2P | Month 7-8 |
| Receive preliminary findings | Month 8 |
| Address critical and high-severity findings | Month 8-9 |
| Final audit report | Month 9 |
| Publish audit report publicly | Month 9 |

### Q3 Deliverables

- [ ] Public testnet with permissionless staking
- [ ] 15+ active validators
- [ ] Faucet hardened with rate limits and CAPTCHA
- [ ] Light client protocol implemented
- [ ] DAG mempool evaluated/integrated
- [ ] PrimeOrders per-market locking implemented
- [ ] On-chain governance module deployed
- [ ] Multi-sig deployed for protocol operations
- [ ] Incident response runbook documented
- [ ] Security audit complete, findings addressed
- [ ] Audit report published

---

## Q4 -- Mainnet Launch (Months 10-12)

**Theme:** Launch mainnet and establish production operations.

### 4.1 Pre-Launch (Month 10)

| Task | Description |
|------|-------------|
| Audit remediation verification | All critical and high-severity findings resolved. Re-verified by auditor or internal review. |
| Genesis validator set | Minimum 7 validators confirmed. Target 21. Hardware requirements met (`mainnet/validator-requirements.md`: 16+ cores, 64 GB RAM, 2 TB NVMe). |
| Genesis ceremony | Multi-party genesis generation. Each validator generates keys independently. Genesis block assembled from validator registrations. Chain ID 13370. |
| Stress test | Sustained load test for 72+ hours. Target: 1,000+ TPS with mixed workload (transfers, DEX swaps, PrimeOrders). Monitor for memory leaks, state bloat, consensus liveness. |
| Mainnet dry run | Deploy mainnet binary on an isolated network. Run through the full launch sequence. Verify monitoring, alerting, and incident response procedures. |

### 4.2 Launch (Month 11)

| Task | Description |
|------|-------------|
| Genesis block | Mainnet genesis block produced. Chain ID 13370. Initial token distribution per tokenomics (700M block rewards pool, team/foundation/ecosystem allocations). |
| Validator monitoring | 24/7 monitoring with on-call rotation. Alert escalation path defined and tested. |
| Ecosystem apps on mainnet | Explorer, DEX, validator dashboard, docs all pointed to mainnet. Testnet versions remain available on separate URLs. |
| Bridge activation | Prime Chain <-> Ethereum mainnet bridge. Start with ETH and USDC. Conservative initial limits. |
| SDK mainnet update | SDK packages updated with mainnet chain config, contract addresses, and RPC endpoints. |
| Documentation update | All docs updated for mainnet: network info, contract addresses, wallet setup, developer guides. Testnet docs moved to a "Testnet" section. |

### 4.3 Post-Launch (Month 12)

| Task | Description |
|------|-------------|
| 30-day stability window | Monitor block production, consensus liveness, validator participation, RPC availability. Target: 99.9% uptime. |
| Performance tuning | Adjust gas limit, block time, mempool parameters based on real mainnet traffic patterns. |
| Community validator program | Open validator onboarding to the public. Documentation, tooling, and support for new validators. |
| First governance proposal | Submit and execute a governance proposal (e.g., parameter adjustment) to validate the governance flow end-to-end. |
| Year 2 planning | Retrospective on Year 1. Define Year 2 priorities: state sharding, cross-chain bridges, advanced ZK features, ecosystem growth. |

### Q4 Deliverables

- [ ] All audit findings resolved and verified
- [ ] Genesis ceremony complete
- [ ] 72-hour stress test passed
- [ ] Mainnet genesis block (Chain ID 13370)
- [ ] 21+ validators active
- [ ] Explorer, DEX, dashboard live on mainnet
- [ ] Bridge active (Prime Chain <-> Ethereum)
- [ ] SDKs updated for mainnet
- [ ] 30-day stability window completed at 99.9% uptime
- [ ] First governance proposal executed
- [ ] Year 2 roadmap drafted

---

## Success Metrics

| Metric | Q1 | Q2 | Q3 | Q4 |
|--------|----|----|----|----|
| **Validators** | 4 (testnet) | 4 (testnet) | 15+ (public testnet) | 21+ (mainnet) |
| **RPC conformance** | 90% eth_* methods | 95% + debug/trace | 98% | 99%+ |
| **CI pipeline** | Build + test + lint | + Docker + integration | + E2E + benchmarks | Full |
| **Test coverage** | Core unit tests | + contract + SDK tests | + E2E + load tests | + audit verification |
| **Uptime (target)** | 95% | 99% | 99.5% | 99.9% |
| **Block time** | ~1s | ~1s | ~1s | ~1s |
| **Security audit** | -- | Scope prepared | Audit complete | Findings resolved |
| **Published packages** | 0 | 3 (TS SDK, Py SDK, HH plugin) | 3 | 3 (mainnet-updated) |
| **Bridge** | -- | Testnet live | Testnet battle-tested | Mainnet live |
| **Indexer** | -- | Live | Battle-tested | Mainnet live |

---

## Dependency Graph

```
Q1: Foundation                  Q2: Production               Q3: Public Testnet           Q4: Mainnet
-----------------               ------------------           --------------------         ---------------

P2P Consensus ──────────────────────────────────────────────> Public Validator Onboarding
                                                                       │
RPC Completeness ───────> Indexer Service ──────────────────> Public Testnet               Genesis Ceremony
           │                                                           │                        │
           └──────────> SDK Publication                                │                        v
                                                                       │                  Mainnet Launch
CI/CD Pipeline ─────────> App Hardening                                │                        │
           │                                                           │                        v
           └──────────────────────────────> Security Audit ───> Audit Remediation ──────> Mainnet Launch
                                                   ^
Security Hardening ────────────────────────────────┘
                                                   ^
Bridge (Testnet) ──────────────────────────────────┘

Alert Rules ────────────────────────────────────────────────────────────────────────> Mainnet Monitoring

                                                 Governance Module ─────────────────> First Governance Proposal
```

---

## Risk Register

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| Audit reveals critical consensus flaw | Medium | Critical | Start internal fuzzing and adversarial testing in Q1. Budget 2 months for remediation. |
| Insufficient validator interest for mainnet | Medium | High | Begin outreach in Q2. Offer incentives. Ensure minimum viable set (7) with team-controlled nodes as fallback. |
| Bridge exploit | Medium | Critical | Start with conservative limits. Implement emergency pause. Include bridge in audit scope. |
| P2P networking issues under load | Medium | High | Load test P2P layer separately in Q2. Evaluate libp2p migration if custom stack proves unreliable. |
| SDK adoption too slow | Low | Medium | Ensure SDK works seamlessly with popular frameworks. Provide comprehensive examples and tutorials. |
| Testnet state corruption | Low | High | Automated state backups. Snapshot export/import for recovery. Document rollback procedures. |

---

## Resource Estimates

| Quarter | Focus Area | Estimated Effort |
|---------|-----------|-----------------|
| Q1 | Core chain + CI/CD + monitoring | 2-3 full-time engineers |
| Q2 | Indexer + bridge + SDKs + security | 3-4 full-time engineers |
| Q3 | Public testnet + audit + governance | 2-3 full-time engineers + external auditors |
| Q4 | Launch + operations | 2-3 full-time engineers + DevOps |

External costs: security audit ($100K-$300K depending on scope and firm), infrastructure scaling for public testnet and mainnet.

---

## References

- [Ecosystem Roadmap](ECOSYSTEM_ROADMAP.md) -- current testnet state and app integration plan
- [Technical Reference](TECHNICAL_REFERENCE.md) -- chain architecture and API documentation
- [Whitepaper](whitepaper.md) -- protocol design and economic model
- [Mainnet Launch Checklist](../mainnet/launch-checklist.md) -- pre/post-launch task list
- [Validator Requirements](../mainnet/validator-requirements.md) -- hardware and staking requirements
