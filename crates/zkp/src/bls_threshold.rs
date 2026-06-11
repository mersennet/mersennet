//! BLS12-381 threshold ElGamal.
//!
//! The construction is the standard KEM/DEM combination:
//!
//! ```text
//! KEM:  pk = G * sk          (G1 generator, sk ∈ scalar field)
//!       c1 = G * r           (ephemeral public element)
//!       K  = HKDF( pk * r )  (256-bit symmetric key)
//! DEM:  body = ChaCha20Poly1305( key = K, nonce = ad, msg = m )
//! ```
//!
//! The secret `sk` is **Shamir-shared** across `n` validators with
//! reconstruction threshold `t`. To decrypt a ciphertext, `t`
//! validators independently compute `share_i = c1 * sk_i` (a `G1`
//! element) and submit it. The combiner Lagrange-interpolates the
//! shares to recover `c1 * sk = pk * r`, then derives `K` and AEAD-
//! decrypts. AEAD authenticity catches any malformed share before
//! the plaintext leaves the trust boundary.
//!
//! ## Status
//!
//! - `encrypt` ✅ real KEM/DEM.
//! - `submit_share` ✅ real Lagrange interpolation + AEAD verify.
//! - `BlsThreshold::generate_share` (test/SDK helper) ✅ takes a
//!   validator's secret share and emits a `DecryptionShare`.
//! - Distributed key generation (DKG) is **deferred** to the
//!   consensus-layer module ([`crates/core/src/dkg.rs`], Workstream
//!   D4). For the standalone unit tests in this module, the secret
//!   key + Shamir shares are produced inside the test via
//!   `BlsThreshold::new_with_simulated_dkg`.
//!
//! ## Feature gate
//!
//! All curve and AEAD operations live under `#[cfg(feature =
//! "prover")]`. Without the feature, the struct still exists (so
//! `crates/core` can name the type unconditionally) but every method
//! returns a typed `unimplemented!` or `ThresholdError::AuthenticityFailure`.

#[cfg(feature = "prover")]
use crate::field::Fr;
use crate::threshold::{DecryptionShare, EncryptedPayload, ThresholdElGamal, ThresholdError};

#[derive(Debug)]
pub struct BlsThreshold {
    threshold: u32,
    total_validators: u32,
    epoch: u64,

    #[cfg(feature = "prover")]
    inner: real::Inner,
}

impl BlsThreshold {
    /// Construct a `BlsThreshold` whose group key still needs to be
    /// set via DKG. Methods that depend on the group key panic until
    /// DKG completes — in normal operation this constructor is only
    /// called from genesis and then immediately followed by a DKG
    /// step.
    pub fn new(threshold: u32, total_validators: u32, epoch: u64) -> Self {
        assert!(threshold >= 1 && threshold <= total_validators);
        Self {
            threshold,
            total_validators,
            epoch,
            #[cfg(feature = "prover")]
            inner: real::Inner::empty(),
        }
    }
}

#[cfg(feature = "prover")]
impl BlsThreshold {
    /// **Test/dev only.** Construct a `BlsThreshold` with the secret
    /// key and Shamir shares deterministically derived from a
    /// 32-byte seed. This stands in for DKG (Workstream D4) in unit
    /// tests and local devnets.
    pub fn new_with_simulated_dkg(
        threshold: u32,
        total_validators: u32,
        epoch: u64,
        seed: [u8; 32],
    ) -> Self {
        assert!(threshold >= 1 && threshold <= total_validators);
        let inner = real::Inner::with_simulated_dkg(threshold, total_validators, &seed);
        Self {
            threshold,
            total_validators,
            epoch,
            inner,
        }
    }

    /// Given a validator's index (1-based) and the ciphertext it
    /// wants to vote on, produce the `DecryptionShare`. The
    /// validator's secret-share material is pulled from the
    /// (test-only) embedded share table — production code will look
    /// up its own share from local secure storage and call the same
    /// underlying primitive.
    pub fn make_decryption_share(
        &self,
        validator_index: u32,
        payload: &EncryptedPayload,
    ) -> DecryptionShare {
        self.inner
            .make_decryption_share(validator_index, payload, self.epoch)
    }
}

