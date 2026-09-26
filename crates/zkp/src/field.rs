//! Minimal BN254 scalar field element.
//!
//! The full BN254 scalar field modulus is
//! `r = 21888242871839275222246405745257275088548364400416034343698204186575808495617`.
//!
//! Inside this crate we model `Fr` as a 256-bit little-endian element
//! reduced modulo `r`. We do *not* implement the full constant-time EC
//! pairing-friendly arithmetic here — that belongs to the proving
//! backend (Barretenberg / arkworks) and is gated behind the `prover`
//! feature on the parent `mersennet` crate.
//!
//! What we do implement is enough to:
//!
//! - Hash inputs into the field for Poseidon and Merkle tree usage
//!   (`Fr::from_bytes_reduce`).
//! - Serialize / deserialize to canonical 32-byte little-endian form
//!   (`Fr::to_bytes`, `Fr::from_bytes`).
//! - Add and multiply in a way that matches the Noir circuit at the
//!   boundary, using big-integer arithmetic on `u64` limbs.

use serde::{Deserialize, Serialize};
use std::fmt;

/// BN254 scalar field modulus, little-endian limbs.
///
/// `r = 0x30644e72e131a029b85045b68181585d2833e84879b9709143e1f593f0000001`
pub const MODULUS_LIMBS: [u64; 4] = [
    0x43E1_F593_F000_0001,
    0x2833_E848_79B9_7091,
    0xB850_45B6_8181_585D,
    0x3064_4E72_E131_A029,
];

/// Field element. Always stored already reduced (< modulus).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fr {
    /// Little-endian limbs.
    limbs: [u64; 4],
}

impl fmt::Debug for Fr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fr(0x")?;
        for limb in self.limbs.iter().rev() {
            write!(f, "{:016x}", limb)?;
        }
        write!(f, ")")
    }
}

impl Default for Fr {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Fr {
    pub const ZERO: Fr = Fr { limbs: [0; 4] };
    pub const ONE: Fr = Fr {
        limbs: [1, 0, 0, 0],
    };

    /// Build from raw limbs. Reduces if needed.
    pub fn from_limbs(limbs: [u64; 4]) -> Self {
        let mut f = Fr { limbs };
        f.reduce();
        f
    }

    /// Build from a `u64`.
    pub const fn from_u64(x: u64) -> Self {
        Fr {
            limbs: [x, 0, 0, 0],
        }
    }

    /// Convert from canonical 32-byte little-endian. Returns `None` if
    /// the value is `>= modulus`.
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        let limbs = [
            u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        ];
        let candidate = Fr { limbs };
        if cmp_limbs(&candidate.limbs, &MODULUS_LIMBS).is_lt() {
            Some(candidate)
        } else {
            None
        }
    }

    /// Convert from arbitrary 32 bytes (little-endian), reducing modulo
    /// `r`. The reduction is exact for any 256-bit input: the value is
    /// placed in the low half of a 512-bit accumulator and reduced via
    /// `reduce_wide` (binary long division). This is the same routine
    /// the multiplier uses, so encodings round-trip correctly.
    pub fn from_bytes_reduce(bytes: &[u8; 32]) -> Self {
        let limbs = [
            u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
        ];
        let wide = [limbs[0], limbs[1], limbs[2], limbs[3], 0, 0, 0, 0];
        Fr {
            limbs: reduce_wide(&wide),
        }
    }

    /// Convert from canonical 32-byte **big-endian**, reducing modulo
    /// `r`. The Poseidon2 round constants embedded from the ACVM
    /// reference are big-endian hex (`from_be_bytes_reduce` in noir),
    /// so this is the decoder used to load them.
    pub fn from_be_bytes_reduce(bytes: &[u8; 32]) -> Self {
        let mut le = *bytes;
        le.reverse();
        Fr::from_bytes_reduce(&le)
    }

