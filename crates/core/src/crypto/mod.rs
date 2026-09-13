use anyhow::{Result, anyhow, bail};
use k256::ecdsa::signature::hazmat::PrehashSigner;
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};
use revm::primitives::{Address, B256, U256, keccak256};
use sha3::{Digest, Keccak256};

use crate::engine::Transaction;
use revm::primitives::Bytes;

mod rlp_decode;

#[derive(Clone, Debug)]
pub struct SignedTransaction {
    pub tx: Transaction,
    pub v: U256,
    pub r: U256,
    pub s: U256,
    /// 0 = legacy, 1 = EIP-2930, 2 = EIP-1559
    pub tx_type: u8,
}

/// Compute the deterministic hash of a transaction for signing (EIP-155 inspired).
///
/// For legacy / EIP-2930 / EIP-1559 (tx_type 0..=2), the hash domain
/// is `chain_id || nonce || gas_price || gas_limit || to || value || data`.
///
/// For shielded (tx_type 0x7E), the hash domain is:
///   `"MERSENNET_SHIELDED_V1" || chain_id || nonce || from
///    || keccak256(bincode(shielded_payload))`
/// — gas fields are intentionally omitted because shielded txs are
/// paid by the prover's bond / sponsoring relayer, not by `tx.from`.
/// All integer fields are big-endian. `to` is the 20-byte address or
/// 20 zero bytes.
pub fn tx_signing_hash(tx: &Transaction) -> B256 {
    if tx.tx_type == crate::shielded_evm::SHIELDED_TX_TYPE {
        return shielded_signing_hash(tx);
    }
    let chain_id = tx.chain_id.unwrap_or(0);
    let mut buf = Vec::with_capacity(8 + 8 + 32 + 8 + 20 + 32 + tx.data.len());
    buf.extend_from_slice(&chain_id.to_be_bytes());
    buf.extend_from_slice(&tx.nonce.to_be_bytes());
    buf.extend_from_slice(&tx.gas_price.to_be_bytes::<32>());
    buf.extend_from_slice(&tx.gas_limit.to_be_bytes());
    match tx.to {
        Some(addr) => buf.extend_from_slice(addr.as_slice()),
        None => buf.extend_from_slice(&[0u8; 20]),
    }
    buf.extend_from_slice(&tx.value.to_be_bytes::<32>());
    buf.extend_from_slice(&tx.data);
    keccak256(buf)
}

/// Signing hash for a `tx_type == 0x7E` shielded transaction.
/// Returns `B256::ZERO` when the payload is missing — the verifier
/// will then reject the signature because the zero hash never
/// matches an honestly produced signature.
fn shielded_signing_hash(tx: &Transaction) -> B256 {
    const DOMAIN: &[u8] = b"MERSENNET_SHIELDED_V1";
    let Some(payload) = &tx.shielded_payload else {
        return B256::ZERO;
    };
    let payload_bytes = match bincode::serialize(payload) {
        Ok(b) => b,
        Err(_) => return B256::ZERO,
    };
    let payload_digest = keccak256(&payload_bytes);
    let chain_id = tx.chain_id.unwrap_or(0);
    let mut buf = Vec::with_capacity(DOMAIN.len() + 8 + 8 + 20 + 32);
    buf.extend_from_slice(DOMAIN);
    buf.extend_from_slice(&chain_id.to_be_bytes());
    buf.extend_from_slice(&tx.nonce.to_be_bytes());
    buf.extend_from_slice(tx.from.as_slice());
    buf.extend_from_slice(payload_digest.as_slice());
    keccak256(buf)
}

/// Sign a transaction with a secp256k1 private key, producing a `SignedTransaction`.
pub fn sign_transaction(tx: &Transaction, private_key: &SigningKey) -> SignedTransaction {
    let hash = tx_signing_hash(tx);
    let (sig, recovery_id): (Signature, RecoveryId) = private_key
        .sign_prehash(hash.as_slice())
        .expect("signing failed");

    let sig_bytes = sig.to_bytes();
    let r = U256::from_be_slice(&sig_bytes[..32]);
    let s = U256::from_be_slice(&sig_bytes[32..64]);

    // Shielded txs use raw y-parity (0/1) — they don't follow EIP-155
    // because they don't traverse the EVM signing path.
    let v = if tx.tx_type == crate::shielded_evm::SHIELDED_TX_TYPE {
        U256::from(recovery_id.to_byte() as u64)
    } else {
        let chain_id = tx.chain_id.unwrap_or(0);
        U256::from(recovery_id.to_byte() as u64 + 35 + chain_id * 2)
    };

    let mut signed_tx = tx.clone();
    signed_tx.signature = Some((r, s, v.to::<u64>()));

    SignedTransaction {
        tx: signed_tx,
        v,
        r,
        s,
        tx_type: tx.tx_type,
    }
}

