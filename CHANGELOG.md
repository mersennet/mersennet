## [unreleased]

### 🚀 Features

- Update math formatting in whitepaper
- Remove jump lines in whitepaper math
- Cargo workspace restructure, testnet deployment, and full feature suite
- Redesign tokenomics — 1B PRIM supply, 70/10/10/5/5 allocation, 13-year emission (#3)
- Finalize Mersenne-prime tokenomics — supply cap 2^89−1 wei (~618.97M MRSN), block reward 2^61−1 wei (~2.3 MRSN), halving every 33,550,336 blocks (5th perfect number)
- Core engine, RPC, crypto, and network improvements
- Smart contracts — MersennetOrders, Multicall3, and Foundry setup
- Rebrand faucet, dashboard, and docs to match Mersennet design system
- CLOB markets, integration contracts, net_peerCount, and full RPC docs
- Chain feature flags and precompile stubs for future upgrades
- *(ws)* Block-watcher thread + method-not-supported error response
- *(zk)* Phase 1 — shielded_state, threshold_mempool, circuit integration
- *(zk)* Phases 2–7 — shielded CLOB, auctions, EVM bridge, state proofs, SDK, ADRs
- *(zk)* Privacy testnet bring-up on chain 7920
- Add GitHub Actions workflow for Rust CI
- Add code publication precompile and RPC methods
- Fix checks and run linters
- Update failling checks
- Fix cargo audit warnings
- Update docs and add capability-based viewing-key model
- Add proving adapters, view-notes SDKs, and SP1 host scaffolding
- Exclude python cache files from git
- Remove pycache files from git tracking
- Continue zk SP1 CI improvements
- Continue SP1 proof and verification adapter development
- Patch SP1 proof generation and verification
- Update scripts and docs for zk privacy
- *(zk/K3,K4,K5)* Coverage, dev image, and rustdoc CI
- *(zk/F5)* Client-side portfolio (balance) reconstruction
- *(sdk)* F1 Noir WASM prover, F2 owner-side scan, F4 migration UX
- *(sdk/F5)* Client-side open-order and position reconstruction
- *(zk/G,E5)* Ethereum bridge contracts + Groth16 verifier
- *(zk/E4)* Resolve sp1-sdk/network c-kzg conflict by making the SP1 host revm-free
- *(zk/E2)* Harden SP1 public-output derivation + engine parity
- *(zk/E2)* Bind prev->new state-root continuity in the executor
- Harden SP1/zk proof pipeline and shielded state handling
- Fix test calling build_proofs with unordered intents
- Update zk privacy data to run sp1 prove request
- *(zk/E3)* Capture real local SP1 prove/verify transcript + pin reproducible vkey
- Update zk-e3 docs and remove logs
- Update zk privacy docs
- *(zk)* Close out E4/E5 + F1-F5 client surfaces
- Fix checks
- Close E5
- *(website)* Add Mersennet landing site
- *(docs)* Rebrand to Mersennet and sync docs to current chain code
- *(rpc)* Return Mersennet/0.1.0 from web3_clientVersion
- *(website)* Live shielded-session terminal, code showcase, proof pipeline, brand polish
- *(docs-site,website)* On-brand docs landing and homepage, nav Build entry
- *(website)* Add investment-grade Thesis, Edge, and Momentum sections
- Rename native token ticker PRIM -> MRSN
- Transition from monorepo to separate repos
- *(zkp)* Make shielded proofs verifiable end-to-end
- *(deploy)* Prover-enabled rolling upgrade with verifier self-test

### 🐛 Bug Fixes

- Change chain ID from 999 to 131071 (6th Mersennet number)
- Update genesis tool hardcoded tokenomics params (#5)
- Faucet token claiming, RPC signature validation, and docs updates
- Failling checks and warnings
- Update foundry toolchain version to stable
- Use renamed mersennet/mersennet_zkp crates in bridge submission example
- Use renamed mersennet_zkp crate in bridge wrap request example
- *(node)* Relay RPC-submitted transactions to peers
- *(network)* Carry tx signature through gossip wire format
- *(node)* Execute transactions on block import so all nodes converge
- *(core)* Resume chain at persisted height on startup
- *(network)* Add block-sync client so behind nodes catch up
- *(network)* TCP block-sync shares the UDP gossip port
- *(rpc)* Route shielded methods via the comprehensive router
- *(core)* Correct value-conservation accounting and reward accrual

### 💼 Other

- Fix checks and failling tests after privacy mode activation
- SP1 test and host example build regressions
- Replace the legacy swirl logo with the five-bar Mersenne mark
- Complete social media kit generated from the five-bar mark
- Add SVG masters and 2K transparent mark renders
- Adopt Mersenne-Prime chain IDs — mainnet 8191, testnet 131071
- Full Mersennet rebrand — dark terminal theme by default
- Slim to pure blockchain code — extract apps, docs, and SDKs to their own repos
- Purge regenerated build caches left from extracted dirs
- Complete Mersennet brand sweep (phase 1)
- Mersennet phase 2 — cryptographic domain separators
- Complete and brand the Grafana dashboards
- Fix Caddyfile scp basename bug; serve RPC+wss on single domain

### 🚜 Refactor

- Remove explorer from monorepo
- Rename mersennet across crates, binaries, metrics, and SDKs

### 📚 Documentation

- Add comprehensive technical whitepaper v2.0
- Add strategic positioning, pitch deck, one-pager, and MCP tests
- Whitepaper v5 (technical depth) + Technical Reference
- Docusaurus documentation site
- Fundraising materials, ecosystem roadmap, and team info
- Update README with ecosystem repo links and project layout
- Refresh developer entry points for the privacy testnet
- Add privacy-fork consolidation & close-out handoff
- Add privacy-fork remaining-work delivery assessment
- Sync documentation with code (part 1)
- Sync READMEs, testnet/mainnet docs, and audit prep with code
- Sync technical docs, ADRs, runbooks, and fundraising figures with code
- Sync docs-site with code and reconcile emission timeline to 1s default block time
- Point repo URLs at mersennet ahead of repo migration
- Point repo links at the canonical mersennet/mersennet home
- Point editUrl at the standalone mersennet/docs repo
- Migrate docs-site from Docusaurus to Astro Starlight
- Enforce link validation, fix 42 broken links, correct consensus claims
- Port standalone-repo updates into the monorepo
- Replace last legacy RPC domain reference
- *(test)* Native token is MRSN, not the legacy PRIM ticker

### 🎨 Styling

- Apply cargo fmt to bridge_export.rs

### ⚙️ Miscellaneous Tasks

- Remove sdk/node_modules from tracking, update .gitignore
- Update .gitignore for ecosystem app artifacts
- *(phase-0)* Repo consolidation + Phase 1 ZKP crate scaffold
- *(zk/E3)* Pin deterministic prove output + pre-fill verify request
- Rebrand Mersennet Chain prose to Mersennet across code comments, UI strings, and configs
