use anyhow::{anyhow, Result};
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};
use k256::ecdsa::signature::hazmat::PrehashSigner;
use revm::primitives::{keccak256, Address, B256, U256};
use sha3::{Digest, Keccak256};

use crate::engine::Transaction;

#[derive(Clone, Debug)]
pub struct SignedTransaction {
    pub tx: Transaction,
    pub v: U256,
    pub r: U256,
    pub s: U256,
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
    }
}

/// Recover the signer address from a `SignedTransaction`.
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