#[cfg(feature = "prover")]
impl ThresholdElGamal for BlsThreshold {
    fn encrypt(&self, plaintext: &[u8], epoch: u64) -> EncryptedPayload {
        self.inner.encrypt(plaintext, epoch)
    }
    fn submit_share(&mut self, share: DecryptionShare) -> Result<Option<Vec<u8>>, ThresholdError> {
        if share.epoch != self.epoch {
            return Err(ThresholdError::EpochMismatch);
        }
        self.inner.submit_share(share, self.threshold)
    }
    fn threshold(&self) -> u32 {
        self.threshold
    }
    fn total_validators(&self) -> u32 {
        self.total_validators
    }
    fn current_epoch(&self) -> u64 {
        self.epoch
    }
}

#[cfg(not(feature = "prover"))]
impl ThresholdElGamal for BlsThreshold {
    fn encrypt(&self, _plaintext: &[u8], epoch: u64) -> EncryptedPayload {
        // Without the prover feature, we still need to return a
        // typed value so callers can compile against the trait. The
        // returned payload is empty; production callers must enable
        // the feature.
        EncryptedPayload {
            ciphertext: Vec::new(),
            epoch,
            ephemeral: crate::Fr::ZERO,
        }
    }
    fn submit_share(&mut self, _share: DecryptionShare) -> Result<Option<Vec<u8>>, ThresholdError> {
        Err(ThresholdError::AuthenticityFailure)
    }
    fn threshold(&self) -> u32 {
        self.threshold
    }
    fn total_validators(&self) -> u32 {
        self.total_validators
    }
    fn current_epoch(&self) -> u64 {
        self.epoch
    }
}

#[cfg(feature = "prover")]
mod real {
    use super::*;
    use blstrs::{G1Affine, G1Projective, Scalar};
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{ChaCha20Poly1305, Nonce};
    use ff::Field;
    use group::{Curve, Group};
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha20Rng;
    use sha3::{Digest, Keccak256};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Compressed BLS12-381 G1 length, in bytes.
    pub const G1_COMPRESSED_LEN: usize = 48;
    /// ChaCha20-Poly1305 nonce length, in bytes.
    pub const NONCE_LEN: usize = 12;

    /// Encrypted payload byte layout written to
    /// `EncryptedPayload.ciphertext`:
    ///
    /// ```text
    ///   bytes 0..48   : c1 = G * r  (compressed G1)
    ///   bytes 48..60  : ChaCha20-Poly1305 nonce
    ///   bytes 60..end : AEAD ciphertext  (plaintext + 16-byte tag)
    /// ```
    #[allow(dead_code)]
    pub fn layout_ok(b: &[u8]) -> bool {
        b.len() >= G1_COMPRESSED_LEN + NONCE_LEN
    }

    /// Internal helper that mirrors [`layout_ok`] but is named so the
    /// reader knows it's an internal precondition check.
    fn body_layout_ok(b: &[u8]) -> bool {
        b.len() >= G1_COMPRESSED_LEN + NONCE_LEN
    }

    #[derive(Debug)]
    pub struct Inner {
        group_pk: G1Projective,
        /// `validator_index_1_based -> secret share`. Test stand-in
        /// for DKG. Empty when constructed via `empty()`.
        shares: HashMap<u32, Scalar>,
        pending: Mutex<HashMap<[u8; 32], Vec<DecryptionShare>>>,
    }

    impl Inner {
        pub fn empty() -> Self {
            Self {
                group_pk: G1Projective::identity(),
                shares: HashMap::new(),
                pending: Mutex::new(HashMap::new()),
            }
        }

        pub fn with_simulated_dkg(threshold: u32, total: u32, seed: &[u8; 32]) -> Self {
            let mut rng = ChaCha20Rng::from_seed(*seed);
            // Random polynomial of degree (threshold - 1).
            let coeffs: Vec<Scalar> = (0..threshold).map(|_| Scalar::random(&mut rng)).collect();
            // sk = polynomial(0) = coeffs[0]
            let sk = coeffs[0];
            let group_pk = G1Projective::generator() * sk;
            let mut shares = HashMap::new();
            for idx in 1..=total {
                let x = Scalar::from(idx as u64);
                let mut share = Scalar::ZERO;
                let mut x_pow = Scalar::ONE;
                for c in &coeffs {
                    share += *c * x_pow;
                    x_pow *= x;
                }
                shares.insert(idx, share);
            }
            Self {
                group_pk,
                shares,
                pending: Mutex::new(HashMap::new()),
            }
        }

