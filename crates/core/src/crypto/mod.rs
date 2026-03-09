use anyhow::{anyhow, Result};
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};
use k256::ecdsa::signature::hazmat::PrehashSigner;
use revm::primitives::{keccak256, Address, B256, U256};
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
/// Format: keccak256(chain_id || nonce || gas_price || gas_limit || to || value || data)
/// All integer fields are big-endian. `to` is the 20-byte address or 20 zero bytes.
pub fn tx_signing_hash(tx: &Transaction) -> B256 {
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

/// Sign a transaction with a secp256k1 private key, producing a `SignedTransaction`.
pub fn sign_transaction(tx: &Transaction, private_key: &SigningKey) -> SignedTransaction {
    let hash = tx_signing_hash(tx);
    let (sig, recovery_id): (Signature, RecoveryId) = private_key
        .sign_prehash(hash.as_slice())
        .expect("signing failed");

    let sig_bytes = sig.to_bytes();
    let r = U256::from_be_slice(&sig_bytes[..32]);
    let s = U256::from_be_slice(&sig_bytes[32..64]);

    let chain_id = tx.chain_id.unwrap_or(0);
    let v = U256::from(recovery_id.to_byte() as u64 + 35 + chain_id * 2);

    let mut signed_tx = tx.clone();
    signed_tx.signature = Some((r, s, v.to::<u64>()));

    SignedTransaction {
        tx: signed_tx,
        v,
        r,
        s,
        tx_type: 0,
    }
}

/// Recover the signer address from a `SignedTransaction` using the Prime Chain custom signing hash.
pub fn recover_signer(signed_tx: &SignedTransaction) -> Result<Address> {
    let hash = tx_signing_hash(&signed_tx.tx);

    let chain_id = signed_tx.tx.chain_id.unwrap_or(0);
    let v_u64 = signed_tx.v.to::<u64>();
    let recovery_byte = v_u64
        .checked_sub(35 + chain_id * 2)
        .ok_or_else(|| anyhow!("invalid v value: {v_u64}"))?;

    let recovery_id =
        RecoveryId::try_from(recovery_byte as u8).map_err(|e| anyhow!("invalid recovery id: {e}"))?;

    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&signed_tx.r.to_be_bytes::<32>());
    sig_bytes[32..64].copy_from_slice(&signed_tx.s.to_be_bytes::<32>());
    let signature =
        Signature::from_bytes((&sig_bytes).into()).map_err(|e| anyhow!("invalid signature: {e}"))?;

    let verifying_key = VerifyingKey::recover_from_prehash(hash.as_slice(), &signature, recovery_id)
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
    public_key_to_address(&key.verifying_key())
}

/// Decode raw signed transaction bytes. Tries standard Ethereum RLP first (for MetaMask/ethers.js
/// compatibility), then falls back to Prime Chain's custom format.
pub fn decode_raw_signed_tx(bytes: &[u8]) -> Result<SignedTransaction> {
    // Try standard Ethereum RLP first
    if let Ok(signed) = rlp_decode::decode_ethereum_tx(bytes) {
        return Ok(signed);
    }

    // Fall back to custom Prime Chain format
    decode_prime_format_tx(bytes)
}

/// Decode raw signed transaction in Prime Chain's custom binary format.
/// Format: chain_id(8) | nonce(8) | gas_price(32) | gas_limit(8) | to(20) | value(32) | data_len(4) | data | r(32) | s(32) | v(8)
fn decode_prime_format_tx(bytes: &[u8]) -> Result<SignedTransaction> {
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
    };
    let signed_temp = SignedTransaction {
        tx: tx.clone(),
        v: U256::from(v),
        r,
        s,
        tx_type: 0,
    };
    let from = recover_signer(&signed_temp)?;
    let tx = Transaction {
        from,
        ..tx
    };
    Ok(SignedTransaction {
        tx,
        v: U256::from(v),
        r,
        s,
        tx_type: 0,
    })
}

/// Encode a signed transaction to raw bytes (Prime Chain format).
pub fn encode_raw_signed_tx(signed: &SignedTransaction) -> Vec<u8> {
    let tx = &signed.tx;
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
            chain_id: Some(7919),
            signature: None,
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
}