/// Half of the secp256k1 group order `n`. Signatures with `s > n/2` are
/// non-canonical (EIP-2) and rejected to prevent signature malleability.
const SECP256K1_HALF_N: U256 = U256::from_be_bytes([
    0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0x5D, 0x57, 0x6E, 0x73, 0x57, 0xA4, 0x50, 0x1D, 0xDF, 0xE9, 0x2F, 0x46, 0x68, 0x1B, 0x20, 0xA0,
]);

/// Recover the signer address from a `SignedTransaction` using the Mersennet custom signing hash.
pub fn recover_signer(signed_tx: &SignedTransaction) -> Result<Address> {
    let hash = tx_signing_hash(&signed_tx.tx);

    let v_u64 = signed_tx.v.to::<u64>();
    let recovery_byte = if signed_tx.tx.tx_type == crate::shielded_evm::SHIELDED_TX_TYPE {
        // Raw y-parity, see `sign_transaction`.
        if v_u64 > 1 {
            return Err(anyhow!("invalid shielded v value: {v_u64}"));
        }
        v_u64 as u8
    } else {
        let chain_id = signed_tx.tx.chain_id.unwrap_or(0);
        v_u64
            .checked_sub(35 + chain_id * 2)
            .ok_or_else(|| anyhow!("invalid v value: {v_u64}"))? as u8
    };

    let recovery_id = RecoveryId::try_from(recovery_byte as u8)
        .map_err(|e| anyhow!("invalid recovery id: {e}"))?;

    // Reject high-`s` signatures (EIP-2 low-`s` rule). Without this, a third
    // party can malleate a submitted signature (`s' = n − s`, flipped parity)
    // into a different raw envelope that recovers the same signer but hashes to
    // a different tx id — polluting mempool/hash tracking. secp256k1 n/2:
    if signed_tx.s > SECP256K1_HALF_N {
        return Err(anyhow!("non-canonical signature: s is greater than n/2"));
    }

    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&signed_tx.r.to_be_bytes::<32>());
    sig_bytes[32..64].copy_from_slice(&signed_tx.s.to_be_bytes::<32>());
    let signature = Signature::from_bytes((&sig_bytes).into())
        .map_err(|e| anyhow!("invalid signature: {e}"))?;

    let verifying_key =
        VerifyingKey::recover_from_prehash(hash.as_slice(), &signature, recovery_id)
            .map_err(|e| anyhow!("ECDSA recovery failed: {e}"))?;

    Ok(public_key_to_address(&verifying_key))
}

/// Derive an Ethereum-style address from a secp256k1 public key.
fn public_key_to_address(key: &VerifyingKey) -> Address {
    let uncompressed = key.to_encoded_point(false);
    let pub_bytes = &uncompressed.as_bytes()[1..];
    let hash = Keccak256::digest(pub_bytes);
    Address::from_slice(&hash[12..])
}

/// Derive the Ethereum-style address for a signing key.
pub fn address_from_signing_key(key: &SigningKey) -> Address {
    public_key_to_address(key.verifying_key())
}

/// Decode raw signed transaction bytes. Tries standard Ethereum RLP first (for MetaMask/ethers.js
/// compatibility), then falls back to Mersennet's custom format.
pub fn decode_raw_signed_tx(bytes: &[u8]) -> Result<SignedTransaction> {
    // Try standard Ethereum RLP first
    if let Ok(signed) = rlp_decode::decode_ethereum_tx(bytes) {
        return Ok(signed);
    }

    // Fall back to custom Mersennet format
    decode_mersennet_format_tx(bytes)
}

