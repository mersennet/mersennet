# Mersennet 12-Month Roadmap

Status snapshot (privacy-fork era): the privacy hard fork is
engineering-complete in-repo — shielded state, shielded CLOB/FBA,
sealed-bid liquidations, threshold mempool, SP1 state proofs (pinned
vkey + captured local transcript), bridge contracts, and SDK shielded
clients are merged on `feat/zk-privacy`. See
[`docs/delivery/privacy-fork-remaining-work.md`](docs/delivery/privacy-fork-remaining-work.md)
and [`docs/STATUS.md`](docs/STATUS.md) for live workstream status.

## Months 1–3 (Privacy fork close-out)

- E5: SP1 Groth16 wrapping circuit + verifying key; install and lock the
  key in the Ethereum bridge `Groth16Verifier`
- Credentialed network-prover run (E4 execution) for delegated proving
- Assemble the external audit packet (`docs/security/privacy-fork-audit-packet.md`)
- Start the 8-week privacy testnet bake (H6) on chain 7920

## Months 4–6 (Audit + hardening)

- External crypto / protocol / Solidity audits (I1–I6) + fix cycle and re-audit
- Bake-period chaos drills, load testing, and incident-response runbook rehearsal
- Performance profiling and mempool/execution optimizations

## Months 7–9 (Governance + activation)

- Governance activation vote and validator upgrade schedule (J1–J6)
- Execute the mainnet hard-fork runbook (`docs/runbooks/zk-fork-activation.md`)
- Mainnet launch checklist (`mainnet/launch-checklist.md`), chain ID 8191

## Months 10–12 (Ecosystem)

- Stable shielded SDK APIs (TypeScript / Go / Python) and indexers
- PrimeTrade shielded order UI (external repo: prime-trade)
- Operator documentation + partner onboarding
