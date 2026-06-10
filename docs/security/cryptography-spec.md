# Mersennet — Cryptography Specification

**Status:** Draft for third-party audit (Workstream I)
**Target chain ID:** 7920 (privacy testnet) → 7919 (mainnet, post-fork)
**Date:** 2026-05-21

This document formally specifies the cryptographic primitives,
protocols, and parameter choices used by Mersennet's privacy
hard-fork. It is the source of truth for the auditor and the
implementation in `crates/zkp/`, `crates/core/src/shielded_*`, and
`crates/core/src/dkg.rs`.

---

## 1. Notation

- `r` = BN254 scalar field modulus
  `21888242871839275222246405745257275088548364400416034343698204186575808495617`
- `q` = BN254 base field modulus (used internally by the curve only)
- `p` = BLS12-381 scalar field modulus
  `52435875175126190479447740508185965837690552500527637822603658699938581184513`
- `Fr_bn` = `GF(r)` — BN254 scalar field
- `Fr_bls` = `GF(p)` — BLS12-381 scalar field
- `G1_bn / G2_bn` = BN254 G1 / G2
- `G1_bls / G2_bls` = BLS12-381 G1 / G2
- `Hₚ` = Poseidon-2 over BN254 (width 3, capacity 1, rate 2, 8 full
  rounds + 56 partial rounds, S-box `x^5`)
- `Hₖ` = Keccak256 (used only for domain separation, never for
  in-circuit hashing)

## 2. Roles

| Role | What it does | Key material |
|---|---|---|
| User wallet | Builds shielded orders, transfers, claims | viewing key `sk_view`, spending key `sk_spend`, BLS pseudonym `pk_psd` |
| Relayer | Submits encrypted intents to mempool | none (anonymous transport) |
| Validator | Decrypts threshold mempool, runs FBA, signs blocks | DKG share `sk_i`, consensus key |
| Combiner | (Logical role — runs inside the block-producer) | none |
| Auditor | Receives a delegated view token | derived `ViewKey` |

## 3. Primitives

### 3.1 Poseidon-2 over BN254 (in-circuit)

**Reference implementation:** Aztec Noir's `std::hash::poseidon2`.

**Rust mirror:** `crates/zkp/src/poseidon.rs`. Round constants + MDS
matrix are pinned in `crates/zkp/params/poseidon-bn254.bin`. Format:

```
byte 0       : magic 0xAE
byte 1       : version 0x01
byte 2       : width                  (= 3)
byte 3       : full_rounds            (= 8)
bytes 4..5   : partial_rounds (LE u16) (= 56)
bytes 6..N   : (full+partial)*width  × 32-byte LE Fr round constants
bytes N..end : width*width            × 32-byte LE Fr MDS matrix entries
```

**Audit checks:**

1. The pinned constants must match Aztec's published Poseidon-2 set
   exactly. Cross-check against `noir-protocol-circuits` upstream.
2. `permute()` ordering must be: half-full → partial → half-full
   with S-box on full rounds applied to all elements, on partial
   rounds applied to `state[0]` only.
3. `hash_two(a, b)` = `permute([0, a, b])[0]` (capacity element is
   `state[0]`).

### 3.2 Pedersen commitment over BN254

**Definition:** `Commit(m₀, …, m_{k-1}; r) = Σᵢ mᵢ·Gᵢ + r·H` where
`G₀…G_{k-1}, H ∈ G1_bn` are independent generators.

**Generator derivation:** Try-and-increment hash-to-curve.

```
G_i = derive("PrimeChain-Pedersen-v0-G", i)
H   = derive("PrimeChain-Pedersen-v0-H", 0)

derive(domain, index):
  ctr = 0
  loop:
    digest = Keccak256(domain || index_LE || ctr_LE)
    x = Fq.from_le_bytes_mod_order(digest)
    y² = x³ + 3
    if y² is QR in Fq:
      y = sqrt(y²)
      y_chosen = min(y, -y)  // lex-smaller representative
      point = G1Affine(x, y_chosen)
      if point in correct subgroup:
        return point
    ctr += 1
```