/// Decode raw signed transaction in Mersennet's custom binary format.
/// Format: chain_id(8) | nonce(8) | gas_price(32) | gas_limit(8) | to(20) | value(32) | data_len(4) | data | r(32) | s(32) | v(8)
fn decode_mersennet_format_tx(bytes: &[u8]) -> Result<SignedTransaction> {
    const MIN_LEN: usize = 8 + 8 + 32 + 8 + 20 + 32 + 4 + 32 + 32 + 8; // 172
    if bytes.len() < MIN_LEN {
        return Err(anyhow!("raw tx too short: {} bytes", bytes.len()));
    }
    let mut off = 0;
    let read_u64 = |off: &mut usize| {
        let v = u64::from_be_bytes(bytes[*off..*off + 8].try_into().unwrap());
        *off += 8;
        v
    };
    let read_u256 = |off: &mut usize| {
        let v = U256::from_be_slice(&bytes[*off..*off + 32]);
        *off += 32;
        v
    };
    let chain_id = read_u64(&mut off);
    let nonce = read_u64(&mut off);
    let gas_price = read_u256(&mut off);
    let gas_limit = read_u64(&mut off);
    let to_bytes: [u8; 20] = bytes[off..off + 20].try_into().unwrap();
    off += 20;
    let to = if to_bytes == [0u8; 20] {
        None
    } else {
        Some(Address::from_slice(&to_bytes))
    };
    let value = read_u256(&mut off);
    let data_len = u32::from_be_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    if bytes.len() < off + data_len + 32 + 32 + 8 {
        return Err(anyhow!("raw tx truncated"));
    }
    let data = Bytes::from(bytes[off..off + data_len].to_vec());
    off += data_len;
    let r = read_u256(&mut off);
    let s = read_u256(&mut off);
    let v = read_u64(&mut off);

    let tx = Transaction {
        from: Address::ZERO,
        to,
        value,
        data,
        gas_limit,
        gas_price,
        nonce,
        chain_id: Some(chain_id),
        signature: Some((r, s, v)),
        tx_type: 0,
        shielded_payload: None,
        hash: None,
    };
    let signed_temp = SignedTransaction {
        tx: tx.clone(),
        v: U256::from(v),
        r,
        s,
        tx_type: 0,
    };
    let from = recover_signer(&signed_temp)?;
    let tx = Transaction { from, ..tx };
    Ok(SignedTransaction {
        tx,
        v: U256::from(v),
        r,
        s,
        tx_type: 0,
    })
}

/// Encode a signed transaction to raw bytes (Mersennet format).
///
/// For `tx_type == 0x7E` (shielded), encodes as the EIP-2718 envelope
/// `0x7E || bincode(SerdeShielded)`. The serde wrapper carries the
/// (chain_id, nonce, from, envelope, r, s, v) tuple so that a
/// roundtrip through `decode_raw_signed_tx` reproduces the original
/// `SignedTransaction`.
pub fn encode_raw_signed_tx(signed: &SignedTransaction) -> Vec<u8> {
    let tx = &signed.tx;
    if tx.tx_type == crate::shielded_evm::SHIELDED_TX_TYPE {
        return encode_raw_shielded_tx(signed);
    }
    let chain_id = tx.chain_id.unwrap_or(0);
    let mut out = Vec::new();
    out.extend_from_slice(&chain_id.to_be_bytes());
    out.extend_from_slice(&tx.nonce.to_be_bytes());
    out.extend_from_slice(&tx.gas_price.to_be_bytes::<32>());
    out.extend_from_slice(&tx.gas_limit.to_be_bytes());
    match &tx.to {
        Some(addr) => out.extend_from_slice(addr.as_slice()),
        None => out.extend_from_slice(&[0u8; 20]),
    }
    out.extend_from_slice(&tx.value.to_be_bytes::<32>());
    out.extend_from_slice(&(tx.data.len() as u32).to_be_bytes());
    out.extend_from_slice(tx.data.as_ref());
    out.extend_from_slice(&signed.r.to_be_bytes::<32>());
    out.extend_from_slice(&signed.s.to_be_bytes::<32>());
    out.extend_from_slice(&signed.v.to::<u64>().to_be_bytes());
    out
}