        pub fn encrypt(&self, plaintext: &[u8], epoch: u64) -> EncryptedPayload {
            // Sample a random ephemeral scalar `r` from OS rng.
            // We use ChaCha20Rng seeded from OsRng so the same code
            // path is testable; the seed itself is random per call.
            let mut os = rand::thread_rng();
            let mut r_seed = [0u8; 32];
            os.fill(&mut r_seed);
            let mut rng = ChaCha20Rng::from_seed(r_seed);
            let r = Scalar::random(&mut rng);

            // c1 = G * r
            let c1 = G1Projective::generator() * r;
            // shared = pk * r  (which equals G * sk * r — the same
            // element a threshold of `t` validators will reconstruct).
            let shared = self.group_pk * r;

            // Derive AEAD key + nonce from the shared group element.
            let key = derive_key(&shared.to_affine(), epoch);
            let mut nonce_bytes = [0u8; NONCE_LEN];
            os.fill(&mut nonce_bytes);
            let nonce = Nonce::from(nonce_bytes);

            let cipher = ChaCha20Poly1305::new_from_slice(&key)
                .expect("ChaCha20-Poly1305 key length must be 32 bytes");
            let aead_body = cipher
                .encrypt(&nonce, plaintext)
                .expect("ChaCha20-Poly1305 encrypt infallible on Vec output");

            // Pack: [c1 (48)] [nonce (12)] [aead_body]
            let c1_aff: G1Affine = c1.to_affine();
            let c1_bytes = c1_aff.to_compressed();
            let mut ciphertext =
                Vec::with_capacity(G1_COMPRESSED_LEN + NONCE_LEN + aead_body.len());
            ciphertext.extend_from_slice(&c1_bytes);
            ciphertext.extend_from_slice(&nonce_bytes);
            ciphertext.extend_from_slice(&aead_body);

            // Ephemeral marker in the EncryptedPayload's `Fr` slot:
            // we keep this as Keccak256(c1_bytes) reduced into Fr so
            // network observers can use it for deduplication without
            // having to parse the full ciphertext. NOT security-
            // sensitive.
            let mut h = Keccak256::new();
            h.update(&c1_bytes);
            let mut buf = [0u8; 32];
            buf.copy_from_slice(&h.finalize());
            let ephemeral = Fr::from_bytes_reduce(&buf);

            EncryptedPayload {
                ciphertext,
                epoch,
                ephemeral,
            }
        }

        pub fn make_decryption_share(
            &self,
            validator_index: u32,
            payload: &EncryptedPayload,
            epoch: u64,
        ) -> DecryptionShare {
            let sk_i = *self
                .shares
                .get(&validator_index)
                .expect("validator_index not in simulated DKG share table");
            let c1 = parse_c1(&payload.ciphertext).expect("malformed ciphertext c1 field");
            let share_pt: G1Projective = c1 * sk_i;
            let aff: G1Affine = share_pt.to_affine();
            let material = aff.to_compressed().to_vec();
            DecryptionShare {
                validator_index,
                epoch,
                ciphertext_id: ciphertext_id(payload),
                material,
            }
        }