**MAX_MSG_LEN = 16** — the generator table allocates 16 slots; that
covers all current note layouts (value, asset_id, owner_pk, rho,
psi, ... at most 5–6 fields).

**Audit checks:**

1. Generators must be in the prime-order subgroup.
2. Generators must be independent (no `G_i = G_j` for `i ≠ j`).
3. `Commit` reduces to a `G1_bn` element, but the public API
   surface returns an `Fr_bn` via Poseidon over the 31-byte chunks
   of the compressed-point serialization. The full G1 element is
   the binding commitment; the `Fr` hash is for in-circuit fan-in.

### 3.3 BLS12-381 threshold ElGamal

Used for the encrypted mempool. KEM/DEM construction:

```
sk        ∈ Fr_bls            (the global secret, never reconstructed)
pk = sk·G ∈ G1_bls            (the global public key)

Encrypt(m, pk):
  sample r  ∈ Fr_bls uniformly
  c₁ = r·G              ∈ G1_bls    (ephemeral)
  shared = r·pk         ∈ G1_bls
  K = Keccak256("PrimeChain-BLSThreshold-v0-KDF" || compress(shared) || epoch_LE)
  nonce ∈ {0,1}^96 fresh
  body  = ChaCha20Poly1305.encrypt(K, nonce, m)
  return concat(compress(c₁), nonce, body)
```

**Threshold decryption.** Validator `i` with secret share `skᵢ`
computes `shareᵢ = c₁·skᵢ ∈ G1_bls`. The combiner accumulates `t`
such shares (where `t = threshold`) and Lagrange-interpolates at
`x = 0`:

```
recovered = Σ_{i ∈ S} λ_i(0) · shareᵢ
λ_i(0) = ∏_{j ∈ S\{i}} (-j) / (i - j)
```

This recovers `c₁·sk = pk·r = shared`. The combiner re-derives `K`
and runs AEAD-decrypt. **A wrong share fails AEAD authenticity**,
detectable in constant time.

**Layout of ciphertext bytes:**

```
bytes 0..48   : c₁ (compressed G1_bls, 48 bytes)
bytes 48..60  : ChaCha20-Poly1305 nonce (12 bytes)
bytes 60..end : AEAD body (plaintext + 16-byte tag)
```

**Reference implementation:** `crates/zkp/src/bls_threshold.rs`
(gated behind `prover` feature). Group ops via `blstrs = "0.7"` +
`group = "0.13"`; AEAD via `chacha20poly1305 = "0.10"`.

**Audit checks:**

1. `c₁` must be in the prime-order subgroup; reject otherwise.
2. Shares must be compressed-G1 of exactly 48 bytes; the size guard
   is in `submit_share`.
3. `KDF` separator must include `"PrimeChain-BLSThreshold-v0-KDF"`
   to prevent cross-protocol key reuse.