fn encode_raw_shielded_tx(signed: &SignedTransaction) -> Vec<u8> {
    let tx = &signed.tx;
    let payload = tx
        .shielded_payload
        .as_ref()
        .expect("shielded tx must have payload");
    let payload_bytes = bincode::serialize(payload).expect("shielded payload serialize");

    // 7-item RLP list: [chain_id, nonce, from, payload, y_parity, r, s]
    let chain_id = tx.chain_id.unwrap_or(0);
    let v_u64 = signed.v.to::<u64>();
    let items: Vec<Vec<u8>> = vec![
        u64_to_be_bytes(chain_id),
        u64_to_be_bytes(tx.nonce),
        tx.from.as_slice().to_vec(),
        payload_bytes,
        vec![v_u64 as u8],
        u256_to_be_bytes(&signed.r),
        u256_to_be_bytes(&signed.s),
    ];
    let inner_rlp = rlp_encode_list_minimal(&items);
    let mut out = Vec::with_capacity(1 + inner_rlp.len());
    out.push(crate::shielded_evm::SHIELDED_TX_TYPE);
    out.extend_from_slice(&inner_rlp);
    out
}

fn u64_to_be_bytes(n: u64) -> Vec<u8> {
    if n == 0 {
        return Vec::new();
    }
    let bytes = n.to_be_bytes();
    bytes[bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or(bytes.len() - 1)..]
        .to_vec()
}

fn u256_to_be_bytes(u: &U256) -> Vec<u8> {
    let bytes = u.to_be_bytes::<32>();
    let leading = bytes.iter().take_while(|&&b| b == 0).count();
    if leading == 32 {
        Vec::new()
    } else {
        bytes[leading..].to_vec()
    }
}

