# Mersennet Security

## Responsible Disclosure Policy

Mersennet and Prime Numbers Labs take security seriously. We encourage security researchers and the community to report vulnerabilities responsibly.

### What We Ask

1. **Report privately first.** Send vulnerability details to security@primenumberslabs.com before public disclosure.
2. **Give us time to respond.** We aim to acknowledge within 48 hours and provide an initial assessment within 7 days.
3. **Do not exploit.** Do not use the vulnerability for malicious purposes, data exfiltration, or disruption of services.
4. **Cooperate with us.** We may request additional information or clarifications to reproduce and fix the issue.

### What You Can Expect

- **Acknowledgment:** We will confirm receipt of your report.
- **Assessment:** We will triage and assess severity.
- **Fix:** We will work on a fix and, when appropriate, coordinate disclosure timing with you.
- **Credit:** With your permission, we will credit you in our security advisories and release notes.

### Out of Scope

- Issues in third-party dependencies that are not directly exploitable in Mersennet
- Social engineering or physical attacks
- Denial-of-service attacks that require excessive resources (e.g., network flooding)
- Issues that require physical access to the user’s machine

## Bug Bounty Program Outline

Mersennet is preparing a formal bug bounty program. The following is an outline of expected structure:

| Severity | Criteria | Typical Reward Range |
|----------|----------|----------------------|
| **Critical** | Remote code execution, consensus bypass, fund theft, state corruption | $5,000 – $25,000 |
| **High** | Authentication bypass, privilege escalation, significant logic flaws | $2,000 – $10,000 |
| **Medium** | Information disclosure, denial of service affecting availability | $500 – $2,000 |
| **Low** | Minor logic issues, edge cases with limited impact | $100 – $500 |

*Rewards are subject to change and may vary based on impact, quality of report, and program rules. Final terms will be published when the program launches.*

## Known Limitations

- **Parallel execution:** Block-STM validation may have edge cases under high contention; additional audit recommended.
- **Bridge:** The in-process bridge (`bridge.rs`) is domain-to-domain; cross-chain bridge (`cross_chain.rs`) proof verification should be audited for production use.
- **Encrypted mempool:** Threshold decryption and key rotation are experimental; not recommended for production without further review.
- **Commit-reveal:** Fixed 2-block window; timing-sensitive; may require tuning for different network conditions.
- **FBA:** Pro-rata allocation when oversubscribed; edge cases with asymmetric order volumes.
- **Precompile context:** Lock-based context injection; ensure no deadlocks under high load.

## Security Assumptions

- **Consensus:** HotStuff-2 provides safety under partial synchrony with &lt; 1/3 Byzantine stake.
- **Cryptography:** ECDSA secp256k1 (k256) for transaction signing; Keccak-256 for hashing.
- **EVM:** revm v12 (Shanghai spec) is used as the execution backend; we assume its correctness.
- **Storage:** redb/sled for persistence; ACID semantics assumed for crash recovery.
- **Network:** Noise protocol for P2P; authenticated encryption assumed.
- **No trusted setup:** No trusted third parties or secret parameters required for protocol correctness.

## Contact Information

- **Security issues:** security@primenumberslabs.com
- **PGP key:** Available on request for encrypted communication

## Security Advisories

Security advisories will be published at [Mersennet GitHub Security Advisories](https://github.com/prime-chain/prime-chain/security/advisories) (or equivalent) when applicable.