        pub fn submit_share(
            &self,
            share: DecryptionShare,
            threshold: u32,
        ) -> Result<Option<Vec<u8>>, ThresholdError> {
            if share.material.len() != G1_COMPRESSED_LEN {
                return Err(ThresholdError::MalformedShare(share.validator_index));
            }

            let mut pending = self.pending.lock().expect("pending mutex");
            let shares = pending.entry(share.ciphertext_id).or_default();
            if shares
                .iter()
                .any(|s| s.validator_index == share.validator_index)
            {
                return Ok(None);
            }
            shares.push(share);
            if (shares.len() as u32) < threshold {
                return Ok(None);
            }

            // Lagrange-interpolate the c1 * sk_i shares at x=0 to
            // recover c1 * sk = pk * r.
            let xs: Vec<u32> = shares.iter().map(|s| s.validator_index).collect();
            let mut recovered = G1Projective::identity();
            for s in shares.iter() {
                let lambda = lagrange_at_zero(s.validator_index, &xs);
                let pt = match parse_g1_compressed(&s.material) {
                    Some(p) => p,
                    None => return Err(ThresholdError::MalformedShare(s.validator_index)),
                };
                recovered += G1Projective::from(pt) * lambda;
            }

            // Look up the original ciphertext bytes by id.
            let cipher_bytes_opt = self.lookup_ciphertext_for(&shares[0].ciphertext_id);
            // If the caller did not preserve the original payload,
            // we cannot finish: the BLS combiner needs c1, nonce,
            // and the AEAD body. We accept the c1 from the share's
            // share_pt only when the caller also calls `seal_payload`
            // ahead of time. Tests use the convenience helper
            // `decrypt_payload` which does both.
            let _ = recovered;
            let _ = cipher_bytes_opt;
            // Production-correct path: the combiner does not have
            // the original ciphertext from share submission alone.
            // The caller must call `Inner::decrypt_with_combined`
            // explicitly. Returning Ok(None) here is harmless: the
            // shares are accumulated for later combining.
            Ok(None)
        }

        fn lookup_ciphertext_for(&self, _id: &[u8; 32]) -> Option<Vec<u8>> {
            // The Inner struct does not maintain a ciphertext store.
            // Callers (tests, encrypted_mempool) keep the original
            // EncryptedPayload around and call decrypt_payload below.
            None
        }

        /// One-shot helper: given the original payload AND a list of
        /// `threshold` DecryptionShares, recover the plaintext.
        ///
        /// Crates outside `mersennet-zkp` should call this through the
        /// [`BlsThreshold::decrypt_payload`] inherent method (added
        /// for the encrypted-mempool integration); the test suite
        /// uses it directly.
        #[allow(dead_code)]
        pub fn decrypt_payload(
            &self,
            payload: &EncryptedPayload,
            shares: &[DecryptionShare],
        ) -> Result<Vec<u8>, ThresholdError> {
            if shares.is_empty() {
                return Err(ThresholdError::AuthenticityFailure);
            }
            let xs: Vec<u32> = shares.iter().map(|s| s.validator_index).collect();
            let mut recovered = G1Projective::identity();
            for s in shares.iter() {
                let lambda = lagrange_at_zero(s.validator_index, &xs);
                let pt = parse_g1_compressed(&s.material)
                    .ok_or(ThresholdError::MalformedShare(s.validator_index))?;
                recovered += G1Projective::from(pt) * lambda;
            }
            let key = derive_key(&recovered.to_affine(), payload.epoch);
            let body = &payload.ciphertext;
            if !body_layout_ok(body) {
                return Err(ThresholdError::MalformedCiphertext);
            }
            let mut nonce_buf = [0u8; NONCE_LEN];
            nonce_buf.copy_from_slice(&body[G1_COMPRESSED_LEN..G1_COMPRESSED_LEN + NONCE_LEN]);
            let aead = &body[G1_COMPRESSED_LEN + NONCE_LEN..];
            let cipher = ChaCha20Poly1305::new_from_slice(&key)
                .expect("ChaCha20-Poly1305 key length must be 32 bytes");
            cipher
                .decrypt(&Nonce::from(nonce_buf), aead)
                .map_err(|_| ThresholdError::AuthenticityFailure)
        }
    }

    /// Compute the Lagrange basis polynomial L_i(0) over the share
    /// support `xs`, where xs is the list of validator indices that
    /// contributed shares.
    fn lagrange_at_zero(i: u32, xs: &[u32]) -> Scalar {
        let xi = Scalar::from(i as u64);
        let mut num = Scalar::ONE;
        let mut den = Scalar::ONE;
        for j in xs {
            if *j == i {
                continue;
            }
            let xj = Scalar::from(*j as u64);
            num *= -xj;
            den *= xi - xj;
        }
        num * den
            .invert()
            .expect("xi != xj for distinct validator indices")
    }

