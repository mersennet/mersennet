# Prime Chain ZK Bridge Contracts

Ethereum-side contracts that anchor Prime Chain's privacy fork (workstreams
G1-G4 and E5). They let Ethereum verify Prime Chain block state-transition
proofs and run a deposit / withdraw message bus secured by that proven state.

## Contracts

| File | Workstream | Purpose |
|---|---|---|
| `IStateProofVerifier.sol` | G1 | Verifier interface + canonical public-input ordering. |
| `Groth16Verifier.sol` | G1 / E5 | BN254 Groth16 verifier using the ecAdd/ecMul/ecPairing precompiles. The verifying key is installed once and then locked. |
| `PrimeChainBridge.sol` | G2 | Consumes state proofs to advance the canonical shielded roots; deposit (lock) / withdraw (Merkle-proven) message bus. |

Tests live in `contracts/test/zk/` (21 tests, run with `forge test --match-path 'test/zk/*'`).

## Public-input contract

The Groth16 proof's public inputs are the nine field elements committed by
`BlockProgramOutput::to_field_elements` (`crates/zkp/src/sp1.rs`), in this
exact order:

| Index | Field |
|---|---|
| 0 | `prev_state_root` |
| 1 | `new_state_root` |
| 2 | `prev_nullifier_root` |
| 3 | `new_nullifier_root` |
| 4 | `block_number` |
| 5 | `block_hash` |
| 6 | `new_market_state_hash` |
| 7 | `shielded_event_root` |
| 8 | `tx_count` |

`Groth16Verifier` therefore has `publicInputCount() == 9` and an IC array of
10 G1 points (`PUBLIC_INPUT_COUNT + 1`).

## Verifying key lifecycle (E5)

The contract is deployable today with no key. The real verifying key is the
output of the SP1 Groth16 wrapping circuit, which is produced as part of E5
(Groth16 wrap) once the SP1 prove path is release-complete. At activation:

1. Deploy `Groth16Verifier`.
2. `setVerifyingKey(alpha1, beta2, gamma2, delta2, ic[10])` with the SP1
   Groth16 export.
3. `lockVerifyingKey()` to make the key immutable.
4. Deploy `PrimeChainBridge(verifier, programVKeyHash, genesisStateRoot,
   genesisNullifierRoot)` where `programVKeyHash` is the pinned
   `PRIME_SP1_VKEY_HASH` (`crates/zkp/params/sp1/state-transition.vk.hash`).

Until the key is installed, `verifyProof` reverts with `NotConfigured`, so
the bridge cannot accept proofs prematurely.

## State-proof intake

`submitStateProof(proof, input)` enforces:

- input length equals `publicInputCount()`;
- `block_number` is strictly monotonic;
- `prev_state_root` / `prev_nullifier_root` match the current on-chain view
  (prev -> new continuity, so the contract tracks one canonical chain);
- the Groth16 proof verifies.

On success it advances `shieldedStateRoot`, `nullifierRoot`, and
`latestProvenBlock`, and emits `StateProofAccepted`.

## Deposit / withdraw bus

- `deposit(shieldedRecipient)` locks ETH and emits `DepositLocked(nonce,
  shieldedRecipient, from, amount)`. Prime Chain watches this event and mints
  a shielded note to `shieldedRecipient`.
- `withdraw(recipient, amount, leafNonce, merkleProof)` releases ETH for an
  unshield authorized on Prime Chain. The withdrawal leaf is
  `keccak256(abi.encode(recipient, amount, leafNonce))` and must be included
  in the latest proven `shieldedStateRoot` via a sorted-pair keccak Merkle
  tree. Each leaf can be spent once (`withdrawalSpent`).

  **Integration contract:** Prime Chain's unshield-authorization Merkle tree
  must use the same leaf encoding and sorted-pair keccak hashing. Finalizing
  that chain-side tree (and committing its root into the proven public inputs
  if a dedicated withdrawal root is preferred over `shieldedStateRoot`) is the
  remaining chain-side integration step.

## Audit notes (G4)

- Reentrancy: `withdraw` is `nonReentrant` and performs state changes before
  the external ETH transfer.
- Replay: state proofs are monotonic + continuous; withdrawals are nullified
  per leaf.
- Field bounds: `Groth16Verifier` rejects public inputs `>= R` (BN254 scalar
  field modulus) and negates A in-field for the pairing check.
- Trust: the verifying key is owner-set once then locked; ownership should be
  transferred to governance / a timelock before mainnet.
- Known limitation: the withdrawal Merkle scheme is the integration contract
  above; the bridge is only as sound as the chain-side authorization tree it
  is pointed at.
