# Mersennet 12-Month Roadmap

Status snapshot (privacy-fork era): the privacy hard fork is
engineering-complete in-repo — shielded state, shielded CLOB/FBA,
sealed-bid liquidations, threshold mempool, SP1 state proofs (pinned
vkey + captured local transcript), bridge contracts, and SDK shielded
clients are merged on `main`, switched off until activation. Developer
documentation: [shielded RPC](https://docs.mersennet.com/developers/privacy/shielded-rpc/)
and [shielded SDK](https://docs.mersennet.com/developers/privacy/shielded-sdk/).

## Months 1–3 (Privacy fork close-out)

- E5: SP1 Groth16 wrapping circuit + verifying key; install and lock the
  key in the Ethereum bridge `Groth16Verifier`
- Credentialed network-prover run (E4 execution) for delegated proving
- Assemble the external audit packet (`docs/security/privacy-fork-audit-packet.md`)
- Start the 8-week privacy testnet bake (H6) on the privacy testnet, chain
  524287 (2^19 − 1); the public testnet 131071 stays transparent

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
- Mersennet Trade shielded order UI (external repo: mersennet/trade)
- Operator documentation + partner onboarding