/// Minimal RLP-list encoder for `Vec<Vec<u8>>` of "atomic" items.
/// Each inner `Vec<u8>` is interpreted as a single string; the list
/// is concatenated and wrapped per RLP rules. We re-implement here
/// rather than depend on `rlp` because the rlp_decode module is
/// hand-rolled too.
fn rlp_encode_list_minimal(items: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = Vec::new();
    for item in items {
        payload.extend_from_slice(&rlp_encode_string(item));
    }
    let len = payload.len();
    let mut out = Vec::with_capacity(payload.len() + 9);
    if len <= 55 {
        out.push(0xc0 + len as u8);
    } else {
        let mut len_bytes = (len as u64).to_be_bytes().to_vec();
        while len_bytes.first() == Some(&0) {
            len_bytes.remove(0);
        }
        out.push(0xf7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
    }
    out.extend_from_slice(&payload);
    out
}

fn rlp_encode_string(s: &[u8]) -> Vec<u8> {
    if s.len() == 1 && s[0] < 0x80 {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len() + 9);
    let len = s.len();
    if len < 56 {
        out.push(0x80 + len as u8);
    } else {
        let mut len_bytes = (len as u64).to_be_bytes().to_vec();
        while len_bytes.first() == Some(&0) {
            len_bytes.remove(0);
        }
        out.push(0xb7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
    }
    out.extend_from_slice(s);
    out
}

// ───── BFT consensus vote signing (Workstream A) ─────

/// Domain-separated digest a validator signs to vote for a block at a
/// given height in the BFT finality protocol.
pub fn vote_digest(height: u64, block_hash: B256) -> B256 {
    const DOMAIN: &[u8] = b"MERSENNET_BFT_VOTE_V1";
    let mut buf = Vec::with_capacity(DOMAIN.len() + 8 + 32);
    buf.extend_from_slice(DOMAIN);
    buf.extend_from_slice(&height.to_be_bytes());
    buf.extend_from_slice(block_hash.as_slice());
    keccak256(buf)
}

/// Sign a finality vote. Returns `(r, s, y_parity)`.
pub fn sign_vote(height: u64, block_hash: B256, key: &SigningKey) -> (U256, U256, u64) {
    let digest = vote_digest(height, block_hash);
    let (sig, recovery_id): (Signature, RecoveryId) = key
        .sign_prehash(digest.as_slice())
        .expect("vote signing failed");
    let sig_bytes = sig.to_bytes();
    let r = U256::from_be_slice(&sig_bytes[..32]);
    let s = U256::from_be_slice(&sig_bytes[32..64]);
    (r, s, recovery_id.to_byte() as u64)
}

/// Recover the validator address that produced a finality-vote signature.
pub fn recover_vote_signer(
    height: u64,
    block_hash: B256,
    r: U256,
    s: U256,
    y_parity: u64,
) -> Result<Address> {
    if y_parity > 1 {
        return Err(anyhow!("invalid vote y_parity: {y_parity}"));
    }
    let digest = vote_digest(height, block_hash);
    let recovery_id =
        RecoveryId::try_from(y_parity as u8).map_err(|e| anyhow!("invalid recovery id: {e}"))?;
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&r.to_be_bytes::<32>());
    sig_bytes[32..64].copy_from_slice(&s.to_be_bytes::<32>());
    let signature = Signature::from_bytes((&sig_bytes).into())
        .map_err(|e| anyhow!("invalid vote signature: {e}"))?;
    let verifying_key =
        VerifyingKey::recover_from_prehash(digest.as_slice(), &signature, recovery_id)
            .map_err(|e| anyhow!("vote ECDSA recovery failed: {e}"))?;
    Ok(public_key_to_address(&verifying_key))
}

/// Digest a block proposer signs to prove it authored the block at `height`
/// with hash `block_hash`. Distinct domain tag from votes so a vote signature
/// can never be replayed as a proposal signature or vice versa.
pub fn block_proposal_digest(height: u64, block_hash: B256) -> B256 {
    const DOMAIN: &[u8] = b"MERSENNET_BLOCK_PROPOSAL_V1";
    let mut buf = Vec::with_capacity(DOMAIN.len() + 8 + 32);
    buf.extend_from_slice(DOMAIN);
    buf.extend_from_slice(&height.to_be_bytes());
    buf.extend_from_slice(block_hash.as_slice());
    keccak256(buf)
}

/// Sign a block proposal. Returns `(r, s, y_parity)`.
pub fn sign_block_proposal(height: u64, block_hash: B256, key: &SigningKey) -> (U256, U256, u64) {
    let digest = block_proposal_digest(height, block_hash);
    let (sig, recovery_id): (Signature, RecoveryId) = key
        .sign_prehash(digest.as_slice())
        .expect("block proposal signing failed");
    let sig_bytes = sig.to_bytes();
    let r = U256::from_be_slice(&sig_bytes[..32]);
    let s = U256::from_be_slice(&sig_bytes[32..64]);
    (r, s, recovery_id.to_byte() as u64)
}

/// Recover the proposer address that produced a block-proposal signature.
pub fn recover_block_proposer(
    height: u64,
    block_hash: B256,
    r: U256,
    s: U256,
    y_parity: u64,
) -> Result<Address> {
    if y_parity > 1 {
        return Err(anyhow!("invalid proposal y_parity: {y_parity}"));
    }
    let digest = block_proposal_digest(height, block_hash);
    let recovery_id =
        RecoveryId::try_from(y_parity as u8).map_err(|e| anyhow!("invalid recovery id: {e}"))?;
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&r.to_be_bytes::<32>());
    sig_bytes[32..64].copy_from_slice(&s.to_be_bytes::<32>());
    let signature = Signature::from_bytes((&sig_bytes).into())
        .map_err(|e| anyhow!("invalid proposal signature: {e}"))?;
    let verifying_key =
        VerifyingKey::recover_from_prehash(digest.as_slice(), &signature, recovery_id)
            .map_err(|e| anyhow!("proposal ECDSA recovery failed: {e}"))?;
    Ok(public_key_to_address(&verifying_key))
}