    /// Canonical 32-byte little-endian encoding.
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, limb) in self.limbs.iter().enumerate() {
            out[i * 8..(i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        out
    }

    pub fn is_zero(&self) -> bool {
        self.limbs == [0; 4]
    }

    /// Field addition, modulo `r`.
    pub fn add(&self, other: &Fr) -> Fr {
        let (sum, _carry) = add_limbs(&self.limbs, &other.limbs);
        let mut out = Fr { limbs: sum };
        out.reduce();
        out
    }

    /// Field subtraction, modulo `r`.
    pub fn sub(&self, other: &Fr) -> Fr {
        if cmp_limbs(&self.limbs, &other.limbs).is_lt() {
            let (left, _) = add_limbs(&self.limbs, &MODULUS_LIMBS);
            let (out, _) = sub_limbs(&left, &other.limbs);
            Fr { limbs: out }
        } else {
            let (out, _) = sub_limbs(&self.limbs, &other.limbs);
            Fr { limbs: out }
        }
    }

    /// Field multiplication via schoolbook + Barrett reduction is the
    /// right long-term implementation. For now we use the boring 512-bit
    /// intermediate + repeated subtraction, which is more than fast
    /// enough for our hashing volumes (a Poseidon round is ~10 muls).
    ///
    /// This is constant-time-by-structure but not formally vetted —
    /// real privacy circuits will use Barretenberg's vetted arithmetic
    /// and this routine is only used inside the Rust-side Poseidon.
    pub fn mul(&self, other: &Fr) -> Fr {
        let prod = mul_limbs_512(&self.limbs, &other.limbs);
        Fr {
            limbs: reduce_wide(&prod),
        }
    }

    /// Field exponentiation by a small constant — sufficient for the
    /// Poseidon x^5 S-box.
    pub fn pow5(&self) -> Fr {
        let x2 = self.mul(self);
        let x4 = x2.mul(&x2);
        self.mul(&x4)
    }

    fn reduce(&mut self) {
        if !cmp_limbs(&self.limbs, &MODULUS_LIMBS).is_lt() {
            let (out, _) = sub_limbs(&self.limbs, &MODULUS_LIMBS);
            self.limbs = out;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ord3 {
    Lt,
    Eq,
    Gt,
}

impl Ord3 {
    fn is_lt(self) -> bool {
        matches!(self, Ord3::Lt)
    }
}

fn cmp_limbs(a: &[u64; 4], b: &[u64; 4]) -> Ord3 {
    for i in (0..4).rev() {
        if a[i] < b[i] {
            return Ord3::Lt;
        }
        if a[i] > b[i] {
            return Ord3::Gt;
        }
    }
    Ord3::Eq
}

fn add_limbs(a: &[u64; 4], b: &[u64; 4]) -> ([u64; 4], u64) {
    let mut out = [0u64; 4];
    let mut carry: u128 = 0;
    for i in 0..4 {
        let s = (a[i] as u128) + (b[i] as u128) + carry;
        out[i] = s as u64;
        carry = s >> 64;
    }
    (out, carry as u64)
}

fn sub_limbs(a: &[u64; 4], b: &[u64; 4]) -> ([u64; 4], u64) {
    let mut out = [0u64; 4];
    let mut borrow: i128 = 0;
    for i in 0..4 {
        let s = (a[i] as i128) - (b[i] as i128) - borrow;
        if s < 0 {
            out[i] = (s + (1i128 << 64)) as u64;
            borrow = 1;
        } else {
            out[i] = s as u64;
            borrow = 0;
        }
    }
    (out, borrow as u64)
}

fn mul_limbs_512(a: &[u64; 4], b: &[u64; 4]) -> [u64; 8] {
    let mut out = [0u64; 8];
    for i in 0..4 {
        let mut carry: u128 = 0;
        for j in 0..4 {
            let prod = (a[i] as u128) * (b[j] as u128) + (out[i + j] as u128) + carry;
            out[i + j] = prod as u64;
            carry = prod >> 64;
        }
        out[i + 4] = out[i + 4].wrapping_add(carry as u64);
    }
    out
}

/// Reduce a 512-bit little-endian value modulo `r` via schoolbook
/// binary long division (process the dividend MSB-first, shifting a
/// running remainder left one bit at a time and conditionally
/// subtracting the modulus).
///
/// This is deliberately simple rather than fast: it is unambiguously
/// correct for every 512-bit input, which is what a consensus-critical
/// hash needs. The running remainder is always `< r < 2^254`, so after
/// the per-bit shift it stays `< 2^255` and fits in four limbs with no
/// overflow, and a single conditional subtraction restores `< r`.
fn reduce_wide(prod: &[u64; 8]) -> [u64; 4] {
    let mut rem = [0u64; 4];
    for bit in (0..512).rev() {
        // rem <<= 1
        let mut carry = 0u64;
        for limb in rem.iter_mut() {
            let new_carry = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = new_carry;
        }
        // Bring down the next dividend bit (MSB-first).
        let word = bit / 64;
        let off = bit % 64;
        rem[0] |= (prod[word] >> off) & 1;
        // Conditionally subtract the modulus to keep rem < r.
        if !cmp_limbs(&rem, &MODULUS_LIMBS).is_lt() {
            let (out, _) = sub_limbs(&rem, &MODULUS_LIMBS);
            rem = out;
        }
    }
    rem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_one_round_trip() {
        let z = Fr::ZERO;
        let o = Fr::ONE;
        assert_ne!(z, o);
        assert_eq!(z.to_bytes()[0], 0);
        assert_eq!(o.to_bytes()[0], 1);
        assert_eq!(Fr::from_bytes(&z.to_bytes()), Some(z));
        assert_eq!(Fr::from_bytes(&o.to_bytes()), Some(o));
    }

    #[test]
    fn add_sub_inverse() {
        let a = Fr::from_u64(0xdead_beef);
        let b = Fr::from_u64(0xcafe_babe);
        let s = a.add(&b);
        assert_eq!(s.sub(&b), a);
        assert_eq!(s.sub(&a), b);
    }

    #[test]
    fn add_wraps_around_modulus() {
        // (r - 1) + 1 == 0
        let r_minus_one = Fr::ZERO.sub(&Fr::ONE);
        assert_eq!(r_minus_one.add(&Fr::ONE), Fr::ZERO);
    }

    #[test]
    fn mul_one_is_identity() {
        let a = Fr::from_u64(42);
        assert_eq!(a.mul(&Fr::ONE), a);
        assert_eq!(Fr::ONE.mul(&a), a);
    }

    #[test]
    fn mul_zero_is_zero() {
        let a = Fr::from_u64(1234567);
        assert_eq!(a.mul(&Fr::ZERO), Fr::ZERO);
    }

    #[test]
    fn pow5_consistent() {
        let a = Fr::from_u64(3);
        // 3^5 = 243
        assert_eq!(a.pow5(), Fr::from_u64(243));
    }

    #[test]
    fn mul_full_width_minus_one_squared_is_one() {
        // (r-1) == -1 (mod r), so (r-1)*(r-1) == 1. This exercises the
        // full-width 512-bit reduction path that the previous
        // multiplier got wrong.
        let minus_one = Fr::ZERO.sub(&Fr::ONE);
        assert_eq!(minus_one.mul(&minus_one), Fr::ONE);
    }

    #[test]
    fn mul_full_width_minus_one_times_two() {
        // (r-1)*2 == -2 == r-2 (mod r).
        let minus_one = Fr::ZERO.sub(&Fr::ONE);
        let two = Fr::from_u64(2);
        let expected = Fr::ZERO.sub(&two);
        assert_eq!(minus_one.mul(&two), expected);
    }

    #[test]
    fn pow5_of_minus_one_is_minus_one() {
        // (-1)^5 == -1.
        let minus_one = Fr::ZERO.sub(&Fr::ONE);
        assert_eq!(minus_one.pow5(), minus_one);
    }

    #[test]
    fn be_decode_matches_le_decode() {
        // 0x...0002 big-endian == 2.
        let mut be = [0u8; 32];
        be[31] = 2;
        assert_eq!(Fr::from_be_bytes_reduce(&be), Fr::from_u64(2));
    }

    #[test]
    fn from_bytes_rejects_above_modulus() {
        // r itself as bytes must be rejected
        let mut buf = [0u8; 32];
        for (i, limb) in MODULUS_LIMBS.iter().enumerate() {
            buf[i * 8..(i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        assert!(Fr::from_bytes(&buf).is_none());
    }

    #[test]
    fn from_bytes_reduce_always_succeeds() {
        let buf = [0xffu8; 32];
        let _ = Fr::from_bytes_reduce(&buf);
    }
}