4. Lagrange basis evaluation uses `Scalar::invert` from `blstrs`,
   which is constant-time; reject if `i == j` (would divide by
   zero, but `S` is dedup'd by construction).

### 3.4 Distributed Key Generation (Pedersen-DKG)

**Goal:** Produce the global `(sk, pk)` of §3.3 without any single
party ever holding `sk`.

**Setup:** `n` validators, threshold `t`. The privacy testnet runs
`t = 5, n = 7`. Mainnet target: same (DKG-Pedersen with abort on
< t commitments).

**Round 1 — Commit.** Each validator `Pᵢ`:

1. Picks a random degree-(t-1) polynomial `fᵢ(x) = aᵢ₀ + aᵢ₁·x + …
   + aᵢ_{t-1}·x^{t-1}` over `Fr_bls`.
2. Broadcasts the Pedersen commitments
   `Cᵢⱼ = aᵢⱼ·G + bᵢⱼ·H` for `j = 0…t-1` where `bᵢⱼ` is fresh
   randomness and `H` is an independent generator
   `H = HashToCurveG1("PrimeChain-DKG-H-v0")`.

**Round 2 — Distribute.** Each `Pᵢ` sends to every `Pⱼ` (j ≠ i):

```
sᵢⱼ = fᵢ(j)           // share value
tᵢⱼ = gᵢ(j)           // blinding factor share, gᵢ is the b-polynomial
```

The share envelope is encrypted under `Pⱼ`'s long-term node key
using ECIES so observers can audit the transcript but not the
plaintext.

**Round 3 — Verify.** `Pⱼ` checks for every `i`:

```
sᵢⱼ·G + tᵢⱼ·H == Σ_{k=0}^{t-1} (j^k)·Cᵢ_k
```

If the check fails for `(i, j)`, `Pⱼ` broadcasts a
`SlashingComplaint{ complainant = j, accused = i, evidence = (sᵢⱼ, tᵢⱼ, ECIES_proof) }`.

**Round 4 — Aggregate.** Let `Qᵢ` be the set of validators that
successfully passed Round 3 for `Pᵢ`. `Pᵢ` is in the **qualified
set** `QUAL` iff `|Qᵢ| ≥ 2t - n`.

The group keys are:

```
pk = Σ_{i ∈ QUAL} aᵢ₀·G        ≡ Σ_{i ∈ QUAL} Cᵢ₀ projected to the G-component
sk = Σ_{i ∈ QUAL} aᵢ₀          (never reconstructed)
skⱼ = Σ_{i ∈ QUAL} sᵢⱼ        (each validator's share of sk)
```

**Audit checks (DKG):**

1. The polynomial degree `t-1` matches the reconstruction threshold.
2. The blinding generator `H` must be **independent** of `G` —
   no known scalar `δ` with `H = δ·G`. We derive `H` via
   try-and-increment hash-to-curve with a fixed domain separator,
   identical to the Pedersen-commitment generator setup.
3. The Round 3 check is the standard Pedersen-VSS verification.
4. Round 4's aggregation is over `QUAL` only — a validator who
   fails Round 3 cannot poison the group key.
5. The complaint mechanism is binding: presenting an invalid
   complaint is itself slashable (the complainant attaches a NIZK
   of "I decrypted the share with my node key and got `(sᵢⱼ,
   tᵢⱼ)`", and a verifier checks both the decryption proof and
   the Pedersen-VSS failure).

**Implementation status (testnet):**

- `crates/core/src/dkg.rs::DkgCoordinator` tracks the ceremony
  state, records commitments + shares + complaints, and finalizes
  via `try_finalize`.
- Round 1 broadcasts: HotStuff-2 sub-round inputs.
- Round 2 share encryption: ECIES under validator node key
  (`crates/core/src/identity.rs::NodeKey`).
- Round 3 verification: deferred until BLS Pedersen-VSS arithmetic
  lands in `crates/zkp/src/bls_threshold.rs::dkg_verify_share`
  (post-bake).
- Round 4 aggregation: deferred until the same module lands.

Until then, the testnet uses `BlsThreshold::new_with_simulated_dkg`
(secrets generated centrally for each genesis seed) so the
remainder of the privacy stack can be exercised end-to-end. The
audit specifically calls out the difference between "DKG simulated"
and "DKG ceremony" modes in `crates/core/src/dkg.rs` doc comments.

### 3.5 Frequent batch auction (FBA) matching

Each block, for each shielded market:

1. Decrypt the threshold-mempool ciphertexts that target this
   market.
2. Sort buys descending by price, sells ascending.
3. Find the clearing price `p*` such that cumulative buy volume at
   `≥p*` equals cumulative sell volume at `≤p*` (uniform-price
   single-call auction).
4. Match at `p*`, emit `FbaCleared` event with
   `(market_id, clearing_price, matched_size, intent_count)`.
5. For each fill, insert the new note commitment into the shielded
   tree.

**Privacy property:** The event reveals market-level aggregates
only; per-fill values, traders, and unmatched intents are private.

### 3.6 Sealed-bid liquidation auctions

When a position becomes liquidatable:

1. Any registered liquidator submits a `LiquidationClaim` with the
   ZK proof `Circuit::LiquidateClaim` that proves "there exists
   some note `c` in the tree whose maintenance equity is negative
   at oracle price P". The proof emits a `claim_tag = Poseidon(c,
   P)` so the auction can deduplicate.
2. The claim carries a threshold-encrypted bid (§3.3).
3. At end of block, the threshold decryption recovers all bids for
   the same `claim_tag`. The highest bid wins; ties broken by
   liquidator-id lex order.
4. Public output: `LiquidationSettled` event with
   `(market_id, winner_bond_commitment, winning_bid)`. The victim's
   identity is never revealed.

### 3.7 Migration (genesis-of-privacy)

At activation height `H`, every transparent EOA `A` with positive
balance `B` is migrated by deriving:

```
rho_A   = Hₖ("PrimeChain-MigrationRho" || A || H)
psi_A   = Hₖ("PrimeChain-MigrationPsi" || A || H)
owner_pk_A = Hₚ(Keccak256(A))         (folded into Fr_bn)
note_A  = Note{ value = B, asset_id = 0, owner_pk = owner_pk_A, rho = rho_A, psi = psi_A }
c_A     = Commit(note_A)
```

The owner can derive `rho_A`, `psi_A`, and `owner_pk_A`
deterministically from the EOA + activation height (no claim
needed) and `Commit` recovers `c_A` from the public migrated note
commitment table.

**Audit checks (migration):**

1. Sum of migrated note values equals sum of pre-fork transparent
   balances. Conservation is verified by the SP1 program that
   produces the activation block's state-transition proof.
2. The derivation is **deterministic** so a hard fork reorg
   produces the same notes.
3. Owners who lose their EOA private key before migration cannot
   recover the note (this is fundamental — pre-fork the EOA was
   already the binding key).

## 4. Domain separators

Every Keccak / Poseidon invocation uses a domain-separated input:

```
"PrimeChain-Poseidon-v0"           : fallback round-constant synthesis (D1)
"PrimeChain-Pedersen-v0-G"         : Pedersen generators (D2)
"PrimeChain-Pedersen-v0-H"         : Pedersen blinding generator (D2)
"PrimeChain-BLSThreshold-v0-KDF"   : threshold ElGamal symmetric key derivation (D3)
"PrimeChain-DKG-H-v0"              : DKG blinding generator (D4)
"PrimeChain-DKG-AggPk-v0"          : DKG aggregated-pk fingerprint (default lane only)
"PrimeChain-DummyThreshold"        : test-only XOR mask (DummyThreshold)
"PrimeChain-MigrationRho"          : migration rho derivation
"PrimeChain-MigrationPsi"          : migration psi derivation
```

**Audit check:** No domain separator may be reused across protocols
or versions. Bumping a separator (e.g. `-v0` → `-v1`) is a hard
fork.

## 5. Out-of-scope (for this document, listed in plan)

- **SP1 program body** — Workstream E. Re-execution proof of the
  block.
- **Barretenberg verifier** — Workstream D5. The in-Rust
  `MockVerifier` is the audit target until then.
- **Noir circuit pinning** — Workstream D6. `crates/zkp/circuits/`
  contains stub circuits today.
- **Solidity bridge** — Workstream G. Mirrors §3.7 on Ethereum.

## 6. Audit-traceability index

| Section | Code location | Test location |
|---|---|---|
| 3.1 Poseidon | `crates/zkp/src/poseidon.rs` | `poseidon::tests::*` |
| 3.2 Pedersen | `crates/zkp/src/pedersen.rs` | `pedersen::tests::*` (`prover` feature) |
| 3.3 BLS threshold | `crates/zkp/src/bls_threshold.rs` | `bls_threshold::tests::*` (`prover` feature) |
| 3.4 DKG | `crates/core/src/dkg.rs` | `dkg::tests::*` |
| 3.5 FBA | `crates/core/src/shielded_orders.rs` | `shielded_orders::tests::*` |
| 3.6 Liquidation auction | `crates/core/src/liquidation_auction.rs` | `liquidation_auction::tests::*` |
| 3.7 Migration | `crates/core/src/shielded_evm.rs::derive_migration_*` | `shielded_evm::tests::migrate_*` |

## 7. Version + change log

| Version | Date | Change |
|---|---|---|
| 0.1 | 2026-05-21 | Initial draft for testnet bring-up. |