/// Generate a random signing key and its corresponding address (useful for tests).
pub fn generate_keypair() -> (SigningKey, Address) {
    let signing_key = SigningKey::random(&mut rand::thread_rng());
    let address = address_from_signing_key(&signing_key);
    (signing_key, address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::primitives::Bytes;

    fn sample_tx(from: Address) -> Transaction {
        Transaction {
            from,
            to: Some(Address::from_slice(&[0xBB; 20])),
            value: U256::from(1_000u64),
            data: Bytes::new(),
            gas_limit: 21_000,
            gas_price: U256::from(1u64),
            nonce: 0,
            chain_id: Some(131071),
            signature: None,
            tx_type: 0,
            shielded_payload: None,
            hash: None,
        }
    }

    #[test]
    fn sign_and_recover_roundtrip() {
        let (key, addr) = generate_keypair();
        let tx = sample_tx(addr);
        let signed = sign_transaction(&tx, &key);
        let recovered = recover_signer(&signed).expect("recovery should succeed");
        assert_eq!(recovered, addr, "recovered address must match signer");
    }

    #[test]
    fn signing_hash_is_deterministic() {
        let (_, addr) = generate_keypair();
        let tx = sample_tx(addr);
        assert_eq!(tx_signing_hash(&tx), tx_signing_hash(&tx));
    }

    #[test]
    fn wrong_key_gives_wrong_address() {
        let (key_a, addr_a) = generate_keypair();
        let (_, addr_b) = generate_keypair();
        let tx = sample_tx(addr_a);
        let signed = sign_transaction(&tx, &key_a);
        let recovered = recover_signer(&signed).unwrap();
        assert_ne!(recovered, addr_b);
    }

    #[test]
    fn high_s_signature_is_rejected() {
        // secp256k1 group order n.
        const SECP256K1_N: U256 = U256::from_be_bytes([
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C,
            0xD0, 0x36, 0x41, 0x41,
        ]);
        let (key, addr) = generate_keypair();
        let tx = sample_tx(addr);
        let signed = sign_transaction(&tx, &key);
        // A correctly produced signature is low-s and must verify.
        assert!(recover_signer(&signed).is_ok());
        assert!(signed.s <= SECP256K1_HALF_N);
        // Malleate to the equivalent high-s form: s' = n - s, flipped parity.
        let mut malleated = signed.clone();
        malleated.s = SECP256K1_N - signed.s;
        let recovered = recover_signer(&malleated);
        assert!(
            recovered.is_err(),
            "high-s (malleated) signatures must be rejected (EIP-2)"
        );
    }

    #[test]
    fn block_proposal_sign_and_recover_roundtrip() {
        let (key, addr) = generate_keypair();
        let hash = B256::from_slice(&[0x9c; 32]);
        let (r, s, y) = sign_block_proposal(42, hash, &key);
        let recovered = recover_block_proposer(42, hash, r, s, y).expect("recover");
        assert_eq!(recovered, addr);
        // Wrong height or hash must not recover the proposer.
        assert_ne!(recover_block_proposer(43, hash, r, s, y).unwrap(), addr);
        let other = B256::from_slice(&[0x01; 32]);
        assert_ne!(recover_block_proposer(42, other, r, s, y).unwrap(), addr);
    }

    #[test]
    fn vote_and_proposal_signatures_are_domain_separated() {
        // A vote signature must not verify as a proposal signature for the
        // same (height, hash) — different domain tags prevent cross-replay.
        let (key, _addr) = generate_keypair();
        let hash = B256::from_slice(&[0x7e; 32]);
        let (vr, vs, vy) = sign_vote(7, hash, &key);
        let as_proposal = recover_block_proposer(7, hash, vr, vs, vy);
        let vote_signer = recover_vote_signer(7, hash, vr, vs, vy).unwrap();
        // Recovery still yields *an* address, but not the true signer.
        if let Ok(addr) = as_proposal {
            assert_ne!(addr, vote_signer);
        }
    }

    fn dummy_shield_tx(addr: Address) -> crate::shielded_evm::ShieldTx {
        use mersennet_zkp::field::Fr;
        use mersennet_zkp::noir::{Circuit, CircuitProof};
        crate::shielded_evm::ShieldTx {
            from: addr,
            amount: U256::from(42u64),
            output_commitment: Fr::ZERO,
            encrypted_output: vec![0u8; 8],
            proof: CircuitProof {
                circuit: Circuit::Output,
                public_inputs: vec![Fr::ZERO; 1],
                proof_bytes: vec![0u8; 32],
                vk_hash: [0u8; 32],
            },
        }
    }

    #[test]
    fn shielded_signing_hash_domain_separates() {
        let (_, addr) = generate_keypair();
        let legacy = sample_tx(addr);
        let mut shielded = legacy.clone();
        shielded.tx_type = crate::shielded_evm::SHIELDED_TX_TYPE;
        shielded.shielded_payload = Some(crate::shielded_evm::ShieldedEnvelope::Shield(
            dummy_shield_tx(addr),
        ));
        // With a payload set, the shielded hash MUST differ from the
        // legacy hash for the same nonce/sender — otherwise a
        // signature could be replayed across tx types.
        assert_ne!(tx_signing_hash(&legacy), tx_signing_hash(&shielded));
    }

    #[test]
    fn shielded_signing_hash_missing_payload_is_zero() {
        let (_, addr) = generate_keypair();
        let mut tx = sample_tx(addr);
        tx.tx_type = crate::shielded_evm::SHIELDED_TX_TYPE;
        tx.shielded_payload = None;
        assert_eq!(tx_signing_hash(&tx), revm::primitives::B256::ZERO);
    }

    #[test]
    fn shielded_encode_decode_roundtrip() {
        let (key, addr) = generate_keypair();
        let envelope = crate::shielded_evm::ShieldedEnvelope::Shield(dummy_shield_tx(addr));
        let mut tx = sample_tx(addr);
        tx.tx_type = crate::shielded_evm::SHIELDED_TX_TYPE;
        tx.shielded_payload = Some(envelope);

        let signed = sign_transaction(&tx, &key);
        let raw = encode_raw_signed_tx(&signed);
        assert_eq!(raw[0], crate::shielded_evm::SHIELDED_TX_TYPE);

        let decoded = decode_raw_signed_tx(&raw).expect("decode shielded");
        assert_eq!(decoded.tx.tx_type, crate::shielded_evm::SHIELDED_TX_TYPE);
        assert_eq!(decoded.tx.from, addr);
        assert_eq!(decoded.tx.nonce, tx.nonce);
        assert!(matches!(
            decoded.tx.shielded_payload,
            Some(crate::shielded_evm::ShieldedEnvelope::Shield(_))
        ));
        let recovered = recover_signer(&decoded).expect("shielded recover");
        assert_eq!(recovered, addr);
    }
}

/// EIP-191 personal-sign digest of `message`.
pub fn eip191_digest(message: &[u8]) -> B256 {
    let mut prefixed = format!("\x19Ethereum Signed Message:\n{}", message.len()).into_bytes();
    prefixed.extend_from_slice(message);
    keccak256(&prefixed)
}

/// Recover the signer of an EIP-191 signature (65 bytes: r ‖ s ‖ v, v ∈ {0,1,27,28}).
pub fn recover_eip191(message: &[u8], sig: &[u8]) -> Result<Address> {
    if sig.len() != 65 {
        bail!("signature must be 65 bytes");
    }
    let v = sig[64];
    let recid = RecoveryId::try_from(if v >= 27 { v - 27 } else { v })
        .map_err(|e| anyhow!("invalid recovery id: {e}"))?;
    let signature =
        Signature::from_slice(&sig[..64]).map_err(|e| anyhow!("invalid signature: {e}"))?;
    let key =
        VerifyingKey::recover_from_prehash(eip191_digest(message).as_slice(), &signature, recid)
            .map_err(|e| anyhow!("recovery failed: {e}"))?;
    Ok(public_key_to_address(&key))
}

/// The statement a node key signs to let `operator` register it as a
/// validator. Static per (operator, identity), so a node can hand it out
/// freely (whoami) — it authorises nothing but that binding.
pub fn validator_registration_message(operator: Address, identity: Address) -> String {
    format!(
        "Mersennet validator registration v1\noperator: 0x{}\nidentity: 0x{}",
        hex::encode(operator.as_slice()),
        hex::encode(identity.as_slice())
    )
}

/// Sign the registration statement with the node key (65-byte EIP-191 signature).
pub fn sign_validator_registration(
    operator: Address,
    identity: Address,
    key: &SigningKey,
) -> Vec<u8> {
    let msg = validator_registration_message(operator, identity);
    let digest = eip191_digest(msg.as_bytes());
    let (sig, recid): (Signature, RecoveryId) = key
        .sign_prehash(digest.as_slice())
        .expect("registration signing failed");
    let mut out = sig.to_bytes().to_vec();
    out.push(27 + recid.to_byte());
    out
}