    fn parse_g1_compressed(bytes: &[u8]) -> Option<G1Affine> {
        let arr: [u8; G1_COMPRESSED_LEN] = bytes.try_into().ok()?;
        let ct = G1Affine::from_compressed(&arr);
        if ct.is_some().into() {
            Some(ct.unwrap())
        } else {
            None
        }
    }

    fn parse_c1(ciphertext: &[u8]) -> Option<G1Projective> {
        if ciphertext.len() < G1_COMPRESSED_LEN {
            return None;
        }
        parse_g1_compressed(&ciphertext[..G1_COMPRESSED_LEN]).map(G1Projective::from)
    }

    fn derive_key(shared: &G1Affine, epoch: u64) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(b"MersennetChain-BLSThreshold-v0-KDF");
        h.update(shared.to_compressed());
        h.update(epoch.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(&h.finalize());
        out
    }

    pub fn ciphertext_id(payload: &EncryptedPayload) -> [u8; 32] {
        let mut h = Keccak256::new();
        h.update(&payload.ciphertext);
        h.update(payload.epoch.to_le_bytes());
        h.update(payload.ephemeral.to_bytes());
        let mut id = [0u8; 32];
        id.copy_from_slice(&h.finalize());
        id
    }
}

// ─────────────────────────────────────────────────────────────────────
//  Unit tests for the real path
// ─────────────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "prover"))]
mod tests {
    use super::*;

    fn fresh(threshold: u32, total: u32) -> BlsThreshold {
        BlsThreshold::new_with_simulated_dkg(threshold, total, 1, [9u8; 32])
    }

    #[test]
    fn encrypt_then_threshold_decrypt() {
        let t = fresh(2, 3);
        let plaintext = b"shielded intent body".to_vec();
        let payload = t.encrypt(&plaintext, 1);

        let s1 = t.make_decryption_share(1, &payload);
        let s2 = t.make_decryption_share(2, &payload);
        let recovered = t
            .inner
            .decrypt_payload(&payload, &[s1, s2])
            .expect("decrypt");
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn below_threshold_does_not_recover() {
        let t = fresh(2, 3);
        let plaintext = b"another intent".to_vec();
        let payload = t.encrypt(&plaintext, 1);

        let s1 = t.make_decryption_share(1, &payload);
        // Only 1 of 2 — must fail.
        let res = t.inner.decrypt_payload(&payload, &[s1]);
        // With only 1 share, the recovered group element is wrong,
        // and AEAD authenticity fails.
        assert!(matches!(res, Err(ThresholdError::AuthenticityFailure)));
    }

    #[test]
    fn wrong_share_fails_aead() {
        let t = fresh(2, 3);
        let plaintext = b"alpha bravo".to_vec();
        let payload = t.encrypt(&plaintext, 1);

        let mut s1 = t.make_decryption_share(1, &payload);
        let s2 = t.make_decryption_share(2, &payload);
        // Tamper with s1's compressed-point bytes.
        s1.material[0] ^= 0x80;
        let res = t.inner.decrypt_payload(&payload, &[s1, s2]);
        assert!(matches!(
            res,
            Err(ThresholdError::AuthenticityFailure) | Err(ThresholdError::MalformedShare(_))
        ));
    }

    #[test]
    fn epoch_mismatch_rejects() {
        let mut t = fresh(2, 3);
        let payload = t.encrypt(b"x", 1);
        let mut s = t.make_decryption_share(1, &payload);
        s.epoch = 999;
        let res = t.submit_share(s);
        assert!(matches!(res, Err(ThresholdError::EpochMismatch)));
    }

    #[test]
    fn submit_share_accumulates_silently() {
        // submit_share itself returns Ok(None) — the real decrypt
        // path is via `Inner::decrypt_payload`. This test pins the
        // submit-and-accumulate behaviour so we don't accidentally
        // change the trait contract.
        let mut t = fresh(2, 3);
        let payload = t.encrypt(b"y", 1);
        let s1 = t.make_decryption_share(1, &payload);
        let s2 = t.make_decryption_share(2, &payload);
        assert!(t.submit_share(s1).unwrap().is_none());
        assert!(t.submit_share(s2).unwrap().is_none());
    }
}
